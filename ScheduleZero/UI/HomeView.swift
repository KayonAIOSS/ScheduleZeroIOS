import SwiftUI

struct HomeView: View {
    let data: AppData
    let now: Date
    let openWeek: (Day) -> Void
    let openSettings: () -> Void
    @EnvironmentObject var store: ScheduleStore
    @EnvironmentObject var signing: SigningMonitor
    @Environment(\.palette) var p
    @Environment(\.editDay) var editDay

    var body: some View {
        let active = data.activeWorkDate(now)
        let today = Day(now)
        let thisWeek = active.weekStart
        let nextWeek = thisWeek.plus(7)
        let lastWeek = thisWeek.minus(7)

        ScrollView {
            LazyVStack(spacing: 10) {
                header(today: today, thisWeek: thisWeek)

                if signing.isWarning(settings: data.settings, now: now), let info = signing.info {
                    ExpiryBanner(info: info, settings: data.settings, now: now,
                                 stuck: signing.autoRefreshLooksStuck(settings: data.settings, now: now))
                }

                TodayHero(data: data, date: active, now: now)

                if !data.hasAnyEntry(thisWeek) {
                    InfoBanner(
                        title: "This week's schedule isn't in yet",
                        message: "Weeks reset Sunday at 11:59 PM so old hours never carry over. Add this week's times to get started."
                    ) {
                        Button("Enter week") { openWeek(thisWeek) }.buttonStyle(.szFilled)
                        if data.hasAnyEntry(lastWeek) {
                            Button("Copy last week") { store.copyWeek(from: lastWeek, to: thisWeek) }.buttonStyle(.szOutlined)
                        }
                    }
                } else if [4, 5, 6].contains(today.weekdayIndex) && !data.hasAnyEntry(nextWeek) {
                    InfoBanner(
                        title: "Got next week's hours?",
                        message: "Enter them any time — they'll load automatically when the week rolls over at Sunday 11:59 PM."
                    ) {
                        Button("Add next week") { openWeek(nextWeek) }.buttonStyle(.szFilled)
                    }
                }

                WeekSummary(data: data, weekStart: thisWeek, today: today) { openWeek(thisWeek) }

                SectionHeader("Coming up")
                ForEach((1...7).map { active.plus($0) }) { d in
                    DayCard(
                        date: d, data: data, now: now,
                        label: d == today.plus(1) ? "Tomorrow" : (d == today ? "Today" : nil),
                        highlight: false
                    )
                }
                Button("Plan next week →") { openWeek(nextWeek) }
                    .buttonStyle(.szText)
                    .frame(maxWidth: .infinity)
            }
            .padding(.horizontal, 16)
            .padding(.bottom, 24)
        }
        .background(p.background.ignoresSafeArea())
        .toolbar(.hidden, for: .navigationBar)
    }

    private func header(today: Day, thisWeek: Day) -> some View {
        HStack(alignment: .center) {
            VStack(alignment: .leading, spacing: 2) {
                HStack(spacing: 10) {
                    if let me = data.me { PersonDot(person: me, size: 14) }
                    Text(greeting(now)).font(.szTitleLarge).foregroundStyle(p.onSurface)
                }
                HStack(spacing: 0) {
                    Text(Fmt.fullDate(today) + " · ").font(.szBodySmall).foregroundStyle(p.onSurfaceVariant)
                    SyncStatusText(onTap: openSettings)
                }
            }
            Spacer()
            Button { openWeek(thisWeek) } label: {
                Image(systemName: "calendar").font(.system(size: 20)).frame(width: 44, height: 44)
            }
            .accessibilityLabel("Week schedule")
            Button(action: openSettings) {
                Image(systemName: "gearshape.fill").font(.system(size: 20)).frame(width: 44, height: 44)
            }
            .accessibilityLabel("Settings")
        }
        .foregroundStyle(p.onSurface)
        .padding(.top, 8)
        .padding(.bottom, 4)
    }

    private func greeting(_ now: Date) -> String {
        switch Calendar.current.component(.hour, from: now) {
        case 5...11: return "Good morning"
        case 12...16: return "Good afternoon"
        case 17...21: return "Good evening"
        default: return "Late night"
        }
    }
}

private struct TodayHero: View {
    let data: AppData
    let date: Day
    let now: Date
    @Environment(\.palette) var p
    @Environment(\.editDay) var editDay

