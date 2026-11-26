# ScheduleZero for iPhone (SwiftUI)
n*Schedule zero vibe coded work application IOS port.*

Native port of the Android app: same look, sync protocol (`server/PROTOCOL.md`), servers,
failover, dots and comparison, password flow and local reminders. iOS 17+, iPhone only.

iPhone-only additions: the **App expiry** settings section (warning banner and notifications,
0–48 h in 1 h steps, default 48) and the **experimental** Shortcuts auto-refresh (off by default).

## Building (no Mac needed)

GitHub Actions builds everything on a macOS runner: `.github/workflows/ios.yml`.
- `xcodegen generate` makes `ScheduleZero.xcodeproj` from `project.yml` (the project file is not committed).
- Unit tests run on an iPhone simulator, including `LiveServerTests`, a full round trip against
  `server/schedulezero_server.py` (a copy of the real server) started on the runner.
- `ci/screenshots.sh` launches the app in demo mode (`-SZDemo YES -SZScreen … -SZTheme …`, fake
  data, no network) and saves PNGs of every screen in both themes → **screenshots** artifact.
- The unsigned `ScheduleZero.ipa` is the **ScheduleZero-ipa** artifact. Pushing a tag `v*` also
  attaches it to a GitHub Release.

SideStore / AltStore sign the .ipa with each person's free Apple ID on install.

## Layout

| Folder | What |
|---|---|
| `ScheduleZero/Data` | `Day` (date without time zone), models, formatting, `ScheduleStore` (local data + unsent-edit queue) |
| `ScheduleZero/Sync` | `SyncAPI` (HTTP/JSON), `SyncManager` (long-poll, main→backup failover, per-server tokens) |
| `ScheduleZero/Notify` | Local notifications: shift reminders (snooze), Sunday check-in, expiry warnings |
| `ScheduleZero/Signing` | Reads `embedded.mobileprovision` for the expiry date; App Intents for the Shortcuts automation |
| `ScheduleZero/UI` | SwiftUI screens, Material-style theme with the Android colors |

## Differences from Android that iOS forces

- Live long-polling runs only while the app is open. In the background, iOS runs a refresh when it
  chooses (`BGAppRefreshTask`). Reminders are local, so they never depend on it.
- iOS has no per-app "network off" switch like Android's; Settings instead warns when **mobile data**
  is turned off for the app (Wi-Fi still works).
- Plain `http://` to the Tailscale IP is allowed in `Info.plist` (ATS exception).
