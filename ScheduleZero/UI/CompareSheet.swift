import SwiftUI

/// "Who's working" for one day: a shared noon-to-2-AM timeline plus the week table.
struct CompareSheet: View {
    let date: Day
    let data: AppData
    let onNavigate: (Day) -> Void
    @Environment(\.palette) var p

    private var order: [Person] {
        (data.me.map { [$0] } ?? []) + Person.allCases.filter { $0 != data.me }
    }

    var body: some View {
        let use24 = data.settings.use24h
        let myPlan = data.plans[date]
        ScrollView {
            VStack(alignment: .leading, spacing: 0) {
                HStack {
                    Button { onNavigate(date.minus(1)) } label: {
                        Image(systemName: "chevron.left").frame(width: 44, height: 44)
                    }
                    .accessibilityLabel("Previous day")
                    VStack(spacing: 2) {
                        Text(Fmt.dayName(date)).font(.szHeadline).foregroundStyle(p.onSurface)
                        Text("\(Fmt.monthDay(date)) · who's working").font(.szBody).foregroundStyle(p.onSurfaceVariant)
                    }
                    .frame(maxWidth: .infinity)
                    Button { onNavigate(date.plus(1)) } label: {
                        Image(systemName: "chevron.right").frame(width: 44, height: 44)
                    }
                    .accessibilityLabel("Next day")
                }
                .foregroundStyle(p.onSurface)
                .padding(.top, 20)

                CardBox(fill: p.surfaceContainerHigh) {
                    VStack(alignment: .leading, spacing: 14) {
                        ForEach(order) { person in
                            let plan = data.planFor(person, date)
                            VStack(alignment: .leading, spacing: 6) {
                                HStack(spacing: 8) {
                                    PersonDot(person: person, size: 12)
                                    Text(person == data.me ? "\(person.displayName) (you)" : person.displayName)
                                        .font(.szTitle).foregroundStyle(p.onSurface)
                                    Spacer()
                                    Text(planText(plan, use24)).font(.szBody)
                                        .foregroundStyle(plan?.work != nil ? p.onSurface : p.onSurfaceVariant)
                                }
                                TimelineBar(plan: plan, person: person)
                                if person != data.me, let rel = relation(mine: myPlan, other: plan) {
                                    Text(rel).font(.szBodySmall).foregroundStyle(p.onSurfaceVariant)
                                }
                            }
                        }
                        TimelineAxis()
                    }
                    .padding(16)
                }
                .padding(.top, 16)

                SectionHeader("Week of \(Fmt.monthDay(date.weekStart))")
                WeekTable(selected: date, data: data, order: order)
            }
            .padding(.horizontal, 20)
            .padding(.bottom, 20)
        }
        .background(p.surfaceContainer.ignoresSafeArea())
    }

    private func planText(_ plan: DayPlan?, _ use24: Bool) -> String {
        switch plan {
        case .work(let w): return Fmt.shift(w, use24)
        case .off: return "Off"
        case nil: return "Not entered"
        }
    }

    /// How [other]'s shift relates to yours, in plain words.
    private func relation(mine: DayPlan?, other: DayPlan?) -> String? {
        guard let o = other?.work else { return nil }
        guard let m = mine?.work else { return mine == .off ? "You're off" : nil }
        func diff(_ a: Int, _ b: Int) -> String {
            if a < b { return "\(Fmt.duration(b - a)) before you" }
            if a > b { return "\(Fmt.duration(a - b)) after you" }
            return "same time as you"
        }
        let overlap = max(0, min(m.end, o.end) - max(m.start, o.start))
        return "Starts \(diff(o.start, m.start)) · ends \(diff(o.end, m.end)) · " +
            (overlap > 0 ? "together \(Fmt.duration(overlap))" : "no overlap")
    }
}

private struct TimelineBar: View {
    let plan: DayPlan?
    let person: Person
    @Environment(\.palette) var p

    var body: some View {
        GeometryReader { g in
            ZStack(alignment: .leading) {
                Capsule().fill(p.surfaceContainerHighest)
                if let w = plan?.work {
                    let span = CGFloat(WorkTime.max - WorkTime.min)
                    Capsule().fill(person.color)
                        .frame(width: g.size.width * CGFloat(w.end - w.start) / span)
                        .offset(x: g.size.width * CGFloat(w.start - WorkTime.min) / span)
                }
            }
        }
        .frame(height: 10)
    }
}

private struct TimelineAxis: View {
    struct Tick { let minutes: Int; let label: String }
    static let ticks = [Tick(minutes: 720, label: "12p"), Tick(minutes: 960, label: "4p"),
                        Tick(minutes: 1200, label: "8p"), Tick(minutes: 1440, label: "12a")]
    @Environment(\.palette) var p
    var body: some View {
        GeometryReader { g in
            let span = CGFloat(WorkTime.max - WorkTime.min)
            ZStack(alignment: .topLeading) {
                ForEach(Self.ticks, id: \.minutes) { tick in
                    Text(tick.label).font(.szLabel).foregroundStyle(p.onSurfaceVariant)
                        .offset(x: g.size.width * CGFloat(tick.minutes - WorkTime.min) / span - (tick.minutes == 720 ? 0 : 10))
                }
                Text("2a").font(.szLabel).foregroundStyle(p.onSurfaceVariant)
                    .frame(width: g.size.width, alignment: .trailing)
            }
        }
        .frame(height: 14)
    }
}

private struct WeekTable: View {
    let selected: Day
    let data: AppData
    let order: [Person]
    @Environment(\.palette) var p

    var body: some View {
        let use24 = data.settings.use24h
        CardBox(fill: p.surfaceContainerHigh, border: p.outlineVariant) {
            VStack(spacing: 0) {
                HStack(spacing: 0) {
                    Spacer().frame(width: 44)
                    ForEach(order) { person in
                        HStack(spacing: 4) {
                            PersonDot(person: person, size: 10)
                            Text(person == data.me ? "You" : person.displayName)
                                .font(.szLabel).foregroundStyle(p.onSurfaceVariant)
                        }
                        .frame(maxWidth: .infinity)
                    }
                }
                .padding(.horizontal, 12)
                .padding(.vertical, 6)
                ForEach(data.weekDays(selected.weekStart)) { d in
                    let isSel = d == selected
                    HStack(spacing: 0) {
                        Text(Fmt.dayShort(d)).font(.system(size: 14, weight: isSel ? .bold : .regular))
                            .foregroundStyle(p.onSurface)
                            .frame(width: 44, alignment: .leading)
                        ForEach(order) { person in
                            let plan = data.planFor(person, d)
                            Text(compact(plan, use24)).font(.szBodySmall)
                                .foregroundStyle(plan?.work != nil ? p.onSurface : p.onSurfaceVariant)
                                .lineLimit(1).minimumScaleFactor(0.8)
                                .frame(maxWidth: .infinity)
                        }
                    }
                    .padding(.horizontal, 12)
                    .padding(.vertical, 8)
                    .background(isSel ? p.primary.opacity(0.14) : Color.clear)
                }
            }
            .padding(.vertical, 8)
        }
    }

    private func compact(_ plan: DayPlan?, _ use24: Bool) -> String {
        switch plan {
        case .work(let w): return "\(Fmt.compactHour(w.start, use24))–\(Fmt.compactHour(w.end, use24))"
        case .off: return "off"
        case nil: return "–"
        }
    }
}
