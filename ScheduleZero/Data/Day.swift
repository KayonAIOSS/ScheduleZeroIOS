import Foundation

/// A calendar date with no time zone (like Java's LocalDate). Stored as days since 1970-01-01.
struct Day: Hashable, Comparable, Codable, Identifiable, CustomStringConvertible {
    let epochDay: Int

    var id: Int { epochDay }

    init(epochDay: Int) { self.epochDay = epochDay }

    init(year: Int, month: Int, day: Int) {
        // Howard Hinnant's days_from_civil.
        let y = month <= 2 ? year - 1 : year
        let era = (y >= 0 ? y : y - 399) / 400
        let yoe = y - era * 400
        let mp = (month + 9) % 12
        let doy = (153 * mp + 2) / 5 + day - 1
        let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy
        epochDay = era * 146_097 + doe - 719_468
    }

    /// Parses "YYYY-MM-DD".
    init?(iso: String) {
        let parts = iso.split(separator: "-")
        guard parts.count == 3, let y = Int(parts[0]), let m = Int(parts[1]), let d = Int(parts[2]),
              (1...12).contains(m), (1...31).contains(d) else { return nil }
        self.init(year: y, month: m, day: d)
        guard self.month == m, self.day == d else { return nil }
    }

    /// The local calendar date of [date].
    init(_ date: Date, calendar: Calendar = .current) {
        let c = calendar.dateComponents([.year, .month, .day], from: date)
        self.init(year: c.year!, month: c.month!, day: c.day!)
    }

    static var today: Day { Day(Date()) }

    private var civil: (year: Int, month: Int, day: Int) {
        let z = epochDay + 719_468
        let era = (z >= 0 ? z : z - 146_096) / 146_097
        let doe = z - era * 146_097
        let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100)
        let mp = (5 * doy + 2) / 153
        let d = doy - (153 * mp + 2) / 5 + 1
        let m = mp < 10 ? mp + 3 : mp - 9
        return (yoe + era * 400 + (m <= 2 ? 1 : 0), m, d)
    }

    var year: Int { civil.year }
    var month: Int { civil.month }
    var day: Int { civil.day }

    /// 0 = Monday ... 6 = Sunday. (1970-01-01 was a Thursday.)
    var weekdayIndex: Int { ((epochDay + 3) % 7 + 7) % 7 }

    func plus(_ days: Int) -> Day { Day(epochDay: epochDay + days) }
    func minus(_ days: Int) -> Day { Day(epochDay: epochDay - days) }

    /// Weeks run Monday through Sunday.
    var weekStart: Day { minus(weekdayIndex) }

    var iso: String { String(format: "%04d-%02d-%02d", year, month, day) }
    var description: String { iso }

    /// Local wall-clock time [minutes] after midnight of this day (may run into the next day, e.g. 1500 = 1:00 AM).
    func at(minutes: Int, calendar: Calendar = .current) -> Date {
        let dayOffset = Int(floor(Double(minutes) / 1440.0))
        let m = minutes - dayOffset * 1440
        let target = plus(dayOffset)
        var c = DateComponents()
        c.year = target.year; c.month = target.month; c.day = target.day
        c.hour = m / 60; c.minute = m % 60
        return calendar.date(from: c) ?? Date(timeIntervalSince1970: TimeInterval(target.epochDay * 86_400 + m * 60))
    }

    var startOfDay: Date { at(minutes: 0) }

    static func < (a: Day, b: Day) -> Bool { a.epochDay < b.epochDay }
}
