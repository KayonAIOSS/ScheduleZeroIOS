import SwiftUI

// MARK: environment actions

/// Opens the "who else is working" comparison for a day. Provided by RootView.
private struct CompareKey: EnvironmentKey { static let defaultValue: (Day) -> Void = { _ in } }
/// Opens the day editor. Provided by RootView.
private struct EditDayKey: EnvironmentKey { static let defaultValue: (Day) -> Void = { _ in } }
/// Toggles a day's reminder (and asks for notification permission). Provided by RootView.
private struct ToggleReminderKey: EnvironmentKey { static let defaultValue: (Day, Bool) -> Void = { _, _ in } }

extension EnvironmentValues {
    var compare: (Day) -> Void {
        get { self[CompareKey.self] }
        set { self[CompareKey.self] = newValue }
    }
    var editDay: (Day) -> Void {
        get { self[EditDayKey.self] }
        set { self[EditDayKey.self] = newValue }
    }
    var toggleReminder: (Day, Bool) -> Void {
        get { self[ToggleReminderKey.self] }
        set { self[ToggleReminderKey.self] = newValue }
    }
}

// MARK: small pieces

struct PersonDot: View {
    let person: Person
    var size: CGFloat = 12
    var body: some View {
        Circle().fill(person.color).frame(width: size, height: size)
    }
}

struct SectionHeader: View {
    let text: String
    init(_ text: String) { self.text = text }
    var body: some View {
        LabelText(text)
            .padding(.leading, 4)
            .padding(.top, 16)
            .padding(.bottom, 8)
            .frame(maxWidth: .infinity, alignment: .leading)
    }
}

struct ReminderIcon: View {
    let enabled: Bool
    let onToggle: () -> Void
    @Environment(\.palette) var p
    var body: some View {
        Button(action: onToggle) {
            Image(systemName: enabled ? "bell.fill" : "bell")
                .font(.system(size: 20))
                .foregroundStyle(enabled ? p.primary : p.onSurfaceVariant)
                .frame(width: 44, height: 44)
        }
        .buttonStyle(.plain)
        .accessibilityLabel(enabled ? "Cancel reminder" : "Queue reminder")
    }
}

/// Dots for the *other* people working that day. Tap to compare. Shows nothing if nobody else works.
struct OthersDots: View {
    let date: Day
    let data: AppData
    @Environment(\.compare) var compare
    var body: some View {
        let people = data.othersWorking(date)
        if !people.isEmpty {
            Button { compare(date) } label: {
                HStack(spacing: 6) { ForEach(people) { PersonDot(person: $0, size: 14) } }
                    .padding(.horizontal, 10)
                    .padding(.vertical, 12)
                    .contentShape(Capsule())
            }
            .buttonStyle(.plain)
            .accessibilityLabel("Also working: " + people.map(\.displayName).joined(separator: ", "))
        }
    }
}

struct DateBadge: View {
    let date: Day
    let plan: DayPlan?
    @Environment(\.palette) var p
    var body: some View {
        let (bg, fg): (Color, Color) = {
            switch plan {
            case .work: return (p.primaryContainer, p.onPrimaryContainer)
            case .off: return (p.secondaryContainer, p.onSecondaryContainer)
            case nil: return (p.surfaceContainerHighest, p.onSurfaceVariant)
            }
        }()
        VStack(spacing: 0) {
            Text(Fmt.dayShort(date).uppercased()).font(.szLabel).tracking(1)
            Text("\(date.day)").font(.system(size: 22, weight: .bold))
        }
        .foregroundStyle(fg)
        .frame(width: 54, height: 54)
        .background(bg, in: RoundedRectangle(cornerRadius: 14, style: .continuous))
    }
}

struct DayCard: View {
    let date: Day
    let data: AppData
    let now: Date
    let label: String?
    let highlight: Bool
    @Environment(\.palette) var p
    @Environment(\.editDay) var editDay
    @Environment(\.toggleReminder) var toggleReminder

    var body: some View {
        let plan = data.plans[date]
        let use24 = data.settings.use24h
        CardBox(border: highlight ? p.primary : p.outlineVariant) {
            HStack(spacing: 0) {
                Button { editDay(date) } label: {
                    HStack(spacing: 14) {
                        DateBadge(date: date, plan: plan)
                        VStack(alignment: .leading, spacing: 1) {
                            Text(label ?? Fmt.dayName(date)).font(.szTitle).foregroundStyle(p.onSurface)
                            switch plan {
                            case .work(let w):
                                Text(Fmt.shift(w, use24)).font(.szBodyLarge).foregroundStyle(p.onSurface)
                                if let leave = data.leaveTime(date) {
                                    Text("Leave \(Fmt.time(leave, use24)) · \(Fmt.hours(w.lengthMinutes))")
                                        .font(.szBodySmall).foregroundStyle(p.onSurfaceVariant)
                                }
                            case .off:
                                Text("Day off").font(.szBodyLarge).foregroundStyle(p.secondary)
                            case nil:
                                Text("Not entered · tap to add").font(.szBody).foregroundStyle(p.onSurfaceVariant)
                            }
                        }
                        Spacer(minLength: 0)
                    }
                    .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                OthersDots(date: date, data: data)
                if plan?.work != nil, let remindAt = data.reminderTime(date), remindAt > now {
                    ReminderIcon(enabled: data.reminderEnabled(date)) {
                        toggleReminder(date, !data.reminderEnabled(date))
                    }
                }
            }
            .padding(12)
        }
    }
}

/// Seven-dot overview of a week; tap a day to edit it.
struct WeekStrip: View {
    let weekStart: Day
    let data: AppData
    let today: Day
    @Environment(\.palette) var p
    @Environment(\.editDay) var editDay

