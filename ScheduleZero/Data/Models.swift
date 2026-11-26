import Foundation
import SwiftUI

/// The only three accounts. IDs match the server (see server/PROTOCOL.md).
enum Person: String, CaseIterable, Codable, Identifiable {
    case samy, austin, kayo

    var id: String { rawValue }

    var displayName: String {
        switch self {
        case .samy: return "Samy"
        case .austin: return "Austin"
        case .kayo: return "Kayo"
        }
    }

    var color: Color {
        switch self {
        case .samy: return Color(hex: 0xE53935)
        case .austin: return Color(hex: 0x1E88E5)
        case .kayo: return Color(hex: 0x9C4DFF)
        }
    }

    static func from(id: String?) -> Person? { id.flatMap { Person(rawValue: $0.lowercased()) } }
}

/// Work times are minutes after midnight of the *work day*.
/// 720 = 12:00 PM, 1440 = 12:00 AM (next calendar day), 1560 = 2:00 AM (next calendar day).
/// A shift that ends after midnight still belongs to the day it started on.
enum WorkTime {
    static let min = 12 * 60
    static let max = 26 * 60
    static let step = 15
    static let defaultStart = 16 * 60
    static let defaultLength = 8 * 60

    static let options: [Int] = Array(stride(from: min, through: max, by: step))

    static func isNextDay(_ minutes: Int) -> Bool { minutes >= 24 * 60 }
}

struct WorkShift: Hashable, Codable {
    var start: Int
    var end: Int
    var lengthMinutes: Int { end - start }
}

enum DayPlan: Hashable, Codable {
    case work(WorkShift)
    case off

    var work: WorkShift? {
        if case .work(let w) = self { return w }
        return nil
    }

    var isOff: Bool { self == .off }
}

struct AppSettings: Codable, Equatable {
    var darkMode = true
    var use24h = false
    var defaultReminders = false
    var reminderLeadMinutes = 60
    var commuteMinutes = 30
    var weeklyPrompt = false
    /// Minutes after midnight on Sunday for the "enter your schedule" prompt.
    var weeklyPromptMinutes = 10 * 60
    var hourlyRate: Double? = nil

    // iPhone only: sideloaded installs expire after 7 days.
    /// Hours before expiry to start warning. 0 = off. 1...48.
    var expiryWarningHours = 48
    /// Experimental: refresh through a Shortcuts automation. Off by default.
    var autoRefreshEnabled = false

    static let leadMin = 15
    static let leadMax = 180
    static let leadStep = 15
    static let expiryWarningMax = 48

    init() {}

    // Tolerant decoding so new fields never wipe older saved settings.
    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        let d = AppSettings()
        darkMode = (try? c.decode(Bool.self, forKey: .darkMode)) ?? d.darkMode
        use24h = (try? c.decode(Bool.self, forKey: .use24h)) ?? d.use24h
        defaultReminders = (try? c.decode(Bool.self, forKey: .defaultReminders)) ?? d.defaultReminders
        reminderLeadMinutes = (try? c.decode(Int.self, forKey: .reminderLeadMinutes)) ?? d.reminderLeadMinutes
        commuteMinutes = (try? c.decode(Int.self, forKey: .commuteMinutes)) ?? d.commuteMinutes
        weeklyPrompt = (try? c.decode(Bool.self, forKey: .weeklyPrompt)) ?? d.weeklyPrompt
        weeklyPromptMinutes = (try? c.decode(Int.self, forKey: .weeklyPromptMinutes)) ?? d.weeklyPromptMinutes
        hourlyRate = try? c.decodeIfPresent(Double.self, forKey: .hourlyRate)
        expiryWarningHours = (try? c.decode(Int.self, forKey: .expiryWarningHours)) ?? d.expiryWarningHours
        autoRefreshEnabled = (try? c.decode(Bool.self, forKey: .autoRefreshEnabled)) ?? d.autoRefreshEnabled
    }
}

struct AppData: Equatable {
    var plans: [Day: DayPlan] = [:]
    /// Per-day reminder choice. Missing = follow settings.defaultReminders.
    var reminderOverrides: [Day: Bool] = [:]
    var settings = AppSettings()
    /// Who this device is signed in as (nil until chosen).
    var me: Person? = nil
    /// Everyone else's schedules, as last received from the server.
    var others: [Person: [Day: DayPlan]] = [:]

    /// Other people working on [date], in fixed order.
    func othersWorking(_ date: Day) -> [Person] {
        Person.allCases.filter { $0 != me && others[$0]?[date]?.work != nil }
    }

    func planFor(_ person: Person, _ date: Day) -> DayPlan? {
        person == me ? plans[date] : others[person]?[date]
    }

    func reminderEnabled(_ date: Day) -> Bool { reminderOverrides[date] ?? settings.defaultReminders }

    func workPlan(_ date: Day) -> WorkShift? { plans[date]?.work }

    func shiftStart(_ date: Day) -> Date? { workPlan(date).map { date.at(minutes: $0.start) } }
    func shiftEnd(_ date: Day) -> Date? { workPlan(date).map { date.at(minutes: $0.end) } }

    func leaveTime(_ date: Day) -> Date? {
        workPlan(date).map { date.at(minutes: $0.start - settings.commuteMinutes) }
    }

    func homeTime(_ date: Day) -> Date? {
        workPlan(date).map { date.at(minutes: $0.end + settings.commuteMinutes) }
    }

    func reminderTime(_ date: Day) -> Date? {
        workPlan(date).map { date.at(minutes: $0.start - settings.reminderLeadMinutes) }
    }

    func weekDays(_ weekStart: Day) -> [Day] { (0..<7).map { weekStart.plus($0) } }

    func hasAnyEntry(_ weekStart: Day) -> Bool { weekDays(weekStart).contains { plans[$0] != nil } }

    func weekMinutes(_ weekStart: Day) -> Int {
        weekDays(weekStart).reduce(0) { $0 + (workPlan($1)?.lengthMinutes ?? 0) }
    }

    func weekShiftCount(_ weekStart: Day) -> Int { weekDays(weekStart).filter { workPlan($0) != nil }.count }

    /// The work day that "now" belongs to. Right after midnight, if yesterday's shift is still
    /// running (or you're still driving home from it) it is still yesterday's work day.
    func activeWorkDate(_ now: Date) -> Day {
        let today = Day(now)
        let yesterday = today.minus(1)
        if let home = homeTime(yesterday), now < home { return yesterday }
        return today
    }

    /// Most frequently used shifts, for one-tap entry.
    func recentShifts(_ limit: Int = 4) -> [WorkShift] {
        var counts: [WorkShift: Int] = [:]
        for p in plans.values { if let w = p.work { counts[w, default: 0] += 1 } }
        return counts.sorted {
            $0.value != $1.value ? $0.value > $1.value : ($0.key.start, $0.key.end) < ($1.key.start, $1.key.end)
        }.prefix(limit).map { $0.key }
    }
}

/// A day as exchanged with the server; plan == nil means "not entered" (a deletion).
struct SyncedDay: Equatable {
    var plan: DayPlan?
    var updated: Int64
}

extension Color {
    init(hex: UInt32, alpha: Double = 1) {
        self.init(
            .sRGB,
            red: Double((hex >> 16) & 0xFF) / 255,
            green: Double((hex >> 8) & 0xFF) / 255,
            blue: Double(hex & 0xFF) / 255,
            opacity: alpha
        )
    }
}

func nowMillis() -> Int64 { Int64((Date().timeIntervalSince1970 * 1000).rounded()) }
