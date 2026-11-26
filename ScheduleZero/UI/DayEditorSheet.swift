import SwiftUI

/// Bottom sheet for one day, with day-to-day navigation so a whole week can be entered quickly.
struct DayEditorSheet: View {
    let date: Day
    let data: AppData
    let onSave: (DayPlan?, Bool) -> Void
    let onNavigate: (Day) -> Void
    let onDismiss: () -> Void

    var body: some View {
        DayEditorContent(date: date, data: data, onSave: onSave, onNavigate: onNavigate, onDismiss: onDismiss)
            .id(date) // fresh state for each day
    }
}

private enum EditMode: Int, CaseIterable {
    case work, off, unset
    var title: String {
        switch self {
        case .work: return "Working"
        case .off: return "Day off"
        case .unset: return "Not set"
        }
    }
}

private enum Picking: Identifiable {
    case start, end
    var id: Int { self == .start ? 0 : 1 }
}

private struct DayEditorContent: View {
    let date: Day
    let data: AppData
    let onSave: (DayPlan?, Bool) -> Void
    let onNavigate: (Day) -> Void
    let onDismiss: () -> Void
    @Environment(\.palette) var p

    @State var mode: EditMode
    @State var start: Int
    @State var end: Int
    @State var reminder: Bool
    @State var picking: Picking?

    init(date: Day, data: AppData, onSave: @escaping (DayPlan?, Bool) -> Void,
         onNavigate: @escaping (Day) -> Void, onDismiss: @escaping () -> Void) {
        self.date = date
        self.data = data
        self.onSave = onSave
        self.onNavigate = onNavigate
        self.onDismiss = onDismiss
        let existing = data.plans[date]
        let suggested = data.recentShifts(1).first
            ?? WorkShift(start: WorkTime.defaultStart, end: WorkTime.defaultStart + WorkTime.defaultLength)
        let initial = existing?.work ?? suggested
        _mode = State(initialValue: existing == .off ? .off : .work)
        _start = State(initialValue: initial.start)
        _end = State(initialValue: initial.end)
        _reminder = State(initialValue: data.reminderEnabled(date))
    }

    private func setStart(_ v: Int) {
        let len = max(end - start, WorkTime.step)
        start = v
        if end <= start { end = min(start + len, WorkTime.max) }
        if end <= start { start = end - WorkTime.step }
    }

    private func build() -> DayPlan? {
        switch mode {
        case .work: return .work(WorkShift(start: start, end: end))
        case .off: return .off
        case .unset: return nil
        }
    }

    var body: some View {
        let use24 = data.settings.use24h
        ScrollView {
            VStack(alignment: .leading, spacing: 0) {
                HStack {
                    Button { onNavigate(date.minus(1)) } label: {
                        Image(systemName: "chevron.left").frame(width: 44, height: 44)
                    }
                    .accessibilityLabel("Previous day")
                    VStack(spacing: 2) {
                        Text(Fmt.dayName(date)).font(.szHeadline).foregroundStyle(p.onSurface)
                        Text(Fmt.monthDay(date)).font(.szBody).foregroundStyle(p.onSurfaceVariant)
                    }
                    .frame(maxWidth: .infinity)
                    Button { onNavigate(date.plus(1)) } label: {
                        Image(systemName: "chevron.right").frame(width: 44, height: 44)
                    }
                    .accessibilityLabel("Next day")
                }
                .foregroundStyle(p.onSurface)
                .padding(.top, 20)

                Picker("Day type", selection: $mode) {
                    ForEach(EditMode.allCases, id: \.self) { Text($0.title).tag($0) }
                }
                .pickerStyle(.segmented)
                .padding(.top, 16)

                Group {
                    switch mode {
                    case .work: workSection(use24)
                    case .off:
                        Text("Marked as a day off. No reminders will fire.")
                            .font(.szBody).foregroundStyle(p.onSurfaceVariant)
                    case .unset:
                        Text("No info for this day yet. It will show as \"Not entered\".")
                            .font(.szBody).foregroundStyle(p.onSurfaceVariant)
                    }
                }
                .padding(.top, 20)

                HStack(spacing: 8) {
                    Button("Cancel", action: onDismiss).buttonStyle(.szText)
                    Spacer()
                    Button("Save & next") {
                        onSave(build(), reminder)
                        onNavigate(date.plus(1))
                    }
                    .buttonStyle(.szTonal)
                    Button("Save") {
                        onSave(build(), reminder)
                        onDismiss()
                    }
                    .buttonStyle(.szFilled)
                }
                .padding(.top, 24)
                .padding(.bottom, 16)
            }
            .padding(.horizontal, 20)
        }
        .background(p.surfaceContainer.ignoresSafeArea())
        .sheet(item: $picking) { which in
            TimePickerList(
                title: which == .start ? "Start time" : "End time",
                options: which == .start ? WorkTime.options.filter { $0 < WorkTime.max } : WorkTime.options.filter { $0 > start },
                selected: which == .start ? start : end,
                use24h: use24
            ) { picked in
                if which == .start { setStart(picked) } else { end = picked }
                picking = nil
            }
            .presentationDetents([.medium, .large])
            .environment(\.palette, p)
        }
    }

