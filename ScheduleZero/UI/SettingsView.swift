import SwiftUI
import UIKit
import UserNotifications

struct SettingsView: View {
    let data: AppData
    let now: Date
    @EnvironmentObject var store: ScheduleStore
    @EnvironmentObject var sync: SyncManager
    @Environment(\.palette) var p
    @Environment(\.scenePhase) var scenePhase
    @Environment(\.toggleReminder) var toggleReminder

    @State var notifStatus: UNAuthorizationStatus = .authorized
    @State var confirmClear = false
    @State var confirmSignOut = false
    @State var rateText = ""

    private var s: AppSettings { data.settings }

    var body: some View {
        let queued = (0...21).map { Day(now).minus(1).plus($0) }
            .filter { d in data.reminderEnabled(d) && (data.reminderTime(d).map { $0 > now } ?? false) }

        ScrollView {
            LazyVStack(spacing: 12) {
                AccountGroup(data: data) { confirmSignOut = true }

                SigningGroup(data: data, now: now)

                SettingsGroup("Appearance") {
                    ToggleRow(title: "Dark mode", subtitle: "Pitch black with purple. Off = light with cyan.",
                              isOn: setting(\.darkMode))
                    RowDivider()
                    ToggleRow(title: "24-hour clock", subtitle: "Show 16:00 instead of 4:00 PM", isOn: setting(\.use24h))
                }

                SettingsGroup("Reminders") {
                    ToggleRow(
                        title: "Remind me before every shift",
                        subtitle: "Off by default. When off, tap the bell on a day to queue a one-off reminder.",
                        isOn: Binding(get: { s.defaultReminders }, set: { v in
                            if v { Reminders.requestPermission() }
                            store.updateSettings { $0.defaultReminders = v }
                            refreshNotifStatus()
                        })
                    )
                    RowDivider()
                    VStack(alignment: .leading, spacing: 4) {
                        HStack {
                            Text("Remind me").font(.szTitle).foregroundStyle(p.onSurface)
                            Spacer()
                            Text("\(Fmt.lead(s.reminderLeadMinutes)) before work").font(.szTitle).foregroundStyle(p.primary)
                        }
                        Slider(
                            value: Binding(
                                get: { Double(s.reminderLeadMinutes) },
                                set: { v in
                                    let m = min(max(Int((v / 15).rounded()) * 15, AppSettings.leadMin), AppSettings.leadMax)
                                    if m != s.reminderLeadMinutes { store.updateSettings { $0.reminderLeadMinutes = m } }
                                }
                            ),
                            in: Double(AppSettings.leadMin)...Double(AppSettings.leadMax),
                            step: Double(AppSettings.leadStep)
                        )
                        .tint(p.primary)
                        Text("15 minutes to 3 hours, in 15-minute steps. Applies to every reminder.")
                            .font(.szBodySmall).foregroundStyle(p.onSurfaceVariant)
                    }
                    .padding(16)
                    if notifStatus == .denied {
                        RowDivider()
                        ActionRow(title: "Notifications are blocked",
                                  subtitle: "Reminders can't appear until you allow them in iPhone Settings.",
                                  action: "Allow") { openAppSettings() }
                    }
                    RowDivider()
                    ActionRow(title: "Test notification", subtitle: "See what a reminder looks like.", action: "Send") {
                        Reminders.requestPermission()
                        Reminders.sendTest()
                        refreshNotifStatus()
                    }
                }

                SettingsGroup("Queued reminders") {
                    if queued.isEmpty {
                        Text("None queued. Tap the bell on any work day to add one.")
                            .font(.szBody).foregroundStyle(p.onSurfaceVariant)
                            .padding(16)
                    }
                    ForEach(queued) { d in
                        if d != queued.first { RowDivider() }
                        HStack {
                            VStack(alignment: .leading, spacing: 2) {
                                Text("\(Fmt.dayName(d)), \(Fmt.monthDay(d))").font(.szTitle).foregroundStyle(p.onSurface)
                                if let at = data.reminderTime(d), let w = data.workPlan(d) {
                                    Text("Alert at \(Fmt.time(at, s.use24h)) · shift \(Fmt.shift(w, s.use24h))")
                                        .font(.szBodySmall).foregroundStyle(p.onSurfaceVariant)
                                }
                            }
                            Spacer()
                            Button { store.setReminder(d, false) } label: {
                                Image(systemName: "xmark").foregroundStyle(p.onSurfaceVariant).frame(width: 44, height: 44)
                            }
                            .accessibilityLabel("Remove reminder")
                        }
                        .padding(.leading, 16).padding(.trailing, 4).padding(.vertical, 6)
                    }
                }

                SettingsGroup("Weekly check-in") {
                    ToggleRow(
                        title: "Sunday reminder to enter hours",
                        subtitle: "Only fires if that week is still empty.",
                        isOn: Binding(get: { s.weeklyPrompt }, set: { v in
                            if v { Reminders.requestPermission() }
                            store.updateSettings { $0.weeklyPrompt = v }
                        })
                    )
                    if s.weeklyPrompt {
                        RowDivider()
                        StepperRow(
                            title: "Sunday at",
                            value: Fmt.workMinutes(s.weeklyPromptMinutes, s.use24h),
                            canDec: s.weeklyPromptMinutes > 6 * 60,
                            canInc: s.weeklyPromptMinutes < 23 * 60 + 45,
                            onDec: { store.updateSettings { $0.weeklyPromptMinutes -= 15 } },
                            onInc: { store.updateSettings { $0.weeklyPromptMinutes += 15 } }
                        )
                    }
                }

                SettingsGroup("Commute & pay") {
                    StepperRow(
                        title: "Drive time",
                        value: "\(s.commuteMinutes) min",
                        canDec: s.commuteMinutes > 5,
                        canInc: s.commuteMinutes < 120,
                        onDec: { store.updateSettings { $0.commuteMinutes -= 5 } },
                        onInc: { store.updateSettings { $0.commuteMinutes += 5 } }
                    )
                    RowDivider()
                    VStack(alignment: .leading, spacing: 4) {
                        Text("Hourly pay (optional)").font(.szBodySmall).foregroundStyle(p.onSurfaceVariant)
                        HStack(spacing: 4) {
                            Text("$").foregroundStyle(p.onSurfaceVariant)
                            TextField("0.00", text: $rateText)
                                .keyboardType(.decimalPad)
                                .foregroundStyle(p.onSurface)
                                .onChange(of: rateText) { _, t in
                                    let clean = String(t.filter { $0.isNumber || $0 == "." }.prefix(8))
                                    if clean != t { rateText = clean; return }
                                    let v = Double(clean).flatMap { $0 > 0 ? $0 : nil }
                                    if v != s.hourlyRate { store.updateSettings { $0.hourlyRate = v } }
                                }
                        }
                        .font(.szBodyLarge)
                        .padding(.horizontal, 14)
                        .frame(height: 48)
                        .overlay(RoundedRectangle(cornerRadius: 10).strokeBorder(p.outline, lineWidth: 1))
                        Text("Shows estimated weekly pay. Leave blank to hide.")
                            .font(.szBodySmall).foregroundStyle(p.onSurfaceVariant)
                    }
                    .padding(16)
                }

                SettingsGroup("Data") {
                    ActionRow(title: "Clear all my schedules",
                              subtitle: "Removes every day you've entered, on all your devices.",
                              action: "Clear") { confirmClear = true }
                }

                Text("ScheduleZero \(Bundle.main.infoDictionary?["CFBundleShortVersionString"] as? String ?? "") for iPhone")
                    .font(.szBodySmall).foregroundStyle(p.onSurfaceVariant)
                    .frame(maxWidth: .infinity)
                    .padding(.top, 8)
            }
            .padding(.horizontal, 16)
            .padding(.bottom, 32)
        }
        .scrollDismissesKeyboard(.interactively)
        .background(p.background.ignoresSafeArea())
        .navigationTitle("Settings")
        .navigationBarTitleDisplayMode(.inline)
        .onAppear {
            rateText = s.hourlyRate.map { String(format: "%.2f", $0) } ?? ""
            refreshNotifStatus()
        }
        .onChange(of: scenePhase) { _, phase in if phase == .active { refreshNotifStatus() } }
        .alert("Switch person?", isPresented: $confirmSignOut) {
            Button("Switch", role: .destructive) { sync.signOut() }
            Button("Cancel", role: .cancel) {}
        } message: {
            Text("This device will forget \(data.me?.displayName ?? "you"). Your schedule stays on the server" +
                 (sync.status.pending ? ", but edits that haven't synced yet will be lost." : "."))
        }
        .alert("Clear everything?", isPresented: $confirmClear) {
            Button("Clear", role: .destructive) { store.clearAllSchedules() }
            Button("Cancel", role: .cancel) {}
        } message: {
            Text("Every day you've entered is removed here and on the server (so on all your devices too). Other people's schedules and your settings are kept.")
        }
    }

