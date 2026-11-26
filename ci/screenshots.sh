#!/bin/sh
# Launches the simulator build in demo mode (fake data, no network) on each screen and saves a PNG.
# Usage: screenshots.sh <simulator-udid> <derived-data-path> <output-dir>
set -e
UDID="$1"; DD="$2"; OUT="$3"
APP="$DD/Build/Products/Debug-iphonesimulator/ScheduleZero.app"
BUNDLE=com.schedulezero.app
mkdir -p "$OUT"
xcrun simctl boot "$UDID" 2>/dev/null || true
xcrun simctl bootstatus "$UDID" -b
xcrun simctl status_bar "$UDID" override --time "9:41" --batteryState charged --batteryLevel 100 || true
xcrun simctl install "$UDID" "$APP"
xcrun simctl privacy "$UDID" grant notifications "$BUNDLE" 2>/dev/null || true

shot() { # name screen theme
  xcrun simctl terminate "$UDID" "$BUNDLE" 2>/dev/null || true
  xcrun simctl launch "$UDID" "$BUNDLE" -SZDemo YES -SZScreen "$2" -SZTheme "$3" >/dev/null
  sleep 4
  xcrun simctl io "$UDID" screenshot "$OUT/$1.png" >/dev/null
  echo "saved $1.png"
}

for theme in dark light; do
  shot "$theme-1-home" home "$theme"
  shot "$theme-2-week" week "$theme"
  shot "$theme-3-editor" editor "$theme"
  shot "$theme-4-compare" compare "$theme"
  shot "$theme-5-settings" settings "$theme"
done
shot "dark-6-signing" signing dark
shot "dark-7-autorefresh-guide" guide dark
shot "dark-8-picker" picker dark
shot "dark-9-password" password dark
shot "dark-10-expiring-home" expiring dark
