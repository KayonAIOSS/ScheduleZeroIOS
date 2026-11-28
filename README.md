# ScheduleZero Web

*Schedule zero vibe coded work application, web version.*

The ScheduleZero app for iPhones, computers and any browser: **Rust compiled to WebAssembly**, with
the same features as the Android app. It comes in one container together with the sync server, for
the TrueNAS at **https://pciememorycontroller.com**.

- **Phones:** layout tuned for iPhone, which installs it with Safari → Share → Add to Home Screen.
  It opens full screen with no App Store, no SideStore and no 7-day expiry. Android keeps its own app.
- **Desktop:** a separate layout with a sidebar, a 7-column week grid and dialogs instead of sheets.
- **Sync:** the container runs the same server as debian-server. The two servers copy everyone's
  schedules to each other every 20 seconds using the shared peer key, so the web app, the Android
  app (main or backup server) and every device agree.
- **Reminders:** shift reminders, snooze and the Sunday check-in use web push from the server. On
  iPhone they need iOS 16.4+ and the Home Screen install.

Installing: [INSTALL.md](INSTALL.md).

## Layout

| Folder | What |
|---|---|
| `app/` | The web app. `src/` Rust: `time` (dates, Android's `Fmt`), `model`, `store` (local data + unsent-edit queue), `sync` (long-poll, password flow), `push` (reminders), `ui/` (screens as HTML), `dom` (rendering), `actions` (taps). `static/` service worker, small JS glue, icons, manifest. `style.css` uses the Android colours. |
| `server/` | The sync server (Python standard library). Same file as `ScheduleZero/server` next to the Android app. It serves the web app when a `web/` folder is present, and sends web pushes. |
| `truenas/` | `schedulezero.yaml`: paste into TrueNAS → Apps → Install via YAML. |
| `ci/` | End-to-end check: drives the app in WebKit (iPhone) and Chromium (desktop) against the real server and saves screenshots. |
| `Dockerfile` | Builds the app with Trunk, then a small Python image with app + server. |

## Building

GitHub Actions (`.github/workflows/build.yml`) does everything on each push to `main`:

1. Server tests (`server/test_server.py`), Rust unit tests (`cargo test` in `app/`).
2. `trunk build --release`, which produces `app/dist`.
3. `ci/screenshots.py`, which signs in, edits a day and checks the edit reached the server. Screenshots
   and logs go to the public **ci-output** branch.
4. Builds and pushes `ghcr.io/kayonaioss/schedulezeroweb:latest`.

Locally: `cd app && trunk serve` (needs Rust with the `wasm32-unknown-unknown` target and Trunk), or
`python3 server/schedulezero_server.py --web app/dist` after a build.

## Keeping Android and web in step

Both apps follow `server/PROTOCOL.md`. When one app gets a feature or fix, it should be applied to
the other where it makes sense. Platform-only differences:

- The web app talks only to the server that served it (browsers block an https page from calling the
  plain-http Tailscale server). Server-to-server sync covers the rest, so it has no server fields in Settings.
- Reminders are server-sent web pushes instead of local alarms, and iPhone has no Snooze button.
  Android and desktop Chrome do.
- No "network access is off" or "exact alarms" rows; the web has no such switches.
