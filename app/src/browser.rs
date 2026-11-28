//! Thin wrappers over the browser APIs the app needs (clock, storage, timers, fetch).

use crate::time::{Day, LocalTime};
use js_sys::{Date, Function, Promise};
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;
use web_sys::{AbortController, Headers, Request, RequestInit, Response};

pub fn window() -> web_sys::Window {
    web_sys::window().expect("no window")
}

pub fn document() -> web_sys::Document {
    window().document().expect("no document")
}

pub fn now_ms() -> i64 {
    Date::now() as i64
}

/// Local wall-clock time right now.
pub fn local_now() -> LocalTime {
    let d = Date::new_0();
    let day = Day::from_ymd(d.get_full_year() as i32, d.get_month() + 1, d.get_date());
    LocalTime(day.0 as i64 * 86_400 + (d.get_hours() * 3600 + d.get_minutes() * 60 + d.get_seconds()) as i64)
}

/// Converts a local wall-clock time to Unix milliseconds using the browser's time zone.
pub fn local_to_ms(t: LocalTime) -> i64 {
    let (y, m, d) = t.day().ymd();
    let date = Date::new_with_year_month_day_hr_min_sec(y as u32, m as i32 - 1, d as i32, t.hour() as i32, t.minute() as i32, 0);
    date.get_time() as i64
}

pub fn storage_get(key: &str) -> Option<String> {
    window().local_storage().ok().flatten()?.get_item(key).ok().flatten()
}

pub fn storage_set(key: &str, value: &str) {
    if let Some(s) = window().local_storage().ok().flatten() {
        let _ = s.set_item(key, value);
    }
}

pub fn storage_remove(key: &str) {
    if let Some(s) = window().local_storage().ok().flatten() {
        let _ = s.remove_item(key);
    }
}

pub fn set_timeout(f: &Function, ms: i32) -> i32 {
    window().set_timeout_with_callback_and_timeout_and_arguments_0(f, ms).unwrap_or(0)
}

pub async fn sleep(ms: i32) {
    let p = Promise::new(&mut |resolve, _| {
        set_timeout(&resolve, ms);
    });
    let _ = JsFuture::from(p).await;
}

pub fn random_id() -> String {
    let mut s = String::with_capacity(36);
    for i in 0..32 {
        if [8, 12, 16, 20].contains(&i) {
            s.push('-');
        }
        let n = (js_sys::Math::random() * 16.0) as u32;
        s.push(std::char::from_digit(n.min(15), 16).unwrap_or('0'));
    }
    s
}

pub fn user_agent() -> String {
    window().navigator().user_agent().unwrap_or_default()
}

/// Calls a global function from `glue.js` (e.g. `szIsStandalone`) and returns its value.
pub fn glue(name: &str, args: &[JsValue]) -> JsValue {
    let f = js_sys::Reflect::get(&window(), &JsValue::from_str(name)).ok();
    match f.and_then(|f| f.dyn_into::<Function>().ok()) {
        Some(f) => {
            let arr = js_sys::Array::new();
            for a in args {
                arr.push(a);
            }
            f.apply(&JsValue::NULL, &arr).unwrap_or(JsValue::UNDEFINED)
        }
        None => JsValue::UNDEFINED,
    }
}

pub fn glue_bool(name: &str) -> bool {
    glue(name, &[]).as_bool().unwrap_or(false)
}

pub fn glue_string(name: &str) -> String {
    glue(name, &[]).as_string().unwrap_or_default()
}

/// Awaits a glue function that returns a Promise of a string.
pub async fn glue_async_string(name: &str, args: &[JsValue]) -> Option<String> {
    let v = glue(name, args);
    let p: Promise = v.dyn_into().ok()?;
    JsFuture::from(p).await.ok()?.as_string()
}

#[derive(Debug)]
pub enum FetchError {
    /// The server answered with an error status.
    Http(u16, String),
    /// Unreachable, timed out, offline, or a broken response.
    Network,
}

thread_local! {
    /// Requests that should be cancelled together when sync restarts (sign-out, page hidden).
    static INFLIGHT: std::cell::RefCell<Vec<AbortController>> = const { std::cell::RefCell::new(Vec::new()) };
}

pub fn abort_all_requests() {
    INFLIGHT.with(|v| {
        for c in v.borrow_mut().drain(..) {
            c.abort();
        }
    });
}

/// JSON request to this server. `timeout_ms` covers the whole request.
pub async fn fetch_json(
    method: &str,
    path: &str,
    headers: &[(&str, String)],
    body: Option<String>,
    timeout_ms: i32,
) -> Result<serde_json::Value, FetchError> {
    let controller = AbortController::new().map_err(|_| FetchError::Network)?;
    INFLIGHT.with(|v| v.borrow_mut().push(controller.clone()));
    let result = fetch_inner(method, path, headers, body, timeout_ms, &controller).await;
    INFLIGHT.with(|v| v.borrow_mut().retain(|c| c != &controller));
    result
}

async fn fetch_inner(
    method: &str,
    path: &str,
    headers: &[(&str, String)],
    body: Option<String>,
    timeout_ms: i32,
    controller: &AbortController,
) -> Result<serde_json::Value, FetchError> {
    let h = Headers::new().map_err(|_| FetchError::Network)?;
    let _ = h.set("Accept", "application/json");
    for (k, v) in headers {
        let _ = h.set(k, v);
    }
    let init = RequestInit::new();
    init.set_method(method);
    init.set_headers(&h);
    init.set_signal(Some(&controller.signal()));
    init.set_cache(web_sys::RequestCache::NoStore);
    if let Some(b) = body {
        let _ = h.set("Content-Type", "application/json; charset=utf-8");
        init.set_body(&JsValue::from_str(&b));
    }
    let req = Request::new_with_str_and_init(path, &init).map_err(|_| FetchError::Network)?;

    let abort = controller.clone();
    let timer = Closure::once_into_js(move || abort.abort());
    let timer_id = set_timeout(timer.unchecked_ref(), timeout_ms);

    let outcome = async {
        let resp: Response = JsFuture::from(window().fetch_with_request(&req))
            .await
            .map_err(|_| FetchError::Network)?
            .dyn_into()
            .map_err(|_| FetchError::Network)?;
        let text = JsFuture::from(resp.text().map_err(|_| FetchError::Network)?)
            .await
            .map_err(|_| FetchError::Network)?
            .as_string()
            .unwrap_or_default();
        if !resp.ok() {
            return Err(FetchError::Http(resp.status(), text));
        }
        serde_json::from_str(&text).map_err(|_| FetchError::Network)
    }
    .await;
    window().clear_timeout_with_handle(timer_id);
    outcome
}
