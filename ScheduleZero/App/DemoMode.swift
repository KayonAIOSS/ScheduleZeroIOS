import Foundation

/// Launch with `-SZDemo YES -SZScreen home|week|editor|compare|settings|signing|guide|picker|password|expiring
/// -SZTheme dark|light` to show fake data with no network and no saved files. Used only by the CI
/// screenshot step so the look can be checked without a Mac.
enum DemoMode {
    static let isOn: Bool = UserDefaults.standard.bool(forKey: "SZDemo")
    static var screen: String { UserDefaults.standard.string(forKey: "SZScreen") ?? "home" }
    static var theme: String { UserDefaults.standard.string(forKey: "SZTheme") ?? "dark" }

    /// Fake signing info: comfortably valid, or about to expire on the "expiring" screen.
    static var signingInfo: SigningInfo? {
        guard isOn else { return nil }
        let now = Date()
        let left: TimeInterval = screen == "expiring" ? 31 * 3600 + 20 * 60 : 4 * 86_400 + 6 * 3600
        return SigningInfo(expires: now.addingTimeInterval(left), created: now.addingTimeInterval(left - 7 * 86_400))
    }

    static func makeData(now: Date = Date()) -> AppData {
        var d = AppData()
        d.me = screen == "picker" ? nil : .kayo
        d.settings.darkMode = theme != "light"
        d.settings.hourlyRate = 18.5
        d.settings.autoRefreshEnabled = screen == "guide" || screen == "expiring"
        let week = Day(now).weekStart
        let mine: [Int: DayPlan] = [
            0: .work(WorkShift(start: 16 * 60, end: 24 * 60)),
            1: .off,
            2: .work(WorkShift(start: 17 * 60, end: 25 * 60 + 30)),
            3: .work(WorkShift(start: 16 * 60, end: 24 * 60)),
            4: .work(WorkShift(start: 18 * 60, end: 26 * 60)),
            5: .off,
        ]
        for (i, p) in mine { d.plans[week.plus(i)] = p }
        // Today is always a work day in the demo so the countdown shows.
        d.plans[Day(now)] = .work(WorkShift(start: 16 * 60, end: 24 * 60 + 30))
        d.reminderOverrides[week.plus(3)] = true
        // Assigned one by one: "today" can be the same day as one of the weekdays, and a
        // dictionary literal with a duplicate key crashes.
        var samy: [Day: DayPlan] = [:]
        samy[week.plus(0)] = .work(WorkShift(start: 15 * 60, end: 23 * 60))
        samy[week.plus(2)] = .work(WorkShift(start: 17 * 60, end: 24 * 60))
        samy[week.plus(4)] = .off
        samy[Day(now)] = .work(WorkShift(start: 17 * 60, end: 25 * 60))
        var austin: [Day: DayPlan] = [:]
        austin[week.plus(0)] = .work(WorkShift(start: 16 * 60, end: 24 * 60))
        austin[week.plus(3)] = .work(WorkShift(start: 18 * 60, end: 26 * 60))
        austin[Day(now)] = .work(WorkShift(start: 14 * 60, end: 22 * 60))
        d.others[.samy] = samy
        d.others[.austin] = austin
        return d
    }
}
