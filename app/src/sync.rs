//! Keeps this browser and the servers in step. Same protocol and behaviour as Android's
//! `SyncManager` (server/PROTOCOL.md):
//!
//! - Two servers. The main one is always the server this page came from (fixed: it's the same
//!   site). The backup is set in Settings and used when the main one can't be reached. Each has its
//!   own password/token. The servers also sync with each other on their own.
//! - While the page is visible: a long-poll gets other devices' changes within a second or two.
//! - After every local edit: the changed days upload immediately. Offline edits stay queued.
//! - Edits are stamped on the server's clock, and the most recent edit of a day always wins.

use crate::app::{self, with, with_mut};
use crate::browser::{self, fetch_json, now_ms, storage_get, storage_set, FetchError};
use crate::model::{Carpool, DayPlan, InboxMessage, Person};
use crate::store::{Repo, SyncedDay};
use crate::time::Day;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::cell::Cell;
use std::collections::BTreeMap;
use wasm_bindgen_futures::spawn_local;

const LONG_POLL_SECONDS: i32 = 25;
const SYNC_KEY: &str = "sz.sync.v1";
/// The main server is this site itself.
pub const MAIN: &str = "";
/// Default backup: debian-server on the tailnet (the Android app's main server).
pub const DEFAULT_SECONDARY: &str = "http://100.93.228.123:8787";

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Connection {
    Offline,
    Connecting,
    Online,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Auth {
    NoPerson,
    /// Haven't been able to ask a server yet (offline). The app still works locally.
    Unknown,
    SignedIn,
    /// Need a password for `Status::auth_server`: create one (`has_password` false) or enter it.
    NeedsPassword { has_password: bool },
}

#[derive(Clone, Debug)]
pub struct Status {
    pub connection: Connection,
    pub auth: Auth,
    /// The server the password screen is for ("" = this site's own server).
    pub auth_server: String,
    /// Server in use right now (None when offline).
    pub active_server: Option<String>,
    pub last_sync: Option<i64>,
    pub pending: bool,
    /// User tapped "Use offline for now" on the password screen.
    pub password_skipped: bool,
}

impl Status {
    pub fn initial(repo: &Repo, saved: &SavedSync) -> Status {
        Status {
            connection: Connection::Offline,
            auth: match (repo.data.me, saved.tokens.is_empty()) {
                (None, _) => Auth::NoPerson,
                (Some(_), false) => Auth::SignedIn,
                (Some(_), true) => Auth::Unknown,
            },
            auth_server: MAIN.into(),
            active_server: None,
            last_sync: None,
            pending: repo.has_pending_uploads(),
            password_skipped: false,
        }
    }

    pub fn using_backup(&self) -> bool {
        matches!(&self.active_server, Some(s) if !s.is_empty())
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct SavedSync {
    pub device_id: String,
    /// Login token per server ("" = this site).
    pub tokens: BTreeMap<String, String>,
    /// Last revision seen per server.
    pub revs: BTreeMap<String, i64>,
    /// Backup server address, e.g. "https://example.com/ScheduleZero". Empty = none.
    pub secondary: String,
    /// The backup was set by hand (otherwise it follows `DEFAULT_SECONDARY`).
    pub secondary_chosen: bool,
    /// Server clock minus this device's clock, in ms (see `edit_time`).
    pub clock_offset: i64,
    /// Version 1 kept a single token for this site.
    token: Option<String>,
}

impl SavedSync {
    pub fn load() -> SavedSync {
        let mut s: SavedSync = storage_get(SYNC_KEY).and_then(|j| serde_json::from_str(&j).ok()).unwrap_or_default();
        if let Some(t) = s.token.take() {
            s.tokens.insert(MAIN.into(), t);
        }
        if s.device_id.is_empty() {
            s.device_id = browser::random_id();
        }
        if !s.secondary_chosen {
            s.secondary = DEFAULT_SECONDARY.to_string();
        }
        s.save();
        CLOCK_OFFSET.with(|c| c.set(s.clock_offset));
        s
    }

    pub fn save(&self) {
        storage_set(SYNC_KEY, &serde_json::to_string(self).unwrap_or_default());
    }

    pub fn servers(&self) -> Vec<String> {
        let mut v = vec![MAIN.to_string()];
        if !self.secondary.is_empty() {
            v.push(self.secondary.clone());
        }
        v
    }

    pub fn token(&self, server: &str) -> Option<String> {
        self.tokens.get(server).cloned()
    }

    fn rev(&self, server: &str) -> i64 {
        *self.revs.get(server).unwrap_or(&-1)
    }
}

/// Cleans up a typed backup address: adds https:// if missing, drops a trailing slash.
pub fn clean_server(input: &str) -> String {
    let c = input.trim().trim_end_matches('/');
    if c.is_empty() {
        return String::new();
    }
    if c.starts_with("http://") || c.starts_with("https://") {
        c.to_string()
    } else if c.split(':').next().is_some_and(|h| h.parse::<std::net::Ipv4Addr>().is_ok()) {
        format!("http://{c}") // a bare IP like the Tailscale one is plain http, like on Android
    } else {
        format!("https://{c}")
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
    static CLOCK_OFFSET: Cell<i64> = const { Cell::new(0) };
}

/// When an edit happened, on the server's clock. Every device stamps edits this way, so "the
/// most recent edit wins" holds even if a phone's own clock is wrong. Kept outside the app state
/// so it can be read while that state is being changed.
pub fn edit_time() -> i64 {
    now_ms() + CLOCK_OFFSET.with(|c| c.get())
}

/// Learns the server's clock from a response's `serverTime`. Call outside `with`/`with_mut`.
fn learn_server_clock(v: &Value) {
    if let Some(server) = v["serverTime"].as_i64() {
        let offset = server - now_ms();
        CLOCK_OFFSET.with(|c| c.set(offset));
        with_mut(|a| {
            if (a.saved.clock_offset - offset).abs() > 1000 {
                a.saved.clock_offset = offset;
                a.saved.save();
            }
        });
    }
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

/// Where a server's API lives: this site's own path prefix (e.g. "/ScheduleZero"), or the backup.
fn base(server: &str) -> String {
    if server.is_empty() {
        browser::glue_string("szBase")
    } else {
        server.trim_end_matches('/').to_string()
    }
}

/// Request to a server ("" = this site).
pub async fn api_at(server: &str, method: &str, path: &str, token: Option<&str>, body: Option<Value>, timeout_ms: i32) -> Result<Value, FetchError> {
    let url = format!("{}{}", base(server), path);
    fetch_json(method, &url, &headers(token), body.map(|b| b.to_string()), timeout_ms).await
}

/// Request to this site's own server.
pub async fn api(method: &str, path: &str, token: Option<&str>, body: Option<Value>, timeout_ms: i32) -> Result<Value, FetchError> {
    api_at(MAIN, method, path, token, body, timeout_ms).await
}

/// This site's own login token (push notifications always go through this site).
pub fn main_token() -> Option<String> {
    with(|a| a.saved.token(MAIN))
}

async fn auth_status(server: &str, p: Person) -> Result<bool, FetchError> {
    let v = api_at(server, "GET", &format!("/api/auth/status?person={}", p.id()), None, None, 10_000).await?;
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
    with_mut(|a| {
        a.status.connection = Connection::Offline;
        a.status.active_server = None;
    });
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

/// Called after local edits: upload to the first server that takes them.
pub fn push_soon() {
    with_mut(|a| a.status.pending = a.repo.has_pending_uploads());
    spawn_local(async {
        let servers = with(|a| a.saved.servers());
        for s in servers {
            let Some(t) = with(|a| a.saved.token(&s)) else { continue };
            if push_to(&s, &t).await {
                return;
            }
        }
        wake_up();
    });
}

pub fn choose_person(p: Person) {
    with_mut(|a| {
        if a.repo.data.me != Some(p) {
            a.saved.tokens.clear();
            a.saved.revs.clear();
            a.saved.save();
        }
        a.repo.set_me(p, edit_time());
        a.status.auth = Auth::Unknown;
        a.status.password_skipped = false;
        a.status.pending = a.repo.has_pending_uploads();
    });
    app::commit();
    restart();
}

/// Ask the servers again (e.g. after "Sync now") and show the password screen if needed.
pub fn refresh_auth_state() {
    with_mut(|a| a.status.password_skipped = false);
    crate::dom::request_render();
    restart();
}

pub fn skip_password() {
    with_mut(|a| a.status.password_skipped = true);
    crate::dom::request_render();
}

/// Sets the backup server ("" = none). Tokens are kept per server.
pub fn set_secondary(input: &str) {
    let s = clean_server(input);
    with_mut(|a| {
        a.saved.secondary = s;
        a.saved.secondary_chosen = true;
        a.saved.save();
        a.ui.backup_text = a.saved.secondary.clone();
        if a.repo.data.me.is_some() {
            a.status.auth = Auth::Unknown;
        }
    });
    restart();
}

pub async fn submit_password(password: String, setup: bool) -> PasswordResult {
    let Some(me) = app::me() else { return PasswordResult::Unreachable };
    let target = with(|a| a.status.auth_server.clone());
    let payload = json!({
        "person": me.id(), "password": password, "deviceId": with(|a| a.saved.device_id.clone()),
        "deviceName": device_name(), "platform": "web",
    });
    let path = if setup { "/api/auth/setup" } else { "/api/auth/login" };
    match api_at(&target, "POST", path, None, Some(payload.clone()), 15_000).await {
        Ok(v) => {
            let Some(token) = v["token"].as_str().map(str::to_string) else { return PasswordResult::Unreachable };
            with_mut(|a| {
                a.saved.tokens.insert(target.clone(), token);
                a.saved.revs.insert(target.clone(), -1);
                a.saved.save();
                a.status.auth = Auth::SignedIn;
                a.status.connection = Connection::Online;
            });
            crate::push::on_signed_in();
            restart();
            // Sign in to the other server too while we have the password, so failover is seamless.
            // In the background: an unreachable backup must not hold up loading the schedule.
            let others: Vec<String> = with(|a| a.saved.servers()).into_iter().filter(|s| *s != target).collect();
            spawn_local(async move {
                for other in others {
                    if with(|a| a.saved.token(&other).is_some()) {
                        continue;
                    }
                    if let Ok(has) = auth_status(&other, me).await {
                        let p = if has { "/api/auth/login" } else { "/api/auth/setup" };
                        if let Ok(v) = api_at(&other, "POST", p, None, Some(payload.clone()), 15_000).await {
                            if let Some(t) = v["token"].as_str() {
                                with_mut(|a| {
                                    a.saved.tokens.insert(other.clone(), t.to_string());
                                    a.saved.save();
                                });
                            }
                        }
                    }
                }
            });
            PasswordResult::Ok
        }
        Err(FetchError::Http(401, _)) => PasswordResult::Wrong,
        Err(FetchError::Http(400, _)) => PasswordResult::TooShort,
        Err(FetchError::Http(409, _)) => {
            set_needs_password(true, &target);
            PasswordResult::AlreadySet
        }
        Err(FetchError::Http(404, _)) => {
            set_needs_password(false, &target);
            PasswordResult::NotSet
        }
        Err(_) => PasswordResult::Unreachable,
    }
}

pub fn sign_out() {
    crate::push::forget();
    with_mut(|a| {
        a.saved.tokens.clear();
        a.saved.revs.clear();
        a.saved.save();
        a.repo.sign_out();
        a.status.auth = Auth::NoPerson;
        a.status.pending = false;
    });
    app::commit();
    restart();
}

/// A carpool or inbox request, on the first server that answers (main first, then backup).
/// Server errors (e.g. "can't answer this carpool") are returned, not retried elsewhere.
pub async fn call_any(path: &str, body: Value) -> Result<Value, FetchError> {
    let servers = with(|a| a.saved.servers());
    let mut last = FetchError::Network;
    for s in servers {
        let Some(t) = with(|a| a.saved.token(&s)) else { continue };
        match api_at(&s, "POST", path, Some(&t), Some(body.clone()), 15_000).await {
            Ok(v) => {
                learn_server_clock(&v);
                return Ok(v);
            }
            Err(FetchError::Http(401, _)) => {
                drop_token(&s);
                last = FetchError::Http(401, String::new());
            }
            Err(FetchError::Http(c, m)) => return Err(FetchError::Http(c, m)),
            Err(e) => last = e,
        }
    }
    Err(last)
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

fn set_needs_password(has_password: bool, server: &str) {
    with_mut(|a| {
        a.status.auth = Auth::NeedsPassword { has_password };
        a.status.auth_server = server.to_string();
        a.status.connection = Connection::Online;
        a.status.active_server = Some(server.to_string());
    });
    crate::dom::request_render();
}

fn set_connection(c: Connection) {
    let changed = with_mut(|a| {
        if c != Connection::Online {
            a.status.active_server = None;
        }
        std::mem::replace(&mut a.status.connection, c) != c
    });
    if changed {
        crate::dom::request_render();
    }
}

fn drop_token(server: &str) {
    with_mut(|a| {
        a.saved.tokens.remove(server);
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
        // Main first, then backup. The first one that answers wins this round.
        let mut reached = false;
        let mut waiting_for_password = false;
        for server in with(|a| a.saved.servers()) {
            if !alive(gen) {
                break;
            }
            match with(|a| a.saved.token(&server)) {
                None => {
                    if let Ok(has) = auth_status(&server, me).await {
                        if alive(gen) {
                            set_needs_password(has, &server);
                        }
                        reached = true;
                        waiting_for_password = true;
                        break;
                    }
                }
                Some(t) => {
                    push_to(&server, &t).await;
                    flush_reads(&server, &t).await;
                    let settled = with(|a| {
                        a.status.connection == Connection::Online
                            && a.status.auth == Auth::SignedIn
                            && a.status.active_server.as_deref() == Some(server.as_str())
                    });
                    let rev = with(|a| a.saved.rev(&server));
                    let wait = if settled { LONG_POLL_SECONDS } else { 0 };
                    let path = format!("/api/state?since={rev}&wait={wait}");
                    match api_at(&server, "GET", &path, Some(&t), None, (wait + 15) * 1000).await {
                        Ok(v) => {
                            if alive(gen) {
                                apply_state(&server, &v);
                            }
                            reached = true;
                            break;
                        }
                        // Password was reset on that server (file deleted). Ask again next round.
                        Err(FetchError::Http(401, _)) => {
                            drop_token(&server);
                            reached = true;
                            break;
                        }
                        Err(_) => {} // unreachable: try the next server quietly
                    }
                }
            }
        }
        if !alive(gen) {
            break;
        }
        if !reached {
            set_connection(Connection::Offline);
            wait_for_wake(backoff).await;
            backoff = (backoff * 2).min(60_000);
        } else {
            backoff = 3_000;
            if waiting_for_password {
                wait_for_wake(30_000).await;
            }
        }
    }
}

fn apply_state(server: &str, v: &Value) {
    learn_server_clock(v);
    let rev = v["rev"].as_i64().unwrap_or(-1);
    let unchanged = v["unchanged"].as_bool().unwrap_or(false);
    let people = (!unchanged).then(|| parse_people(&v["people"]));
    let carpools: Option<Vec<Carpool>> = (!unchanged).then(|| serde_json::from_value(v["carpools"].clone()).ok()).flatten();
    let inbox: Option<Vec<InboxMessage>> = (!unchanged).then(|| serde_json::from_value(v["inbox"].clone()).ok()).flatten();
    let before: Vec<String> = with(|a| a.repo.data.inbox.iter().map(|m| m.id.clone()).collect());
    with_mut(|a| {
        if let Some(p) = &people {
            a.repo.apply_server(p);
        }
        a.repo.apply_server_extras(carpools, inbox);
        a.saved.revs.insert(server.to_string(), rev);
        a.saved.save();
        a.status.connection = Connection::Online;
        a.status.auth = Auth::SignedIn;
        a.status.active_server = Some(server.to_string());
        a.status.last_sync = Some(now_ms());
        a.status.pending = a.repo.has_pending_uploads();
    });
    // New messages while the page is open: show them as notifications too (when the browser
    // allows it and push isn't already doing it for this site).
    let fresh: Vec<InboxMessage> = with(|a| {
        if before.is_empty() {
            return vec![];
        }
        a.repo.data.inbox.iter().filter(|m| m.read == 0 && !before.contains(&m.id)).cloned().collect()
    });
    crate::push::show_in_page(&fresh);
    if people.is_some() {
        app::commit();
    } else {
        crate::dom::request_render();
    }
}

/// Sends "mark as read" taps that happened offline.
async fn flush_reads(server: &str, token: &str) {
    let ids = with(|a| a.repo.pending_read());
    if ids.is_empty() {
        return;
    }
    if api_at(server, "POST", "/api/inbox/read", Some(token), Some(json!({ "ids": ids })), 15_000).await.is_ok() {
        with_mut(|a| a.repo.read_uploaded(&ids));
        app::commit();
    }
}

/// Uploads pending edits to `server`. Returns true if nothing is left to upload.
async fn push_to(server: &str, token: &str) -> bool {
    if PUSHING.with(|p| p.replace(true)) {
        PUSH_AGAIN.with(|p| p.set(true));
        return false;
    }
    let mut ok = true;
    loop {
        PUSH_AGAIN.with(|p| p.set(false));
        let pending = with(|a| a.repo.pending_uploads(edit_time()));
        if pending.is_empty() {
            break;
        }
        let days: serde_json::Map<String, Value> = pending.iter().map(|(d, s)| (Day(*d).iso(), day_json(s))).collect();
        match api_at(server, "POST", "/api/schedule", Some(token), Some(json!({ "days": days })), 15_000).await {
            Ok(v) => {
                learn_server_clock(&v);
                // Fewer applied than sent = another device edited some of these days later.
                // Fetch the full state next so those newer edits replace ours here too.
                let rejected = v["applied"].as_u64().map_or(false, |n| (n as usize) < pending.len());
                with_mut(|a| {
                    a.repo.mark_uploaded(&pending);
                    a.status.pending = a.repo.has_pending_uploads();
                    if rejected {
                        a.saved.revs.insert(server.to_string(), -1);
                        a.saved.save();
                    }
                });
                app::commit();
                if rejected {
                    wake_up();
                }
            }
            Err(e) => {
                if matches!(e, FetchError::Http(401, _)) {
                    drop_token(server);
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
