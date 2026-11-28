//! Carpool actions and the inbox. Requests ("can someone drive me?") and offers ("I can drive
//! you") are per day and per direction (to work / home from work, independent). Answering yes/no
//! happens only in the app, never from a notification. Every new carpool, answer and
//! cancellation lands in the other people's inboxes and notifies them.

use crate::app::{self, with, with_mut, CarpoolSheet, Compose};
use crate::browser::FetchError;
use crate::model::{Carpool, Dir, Person};
use crate::sync;
use crate::time::Day;
use serde_json::json;
use wasm_bindgen_futures::spawn_local;

pub fn open(d: Day) {
    with_mut(|a| {
        a.ui.carpool = Some(CarpoolSheet { date: d, compose: None, busy: false, error: None });
        a.ui.editor = None;
        a.ui.comparing = None;
    });
}

pub fn navigate(d: Day) {
    with_mut(|a| {
        if let Some(s) = a.ui.carpool.as_mut() {
            *s = CarpoolSheet { date: d, compose: None, busy: false, error: None };
        }
    });
}

/// Starts writing a request or offer. Pre-selects the people who match, or everyone else.
pub fn compose(dir: Dir, kind: &'static str) {
    with_mut(|a| {
        let Some(sheet) = a.ui.carpool.as_mut() else { return };
        let data = &a.repo.data;
        let mut to = data.carpool_matches(sheet.date, dir);
        if to.is_empty() {
            to = Person::ALL.into_iter().filter(|p| Some(*p) != data.me).collect();
        }
        sheet.compose = Some(Compose { dir, kind, to, note: String::new() });
        sheet.error = None;
    });
}

pub fn toggle_recipient(p: Person) {
    with_mut(|a| {
        if let Some(c) = a.ui.carpool.as_mut().and_then(|s| s.compose.as_mut()) {
            if let Some(i) = c.to.iter().position(|x| *x == p) {
                c.to.remove(i);
            } else {
                c.to.push(p);
                c.to.sort();
            }
        }
    });
}

pub fn set_note(note: &str) {
    with_mut(|a| {
        if let Some(c) = a.ui.carpool.as_mut().and_then(|s| s.compose.as_mut()) {
            c.note = note.chars().take(140).collect();
        }
    });
}

pub fn cancel_compose() {
    with_mut(|a| {
        if let Some(s) = a.ui.carpool.as_mut() {
            s.compose = None;
            s.error = None;
        }
    });
}

fn error_text(e: &FetchError) -> String {
    match e {
        FetchError::Http(_, body) => {
            let msg = serde_json::from_str::<serde_json::Value>(body).ok().and_then(|v| v["error"].as_str().map(str::to_string));
            format!("The server said no: {}.", msg.unwrap_or_else(|| "try again".into()))
        }
        FetchError::Network => "You're offline. Carpools need a connection, so try again when you're back online.".into(),
    }
}

/// Runs a carpool action on the server and shows the result right away.
fn run(body: serde_json::Value, after: impl FnOnce() + 'static) {
    with_mut(|a| {
        if let Some(s) = a.ui.carpool.as_mut() {
            s.busy = true;
            s.error = None;
        }
    });
    crate::dom::request_render();
    spawn_local(async move {
        let result = sync::call_any("/api/carpool", body).await;
        with_mut(|a| {
            if let Some(s) = a.ui.carpool.as_mut() {
                s.busy = false;
            }
            match &result {
                Ok(v) => {
                    if let Ok(c) = serde_json::from_value::<Carpool>(v["carpool"].clone()) {
                        a.repo.upsert_carpool(c);
                    }
                }
                Err(e) => {
                    if let Some(s) = a.ui.carpool.as_mut() {
                        s.error = Some(error_text(e));
                    }
                }
            }
        });
        if result.is_ok() {
            after();
            sync::wake_up();
        }
        app::commit();
    });
}

pub fn send() {
    let Some((date, c)) = with(|a| a.ui.carpool.as_ref().and_then(|s| s.compose.clone().map(|c| (s.date, c)))) else { return };
    if c.to.is_empty() {
        with_mut(|a| {
            if let Some(s) = a.ui.carpool.as_mut() {
                s.error = Some("Pick at least one person.".into());
            }
        });
        return;
    }
    let to: Vec<&str> = c.to.iter().map(|p| p.id()).collect();
    run(
        json!({"action": "create", "kind": c.kind, "day": date.iso(), "dir": c.dir.id(), "to": to, "note": c.note.trim()}),
        cancel_compose,
    );
}

pub fn answer(id: &str, yes: bool) {
    run(json!({"action": "respond", "id": id, "answer": if yes { "yes" } else { "no" }}), || {});
}

pub fn cancel(id: &str) {
    run(json!({"action": "cancel", "id": id}), || {});
}

// ---- inbox ----------------------------------------------------------------------------------

/// Marks messages read (all when `ids` is None): at once on screen, on the server when online.
pub fn mark_read(ids: Option<Vec<String>>) {
    with_mut(|a| a.repo.mark_read_local(ids.as_deref(), sync::edit_time()));
    app::commit();
    let body = match &ids {
        Some(ids) => json!({ "ids": ids }),
        None => json!({ "all": true }),
    };
    spawn_local(async move {
        if sync::call_any("/api/inbox/read", body).await.is_ok() {
            let sent = with(|a| a.repo.pending_read());
            let sent: Vec<String> = match ids {
                Some(ids) => sent.into_iter().filter(|s| ids.contains(s)).collect(),
                None => sent,
            };
            with_mut(|a| a.repo.read_uploaded(&sent));
            app::commit();
        }
    });
}

/// Opening a message: mark it read and show its carpool (where Yes/No live).
pub fn open_message(id: &str) {
    let day = with(|a| {
        let m = a.repo.data.inbox.iter().find(|m| m.id == id)?;
        let c = a.repo.data.carpools.iter().find(|c| c.id == m.carpool)?;
        c.day()
    });
    if with(|a| a.repo.data.inbox.iter().any(|m| m.id == id && m.read == 0)) {
        mark_read(Some(vec![id.to_string()]));
    }
    if let Some(d) = day {
        open(d);
    }
}

pub fn dismiss_suggestion(key: &str) {
    with_mut(|a| {
        a.repo.data.dismissed_suggestions.insert(key.to_string());
    });
    app::commit();
}