    var body: some View {
        let use24 = data.settings.use24h
        let isCarryOver = date != Day(now)
        VStack(alignment: .leading, spacing: 0) {
            HStack {
                LabelText(isCarryOver ? "Tonight · \(Fmt.dayName(date))'s shift" : "Today")
                Spacer()
                Button { editDay(date) } label: {
                    Image(systemName: "pencil").font(.system(size: 16)).foregroundStyle(p.onSurfaceVariant)
                        .frame(width: 32, height: 32)
                }
                .accessibilityLabel("Edit today")
            }
            switch data.plans[date] {
            case nil:
                Text("No schedule yet").font(.szHeadline).foregroundStyle(p.onSurface)
                Text("Add today's hours to see when to leave.")
                    .font(.szBody).foregroundStyle(p.onSurfaceVariant).padding(.top, 6)
                Button("Add today's hours") { editDay(date) }.buttonStyle(.szFilled).padding(.top, 14)
            case .off:
                Text("Day off").font(.szDisplay).foregroundStyle(p.onSurface)
                Text(nextShiftText(use24))
                    .font(.szBodyLarge).foregroundStyle(p.onSurfaceVariant).padding(.top, 6)
            case .work(let w):
                WorkHero(data: data, date: date, plan: w, now: now)
            }
            AlsoWorking(data: data, date: date)
        }
        .padding(20)
        .background(
            LinearGradient(colors: [p.primary.opacity(0.16), p.surfaceContainer], startPoint: .top, endPoint: .bottom),
            in: RoundedRectangle(cornerRadius: 28, style: .continuous)
        )
        .overlay(RoundedRectangle(cornerRadius: 28, style: .continuous).strokeBorder(p.primary.opacity(0.55), lineWidth: 1))
    }

    private func nextShiftText(_ use24: Bool) -> String {
        for i in 1...14 {
            let d = date.plus(i)
            if let w = data.workPlan(d) { return "Next shift: \(Fmt.dayName(d)) at \(Fmt.workMinutes(w.start, use24))" }
        }
        return "Nothing scheduled. Enjoy it."
    }
}

/// "Also working" strip on the hero card; tap to compare schedules.
private struct AlsoWorking: View {
    let data: AppData
    let date: Day
    @Environment(\.palette) var p
    @Environment(\.compare) var compare

    var body: some View {
        let people = data.othersWorking(date)
        if !people.isEmpty {
            Button { compare(date) } label: {
                VStack(alignment: .leading, spacing: 6) {
                    LabelText("Also working", color: p.onSurfaceVariant)
                    ForEach(people) { person in
                        if let w = data.others[person]?[date]?.work {
                            HStack(spacing: 8) {
                                PersonDot(person: person, size: 12)
                                Text(person.displayName).font(.szBody).foregroundStyle(p.onSurface)
                                Spacer()
                                Text(Fmt.shift(w, data.settings.use24h)).font(.szBody).foregroundStyle(p.onSurface)
                            }
                        }
                    }
                }
                .padding(.horizontal, 14)
                .padding(.vertical, 10)
                .frame(maxWidth: .infinity, alignment: .leading)
                .background(p.surfaceContainerHighest, in: RoundedRectangle(cornerRadius: 16, style: .continuous))
            }
            .buttonStyle(.plain)
            .padding(.top, 14)
        }
    }
}

private struct WorkHero: View {
    let data: AppData
    let date: Day
    let plan: WorkShift
    let now: Date
    @Environment(\.palette) var p
    @Environment(\.toggleReminder) var toggleReminder

    var body: some View {
        let use24 = data.settings.use24h
        let leave = date.at(minutes: plan.start - data.settings.commuteMinutes)
        let start = date.at(minutes: plan.start)
        let end = date.at(minutes: plan.end)
        let home = date.at(minutes: plan.end + data.settings.commuteMinutes)
        let cd = countdown(leave: leave, start: start, end: end, home: home, use24: use24)

        VStack(alignment: .leading, spacing: 0) {
            Text(cd.label).font(.szTitle).foregroundStyle(p.onSurfaceVariant)
            Text(cd.big).font(.szDisplay).foregroundStyle(p.onSurface)
            Text(cd.sub).font(.szBodyLarge).foregroundStyle(p.primary)

            if now >= start && now < end {
                let progress = now.timeIntervalSince(start) / end.timeIntervalSince(start)
                ProgressBar(value: min(max(progress, 0), 1)).padding(.top, 12)
            }

            HStack(spacing: 8) {
                TimeTile(label: "Leave", time: leave, workDate: date, use24: use24)
                TimeTile(label: "Start", time: start, workDate: date, use24: use24, emphasize: true)
                TimeTile(label: "End", time: end, workDate: date, use24: use24, emphasize: true)
                TimeTile(label: "Home", time: home, workDate: date, use24: use24)
            }
            .padding(.top, 18)

            HStack {
                Text("\(Fmt.duration(plan.lengthMinutes)) shift · \(data.settings.commuteMinutes) min drive")
                    .font(.szBodySmall).foregroundStyle(p.onSurfaceVariant)
                Spacer()
                if let remindAt = data.reminderTime(date), remindAt > now {
                    let on = data.reminderEnabled(date)
                    Button { toggleReminder(date, !on) } label: {
                        Label(on ? "Reminder \(Fmt.time(remindAt, use24))" : "Remind me",
                              systemImage: on ? "bell.fill" : "bell")
                    }
                    .buttonStyle(.szTonal)
                }
            }
            .padding(.top, 14)
        }
    }

