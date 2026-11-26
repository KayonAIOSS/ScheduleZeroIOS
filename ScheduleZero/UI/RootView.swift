import SwiftUI
import Combine

enum Route: Hashable {
    case week(Day)
    case settings
    case guide
}

struct RootView: View {
    @EnvironmentObject var store: ScheduleStore
    @EnvironmentObject var sync: SyncManager

    @State var now = Date()
    @State var path: [Route] = []
    @State var editing: Day?
    @State var comparing: Day?
    @State var demoApplied = false

    let tick = Timer.publish(every: 5, on: .main, in: .common).autoconnect()

    var body: some View {
        let data = store.data
        let palette: Palette = data.settings.darkMode ? .dark : .light
        content(data)
            .environment(\.palette, palette)
            .preferredColorScheme(data.settings.darkMode ? .dark : .light)
            .tint(palette.primary)
            .onReceive(tick) { now = $0 }
            .onAppear(perform: applyDemoScreen)
    }

    @ViewBuilder
    private func content(_ data: AppData) -> some View {
        let palette: Palette = data.settings.darkMode ? .dark : .light
        if let me = data.me {
            if case .needsPassword(let has, _, let isBackup) = sync.status.auth, !sync.status.passwordSkipped {
                PasswordView(person: me, hasPassword: has, isBackup: isBackup)
            } else {
                NavigationStack(path: $path) {
                    HomeView(
                        data: data, now: now,
                        openWeek: { path.append(.week($0.weekStart)) },
                        openSettings: { path.append(.settings) }
                    )
                    .navigationDestination(for: Route.self) { route in
                        switch route {
                        case .week(let w): WeekView(initialWeek: w, data: data, now: now)
                        case .settings: SettingsView(data: data, now: now)
                        case .guide: AutoRefreshGuideView()
                        }
                    }
                }
                .environment(\.editDay, { editing = $0 })
                .environment(\.compare, { comparing = $0 })
                .environment(\.toggleReminder, toggleReminder)
                .sheet(item: $editing) { day in
                    DayEditorSheet(
                        date: day, data: store.data,
                        onSave: { plan, reminder in save(day, plan, reminder) },
                        onNavigate: { editing = $0 },
                        onDismiss: { editing = nil }
                    )
                    .environment(\.palette, palette)
                    .presentationDetents([.large])
                    .presentationDragIndicator(.visible)
                    .presentationBackground(palette.surfaceContainer)
                }
                .sheet(item: $comparing) { day in
                    CompareSheet(date: day, data: store.data, onNavigate: { comparing = $0 })
                        .environment(\.palette, palette)
                        .presentationDetents([.large])
                        .presentationDragIndicator(.visible)
                        .presentationBackground(palette.surfaceContainer)
                }
            }
        } else {
            PersonPickerView { sync.choosePerson($0) }
        }
    }

    private func toggleReminder(_ date: Day, _ on: Bool) {
        if on { Reminders.requestPermission() }
        store.setReminder(date, on)
    }

    private func save(_ date: Day, _ plan: DayPlan?, _ reminder: Bool) {
        store.setPlan(date, plan)
        let current = store.data
        if plan?.work != nil, let remindAt = current.reminderTime(date), remindAt > Date(),
           reminder != current.reminderEnabled(date) {
            toggleReminder(date, reminder)
        }
    }

    /// Screenshot demo: open the requested screen.
    private func applyDemoScreen() {
        guard DemoMode.isOn, !demoApplied else { return }
        demoApplied = true
        let today = Day(now)
        switch DemoMode.screen {
        case "week": path = [.week(today.weekStart)]
        case "settings", "signing": path = [.settings]
        case "guide": path = [.settings, .guide]
        case "editor": editing = today.plus(1)
        case "compare": comparing = today
        default: break
        }
    }
}
