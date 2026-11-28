//! Dates without time zones, plus the same text formatting as the Android app (`Fmt`).
//!
//! Work times are minutes after midnight of the *work day*: 720 = 12:00 PM, 1440 = 12:00 AM the
//! next calendar day, 1560 = 2:00 AM the next calendar day. A shift that ends after midnight still
//! belongs to the day it started on. Like Android's `LocalDateTime`, local times here are "wall
//! clock" values: arithmetic ignores daylight-saving jumps.

/// A calendar day, stored as days since 1970-01-01.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct Day(pub i32);

const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
const DAY_NAMES: [&str; 7] = ["Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday", "Sunday"];
const DAY_SHORT: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];

impl Day {
    pub fn from_ymd(y: i32, m: u32, d: u32) -> Day {
        let y = if m <= 2 { y - 1 } else { y };
        let era = (if y >= 0 { y } else { y - 399 }) / 400;
        let yoe = y - era * 400;
        let mp = (m as i32 + 9) % 12;
        let doy = (153 * mp + 2) / 5 + d as i32 - 1;
        let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
        Day(era * 146_097 + doe - 719_468)
    }

    pub fn ymd(self) -> (i32, u32, u32) {
        let z = self.0 + 719_468;
        let era = (if z >= 0 { z } else { z - 146_096 }) / 146_097;
        let doe = z - era * 146_097;
        let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
        let y = yoe + era * 400;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
        let m = (if mp < 10 { mp + 3 } else { mp - 9 }) as u32;
        (if m <= 2 { y + 1 } else { y }, m, d)
    }

    /// "2026-11-27"
    pub fn iso(self) -> String {
        let (y, m, d) = self.ymd();
        format!("{y:04}-{m:02}-{d:02}")
    }

    pub fn parse_iso(s: &str) -> Option<Day> {
        let mut it = s.split('-');
        let y: i32 = it.next()?.parse().ok()?;
        let m: u32 = it.next()?.parse().ok()?;
        let d: u32 = it.next()?.parse().ok()?;
        if it.next().is_some() || !(1..=12).contains(&m) || !(1..=31).contains(&d) {
            return None;
        }
        let day = Day::from_ymd(y, m, d);
        (day.ymd() == (y, m, d)).then_some(day)
    }

    /// 0 = Monday … 6 = Sunday.
    pub fn weekday(self) -> usize {
        (self.0 + 3).rem_euclid(7) as usize
    }

    pub fn plus(self, days: i32) -> Day {
        Day(self.0 + days)
    }

    /// Weeks run Monday through Sunday.
    pub fn week_start(self) -> Day {
        Day(self.0 - self.weekday() as i32)
    }

    pub fn day_of_month(self) -> u32 {
        self.ymd().2
    }

    /// Local time `minutes` after this day's midnight (may roll into the next day).
    pub fn at(self, minutes: i32) -> LocalTime {
        LocalTime(self.0 as i64 * 86_400 + minutes as i64 * 60)
    }
}

/// Wall-clock local time, as seconds since 1970-01-01 00:00 local.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub struct LocalTime(pub i64);

impl LocalTime {
    pub fn day(self) -> Day {
        Day(self.0.div_euclid(86_400) as i32)
    }

    pub fn seconds_of_day(self) -> i64 {
        self.0.rem_euclid(86_400)
    }

    pub fn hour(self) -> u32 {
        (self.seconds_of_day() / 3600) as u32
    }

    pub fn minute(self) -> u32 {
        ((self.seconds_of_day() / 60) % 60) as u32
    }

    pub fn plus_minutes(self, m: i64) -> LocalTime {
        LocalTime(self.0 + m * 60)
    }

    /// Minutes from `self` until `to`, rounded up like the Android countdown.
    pub fn minutes_until(self, to: LocalTime) -> i64 {
        (to.0 - self.0 + 59).div_euclid(60)
    }
}

pub mod fmt {
    use super::{Day, LocalTime, DAY_NAMES, DAY_SHORT, MONTHS};

    fn clock(h24: u32, m: u32, use24h: bool) -> String {
        if use24h {
            format!("{h24:02}:{m:02}")
        } else {
            let h12 = if h24 % 12 == 0 { 12 } else { h24 % 12 };
            format!("{h12}:{m:02} {}", if h24 < 12 { "AM" } else { "PM" })
        }
    }

    pub fn time(t: LocalTime, use24h: bool) -> String {
        clock(t.hour(), t.minute(), use24h)
    }

    /// Formats work-day minutes (720..1560).
    pub fn work_minutes(minutes: i32, use24h: bool) -> String {
        clock(((minutes / 60) % 24) as u32, (minutes % 60) as u32, use24h)
    }

    /// Very short start label for tight spaces: "4p", "12:30p", "16", "16:30".
    pub fn compact_hour(minutes: i32, use24h: bool) -> String {
        let h24 = (minutes / 60) % 24;
        let m = minutes % 60;
        let mm = if m == 0 { String::new() } else { format!(":{m:02}") };
        if use24h {
            return format!("{h24}{mm}");
        }
        let h12 = if h24 % 12 == 0 { 12 } else { h24 % 12 };
        format!("{h12}{mm}{}", if h24 < 12 { "a" } else { "p" })
    }

