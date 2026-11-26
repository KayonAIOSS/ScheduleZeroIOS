import XCTest
@testable import ScheduleZero

final class DayTests: XCTestCase {
    func testIsoRoundTripAndWeekday() {
        let d = Day(iso: "2026-11-23")!
        XCTAssertEqual(d.iso, "2026-11-23")
        XCTAssertEqual(d.weekdayIndex, 0, "Nov 23 2026 is a Monday")
        XCTAssertEqual(d.plus(6).iso, "2026-11-29")
        XCTAssertEqual(d.plus(6).weekdayIndex, 6, "Sunday")
        XCTAssertEqual(Day(iso: "2026-11-29")!.weekStart, d)
        XCTAssertEqual(Day(iso: "1970-01-01")!.epochDay, 0)
        XCTAssertEqual(Day(iso: "2024-03-01")!.minus(1).iso, "2024-02-29")
        XCTAssertNil(Day(iso: "2026-02-30"))
        XCTAssertNil(Day(iso: "nope"))
    }

    func testAtMinutesRollsIntoNextDay() {
        let d = Day(iso: "2026-11-25")!
        let t = d.at(minutes: 25 * 60 + 30) // 1:30 AM next day
        let c = Calendar.current.dateComponents([.year, .month, .day, .hour, .minute], from: t)
        XCTAssertEqual([c.year, c.month, c.day, c.hour, c.minute], [2026, 11, 26, 1, 30])
    }
}

final class ModelTests: XCTestCase {
    private func data(_ plans: [String: DayPlan]) -> AppData {
        var d = AppData()
        d.me = .kayo
        for (k, v) in plans { d.plans[Day(iso: k)!] = v }
        return d
    }

    func testActiveWorkDateStaysOnYesterdayUntilHome() {
        let d = data(["2026-11-25": .work(WorkShift(start: 16 * 60, end: 25 * 60))]) // ends 1 AM, home 1:30
        let wed = Day(iso: "2026-11-25")!
        XCTAssertEqual(d.activeWorkDate(wed.at(minutes: 24 * 60 + 60 + 10)), wed)    // 1:10 AM Thu
        XCTAssertEqual(d.activeWorkDate(wed.at(minutes: 24 * 60 + 60 + 31)), wed.plus(1)) // 1:31 AM Thu
    }

    func testTimesAndWeekTotals() {
        let d = data([
            "2026-11-23": .work(WorkShift(start: 960, end: 1440)),
            "2026-11-24": .off,
            "2026-11-25": .work(WorkShift(start: 1020, end: 1530)),
        ])
        let mon = Day(iso: "2026-11-23")!
        XCTAssertEqual(d.weekMinutes(mon), 480 + 510)
        XCTAssertEqual(d.weekShiftCount(mon), 2)
        XCTAssertTrue(d.hasAnyEntry(mon))
        XCTAssertFalse(d.hasAnyEntry(mon.plus(7)))
        XCTAssertEqual(d.leaveTime(mon), mon.at(minutes: 930))
        XCTAssertEqual(d.reminderTime(mon), mon.at(minutes: 900))
    }

    func testFormatting() {
        XCTAssertEqual(Fmt.workMinutes(720, false), "12:00 PM")
        XCTAssertEqual(Fmt.workMinutes(1440, false), "12:00 AM")
        XCTAssertEqual(Fmt.workMinutes(1530, false), "1:30 AM")
        XCTAssertEqual(Fmt.workMinutes(1530, true), "01:30")
        XCTAssertEqual(Fmt.compactHour(960, false), "4p")
        XCTAssertEqual(Fmt.compactHour(750, false), "12:30p")
        XCTAssertEqual(Fmt.hours(480), "8h")
        XCTAssertEqual(Fmt.hours(450), "7.5h")
        XCTAssertEqual(Fmt.hours(435), "7.25h")
        XCTAssertEqual(Fmt.duration(510), "8h 30m")
        XCTAssertEqual(Fmt.duration(45), "45m")
        XCTAssertEqual(Fmt.lead(45), "45 min")
        XCTAssertEqual(Fmt.lead(90), "1h 30m")
        XCTAssertEqual(Fmt.hoursLeft(31 * 3600 + 100), "31h")
        XCTAssertEqual(Fmt.hoursLeft(50 * 3600), "2d 2h")
    }

    func testRecentShiftsMostCommonFirst() {
        let a = WorkShift(start: 960, end: 1440), b = WorkShift(start: 1080, end: 1560)
        let d = data(["2026-11-23": .work(a), "2026-11-24": .work(b), "2026-11-25": .work(b)])
        XCTAssertEqual(d.recentShifts(2), [b, a])
    }