    private func setting(_ key: WritableKeyPath<AppSettings, Bool>) -> Binding<Bool> {
        Binding(get: { data.settings[keyPath: key] }, set: { v in store.updateSettings { $0[keyPath: key] = v } })
    }

    private func refreshNotifStatus() {
        Task {
            let st = await UNUserNotificationCenter.current().notificationSettings().authorizationStatus
            notifStatus = st
        }
    }
}

func openAppSettings() {
    if let url = URL(string: UIApplication.openSettingsURLString) { UIApplication.shared.open(url) }
}

// MARK: account

private struct AccountGroup: View {
    let data: AppData
    let onSignOut: () -> Void
    @EnvironmentObject var sync: SyncManager
    @Environment(\.palette) var p
    @State var primary = ""
    @State var secondary = ""

    var body: some View {
        let status = sync.status
        if let me = data.me {
            SettingsGroup("Account & sync") {
                if status.cellularBlocked {
                    ActionRow(title: "Mobile data is off",
                              subtitle: "iPhone Settings blocks mobile data for ScheduleZero, so it only syncs on Wi-Fi. Turn on Mobile Data in Settings > ScheduleZero.",
                              action: "Open") { openAppSettings() }
                    RowDivider()
                }
                HStack(spacing: 14) {
                    PersonDot(person: me, size: 28)
                    VStack(alignment: .leading, spacing: 2) {
                        Text(me.displayName).font(.szTitle).foregroundStyle(p.onSurface)
                        Text(statusLine(status)).font(.szBodySmall).foregroundStyle(p.onSurfaceVariant)
                    }
                    Spacer()
                    Button(status.needsPassword ? "Sign in" : "Sync now") {
                        sync.refreshAuthState()
                        sync.launchSyncOnce()
                    }
                    .buttonStyle(.szTonal)
                }
                .padding(16)
                RowDivider()
                VStack(alignment: .leading, spacing: 8) {
                    serverField("Main server", text: $primary)
                    serverField("Backup server", text: $secondary)
                    Text("Used automatically when the main server can't be reached.")
                        .font(.szBodySmall).foregroundStyle(p.onSurfaceVariant)
                    HStack {
                        Button("Swap main and backup") { sync.swapServers() }.buttonStyle(.szText)
                        Spacer()
                        let dirty = SyncManager.cleanURL(primary, SyncManager.defaultPrimary) != status.primary ||
                            trimmed(secondary) != status.secondary
                        Button("Save") { sync.setServers(primary: primary, secondary: secondary) }
                            .buttonStyle(.szText)
                            .disabled(!dirty)
                    }
                }
                .padding(.horizontal, 16).padding(.top, 12).padding(.bottom, 8)
                RowDivider()
                ActionRow(title: "Switch person", subtitle: "Sign this device in as someone else.", action: "Switch") { onSignOut() }
            }
            .onAppear { primary = status.primary; secondary = status.secondary }
            .onChange(of: status.primary) { _, v in primary = v }
            .onChange(of: status.secondary) { _, v in secondary = v }
        }
    }

