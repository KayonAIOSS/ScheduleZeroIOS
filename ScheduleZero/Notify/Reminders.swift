import Foundation
import UIKit
import UserNotifications

/// Local notifications: shift reminders, the Sunday check-in, and (iPhone only) the expiry warning.
/// Everything is recomputed from scratch after each change, like Android's ReminderScheduler.
/// iOS keeps at most 64 pending notifications per app; we use ~25.
enum Reminders {
    static let shiftCategory = "SHIFT"
    static let expiryCategory = "EXPIRY"
    static let expiryAutoCategory = "EXPIRY_AUTO"
    static let snoozeAction = "SNOOZE"
    static let runShortcutAction = "RUN_SHORTCUT"
    static let openSideStoreAction = "OPEN_SIDESTORE"

    private static var center: UNUserNotificationCenter { .current() }

    static func registerCategories() {
        let snooze = UNNotificationAction(identifier: snoozeAction, title: "Snooze 15 min", options: [])
        let openStore = UNNotificationAction(identifier: openSideStoreAction, title: "Open SideStore", options: [.foreground])
        let run = UNNotificationAction(identifier: runShortcutAction, title: "Run refresh shortcut", options: [.foreground])
        center.setNotificationCategories([
            UNNotificationCategory(identifier: shiftCategory, actions: [snooze], intentIdentifiers: [], options: []),
            UNNotificationCategory(identifier: expiryCategory, actions: [openStore], intentIdentifiers: [], options: []),
            UNNotificationCategory(identifier: expiryAutoCategory, actions: [run, openStore], intentIdentifiers: [], options: []),
        ])
    }

    static func requestPermission() {
        center.requestAuthorization(options: [.alert, .sound, .badge]) { _, _ in }
    }

    static func isAllowed() async -> Bool {
        let s = await center.notificationSettings()
        return s.authorizationStatus == .authorized || s.authorizationStatus == .provisional
    }

    // MARK: shifts + weekly prompt

    static func shiftId(_ d: Day) -> String { "shift-\(d.iso)" }

    static func sync(_ data: AppData, now: Date = Date()) {
        if DemoMode.isOn { return }
        var requests: [UNNotificationRequest] = []
        let today = Day(now)
        let lastDate = max(data.plans.keys.max() ?? today, today.plus(21))
        var d = today.minus(1)
        while d <= lastDate {
            if let at = data.reminderTime(d), data.reminderEnabled(d), at > now {
                requests.append(UNNotificationRequest(identifier: shiftId(d), content: shiftContent(data, d), trigger: trigger(at)))
            }
            d = d.plus(1)
        }
        if let weekly = weeklyRequest(data, now: now) { requests.append(weekly) }
        let toAdd = requests
        center.getPendingNotificationRequests { pending in
            let stale = pending.map(\.identifier).filter { $0.hasPrefix("shift-") || $0 == "weekly" }
            center.removePendingNotificationRequests(withIdentifiers: stale)
            for r in toAdd { center.add(r) }
        }
    }

    static func shiftContent(_ data: AppData, _ d: Day) -> UNMutableNotificationContent {
        let c = UNMutableNotificationContent()
        guard let plan = data.workPlan(d), let leave = data.leaveTime(d) else { return c }
        let use24 = data.settings.use24h
        c.title = "Work at \(Fmt.workMinutes(plan.start, use24))"
        c.body = "Leave by \(Fmt.time(leave, use24)) (\(data.settings.commuteMinutes) min drive).\n" +
            "Shift: \(Fmt.shift(plan, use24)) · \(Fmt.duration(plan.lengthMinutes))"
        c.sound = .default
        c.categoryIdentifier = shiftCategory
        c.threadIdentifier = "shifts"
        return c
    }

    private static func weeklyRequest(_ data: AppData, now: Date) -> UNNotificationRequest? {
        guard data.settings.weeklyPrompt else { return nil }
        let today = Day(now)
        // previousOrSame(Sunday): weekdayIndex 6 = Sunday.
        let sunday = today.minus((today.weekdayIndex + 1) % 7)
        var promptDay = sunday
        var at = sunday.at(minutes: data.settings.weeklyPromptMinutes)
        // Skip to next Sunday if this one's prompt time passed or the coming Mon–Sun week is already entered.
        if at <= now || data.hasAnyEntry(sunday.plus(1).weekStart) {
            promptDay = sunday.plus(7)
            at = promptDay.at(minutes: data.settings.weeklyPromptMinutes)
        }
        let c = UNMutableNotificationContent()
        c.title = "Next week's schedule isn't in yet"
        c.body = "Tap to enter your hours for Monday – Sunday."
        c.sound = .default
        return UNNotificationRequest(identifier: "weekly", content: c, trigger: trigger(at))
    }

