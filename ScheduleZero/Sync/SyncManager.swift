import Foundation
import Network
import UIKit
import CoreTelephony

/// Keeps this device and the servers in step (port of Android's SyncManager).
///
/// - Two servers: main and backup. The app always tries the main first and falls back to the
///   backup when the main can't be reached. The servers sync with each other on their own.
/// - While the app is open: a long-poll gets other devices' changes within a second or two.
/// - After every local edit: the changed days upload immediately.
/// - Offline? Nothing is shown as an error. Edits stay queued and upload when there's a network
///   (or during iOS background refresh, which runs whenever iOS decides).
/// - Each server has its own password/token (passwords are never copied between servers).
@MainActor
final class SyncManager: ObservableObject {
    static let shared = SyncManager()

    nonisolated static let defaultPrimary = "http://100.93.228.123:8787"
    nonisolated static let defaultSecondary = "https://pciememorycontroller.com"
    nonisolated static let longPollSeconds = 25

    enum Connection { case offline, connecting, online }

    enum Auth: Equatable {
        case noPerson
        /// Haven't been able to ask a server yet (offline). The app still works locally.
        case unknown
        case signedIn
        /// Need a password for [server]: create one (hasPassword false) or enter the existing one.
        case needsPassword(hasPassword: Bool, server: String, isBackup: Bool)
    }

    struct Status: Equatable {
        var connection: Connection = .offline
        var auth: Auth = .noPerson
        var lastSync: Date?
        var primary = SyncManager.defaultPrimary
        var secondary = SyncManager.defaultSecondary
        /// Server currently in use (nil when offline).
        var activeServer: String?
        var pending = false
        /// iOS Settings has mobile data turned off for this app (Wi-Fi still works).
        var cellularBlocked = false
        /// User tapped "Use offline for now" on the password screen.
        var passwordSkipped = false

        var usingBackup: Bool { activeServer != nil && activeServer == secondary }
        var needsPassword: Bool { if case .needsPassword = auth { return true } else { return false } }
    }

    enum PasswordResult { case ok, wrong, tooShort, alreadySet, notSet, unreachable }

    @Published private(set) var status = Status()

    private let defaults: UserDefaults
    private var store: ScheduleStore { ScheduleStore.shared }
    private var liveTask: Task<Void, Never>?
    private var pushing = false
    private var pathMonitor: NWPathMonitor?
    private let cellular = CTCellularData()

    // Wake-up signal for the live loop (network came back, local edit, servers changed...).
    private var wakeContinuation: CheckedContinuation<Void, Never>?
    private var wakePending = false
    private var waitID = 0

    init(defaults: UserDefaults = .standard) {
        self.defaults = defaults
    }

    var primaryURL: String { defaults.string(forKey: "server") ?? Self.defaultPrimary }
    var secondaryURL: String { defaults.string(forKey: "server2") ?? Self.defaultSecondary }

    private func servers() -> [String] {
        var out: [String] = []
        for u in [primaryURL, secondaryURL] where !u.trimmingCharacters(in: .whitespaces).isEmpty && !out.contains(u) {
            out.append(u)
        }
        return out
    }

    private func token(for url: String) -> String? { defaults.string(forKey: "token@\(url)") }
    private func setToken(_ url: String, _ token: String?) { defaults.set(token, forKey: "token@\(url)") }
    private func rev(for url: String) -> Int64 {
        (defaults.object(forKey: "rev@\(url)") as? NSNumber)?.int64Value ?? -1
    }
    private func setRev(_ url: String, _ rev: Int64) { defaults.set(NSNumber(value: rev), forKey: "rev@\(url)") }

    private func clearAllTokens() {
        for key in defaults.dictionaryRepresentation().keys where key.hasPrefix("token@") || key.hasPrefix("rev@") {
            defaults.removeObject(forKey: key)
        }
    }

    private var deviceId: String {
        if let id = defaults.string(forKey: "deviceId") { return id }
        let id = UUID().uuidString.lowercased()
        defaults.set(id, forKey: "deviceId")
        return id
    }

    private var deviceName: String {
        var info = utsname()
        uname(&info)
        let model = withUnsafePointer(to: &info.machine) {
            $0.withMemoryRebound(to: CChar.self, capacity: 1) { String(cString: $0) }
        }
        return "Apple \(model)" // e.g. "Apple iPhone17,5" (iPhone 16e)
    }

    private func api(_ url: String) -> SyncAPI { SyncAPI(baseURL: url, deviceId: deviceId, deviceName: deviceName) }

