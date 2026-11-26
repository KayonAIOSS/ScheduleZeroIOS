import AppIntents
import Foundation

// Shortcuts actions for the EXPERIMENTAL auto-refresh. The user's "ScheduleZero Refresh" shortcut
// starts with "Does ScheduleZero Need a Refresh?" and stops when the answer is No, so the daily
// automation only touches the VPNs when the 7-day signature is close to running out.

struct NeedsRefreshIntent: AppIntent {
    static var title: LocalizedStringResource = "Does ScheduleZero Need a Refresh?"
    static var description = IntentDescription(
        "Yes when ScheduleZero's 7-day signature runs out within the given number of hours and Auto-refresh is on in ScheduleZero's settings."
    )
    static var openAppWhenRun = false

    @Parameter(title: "Hours left", default: 48)
    var hours: Int

    static var parameterSummary: some ParameterSummary {
        Summary("Does ScheduleZero expire within \(\.$hours) hours?")
    }

    func perform() async throws -> some IntentResult & ReturnsValue<Bool> & ProvidesDialog {
        SigningMonitor.recordAutomationCheck()
        if SigningMonitor.consumeForce() {
            return .result(value: true, dialog: "Test run: refreshing now.")
        }
        let enabled = await MainActor.run { ScheduleStore.shared.data.settings.autoRefreshEnabled }
        guard enabled else {
            return .result(value: false, dialog: "Auto-refresh is off in ScheduleZero's settings.")
        }
        guard let info = SigningInfo.load() else {
            return .result(value: false, dialog: "Can't read this install's expiry date.")
        }
        let left = info.secondsLeft()
        let needs = left <= TimeInterval(max(1, hours)) * 3600
        let text = needs
            ? "Expires in \(Fmt.hoursLeft(left)). Refreshing."
            : "Expires in \(Fmt.hoursLeft(left)). No refresh needed yet."
        return .result(value: needs, dialog: IntentDialog(stringLiteral: text))
    }
}

struct HoursUntilExpiryIntent: AppIntent {
    static var title: LocalizedStringResource = "Hours Until ScheduleZero Expires"
    static var description = IntentDescription("Whole hours left on this install's 7-day signature (-1 if unknown).")
    static var openAppWhenRun = false

    func perform() async throws -> some IntentResult & ReturnsValue<Int> {
        let info = SigningInfo.load()
        return .result(value: info.map { Int($0.secondsLeft() / 3600) } ?? -1)
    }
}

struct ScheduleZeroShortcuts: AppShortcutsProvider {
    static var appShortcuts: [AppShortcut] {
        AppShortcut(
            intent: NeedsRefreshIntent(),
            phrases: ["Does \(.applicationName) need a refresh"],
            shortTitle: "Needs Refresh?",
            systemImageName: "arrow.clockwise"
        )
    }
}
