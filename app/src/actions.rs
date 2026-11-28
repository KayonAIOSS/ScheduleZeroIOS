//! What every tap does. Names match the `data-act` attributes written by `ui`.

use crate::app::{self, commit, commit_schedule, with, with_mut, Confirm, EditMode, Pick, Route};
use crate::browser::{self, window};
use crate::carpool;
use crate::model::{work_time, Dir, Person, CARPOOL_WINDOWS, LEAD_MAX, LEAD_MIN, LEAD_STEP};
use crate::sync::{self, PasswordResult};
use crate::time::Day;
use wasm_bindgen::JsValue;
use wasm_bindgen_futures::spawn_local;

fn day(arg: &str) -> Option<Day> {
    arg.parse::<i32>().ok().map(Day)
}

fn parse_route(arg: &str) -> Route {
    if arg == "settings" {
        Route::Settings
    } else if arg == "inbox" {
        Route::Inbox
    } else if let Some(w) = arg.strip_prefix("week:").and_then(day) {
        Route::Week(w.week_start())
    } else {
        Route::Home
    }
}

fn push_route(r: Route) {
    with_mut(|a| {
        a.ui.stack.push(r);
        a.ui.menu_open = false;
    });
    if let Ok(h) = window().history() {
        let _ = h.push_state_with_url(&JsValue::NULL, "", Some("#"));
    }
    scroll_top();
}

/// Desktop sidebar: jump straight to a section instead of stacking screens.
fn side_route(r: Route) {
    with_mut(|a| {
        a.ui.stack = if r == Route::Home { vec![Route::Home] } else { vec![Route::Home, r] };
        a.ui.menu_open = false;
    });
    scroll_top();
}

fn scroll_top() {
    window().scroll_to_with_x_and_y(0.0, 0.0);
}

fn go_back() {
    if with(|a| a.ui.stack.len() > 1) {
        if let Ok(h) = window().history() {
            if h.back().is_ok() {
                return; // popstate pops the screen
            }
        }
        history_back();
    }
}

/// Browser/OS back button.
pub fn history_back() {
    let closed_overlay = with_mut(|a| {
        a.ui.menu_open = false;
        if a.ui.editor.is_some() || a.ui.comparing.is_some() || a.ui.confirm.is_some() || a.ui.carpool.is_some() {
            a.ui.editor = None;
            a.ui.comparing = None;
            a.ui.confirm = None;
            a.ui.carpool = None;
            true
        } else {
            if a.ui.stack.len() > 1 {
                a.ui.stack.pop();
            }
            false
        }
    });
    // Back closed a sheet instead of leaving the screen: put the history entry back so the
    // screen stack and the browser history stay the same length.
    if closed_overlay && with(|a| a.ui.stack.len() > 1) {
        if let Ok(h) = window().history() {
            let _ = h.push_state_with_url(&JsValue::NULL, "", Some("#"));
        }
    }
}

fn close_overlay() {
    with_mut(|a| {
        if let Some(ed) = a.ui.editor.as_mut() {
            if ed.picking.is_some() {
                ed.picking = None;
                return;
            }
        }
        if a.ui.confirm.is_some() {
            a.ui.confirm = None;
        } else if a.ui.menu_open {
            a.ui.menu_open = false;
        } else if a.ui.carpool.as_ref().is_some_and(|s| s.compose.is_some()) {
            if let Some(s) = a.ui.carpool.as_mut() {
                s.compose = None;
            }
        } else {
            a.ui.editor = None;
            a.ui.comparing = None;
            a.ui.carpool = None;
        }
    });
}

fn editor(f: impl FnOnce(&mut app::Editor)) {
    with_mut(|a| {
        if let Some(ed) = a.ui.editor.as_mut() {
            f(ed);
        }
    });
}

