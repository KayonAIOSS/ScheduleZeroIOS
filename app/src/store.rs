//! Single source of truth on this device, same rules as Android's `ScheduleRepository`.
//!
//! Schedules are keyed by calendar date, so each week (Mon–Sun) is independent. Every day you edit
//! gets an `updated` timestamp and stays "dirty" until the server has accepted it, so edits made
//! offline upload automatically later. Saved as one JSON value in localStorage.

use crate::model::{AppData, DayPlan, Person, Settings};
use crate::time::Day;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const KEEP_WEEKS: i32 = 12;

/// A day as exchanged with the server; `plan == None` means "not entered" (a deletion).
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct SyncedDay {
    pub plan: Option<DayPlan>,
    pub updated: i64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Repo {
    pub data: AppData,
    updated_at: BTreeMap<i32, i64>,
    dirty: BTreeSet<i32>,
}

impl Repo {
    /// Loads saved JSON, dropping anything older than `KEEP_WEEKS` before this week.
    pub fn load(json: Option<&str>, today: Day) -> Repo {
        let mut r: Repo = json.and_then(|j| serde_json::from_str(j).ok()).unwrap_or_default();
        let cutoff = today.week_start().plus(-7 * KEEP_WEEKS).0;
        r.data.plans.retain(|d, _| *d >= cutoff);
        r.data.reminder_overrides.retain(|d, _| *d >= cutoff);
        r.updated_at.retain(|d, _| *d >= cutoff);
        for days in r.data.others.iter_mut() {
            days.retain(|d, _| *d >= cutoff);
        }
        r
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }

    // ---- local edits (these sync) -------------------------------------------------------

    pub fn set_plan(&mut self, d: Day, plan: Option<DayPlan>, now_ms: i64) {
        self.edit(now_ms, |data| match plan {
            Some(p) => {
                data.plans.insert(d.0, p);
            }
            None => {
                data.plans.remove(&d.0);
            }
        });
    }

    /// Copies every day of `from` onto `to`, replacing whatever was there.
    pub fn copy_week(&mut self, from: Day, to: Day, now_ms: i64) {
        self.edit(now_ms, |data| {
            for i in 0..7 {
                match data.plans.get(&from.plus(i).0).copied() {
                    Some(p) => data.plans.insert(to.plus(i).0, p),
                    None => data.plans.remove(&to.plus(i).0),
                };
            }
        });
    }

    pub fn mark_unset_off(&mut self, week: Day, now_ms: i64) {
        self.edit(now_ms, |data| {
            for d in AppData::week_days(week) {
                data.plans.entry(d.0).or_insert(DayPlan::Off);
            }
        });
    }

    pub fn clear_week(&mut self, week: Day, now_ms: i64) {
        self.edit(now_ms, |data| {
            for d in AppData::week_days(week) {
                data.plans.remove(&d.0);
                data.reminder_overrides.remove(&d.0);
            }
        });
    }

    pub fn clear_all_schedules(&mut self, now_ms: i64) {
        self.edit(now_ms, |data| {
            data.plans.clear();
            data.reminder_overrides.clear();
        });
    }

    // ---- device-only changes (never synced) -----------------------------------------------

    pub fn set_reminder(&mut self, d: Day, enabled: bool) {
        if enabled == self.data.settings.default_reminders {
            self.data.reminder_overrides.remove(&d.0);
        } else {
            self.data.reminder_overrides.insert(d.0, enabled);
        }
    }

    pub fn update_settings(&mut self, f: impl FnOnce(&mut Settings)) {
        f(&mut self.data.settings);
    }

    // ---- account ----------------------------------------------------------------------------

    /// Sets who this device belongs to. Switching to a different person wipes the local copy
    /// (their schedule comes from the server). The first sign-in keeps what was entered before and
    /// uploads it.
    pub fn set_me(&mut self, p: Person, now_ms: i64) {
        if self.data.me == Some(p) {
            return;
        }
        if self.data.me.is_none() {
            for d in self.data.plans.keys() {
                self.updated_at.insert(*d, now_ms);
                self.dirty.insert(*d);
            }
            self.data.others[p.index()].clear();
        } else {
            self.updated_at.clear();
            self.dirty.clear();
            self.data.plans.clear();
            self.data.reminder_overrides.clear();
            self.data.others = Default::default();
        }
        self.data.me = Some(p);
    }

    pub fn sign_out(&mut self) {
        self.updated_at.clear();
        self.dirty.clear();
        self.data.me = None;
        self.data.plans.clear();
        self.data.reminder_overrides.clear();
        self.data.others = Default::default();
    }

    // ---- sync hooks -------------------------------------------------------------------------

    pub fn pending_uploads(&self, now_ms: i64) -> BTreeMap<i32, SyncedDay> {
        self.dirty
            .iter()
            .map(|d| (*d, SyncedDay { plan: self.data.plans.get(d).copied(), updated: *self.updated_at.get(d).unwrap_or(&now_ms) }))
            .collect()
    }

    pub fn has_pending_uploads(&self) -> bool {
        !self.dirty.is_empty()
    }

    /// Clears dirty flags for days the server accepted, unless they were edited again meanwhile.
    pub fn mark_uploaded(&mut self, sent: &BTreeMap<i32, SyncedDay>) {
        for (d, day) in sent {
            if self.updated_at.get(d) == Some(&day.updated) {
                self.dirty.remove(d);
            }
        }
    }

    /// Applies the server's copy of everyone. Your own unsent edits always win.
    pub fn apply_server(&mut self, people: &BTreeMap<Person, BTreeMap<i32, SyncedDay>>) {
        let Some(me) = self.data.me else { return };
        if let Some(mine) = people.get(&me) {
            for (d, remote) in mine {
                if self.dirty.contains(d) {
                    continue;
                }
                let newer = self.updated_at.get(d).map_or(true, |local| remote.updated > *local);
                if newer {
                    match remote.plan {
                        Some(p) => self.data.plans.insert(*d, p),
                        None => self.data.plans.remove(d),
                    };
                    self.updated_at.insert(*d, remote.updated);
                }
            }
        }
        for (p, days) in people {
            if *p != me {
                self.data.others[p.index()] = days.iter().filter_map(|(d, s)| s.plan.map(|pl| (*d, pl))).collect();
            }
        }
    }

    fn edit(&mut self, now_ms: i64, f: impl FnOnce(&mut AppData)) {
        let before = self.data.plans.clone();
        f(&mut self.data);
        let keys: BTreeSet<i32> = before.keys().chain(self.data.plans.keys()).copied().collect();
        for d in keys {
            if before.get(&d) != self.data.plans.get(&d) {
                self.updated_at.insert(d, now_ms);
                self.dirty.insert(d);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn work(s: i32, e: i32) -> Option<DayPlan> {
        Some(DayPlan::Work { start: s, end: e })
    }

    #[test]
    fn edits_are_dirty_until_uploaded() {
        let mut r = Repo::default();
        r.set_me(Person::Kayo, 1);
        let d = Day::from_ymd(2026, 11, 27);
        r.set_plan(d, work(960, 1440), 100);
        let sent = r.pending_uploads(100);
        assert_eq!(sent.get(&d.0).unwrap().updated, 100);
        r.set_plan(d, work(1000, 1440), 200); // edited again while uploading
        r.mark_uploaded(&sent);
        assert!(r.has_pending_uploads());
        let sent = r.pending_uploads(300);
        r.mark_uploaded(&sent);
        assert!(!r.has_pending_uploads());
    }

    #[test]
    fn server_merge_respects_unsent_edits_and_timestamps() {
        let mut r = Repo::default();
        r.set_me(Person::Kayo, 1);
        let d = Day::from_ymd(2026, 11, 27);
        r.set_plan(d, work(960, 1440), 100);
        let mut people = BTreeMap::new();
        let mut mine = BTreeMap::new();
        mine.insert(d.0, SyncedDay { plan: Some(DayPlan::Off), updated: 999 });
        mine.insert(d.plus(1).0, SyncedDay { plan: work(720, 900), updated: 50 });
        people.insert(Person::Kayo, mine.clone());
        let mut samy = BTreeMap::new();
        samy.insert(d.0, SyncedDay { plan: work(900, 1300), updated: 5 });
        samy.insert(d.plus(1).0, SyncedDay { plan: None, updated: 6 });
        people.insert(Person::Samy, samy);
        r.apply_server(&people);
        assert_eq!(r.data.plan(d), work(960, 1440)); // still dirty: ours wins
        assert_eq!(r.data.plan(d.plus(1)), work(720, 900));
        assert_eq!(r.data.others_working(d), vec![Person::Samy]);
        assert_eq!(r.data.others[Person::Samy.index()].len(), 1);

        let sent = r.pending_uploads(0);
        r.mark_uploaded(&sent);
        r.apply_server(&people); // now the newer server copy (999 > 100) wins
        assert_eq!(r.data.plan(d), Some(DayPlan::Off));
        mine.insert(d.0, SyncedDay { plan: None, updated: 10 }); // older: ignored
        people.insert(Person::Kayo, mine);
        r.apply_server(&people);
        assert_eq!(r.data.plan(d), Some(DayPlan::Off));
    }

    #[test]
    fn switching_person_wipes_first_sign_in_keeps() {
        let mut r = Repo::default();
        let d = Day::from_ymd(2026, 11, 27);
        r.set_plan(d, work(960, 1440), 5);
        r.mark_uploaded(&r.pending_uploads(0));
        r.set_me(Person::Samy, 7);
        assert_eq!(r.pending_uploads(0).get(&d.0).unwrap().updated, 7);
        r.set_me(Person::Austin, 8);
        assert!(r.data.plans.is_empty() && !r.has_pending_uploads());
    }

    #[test]
    fn week_tools() {
        let mut r = Repo::default();
        r.set_me(Person::Kayo, 1);
        let mon = Day::from_ymd(2026, 11, 23);
        r.set_plan(mon, work(960, 1440), 2);
        r.copy_week(mon, mon.plus(7), 3);
        assert_eq!(r.data.plan(mon.plus(7)), work(960, 1440));
        r.mark_unset_off(mon, 4);
        assert_eq!(r.data.plan(mon.plus(6)), Some(DayPlan::Off));
        r.clear_week(mon, 5);
        assert!(!r.data.has_any_entry(mon));
        assert_eq!(r.pending_uploads(0).get(&mon.0).unwrap().plan, None);
    }

    #[test]
    fn load_prunes_old_weeks_and_survives_garbage() {
        let mut r = Repo::default();
        let today = Day::from_ymd(2026, 11, 27);
        r.data.plans.insert(today.plus(-200).0, DayPlan::Off);
        r.data.plans.insert(today.0, DayPlan::Off);
        let back = Repo::load(Some(&r.to_json()), today);
        assert_eq!(back.data.plans.len(), 1);
        assert!(Repo::load(Some("not json"), today).data.plans.is_empty());
    }
}