    private func trimmed(_ s: String) -> String {
        let t = s.trimmingCharacters(in: .whitespacesAndNewlines)
        return t.isEmpty ? "" : SyncManager.cleanURL(t, SyncManager.defaultSecondary)
    }

    private func statusLine(_ status: SyncManager.Status) -> String {
        let ago: String? = status.lastSync.map {
            let mins = Int(Date().timeIntervalSince($0) / 60)
            return mins < 1 ? "just now" : Fmt.duration(mins) + " ago"
        }
        let whereTo = status.usingBackup ? "backup server" : "main server"
        if status.needsPassword { return "Not signed in. Tap Sign in." }
        switch status.connection {
        case .online: return "Connected to \(whereTo)" + (ago.map { " · synced \($0)" } ?? "")
        case .connecting: return "Connecting…"
        case .offline:
            if status.pending { return "Offline. Your changes will upload when connected." }
            return "Offline" + (ago.map { " · last synced \($0)" } ?? "")
        }
    }

    private func serverField(_ label: String, text: Binding<String>) -> some View {
        VStack(alignment: .leading, spacing: 2) {
            Text(label).font(.szBodySmall).foregroundStyle(p.onSurfaceVariant)
            TextField(label, text: text)
                .keyboardType(.URL)
                .textInputAutocapitalization(.never)
                .autocorrectionDisabled()
                .font(.szBody)
                .foregroundStyle(p.onSurface)
                .padding(.horizontal, 12)
                .frame(height: 44)
                .overlay(RoundedRectangle(cornerRadius: 10).strokeBorder(p.outline, lineWidth: 1))
        }
    }
}

