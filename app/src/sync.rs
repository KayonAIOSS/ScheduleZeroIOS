//! Keeps this browser and the server in step. Same protocol and behaviour as Android's
//! `SyncManager` (server/PROTOCOL.md), with one difference the browser forces: the web app talks
//! only to the server it was loaded from (an https page can't call the plain-http main server).
//! That server keeps itself in sync with the other one, so nothing is lost.
//!
//! - While the page is visible: a long-poll gets other devices' changes within a second or two.
//! - After every local edit: the changed days upload immediately.
//! - Offline? Nothing is shown as an error. Edits stay queued and upload on the next round.

use crate::app::{self, with, with_mut};
use crate::browser::{self, fetch_json, now_ms, storage_get, storage_set, FetchError};
use crate::model::{DayPlan, Person};
use crate::store::{Repo, SyncedDay};
use crate::time::Day;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::cell::Cell;
use std::collections::BTreeMap;
use wasm_bindgen_futures::spawn_local;

const LONG_POLL_SECONDS: i32 = 25;
const SYNC_KEY: &str = "sz.sync.v1";

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Connection {
    Offline,
    Connecting,
    Online,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Auth {
    NoPerson,
    /// Haven't been able to ask the server yet (offline). The app still works locally.
    Unknown,
    SignedIn,
    /// Need a password: create one (`has_password` false) or enter the existing one.
    NeedsPassword { has_password: bool },
}

#[derive(Clone, Debug)]
pub struct Status {
    pub connection: Connection,
    pub auth: Auth,
    pub last_sync: Option<i64>,
    pub pending: bool,
    /// User tapped "Use offline for now" on the password screen.
    pub password_skipped: bool,
}

impl Status {
    pub fn initial(repo: &Repo, saved: &SavedSync) -> Status {
        Status {
            connection: Connection::Offline,
            auth: match (repo.data.me, &saved.token) {
                (None, _) => Auth::NoPerson,
                (Some(_), Some(_)) => Auth::SignedIn,
                (Some(_), None) => Auth::Unknown,
            },
            last_sync: None,
            pending: repo.has_pending_uploads(),
            password_skipped: false,
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct SavedSync {
    pub device_id: String,
    pub token: Option<String>,
    pub rev: i64,
}

impl SavedSync {
    pub fn load() -> SavedSync {
        let mut s: SavedSync = storage_get(SYNC_KEY).and_then(|j| serde_json::from_str(&j).ok()).unwrap_or_default();
        if s.device_id.is_empty() {
            s.device_id = browser::random_id();
            s.rev = -1;
            s.save();
        }
        s
    }

    pub fn save(&self) {
        storage_set(SYNC_KEY, &serde_json::to_string(self).unwrap_or_default());
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum PasswordResult {
    Ok,
    Wrong,
    TooShort,
    AlreadySet,
    NotSet,
    Unreachable,
}

thread_local! {
    static GENERATION: Cell<u32> = const { Cell::new(0) };
    static RUNNING: Cell<bool> = const { Cell::new(false) };
    static WOKEN: Cell<bool> = const { Cell::new(false) };
    static WAKER: std::cell::RefCell<Option<js_sys::Function>> = const { std::cell::RefCell::new(None) };
    static PUSHING: Cell<bool> = const { Cell::new(false) };
    static PUSH_AGAIN: Cell<bool> = const { Cell::new(false) };
}

// ---- HTTP -----------------------------------------------------------------------------------

fn device_name() -> String {
    let ua = browser::user_agent();
    let kind = if ua.contains("iPhone") {
        "iPhone"
    } else if ua.contains("iPad") {
        "iPad"
    } else if ua.contains("Android") {
        "Android"
    } else if ua.contains("Windows") {
        "Windows"
    } else if ua.contains("Mac OS") {
        "Mac"
    } else if ua.contains("Linux") {
        "Linux"
    } else {
        "Browser"
    };
    format!("{kind} (web)")
}

fn headers(token: Option<&str>) -> Vec<(&'static str, String)> {
    let mut h = vec![
        ("X-Device-Id", with(|a| a.saved.device_id.clone())),
        ("X-Device-Name", device_name()),
        ("X-Platform", "web".to_string()),
    ];
    if let Some(t) = token {
        h.push(("Authorization", format!("Bearer {t}")));
    }
    h
}

pub async fn api(method: &str, path: &str, token: Option<&str>, body: Option<Value>, timeout_ms: i32) -> Result<Value, FetchError> {
    fetch_json(method, path, &headers(token), body.map(|b| b.to_string()), timeout_ms).await
}

async fn auth_status(p: Person) -> Result<bool, FetchError> {
    let v = api("GET", &format!("/api/auth/status?person={}", p.id()), None, None, 10_000).await?;
    Ok(v["hasPassword"].as_bool().unwrap_or(false))
}

fn parse_day(v: &Value) -> Option<SyncedDay> {
    let updated = v["updated"].as_i64().unwrap_or(0);
    let plan = match v["type"].as_str()? {
        "work" => Some(DayPlan::Work { start: v["start"].as_i64()? as i32, end: v["end"].as_i64()? as i32 }),
        "off" => Some(DayPlan::Off),
        "none" => None,
        _ => return None,
    };
    Some(SyncedDay { plan, updated })
}

fn day_json(day: &SyncedDay) -> Value {
    match day.plan {
        Some(DayPlan::Work { start, end }) => json!({"type": "work", "start": start, "end": end, "updated": day.updated}),
        Some(DayPlan::Off) => json!({"type": "off", "updated": day.updated}),
        None => json!({"type": "none", "updated": day.updated}),
    }
}

pub fn parse_people(v: &Value) -> BTreeMap<Person, BTreeMap<i32, SyncedDay>> {
    let mut out = BTreeMap::new();
    for p in Person::ALL {
        let mut days = BTreeMap::new();
        if let Some(obj) = v[p.id()].as_object() {
            for (k, dv) in obj {
                if let (Some(d), Some(s)) = (Day::parse_iso(k), parse_day(dv)) {
                    days.insert(d.0, s);
                }
            }
        }
        out.insert(p, days);
    }
    out
}

// ---- public actions -------------------------------------------------------------------------

pub fn start() {
    if RUNNING.with(|r| r.get()) {
        wake_up();
        return;
    }
    RUNNING.with(|r| r.set(true));
    let gen = GENERATION.with(|g| g.get());
    spawn_local(live_loop(gen));
}

/// Stops the live loop (page hidden, person switched). `start()` begins a fresh one.
pub fn stop() {
    GENERATION.with(|g| g.set(g.get().wrapping_add(1)));
    RUNNING.with(|r| r.set(false));
    browser::abort_all_requests();
    wake_up();
    with_mut(|a| a.status.connection = Connection::Offline);
    crate::dom::request_render();
}

pub fn restart() {
    stop();
    start();
}

pub fn wake_up() {
    let f = WAKER.with(|w| w.borrow_mut().take());
    match f {
        Some(f) => {
            let _ = f.call0(&wasm_bindgen::JsValue::NULL);
        }
        None => WOKEN.with(|w| w.set(true)),
    }
}

/// Called after local edits.
pub fn push_soon() {
    with_mut(|a| a.status.pending = a.repo.has_pending_uploads());
    let token = with(|a| a.saved.token.clone());
    if let Some(t) = token {
        spawn_local(async move {
            if !push_to(&t).await {
                wake_up();
            }
        });
    }
}

pub fn choose_person(p: Person) {
    with_mut(|a| {
        if a.repo.data.me != Some(p) {
            a.saved.token = None;
            a.saved.rev = -1;
            a.saved.save();
        }
        a.repo.set_me(p, now_ms());
        a.status.auth = Auth::Unknown;
        a.status.password_skipped = false;
        a.status.pending = a.repo.has_pending_uploads();
    });
    app::commit();
    restart();
}

/// Ask the server again (e.g. after "Sync now") and show the password screen if needed.
pub fn refresh_auth_state() {
    with_mut(|a| a.status.password_skipped = false);
    crate::dom::request_render();
    restart();
}

pub fn skip_password() {
    with_mut(|a| a.status.password_skipped = true);
    crate::dom::request_render();
}

pub async fn submit_password(password: String, setup: bool) -> PasswordResult {
    let Some(me) = app::me() else { return PasswordResult::Unreachable };
    let body = json!({
        "person": me.id(), "password": password, "deviceId": with(|a| a.saved.device_id.clone()),
        "deviceName": device_name(), "platform": "web",
    });
    let path = if setup { "/api/auth/setup" } else { "/api/auth/login" };
    match api("POST", path, None, Some(body), 15_000).await {
        Ok(v) => {
            let Some(token) = v["token"].as_str().map(str::to_string) else { return PasswordResult::Unreachable };
            with_mut(|a| {
                a.saved.token = Some(token);
                a.saved.rev = -1;
                a.saved.save();
                a.status.auth = Auth::SignedIn;
                a.status.connection = Connection::Online;
            });
            crate::push::on_signed_in();
            restart();
            PasswordResult::Ok
        }
        Err(FetchError::Http(401, _)) => PasswordResult::Wrong,
        Err(FetchError::Http(400, _)) => PasswordResult::TooShort,
        Err(FetchError::Http(409, _)) => {
            set_needs_password(true);
            PasswordResult::AlreadySet
        }
        Err(FetchError::Http(404, _)) => {
            set_needs_password(false);
            PasswordResult::NotSet
        }
        Err(_) => PasswordResult::Unreachable,
    }
}

pub fn sign_out() {
    crate::push::forget();
    with_mut(|a| {
        a.saved.token = None;
        a.saved.rev = -1;
        a.saved.save();
        a.repo.sign_out();
        a.status.auth = Auth::NoPerson;
        a.status.pending = false;
    });
    app::commit();
    restart();
}

// ---- internals ------------------------------------------------------------------------------

fn alive(gen: u32) -> bool {
    GENERATION.with(|g| g.get()) == gen
}

async fn wait_for_wake(ms: i32) {
    if WOKEN.with(|w| w.replace(false)) {
        return;
    }
    let p = js_sys::Promise::new(&mut |resolve, _| {
        browser::set_timeout(&resolve, ms);
        WAKER.with(|w| *w.borrow_mut() = Some(resolve));
    });
    let _ = wasm_bindgen_futures::JsFuture::from(p).await;
    WAKER.with(|w| *w.borrow_mut() = None);
}

fn set_needs_password(has_password: bool) {
    with_mut(|a| {
        a.status.auth = Auth::NeedsPassword { has_password };
        a.status.connection = Connection::Online;
    });
    crate::dom::request_render();
}

fn set_connection(c: Connection) {
    let changed = with_mut(|a| std::mem::replace(&mut a.status.connection, c) != c);
    if changed {
        crate::dom::request_render();
    }
}

fn drop_token() {
    with_mut(|a| {
        a.saved.token = None;
        a.saved.save();
    });
}

async fn live_loop(gen: u32) {
    let mut backoff = 3_000;
    while alive(gen) {
        let Some(me) = app::me() else {
            wait_for_wake(30_000).await;
            continue;
        };
        if with(|a| a.status.connection) != Connection::Online {
            set_connection(Connection::Connecting);
        }
        let token = with(|a| a.saved.token.clone());
        let reached = match token {
            None => match auth_status(me).await {
                Ok(has) if alive(gen) => {
                    set_needs_password(has);
                    wait_for_wake(30_000).await;
                    true
                }
                Ok(_) => true,
                Err(_) => false,
            },
            Some(t) => {
                push_to(&t).await;
                let settled = with(|a| a.status.connection == Connection::Online && a.status.auth == Auth::SignedIn);
                let rev = with(|a| a.saved.rev);
                let wait = if settled { LONG_POLL_SECONDS } else { 0 };
                let path = format!("/api/state?since={rev}&wait={wait}");
                match api("GET", &path, Some(&t), None, (wait + 15) * 1000).await {
                    Ok(v) if alive(gen) => {
                        apply_state(&v);
                        true
                    }
                    Ok(_) => true,
                    // Password was reset on the server (file deleted). Ask again next round.
                    Err(FetchError::Http(401, _)) => {
                        drop_token();
                        true
                    }
                    Err(_) => false,
                }
            }
        };
        if !alive(gen) {
            break;
        }
        if reached {
            backoff = 3_000;
        } else {
            set_connection(Connection::Offline);
            wait_for_wake(backoff).await;
            backoff = (backoff * 2).min(60_000);
        }
    }
}

fn apply_state(v: &Value) {
    let rev = v["rev"].as_i64().unwrap_or(-1);
    let unchanged = v["unchanged"].as_bool().unwrap_or(false);
    let people = (!unchanged).then(|| parse_people(&v["people"]));
    with_mut(|a| {
        if let Some(p) = &people {
            a.repo.apply_server(p);
        }
        a.saved.rev = rev;
        a.saved.save();
        a.status.connection = Connection::Online;
        a.status.auth = Auth::SignedIn;
        a.status.last_sync = Some(now_ms());
        a.status.pending = a.repo.has_pending_uploads();
    });
    if people.is_some() {
        app::commit();
    } else {
        crate::dom::request_render();
    }
}

/// Uploads pending edits. Returns true if nothing is left to upload.
async fn push_to(token: &str) -> bool {
    if PUSHING.with(|p| p.replace(true)) {
        PUSH_AGAIN.with(|p| p.set(true));
        return false;
    }
    let mut ok = true;
    loop {
        PUSH_AGAIN.with(|p| p.set(false));
        let pending = with(|a| a.repo.pending_uploads(now_ms()));
        if pending.is_empty() {
            break;
        }
        let days: serde_json::Map<String, Value> = pending.iter().map(|(d, s)| (Day(*d).iso(), day_json(s))).collect();
        match api("POST", "/api/schedule", Some(token), Some(json!({ "days": days })), 15_000).await {
            Ok(_) => {
                with_mut(|a| {
                    a.repo.mark_uploaded(&pending);
                    a.status.pending = a.repo.has_pending_uploads();
                });
                app::commit();
            }
            Err(e) => {
                if matches!(e, FetchError::Http(401, _)) {
                    drop_token();
                }
                ok = false;
                break;
            }
        }
        if !PUSH_AGAIN.with(|p| p.get()) {
            break;
        }
    }
    PUSHING.with(|p| p.set(false));
    ok && !with(|a| a.repo.has_pending_uploads())
}

pub async fn fetch_server_version() {
    if let Ok(v) = api("GET", "/api/ping", None, None, 10_000).await {
        let ver = v["version"].as_str().map(str::to_string);
        with_mut(|a| a.ui.server_version = ver);
        crate::dom::request_render();
    }
}
