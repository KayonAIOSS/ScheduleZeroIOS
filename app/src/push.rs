//! Reminders for the web app.
//!
//! Android sets local alarms. A web page can't wake itself up when it's closed, so the web app
//! works out the same reminders (shift reminders, Sunday check-in) and hands them to its server,
//! which sends them as web pushes at the right time (see server `PushStore`). The server drops a
//! reminder if the schedule changed so it no longer applies, like Android's receiver does.
//!
//! iPhone: web push only works once ScheduleZero is added to the Home Screen (iOS 16.4+).

use crate::app::{with, with_mut};
use crate::browser::{self, glue, glue_async_string, glue_string, storage_get, storage_set};
use crate::model::AppData;
use crate::sync::api;
use crate::time::{fmt, Day, LocalTime};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::cell::Cell;
use wasm_bindgen::JsValue;
use wasm_bindgen_futures::{spawn_local, JsFuture};

const PUSH_KEY: &str = "sz.push.v1";

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct PushState {
    /// Inbox key from the server; Some = this browser is registered for pushes.
    pub key: Option<String>,
    pub vapid_key: Option<String>,
    /// Last reminder list the server accepted (skip re-uploading the same list).
    pub last_sig: String,
    #[serde(skip)]
    pub permission: String,
    #[serde(skip)]
    pub supported: bool,
    #[serde(skip)]
    pub busy: bool,
    #[serde(skip)]
    pub message: Option<String>,
}

impl PushState {
    pub fn load() -> PushState {
        let mut p: PushState = storage_get(PUSH_KEY).and_then(|j| serde_json::from_str(&j).ok()).unwrap_or_default();
        p.supported = browser::glue_bool("szPushSupported");
        p.permission = glue_string("szPermission");
        p
    }

    fn save(&self) {
        storage_set(PUSH_KEY, &serde_json::to_string(self).unwrap_or_default());
    }

    pub fn enabled(&self) -> bool {
        self.key.is_some() && self.permission == "granted"
    }
}

thread_local! {
    static DEBOUNCE: Cell<u32> = const { Cell::new(0) };
}

/// Reminders this device wants, as the server's push items.
pub fn reminder_items(data: &AppData, now: LocalTime, to_ms: &dyn Fn(LocalTime) -> i64) -> Vec<Value> {
    let mut items = Vec::new();
    let use24 = data.settings.use24h;
    let today = now.day();
    let last = data.plans.keys().next_back().map(|d| Day(*d)).unwrap_or(today).max(today.plus(21));
    let mut d = today.plus(-1);
    while d <= last {
        if let (Some(at), Some((start, end)), true) = (data.reminder_time(d), data.work_plan(d), data.reminder_enabled(d)) {
            if at > now {
                let leave = data.leave_time(d).unwrap_or(at);
                let carpool = if data.settings.carpool_in_reminders {
                    data.ride_summary(d, crate::model::Dir::To).map(|s| format!("\nCarpool: {s}")).unwrap_or_default()
                } else {
                    String::new()
                };
                items.push(json!({
                    "id": format!("shift-{}", d.iso()),
                    "at": to_ms(at),
                    "until": to_ms(d.at(end)),
                    "title": format!("Work at {}", fmt::work_minutes(start, use24)),
                    "body": format!(
                        "Leave by {} ({} min drive).\nShift: {} · {}{carpool}",
                        fmt::time(leave, use24),
                        data.settings.commute_minutes,
                        fmt::shift(start, end, use24),
                        fmt::duration((end - start) as i64)
                    ),
                    "check": {"type": "shift", "day": d.iso(), "start": start},
                }));
            }
        }
        d = d.plus(1);
    }
    if data.settings.weekly_prompt {
        let sunday = today.plus(-(((today.weekday() + 1) % 7) as i32));
        let mut at = sunday.at(data.settings.weekly_prompt_minutes);
        if at <= now || data.has_any_entry(at.day().plus(1).week_start()) {
            at = at.plus_minutes(7 * 24 * 60);
        }
        items.push(json!({
            "id": "weekly",
            "at": to_ms(at),
            "title": "Next week's schedule isn't in yet",
            "body": "Tap to enter your hours for Monday – Sunday.",
            "check": {"type": "weekly", "week": at.day().plus(1).week_start().iso()},
        }));
    }
    items
}

/// New inbox messages that arrived while the page is open. When this browser has notifications
/// allowed but isn't registered for pushes from this site (e.g. using the backup server), show
/// them straight from the page; otherwise the server's push already shows them.
pub fn show_in_page(msgs: &[crate::model::InboxMessage]) {
    if msgs.is_empty() {
        return;
    }
    let (on, pushed) = with(|a| (a.repo.data.settings.carpool_notifications, a.push.key.is_some() && !a.status.using_backup()));
    if !on || pushed || glue_string("szPermission") != "granted" {
        return;
    }
    for m in msgs {
        glue("szShowNotification", &[JsValue::from_str(&m.title), JsValue::from_str(&m.body), JsValue::from_str(&m.id)]);
    }
}