// MARK: app expiry (iPhone only)

struct SigningGroup: View {
    let data: AppData
    let now: Date
    @EnvironmentObject var store: ScheduleStore
    @EnvironmentObject var signing: SigningMonitor
    @Environment(\.palette) var p

    var body: some View {
        let s = data.settings
        let use24 = s.use24h
        SettingsGroup("App expiry") {
            VStack(alignment: .leading, spacing: 2) {
                if let info = signing.info {
                    let left = info.secondsLeft(now)
                    Text(left > 0 ? "Expires in \(Fmt.hoursLeft(left))" : "Expired").font(.szTitle)
                        .foregroundStyle(signing.isWarning(settings: s, now: now) ? p.warning : p.onSurface)
                    Text("Stops opening \(Fmt.dayTime(info.expires, use24)), \(Fmt.monthDay(Day(info.expires)))" +
                         (info.created.map { " · last refreshed \(Fmt.dayTime($0, use24))" } ?? ""))
                        .font(.szBodySmall).foregroundStyle(p.onSurfaceVariant)
                } else {
                    Text("No expiry date found").font(.szTitle).foregroundStyle(p.onSurface)
                    Text("This install wasn't signed by SideStore or AltStore, so it doesn't expire.")
                        .font(.szBodySmall).foregroundStyle(p.onSurfaceVariant)
                }
            }
            .padding(16)
            RowDivider()
            VStack(alignment: .leading, spacing: 4) {
                HStack {
                    Text("Warn me").font(.szTitle).foregroundStyle(p.onSurface)
                    Spacer()
                    Text(s.expiryWarningHours == 0 ? "Off"
                         : "\(s.expiryWarningHours) hour\(s.expiryWarningHours == 1 ? "" : "s") before")
                        .font(.szTitle).foregroundStyle(p.primary)
                }
                Slider(
                    value: Binding(
                        get: { Double(s.expiryWarningHours) },
                        set: { v in
                            let h = min(max(Int(v.rounded()), 0), AppSettings.expiryWarningMax)
                            if h != s.expiryWarningHours { store.updateSettings { $0.expiryWarningHours = h } }
                        }
                    ),
                    in: 0...Double(AppSettings.expiryWarningMax),
                    step: 1,
                    onEditingChanged: { editing in if !editing && s.expiryWarningHours > 0 { Reminders.requestPermission() } }
                )
                .tint(p.primary)
                Text("Free Apple ID installs stop opening after 7 days until they're refreshed in SideStore. Shows a banner and sends notifications during this window. Off to 48 hours, in 1-hour steps.")
                    .font(.szBodySmall).foregroundStyle(p.onSurfaceVariant)
                    .fixedSize(horizontal: false, vertical: true)
            }
            .padding(16)
            RowDivider()
            VStack(alignment: .leading, spacing: 6) {
                Toggle(isOn: Binding(get: { s.autoRefreshEnabled }, set: { v in store.updateSettings { $0.autoRefreshEnabled = v } })) {
                    HStack(spacing: 8) {
                        Text("Auto-refresh").font(.szTitle).foregroundStyle(p.onSurface)
                        ExperimentalTag()
                    }
                }
                .tint(p.primary)
                Text("Refreshes through a Shortcuts automation. Experimental: it may not work on every iPhone or SideStore version, so keep an eye on the expiry date until you've seen it work.")
                    .font(.szBodySmall).foregroundStyle(p.onSurfaceVariant)
                    .fixedSize(horizontal: false, vertical: true)
            }
            .padding(16)
            if s.autoRefreshEnabled {
                RowDivider()
                NavigationLink(value: Route.guide) {
                    HStack {
                        VStack(alignment: .leading, spacing: 2) {
                            Text("Set up the shortcut").font(.szTitle).foregroundStyle(p.onSurface)
                            Text("Step-by-step, about 5 minutes, once.").font(.szBodySmall).foregroundStyle(p.onSurfaceVariant)
                        }
                        Spacer()
                        Image(systemName: "chevron.right").foregroundStyle(p.onSurfaceVariant)
                    }
                    .padding(16)
                    .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                RowDivider()
                ActionRow(title: "Test it now",
                          subtitle: lastCheckText(),
                          action: "Run") { SigningMonitor.runRefreshShortcut(force: true) }
            }
        }
    }

    private func lastCheckText() -> String {
        guard let last = signing.lastAutomationCheck else {
            return "Runs the \"\(SigningMonitor.shortcutName)\" shortcut. It hasn't checked in yet."
        }
        let mins = Int(Date().timeIntervalSince(last) / 60)
        return "Runs the \"\(SigningMonitor.shortcutName)\" shortcut. Last check-in: " +
            (mins < 1 ? "just now" : Fmt.duration(mins) + " ago") + "."
    }
}

struct ExperimentalTag: View {
    @Environment(\.palette) var p
    var body: some View {
        Text("EXPERIMENTAL")
            .font(.system(size: 9, weight: .bold))
            .tracking(0.8)
            .foregroundStyle(p.warning)
            .padding(.horizontal, 6)
            .padding(.vertical, 2)
            .overlay(Capsule().strokeBorder(p.warning, lineWidth: 1))
    }
}

// MARK: building blocks

struct SettingsGroup<Content: View>: View {
    let title: String
    @ViewBuilder var content: () -> Content
    @Environment(\.palette) var p