    @ViewBuilder
    private func workSection(_ use24: Bool) -> some View {
        VStack(alignment: .leading, spacing: 0) {
            HStack(spacing: 12) {
                TimeField(label: "Start", minutes: start, use24: use24) { picking = .start }
                TimeField(label: "End", minutes: end, use24: use24) { picking = .end }
            }
            let leave = date.at(minutes: start - data.settings.commuteMinutes)
            Text("\(Fmt.duration(end - start)) shift · leave home \(Fmt.time(leave, use24))" +
                 (WorkTime.isNextDay(end) ? " · ends after midnight (still counts as \(Fmt.dayName(date)))" : ""))
                .font(.szBodySmall).foregroundStyle(p.onSurfaceVariant)
                .padding(.top, 10)

            let recents = data.recentShifts(4)
            if !recents.isEmpty {
                SectionHeader("Quick pick")
                ScrollView(.horizontal, showsIndicators: false) {
                    HStack(spacing: 8) {
                        ForEach(recents, id: \.self) { r in
                            Button(Fmt.shift(r, use24)) { start = r.start; end = r.end }
                                .buttonStyle(.szOutlined)
                        }
                    }
                }
            }

            SectionHeader("Reminder")
            let remindAt = date.at(minutes: start - data.settings.reminderLeadMinutes)
            let past = remindAt <= Date()
            CardBox(radius: 16, fill: p.surfaceContainerHigh) {
                Toggle(isOn: Binding(get: { reminder && !past }, set: { reminder = $0 })) {
                    VStack(alignment: .leading, spacing: 2) {
                        Text("Remind me before work").font(.szTitle).foregroundStyle(p.onSurface)
                        Text(past ? "That time has already passed today."
                             : "\(Fmt.lead(data.settings.reminderLeadMinutes)) before · at \(Fmt.time(remindAt, use24))")
                            .font(.szBodySmall).foregroundStyle(p.onSurfaceVariant)
                    }
                }
                .tint(p.primary)
                .disabled(past)
                .padding(16)
            }
        }
    }
}

private struct TimeField: View {
    let label: String
    let minutes: Int
    let use24: Bool
    let onTap: () -> Void
    @Environment(\.palette) var p

    var body: some View {
        Button(action: onTap) {
            VStack(alignment: .leading, spacing: 2) {
                LabelText(label)
                Text(Fmt.workMinutes(minutes, use24)).font(.szHeadlineSmall).foregroundStyle(p.onSurface)
                Text(WorkTime.isNextDay(minutes) ? "next morning" : " ").font(.szBodySmall).foregroundStyle(p.onSurfaceVariant)
            }
            .padding(14)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(p.surfaceContainerHigh, in: RoundedRectangle(cornerRadius: 16, style: .continuous))
            .overlay(RoundedRectangle(cornerRadius: 16, style: .continuous).strokeBorder(p.primary.opacity(0.6), lineWidth: 1))
        }
        .buttonStyle(.plain)
    }
}

/// Scrollable list of 15-minute slots between noon and 2 AM.
struct TimePickerList: View {
    let title: String
    let options: [Int]
    let selected: Int
    let use24h: Bool
    let onPick: (Int) -> Void
    @Environment(\.palette) var p
    @Environment(\.dismiss) var dismiss

    var body: some View {
        NavigationStack {
            ScrollViewReader { proxy in
                ScrollView {
                    LazyVStack(spacing: 2) {
                        ForEach(options, id: \.self) { m in
                            let isSel = m == selected
                            Button { onPick(m) } label: {
                                HStack {
                                    Text(Fmt.workMinutes(m, use24h)).font(.szTitle)
                                        .foregroundStyle(isSel ? p.onPrimary : p.onSurface)
                                    Spacer()
                                    if WorkTime.isNextDay(m) {
                                        Text("NEXT DAY").font(.szLabel).tracking(1)
                                            .foregroundStyle(isSel ? p.onPrimary : p.onSurfaceVariant)
                                    }
                                }
                                .padding(.horizontal, 16)
                                .padding(.vertical, 12)
                                .background(isSel ? p.primary : p.surfaceContainerHigh,
                                            in: RoundedRectangle(cornerRadius: 12, style: .continuous))
                                .contentShape(Rectangle())
                            }
                            .buttonStyle(.plain)
                            .id(m)
                        }
                    }
                    .padding(16)
                }
                .onAppear { proxy.scrollTo(selected, anchor: .center) }
            }
            .background(p.surfaceContainerHigh.ignoresSafeArea())
            .navigationTitle(title)
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) { Button("Cancel") { dismiss() } }
            }
        }
    }
}
