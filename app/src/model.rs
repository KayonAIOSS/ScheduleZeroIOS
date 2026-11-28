//! Same data model and rules as the Android app (`data/Models.kt`, `data/Person.kt`).

use crate::time::{Day, LocalTime};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// The only three accounts. IDs match the server (see server/PROTOCOL.md).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Person {
    Samy,
    Austin,
    Kayo,
}

impl Person {
    pub const ALL: [Person; 3] = [Person::Samy, Person::Austin, Person::Kayo];

    pub fn id(self) -> &'static str {
        match self {
            Person::Samy => "samy",
            Person::Austin => "austin",
            Person::Kayo => "kayo",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Person::Samy => "Samy",
            Person::Austin => "Austin",
            Person::Kayo => "Kayo",
        }
    }

    pub fn color(self) -> &'static str {
        match self {
            Person::Samy => "#E53935",
            Person::Austin => "#1E88E5",
            Person::Kayo => "#9C4DFF",
        }
    }

    pub fn index(self) -> usize {
        self as usize
    }

    pub fn from_id(id: &str) -> Option<Person> {
        Person::ALL.into_iter().find(|p| p.id() == id)
    }
}

pub mod work_time {
    pub const MIN: i32 = 12 * 60;
    pub const MAX: i32 = 26 * 60;
    pub const STEP: i32 = 15;
    pub const DEFAULT_START: i32 = 16 * 60;
    pub const DEFAULT_LENGTH: i32 = 8 * 60;

    pub fn options() -> impl Iterator<Item = i32> {
        (MIN..=MAX).step_by(STEP as usize)
    }

    pub fn is_next_day(minutes: i32) -> bool {
        minutes >= 24 * 60
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
#[serde(tag = "t", rename_all = "lowercase")]
pub enum DayPlan {
    Work { start: i32, end: i32 },
    Off,
}

impl DayPlan {
    pub fn work(self) -> Option<(i32, i32)> {
        match self {
            DayPlan::Work { start, end } => Some((start, end)),
            DayPlan::Off => None,
        }
    }
}

pub const LEAD_MIN: i32 = 15;
pub const LEAD_MAX: i32 = 180;
pub const LEAD_STEP: i32 = 15;

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Settings {
    pub dark_mode: bool,
    pub use24h: bool,
    pub default_reminders: bool,
    pub reminder_lead_minutes: i32,
    pub commute_minutes: i32,
    pub weekly_prompt: bool,
    /// Minutes after midnight on Sunday for the "enter your schedule" prompt.
    pub weekly_prompt_minutes: i32,
    pub hourly_rate: Option<f64>,
    /// Notify about carpool requests, offers and answers.
    pub carpool_notifications: bool,
    /// Ask "carpool?" when someone starts or ends work at about the same time as you.
    pub suggest_carpools: bool,
    /// How close start/end times must be to count as a carpool match, in minutes (0 = same time).
    pub carpool_window: i32,
    /// Shift reminders mention who you're riding with.
    pub carpool_in_reminders: bool,
}

pub const CARPOOL_WINDOWS: [i32; 4] = [0, 15, 30, 60];

impl Default for Settings {
    fn default() -> Self {
        Settings {
            dark_mode: true,
            use24h: false,
            default_reminders: false,
            reminder_lead_minutes: 60,
            commute_minutes: 30,
            weekly_prompt: false,
            weekly_prompt_minutes: 10 * 60,
            hourly_rate: None,
            carpool_notifications: true,
            suggest_carpools: true,
            carpool_window: 0,
            carpool_in_reminders: true,
        }
    }
}

/// To work, or home from work. The two are independent carpools.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Dir {
    To,
    From,
}

impl Dir {
    pub fn id(self) -> &'static str {
        match self {
            Dir::To => "to",
            Dir::From => "from",
        }
    }
    pub fn parse(s: &str) -> Option<Dir> {
        match s {
            "to" => Some(Dir::To),
            "from" => Some(Dir::From),
            _ => None,
        }
    }
    pub fn words(self) -> &'static str {
        match self {
            Dir::To => "to work",
            Dir::From => "home from work",
        }
    }
    pub fn title(self) -> &'static str {
        match self {
            Dir::To => "To work",
            Dir::From => "Home from work",
        }
    }
}

#[derive(Clone, PartialEq, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Answer {
    pub answer: String,
    pub at: i64,
}

/// A ride request ("can someone drive me?") or offer ("I can drive you"), as the server keeps it.
#[derive(Clone, PartialEq, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Carpool {
    pub id: String,
    pub from: String,
    /// "request" or "offer"
    pub kind: String,
    pub day: String,
    pub dir: String,
    pub to: Vec<String>,
    pub note: String,
    pub created: i64,
    pub cancelled: i64,
    pub responses: BTreeMap<String, Answer>,
}

