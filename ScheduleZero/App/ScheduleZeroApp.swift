import SwiftUI
import BackgroundTasks
import UserNotifications

@main
@MainActor
struct ScheduleZeroApp: App {
    @Environment(\.scenePhase) var scenePhase
    @StateObject private var store = ScheduleStore.shared
    @StateObject private var sync = SyncManager.shared
    @StateObject private var signing = SigningMonitor.shared

    nonisolated static let refreshTaskId = "com.schedulezero.app.refresh"

    init() {
        let store = ScheduleStore.shared
        let signing = SigningMonitor.shared
        if DemoMode.isOn {
            store.replaceForDemo(DemoMode.makeData())
        }
        UNUserNotificationCenter.current().delegate = NotificationDelegate.shared
        Reminders.registerCategories()
        store.onCommit = { data in
            Reminders.sync(data)
            Reminders.syncExpiry(settings: data.settings, info: SigningMonitor.shared.info)
        }
        if DemoMode.isOn {
            SyncManager.shared.setDemoStatus()
        } else {
            SyncManager.shared.setUp()
        }
        Reminders.sync(store.data)
        Reminders.syncExpiry(settings: store.data.settings, info: signing.info)
    }

    var body: some Scene {
        WindowGroup {
            RootView()
                .environmentObject(store)
                .environmentObject(sync)
                .environmentObject(signing)
        }
        .onChange(of: scenePhase) { _, phase in
            switch phase {
            case .active:
                signing.reload()
                Reminders.syncExpiry(settings: store.data.settings, info: signing.info)
                if !DemoMode.isOn {
                    sync.start()
                    sync.wakeUp()
                }
            case .background:
                sync.stop()
                Self.scheduleBackgroundRefresh()
            default:
                break
            }
        }
        .backgroundTask(.appRefresh(Self.refreshTaskId)) {
            await Self.backgroundRefresh()
        }
    }

    /// iOS runs this now and then (its choice, often every few hours) so other people's changes and
    /// queued edits still sync while the app is closed. Reminders are local, so they never depend on it.
    static func scheduleBackgroundRefresh() {
        if DemoMode.isOn { return }
        let req = BGAppRefreshTaskRequest(identifier: refreshTaskId)
        req.earliestBeginDate = Date(timeIntervalSinceNow: 15 * 60)
        try? BGTaskScheduler.shared.submit(req)
    }

    static func backgroundRefresh() async {
        await MainActor.run { scheduleBackgroundRefresh() }
        _ = await SyncManager.shared.syncOnce()
    }
}
