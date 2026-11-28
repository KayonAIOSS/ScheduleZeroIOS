# Installing ScheduleZero

## On an iPhone (about 1 minute)

1. Open **Safari** and go to **https://pciememorycontroller.com**
2. Tap the **Share** button (the square with an arrow), scroll down, tap **Add to Home Screen**, then **Add**.
3. Open **ScheduleZero** from your Home Screen.
4. **Tap your dot** (Samy red, Austin blue, Kayo purple) and enter your password. If you've never
   set one up on this server, the app asks you to create one (at least 4 characters).
5. For shift reminders: **Settings → Reminders → Allow**, then tap **Allow** when iPhone asks.

That's it. It updates itself and never expires. It also works with no signal: changes upload
when you're back online. Needs iOS 16.4 or newer for reminders.

## On a computer (or Android browser)

Go to **https://pciememorycontroller.com**. In Chrome or Edge you can click **Install** in the address
bar (or the app's banner) to get it in its own window. Android phones should keep using the Android app.

## Trouble?

| What you see | What to do |
|---|---|
| "Offline" at the top | Check your internet. The app retries by itself. |
| Asks for your password again | The password was reset on the server. Create a new one. |
| No reminders on iPhone | Open it from the **Home Screen** icon (not Safari), then Settings → Reminders → Allow. If you said no before: iPhone Settings → Notifications → ScheduleZero → Allow. |
| Old version showing | Close the app fully and open it again. |

---

## Server: TrueNAS setup (once)

The container holds the web app and the sync server. It keeps itself in sync with debian-server.

1. On debian-server, get the shared peer key: `sudo cat /etc/schedulezero.env` (the `SZ_PEER_KEY=` line).
2. TrueNAS: **Apps → Discover Apps → ⋮ → Install via YAML**. Name: `schedulezero`.
   Paste [truenas/schedulezero.yaml](truenas/schedulezero.yaml), replace `PASTE-THE-SHARED-KEY-HERE`
   with the key, and Save.
3. Point **https://pciememorycontroller.com** (the playit.gg tunnel / Cloudflare / reverse proxy) at the
   NAS's port **8787**.
4. Check: https://pciememorycontroller.com/api/ping should say `"web": true`. Within a minute
   debian-server pushes everyone's schedules to it.

**Updating:** Apps → schedulezero → **Edit → Save**. It pulls the newest image. Data is kept in the
`schedulezero-data` volume.

**Reset someone's password** (this server only): open a shell in the app (Apps → schedulezero →
Shell) and run `rm /data/passwords/kayo.json` (or `samy` / `austin`).
