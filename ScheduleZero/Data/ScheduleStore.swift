import Foundation
import Combine

/// Single source of truth on the device (port of Android's ScheduleRepository). Schedules are keyed
/// by calendar date, so each week (Mon–Sun) is independent: when Sunday 11:59 PM passes, the new
/// week starts empty unless it was entered ahead of time.
///
/// Sync bookkeeping: every day you edit gets an `updated` timestamp and is marked dirty until the
/// server has accepted it, so edits made offline upload automatically later.
@MainActor
final class ScheduleStore: ObservableObject {
    static let shared = ScheduleStore()

    @Published private(set) var data: AppData

    private var updatedAt: [Day: Int64] = [:]
    private var dirty: Set<Day> = []
    private let fileURL: URL?
    /// Called after local schedule edits (wired to SyncManager.pushSoon).
    var onLocalEdit: (() -> Void)?
    /// Called after every change (wired to Reminders.sync).
    var onCommit: ((AppData) -> Void)?

    nonisolated static let keepWeeks = 12

    /// [fileURL] nil = in-memory only (tests, demo mode).
    init(fileURL: URL? = ScheduleStore.defaultURL) {
        self.fileURL = fileURL
        data = AppData()
        load()
    }

    nonisolated static var defaultURL: URL? {
        if DemoMode.isOn { return nil } // screenshots: never touch real data
        guard let dir = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask).first else { return nil }
        try? FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        return dir.appendingPathComponent("schedulezero.json")
    }

    // MARK: local edits (these sync)

    func setPlan(_ date: Day, _ plan: DayPlan?) {
        editSchedule { d in
            if let plan { d.plans[date] = plan } else { d.plans.removeValue(forKey: date) }
        }
    }

    /// Copies every day of [fromWeek] onto [toWeek], replacing whatever was there.
    func copyWeek(from fromWeek: Day, to toWeek: Day) {
        editSchedule { d in
            let src = d.plans
            for i in 0..<7 {
                let dst = toWeek.plus(i)
                if let p = src[fromWeek.plus(i)] { d.plans[dst] = p } else { d.plans.removeValue(forKey: dst) }
            }
        }
    }

    func markUnsetOff(_ weekStart: Day) {
        editSchedule { d in
            for day in d.weekDays(weekStart) where d.plans[day] == nil { d.plans[day] = .off }
        }
    }

    func clearWeek(_ weekStart: Day) {
        editSchedule { d in
            for day in d.weekDays(weekStart) {
                d.plans.removeValue(forKey: day)
                d.reminderOverrides.removeValue(forKey: day)
            }
        }
    }

    func clearAllSchedules() {
        editSchedule { d in
            d.plans = [:]
            d.reminderOverrides = [:]
        }
    }

    // MARK: device-only changes (never synced)

    func setReminder(_ date: Day, _ enabled: Bool) {
        mutate { d in
            if enabled == d.settings.defaultReminders {
                d.reminderOverrides.removeValue(forKey: date)
            } else {
                d.reminderOverrides[date] = enabled
            }
        }
    }

    func updateSettings(_ transform: (inout AppSettings) -> Void) {
        mutate { d in transform(&d.settings) }
    }

    // MARK: account

    /// Sets who this device belongs to. Switching to a different person wipes the local copy (their
    /// schedule comes from the server). The very first sign-in keeps what was entered before and uploads it.
    func setMe(_ person: Person) {
        var d = data
        if d.me == person { return }
        if d.me == nil {
            let now = nowMillis()
            for day in d.plans.keys { updatedAt[day] = now; dirty.insert(day) }
            d.me = person
            d.others.removeValue(forKey: person)
        } else {
            updatedAt = [:]; dirty = []
            d.me = person
            d.plans = [:]; d.reminderOverrides = [:]; d.others = [:]
        }
        commit(d)
    }

    func signOut() {
        updatedAt = [:]; dirty = []
        var d = data
        d.me = nil; d.plans = [:]; d.reminderOverrides = [:]; d.others = [:]
        commit(d)
    }

    // MARK: sync hooks

    func pendingUploads() -> [Day: SyncedDay] {
        var out: [Day: SyncedDay] = [:]
        for day in dirty { out[day] = SyncedDay(plan: data.plans[day], updated: updatedAt[day] ?? nowMillis()) }
        return out
    }

    var hasPendingUploads: Bool { !dirty.isEmpty }

    /// Clears dirty flags for days the server accepted, unless they were edited again meanwhile.
    func markUploaded(_ sent: [Day: SyncedDay]) {
        for (day, s) in sent where updatedAt[day] == s.updated { dirty.remove(day) }
        save()
    }

    /// Applies the server's copy of everyone. Your own unsent edits always win.
    func applyServer(_ people: [Person: [Day: SyncedDay]]) {
        guard let me = data.me else { return }
        var d = data
        for (day, remote) in people[me] ?? [:] {
            if dirty.contains(day) { continue }
            let local = updatedAt[day]
            if local == nil || remote.updated > local! {
                if let p = remote.plan { d.plans[day] = p } else { d.plans.removeValue(forKey: day) }
                updatedAt[day] = remote.updated
            }
        }
        for (person, days) in people where person != me {
            var map: [Day: DayPlan] = [:]
            for (day, s) in days { if let p = s.plan { map[day] = p } }
            d.others[person] = map
        }
        if d != data { commit(d) } else { save() }
    }

    // MARK: internals

    private func editSchedule(_ transform: (inout AppData) -> Void) {
        let old = data
        var new = old
        transform(&new)
        let now = nowMillis()
        for day in Set(old.plans.keys).union(new.plans.keys) where old.plans[day] != new.plans[day] {
            updatedAt[day] = now
            dirty.insert(day)
        }
        commit(new)
        onLocalEdit?()
    }

    private func mutate(_ transform: (inout AppData) -> Void) {
        var d = data
        transform(&d)
        commit(d)
    }

    private func commit(_ d: AppData) {
        data = d
        save()
        onCommit?(d)
    }

    /// Replaces everything (demo mode and tests).
    func replaceForDemo(_ d: AppData) { data = d }

    // MARK: persistence

    private struct Saved: Codable {
        var plans: [String: DayPlan] = [:]
        var reminderOverrides: [String: Bool] = [:]
        var updatedAt: [String: Int64] = [:]
        var dirty: [String] = []
        var me: String?
        var others: [String: [String: DayPlan]] = [:]
        var settings = AppSettings()
    }

    private func load() {
        guard let url = fileURL, let raw = try? Data(contentsOf: url),
              let s = try? JSONDecoder().decode(Saved.self, from: raw) else { return }
        let cutoff = Day.today.weekStart.minus(Self.keepWeeks * 7)
        func keyed<V>(_ m: [String: V]) -> [Day: V] {
            var out: [Day: V] = [:]
            for (k, v) in m { if let d = Day(iso: k), d >= cutoff { out[d] = v } }
            return out
        }
        var d = AppData()
        d.plans = keyed(s.plans)
        d.reminderOverrides = keyed(s.reminderOverrides)
        d.settings = s.settings
        d.me = Person.from(id: s.me)
        for (pid, days) in s.others { if let p = Person.from(id: pid) { d.others[p] = keyed(days) } }
        updatedAt = keyed(s.updatedAt)
        dirty = Set(s.dirty.compactMap { Day(iso: $0) })
        data = d
    }

    private func save() {
        guard let url = fileURL else { return }
        func strKeyed<V>(_ m: [Day: V]) -> [String: V] {
            var out: [String: V] = [:]
            for (k, v) in m { out[k.iso] = v }
            return out
        }
        var s = Saved()
        s.plans = strKeyed(data.plans)
        s.reminderOverrides = strKeyed(data.reminderOverrides)
        s.updatedAt = strKeyed(updatedAt)
        s.dirty = dirty.map { $0.iso }
        s.me = data.me?.rawValue
        for (p, days) in data.others { s.others[p.rawValue] = strKeyed(days) }
        s.settings = data.settings
        if let raw = try? JSONEncoder().encode(s) { try? raw.write(to: url, options: .atomic) }
    }
}