    pub fn shift(start: i32, end: i32, use24h: bool) -> String {
        format!("{} – {}", work_minutes(start, use24h), work_minutes(end, use24h))
    }

    pub fn day_name(d: Day) -> &'static str {
        DAY_NAMES[d.weekday()]
    }

    pub fn day_short(d: Day) -> &'static str {
        DAY_SHORT[d.weekday()]
    }

    pub fn month_day(d: Day) -> String {
        let (_, m, dd) = d.ymd();
        format!("{} {dd}", MONTHS[(m - 1) as usize])
    }

    pub fn full_date(d: Day) -> String {
        format!("{}, {}", day_name(d), month_day(d))
    }

    pub fn week_range(week_start: Day) -> String {
        format!("{} – {}", month_day(week_start), month_day(week_start.plus(6)))
    }

    /// "8h 30m", "45m", "3h".
    pub fn duration(total_minutes: i64) -> String {
        let m = total_minutes.abs();
        let (h, r) = (m / 60, m % 60);
        match (h, r) {
            (0, _) => format!("{r}m"),
            (_, 0) => format!("{h}h"),
            _ => format!("{h}h {r}m"),
        }
    }

    /// "8h", "7.5h", "7.75h".
    pub fn hours(total_minutes: i32) -> String {
        if total_minutes % 60 == 0 {
            return format!("{}h", total_minutes / 60);
        }
        format!("{:.2}h", total_minutes as f64 / 60.0).replace("0h", "h")
    }

    pub fn lead(minutes: i32) -> String {
        if minutes < 60 {
            format!("{minutes} min")
        } else {
            duration(minutes as i64)
        }
    }

    /// "$1,234.50"
    pub fn money(amount: f64) -> String {
        let cents = (amount * 100.0).round() as i64;
        let (neg, cents) = (cents < 0, cents.abs());
        let whole = (cents / 100).to_string();
        let mut grouped = String::new();
        for (i, ch) in whole.chars().enumerate() {
            if i > 0 && (whole.len() - i) % 3 == 0 {
                grouped.push(',');
            }
            grouped.push(ch);
        }
        format!("{}${grouped}.{:02}", if neg { "-" } else { "" }, cents % 100)
    }
}

#[cfg(test)]
mod tests {
    use super::fmt::*;
    use super::*;

    #[test]
    fn civil_round_trip() {
        assert_eq!(Day::from_ymd(1970, 1, 1), Day(0));
        assert_eq!(Day::from_ymd(2026, 11, 27).iso(), "2026-11-27");
        for z in [-800_000, -1, 0, 59, 60, 11_016, 20_000, 2_000_000] {
            let (y, m, d) = Day(z).ymd();
            assert_eq!(Day::from_ymd(y, m, d), Day(z));
        }
        assert_eq!(Day::parse_iso("2024-02-29"), Some(Day::from_ymd(2024, 2, 29)));
        assert_eq!(Day::parse_iso("2025-02-29"), None);
        assert_eq!(Day::parse_iso("nope"), None);
    }

    #[test]
    fn weeks_start_monday() {
        let fri = Day::from_ymd(2026, 11, 27);
        assert_eq!(day_name(fri), "Friday");
        assert_eq!(fri.week_start(), Day::from_ymd(2026, 11, 23));
        let sun = Day::from_ymd(2026, 11, 29);
        assert_eq!(sun.weekday(), 6);
        assert_eq!(sun.week_start(), Day::from_ymd(2026, 11, 23));
        assert_eq!(week_range(sun.week_start()), "Nov 23 – Nov 29");
    }

    #[test]
    fn formats_match_android() {
        assert_eq!(work_minutes(960, false), "4:00 PM");
        assert_eq!(work_minutes(1440, false), "12:00 AM");
        assert_eq!(work_minutes(1530, true), "01:30");
        assert_eq!(work_minutes(720, false), "12:00 PM");
        assert_eq!(compact_hour(990, false), "4:30p");
        assert_eq!(compact_hour(1500, false), "1a");
        assert_eq!(compact_hour(960, true), "16");
        assert_eq!(shift(960, 1440, false), "4:00 PM – 12:00 AM");
        assert_eq!(duration(510), "8h 30m");
        assert_eq!(duration(45), "45m");
        assert_eq!(duration(180), "3h");
        assert_eq!(hours(480), "8h");
        assert_eq!(hours(450), "7.5h");
        assert_eq!(hours(465), "7.75h");
        assert_eq!(hours(630), "10.5h");
        assert_eq!(lead(45), "45 min");
        assert_eq!(lead(90), "1h 30m");
        assert_eq!(money(1234.5), "$1,234.50");
        assert_eq!(money(18.5 * 32.0), "$592.00");
        assert_eq!(money(0.0), "$0.00");
        assert_eq!(full_date(Day::from_ymd(2026, 11, 27)), "Friday, Nov 27");
    }

    #[test]
    fn local_time_rolls_over() {
        let d = Day::from_ymd(2026, 11, 27);
        let t = d.at(25 * 60 + 30);
        assert_eq!(t.day(), d.plus(1));
        assert_eq!((t.hour(), t.minute()), (1, 30));
        assert_eq!(d.at(0).minutes_until(d.at(90)), 90);
        assert_eq!(LocalTime(0).minutes_until(LocalTime(61)), 2);
    }
}