    private func countdown(leave: Date, start: Date, end: Date, home: Date, use24: Bool) -> (label: String, big: String, sub: String) {
        if now < leave { return ("Leave in", Fmt.duration(Fmt.minutesUntil(now, leave)), "Head out at \(Fmt.time(leave, use24))") }
        if now < start { return ("Shift starts in", Fmt.duration(Fmt.minutesUntil(now, start)), "You should be on the road") }
        if now < end { return ("Shift ends in", Fmt.duration(Fmt.minutesUntil(now, end)), "Off at \(Fmt.time(end, use24))") }
        if now < home { return ("Home in about", Fmt.duration(Fmt.minutesUntil(now, home)), "Drive safe") }
        return ("Shift complete", "Done", "Nice work today")
    }
}

struct ProgressBar: View {
    let value: Double
    @Environment(\.palette) var p
    var body: some View {
        GeometryReader { g in
            ZStack(alignment: .leading) {
                Capsule().fill(p.surfaceContainerHighest)
                Capsule().fill(p.primary).frame(width: g.size.width * value)
            }
        }
        .frame(height: 6)
    }
}

private struct TimeTile: View {
    let label: String
    let time: Date
    let workDate: Day
    let use24: Bool
    var emphasize = false
    @Environment(\.palette) var p

    var body: some View {
        let text = Fmt.time(time, use24)
        let main = use24 ? text : String(text.split(separator: " ").first ?? "")
        var small = use24 ? "" : String(text.split(separator: " ").last ?? "")
        if Day(time) > workDate { small += small.isEmpty ? "+1 day" : " +1" }
        return VStack(spacing: 2) {
            Text(label.uppercased()).font(.szLabel).tracking(1)
                .foregroundStyle(emphasize ? p.onPrimaryContainer : p.onSurfaceVariant)
            Text(main).font(.system(size: 16, weight: .bold))
                .foregroundStyle(emphasize ? p.onPrimaryContainer : p.onSurface)
                .minimumScaleFactor(0.8).lineLimit(1)
            Text(small.isEmpty ? " " : small).font(.szLabel)
                .foregroundStyle(emphasize ? p.onPrimaryContainer : p.onSurfaceVariant)
        }
        .padding(.vertical, 10)
        .padding(.horizontal, 6)
        .frame(maxWidth: .infinity)
        .background(emphasize ? p.primaryContainer : p.surfaceContainerHighest,
                    in: RoundedRectangle(cornerRadius: 14, style: .continuous))
    }
}

struct WeekSummary: View {
    let data: AppData
    let weekStart: Day
    let today: Day
    let onOpen: () -> Void
    @Environment(\.palette) var p

    var body: some View {
        let minutes = data.weekMinutes(weekStart)
        let shifts = data.weekShiftCount(weekStart)
        CardBox(border: p.outlineVariant) {
            VStack(alignment: .leading, spacing: 12) {
                Button(action: onOpen) {
                    HStack(alignment: .bottom) {
                        VStack(alignment: .leading, spacing: 2) {
                            LabelText("This week")
                            Text(Fmt.weekRange(weekStart)).font(.szBodySmall).foregroundStyle(p.onSurfaceVariant)
                        }
                        Spacer()
                        VStack(alignment: .trailing, spacing: 2) {
                            Text("\(Fmt.hours(minutes)) · \(shifts) shift\(shifts == 1 ? "" : "s")")
                                .font(.szTitle).foregroundStyle(p.onSurface)
                            if let rate = data.settings.hourlyRate {
                                Text("≈ \(Fmt.money(rate * Double(minutes) / 60)) before tax")
                                    .font(.szBodySmall).foregroundStyle(p.onSurfaceVariant)
                            }
                        }
                    }
                    .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                WeekStrip(weekStart: weekStart, data: data, today: today)
            }
            .padding(16)
        }
    }
}