pub fn click(act: &str, arg: &str) {
    match act {
        // ---- navigation ----
        "nav" => push_route(parse_route(arg)),
        "side" => side_route(parse_route(arg)),
        "back" => go_back(),
        "week-go" => {
            if let Some(w) = day(arg) {
                with_mut(|a| {
                    if let Some(Route::Week(cur)) = a.ui.stack.last_mut() {
                        *cur = w.week_start();
                    }
                });
            }
        }
        "menu" => with_mut(|a| a.ui.menu_open = !a.ui.menu_open),
        "close-overlay" => close_overlay(),

        // ---- week tools ----
        "copy-week" => {
            // arg "from:to" (epoch days of the two Mondays)
            let mut it = arg.split(':').filter_map(|s| s.parse::<i32>().ok());
            if let (Some(from), Some(to)) = (it.next(), it.next()) {
                let (from, to) = (Day(from), Day(to));
                let has = with(|a| a.repo.data.has_any_entry(to));
                with_mut(|a| a.ui.menu_open = false);
                if has {
                    with_mut(|a| a.ui.confirm = Some(Confirm::CopyWeek { from, to }));
                } else {
                    with_mut(|a| a.repo.copy_week(from, to, crate::sync::edit_time()));
                    commit_schedule();
                }
            }
        }
        "mark-off" => {
            if let Some(w) = day(arg) {
                with_mut(|a| {
                    a.ui.menu_open = false;
                    a.repo.mark_unset_off(w, crate::sync::edit_time());
                });
                commit_schedule();
            }
        }
        "clear-week" => {
            if let Some(w) = day(arg) {
                with_mut(|a| {
                    a.ui.menu_open = false;
                    a.ui.confirm = Some(Confirm::ClearWeek(w));
                });
            }
        }

        // ---- days ----
        "edit" => {
            if let Some(d) = day(arg) {
                app::open_editor(d);
            }
        }
        "compare" => {
            if let Some(d) = day(arg) {
                with_mut(|a| {
                    a.ui.comparing = Some(d);
                    a.ui.editor = None;
                });
            }
        }
        "cmp-nav" => {
            if let Some(d) = day(arg) {
                with_mut(|a| a.ui.comparing = Some(d));
            }
        }
        "rem" => {
            let mut it = arg.split(':');
            if let (Some(d), Some(on)) = (it.next().and_then(day), it.next()) {
                app::toggle_reminder(d, on == "1");
            }
        }

        // ---- day editor ----
        "ed-mode" => editor(|ed| {
            ed.mode = match arg {
                "1" => EditMode::Off,
                "2" => EditMode::Unset,
                _ => EditMode::Work,
            }
        }),
        "ed-pick" => editor(|ed| ed.picking = Some(if arg == "end" { Pick::End } else { Pick::Start })),
        "ed-slot" => {
            if let Ok(m) = arg.parse::<i32>() {
                editor(|ed| {
                    match ed.picking {
                        Some(Pick::Start) => {
                            let len = (ed.end - ed.start).max(work_time::STEP);
                            ed.start = m;
                            if ed.end <= ed.start {
                                ed.end = (ed.start + len).min(work_time::MAX);
                            }
                            if ed.end <= ed.start {
                                ed.start = ed.end - work_time::STEP;
                            }
                        }
                        Some(Pick::End) => ed.end = m,
                        None => {}
                    }
                    ed.picking = None;
                });
            }
        }
        "ed-quick" => {
            let mut it = arg.split(':').filter_map(|s| s.parse::<i32>().ok());
            if let (Some(s), Some(e)) = (it.next(), it.next()) {
                editor(|ed| {
                    ed.start = s;
                    ed.end = e;
                });
            }
        }
        "ed-rem" => editor(|ed| ed.reminder = !ed.reminder),
        "ed-nav" => {
            if let Some(d) = day(arg) {
                app::open_editor(d);
            }
        }
        "ed-save" => app::save_editor(false),
        "ed-save-next" => app::save_editor(true),
        "ed-cancel" => with_mut(|a| a.ui.editor = None),

        // ---- confirmations ----
        "confirm-yes" => {
            let c = with_mut(|a| a.ui.confirm.take());
            match c {
                Some(Confirm::CopyWeek { from, to }) => {
                    with_mut(|a| a.repo.copy_week(from, to, crate::sync::edit_time()));
                    commit_schedule();
                }
                Some(Confirm::ClearWeek(w)) => {
                    with_mut(|a| a.repo.clear_week(w, crate::sync::edit_time()));
                    commit_schedule();
                }
                Some(Confirm::ClearAll) => {
                    with_mut(|a| a.repo.clear_all_schedules(crate::sync::edit_time()));
                    commit_schedule();
                }
                Some(Confirm::SignOut) => {
                    with_mut(|a| a.ui.stack = vec![Route::Home]);
                    sync::sign_out();
                }
                Some(Confirm::InstallHelp) | None => {}
            }
        }
        "confirm-no" => with_mut(|a| a.ui.confirm = None),

        // ---- settings ----
        "s-dark" => setting(|s| s.dark_mode = !s.dark_mode),
        "theme" => sync::set_theme(arg),
        "s-24h" => setting(|s| s.use24h = !s.use24h),
        "s-defrem" => {
            let turning_on = with(|a| !a.repo.data.settings.default_reminders);
            if turning_on {
                crate::push::ensure_enabled_from_gesture();
            }
            setting(|s| s.default_reminders = !s.default_reminders);
        }
        "s-weekly" => {
            let turning_on = with(|a| !a.repo.data.settings.weekly_prompt);
            if turning_on {
                crate::push::ensure_enabled_from_gesture();
            }
            setting(|s| s.weekly_prompt = !s.weekly_prompt);
        }
        "weekly-dec" => setting(|s| s.weekly_prompt_minutes = (s.weekly_prompt_minutes - 15).max(6 * 60)),
        "weekly-inc" => setting(|s| s.weekly_prompt_minutes = (s.weekly_prompt_minutes + 15).min(23 * 60 + 45)),
        "commute-dec" => setting(|s| s.commute_minutes = (s.commute_minutes - 5).max(5)),
        "commute-inc" => setting(|s| s.commute_minutes = (s.commute_minutes + 5).min(120)),
        "notif-enable" => crate::push::ensure_enabled_from_gesture(),
        "notif-test" => crate::push::send_test(),
        "rem-remove" => {
            if let Some(d) = day(arg) {
                app::toggle_reminder(d, false);
            }
        }
        "clear-all" => with_mut(|a| a.ui.confirm = Some(Confirm::ClearAll)),
        "sync-now" => sync::refresh_auth_state(),
        "switch-person" => with_mut(|a| a.ui.confirm = Some(Confirm::SignOut)),
        "install-help" => with_mut(|a| a.ui.confirm = Some(Confirm::InstallHelp)),
        "install-dismiss" => app::dismiss_install(),
        "install-app" => {
            browser::glue("szInstall", &[]);
        }

        // ---- account ----
        "pick" => {
            if let Some(p) = Person::from_id(arg) {
                with_mut(|a| {
                    a.ui.password.clear();
                    a.ui.password2.clear();
                    a.ui.password_error = None;
                });
                sync::choose_person(p);
            }
        }
        "pw-submit" => submit_password(),
        "pw-skip" => sync::skip_password(),
        "pw-switch" => sync::sign_out(),

        // ---- carpools ----
        "carpool" => {
            if let Some(d) = day(arg) {
                carpool::open(d);
            }
        }
        "cp-nav" => {
            if let Some(d) = day(arg) {
                carpool::navigate(d);
            }
        }
        "cp-compose" => {
            // arg "to:request", "from:offer", ...
            let mut it = arg.split(':');
            if let (Some(dir), Some(kind)) = (it.next().and_then(Dir::parse), it.next()) {
                carpool::compose(dir, if kind == "offer" { "offer" } else { "request" });
            }
        }
        "cp-to" => {
            if let Some(p) = Person::from_id(arg) {
                carpool::toggle_recipient(p);
            }
        }
        "cp-send" => carpool::send(),
        "cp-compose-cancel" => carpool::cancel_compose(),
        "cp-yes" => carpool::answer(arg, true),
        "cp-no" => carpool::answer(arg, false),
        "cp-cancel" => carpool::cancel(arg),
        "cp-dismiss" => carpool::dismiss_suggestion(arg),

        // ---- inbox ----
        "inbox-read" => carpool::mark_read(Some(vec![arg.to_string()])),
        "inbox-all" => carpool::mark_read(None),
        "inbox-open" => carpool::open_message(arg),

        // ---- carpool & server settings ----
        "s-cp-notif" => setting(|s| s.carpool_notifications = !s.carpool_notifications),
        "s-cp-suggest" => setting(|s| s.suggest_carpools = !s.suggest_carpools),
        "s-cp-rem" => setting(|s| s.carpool_in_reminders = !s.carpool_in_reminders),
        "cpwin-dec" | "cpwin-inc" => setting(|s| {
            let i = CARPOOL_WINDOWS.iter().position(|w| *w == s.carpool_window).unwrap_or(0);
            let j = if act == "cpwin-inc" { (i + 1).min(CARPOOL_WINDOWS.len() - 1) } else { i.saturating_sub(1) };
            s.carpool_window = CARPOOL_WINDOWS[j];
        }),
        "backup-save" => {
            let text = with(|a| a.ui.backup_text.clone());
            sync::set_secondary(&text);
        }
        "backup-clear" => sync::set_secondary(""),
        "backup-default" => sync::set_secondary(sync::DEFAULT_SECONDARY),
        _ => {}
    }
}

