# Sync server

`schedulezero_server.py` is the ScheduleZero sync server (Python standard library only). It is the
**same file** as `ScheduleZero/server/schedulezero_server.py` next to the Android app, which runs on
debian-server. When one changes, copy it to the other.

- In the container (`Dockerfile`) it also serves the web app from `/app/web` and sends web pushes.
- On debian-server there is no `web/` folder, so it stays sync-only.

API: [PROTOCOL.md](PROTOCOL.md). Tests: `python3 test_server.py`.