    func setUp() {
        status = Status(
            auth: store.data.me == nil ? .noPerson : (servers().contains { token(for: $0) != nil } ? .signedIn : .unknown),
            primary: primaryURL,
            secondary: secondaryURL,
            pending: store.hasPendingUploads
        )
        store.onLocalEdit = { [weak self] in self?.pushSoon() }
        // Wake the loop as soon as a network shows up again.
        let monitor = NWPathMonitor()
        monitor.pathUpdateHandler = { path in
            if path.status == .satisfied { Task { @MainActor in SyncManager.shared.wakeUp() } }
        }
        monitor.start(queue: DispatchQueue(label: "sz.path"))
        pathMonitor = monitor
        cellular.cellularDataRestrictionDidUpdateNotifier = { state in
            Task { @MainActor in SyncManager.shared.status.cellularBlocked = (state == .restricted) }
        }
    }

    /// Screenshot demo: looks connected (or waiting for a password) without touching the network.
    func setDemoStatus() {
        status.connection = .online
        status.activeServer = primaryURL
        status.lastSync = Date()
        status.auth = DemoMode.screen == "password"
            ? .needsPassword(hasPassword: false, server: primaryURL, isBackup: false)
            : (DemoMode.screen == "picker" ? .noPerson : .signedIn)
    }

    // MARK: public actions

    func start() {
        guard liveTask == nil else { wakeUp(); return }
        liveTask = Task { await liveLoop() }
    }

    func stop() {
        liveTask?.cancel()
        liveTask = nil
        wakeUp() // let a waiting loop notice it was cancelled
        status.connection = .offline
        status.activeServer = nil
    }

    func wakeUp() {
        if let c = wakeContinuation {
            wakeContinuation = nil
            c.resume()
        } else {
            wakePending = true
        }
    }

    /// Called after local edits.
    func pushSoon() {
        status.pending = true
        Task {
            for url in servers() {
                if let t = token(for: url), await pushTo(url, t) { break }
            }
        }
    }

    func choosePerson(_ person: Person) {
        if store.data.me != person { clearAllTokens() }
        store.setMe(person)
        status.auth = .unknown
        status.passwordSkipped = false
        wakeUp()
    }

    /// Ask the servers again (e.g. after "Sync now") and show the password screen if needed.
    func refreshAuthState() {
        status.passwordSkipped = false
        wakeUp()
    }

    func skipPassword() { status.passwordSkipped = true }

    func submitPassword(_ password: String, setup: Bool) async -> PasswordResult {
        guard let me = store.data.me else { return .unreachable }
        var target = primaryURL
        if case .needsPassword(_, let server, _) = status.auth { target = server }
        do {
            let newToken = try await api(target).authenticate(me, password: password, setup: setup)
            setToken(target, newToken)
            status.auth = .signedIn
            status.connection = .online
            status.activeServer = target
            // Sign in to the other server too while we have the password, so failover is seamless.
            for other in servers() where other != target && token(for: other) == nil {
                do {
                    let has = try await api(other).authStatus(me)
                    let otherToken = try await api(other).authenticate(me, password: password, setup: !has)
                    setToken(other, otherToken)
                } catch {
                    // Backup not reachable / not set up yet: fine, it will ask later if ever needed.
                }
            }
            wakeUp()
            return .ok
        } catch let e as HTTPError {
            switch e.code {
            case 401: return .wrong
            case 400: return .tooShort
            case 409: setNeedsPassword(true, target); return .alreadySet
            case 404: setNeedsPassword(false, target); return .notSet
            default: return .unreachable
            }
        } catch {
            return .unreachable
        }
    }

    func signOut() {
        clearAllTokens()
        store.signOut()
        status.auth = .noPerson
        status.pending = false
        status.activeServer = nil
    }

    nonisolated static func cleanURL(_ u: String, _ def: String) -> String {
        var c = u.trimmingCharacters(in: .whitespacesAndNewlines)
        while c.hasSuffix("/") { c.removeLast() }
        if c.isEmpty { return def }
        return c.hasPrefix("http://") || c.hasPrefix("https://") ? c : "http://\(c)"
    }

    func setServers(primary: String, secondary: String) {
        let p = Self.cleanURL(primary, Self.defaultPrimary)
        let s2 = secondary.trimmingCharacters(in: .whitespacesAndNewlines)
        let s = s2.isEmpty ? "" : Self.cleanURL(s2, Self.defaultSecondary)
        defaults.set(p, forKey: "server")
        defaults.set(s, forKey: "server2")
        status.primary = p
        status.secondary = s
        status.activeServer = nil
        status.connection = .connecting
        status.auth = store.data.me == nil ? .noPerson : .unknown
        // Drop any long-poll still waiting on the old server and reconnect right away.
        if liveTask != nil {
            liveTask?.cancel()
            wakeUp()
            liveTask = Task { await liveLoop() }
        } else {
            wakeUp()
        }
    }

