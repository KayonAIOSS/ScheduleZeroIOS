import SwiftUI

/// Walks through building the "ScheduleZero Refresh" shortcut and the daily automation.
/// EXPERIMENTAL: depends on SideStore, LocalDevVPN and Shortcuts behaving, which can change.
struct AutoRefreshGuideView: View {
    @EnvironmentObject var signing: SigningMonitor
    @Environment(\.palette) var p

    private struct Step: Identifiable {
        let id: Int
        let title: String
        let detail: String
    }

    private let makeShortcut: [Step] = [
        Step(id: 1, title: "Create a new shortcut",
             detail: "Open the Shortcuts app, tap +, and name it exactly \"\(SigningMonitor.shortcutName)\"."),
        Step(id: 2, title: "Add “Does ScheduleZero Need a Refresh?”",
             detail: "Tap Add Action, search “ScheduleZero”, and pick it. Leave it at 48 hours."),
        Step(id: 3, title: "Stop when no refresh is needed",
             detail: "Add “If”. Set the condition to “is false”. Then add “Stop This Shortcut” inside the If (above Otherwise). Leave Otherwise empty."),
        Step(id: 4, title: "Turn on SideStore's VPN",
             detail: "After End If, add LocalDevVPN's “start” action (search “LocalDevVPN”). If there isn't one, add “Set VPN”, choose Connect, and pick LocalDevVPN. This briefly replaces Tailscale."),
        Step(id: 5, title: "Open SideStore",
             detail: "Add “Open URLs” with sidestore:// and then “Wait” for 5 seconds."),
        Step(id: 6, title: "Refresh",
             detail: "Add SideStore's “Refresh All Apps” action, then “Wait” for 10 seconds."),
        Step(id: 7, title: "Turn SideStore's VPN off",
             detail: "Add LocalDevVPN's “stop” action, or “Set VPN” → Disconnect."),
        Step(id: 8, title: "Bring Tailscale back",
             detail: "Add “Set VPN”, choose Connect, and pick Tailscale. Without this, syncing to the main server stops until Tailscale is turned back on."),
    ]

    private let automate: [Step] = [
        Step(id: 9, title: "Run it every day",
             detail: "In Shortcuts, go to Automation → + → Time of Day. Pick a time your phone is usually unlocked and nearby (for example 12:00 PM), Daily, then “Run Immediately”, and choose \(SigningMonitor.shortcutName)."),
        Step(id: 10, title: "Optional: also when you open ScheduleZero",
             detail: "Add a second automation: App → ScheduleZero → Is Opened → Run Immediately → the same shortcut. Most days it does nothing and finishes instantly."),
        Step(id: 11, title: "Test it",
             detail: "Tap “Test it now” below. SideStore should open and refresh, then Tailscale should reconnect. Afterwards, “last refreshed” in Settings should show today."),
    ]

    init() {}

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 12) {
                CardBox(fill: p.surfaceContainer, border: p.warning.opacity(0.8)) {
                    VStack(alignment: .leading, spacing: 6) {
                        ExperimentalTag()
                        Text("This may not work on every phone")
                            .font(.szTitle).foregroundStyle(p.onSurface)
                        Text("It relies on SideStore, LocalDevVPN and Shortcuts, which can change with updates. iOS only allows one VPN at a time, so Tailscale is paused for about a minute while it refreshes. The expiry warning keeps working either way, so you'll still be told if it didn't refresh.")
                            .font(.szBody).foregroundStyle(p.onSurfaceVariant)
                            .fixedSize(horizontal: false, vertical: true)
                    }
                    .padding(16)
                }

                Text("Before you start: SideStore must already be able to refresh by hand (Refresh All works today).")
                    .font(.szBody).foregroundStyle(p.onSurfaceVariant)
                    .fixedSize(horizontal: false, vertical: true)

                SectionHeader("1. Make the shortcut")
                ForEach(makeShortcut) { stepCard($0) }
                Button("Open Shortcuts") { SigningMonitor.openShortcutsApp() }.buttonStyle(.szTonal)

                SectionHeader("2. Run it automatically")
                ForEach(automate) { stepCard($0) }

                HStack(spacing: 8) {
                    Button("Test it now") { SigningMonitor.runRefreshShortcut(force: true) }.buttonStyle(.szFilled)
                    Button("Open SideStore") { SigningMonitor.openSideStore() }.buttonStyle(.szOutlined)
                }
                .padding(.top, 4)

                Text("Tip: in each automation, turn off “Notify When Run” if the banner bothers you. If anything goes wrong, open SideStore and tap Refresh All by hand.")
                    .font(.szBodySmall).foregroundStyle(p.onSurfaceVariant)
                    .fixedSize(horizontal: false, vertical: true)
                    .padding(.top, 8)
            }
            .padding(.horizontal, 16)
            .padding(.bottom, 32)
        }
        .background(p.background.ignoresSafeArea())
        .navigationTitle("Auto-refresh setup")
        .navigationBarTitleDisplayMode(.inline)
    }

    private func stepCard(_ step: Step) -> some View {
        CardBox(border: p.outlineVariant) {
            HStack(alignment: .top, spacing: 12) {
                Text("\(step.id)")
                    .font(.system(size: 14, weight: .bold))
                    .foregroundStyle(p.onPrimaryContainer)
                    .frame(width: 28, height: 28)
                    .background(p.primaryContainer, in: Circle())
                VStack(alignment: .leading, spacing: 4) {
                    Text(step.title).font(.szTitle).foregroundStyle(p.onSurface)
                    Text(step.detail).font(.szBody).foregroundStyle(p.onSurfaceVariant)
                        .fixedSize(horizontal: false, vertical: true)
                        .textSelection(.enabled)
                }
            }
            .padding(14)
        }
    }
}
