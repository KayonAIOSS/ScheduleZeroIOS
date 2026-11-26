# Installing ScheduleZero on an iPhone

ScheduleZero isn't on the App Store. It's installed with **SideStore** (or AltStore) using a free
Apple ID. Free installs last **7 days** and then need a one-minute refresh. The app warns you
before that happens.

You need:
- **Tailscale** on the iPhone, signed in (you already have this). It's how the app reaches the main server.
- **SideStore** (or AltStore) on the iPhone. If you don't have one yet, ask Kayo. The first setup needs a computer once.
  SideStore's official guide: https://docs.sidestore.io

## 1. Get the app file

1. On the iPhone, open this link in **Safari**: https://github.com/KayonAIOSS/ScheduleZeroIOS/releases/latest/download/ScheduleZero.ipa
   It downloads a file called `ScheduleZero.ipa` (always the newest version).
2. Tap the download, then **Share** → **SideStore** (or open SideStore, go to **My Apps**, tap **+**, and pick the file).
3. Wait for SideStore to finish installing. ScheduleZero appears on the home screen.

If iPhone says the developer isn't trusted or Developer Mode is needed, follow the message
(Settings → Privacy & Security → Developer Mode), or ask Kayo.

## 2. First launch

1. Open ScheduleZero and **tap your dot** (Samy red, Austin blue, Kayo purple).
2. **Create a password** the first time (at least 4 characters). On your other devices you'll enter the same one.
3. Allow notifications if you want shift reminders.

Your schedule syncs with everyone's phones. You can still use the app with no signal: changes upload later on their own.

## 3. Every week: refresh (about 1 minute)

Do this any day before the 7 days run out. **Sunday** is an easy habit.

1. Open **SideStore**. If it asks to turn on its VPN, allow it (Tailscale pauses for a moment).
2. Tap **Refresh All** and wait until it finishes.
3. Open **Tailscale** and make sure it's connected again.

ScheduleZero tells you when it's getting close: there's a banner on the home screen and
notifications up to 48 hours before. Change how early in **Settings → App expiry → Warn me** (off to 48 hours).

**If it expires anyway:** nothing is lost. Do the same refresh and it opens again with all your data.

### Optional: automatic refresh (experimental)

**Settings → App expiry → Auto-refresh** walks you through a Shortcuts automation that refreshes
by itself. It's marked experimental because it depends on SideStore and Shortcuts behaving, so
keep an eye on the expiry date until you've seen it work once.

## Updating to a new version

Install the new `ScheduleZero.ipa` the same way as in step 1. It replaces the old one and keeps your data.

## Trouble?

| What you see | What to do |
|---|---|
| "Offline" at the top | Turn Tailscale on. The app retries by itself. |
| Asks for your password again | The password was reset on the server. Create a new one. |
| App won't open | It expired. Open SideStore → Refresh All. |
| "Mobile data is off" in Settings | iPhone Settings → ScheduleZero → turn on Mobile Data. |
| No reminders | iPhone Settings → Notifications → ScheduleZero → Allow. |
