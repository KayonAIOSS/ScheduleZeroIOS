import Foundation
import UIKit

/// When this install stops opening. Free Apple ID signing (SideStore / AltStore) lasts 7 days; the
/// date is inside the provisioning profile that the signer embeds in the app
/// (`embedded.mobileprovision`, a signed CMS blob with a plain XML plist inside).
/// There is no profile in the simulator or when the app isn't sideloaded, so this is nil there.
struct SigningInfo: Equatable {
    let expires: Date
    /// When the current signature was made (= the last refresh).
    let created: Date?

    static func load(bundle: Bundle = .main) -> SigningInfo? {
        if let demo = DemoMode.signingInfo { return demo }
        guard let url = bundle.url(forResource: "embedded", withExtension: "mobileprovision"),
              let data = try? Data(contentsOf: url) else { return nil }
        return parse(data)
    }

    static func parse(_ data: Data) -> SigningInfo? {
        guard let start = data.range(of: Data("<?xml".utf8)),
              let end = data.range(of: Data("</plist>".utf8), in: start.lowerBound..<data.endIndex) else { return nil }
        let xml = data.subdata(in: start.lowerBound..<end.upperBound)
        guard let plist = try? PropertyListSerialization.propertyList(from: xml, format: nil) as? [String: Any],
              let expires = plist["ExpirationDate"] as? Date else { return nil }
        return SigningInfo(expires: expires, created: plist["CreationDate"] as? Date)
    }

    func secondsLeft(_ now: Date = Date()) -> TimeInterval { expires.timeIntervalSince(now) }
}

/// App-side state for the expiry warning and the experimental Shortcuts auto-refresh.
@MainActor
final class SigningMonitor: ObservableObject {
    static let shared = SigningMonitor()

    /// The shortcut the user builds by following the in-app guide.
    nonisolated static let shortcutName = "ScheduleZero Refresh"

    @Published private(set) var info: SigningInfo?
    @Published private(set) var lastAutomationCheck: Date?

    private let defaults = UserDefaults.standard

    init() { reload() }

    func reload() {
        info = SigningInfo.load()
        lastAutomationCheck = defaults.object(forKey: "sz.lastAutomationCheck") as? Date
    }

    /// True while the warning banner should show (inside the chosen window, before expiry).
    func isWarning(settings: AppSettings, now: Date = Date()) -> Bool {
        guard let info, settings.expiryWarningHours > 0 else { return false }
        let left = info.secondsLeft(now)
        return left > 0 && left <= TimeInterval(settings.expiryWarningHours) * 3600
    }

    /// With auto-refresh on, the shortcut checks from 48h before expiry. If we're well past that
    /// and still not refreshed, the automation probably isn't running.
    func autoRefreshLooksStuck(settings: AppSettings, now: Date = Date()) -> Bool {
        guard settings.autoRefreshEnabled, let info else { return false }
        let left = info.secondsLeft(now)
        return left > 0 && left < 24 * 3600
    }

    // MARK: opening other apps

    static func openSideStore() {
        let app = UIApplication.shared
        for scheme in ["sidestore://", "altstore://"] {
            if let url = URL(string: scheme), app.canOpenURL(url) {
                app.open(url)
                return
            }
        }
        if let url = URL(string: "sidestore://") { app.open(url) }
    }

    /// Runs the user's "ScheduleZero Refresh" shortcut. [force] makes its "needs refresh" check
    /// answer yes for the next 3 minutes, so it can be tested any day.
    static func runRefreshShortcut(force: Bool) {
        if force { UserDefaults.standard.set(Date().addingTimeInterval(180), forKey: "sz.forceRefreshUntil") }
        var c = URLComponents()
        c.scheme = "shortcuts"
        c.host = "run-shortcut"
        c.queryItems = [URLQueryItem(name: "name", value: shortcutName)]
        if let url = c.url { UIApplication.shared.open(url) }
    }

    static func openShortcutsApp() {
        if let url = URL(string: "shortcuts://") { UIApplication.shared.open(url) }
    }

    // MARK: used by the App Intent (runs inside this app's process)

    nonisolated static func recordAutomationCheck() {
        UserDefaults.standard.set(Date(), forKey: "sz.lastAutomationCheck")
    }

    nonisolated static func consumeForce() -> Bool {
        let d = UserDefaults.standard
        guard let until = d.object(forKey: "sz.forceRefreshUntil") as? Date else { return false }
        d.removeObject(forKey: "sz.forceRefreshUntil")
        return until > Date()
    }
}
