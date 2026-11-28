//! Screens as HTML strings. Two layouts share the same pieces:
//! - mobile (phones, tuned for iPhone): top bar, one column, bottom sheets — like the Android app;
//! - desktop (900 px and wider): sidebar, multi-column pages, centred dialogs.

pub mod account;
pub mod screens;
pub mod sheets;

use crate::app::{App, Route};
use crate::model::{AppData, DayPlan, Person};
use crate::sync::{Auth, Connection, Status};
use crate::time::{fmt, Day, LocalTime};
use std::fmt::Write;

pub fn esc(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(c),
        }
    }
    out
}

pub fn icon(name: &str) -> String {
    let path = match name {
        "back" => "M20 11H7.83l5.59-5.59L12 4l-8 8 8 8 1.41-1.41L7.83 13H20v-2z",
        "left" => "M15.41 7.41L14 6l-6 6 6 6 1.41-1.41L10.83 12z",
        "right" => "M10 6L8.59 7.41 13.17 12l-4.58 4.59L10 18l6-6z",
        "more" => "M12 8c1.1 0 2-.9 2-2s-.9-2-2-2-2 .9-2 2 .9 2 2 2zm0 2c-1.1 0-2 .9-2 2s.9 2 2 2 2-.9 2-2-.9-2-2-2zm0 6c-1.1 0-2 .9-2 2s.9 2 2 2 2-.9 2-2-.9-2-2-2z",
        "close" => "M19 6.41L17.59 5 12 10.59 6.41 5 5 6.41 10.59 12 5 17.59 6.41 19 12 13.41 17.59 19 19 17.59 13.41 12z",
        "edit" => "M3 17.25V21h3.75L17.81 9.94l-3.75-3.75L3 17.25zM20.71 7.04a1 1 0 0 0 0-1.41l-2.34-2.34a1 1 0 0 0-1.41 0l-1.83 1.83 3.75 3.75 1.83-1.83z",
        "bell" => "M12 22c1.1 0 2-.9 2-2h-4c0 1.1.89 2 2 2zm6-6v-5c0-3.07-1.64-5.64-4.5-6.32V4c0-.83-.67-1.5-1.5-1.5s-1.5.67-1.5 1.5v.68C7.63 5.36 6 7.92 6 11v5l-2 2v1h16v-1l-2-2z",
        "bell-off" => "M12 22c1.1 0 2-.9 2-2h-4c0 1.1.9 2 2 2zm6-6v-5c0-3.07-1.63-5.64-4.5-6.32V4c0-.83-.67-1.5-1.5-1.5s-1.5.67-1.5 1.5v.68C7.64 5.36 6 7.92 6 11v5l-2 2v1h16v-1l-2-2zm-2 1H8v-6c0-2.48 1.51-4.5 4-4.5s4 2.02 4 4.5v6z",
        "week" => "M9 11H7v2h2v-2zm4 0h-2v2h2v-2zm4 0h-2v2h2v-2zm2-7h-1V2h-2v2H8V2H6v2H5c-1.11 0-1.99.9-1.99 2L3 20c0 1.1.89 2 2 2h14c1.1 0 2-.9 2-2V6c0-1.1-.9-2-2-2zm0 16H5V9h14v11z",
        "settings" => "M19.14 12.94c.04-.3.06-.61.06-.94 0-.32-.02-.64-.07-.94l2.03-1.58a.49.49 0 0 0 .12-.61l-1.92-3.32a.49.49 0 0 0-.59-.22l-2.39.96c-.5-.38-1.03-.7-1.62-.94l-.36-2.54a.48.48 0 0 0-.48-.41h-3.84c-.24 0-.43.17-.47.41l-.36 2.54c-.59.24-1.13.57-1.62.94l-2.39-.96a.49.49 0 0 0-.59.22L2.74 8.87c-.12.21-.08.47.12.61l2.03 1.58c-.05.3-.09.63-.09.94s.02.64.07.94l-2.03 1.58a.49.49 0 0 0-.12.61l1.92 3.32c.12.22.37.29.59.22l2.39-.96c.5.38 1.03.7 1.62.94l.36 2.54c.05.24.24.41.48.41h3.84c.24 0 .44-.17.47-.41l.36-2.54c.59-.24 1.13-.56 1.62-.94l2.39.96c.22.08.47 0 .59-.22l1.92-3.32a.49.49 0 0 0-.12-.61l-2.01-1.58zM12 15.6A3.6 3.6 0 1 1 12 8.4a3.6 3.6 0 0 1 0 7.2z",
        "home" => "M10 20v-6h4v6h5v-8h3L12 3 2 12h3v8z",
        "people" => "M16 11c1.66 0 2.99-1.34 2.99-3S17.66 5 16 5s-3 1.34-3 3 1.34 3 3 3zm-8 0c1.66 0 2.99-1.34 2.99-3S9.66 5 8 5 5 6.34 5 8s1.34 3 3 3zm0 2c-2.33 0-7 1.17-7 3.5V19h14v-2.5c0-2.33-4.67-3.5-7-3.5zm8 0c-.29 0-.62.02-.97.05 1.16.84 1.97 1.97 1.97 3.45V19h6v-2.5c0-2.33-4.67-3.5-7-3.5z",
        "share" => "M12 2.5l-4.2 4.2 1.4 1.4L11 6.3V15h2V6.3l1.8 1.8 1.4-1.4L12 2.5zM5 10v10.5c0 .8.7 1.5 1.5 1.5h11c.8 0 1.5-.7 1.5-1.5V10h-4v2h2v8H7v-8h2v-2H5z",
        "add-box" => "M19 3H5a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2V5a2 2 0 0 0-2-2zm-2 10h-4v4h-2v-4H7v-2h4V7h2v4h4v2z",
        "car" => "M18.92 6.01C18.72 5.42 18.16 5 17.5 5h-11c-.66 0-1.21.42-1.42 1.01L3 12v8c0 .55.45 1 1 1h1c.55 0 1-.45 1-1v-1h12v1c0 .55.45 1 1 1h1c.55 0 1-.45 1-1v-8l-2.08-5.99zM6.5 16a1.5 1.5 0 1 1 0-3 1.5 1.5 0 0 1 0 3zm11 0a1.5 1.5 0 1 1 0-3 1.5 1.5 0 0 1 0 3zM5 11l1.5-4.5h11L19 11H5z",
        "inbox" => "M19 3H4.99c-1.11 0-1.98.89-1.98 2L3 19c0 1.1.88 2 1.99 2H19c1.1 0 2-.9 2-2V5c0-1.11-.9-2-2-2zm0 12h-4c0 1.66-1.35 3-3 3s-3-1.34-3-3H4.99V5H19v10z",
        _ => "",
    };
    format!(r#"<svg class="ic" viewBox="0 0 24 24" aria-hidden="true"><path d="{path}"/></svg>"#)
}

pub fn dot(p: Person, size: u32) -> String {
    format!(r#"<span class="dot" style="width:{size}px;height:{size}px;background:{}" title="{}"></span>"#, p.color(), p.name())
}

pub fn section(text: &str) -> String {
    format!(r#"<div class="section">{}</div>"#, esc(&text.to_uppercase()))
}

/// A button. `kind`: filled, tonal, outlined, text, icon.
pub fn btn(kind: &str, label: &str, act: &str, arg: &str) -> String {
    format!(r#"<button class="btn {kind}" data-act="{act}" data-arg="{}">{label}</button>"#, esc(arg))
}

pub fn btn_disabled(kind: &str, label: &str, act: &str, arg: &str, disabled: bool) -> String {
    if disabled {
        format!(r#"<button class="btn {kind}" disabled data-act="{act}" data-arg="{}">{label}</button>"#, esc(arg))
    } else {
        btn(kind, label, act, arg)
    }
}

pub fn icon_btn(name: &str, title: &str, act: &str, arg: &str, disabled: bool) -> String {
    format!(
        r#"<button class="btn icon" aria-label="{t}" title="{t}" data-act="{act}" data-arg="{}"{}>{}</button>"#,
        esc(arg),
        if disabled { " disabled" } else { "" },
        icon(name),
        t = esc(title)
    )
}

pub fn switch(on: bool, act: &str) -> String {
    format!(r#"<button class="switch{}" role="switch" aria-checked="{on}" data-act="{act}"><span></span></button>"#, if on { " on" } else { "" })
}

pub fn info_banner(title: &str, body: &str, actions: &str) -> String {
    format!(r#"<div class="card banner"><div class="t-title">{}</div><div class="t-body dim85">{}</div><div class="row gap8 wrap mt10">{actions}</div></div>"#, esc(title), esc(body))
}

pub fn greeting(now: LocalTime) -> &'static str {
    match now.hour() {
        5..=11 => "Good morning",
        12..=16 => "Good afternoon",
        17..=21 => "Good evening",
        _ => "Late night",
    }
}

/// One-line sync state, e.g. "Synced", "Offline · will sync", "Sign in to sync". Never alarming.
pub fn sync_text(s: &Status) -> (&'static str, &'static str) {
    match (s.auth, s.connection) {
        (Auth::NeedsPassword { .. }, _) => ("Sign in to sync", "accent"),
        (Auth::NoPerson, _) => ("Not synced", ""),
        (_, Connection::Online) if !s.pending && s.using_backup() => ("Synced (backup)", ""),
        (_, Connection::Online) if !s.pending => ("Synced", ""),
        (_, Connection::Online) => ("Syncing…", ""),
        (_, Connection::Connecting) => ("Connecting…", ""),
        _ if s.pending => ("Offline · will sync", ""),
        _ => ("Offline", ""),
    }
}

pub fn sync_status(s: &Status) -> String {
    let (text, cls) = sync_text(s);
    format!(r#"<button class="linkish {cls}" data-act="nav" data-arg="settings">{text}</button>"#)
}

fn badge_class(plan: Option<DayPlan>) -> &'static str {
    match plan {
        Some(DayPlan::Work { .. }) => "work",
        Some(DayPlan::Off) => "off",
        None => "none",
    }
}

pub fn date_badge(d: Day, plan: Option<DayPlan>) -> String {
    format!(
        r#"<div class="badge {}"><span class="t-label">{}</span><span class="badge-num">{}</span></div>"#,
        badge_class(plan),
        fmt::day_short(d).to_uppercase(),
        d.day_of_month()
    )
}

/// Dots for the *other* people working that day. Tap to compare.
pub fn others_dots(d: Day, data: &AppData) -> String {
    let people = data.others_working(d);
    if people.is_empty() {
        return String::new();
    }
    let dots: String = people.iter().map(|p| dot(*p, 14)).collect();
    format!(r#"<button class="others" aria-label="Who else is working" data-act="compare" data-arg="{}">{dots}</button>"#, d.0)
}

pub fn reminder_icon(d: Day, enabled: bool) -> String {
    format!(
        r#"<button class="btn icon{}" aria-label="{}" data-act="rem" data-arg="{}:{}">{}</button>"#,
        if enabled { " accent" } else { " dim" },
        if enabled { "Cancel reminder" } else { "Queue reminder" },
        d.0,
        if enabled { 0 } else { 1 },
        icon(if enabled { "bell" } else { "bell-off" })
    )
}

/// Carpool button for a day: always there, highlighted when someone starts or ends near you or a
/// carpool is already set up.
pub fn car_btn(d: Day, data: &AppData) -> String {
    use crate::model::Dir;
    let active = data.has_active_carpool(d, Dir::To) || data.has_active_carpool(d, Dir::From);
    let matched = !data.carpool_matches(d, Dir::To).is_empty() || !data.carpool_matches(d, Dir::From).is_empty();
    format!(
        r#"<button class="btn icon car{}" aria-label="Carpool" title="Carpool" data-act="carpool" data-arg="{}">{}</button>"#,
        if active { " on" } else if matched { " match" } else { " dim" },
        d.0,
        icon("car")
    )
}

/// Inbox button with an unread count.
pub fn inbox_btn(data: &AppData, act: &str) -> String {
    let n = data.unread_count();
    let badge = if n > 0 { format!(r#"<span class="badge-count">{}</span>"#, n.min(99)) } else { String::new() };
    format!(r#"<button class="btn icon inbox-btn" aria-label="Inbox ({n} unread)" data-act="{act}" data-arg="inbox">{}{badge}</button>"#, icon("inbox"))
}

pub fn day_card(d: Day, data: &AppData, now: LocalTime, label: Option<&str>, highlight: bool) -> String {
    let plan = data.plan(d);
    let use24 = data.settings.use24h;
    let mut body = String::new();
    let title = label.map(str::to_string).unwrap_or_else(|| fmt::day_name(d).to_string());
    let _ = write!(body, r#"<div class="t-title">{}</div>"#, esc(&title));
    match plan {
        Some(DayPlan::Work { start, end }) => {
            let leave = data.leave_time(d).unwrap_or(d.at(start));
            let _ = write!(
                body,
                r#"<div class="t-large">{}</div><div class="t-small dim">Leave {} · {}</div>"#,
                fmt::shift(start, end, use24),
                fmt::time(leave, use24),
                fmt::hours(end - start)
            );
        }
        Some(DayPlan::Off) => body.push_str(r#"<div class="t-large secondary">Day off</div>"#),
        None => body.push_str(r#"<div class="t-body dim">Not entered · tap to add</div>"#),
    }
    let remind = match (plan, data.reminder_time(d)) {
        (Some(DayPlan::Work { .. }), Some(at)) if at > now => reminder_icon(d, data.reminder_enabled(d)),
        _ => String::new(),
    };
    let ride = [crate::model::Dir::To, crate::model::Dir::From]
        .iter()
        .filter_map(|dir| data.ride_summary(d, *dir).map(|s| format!("{}: {s}", dir.title())))
        .collect::<Vec<_>>();
    if !ride.is_empty() {
        let _ = write!(body, r#"<div class="t-small accent">{}</div>"#, esc(&ride.join(" · ")));
    }
    format!(
        r#"<div class="card day-card{}" role="button" tabindex="0" data-act="edit" data-arg="{}">{}<div class="grow">{body}</div>{}{}{remind}</div>"#,
        if highlight { " hl" } else { "" },
        d.0,
        date_badge(d, plan),
        others_dots(d, data),
        car_btn(d, data)
    )
}

/// Seven-dot overview of a week; tap a day to edit it.
pub fn week_strip(week: Day, data: &AppData, today: Day) -> String {
    let mut out = String::from(r#"<div class="strip">"#);
    for d in AppData::week_days(week) {
        let plan = data.plan(d);
        let text = match plan {
            Some(DayPlan::Work { start, end }) => fmt::hours(end - start),
            Some(DayPlan::Off) => "off".into(),
            None => "–".into(),
        };
        let _ = write!(
            out,
            r#"<button class="strip-day{}" data-act="edit" data-arg="{}"><span class="t-label">{}</span><span class="pill {}">{text}</span><span class="today-dot"></span></button>"#,
            if d == today { " today" } else { "" },
            d.0,
            &fmt::day_short(d)[..1],
            badge_class(plan)
        );
    }
    out.push_str("</div>");
    out
}

pub fn shift_count(n: usize) -> String {
    format!("{n} shift{}", if n == 1 { "" } else { "s" })
}

// ---- top level --------------------------------------------------------------------------------

pub fn render_app() -> String {
    crate::app::with(|a| render(a, crate::browser::local_now()))
}

fn render(a: &App, now: LocalTime) -> String {
    let desktop = a.ui.desktop;
    let Some(me) = a.repo.data.me else {
        return account::picker(desktop);
    };
    if let Auth::NeedsPassword { has_password } = a.status.auth {
        if !a.status.password_skipped {
            return account::password(a, me, has_password, desktop);
        }
    }
    let screen = match a.ui.route() {
        Route::Home => screens::home(a, now),
        Route::Week(w) => screens::week(a, now, w),
        Route::Settings => screens::settings(a, now),
        Route::Inbox => screens::inbox(a),
    };
    let overlays = sheets::overlays(a, now);
    if desktop {
        format!(r#"<div class="layout-d" data-key="d">{}<main class="d-main">{screen}</main></div>{overlays}"#, sidebar(a, now))
    } else {
        format!(r#"<div class="layout-m" data-key="m">{screen}</div>{overlays}"#)
    }
}

fn sidebar(a: &App, now: LocalTime) -> String {
    let me = a.repo.data.me;
    let route = a.ui.route();
    let this_week = a.repo.data.active_work_date(now).week_start();
    let item = |active: bool, ic: &str, label: &str, arg: &str| {
        format!(r#"<button class="nav-item{}" data-act="side" data-arg="{arg}">{}<span>{label}</span></button>"#, if active { " active" } else { "" }, icon(ic))
    };
    let (text, cls) = sync_text(&a.status);
    format!(
        r#"<aside class="side"><div class="brand">Schedule<b>Zero</b></div>
<div class="side-me">{}<div><div class="t-title">{}</div><div class="t-small dim">{}</div></div></div>
<nav>{}{}{}{}</nav>
<div class="side-foot"><button class="linkish {cls}" data-act="side" data-arg="settings">{text}</button><div class="t-small dim">{}</div></div></aside>"#,
        me.map(|p| dot(p, 28)).unwrap_or_default(),
        greeting(now),
        fmt::full_date(now.day()),
        item(route == Route::Home, "home", "Today", "home"),
        item(matches!(route, Route::Week(_)), "week", "Week", &format!("week:{}", this_week.0)),
        item(route == Route::Inbox, "inbox", &match a.repo.data.unread_count() { 0 => "Inbox".to_string(), n => format!("Inbox <span class=\"badge-count inline\">{n}</span>") }, "inbox"),
        item(route == Route::Settings, "settings", "Settings", "settings"),
        if a.status.connection == Connection::Online { "Live" } else { "" },
    )
}

/// Mobile top bar. `back` shows an arrow; `actions` sit on the right.
pub fn top_bar(title_html: &str, back: bool, actions: &str) -> String {
    format!(
        r#"<header class="topbar">{}<div class="grow topbar-title">{title_html}</div><div class="row">{actions}</div></header>"#,
        if back { icon_btn("back", "Back", "back", "", false) } else { String::new() }
    )
}
