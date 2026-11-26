# ScheduleZero sync protocol (v1)

Plain HTTP + JSON, so the Android app, a future iOS app, or anything else can talk to the same server.
Traffic runs over Tailscale (WireGuard-encrypted), so the app uses plain `http://`.

Default servers: main `http://100.93.228.123:8787` (the `debian-server` Tailscale node), backup
`https://pciememorycontroller.com`. Clients try the main server first and fall back to the backup.
Each server has its own passwords/tokens; servers sync schedules with each other (see below).

## People

Exactly three accounts. IDs are lowercase and fixed:

| id       | name   | dot color |
|----------|--------|-----------|
| `samy`   | Samy   | red `#E53935` |
| `austin` | Austin | blue `#1E88E5` |
| `kayo`   | Kayo   | purple `#9C4DFF` |

One person can use any number of devices. Each install generates a random device ID and sends it in
`X-Device-Id` (plus optional `X-Device-Name`, `X-Platform`: `android` / `ios`).

## Day entries

Keyed by the **work day** as `YYYY-MM-DD`. Times are minutes after midnight of that work day, in
15-minute steps from 720 (12:00 PM) to 1560 (2:00 AM the next morning). A shift ending after
midnight still belongs to the day it started.

```json
{"type": "work", "start": 1005, "end": 1440, "updated": 1795725900000}
{"type": "off",  "updated": 1795725900000}
{"type": "none", "updated": 1795725900000}
```

`none` is a deletion marker ("not entered"). `updated` is the editing device's clock in Unix ms.
The server keeps whichever version of a day has the highest `updated` (last write wins), so devices
can edit offline and upload later.

Reminders, theme and other settings are per-device and never synced.

## Endpoints

| Method | Path | Auth | Purpose |
|---|---|---|---|
| GET  | `/api/ping` | – | Health check, people list. |
| GET  | `/api/auth/status?person=kayo` | – | `{"person":"kayo","hasPassword":true}` |
| POST | `/api/auth/setup` | – | First-time password. Body `{"person","password","deviceId","deviceName","platform"}` → `{"token"}`. `409` if a password already exists. Min length 4. |
| POST | `/api/auth/login` | – | Same body → `{"token"}`. `401` wrong password, `404` no password set (go to setup). |
| GET  | `/api/state?since=REV&wait=25` | Bearer | Long-poll. Returns immediately with the full state if the server's `rev` > `since`, otherwise waits up to `wait` seconds (max 25) and returns `{"rev":N,"unchanged":true}`. Use `since=-1&wait=0` for a one-shot fetch. |
| POST | `/api/schedule` | Bearer | Upload your own days: `{"days":{"2026-11-25":{...entry...}}}` → `{"applied":n,"rev":N}`. You can only write the person your token belongs to. |
| GET  | `/api/devices` | Bearer | Your own devices and when they were last seen. |

Auth header: `Authorization: Bearer <token>`. Tokens look like `kayo.<random>`.

Full state:

```json
{
  "rev": 42,
  "serverTime": 1795725900000,
  "you": "kayo",
  "people": {
    "samy":   {"2026-11-25": {"type": "work", "start": 960, "end": 1440, "updated": 1795725900000}},
    "austin": {},
    "kayo":   {"2026-11-25": {"type": "off", "updated": 1795725900000}}
  }
}
```

## Server-to-server sync

Both endpoints need the header `X-Peer-Key: <SZ_PEER_KEY>` (identical on all servers); otherwise `403`.

| Method | Path | Purpose |
|---|---|---|
| GET  | `/api/peer/state` | Full state, same shape as `/api/state`. |
| POST | `/api/peer/merge` | `{"people":{...}}`. Merged last-write-wins per day. |

A server with `SZ_PEERS` set does GET + merge + POST every `SZ_PEER_INTERVAL` seconds. Only one side
needs to be configured. Passwords are never exchanged.

## Client loop (what Android does; iOS should do the same)

1. Pick a person (no free-text usernames). `GET /api/auth/status` → show "create password" or "enter password".
2. Store the token. On any `401`: drop the token, call `/api/auth/status` again and show the right password screen.
   (Deleting `passwords/<person>.json` on the server voids every token for that person but keeps their schedule.)
3. While the app is open: keep a long-poll running (`since` = last `rev` seen). After every local edit,
   immediately `POST /api/schedule` with the changed days; if offline, keep them marked "dirty" and retry later.
4. When merging the server's copy of your own days, skip days you still have unsent edits for, and only
   take a day if its `updated` is newer than yours.
5. Network errors are normal (phone asleep, no signal). Back off quietly and retry; never alert the user.