/// Why reminders can't be delivered on this device right now (None = they can).
pub fn blocker() -> Option<(&'static str, &'static str)> {
    with(|a| {
        let p = &a.push;
        if crate::app::is_ios() && !crate::app::is_standalone() {
            return Some(("Add to Home Screen for reminders", "On iPhone, reminders only work after you add ScheduleZero to your Home Screen."));
        }
        if !p.supported {
            return Some(("Reminders aren't supported here", "This browser can't show notifications. Try Safari, Chrome, Edge or Firefox."));
        }
        if p.permission == "denied" {
            return Some(("Notifications are blocked", "Allow notifications for this site in your browser's settings, then come back."));
        }
        if !p.enabled() {
            return Some(("Notifications are off", "Reminders can't appear until you allow them."));
        }
        None
    })
}

/// Turns on notifications. Must be called straight from a tap/click: browsers (iOS especially)
/// only show the permission prompt during a user gesture, so the prompt is requested here,
/// synchronously, and the rest happens afterwards.
pub fn ensure_enabled_from_gesture() {
    let (go, key) = with(|a| {
        let p = &a.push;
        let ios_blocked = crate::app::is_ios() && !crate::app::is_standalone();
        (p.supported && !p.enabled() && p.permission != "denied" && !ios_blocked && !p.busy, p.vapid_key.clone().unwrap_or_default())
    });
    if !go {
        return;
    }
    let promise = glue("szSubscribe", &[JsValue::from_str(&key)]);
    with_mut(|a| a.push.busy = true);
    crate::dom::request_render();
    spawn_local(async move {
        let sub = match promise.dyn_into_promise() {
            Some(p) => JsFuture::from(p).await.ok().and_then(|v| v.as_string()).unwrap_or_default(),
            None => String::new(),
        };
        let permission = glue_string("szPermission");
        with_mut(|a| a.push.permission = permission);
        if !sub.is_empty() {
            register(&sub).await;
        }
        with_mut(|a| a.push.busy = false);
        crate::dom::request_render();
    });
}

trait IntoPromise {
    fn dyn_into_promise(self) -> Option<js_sys::Promise>;
}

impl IntoPromise for JsValue {
    fn dyn_into_promise(self) -> Option<js_sys::Promise> {
        wasm_bindgen::JsCast::dyn_into::<js_sys::Promise>(self).ok()
    }
}

/// Sends this browser's push subscription to the server and uploads the reminders.
async fn register(sub_json: &str) {
    let Ok(sub) = serde_json::from_str::<Value>(sub_json) else { return };
    let (token, device) = with(|a| (a.saved.token(crate::sync::MAIN), a.saved.device_id.clone()));
    let Some(token) = token else { return };
    let body = json!({"deviceId": device, "subscription": sub});
    match api("POST", "/api/push/subscribe", Some(&token), Some(body), 15_000).await {
        Ok(v) => {
            let Some(key) = v["key"].as_str().map(str::to_string) else { return };
            let session = json!({"deviceId": device, "key": key}).to_string();
            if let Some(p) = glue("szSaveSession", &[JsValue::from_str(&session)]).dyn_into_promise() {
                let _ = JsFuture::from(p).await;
            }
            with_mut(|a| {
                a.push.key = Some(key);
                a.push.last_sig.clear();
                a.push.message = None;
                a.push.save();
            });
            sync_items().await;
        }
        Err(_) => with_mut(|a| a.push.message = Some("Couldn't reach the server to turn on reminders. Try again.".into())),
    }
}

/// At start-up and after signing in: make sure the server still knows this browser.
pub fn on_signed_in() {
    spawn_local(async {
        if with(|a| a.push.vapid_key.is_none()) {
            if let Ok(v) = api("GET", "/api/push/key", None, None, 10_000).await {
                let k = v["publicKey"].as_str().map(str::to_string);
                with_mut(|a| {
                    a.push.vapid_key = k;
                    a.push.save();
                });
            }
        }
        let permission = glue_string("szPermission");
        with_mut(|a| a.push.permission = permission.clone());
        if permission == "granted" {
            if let Some(sub) = glue_async_string("szCurrentSubscription", &[]).await {
                if !sub.is_empty() {
                    register(&sub).await;
                }
            }
        }
        crate::dom::request_render();
    });
}