    /// Swap main and backup (tokens are kept per server, so no new sign-in is needed).
    func swapServers() { setServers(primary: secondaryURL, secondary: primaryURL) }

    func launchSyncOnce() { Task { _ = await syncOnce() } }

    /// One full round trip on the first reachable server. Used by background refresh.
    func syncOnce() async -> Bool {
        guard store.data.me != nil else { return false }
        for url in servers() {
            guard let t = token(for: url) else { continue }
            do {
                _ = await pushTo(url, t)
                let state = try await api(url).state(token: t, since: -1, waitSeconds: 0)
                applyState(url, state)
                return true
            } catch let e as HTTPError {
                if e.code == 401 { setToken(url, nil) }
            } catch {
                // unreachable: try the next server
            }
        }
        return false
    }

    // MARK: internals

    private func liveLoop() async {
        var backoff: Double = 3
        while !Task.isCancelled {
            guard let me = store.data.me else {
                await waitForWake(30)
                continue
            }
            if status.connection != .online { status.connection = .connecting }

            // Main first, then backup. The first one that answers wins this round.
            var reached = false
            var waitingForPassword = false
            for url in servers() {
                let t = token(for: url)
                do {
                    guard let t else {
                        let has = try await api(url).authStatus(me)
                        if Task.isCancelled { return }
                        setNeedsPassword(has, url)
                        reached = true
                        waitingForPassword = true
                        break
                    }
                    _ = await pushTo(url, t)
                    // Quick check-in first when (re)connecting, so the status turns "Connected" at once.
                    let settled = status.connection == .online && status.activeServer == url
                    let state = try await api(url).state(
                        token: t, since: rev(for: url), waitSeconds: settled ? Self.longPollSeconds : 0
                    )
                    if Task.isCancelled { return } // servers were changed while we waited
                    applyState(url, state)
                    reached = true
                    break
                } catch let e as HTTPError {
                    if e.code == 401 {
                        // Password was reset on that server (file deleted). Ask again next round.
                        setToken(url, nil)
                        reached = true
                        break
                    }
                } catch {
                    if Task.isCancelled { return }
                    // Unreachable / no signal / asleep: normal. Try the next server quietly.
                }
            }
            if Task.isCancelled { return }
            if !reached {
                status.connection = .offline
                status.activeServer = nil
                await waitForWake(backoff)
                backoff = min(backoff * 2, 60)
            } else {
                backoff = 3
                if waitingForPassword { await waitForWake(30) }
            }
        }
    }

    private func setNeedsPassword(_ hasPassword: Bool, _ url: String) {
        status.auth = .needsPassword(hasPassword: hasPassword, server: url, isBackup: url != primaryURL)
        status.connection = .online
        status.activeServer = url
    }

    private func waitForWake(_ seconds: Double) async {
        if wakePending { wakePending = false; return }
        waitID += 1
        let id = waitID
        await withCheckedContinuation { (c: CheckedContinuation<Void, Never>) in
            wakeContinuation = c
            Task { @MainActor [weak self] in
                try? await Task.sleep(nanoseconds: UInt64(seconds * 1_000_000_000))
                guard let self, self.waitID == id, let cont = self.wakeContinuation else { return }
                self.wakeContinuation = nil
                cont.resume()
            }
        }
    }

    private func applyState(_ url: String, _ state: SyncAPI.State) {
        if let people = state.people { store.applyServer(people) }
        setRev(url, state.rev)
        status.connection = .online
        status.auth = .signedIn
        status.activeServer = url
        status.lastSync = Date()
        status.pending = store.hasPendingUploads
    }

    /// Uploads pending edits to [url]. Returns true if nothing is left to upload.
    private func pushTo(_ url: String, _ token: String) async -> Bool {
        // Serialize uploads (we're on the main actor, so a flag is enough).
        while pushing { try? await Task.sleep(nanoseconds: 100_000_000) }
        pushing = true
        defer { pushing = false }
        let pending = store.pendingUploads()
        if pending.isEmpty { return true }
        do {
            try await api(url).upload(token: token, days: pending)
            store.markUploaded(pending)
            status.pending = store.hasPendingUploads
            return true
        } catch let e as HTTPError {
            if e.code == 401 { setToken(url, nil) }
            return false
        } catch {
            return false
        }
    }
}