    static func snooze(_ content: UNNotificationContent, minutes: Int = 15) {
        let c = (content.mutableCopy() as? UNMutableNotificationContent) ?? UNMutableNotificationContent()
        let t = UNTimeIntervalNotificationTrigger(timeInterval: TimeInterval(minutes * 60), repeats: false)
        center.add(UNNotificationRequest(identifier: "snooze-\(UUID().uuidString)", content: c, trigger: t))
    }

    static func sendTest() {
        let c = UNMutableNotificationContent()
        c.title = "Test reminder"
        c.body = "This is what a shift reminder will look like."
        c.sound = .default
        center.add(UNNotificationRequest(
            identifier: "test", content: c,
            trigger: UNTimeIntervalNotificationTrigger(timeInterval: 2, repeats: false)
        ))
    }

    // MARK: expiry warning (iPhone only)

    /// Points (hours before expiry) at which a warning notification fires, limited to the window.
    static func expiryWarningPoints(windowHours: Int) -> [Int] {
        guard windowHours > 0 else { return [] }
        return Array(Set([windowHours, 6, 1].filter { $0 <= windowHours })).sorted(by: >)
    }

    static func syncExpiry(settings: AppSettings, info: SigningInfo?, now: Date = Date()) {
        if DemoMode.isOn { return }
        var requests: [UNNotificationRequest] = []
        if let info {
            for h in expiryWarningPoints(windowHours: settings.expiryWarningHours) {
                let at = info.expires.addingTimeInterval(-TimeInterval(h) * 3600)
                guard at > now else { continue }
                let c = UNMutableNotificationContent()
                c.title = "ScheduleZero expires in \(h == 1 ? "1 hour" : "\(h) hours")"
                if settings.autoRefreshEnabled {
                    c.body = "Automatic refresh hasn't happened yet. Run the refresh shortcut, or open SideStore and tap Refresh All."
                    c.categoryIdentifier = expiryAutoCategory
                } else {
                    c.body = "Open SideStore and tap Refresh All so the app keeps opening. Your schedule is safe either way."
                    c.categoryIdentifier = expiryCategory
                }
                c.sound = .default
                c.threadIdentifier = "expiry"
                requests.append(UNNotificationRequest(identifier: "expiry-\(h)", content: c, trigger: trigger(at)))
            }
        }
        let toAdd = requests
        center.getPendingNotificationRequests { pending in
            center.removePendingNotificationRequests(withIdentifiers: pending.map(\.identifier).filter { $0.hasPrefix("expiry-") })
            for r in toAdd { center.add(r) }
        }
    }

    private static func trigger(_ at: Date) -> UNCalendarNotificationTrigger {
        let c = Calendar.current.dateComponents([.year, .month, .day, .hour, .minute, .second], from: at)
        return UNCalendarNotificationTrigger(dateMatching: c, repeats: false)
    }
}

/// Shows notifications while the app is open and handles the notification buttons.
final class NotificationDelegate: NSObject, UNUserNotificationCenterDelegate {
    static let shared = NotificationDelegate()

    func userNotificationCenter(
        _ center: UNUserNotificationCenter, willPresent notification: UNNotification
    ) async -> UNNotificationPresentationOptions {
        [.banner, .list, .sound]
    }

    func userNotificationCenter(_ center: UNUserNotificationCenter, didReceive response: UNNotificationResponse) async {
        switch response.actionIdentifier {
        case Reminders.snoozeAction:
            Reminders.snooze(response.notification.request.content)
        case Reminders.runShortcutAction:
            await MainActor.run { SigningMonitor.runRefreshShortcut(force: true) }
        case Reminders.openSideStoreAction:
            await MainActor.run { SigningMonitor.openSideStore() }
        default:
            break
        }
    }
}