/// Signing out: stop reminders for this browser.
pub fn forget() {
    let (token, device) = with(|a| (a.saved.token(crate::sync::MAIN), a.saved.device_id.clone()));
    with_mut(|a| {
        a.push.key = None;
        a.push.last_sig.clear();
        a.push.save();
    });
    if let Some(t) = token {
        spawn_local(async move {
            let _ = api("POST", "/api/push/unsubscribe", Some(&t), Some(json!({"deviceId": device})), 10_000).await;
        });
    }
}

/// Re-uploads the reminder list a moment after changes settle.
pub fn schedule_sync_soon() {
    let gen = DEBOUNCE.with(|d| {
        d.set(d.get().wrapping_add(1));
        d.get()
    });
    spawn_local(async move {
        browser::sleep(1500).await;
        if DEBOUNCE.with(|d| d.get()) == gen {
            sync_items().await;
        }
    });
}

async fn sync_items() {
    let (enabled, token, device) = with(|a| (a.push.key.is_some(), a.saved.token(crate::sync::MAIN), a.saved.device_id.clone()));
    let Some(token) = token else { return };
    if !enabled {
        return;
    }
    let items = with(|a| reminder_items(&a.repo.data, browser::local_now(), &browser::local_to_ms));
    let messages = with(|a| a.repo.data.settings.carpool_notifications);
    let sig = format!("{messages}{}", serde_json::to_string(&items).unwrap_or_default());
    if with(|a| a.push.last_sig == sig) {
        return;
    }
    match api("POST", "/api/push/schedule", Some(&token), Some(json!({"deviceId": device, "items": items, "messages": messages})), 15_000).await {
        Ok(_) => with_mut(|a| {
            a.push.last_sig = sig;
            a.push.save();
        }),
        // The server forgot this browser (new server, data reset): register again.
        Err(browser::FetchError::Http(404, _)) => {
            with_mut(|a| {
                a.push.key = None;
                a.push.save();
            });
            on_signed_in();
        }
        Err(_) => {}
    }
}

pub fn send_test() {
    ensure_enabled_from_gesture();
    let (token, device, enabled) = with(|a| (a.saved.token(crate::sync::MAIN), a.saved.device_id.clone(), a.push.enabled()));
    let Some(token) = token else { return };
    if !enabled {
        return;
    }
    spawn_local(async move {
        let r = api("POST", "/api/push/test", Some(&token), Some(json!({"deviceId": device})), 10_000).await;
        with_mut(|a| {
            a.push.message = Some(match r {
                Ok(_) => "Sent. It should appear within about 10 seconds.".into(),
                Err(_) => "Couldn't reach the server. Try again when you're connected.".into(),
            })
        });
        crate::dom::request_render();
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{DayPlan, Person};

    #[test]
    fn items_match_android_alarms() {
        let mut d = AppData { me: Some(Person::Kawaii), ..Default::default() };
        let fri = Day::from_ymd(2026, 11, 27);
        d.plans.insert(fri.0, DayPlan::Work { start: 960, end: 1440 });
        d.plans.insert(fri.plus(1).0, DayPlan::Work { start: 1020, end: 1500 });
        d.plans.insert(fri.plus(2).0, DayPlan::Off);
        d.reminder_overrides.insert(fri.0, true);
        let now = fri.at(10 * 60);
        let items = reminder_items(&d, now, &|t| t.0 * 1000);
        assert_eq!(items.len(), 1, "only the day with a reminder on");
        assert_eq!(items[0]["id"], "shift-2026-11-27");
        assert_eq!(items[0]["at"], fri.at(900).0 * 1000);
        assert_eq!(items[0]["title"], "Work at 4:00 PM");
        assert_eq!(items[0]["check"]["start"], 960);

        d.settings.default_reminders = true;
        assert_eq!(reminder_items(&d, now, &|t| t.0).len(), 2);
        assert!(reminder_items(&d, fri.at(15 * 60 + 1), &|t| t.0).len() == 1, "today's reminder time passed");
    }

    #[test]
    fn weekly_prompt_skips_entered_weeks() {
        let mut d = AppData { me: Some(Person::Kawaii), ..Default::default() };
        d.settings.weekly_prompt = true;
        let fri = Day::from_ymd(2026, 11, 27);
        let sunday = Day::from_ymd(2026, 11, 29);
        // Friday: last Sunday's prompt has passed, so it's this coming Sunday.
        let items = reminder_items(&d, fri.at(600), &|t| t.0);
        assert_eq!(items[0]["at"], sunday.at(600).0);
        assert_eq!(items[0]["check"]["week"], "2026-11-30");
        // Sunday 9 AM with next week already entered: skip to the Sunday after.
        d.plans.insert(Day::from_ymd(2026, 12, 1).0, DayPlan::Off);
        let items = reminder_items(&d, sunday.at(540), &|t| t.0);
        assert_eq!(items[0]["at"], sunday.plus(7).at(600).0);
        assert_eq!(items[0]["check"]["week"], "2026-12-07");
    }
}
