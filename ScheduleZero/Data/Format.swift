import Foundation

enum Fmt {
    private static func formatter(_ pattern: String) -> DateFormatter {
        let f = DateFormatter()
        f.locale = Locale(identifier: "en_US_POSIX")
        f.dateFormat = pattern
        return f
    }

    private static let t12 = formatter("h:mm a")
    private static let t24 = formatter("HH:mm")
    private static let monthDayF = formatter("MMM d")
    private static let fullF = formatter("EEEE, MMM d")
    private static let dayNameF = formatter("EEEE")
    private static let dayShortF = formatter("EEE")

    static func time(_ date: Date, _ use24h: Bool) -> String {
        let f = use24h ? t24 : t12
        f.timeZone = .current
        return f.string(from: date)
    }

    /// Formats work-day minutes (720..1560).
    static func workMinutes(_ minutes: Int, _ use24h: Bool) -> String {
        let h24 = (minutes / 60) % 24
        let m = minutes % 60
        if use24h { return String(format: "%02d:%02d", h24, m) }
        let h12 = h24 % 12 == 0 ? 12 : h24 % 12
        return String(format: "%d:%02d %@", h12, m, h24 < 12 ? "AM" : "PM")
    }

    /// Very short start label for tight spaces: "4p", "12:30p", "16", "16:30".
    static func compactHour(_ minutes: Int, _ use24h: Bool) -> String {
        let h24 = (minutes / 60) % 24
        let m = minutes % 60
        let mm = m == 0 ? "" : String(format: ":%02d", m)
        if use24h { return "\(h24)\(mm)" }
        let h12 = h24 % 12 == 0 ? 12 : h24 % 12
        return "\(h12)\(mm)\(h24 < 12 ? "a" : "p")"
    }

    static func shift(_ w: WorkShift, _ use24h: Bool) -> String {
        "\(workMinutes(w.start, use24h)) – \(workMinutes(w.end, use24h))"
    }

    private static func noon(_ d: Day) -> Date { d.at(minutes: 12 * 60) }

    static func dayName(_ d: Day) -> String { dayNameF.string(from: noon(d)) }
    static func dayShort(_ d: Day) -> String { dayShortF.string(from: noon(d)) }
    static func monthDay(_ d: Day) -> String { monthDayF.string(from: noon(d)) }
    static func fullDate(_ d: Day) -> String { fullF.string(from: noon(d)) }

    static func weekRange(_ weekStart: Day) -> String {
        "\(monthDay(weekStart)) – \(monthDay(weekStart.plus(6)))"
    }

    /// "8h 30m", "45m", "3h".
    static func duration(_ totalMinutes: Int) -> String {
        let m = abs(totalMinutes)
        let h = m / 60
        let r = m % 60
        if h == 0 { return "\(r)m" }
        if r == 0 { return "\(h)h" }
        return "\(h)h \(r)m"
    }

    /// "8h", "7.5h", "7.25h" (same output as Android).
    static func hours(_ totalMinutes: Int) -> String {
        if totalMinutes % 60 == 0 { return "\(totalMinutes / 60)h" }
        var s = String(format: "%.2fh", Double(totalMinutes) / 60.0)
        s = s.replacingOccurrences(of: "0h", with: "h")
        return s
    }

    static func lead(_ minutes: Int) -> String { minutes < 60 ? "\(minutes) min" : duration(minutes) }

    static func money(_ amount: Double) -> String {
        let f = NumberFormatter()
        f.locale = Locale(identifier: "en_US")
        f.numberStyle = .currency
        f.currencySymbol = "$"
        return f.string(from: NSNumber(value: amount)) ?? String(format: "$%.2f", amount)
    }

    /// Rounds up to whole minutes, like Android's countdown.
    static func minutesUntil(_ from: Date, _ to: Date) -> Int {
        let secs = Int(to.timeIntervalSince(from))
        return (secs + 59) / 60
    }

    /// "2d 5h", "31h"-style countdown for the expiry warning.
    static func hoursLeft(_ seconds: TimeInterval) -> String {
        let totalH = Int(max(0, seconds) / 3600)
        if totalH >= 48 { return "\(totalH / 24)d \(totalH % 24)h" }
        if totalH >= 1 { return "\(totalH)h" }
        return "\(max(0, Int(seconds / 60)))m"
    }

    /// "Fri 4:12 PM"
    static func dayTime(_ date: Date, _ use24h: Bool) -> String {
        "\(dayShort(Day(date))) \(time(date, use24h))"
    }
}
