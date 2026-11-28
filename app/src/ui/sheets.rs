//! Day editor, time picker, "who's working" comparison and confirmations.
//! Bottom sheets on phones, centred dialogs on desktop (same markup, different CSS).

use super::*;
use crate::app::{App, Confirm, EditMode, Pick};
use crate::model::{work_time, AppData, DayPlan, Person};
use crate::time::{fmt, Day, LocalTime};
use std::fmt::Write;

fn overlay(key: &str, kind: &str, body: &str) -> String {
    format!(
        r#"<div class="overlay {kind}" data-key="{key}"><div class="scrim" data-act="close-overlay"></div><div class="sheet" role="dialog" aria-modal="true"><div class="grabber"></div>{body}</div></div>"#
    )
}

pub fn overlays(a: &App, now: LocalTime) -> String {
    let kind = if a.ui.desktop { "as-modal" } else { "as-sheet" };
    let mut out = String::new();
    if let Some(d) = a.ui.comparing {
        out.push_str(&overlay("ov-compare", &format!("{kind} wide"), &compare(d, &a.repo.data)));
    }
    if let Some(ed) = &a.ui.editor {
        out.push_str(&overlay("ov-editor", kind, &editor(ed, &a.repo.data, now)));
        if let Some(p) = ed.picking {
            out.push_str(&overlay("ov-picker", "as-dialog", &picker(ed, p, a.repo.data.settings.use24h)));
        }
    }
    if let Some(c) = a.ui.confirm {
        out.push_str(&overlay("ov-confirm", "as-dialog", &confirm(c, a)));
    }
    out
}

fn day_header(d: Day, nav_act: &str, sub: &str) -> String {
    format!(
        r#"<div class="row center">{}<div class="grow center-text"><div class="t-headline">{}</div><div class="t-body dim">{}</div></div>{}</div>"#,
        icon_btn("left", "Previous day", nav_act, &d.plus(-1).0.to_string(), false),
        fmt::day_name(d),
        esc(sub),
        icon_btn("right", "Next day", nav_act, &d.plus(1).0.to_string(), false)
    )
}

// ---- editor -----------------------------------------------------------------------------------

