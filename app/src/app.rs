//! Global app state. Everything runs on the browser's single thread, so state lives in one
//! thread-local `RefCell`. Never hold a borrow across an `.await`.

use crate::browser::{self, local_now, now_ms, storage_get, storage_set};
use crate::model::{DayPlan, Person};
use crate::push::PushState;
use crate::store::Repo;
use crate::sync::{SavedSync, Status};
use crate::time::Day;
use std::cell::RefCell;

const DATA_KEY: &str = "sz.data.v1";

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Route {
    Home,
    /// Week screen showing the week starting on this Monday.
    Week(Day),
    Settings,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum EditMode {
    Work,
    Off,
    Unset,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Pick {
    Start,
    End,
}

#[derive(Clone, PartialEq, Debug)]
pub struct Editor {
    pub date: Day,
    pub mode: EditMode,
    pub start: i32,
    pub end: i32,
    pub reminder: bool,
    pub picking: Option<Pick>,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Confirm {
    CopyWeek { from: Day, to: Day },
    ClearWeek(Day),
    ClearAll,
    SignOut,
    InstallHelp,
}

#[derive(Clone, Debug, Default)]
pub struct Ui {
    pub stack: Vec<Route>,
    pub editor: Option<Editor>,
    pub comparing: Option<Day>,
    pub confirm: Option<Confirm>,
    pub menu_open: bool,
    pub desktop: bool,
    pub password: String,
    pub password2: String,
    pub password_error: Option<String>,
    pub password_busy: bool,
    pub rate_text: String,
    pub install_dismissed: bool,
    pub server_version: Option<String>,
}

impl Ui {
    pub fn route(&self) -> Route {
        *self.stack.last().unwrap_or(&Route::Home)
    }
}

pub struct App {
    pub repo: Repo,
    pub saved: SavedSync,
    pub status: Status,
    pub ui: Ui,
    pub push: PushState,
}

thread_local! {
    static APP: RefCell<Option<App>> = const { RefCell::new(None) };
}

pub fn init() {
    let repo = Repo::load(storage_get(DATA_KEY).as_deref(), local_now().day());
    let saved = SavedSync::load();
    let rate_text = repo.data.settings.hourly_rate.map(|r| format!("{r:.2}")).unwrap_or_default();
    let status = Status::initial(&repo, &saved);
    let ui = Ui {
        stack: vec![Route::Home],
        rate_text,
        install_dismissed: storage_get("sz.installDismissed").is_some(),
        ..Default::default()
    };
    APP.with(|a| *a.borrow_mut() = Some(App { repo, saved, status, ui, push: PushState::load() }));
}

pub fn with<R>(f: impl FnOnce(&App) -> R) -> R {
    APP.with(|a| f(a.borrow().as_ref().expect("app not initialised")))
}

pub fn with_mut<R>(f: impl FnOnce(&mut App) -> R) -> R {
    APP.with(|a| f(a.borrow_mut().as_mut().expect("app not initialised")))
}

/// Saves local data and asks for a redraw. Call after any change to `repo`.
pub fn commit() {
    with(|a| storage_set(DATA_KEY, &a.repo.to_json()));
    crate::push::schedule_sync_soon();
    crate::dom::request_render();
}

/// A change to the schedule itself: save, then upload right away.
pub fn commit_schedule() {
    with_mut(|a| a.status.pending = a.repo.has_pending_uploads());
    commit();
    crate::sync::push_soon();
}

// ---- edits shared by several screens --------------------------------------------------------

pub fn toggle_reminder(d: Day, on: bool) {
    if on {
        crate::push::ensure_enabled_from_gesture();
    }
    with_mut(|a| a.repo.set_reminder(d, on));
    commit();
}

pub fn save_editor(next: bool) {
    let Some(ed) = with(|a| a.ui.editor.clone()) else { return };
    let plan = match ed.mode {
        EditMode::Work => Some(DayPlan::Work { start: ed.start, end: ed.end }),
        EditMode::Off => Some(DayPlan::Off),
        EditMode::Unset => None,
    };
    let now = local_now();
    with_mut(|a| {
        a.repo.set_plan(ed.date, plan, now_ms());
        if let (Some(DayPlan::Work { .. }), Some(at)) = (plan, a.repo.data.reminder_time(ed.date)) {
            if at > now && ed.reminder != a.repo.data.reminder_enabled(ed.date) {
                a.repo.set_reminder(ed.date, ed.reminder);
            }
        }
    });
    if ed.reminder && plan.is_some() {
        crate::push::ensure_enabled_from_gesture();
    }
    commit_schedule();
    if next {
        open_editor(ed.date.plus(1));
    } else {
        with_mut(|a| a.ui.editor = None);
    }
}

pub fn open_editor(d: Day) {
    with_mut(|a| {
        let data = &a.repo.data;
        let existing = data.plan(d);
        let suggested = data
            .recent_shifts(1)
            .first()
            .copied()
            .unwrap_or((crate::model::work_time::DEFAULT_START, crate::model::work_time::DEFAULT_START + crate::model::work_time::DEFAULT_LENGTH));
        let (start, end) = existing.and_then(DayPlan::work).unwrap_or(suggested);
        a.ui.editor = Some(Editor {
            date: d,
            mode: if existing == Some(DayPlan::Off) { EditMode::Off } else { EditMode::Work },
            start,
            end,
            reminder: data.reminder_enabled(d),
            picking: None,
        });
        a.ui.comparing = None;
    });
    crate::dom::request_render();
}

pub fn me() -> Option<Person> {
    with(|a| a.repo.data.me)
}

pub fn dismiss_install() {
    storage_set("sz.installDismissed", "1");
    with_mut(|a| a.ui.install_dismissed = true);
    crate::dom::request_render();
}

pub fn is_ios() -> bool {
    browser::glue_bool("szIsIOS")
}

pub fn is_standalone() -> bool {
    browser::glue_bool("szIsStandalone")
}
