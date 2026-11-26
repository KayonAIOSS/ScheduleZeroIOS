import SwiftUI

struct WeekView: View {
    let initialWeek: Day
    let data: AppData
    let now: Date
    @EnvironmentObject var store: ScheduleStore
    @Environment(\.palette) var p
    @Environment(\.editDay) var editDay

    @State var week: Day?
    @State var confirmCopy = false
    @State var confirmClear = false

    private static let maxWeeksBack = 8
    private static let maxWeeksAhead = 8

    var body: some View {
        let activeDate = data.activeWorkDate(now)
        let currentWeek = activeDate.weekStart
        let week = self.week ?? initialWeek
        let prevWeek = week.minus(7)
        let offset = (week.epochDay - currentWeek.epochDay) / 7

        ScrollView {
            LazyVStack(spacing: 10) {
                HStack {
                    Button { self.week = prevWeek } label: {
                        Image(systemName: "chevron.left").frame(width: 44, height: 44)
                    }
                    .disabled(offset <= -Self.maxWeeksBack)
                    .accessibilityLabel("Previous week")
                    VStack(spacing: 2) {
                        Text(relativeWeek(offset)).font(.szTitleLarge).foregroundStyle(p.onSurface)
                        Text(Fmt.weekRange(week)).font(.szBody).foregroundStyle(p.onSurfaceVariant)
                    }
                    .frame(maxWidth: .infinity)
                    Button { self.week = week.plus(7) } label: {
                        Image(systemName: "chevron.right").frame(width: 44, height: 44)
                    }
                    .disabled(offset >= Self.maxWeeksAhead)
                    .accessibilityLabel("Next week")
                }
                .foregroundStyle(p.onSurface)
                if offset != 0 {
                    Button("Jump to this week") { self.week = currentWeek }.buttonStyle(.szText)
                }

                WeekStats(data: data, week: week)

                if !data.hasAnyEntry(week) {
                    CardBox(fill: p.primaryContainer) {
                        VStack(alignment: .leading, spacing: 4) {
                            Text("Nothing entered for this week").font(.szTitle).foregroundStyle(p.onPrimaryContainer)
                            Text("Tap a day below to add hours, then use \"Save & next\" to move through the week.")
                                .font(.szBody).foregroundStyle(p.onPrimaryContainer.opacity(0.85))
                                .fixedSize(horizontal: false, vertical: true)
                            HStack(spacing: 8) {
                                Button("Start with Monday") { editDay(week) }.buttonStyle(.szFilled)
                                if data.hasAnyEntry(prevWeek) {
                                    Button("Copy previous") { store.copyWeek(from: prevWeek, to: week) }.buttonStyle(.szOutlined)
                                }
                            }
                            .padding(.top, 6)
                        }
                        .padding(16)
                    }
                }

                SectionHeader("Monday – Sunday")
                ForEach(data.weekDays(week)) { d in
                    DayCard(
                        date: d, data: data, now: now,
                        label: d == activeDate ? "\(Fmt.dayName(d)) · Today" : nil,
                        highlight: d == activeDate
                    )
                }

                Text("Weeks run Monday to Sunday and reset at Sunday 11:59 PM, so last week's hours are never " +
                     "reused by accident. You can enter next week any time before then and it'll load on its own. " +
                     "Shifts that end after midnight (up to 2 AM) count toward the day they started.")
                    .font(.szBodySmall)
                    .foregroundStyle(p.onSurfaceVariant)
                    .multilineTextAlignment(.center)
                    .padding(.top, 12)
                    .padding(.horizontal, 8)
            }
            .padding(.horizontal, 16)
            .padding(.bottom, 24)
        }
        .background(p.background.ignoresSafeArea())
        .navigationTitle("Week schedule")
        .navigationBarTitleDisplayMode(.inline)
        .toolbar {
            ToolbarItem(placement: .topBarTrailing) {
                Menu {
                    Button("Copy previous week") {
                        if data.hasAnyEntry(week) { confirmCopy = true } else { store.copyWeek(from: prevWeek, to: week) }
                    }
                    .disabled(!data.hasAnyEntry(prevWeek))
                    Button("Mark unset days as off") { store.markUnsetOff(week) }
                        .disabled(!data.weekDays(week).contains { data.plans[$0] == nil })
                    Button("Clear this week", role: .destructive) { confirmClear = true }
                        .disabled(!data.hasAnyEntry(week))
                } label: {
                    Image(systemName: "ellipsis.circle")
                }
                .accessibilityLabel("More")
            }
        }
        .alert("Replace this week?", isPresented: $confirmCopy) {
            Button("Replace", role: .destructive) { store.copyWeek(from: prevWeek, to: week) }
            Button("Cancel", role: .cancel) {}
        } message: {
            Text("This week already has entries. Copying will overwrite them with \(Fmt.weekRange(prevWeek)).")
        }
        .alert("Clear this week?", isPresented: $confirmClear) {
            Button("Clear", role: .destructive) { store.clearWeek(week) }
            Button("Cancel", role: .cancel) {}
        } message: {
            Text("Removes every entry and queued reminder for \(Fmt.weekRange(week)).")
        }
    }

    private func relativeWeek(_ offset: Int) -> String {
        switch offset {
        case 0: return "This week"
        case 1: return "Next week"
        case -1: return "Last week"
        case 2...: return "In \(offset) weeks"
        default: return "\(-offset) weeks ago"
        }
    }
}

private struct WeekStats: View {
    let data: AppData
    let week: Day
    @Environment(\.palette) var p

    var body: some View {
        let days = data.weekDays(week)
        let minutes = data.weekMinutes(week)
        CardBox(border: p.outlineVariant) {
            VStack(spacing: 0) {
                HStack {
                    stat(Fmt.hours(minutes), "hours")
                    stat("\(data.weekShiftCount(week))", "shifts")
                    stat("\(days.filter { data.plans[$0] == .off }.count)", "off")
                    stat("\(days.filter { data.plans[$0] == nil }.count)", "not set")
                }
                .padding(.vertical, 14)
                if let rate = data.settings.hourlyRate {
                    Text("Estimated pay ≈ \(Fmt.money(rate * Double(minutes) / 60)) before tax")
                        .font(.szBodySmall).foregroundStyle(p.onSurfaceVariant)
                        .frame(maxWidth: .infinity)
                        .padding(.bottom, 12)
                }
            }
        }
    }

    private func stat(_ value: String, _ label: String) -> some View {
        VStack(spacing: 2) {
            Text(value).font(.szTitleLarge).foregroundStyle(p.onSurface)
            Text(label).font(.szBodySmall).foregroundStyle(p.onSurfaceVariant)
        }
        .frame(maxWidth: .infinity)
    }
}