    init(_ title: String, @ViewBuilder content: @escaping () -> Content) {
        self.title = title
        self.content = content
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            LabelText(title).padding(.leading, 4).padding(.top, 8).padding(.bottom, 8)
            CardBox(border: p.outlineVariant) {
                VStack(alignment: .leading, spacing: 0) { content() }
            }
        }
    }
}

struct RowDivider: View {
    @Environment(\.palette) var p
    var body: some View { Rectangle().fill(p.outlineVariant).frame(height: 1) }
}

struct ToggleRow: View {
    let title: String
    let subtitle: String
    let isOn: Binding<Bool>
    @Environment(\.palette) var p
    var body: some View {
        Toggle(isOn: isOn) {
            VStack(alignment: .leading, spacing: 2) {
                Text(title).font(.szTitle).foregroundStyle(p.onSurface)
                Text(subtitle).font(.szBodySmall).foregroundStyle(p.onSurfaceVariant)
                    .fixedSize(horizontal: false, vertical: true)
            }
        }
        .tint(p.primary)
        .padding(16)
    }
}

struct ActionRow: View {
    let title: String
    let subtitle: String
    let action: String
    let onTap: () -> Void
    @Environment(\.palette) var p
    var body: some View {
        HStack(spacing: 12) {
            VStack(alignment: .leading, spacing: 2) {
                Text(title).font(.szTitle).foregroundStyle(p.onSurface)
                Text(subtitle).font(.szBodySmall).foregroundStyle(p.onSurfaceVariant)
                    .fixedSize(horizontal: false, vertical: true)
            }
            Spacer(minLength: 0)
            Button(action, action: onTap).buttonStyle(.szTonal)
        }
        .padding(16)
    }
}

struct StepperRow: View {
    let title: String
    let value: String
    let canDec: Bool
    let canInc: Bool
    let onDec: () -> Void
    let onInc: () -> Void
    @Environment(\.palette) var p
    var body: some View {
        HStack {
            Text(title).font(.szTitle).foregroundStyle(p.onSurface)
            Spacer()
            Button(action: onDec) { Image(systemName: "chevron.left").frame(width: 44, height: 44) }
                .disabled(!canDec).accessibilityLabel("Decrease")
            Text(value).font(.szTitle).foregroundStyle(p.primary)
            Button(action: onInc) { Image(systemName: "chevron.right").frame(width: 44, height: 44) }
                .disabled(!canInc).accessibilityLabel("Increase")
        }
        .foregroundStyle(p.onSurface)
        .padding(.leading, 16).padding(.trailing, 8).padding(.vertical, 8)
    }
}