    var body: some View {
        HStack(spacing: 0) {
            ForEach(data.weekDays(weekStart)) { d in
                let plan = data.plans[d]
                let isToday = d == today
                Button { editDay(d) } label: {
                    VStack(spacing: 6) {
                        Text(String(Fmt.dayShort(d).prefix(1)))
                            .font(.szLabel)
                            .foregroundStyle(isToday ? p.primary : p.onSurfaceVariant)
                        ZStack {
                            Circle().fill(fill(plan))
                            Text(label(plan))
                                .font(.system(size: 10, weight: .semibold))
                                .minimumScaleFactor(0.7)
                                .lineLimit(1)
                                .foregroundStyle(ink(plan))
                                .padding(2)
                        }
                        .frame(width: 34, height: 34)
                        Circle().fill(isToday ? p.primary : Color.clear).frame(width: 5, height: 5)
                    }
                    .padding(.vertical, 6)
                    .padding(.horizontal, 4)
                    .frame(maxWidth: .infinity)
                    .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
            }
        }
    }

    private func fill(_ plan: DayPlan?) -> Color {
        switch plan {
        case .work: return p.primary
        case .off: return p.secondaryContainer
        case nil: return p.surfaceContainerHighest
        }
    }

    private func ink(_ plan: DayPlan?) -> Color {
        switch plan {
        case .work: return p.onPrimary
        case .off: return p.onSecondaryContainer
        case nil: return p.onSurfaceVariant
        }
    }

    private func label(_ plan: DayPlan?) -> String {
        switch plan {
        case .work(let w): return Fmt.hours(w.lengthMinutes)
        case .off: return "off"
        case nil: return "–"
        }
    }
}

struct InfoBanner<Actions: View>: View {
    let title: String
    let message: String
    @ViewBuilder var actions: () -> Actions
    @Environment(\.palette) var p

    var body: some View {
        CardBox(fill: p.primaryContainer) {
            VStack(alignment: .leading, spacing: 4) {
                Text(title).font(.szTitle).foregroundStyle(p.onPrimaryContainer)
                Text(message).font(.szBody).foregroundStyle(p.onPrimaryContainer.opacity(0.85))
                    .fixedSize(horizontal: false, vertical: true)
                HStack(spacing: 8) { actions() }.padding(.top, 6)
            }
            .padding(16)
        }
    }
}

/// One-line sync state, e.g. "Synced", "Offline · will sync", "Sign in to sync". Never alarming.
struct SyncStatusText: View {
    let onTap: () -> Void
    @EnvironmentObject var sync: SyncManager
    @Environment(\.palette) var p

    var body: some View {
        let s = sync.status
        let (text, color): (String, Color) = {
            if s.needsPassword { return ("Sign in to sync", p.primary) }
            if s.auth == .noPerson { return ("Not synced", p.onSurfaceVariant) }
            if s.connection == .online && !s.pending { return (s.usingBackup ? "Synced (backup)" : "Synced", p.onSurfaceVariant) }
            if s.connection == .online { return ("Syncing…", p.onSurfaceVariant) }
            if s.connection == .connecting { return ("Connecting…", p.onSurfaceVariant) }
            if s.pending { return ("Offline · will sync", p.onSurfaceVariant) }
            return ("Offline", p.onSurfaceVariant)
        }()
        Button(action: onTap) { Text(text).font(.szBodySmall).foregroundStyle(color) }
            .buttonStyle(.plain)
    }
}

/// iPhone only: shown while the install is inside the chosen expiry-warning window.
struct ExpiryBanner: View {
    let info: SigningInfo
    let settings: AppSettings
    let now: Date
    let stuck: Bool
    @Environment(\.palette) var p

    var body: some View {
        CardBox(fill: p.surfaceContainer, border: p.warning.opacity(0.8)) {
            VStack(alignment: .leading, spacing: 6) {
                HStack(spacing: 8) {
                    Image(systemName: "clock.badge.exclamationmark").foregroundStyle(p.warning)
                    LabelText("App expires in \(Fmt.hoursLeft(info.secondsLeft(now)))", color: p.warning)
                }
                Text("Refresh ScheduleZero before \(Fmt.dayTime(info.expires, settings.use24h)).")
                    .font(.szTitle).foregroundStyle(p.onSurface)
                Text(stuck
                     ? "The automatic refresh doesn't seem to have run. Run it now, or refresh by hand in SideStore."
                     : "Open SideStore and tap Refresh All. It takes about a minute. If it expires, nothing is lost: refresh and it opens again.")
                    .font(.szBody).foregroundStyle(p.onSurfaceVariant)
                    .fixedSize(horizontal: false, vertical: true)
                HStack(spacing: 8) {
                    if settings.autoRefreshEnabled {
                        Button("Run shortcut") { SigningMonitor.runRefreshShortcut(force: true) }
                            .buttonStyle(.szFilled)
                        Button("Open SideStore") { SigningMonitor.openSideStore() }.buttonStyle(.szOutlined)
                    } else {
                        Button("Open SideStore") { SigningMonitor.openSideStore() }.buttonStyle(.szFilled)
                    }
                }
                .padding(.top, 4)
            }
            .padding(16)
        }
    }
}
