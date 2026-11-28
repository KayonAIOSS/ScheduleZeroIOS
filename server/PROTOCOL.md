# ScheduleZero sync protocol (v1)

Plain HTTP + JSON, so the Android app, the web app, or anything else can talk to the same server.
Traffic to the main server runs over Tailscale (WireGuard-encrypted), so the Android app uses plain
`http://` there. The web app (ScheduleZeroWeb) is served by a server itself and only talks to the
server it was loaded from.

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
`X-Device-Id` (plus optional `X-Device-Name`, `X-Platform`: `android` / `web`).

## Day entries

Keyed by the **work day** as `YYYY-MM-DD`. Times are minutes after midnight of that work day, in
15-minute steps from 720 (12:00 PM) to 1560 (2:00 AM the next morning). A shift ending after
midnight still belongs to the day it started.

```json
{"type": "work", "start": 1005, "end": 1440, "updated": 1795725900000}
{"type": "off",  "updated": 1795725900000}
{"type": "none", "updated": 1795725900000}
```

`none` is a deletion marker ("not entered"). `updated` is **when the edit was made**, in Unix ms on
the server's clock: devices add the offset they learn from `serverTime` (in `/api/state` and
`/api/schedule` responses) to their own clock, and remember it for offline edits. The server keeps
whichever version of a day has the highest `updated` (the most recent edit wins). An edit made
offline uploads later but keeps its original time, so it never overwrites a later edit from
another device. Uploads with `updated` more than 2 minutes in the server's future are clamped to
the server's current time, so a phone with a wrong clock can't win forever.

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

`/api/ping` also reports `version`, `web` (serves the web app) and `push` (web push available).

## Carpools and inbox

A carpool is a **request** ("can someone drive me?") or an **offer** ("I can drive you") for one day
and one direction, `to` work or `from` work. The two directions are independent. Everyone sees all
carpools in `/api/state` (`"carpools": [...]`); each device also gets its person's `"inbox"`
(newest first, up to 100 messages).

| Method | Path | Auth | Purpose |
|---|---|---|---|
| POST | `/api/carpool` | Bearer | `{"action":"create","kind":"request"\|"offer","day","dir":"to"\|"from","to":["samy",...],"note"}` |
| POST | `/api/carpool` | Bearer | `{"action":"respond","id","answer":"yes"\|"no"}`, only by a person it was sent to; can be changed later |
| POST | `/api/carpool` | Bearer | `{"action":"cancel","id"}`, only by the sender |
| POST | `/api/inbox/read` | Bearer | `{"ids":[...]}` or `{"all":true}` |

Carpool: `{"id","from","kind","day","dir","to":[...],"note","created","cancelled":ms|0,"responses":{"samy":{"answer":"yes","at":ms}}}`.
Each create, answer and cancel makes inbox messages for the other people involved:
`{"id","to","from","kind":"carpool-new|carpool-answer|carpool-cancel","carpool","title","body","created","read":ms|0}`.
Devices notify about new unread messages. Android does it on sync; the web app gets a web push from
the server with a "Mark as read" button. Confirm/deny is never on a notification, only in the app.
Servers merge carpools (answers per person, newest wins) and messages (read = latest) during peer sync.

## Web address prefix

The web container runs under `https://pciememorycontroller.com/ScheduleZero` (`SZ_BASE_PATH`). The server
accepts paths with or without that prefix, so it works whether or not the reverse proxy strips it.
API responses allow any origin (CORS), so the web app can use another server as its backup.

## Web push (web app reminders)

Android schedules its own alarms. A web page can't wake itself up, so the web app uploads the
reminders it wants and the server sends them as payload-less web pushes (VAPID, RFC 8292). The
service worker then fetches the text from the inbox. Each server has its own key pair
(`vapid.json` in the data directory) and its own subscriptions (`push.json`); none of this is synced.

| Method | Path | Auth | Purpose |
|---|---|---|---|
| GET  | `/api/push/key` | – | `{"publicKey"}` for `pushManager.subscribe`. |
| POST | `/api/push/subscribe` | Bearer | `{"deviceId","subscription":{endpoint,keys}}` → `{"key"}` (inbox key for the service worker). |
| POST | `/api/push/schedule` | Bearer | `{"deviceId","items":[...]}` replaces this device's reminders. |
| POST | `/api/push/unsubscribe` | Bearer | `{"deviceId"}` |
| POST | `/api/push/test` | Bearer | `{"deviceId"}` sends a test reminder now. |
| GET  | `/api/push/inbox?device=&key=` | inbox key | Pops one message: `{"message":{id,title,body,tag,snoozable}}` or `{"message":null}`. |
| POST | `/api/push/snooze` | inbox key | `{"deviceId","key","id"}` sends that reminder again in 15 minutes. |

Item: `{"id":"shift-2026-11-27","at":ms,"until":ms,"title","body","check":{...}}`. `check` keeps
stale reminders from firing, like Android's alarm receiver: `{"type":"shift","day","start"}` only
fires if that day is still work starting at `start`; `{"type":"weekly","week":"YYYY-MM-DD"}` only fires
if that Monday–Sunday week is still empty. Reminders more than 6 h late are dropped.

## Server-to-server sync

Both endpoints need the header `X-Peer-Key: <SZ_PEER_KEY>` (identical on all servers); otherwise `403`.

| Method | Path | Purpose |
|---|---|---|
| GET  | `/api/peer/state` | Full state, same shape as `/api/state`. |
| POST | `/api/peer/merge` | `{"people":{...}}`. Merged last-write-wins per day. |

A server with `SZ_PEERS` set does GET + merge + POST every `SZ_PEER_INTERVAL` seconds. Only one side
needs to be configured. Passwords are never exchanged.

## Client loop (what Android and the web app do)

1. Pick a person (no free-text usernames). `GET /api/auth/status` → show "create password" or "enter password".
2. Store the token. On any `401`: drop the token, call `/api/auth/status` again and show the right password screen.
   (Deleting `passwords/<person>.json` on the server voids every token for that person but keeps their schedule.)
3. While the app is open: keep a long-poll running (`since` = last `rev` seen). After every local edit,
   immediately `POST /api/schedule` with the changed days; if offline, keep them marked "dirty" and retry later.
4. When merging the server's copy of your own days, take a day only if its `updated` is newer than
   yours, and then also drop your unsent edit for that day (it's older, the server would reject it).
   `POST /api/schedule` returns `applied`; if it's less than the number of days sent, another device
   edited some of them later, so fetch the full state next (`since=-1`).
5. Network errors are normal (phone asleep, no signal). Back off quietly and retry; never alert the user.
