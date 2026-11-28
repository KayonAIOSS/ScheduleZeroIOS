//! Home, Week and Settings. Same content as the Android screens; the desktop versions lay the
//! same pieces out in columns.

use super::*;
use crate::app::App;
use crate::model::{AppData, DayPlan, Dir, LEAD_MAX, LEAD_MIN, LEAD_STEP};
use crate::sync::{Auth, Connection};
use crate::time::{fmt, Day, LocalTime};
use std::fmt::Write;

const MAX_WEEKS_BACK: i32 = 8;
const MAX_WEEKS_AHEAD: i32 = 8;

// ---- Home -------------------------------------------------------------------------------------

pub fn home(a: &App, now: LocalTime) -> String {
    let data = &a.repo.data;
    let active = data.active_work_date(now);
    let today = now.day();
    let this_week = active.week_start();
    let next_week = this_week.plus(7);
    let last_week = this_week.plus(-7);

    let mut banners = String::new();
    banners.push_str(&install_banner(a));
    banners.push_str(&carpool_suggestions(data, active, now));
    let waiting = data.carpools_awaiting_me();
    if waiting > 0 {
        banners.push_str(&info_banner(
            if waiting == 1 { "A carpool needs your answer" } else { "Carpools need your answer" },
            "Someone asked for or offered a ride. Answer yes or no in the inbox.",
            &btn("filled", "Open inbox", if a.ui.desktop { "side" } else { "nav" }, "inbox"),
        ));
    }
    if !data.has_any_entry(this_week) {
        let mut actions = btn("filled", "Enter week", "nav", &format!("week:{}", this_week.0));
        if data.has_any_entry(last_week) {
            actions.push_str(&btn("outlined", "Copy last week", "copy-week", &format!("{}:{}", last_week.0, this_week.0)));
        }
        banners.push_str(&info_banner(
            "This week's schedule isn't in yet",
            "Weeks reset Sunday at 11:59 PM so old hours never carry over. Add this week's times to get started.",
            &actions,
        ));
    } else if today.weekday() >= 4 && !data.has_any_entry(next_week) {
        banners.push_str(&info_banner(
            "Got next week's hours?",
            "Enter them any time — they'll load automatically when the week rolls over at Sunday 11:59 PM.",
            &btn("filled", "Add next week", "nav", &format!("week:{}", next_week.0)),
        ));
    }

    let mut upcoming = String::new();
    for i in 1..=7 {
        let d = active.plus(i);
        let label = if d == today.plus(1) {
            Some("Tomorrow")
        } else if d == today {
            Some("Today")
        } else {
            None
        };
        upcoming.push_str(&day_card(d, data, now, label, false));
    }
    let hero = hero(data, active, now);
    let summary = week_summary(data, this_week, today);
    let plan_next = format!(r#"<button class="btn text full" data-act="nav" data-arg="week:{}">Plan next week →</button>"#, next_week.0);

    if a.ui.desktop {
        return format!(
            r#"<div class="d-head"><div><div class="t-headline">{}</div><div class="t-body dim">{}</div></div></div>
<div class="d-cols"><div class="col">{hero}{banners}{summary}</div><div class="col">{}{upcoming}{plan_next}</div></div>"#,
            greeting(now),
            fmt::full_date(today),
            section("Coming up")
        );
    }
    let title = format!(
        r#"<div class="row center gap10">{}<span class="t-title-l">{}</span></div><div class="row center t-small dim">{} ·&nbsp;{}</div>"#,
        dot(data.me.unwrap_or(crate::model::Person::Kayo), 14),
        greeting(now),
        fmt::full_date(today),
        sync_status(&a.status)
    );
    let actions = format!(
        "{}{}{}",
        inbox_btn(data, "nav"),
        icon_btn("week", "Week schedule", "nav", &format!("week:{}", this_week.0), false),
        icon_btn("settings", "Settings", "nav", "settings", false)
    );
    format!(
        r#"{}<div class="content">{hero}{banners}{summary}{}{upcoming}{plan_next}</div>"#,
        top_bar(&title, false, &actions),
        section("Coming up")
    )
}

fn install_banner(a: &App) -> String {
    if a.ui.install_dismissed || crate::app::is_standalone() {
        return String::new();
    }
    if crate::app::is_ios() {
        return info_banner(
            "Add ScheduleZero to your Home Screen",
            "It opens full screen like an app, and it's the only way iPhone allows shift reminders.",
            &format!("{}{}", btn("filled", "Show me how", "install-help", ""), btn("text", "Not now", "install-dismiss", "")),
        );
    }
    if crate::browser::glue_bool("szCanInstall") {
        return info_banner(
            "Install ScheduleZero",
            "Get it as an app on this device, with its own icon and window.",
            &format!("{}{}", btn("filled", "Install", "install-app", ""), btn("text", "Not now", "install-dismiss", "")),
        );
    }
    String::new()
}

fn hero(data: &AppData, d: Day, now: LocalTime) -> String {
    let plan = data.plan(d);
    let carry_over = d != now.day();
    let use24 = data.settings.use24h;
    let label = if carry_over { format!("TONIGHT · {}'S SHIFT", fmt::day_name(d).to_uppercase()) } else { "TODAY".into() };
    let body = match plan {
        None => format!(
            r#"<div class="t-headline">No schedule yet</div><div class="t-body dim mt6">Add today's hours to see when to leave.</div><div class="mt14">{}</div>"#,
            btn("filled", "Add today's hours", "edit", &d.0.to_string())
        ),
        Some(DayPlan::Off) => {
            let next = (1..=14).map(|i| d.plus(i)).find(|x| data.work_plan(*x).is_some());
            let text = match next {
                Some(n) => format!("Next shift: {} at {}", fmt::day_name(n), fmt::work_minutes(data.work_plan(n).map(|w| w.0).unwrap_or(0), use24)),
                None => "Nothing scheduled. Enjoy it.".into(),
            };
            format!(r#"<div class="t-display">Day off</div><div class="t-body-l dim mt6">{}</div>"#, esc(&text))
        }
        Some(DayPlan::Work { start, end }) => work_hero(data, d, start, end, now),
    };
    let rides: Vec<String> = [Dir::To, Dir::From]
        .iter()
        .filter_map(|dir| data.ride_summary(d, *dir).map(|s| format!("{}: {s}", dir.title())))
        .collect();
    let ride_line = if rides.is_empty() {
        String::new()
    } else {
        format!(r#"<div class="ride-line mt14">{}<span>{}</span></div>"#, icon("car"), esc(&rides.join(" · ")))
    };
    format!(
        r#"<div class="card hero"><div class="row center"><span class="t-label accent grow">{}</span>{}{}</div>{body}{ride_line}{}</div>"#,
        esc(&label),
        car_btn(d, data),
        icon_btn("edit", "Edit today", "edit", &d.0.to_string(), false),
        also_working(data, d)
    )
}

fn work_hero(data: &AppData, d: Day, start_m: i32, end_m: i32, now: LocalTime) -> String {
    let use24 = data.settings.use24h;
    let leave = data.leave_time(d).unwrap_or(d.at(start_m));
    let start = d.at(start_m);
    let end = d.at(end_m);
    let home = data.home_time(d).unwrap_or(end);
    let (label, big, sub) = if now < leave {
        ("Leave in", fmt::duration(now.minutes_until(leave)), format!("Head out at {}", fmt::time(leave, use24)))
    } else if now < start {
        ("Shift starts in", fmt::duration(now.minutes_until(start)), "You should be on the road".into())
    } else if now < end {
        ("Shift ends in", fmt::duration(now.minutes_until(end)), format!("Off at {}", fmt::time(end, use24)))
    } else if now < home {
        ("Home in about", fmt::duration(now.minutes_until(home)), "Drive safe".into())
    } else {
        ("Shift complete", "Done".into(), "Nice work today".into())
    };
    let mut out = format!(r#"<div class="t-title dim">{label}</div><div class="t-display">{big}</div><div class="t-body-l accent">{}</div>"#, esc(&sub));
    if now >= start && now < end {
        let pct = ((now.0 - start.0) as f64 / (end.0 - start.0).max(1) as f64 * 100.0).clamp(0.0, 100.0);
        let _ = write!(out, r#"<div class="progress mt12"><div style="width:{pct:.1}%"></div></div>"#);
    }
    let _ = write!(
        out,
        r#"<div class="tiles mt18">{}{}{}{}</div>"#,
        tile("Leave", leave, d, use24, false),
        tile("Start", start, d, use24, true),
        tile("End", end, d, use24, true),
        tile("Home", home, d, use24, false)
    );
    let remind = match data.reminder_time(d) {
        Some(at) if at > now => {
            let on = data.reminder_enabled(d);
            format!(
                r#"<button class="btn tonal" data-act="rem" data-arg="{}:{}">{}<span>{}</span></button>"#,
                d.0,
                if on { 0 } else { 1 },
                icon(if on { "bell" } else { "bell-off" }),
                if on { format!("Reminder {}", fmt::time(at, use24)) } else { "Remind me".into() }
            )
        }
        _ => String::new(),
    };
    let _ = write!(
        out,
        r#"<div class="row center mt14 gap8"><span class="t-small dim grow">{} shift · {} min drive</span>{remind}</div>"#,
        fmt::duration((end_m - start_m) as i64),
        data.settings.commute_minutes
    );
    out
}

fn tile(label: &str, t: LocalTime, work_day: Day, use24: bool, emphasize: bool) -> String {
    let text = fmt::time(t, use24);
    let (main, ampm) = if use24 { (text.clone(), String::new()) } else {
        let mut p = text.splitn(2, ' ');
        (p.next().unwrap_or("").to_string(), p.next().unwrap_or("").to_string())
    };
    let mut sub = ampm;
    if t.day() > work_day {
        sub.push_str(if sub.is_empty() { "+1 day" } else { " +1" });
    }
    if sub.is_empty() {
        sub.push_str("&nbsp;");
    }
    format!(
        r#"<div class="tile{}"><span class="t-label">{}</span><span class="tile-time">{main}</span><span class="t-label">{sub}</span></div>"#,
        if emphasize { " em" } else { "" },
        label.to_uppercase()
    )
}

/// "Also working" strip on the hero card; tap to compare schedules.
fn also_working(data: &AppData, d: Day) -> String {
    let people = data.others_working(d);
    if people.is_empty() {
        return String::new();
    }
    let mut rows = String::new();
    for p in people {
        if let Some(DayPlan::Work { start, end }) = data.plan_for(p, d) {
            let _ = write!(
                rows,
                r#"<div class="row center gap8">{}<span class="grow">{}</span><span>{}</span></div>"#,
                dot(p, 12),
                p.name(),
                fmt::shift(start, end, data.settings.use24h)
            );
        }
    }
    format!(r#"<div class="also mt14" role="button" tabindex="0" data-act="compare" data-arg="{}"><span class="t-label dim">ALSO WORKING</span>{rows}</div>"#, d.0)
}

fn week_summary(data: &AppData, week: Day, today: Day) -> String {
    let minutes = data.week_minutes(week);
    let pay = data
        .settings
        .hourly_rate
        .map(|r| format!(r#"<div class="t-small dim">≈ {} before tax</div>"#, fmt::money(r * minutes as f64 / 60.0)))
        .unwrap_or_default();
    format!(
        r#"<div class="card summary" role="button" tabindex="0" data-act="nav" data-arg="week:{}"><div class="row end"><div class="grow"><div class="t-label accent">THIS WEEK</div><div class="t-small dim">{}</div></div><div class="right"><div class="t-title">{} · {}</div>{pay}</div></div><div class="mt12">{}</div></div>"#,
        week.0,
        fmt::week_range(week),
        fmt::hours(minutes),
        shift_count(data.week_shift_count(week)),
        week_strip(week, data, today)
    )
}

// ---- Week -------------------------------------------------------------------------------------

fn relative_week(offset: i32) -> String {
    match offset {
        0 => "This week".into(),
        1 => "Next week".into(),
        -1 => "Last week".into(),
        n if n > 1 => format!("In {n} weeks"),
        n => format!("{} weeks ago", -n),
    }
}

pub fn week(a: &App, now: LocalTime, week: Day) -> String {
    let data = &a.repo.data;
    let active = data.active_work_date(now);
    let current = active.week_start();
    let prev = week.plus(-7);
    let offset = (week.0 - current.0) / 7;
    let nav = format!(
        r#"<div class="row center">{}<div class="grow center-text"><div class="t-title-l">{}</div><div class="t-body dim">{}</div></div>{}</div>{}"#,
        icon_btn("left", "Previous week", "week-go", &prev.0.to_string(), offset <= -MAX_WEEKS_BACK),
        relative_week(offset),
        fmt::week_range(week),
        icon_btn("right", "Next week", "week-go", &week.plus(7).0.to_string(), offset >= MAX_WEEKS_AHEAD),
        if offset != 0 { format!(r#"<button class="btn text full" data-act="week-go" data-arg="{}">Jump to this week</button>"#, current.0) } else { String::new() }
    );
    let empty = if !data.has_any_entry(week) {
        let mut actions = btn("filled", "Start with Monday", "edit", &week.0.to_string());
        if data.has_any_entry(prev) {
            actions.push_str(&btn("outlined", "Copy previous", "copy-week", &format!("{}:{}", prev.0, week.0)));
        }
        info_banner("Nothing entered for this week", "Tap a day below to add hours, then use \"Save & next\" to move through the week.", &actions)
    } else {
        String::new()
    };
    let help = r#"<p class="t-small dim center-text help">Weeks run Monday to Sunday and reset at Sunday 11:59 PM, so last week's hours are never reused by accident. You can enter next week any time before then and it'll load on its own. Shifts that end after midnight (up to 2 AM) count toward the day they started.</p>"#;
    let has_prev = data.has_any_entry(prev);
    let has_unset = AppData::week_days(week).iter().any(|d| data.plan(*d).is_none());
    let has_any = data.has_any_entry(week);
    let tools = [
        ("Copy previous week", "copy-week", format!("{}:{}", prev.0, week.0), !has_prev),
        ("Mark unset days as off", "mark-off", week.0.to_string(), !has_unset),
        ("Clear this week", "clear-week", week.0.to_string(), !has_any),
    ];

    if a.ui.desktop {
        let tool_btns: String = tools.iter().map(|(l, act, arg, dis)| btn_disabled("outlined", l, act, arg, *dis)).collect();
        let mut grid = String::new();
        for d in AppData::week_days(week) {
            grid.push_str(&day_tile(d, data, now, d == active));
        }
        return format!(
            r#"<div class="d-head"><div class="t-headline">Week schedule</div><div class="row gap8 wrap">{tool_btns}</div></div>
<div class="card pad16 week-nav">{nav}</div>{}{empty}<div class="week-grid">{grid}</div>{}{}{help}"#,
            week_stats(data, week),
            section("Everyone this week"),
            sheets::week_table(active, week, data)
        );
    }

    let mut days = String::new();
    for d in AppData::week_days(week) {
        let label = (d == active).then(|| format!("{} · Today", fmt::day_name(d)));
        days.push_str(&day_card(d, data, now, label.as_deref(), d == active));
    }
    let menu = if a.ui.menu_open {
        let items: String = tools
            .iter()
            .map(|(l, act, arg, dis)| format!(r#"<button class="menu-item" data-act="{act}" data-arg="{arg}"{}>{l}</button>"#, if *dis { " disabled" } else { "" }))
            .collect();
        format!(r#"<div class="menu-scrim" data-act="menu"></div><div class="menu">{items}</div>"#)
    } else {
        String::new()
    };
    format!(
        r#"{}{menu}<div class="content">{nav}{}{empty}{}{days}{help}</div>"#,
        top_bar(r#"<span class="t-title-l">Week schedule</span>"#, true, &icon_btn("more", "More", "menu", "", false)),
        week_stats(data, week),
        section("Monday – Sunday")
    )
}

fn week_stats(data: &AppData, week: Day) -> String {
    let days = AppData::week_days(week);
    let minutes = data.week_minutes(week);
    let off = days.iter().filter(|d| data.plan(**d) == Some(DayPlan::Off)).count();
    let unset = days.iter().filter(|d| data.plan(**d).is_none()).count();
    let pay = data
        .settings
        .hourly_rate
        .map(|r| format!(r#"<div class="t-small dim center-text pb12">Estimated pay ≈ {} before tax</div>"#, fmt::money(r * minutes as f64 / 60.0)))
        .unwrap_or_default();
    let stat = |v: String, l: &str| format!(r#"<div class="stat"><div class="t-title-l">{v}</div><div class="t-small dim">{l}</div></div>"#);
    format!(
        r#"<div class="card stats"><div class="row">{}{}{}{}</div>{pay}</div>"#,
        stat(fmt::hours(minutes), "hours"),
        stat(data.week_shift_count(week).to_string(), "shifts"),
        stat(off.to_string(), "off"),
        stat(unset.to_string(), "not set")
    )
}

/// Desktop week grid cell.
fn day_tile(d: Day, data: &AppData, now: LocalTime, today: bool) -> String {
    let use24 = data.settings.use24h;
    let plan = data.plan(d);
    let body = match plan {
        Some(DayPlan::Work { start, end }) => format!(
            r#"<div class="t-title">{}</div><div class="t-title">– {}</div><div class="t-small dim">{} · leave {}</div>"#,
            fmt::work_minutes(start, use24),
            fmt::work_minutes(end, use24),
            fmt::hours(end - start),
            fmt::time(data.leave_time(d).unwrap_or(d.at(start)), use24)
        ),
        Some(DayPlan::Off) => r#"<div class="t-title secondary">Day off</div>"#.into(),
        None => r#"<div class="t-body dim">Not entered</div>"#.into(),
    };
    let remind = match (plan, data.reminder_time(d)) {
        (Some(DayPlan::Work { .. }), Some(at)) if at > now => reminder_icon(d, data.reminder_enabled(d)),
        _ => String::new(),
    };
    format!(
        r#"<div class="card tile-day {}{}" role="button" tabindex="0" data-act="edit" data-arg="{}"><div class="row center"><span class="t-label grow">{} {}</span>{remind}</div>{body}<div class="grow"></div><div class="row center"><span class="grow row">{}</span>{}</div></div>"#,
        match plan {
            Some(DayPlan::Work { .. }) => "work",
            Some(DayPlan::Off) => "off",
            None => "none",
        },
        if today { " hl" } else { "" },
        d.0,
        fmt::day_short(d).to_uppercase(),
        d.day_of_month(),
        others_dots(d, data),
        car_btn(d, data)
    )
}

// ---- Settings ---------------------------------------------------------------------------------

fn group(title: &str, body: &str) -> String {
    format!(r#"<div class="group"><div class="section">{}</div><div class="card list">{body}</div></div>"#, esc(&title.to_uppercase()))
}

fn toggle_row(title: &str, sub: &str, on: bool, act: &str) -> String {
    format!(
        r#"<div class="list-row"><div class="grow"><div class="t-title">{}</div><div class="t-small dim">{}</div></div>{}</div>"#,
        esc(title),
        esc(sub),
        switch(on, act)
    )
}

fn action_row(title: &str, sub: &str, label: &str, act: &str) -> String {
    format!(
        r#"<div class="list-row"><div class="grow"><div class="t-title">{}</div><div class="t-small dim">{}</div></div>{}</div>"#,
        esc(title),
        esc(sub),
        btn("tonal", label, act, "")
    )
}

fn stepper(title: &str, value: &str, dec: &str, inc: &str, can_dec: bool, can_inc: bool) -> String {
    format!(
        r#"<div class="list-row"><div class="grow t-title">{}</div>{}<span class="t-title accent stepper-val">{}</span>{}</div>"#,
        esc(title),
        icon_btn("left", "Decrease", dec, "", !can_dec),
        esc(value),
        icon_btn("right", "Increase", inc, "", !can_inc)
    )
}

pub fn settings(a: &App, now: LocalTime) -> String {
    let data = &a.repo.data;
    let s = &data.settings;
    let mut groups: Vec<String> = Vec::new();

    // Account & sync
    if let Some(me) = data.me {
        let ago = a.status.last_sync.map(|t| {
            let mins = (crate::browser::now_ms() - t) / 60_000;
            if mins < 1 { "just now".to_string() } else { format!("{} ago", fmt::duration(mins)) }
        });
        let line = match (a.status.auth, a.status.connection) {
            (Auth::NeedsPassword { .. }, _) => "Not signed in. Tap Sign in.".to_string(),
            (_, Connection::Online) => format!(
                "Connected to {}{}",
                if a.status.using_backup() { "backup server" } else { "main server" },
                ago.as_ref().map(|x| format!(" · synced {x}")).unwrap_or_default()
            ),
            (_, Connection::Connecting) => "Connecting…".into(),
            _ if a.status.pending => "Offline. Your changes will upload when connected.".into(),
            _ => format!("Offline{}", ago.as_ref().map(|x| format!(" · last synced {x}")).unwrap_or_default()),
        };
        let sign_in = matches!(a.status.auth, Auth::NeedsPassword { .. });
        let loc = crate::browser::window().location();
        let main_url = format!("{}{}", loc.origin().unwrap_or_default(), crate::browser::glue_string("szBase"));
        let typed = crate::sync::clean_server(&a.ui.backup_text);
        let changed = typed != a.saved.secondary;
        let https_page = main_url.starts_with("https://");
        let warn = if https_page && typed.starts_with("http://") {
            r#"<div class="t-small dim">Note: browsers block http:// servers from an https page, so this backup only works when the app is opened over http (e.g. on the tailnet). An https:// address works everywhere.</div>"#
        } else {
            ""
        };
        groups.push(group(
            "Account & sync",
            &format!(
                r#"<div class="list-row">{}<div class="grow px14"><div class="t-title">{}</div><div class="t-small dim">{}</div></div>{}</div><div class="divider"></div>
<div class="list-col"><span class="field-label">Main server (this site)</span><div class="fixed-field">{}</div>
<label class="field mt10"><span class="field-label">Backup server</span><span class="field-box"><input type="url" inputmode="url" autocomplete="off" autocapitalize="off" spellcheck="false" id="backup" data-input="backup" data-enter="backup-save" value="{}" placeholder="https://… (optional)"></span></label>
{warn}<div class="t-small dim">Used automatically when this site's server can't be reached. The two servers also keep each other in sync.</div>
<div class="row center mt4"><span class="grow"></span>{}{}</div></div><div class="divider"></div>{}"#,
                dot(me, 28),
                me.name(),
                esc(&line),
                btn("tonal", if sign_in { "Sign in" } else { "Sync now" }, "sync-now", ""),
                esc(&main_url),
                esc(&a.ui.backup_text),
                format!(
                    "{}{}",
                    if a.saved.secondary == crate::sync::DEFAULT_SECONDARY { String::new() } else { btn("text", "Use default", "backup-default", "") },
                    if a.saved.secondary.is_empty() { String::new() } else { btn("text", "Remove", "backup-clear", "") }
                ),
                btn_disabled("text", "Save", "backup-save", "", !changed),
                action_row("Switch person", "Sign this device in as someone else.", "Switch", "switch-person")
            ),
        ));
    }

    groups.push(group(
        "Appearance",
        &format!(
            r#"{}<div class="divider"></div>{}"#,
            toggle_row("Dark mode", "Pitch black with purple. Off = light with cyan.", s.dark_mode, "s-dark"),
            toggle_row("24-hour clock", "Show 16:00 instead of 4:00 PM", s.use24h, "s-24h")
        ),
    ));

    // Reminders
    let mut rem = toggle_row(
        "Remind me before every shift",
        "Off by default. When off, tap the bell on a day to queue a one-off reminder.",
        s.default_reminders,
        "s-defrem",
    );
    let _ = write!(
        rem,
        r#"<div class="divider"></div><div class="list-col"><div class="row center"><span class="t-title grow">Remind me</span><span class="t-title accent">{} before work</span></div><input type="range" class="slider" min="{LEAD_MIN}" max="{LEAD_MAX}" step="{LEAD_STEP}" value="{}" data-input="lead" aria-label="Reminder lead time"><div class="t-small dim">15 minutes to 3 hours, in 15-minute steps. Applies to every reminder.</div></div>"#,
        fmt::lead(s.reminder_lead_minutes),
        s.reminder_lead_minutes
    );
    if let Some((title, sub)) = crate::push::blocker() {
        rem.push_str(r#"<div class="divider"></div>"#);
        let button = if title.starts_with("Add to Home") {
            btn("tonal", "How", "install-help", "")
        } else if title == "Notifications are off" {
            btn_disabled("tonal", if a.push.busy { "…" } else { "Allow" }, "notif-enable", "", a.push.busy)
        } else {
            String::new()
        };
        let _ = write!(
            rem,
            r#"<div class="list-row"><div class="grow"><div class="t-title">{}</div><div class="t-small dim">{}</div></div>{button}</div>"#,
            esc(title),
            esc(sub)
        );
    }
    rem.push_str(r#"<div class="divider"></div>"#);
    rem.push_str(&action_row("Test notification", "See what a reminder looks like.", "Send", "notif-test"));
    if let Some(m) = &a.push.message {
        let _ = write!(rem, r#"<div class="list-row t-small accent">{}</div>"#, esc(m));
    }
    groups.push(group("Reminders", &rem));

    // Queued reminders (same 3-week window as Android).
    let mut queued = String::new();
    let start = now.day().plus(-1);
    for i in 0..=21 {
        let d = start.plus(i);
        let (Some(at), Some((ws, we))) = (data.reminder_time(d), data.work_plan(d)) else { continue };
        if !data.reminder_enabled(d) || at <= now {
            continue;
        }
        if !queued.is_empty() {
            queued.push_str(r#"<div class="divider"></div>"#);
        }
        let _ = write!(
            queued,
            r#"<div class="list-row tight"><div class="grow"><div class="t-title">{}, {}</div><div class="t-small dim">Alert at {} · shift {}</div></div>{}</div>"#,
            fmt::day_name(d),
            fmt::month_day(d),
            fmt::time(at, s.use24h),
            fmt::shift(ws, we, s.use24h),
            icon_btn("close", "Remove reminder", "rem-remove", &d.0.to_string(), false)
        );
    }
    if queued.is_empty() {
        queued = r#"<div class="list-row t-body dim">None queued. Tap the bell on any work day to add one.</div>"#.into();
    }
    groups.push(group("Queued reminders", &queued));

    let mut weekly = toggle_row("Sunday reminder to enter hours", "Only fires if that week is still empty.", s.weekly_prompt, "s-weekly");
    if s.weekly_prompt {
        weekly.push_str(r#"<div class="divider"></div>"#);
        let m = s.weekly_prompt_minutes;
        weekly.push_str(&stepper(
            "Sunday at",
            &fmt::time(LocalTime(m as i64 * 60), s.use24h),
            "weekly-dec",
            "weekly-inc",
            m > 6 * 60,
            m < 23 * 60 + 45,
        ));
    }
    groups.push(group("Weekly check-in", &weekly));

    let window_text = match s.carpool_window {
        0 => "Same time".to_string(),
        m => format!("Within {m} min"),
    };
    groups.push(group(
        "Carpool",
        &format!(
            r#"{}<div class="divider"></div>{}<div class="divider"></div>{}<div class="divider"></div>{}"#,
            toggle_row("Carpool notifications", "Requests, offers and answers. Yes/No is always answered in the app.", s.carpool_notifications, "s-cp-notif"),
            toggle_row("Suggest carpools", "Ask \"carpool?\" when someone starts or ends work when you do.", s.suggest_carpools, "s-cp-suggest"),
            stepper(
                "Counts as the same time",
                &window_text,
                "cpwin-dec",
                "cpwin-inc",
                s.carpool_window > 0,
                s.carpool_window < *crate::model::CARPOOL_WINDOWS.last().unwrap_or(&60)
            ),
            toggle_row("Carpool in shift reminders", "Shift reminders say who's driving.", s.carpool_in_reminders, "s-cp-rem")
        ),
    ));

    groups.push(group(
        "Commute & pay",
        &format!(
            r#"{}<div class="divider"></div><div class="list-col"><label class="field"><span class="field-label">Hourly pay (optional)</span><span class="field-box"><span class="dim">$</span><input type="text" inputmode="decimal" autocomplete="off" id="rate" data-input="rate" value="{}" placeholder="0.00"></span></label><div class="t-small dim">Shows estimated weekly pay. Leave blank to hide.</div></div>"#,
            stepper("Drive time", &format!("{} min", s.commute_minutes), "commute-dec", "commute-inc", s.commute_minutes > 5, s.commute_minutes < 120),
            esc(&a.ui.rate_text)
        ),
    ));

    if !crate::app::is_standalone() {
        let (sub, label, act) = if crate::app::is_ios() {
            ("Opens full screen and allows reminders on iPhone.", "How", "install-help")
        } else if crate::browser::glue_bool("szCanInstall") {
            ("Its own icon and window, like a normal app.", "Install", "install-app")
        } else {
            ("Use your browser's menu: Install app / Add to Home Screen.", "How", "install-help")
        };
        groups.push(group("Install", &action_row("Add to this device", sub, label, act)));
    }

    groups.push(group("Data", &action_row("Clear all my schedules", "Removes every day you've entered, on all your devices.", "Clear", "clear-all")));

    let footer = format!(
        r#"<div class="t-small dim center-text mt8">ScheduleZero web {}{}</div>"#,
        env!("CARGO_PKG_VERSION"),
        a.ui.server_version.as_ref().map(|v| format!(" · server {}", esc(v))).unwrap_or_default()
    );
    let all: String = groups.concat();
    if a.ui.desktop {
        return format!(r#"<div class="d-head"><div class="t-headline">Settings</div></div><div class="settings-grid">{all}</div>{footer}"#);
    }
    format!(
        r#"{}<div class="content">{all}{footer}</div>"#,
        top_bar(r#"<span class="t-title-l">Settings</span>"#, true, "")
    )
}

// ---- Carpool suggestions & Inbox ----------------------------------------------------------------

/// "Carpool Monday?" cards for the coming week, when someone starts or ends near you and nothing
/// is set up yet. To work and home are separate suggestions.
fn carpool_suggestions(data: &AppData, from: Day, now: LocalTime) -> String {
    if !data.settings.suggest_carpools {
        return String::new();
    }
    let mut out = String::new();
    let mut shown = 0;
    for i in 0..7 {
        let d = from.plus(i);
        for dir in [Dir::To, Dir::From] {
            if shown >= 2 {
                return out;
            }
            let key = format!("{}:{}", d.0, dir.id());
            let people = data.carpool_matches(d, dir);
            let Some((s, e)) = data.work_plan(d) else { continue };
            let when = d.at(if dir == Dir::To { s } else { e });
            if people.is_empty() || when <= now || data.has_active_carpool(d, dir) || data.dismissed_suggestions.contains(&key) {
                continue;
            }
            let names: Vec<&str> = people.iter().map(|p| p.name()).collect();
            let verb = if dir == Dir::To { "start" } else { "finish" };
            let day_word = if d == now.day() { "today".to_string() } else { fmt::day_name(d).to_string() };
            let body = format!(
                "{} also {verb}{} around {} {}. Ask for a ride or offer one?",
                names.join(" and "),
                if people.len() == 1 { "s" } else { "" },
                fmt::time(when, data.settings.use24h),
                day_word
            );
            out.push_str(&format!(
                r#"<div class="card banner suggest"><div class="row center gap8">{}<div class="t-title">Carpool {} {}?</div></div><div class="t-body dim85">{}</div><div class="row gap8 wrap mt10">{}{}</div></div>"#,
                icon("car"),
                dir.words(),
                esc(&day_word),
                esc(&body),
                btn("filled", "Plan carpool", "carpool", &d.0.to_string()),
                btn("text", "Not now", "cp-dismiss", &key)
            ));
            shown += 1;
        }
    }
    out
}

fn ago(ms: i64) -> String {
    let mins = (crate::browser::now_ms() - ms).max(0) / 60_000;
    match mins {
        0 => "just now".into(),
        m if m < 60 => format!("{m}m ago"),
        m if m < 24 * 60 => format!("{}h ago", m / 60),
        m => format!("{}d ago", m / (24 * 60)),
    }
}

pub fn inbox(a: &App) -> String {
    let data = &a.repo.data;
    let mut list = String::new();
    for m in &data.inbox {
        let unread = m.read == 0;
        let sender = crate::model::Person::from_id(&m.from).map(|p| dot(p, 12)).unwrap_or_default();
        let mark = if unread { btn("text", "Mark as read", "inbox-read", &m.id) } else { String::new() };
        list.push_str(&format!(
            r#"<div class="card msg{}" data-key="msg-{}"><div class="row center gap8">{}{sender}<span class="t-title grow">{}</span><span class="t-small dim nowrap">{}</span></div><div class="t-body dim mt4">{}</div><div class="row center mt6"><span class="grow"></span>{mark}{}</div></div>"#,
            if unread { " unread" } else { "" },
            esc(&m.id),
            if unread { r#"<span class="unread-dot"></span>"# } else { "" },
            esc(&m.title),
            ago(m.created),
            esc(&m.body),
            btn("tonal", "Open", "inbox-open", &m.id)
        ));
    }
    if list.is_empty() {
        list = r#"<div class="card pad16 t-body dim center-text">No messages yet. Carpool requests, offers and answers show up here.</div>"#.into();
    }
    let unread = data.unread_count();
    let all = btn_disabled("text", "Mark all read", "inbox-all", "", unread == 0);
    if a.ui.desktop {
        return format!(r#"<div class="d-head"><div class="t-headline">Inbox</div>{all}</div><div class="inbox-list">{list}</div>"#);
    }
    format!(
        r#"{}<div class="content">{list}</div>"#,
        top_bar(r#"<span class="t-title-l">Inbox</span>"#, true, &all)
    )
}