impl Carpool {
    pub fn day(&self) -> Option<Day> {
        Day::parse_iso(&self.day)
    }
    pub fn direction(&self) -> Option<Dir> {
        Dir::parse(&self.dir)
    }
    pub fn sender(&self) -> Option<Person> {
        Person::from_id(&self.from)
    }
    pub fn recipients(&self) -> Vec<Person> {
        self.to.iter().filter_map(|p| Person::from_id(p)).collect()
    }
    pub fn answer(&self, p: Person) -> Option<&str> {
        self.responses.get(p.id()).map(|a| a.answer.as_str())
    }
    pub fn involves(&self, p: Person) -> bool {
        self.from == p.id() || self.to.iter().any(|t| t == p.id())
    }
    pub fn is_request(&self) -> bool {
        self.kind == "request"
    }
    /// Who drives whom, for accepted answers: (driver, rider).
    pub fn rides(&self) -> Vec<(Person, Person)> {
        let Some(sender) = self.sender() else { return vec![] };
        if self.cancelled > 0 {
            return vec![];
        }
        self.recipients()
            .into_iter()
            .filter(|p| self.answer(*p) == Some("yes"))
            .map(|p| if self.is_request() { (p, sender) } else { (sender, p) })
            .collect()
    }
}

/// One inbox message (carpool request, offer, answer or cancellation).
#[derive(Clone, PartialEq, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct InboxMessage {
    pub id: String,
    pub from: String,
    pub kind: String,
    pub carpool: String,
    pub title: String,
    pub body: String,
    pub created: i64,
    pub read: i64,
}

#[derive(Clone, PartialEq, Debug, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct AppData {
    /// This device's person's days, keyed by epoch day.
    pub plans: BTreeMap<i32, DayPlan>,
    /// Per-day reminder choice. Missing = follow `settings.default_reminders`.
    pub reminder_overrides: BTreeMap<i32, bool>,
    pub settings: Settings,
    /// Who this device is signed in as (None until chosen).
    pub me: Option<Person>,
    /// Everyone else's schedules as last received from the server, indexed by `Person::index`.
    pub others: [BTreeMap<i32, DayPlan>; 3],
    /// Everyone's carpools, as last received from the server.
    pub carpools: Vec<Carpool>,
    /// This person's messages, newest first.
    pub inbox: Vec<InboxMessage>,
    /// "Carpool?" suggestions this device said "Not now" to ("<epoch day>:<to|from>").
    pub dismissed_suggestions: std::collections::BTreeSet<String>,
}

impl AppData {
    pub fn plan(&self, d: Day) -> Option<DayPlan> {
        self.plans.get(&d.0).copied()
    }

    /// Other people working on `d`, in fixed order.
    pub fn others_working(&self, d: Day) -> Vec<Person> {
        Person::ALL
            .into_iter()
            .filter(|p| Some(*p) != self.me && matches!(self.others[p.index()].get(&d.0), Some(DayPlan::Work { .. })))
            .collect()
    }

    pub fn plan_for(&self, p: Person, d: Day) -> Option<DayPlan> {
        if Some(p) == self.me {
            self.plan(d)
        } else {
            self.others[p.index()].get(&d.0).copied()
        }
    }

    pub fn reminder_enabled(&self, d: Day) -> bool {
        self.reminder_overrides.get(&d.0).copied().unwrap_or(self.settings.default_reminders)
    }

    pub fn work_plan(&self, d: Day) -> Option<(i32, i32)> {
        self.plan(d).and_then(DayPlan::work)
    }

    pub fn shift_start(&self, d: Day) -> Option<LocalTime> {
        self.work_plan(d).map(|(s, _)| d.at(s))
    }

    pub fn shift_end(&self, d: Day) -> Option<LocalTime> {
        self.work_plan(d).map(|(_, e)| d.at(e))
    }

    pub fn leave_time(&self, d: Day) -> Option<LocalTime> {
        self.shift_start(d).map(|t| t.plus_minutes(-(self.settings.commute_minutes as i64)))
    }

    pub fn home_time(&self, d: Day) -> Option<LocalTime> {
        self.shift_end(d).map(|t| t.plus_minutes(self.settings.commute_minutes as i64))
    }

    pub fn reminder_time(&self, d: Day) -> Option<LocalTime> {
        self.shift_start(d).map(|t| t.plus_minutes(-(self.settings.reminder_lead_minutes as i64)))
    }

