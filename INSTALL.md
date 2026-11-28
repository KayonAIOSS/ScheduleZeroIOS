# Installing ScheduleZero

## On an iPhone (about 1 minute)

1. Open **Safari** and go to **https://pciememorycontroller.com/ScheduleZero/**
2. Tap the **Share** button (the square with an arrow), scroll down, tap **Add to Home Screen**, then **Add**.
3. Open **ScheduleZero** from your Home Screen.
4. **Tap your dot** (Samy red, Austin blue, Kayo purple) and enter your password. If you've never
   set one up on this server, the app asks you to create one (at least 4 characters).
5. For shift reminders: **Settings → Reminders → Allow**, then tap **Allow** when iPhone asks.

That's it. It updates itself and never expires. It also works with no signal: changes upload
when you're back online. Needs iOS 16.4 or newer for reminders.

## On a computer (or Android browser)

Go to **https://pciememorycontroller.com/ScheduleZero/**. In Chrome or Edge you can click **Install** in the address
bar (or the app's banner) to get it in its own window. Android phones should keep using the Android app.

## Trouble?

| What you see | What to do |
|---|---|
| "Offline" at the top | Check your internet. The app retries by itself. |
| Asks for your password again | The password was reset on the server. Create a new one. |
| No reminders on iPhone | Open it from the **Home Screen** icon (not Safari), then Settings → Reminders → Allow. If you said no before: iPhone Settings → Notifications → ScheduleZero → Allow. |
| Old version showing | Close the app fully and open it again. |

---

## Server: TrueNAS setup (once, all in the TrueNAS web UI)

The container holds the web app and the sync server, and keeps itself in sync with debian-server.
No shell is needed.

1. **Apps → Discover Apps → ⋮ (top right) → Install via YAML.** Name: `schedulezero`.
2. Paste [truenas/schedulezero.yaml](truenas/schedulezero.yaml). Put the shared key in `SZ_PEER_KEY`,
   and use the same key on debian-server. Then **Save**. TrueNAS downloads the image and starts it.
   The app uses port 10119 on the NAS. If that is taken, change the left number in `"10119:8787"`.
3. Check it on the NAS itself: `http://<NAS-IP>:10119/ScheduleZero/api/ping` should show `"web": true`.
4. Make **https://pciememorycontroller.com/ScheduleZero** reach it: in whatever already serves
   pciememorycontroller.com (Nginx Proxy Manager, the playit.gg tunnel, Cloudflare…), send the path
   `/ScheduleZero` to `http://<NAS-IP>:10119`. Keeping or stripping the `/ScheduleZero` prefix both work.
5. Open https://pciememorycontroller.com/ScheduleZero/ and sign in.

**Updating:** Apps → schedulezero → **Edit → Save**. It pulls the newest image. Data stays in the
`schedulezero-data` volume.

**Reset someone's password** (this server only): Apps → schedulezero → Edit, set
`SZ_RESET_PASSWORDS` to `kayo` (or `samy` / `austin`), and Save. It happens once. To reset the same
person again later, change the text, e.g. `kayo 2`. Schedules are kept.