fn setting(f: impl FnOnce(&mut crate::model::Settings)) {
    with_mut(|a| a.repo.update_settings(f));
    commit();
}

fn submit_password() {
    let (pw, pw2, setup, busy) = with(|a| {
        let setup = matches!(a.status.auth, sync::Auth::NeedsPassword { has_password: false });
        (a.ui.password.clone(), a.ui.password2.clone(), setup, a.ui.password_busy)
    });
    if busy {
        return;
    }
    let early = if pw.chars().count() < 4 {
        Some("Use at least 4 characters.")
    } else if setup && pw != pw2 {
        Some("Passwords don't match.")
    } else {
        None
    };
    if let Some(e) = early {
        with_mut(|a| a.ui.password_error = Some(e.into()));
        return;
    }
    with_mut(|a| {
        a.ui.password_busy = true;
        a.ui.password_error = None;
    });
    spawn_local(async move {
        let r = sync::submit_password(pw, setup).await;
        with_mut(|a| {
            a.ui.password_busy = false;
            a.ui.password_error = match r {
                PasswordResult::Ok => None,
                PasswordResult::Wrong => Some("That password isn't right.".into()),
                PasswordResult::TooShort => Some("Use at least 4 characters.".into()),
                PasswordResult::AlreadySet => Some("A password was just set from another device. Enter that one.".into()),
                PasswordResult::NotSet => Some("No password yet. Create one.".into()),
                PasswordResult::Unreachable => Some("Can't reach the server right now. Try again, or use offline for now.".into()),
            };
            if matches!(r, PasswordResult::AlreadySet | PasswordResult::NotSet | PasswordResult::Ok) {
                a.ui.password.clear();
                a.ui.password2.clear();
            }
        });
        crate::dom::request_render();
    });
}