    pub fn week_days(week_start: Day) -> [Day; 7] {
        std::array::from_fn(|i| week_start.plus(i as i32))
    }

    pub fn has_any_entry(&self, week_start: Day) -> bool {
        Self::week_days(week_start).iter().any(|d| self.plans.contains_key(&d.0))
    }

    pub fn week_minutes(&self, week_start: Day) -> i32 {
        Self::week_days(week_start).iter().filter_map(|d| self.work_plan(*d)).map(|(s, e)| e - s).sum()
    }

    pub fn week_shift_count(&self, week_start: Day) -> usize {
        Self::week_days(week_start).iter().filter(|d| self.work_plan(**d).is_some()).count()
    }

    /// The work day that "now" belongs to. Right after midnight, if yesterday's shift is still
    /// running (or you're still driving home from it) it is still yesterday's work day.
    pub fn active_work_date(&self, now: LocalTime) -> Day {
        let today = now.day();
        let yesterday = today.plus(-1);
        match self.home_time(yesterday) {
            Some(home) if now < home => yesterday,
            _ => today,
        }
    }

    /// Other people whose start (to work) or end (home) is within the carpool window of mine.
    /// Empty if I'm not working that day.
    pub fn carpool_matches(&self, d: Day, dir: Dir) -> Vec<Person> {
        let Some((ms, me_end)) = self.work_plan(d) else { return vec![] };
        let mine = if dir == Dir::To { ms } else { me_end };
        Person::ALL
            .into_iter()
            .filter(|p| Some(*p) != self.me)
            .filter(|p| match self.plan_for(*p, d).and_then(DayPlan::work) {
                Some((s, e)) => ((if dir == Dir::To { s } else { e }) - mine).abs() <= self.settings.carpool_window,
                None => false,
            })
            .collect()
    }

    /// Carpools on this day and direction that involve me (newest first), cancelled ones included.
    pub fn carpools_for(&self, d: Day, dir: Dir) -> Vec<&Carpool> {
        let Some(me) = self.me else { return vec![] };
        let iso = d.iso();
        let mut v: Vec<&Carpool> =
            self.carpools.iter().filter(|c| c.day == iso && c.dir == dir.id() && c.involves(me)).collect();
        v.sort_by(|a, b| b.created.cmp(&a.created));
        v
    }

    /// Something to show for this day/direction: a live (not cancelled) carpool involving me.
    pub fn has_active_carpool(&self, d: Day, dir: Dir) -> bool {
        self.carpools_for(d, dir).iter().any(|c| c.cancelled == 0)
    }

    /// "Samy drives you", "You drive Austin", for accepted carpools on this day/direction.
    pub fn ride_summary(&self, d: Day, dir: Dir) -> Option<String> {
        let me = self.me?;
        let mut parts = Vec::new();
        for c in self.carpools_for(d, dir) {
            for (driver, rider) in c.rides() {
                if driver == me {
                    parts.push(format!("You drive {}", rider.name()));
                } else if rider == me {
                    parts.push(format!("{} drives you", driver.name()));
                }
            }
        }
        parts.dedup();
        (!parts.is_empty()).then(|| parts.join(", "))
    }

    /// Carpools waiting for my answer.
    pub fn carpools_awaiting_me(&self) -> usize {
        let Some(me) = self.me else { return 0 };
        self.carpools.iter().filter(|c| c.cancelled == 0 && c.to.iter().any(|t| t == me.id()) && c.answer(me).is_none()).count()
    }

    pub fn unread_count(&self) -> usize {
        self.inbox.iter().filter(|m| m.read == 0).count()
    }

