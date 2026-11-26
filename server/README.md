# Server copy (reference + CI only)

`schedulezero_server.py` and `PROTOCOL.md` are copies of the real sync server, which lives with the
Android app in the main ScheduleZero folder and runs on debian-server. CI starts this copy so the
iPhone app's live sync test talks to the exact same server code. When the server changes, copy the
two files here again.