fn editor(ed: &crate::app::Editor, data: &AppData, now: LocalTime) -> String {
    let use24 = data.settings.use24h;
    let d = ed.date;
    let mut out = day_header(d, "ed-nav", &fmt::month_day(d));
    out.push_str(r#"<div class="segmented mt16">"#);
    for (i, label) in ["Working", "Day off", "Not set"].iter().enumerate() {
        let sel = matches!((i, ed.mode), (0, EditMode::Work) | (1, EditMode::Off) | (2, EditMode::Unset));
        let _ = write!(out, r#"<button class="seg{}" data-act="ed-mode" data-arg="{i}">{label}</button>"#, if sel { " on" } else { "" });
    }
    out.push_str("</div>");

    match ed.mode {
        EditMode::Work => {
            let field = |label: &str, m: i32, which: &str| {
                format!(
                    r#"<button class="time-field" data-act="ed-pick" data-arg="{which}"><span class="t-label accent">{label}</span><span class="time-big">{}</span><span class="t-small dim">{}</span></button>"#,
                    fmt::work_minutes(m, use24),
                    if work_time::is_next_day(m) { "next morning" } else { "&nbsp;" }
                )
            };
            let _ = write!(out, r#"<div class="row gap12 mt20">{}{}</div>"#, field("START", ed.start, "start"), field("END", ed.end, "end"));
            let leave = d.at(ed.start - data.settings.commute_minutes);
            let _ = write!(
                out,
                r#"<div class="t-small dim mt10">{} shift · leave home {}{}</div>"#,
                fmt::duration((ed.end - ed.start) as i64),
                fmt::time(leave, use24),
                if work_time::is_next_day(ed.end) { format!(" · ends after midnight (still counts as {})", fmt::day_name(d)) } else { String::new() }
            );
            let recents = data.recent_shifts(4);
            if !recents.is_empty() {
                out.push_str(&section("Quick pick"));
                out.push_str(r#"<div class="chips">"#);
                for (s, e) in recents {
                    let _ = write!(out, r#"<button class="chip" data-act="ed-quick" data-arg="{s}:{e}">{}</button>"#, fmt::shift(s, e, use24));
                }
                out.push_str("</div>");
            }
            out.push_str(&section("Reminder"));
            let remind_at = d.at(ed.start - data.settings.reminder_lead_minutes);
            let past = remind_at <= now;
            let sub = if past {
                "That time has already passed today.".to_string()
            } else {
                format!("{} before · at {}", fmt::lead(data.settings.reminder_lead_minutes), fmt::time(remind_at, use24))
            };
            let sw = if past {
                r#"<button class="switch" disabled role="switch" aria-checked="false"><span></span></button>"#.to_string()
            } else {
                switch(ed.reminder, "ed-rem")
            };
            let _ = write!(
                out,
                r#"<div class="card inner list-row"><div class="grow"><div class="t-title">Remind me before work</div><div class="t-small dim">{}</div></div>{sw}</div>"#,
                esc(&sub)
            );
        }
        EditMode::Off => out.push_str(r#"<p class="t-body dim mt20">Marked as a day off. No reminders will fire.</p>"#),
        EditMode::Unset => out.push_str(r#"<p class="t-body dim mt20">No info for this day yet. It will show as "Not entered".</p>"#),
    }
    let _ = write!(
        out,
        r#"<div class="row center gap8 mt24 sheet-actions">{}<span class="grow"></span>{}{}</div>"#,
        btn("text", "Cancel", "ed-cancel", ""),
        btn("tonal", "Save &amp; next", "ed-save-next", ""),
        btn("filled", "Save", "ed-save", "")
    );
    out
}

/// Scrollable list of 15-minute slots between noon and 2 AM.
fn picker(ed: &crate::app::Editor, which: Pick, use24: bool) -> String {
    let (title, selected) = match which {
        Pick::Start => ("Start time", ed.start),
        Pick::End => ("End time", ed.end),
    };
    let mut list = String::new();
    for m in work_time::options() {
        let ok = match which {
            Pick::Start => m < work_time::MAX,
            Pick::End => m > ed.start,
        };
        if !ok {
            continue;
        }
        let sel = m == selected;
        let _ = write!(
            list,
            r#"<button class="slot{}" data-act="ed-slot" data-arg="{m}"{}><span class="grow">{}</span>{}</button>"#,
            if sel { " on" } else { "" },
            if sel { " data-scroll-once" } else { "" },
            fmt::work_minutes(m, use24),
            if work_time::is_next_day(m) { r#"<span class="t-label">next day</span>"# } else { "" }
        );
    }
    format!(
        r#"<div class="t-headline-s">{title}</div><div class="slots mt12">{list}</div><div class="row mt12"><span class="grow"></span>{}</div>"#,
        btn("text", "Cancel", "close-overlay", "")
    )
}

// ---- compare ----------------------------------------------------------------------------------

fn plan_text(plan: Option<DayPlan>, use24: bool) -> String {
    match plan {
        Some(DayPlan::Work { start, end }) => fmt::shift(start, end, use24),
        Some(DayPlan::Off) => "Off".into(),
        None => "Not entered".into(),
    }
}

fn compact_plan(plan: Option<DayPlan>, use24: bool) -> String {
    match plan {
        Some(DayPlan::Work { start, end }) => format!("{}–{}", fmt::compact_hour(start, use24), fmt::compact_hour(end, use24)),
        Some(DayPlan::Off) => "off".into(),
        None => "–".into(),
    }
}

/// How `other`'s shift relates to yours, in plain words.
fn relation(mine: Option<DayPlan>, other: Option<DayPlan>) -> Option<String> {
    let (os, oe) = other.and_then(DayPlan::work)?;
    let Some((ms, me)) = mine.and_then(DayPlan::work) else {
        return (mine == Some(DayPlan::Off)).then(|| "You're off".to_string());
    };
    let diff = |a: i32, b: i32| {
        if a < b {
            format!("{} before you", fmt::duration((b - a) as i64))
        } else if a > b {
            format!("{} after you", fmt::duration((a - b) as i64))
        } else {
            "same time as you".to_string()
        }
    };
    let overlap = (me.min(oe) - ms.max(os)).max(0);
    Some(format!(
        "Starts {} · ends {} · {}",
        diff(os, ms),
        diff(oe, me),
        if overlap > 0 { format!("together {}", fmt::duration(overlap as i64)) } else { "no overlap".into() }
    ))
}

fn order(me: Option<Person>) -> Vec<Person> {
    let mut v: Vec<Person> = me.into_iter().collect();
    v.extend(Person::ALL.into_iter().filter(|p| Some(*p) != me));
    v
}

fn compare(d: Day, data: &AppData) -> String {
    let use24 = data.settings.use24h;
    let me = data.me;
    let span = (work_time::MAX - work_time::MIN) as f64;
    let mut rows = String::new();
    for p in order(me) {
        let plan = data.plan_for(p, d);
        let bar = match plan {
            Some(DayPlan::Work { start, end }) => format!(
                r#"<div class="bar" style="left:{:.2}%;width:{:.2}%;background:{}"></div>"#,
                (start - work_time::MIN) as f64 / span * 100.0,
                (end - start) as f64 / span * 100.0,
                p.color()
            ),
            _ => String::new(),
        };
        let rel = if Some(p) != me {
            relation(data.plan(d), plan).map(|r| format!(r#"<div class="t-small dim mt4">{}</div>"#, esc(&r))).unwrap_or_default()
        } else {
            String::new()
        };
        let _ = write!(
            rows,
            r#"<div class="cmp-row"><div class="row center gap8">{}<span class="t-title grow">{}</span><span class="t-body{}">{}</span></div><div class="track mt6">{bar}</div>{rel}</div>"#,
            dot(p, 12),
            if Some(p) == me { format!("{} (you)", p.name()) } else { p.name().to_string() },
            if matches!(plan, Some(DayPlan::Work { .. })) { "" } else { " dim" },
            plan_text(plan, use24)
        );
    }
    let axis: String = [(720, "12p"), (960, "4p"), (1200, "8p"), (1440, "12a"), (1560, "2a")]
        .iter()
        .map(|(m, l)| format!(r#"<span style="left:{:.2}%">{l}</span>"#, (*m - work_time::MIN) as f64 / span * 100.0))
        .collect();
    format!(
        r#"{}<div class="card inner pad16 mt16">{rows}<div class="axis t-label dim">{axis}</div></div>{}{}"#,
        day_header(d, "cmp-nav", &format!("{} · who's working", fmt::month_day(d))),
        section(&format!("Week of {}", fmt::month_day(d.week_start()))),
        week_table(d, d.week_start(), data)
    )
}

/// Everyone's week side by side; `selected` is highlighted.
pub fn week_table(selected: Day, week: Day, data: &AppData) -> String {
    let use24 = data.settings.use24h;
    let people = order(data.me);
    let mut out = String::from(r#"<div class="card inner wtable"><div class="wrow head"><span class="wday"></span>"#);
    for p in &people {
        let _ = write!(out, r#"<span class="wcell">{}<span class="t-label dim">{}</span></span>"#, dot(*p, 10), if Some(*p) == data.me { "You" } else { p.name() });
    }
    out.push_str("</div>");
    for d in AppData::week_days(week) {
        let _ = write!(
            out,
            r#"<div class="wrow{}" role="button" tabindex="0" data-act="compare" data-arg="{}"><span class="wday">{}</span>"#,
            if d == selected { " sel" } else { "" },
            d.0,
            fmt::day_short(d)
        );
        for p in &people {
            let plan = data.plan_for(*p, d);
            let _ = write!(out, r#"<span class="wcell t-small{}">{}</span>"#, if matches!(plan, Some(DayPlan::Work { .. })) { "" } else { " dim" }, compact_plan(plan, use24));
        }
        out.push_str("</div>");
    }
    out.push_str("</div>");
    out
}

// ---- confirmations ------------------------------------------------------------------------------

fn dialog(title: &str, body: &str, yes: Option<&str>) -> String {
    format!(
        r#"<div class="t-headline-s">{}</div><div class="t-body dim mt12">{body}</div><div class="row mt20"><span class="grow"></span>{}{}</div>"#,
        esc(title),
        btn("text", if yes.is_some() { "Cancel" } else { "Done" }, "confirm-no", ""),
        yes.map(|y| btn("text accent-text", y, "confirm-yes", "")).unwrap_or_default()
    )
}

fn confirm(c: Confirm, a: &App) -> String {
    match c {
        Confirm::CopyWeek { from, .. } => dialog(
            "Replace this week?",
            &format!("This week already has entries. Copying will overwrite them with {}.", fmt::week_range(from)),
            Some("Replace"),
        ),
        Confirm::ClearWeek(w) => dialog("Clear this week?", &format!("Removes every entry and queued reminder for {}.", fmt::week_range(w)), Some("Clear")),
        Confirm::ClearAll => dialog(
            "Clear everything?",
            "Every day you've entered is removed here and on the server (so on all your devices too). Other people's schedules and your settings are kept.",
            Some("Clear"),
        ),
        Confirm::SignOut => dialog(
            "Switch person?",
            &format!(
                "This device will forget {}. Your schedule stays on the server{}",
                a.repo.data.me.map(|p| p.name()).unwrap_or("you"),
                if a.status.pending { ", but edits that haven't synced yet will be lost." } else { "." }
            ),
            Some("Switch"),
        ),
        Confirm::InstallHelp => {
            let steps = if crate::app::is_ios() {
                format!(
                    r#"<ol class="steps"><li>Open this page in <b>Safari</b>.</li><li>Tap the <b>Share</b> button {}.</li><li>Scroll down and tap <b>Add to Home Screen</b> {}.</li><li>Tap <b>Add</b>, then open ScheduleZero from your Home Screen.</li><li>Sign in again there, then turn on reminders in Settings.</li></ol>"#,
                    icon("share"),
                    icon("add-box")
                )
            } else {
                r#"<ol class="steps"><li>Open your browser's menu (⋮ or ···).</li><li>Choose <b>Install app</b>, <b>Install ScheduleZero</b> or <b>Add to Home Screen</b>.</li><li>Open it from your apps or Home Screen.</li></ol>"#.to_string()
            };
            dialog("Add ScheduleZero to your Home Screen", &steps, None)
        }
    }
}