/// Text and slider input. Returns true to redraw.
pub fn input(name: &str, value: &str) -> bool {
    match name {
        "pw" => with_mut(|a| {
            a.ui.password = value.to_string();
            a.ui.password_error = None;
        }),
        "pw2" => with_mut(|a| {
            a.ui.password2 = value.to_string();
            a.ui.password_error = None;
        }),
        "lead" => {
            if let Ok(f) = value.parse::<f64>() {
                let v = (((f / LEAD_STEP as f64).round() as i32) * LEAD_STEP).clamp(LEAD_MIN, LEAD_MAX);
                if with(|a| a.repo.data.settings.reminder_lead_minutes != v) {
                    setting(|s| s.reminder_lead_minutes = v);
                }
            }
        }
        "cp-note" => carpool::set_note(value),
        "backup" => with_mut(|a| a.ui.backup_text = value.to_string()),
        "rate" => {
            let clean: String = value.chars().filter(|c| c.is_ascii_digit() || *c == '.').take(8).collect();
            let rate = clean.parse::<f64>().ok().filter(|v| *v > 0.0);
            with_mut(|a| {
                a.ui.rate_text = clean;
                a.repo.update_settings(|s| s.hourly_rate = rate);
            });
            commit();
        }
        _ => return false,
    }
    true
}

pub fn change(_name: &str, _value: &str, _checked: bool) {}