    /// Most frequently used shifts, for one-tap entry.
    pub fn recent_shifts(&self, limit: usize) -> Vec<(i32, i32)> {
        let mut counts: Vec<((i32, i32), usize)> = Vec::new();
        for plan in self.plans.values() {
            if let Some(w) = plan.work() {
                match counts.iter_mut().find(|(k, _)| *k == w) {
                    Some(entry) => entry.1 += 1,
                    None => counts.push((w, 1)),
                }
            }
        }
        counts.sort_by(|a, b| b.1.cmp(&a.1)); // stable: ties keep first-seen order
        counts.into_iter().take(limit).map(|(k, _)| k).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn data() -> AppData {
        let mut d = AppData { me: Some(Person::Kayo), ..Default::default() };
        let mon = Day::from_ymd(2026, 11, 23);
        d.plans.insert(mon.0, DayPlan::Work { start: 960, end: 1440 });
        d.plans.insert(mon.plus(1).0, DayPlan::Off);
        d.plans.insert(mon.plus(2).0, DayPlan::Work { start: 1020, end: 1530 });
        d.plans.insert(mon.plus(3).0, DayPlan::Work { start: 960, end: 1440 });
        d.others[Person::Samy.index()].insert(mon.0, DayPlan::Work { start: 900, end: 1380 });
        d.others[Person::Austin.index()].insert(mon.0, DayPlan::Off);
        d
    }

    #[test]
    fn week_math() {
        let d = data();
        let mon = Day::from_ymd(2026, 11, 23);
        assert_eq!(d.week_minutes(mon), 480 + 510 + 480);
        assert_eq!(d.week_shift_count(mon), 3);
        assert!(d.has_any_entry(mon));
        assert!(!d.has_any_entry(mon.plus(7)));
        assert_eq!(d.others_working(mon), vec![Person::Samy]);
        assert_eq!(d.recent_shifts(4), vec![(960, 1440), (1020, 1530)]);
    }

    #[test]
    fn times_and_carry_over() {
        let d = data();
        let wed = Day::from_ymd(2026, 11, 25);
        assert_eq!(d.leave_time(wed), Some(wed.at(1020 - 30)));
        assert_eq!(d.home_time(wed), Some(wed.at(1530 + 30)));
        assert_eq!(d.reminder_time(wed), Some(wed.at(1020 - 60)));
        // 1:45 AM Thursday: Wednesday's shift (ends 1:30, home 2:00) is still the active work day.
        assert_eq!(d.active_work_date(wed.plus(1).at(105)), wed);
        assert_eq!(d.active_work_date(wed.plus(1).at(125)), wed.plus(1));
    }

    #[test]
    fn reminders_follow_default_unless_overridden() {
        let mut d = data();
        let mon = Day::from_ymd(2026, 11, 23);
        assert!(!d.reminder_enabled(mon));
        d.settings.default_reminders = true;
        assert!(d.reminder_enabled(mon));
        d.reminder_overrides.insert(mon.0, false);
        assert!(!d.reminder_enabled(mon));
    }

    #[test]
    fn carpool_matches_use_start_for_to_and_end_for_from() {
        let mut d = data();
        let mon = Day::from_ymd(2026, 11, 23);
        // Kayo 960–1440, Samy 900–1380: no exact match either way.
        assert!(d.carpool_matches(mon, Dir::To).is_empty());
        d.others[Person::Samy.index()].insert(mon.0, DayPlan::Work { start: 960, end: 1200 });
        assert_eq!(d.carpool_matches(mon, Dir::To), vec![Person::Samy]);
        assert!(d.carpool_matches(mon, Dir::From).is_empty(), "to and from are independent");
        d.settings.carpool_window = 30;
        d.others[Person::Austin.index()].insert(mon.0, DayPlan::Work { start: 700, end: 1420 });
        assert_eq!(d.carpool_matches(mon, Dir::From), vec![Person::Austin]);
        assert!(d.carpool_matches(mon.plus(1), Dir::To).is_empty(), "I'm off Tuesday");
    }

    #[test]
    fn rides_and_summary() {
        let mut d = data();
        let mon = Day::from_ymd(2026, 11, 23);
        let mut c = Carpool { id: "c1".into(), from: "kayo".into(), kind: "request".into(), day: mon.iso(), dir: "to".into(),
            to: vec!["samy".into(), "austin".into()], ..Default::default() };
        d.carpools.push(c.clone());
        assert!(d.has_active_carpool(mon, Dir::To));
        assert!(!d.has_active_carpool(mon, Dir::From));
        assert_eq!(d.ride_summary(mon, Dir::To), None);
        c.responses.insert("samy".into(), Answer { answer: "yes".into(), at: 1 });
        c.responses.insert("austin".into(), Answer { answer: "no".into(), at: 1 });
        d.carpools = vec![c.clone()];
        assert_eq!(d.ride_summary(mon, Dir::To).as_deref(), Some("Samy drives you"));
        c.kind = "offer".into();
        d.carpools = vec![c.clone()];
        assert_eq!(d.ride_summary(mon, Dir::To).as_deref(), Some("You drive Samy"));
        c.cancelled = 5;
        d.carpools = vec![c];
        assert_eq!(d.ride_summary(mon, Dir::To), None);
        assert!(!d.has_active_carpool(mon, Dir::To));
    }

    #[test]
    fn settings_tolerate_missing_fields() {
        let s: Settings = serde_json::from_str(r#"{"darkMode":false}"#).unwrap();
        assert!(!s.dark_mode);
        assert_eq!(s.commute_minutes, 30);
        let back: AppData = serde_json::from_str(&serde_json::to_string(&data()).unwrap()).unwrap();
        assert_eq!(back, data());
    }
}