    func testSettingsDecodeKeepsDefaultsForMissingFields() throws {
        let s = try JSONDecoder().decode(AppSettings.self, from: Data(#"{"darkMode":false}"#.utf8))
        XCTAssertFalse(s.darkMode)
        XCTAssertEqual(s.expiryWarningHours, 48)
        XCTAssertFalse(s.autoRefreshEnabled, "auto-refresh must be off by default")
        XCTAssertEqual(s.commuteMinutes, 30)
    }
}

@MainActor
final class StoreTests: XCTestCase {
    func testLocalEditsAreDirtyAndWinOverServer() {
        let store = ScheduleStore(fileURL: nil)
        store.setMe(.kayo)
        let d = Day(iso: "2026-11-25")!
        store.setPlan(d, .work(WorkShift(start: 960, end: 1440)))
        XCTAssertTrue(store.hasPendingUploads)

        // Server has an older/different copy: unsent local edit wins.
        store.applyServer([.kayo: [d: SyncedDay(plan: .off, updated: Int64.max)]])
        XCTAssertEqual(store.data.plans[d], .work(WorkShift(start: 960, end: 1440)))

        // After upload, a newer server version is taken.
        let sent = store.pendingUploads()
        store.markUploaded(sent)
        XCTAssertFalse(store.hasPendingUploads)
        store.applyServer([.kayo: [d: SyncedDay(plan: .off, updated: sent[d]!.updated + 1)]])
        XCTAssertEqual(store.data.plans[d], .off)

        // A "none" entry deletes the day.
        store.applyServer([.kayo: [d: SyncedDay(plan: nil, updated: sent[d]!.updated + 2)]])
        XCTAssertNil(store.data.plans[d])
    }

    func testOthersAndCopyWeek() {
        let store = ScheduleStore(fileURL: nil)
        store.setMe(.kayo)
        let mon = Day(iso: "2026-11-23")!
        store.applyServer([.samy: [mon: SyncedDay(plan: .work(WorkShift(start: 900, end: 1380)), updated: 1)]])
        XCTAssertEqual(store.data.othersWorking(mon), [.samy])

        store.setPlan(mon, .off)
        store.copyWeek(from: mon, to: mon.plus(7))
        XCTAssertEqual(store.data.plans[mon.plus(7)], .off)
        store.markUnsetOff(mon)
        XCTAssertEqual(store.data.weekDays(mon).filter { store.data.plans[$0] == .off }.count, 7)
        store.clearWeek(mon)
        XCTAssertFalse(store.data.hasAnyEntry(mon))
    }

    func testPersistsToDisk() throws {
        let url = FileManager.default.temporaryDirectory.appendingPathComponent("sz-test-\(UUID()).json")
        defer { try? FileManager.default.removeItem(at: url) }
        let d = Day.today
        do {
            let store = ScheduleStore(fileURL: url)
            store.setMe(.austin)
            store.setPlan(d, .work(WorkShift(start: 720, end: 1560)))
            store.updateSettings { $0.expiryWarningHours = 12; $0.autoRefreshEnabled = true }
        }
        let again = ScheduleStore(fileURL: url)
        XCTAssertEqual(again.data.me, .austin)
        XCTAssertEqual(again.data.plans[d], .work(WorkShift(start: 720, end: 1560)))
        XCTAssertEqual(again.data.settings.expiryWarningHours, 12)
        XCTAssertTrue(again.data.settings.autoRefreshEnabled)
        XCTAssertTrue(again.hasPendingUploads, "unsent edits survive a restart")
    }
}

final class SigningTests: XCTestCase {
    func testParsesExpiryOutOfCMSBlob() {
        let plist = """
        <?xml version="1.0" encoding="UTF-8"?>
        <!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
        <plist version="1.0"><dict>
        <key>CreationDate</key><date>2026-11-20T18:00:00Z</date>
        <key>ExpirationDate</key><date>2026-11-27T18:00:00Z</date>
        <key>Name</key><string>SideStore</string>
        </dict></plist>
        """
        var blob = Data([0x30, 0x80, 0x06, 0x09, 0xFF, 0x00])
        blob.append(Data(plist.utf8))
        blob.append(Data([0xA0, 0x82, 0x00, 0x00, 0x13]))
        let info = SigningInfo.parse(blob)
        XCTAssertEqual(info?.expires, ISO8601DateFormatter().date(from: "2026-11-27T18:00:00Z"))
        XCTAssertEqual(info?.created, ISO8601DateFormatter().date(from: "2026-11-20T18:00:00Z"))
        XCTAssertNil(SigningInfo.parse(Data("garbage".utf8)))
    }

    func testWarningPoints() {
        XCTAssertEqual(Reminders.expiryWarningPoints(windowHours: 0), [])
        XCTAssertEqual(Reminders.expiryWarningPoints(windowHours: 48), [48, 6, 1])
        XCTAssertEqual(Reminders.expiryWarningPoints(windowHours: 6), [6, 1])
        XCTAssertEqual(Reminders.expiryWarningPoints(windowHours: 3), [3, 1])
        XCTAssertEqual(Reminders.expiryWarningPoints(windowHours: 1), [1])
    }

    @MainActor
    func testWarningWindow() {
        let monitor = SigningMonitor()
        var s = AppSettings()
        // No profile in the simulator.
        XCTAssertNil(monitor.info)
        XCTAssertFalse(monitor.isWarning(settings: s))
        s.expiryWarningHours = 0
        XCTAssertFalse(monitor.isWarning(settings: s))
    }
}

final class ProtocolTests: XCTestCase {
    func testParseState() throws {
        let json: [String: Any] = [
            "rev": 42,
            "people": [
                "samy": ["2026-11-25": ["type": "work", "start": 960, "end": 1440, "updated": 1795725900000]],
                "kayo": ["2026-11-25": ["type": "off", "updated": 5], "2026-11-26": ["type": "none", "updated": 6]],
            ],
        ]
        let st = try SyncAPI.parseState(json)
        XCTAssertEqual(st.rev, 42)
        let day = Day(iso: "2026-11-25")!
        XCTAssertEqual(st.people?[.samy]?[day], SyncedDay(plan: .work(WorkShift(start: 960, end: 1440)), updated: 1795725900000))
        XCTAssertEqual(st.people?[.kayo]?[day]?.plan, .off)
        XCTAssertEqual(st.people?[.kayo]?[day.plus(1)], SyncedDay(plan: nil, updated: 6))
        XCTAssertEqual(st.people?[.austin]?.count, 0)
        XCTAssertNil(try SyncAPI.parseState(["rev": 3, "unchanged": true]).people)
    }

    func testDayJSON() {
        XCTAssertEqual(SyncAPI.dayJSON(SyncedDay(plan: nil, updated: 9))["type"] as? String, "none")
        let w = SyncAPI.dayJSON(SyncedDay(plan: .work(WorkShift(start: 720, end: 1560)), updated: 9))
        XCTAssertEqual(w["start"] as? Int, 720)
        XCTAssertEqual(w["end"] as? Int, 1560)
    }

    func testCleanURL() {
        XCTAssertEqual(SyncManager.cleanURL(" 100.93.228.123:8787/ ", "x"), "http://100.93.228.123:8787")
        XCTAssertEqual(SyncManager.cleanURL("https://pciememorycontroller.com", "x"), "https://pciememorycontroller.com")
        XCTAssertEqual(SyncManager.cleanURL("", "fallback"), "fallback")
    }
}

/// Runs against the real Python server when CI starts one (SZ_TEST_SERVER); skipped otherwise.
final class LiveServerTests: XCTestCase {
    func testFullProtocolRoundTrip() async throws {
        guard let base = ProcessInfo.processInfo.environment["SZ_TEST_SERVER"], !base.isEmpty else {
            throw XCTSkip("SZ_TEST_SERVER not set")
        }
        let api = SyncAPI(baseURL: base, deviceId: "ios-test", deviceName: "CI")
        let person = Person.austin

        let hadPassword = try await api.authStatus(person)
        XCTAssertFalse(hadPassword)
        do {
            _ = try await api.authenticate(person, password: "x", setup: true)
            XCTFail("too-short password accepted")
        } catch let e as HTTPError { XCTAssertEqual(e.code, 400) }

        let token = try await api.authenticate(person, password: "test-pass", setup: true)
        XCTAssertTrue(token.hasPrefix("austin."))
        let hasPassword = try await api.authStatus(person)
        XCTAssertTrue(hasPassword)
        do {
            _ = try await api.authenticate(person, password: "wrong-pass", setup: false)
            XCTFail("wrong password accepted")
        } catch let e as HTTPError { XCTAssertEqual(e.code, 401) }

        let day = Day.today
        let start = try await api.state(token: token, since: -1, waitSeconds: 0)
        try await api.upload(token: token, days: [day: SyncedDay(plan: .work(WorkShift(start: 960, end: 1530)), updated: nowMillis())])
        let after = try await api.state(token: token, since: start.rev, waitSeconds: 2)
        XCTAssertGreaterThan(after.rev, start.rev)
        XCTAssertEqual(after.people?[.austin]?[day]?.plan, .work(WorkShift(start: 960, end: 1530)))

        // Long-poll with nothing new returns "unchanged".
        let idle = try await api.state(token: token, since: after.rev, waitSeconds: 1)
        XCTAssertNil(idle.people)

        // A bad token is a 401 (what the app treats as "password was reset").
        do {
            _ = try await api.state(token: "austin.nope", since: -1, waitSeconds: 0)
            XCTFail("bad token accepted")
        } catch let e as HTTPError { XCTAssertEqual(e.code, 401) }
    }
}
