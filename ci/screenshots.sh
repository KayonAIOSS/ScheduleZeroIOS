#!/bin/sh
# Launches the simulator build in demo mode (fake data, no network) on each screen and saves a PNG.
# Also saves the app's console output and any crash reports next to the screenshots.
# Usage: screenshots.sh <simulator-udid> <derived-data-path> <output-dir>
set -e
UDID="$1"; DD="$2"; OUT="$3"
APP="$DD/Build/Products/Debug-iphonesimulator/ScheduleZero.app"
BUNDLE=com.schedulezero.app
mkdir -p "$OUT/logs"
xcrun simctl boot "$UDID" 2>/dev/null || true
xcrun simctl bootstatus "$UDID" -b
xcrun simctl status_bar "$UDID" override --time "9:41" --batteryState charged --batteryLevel 100 || true
xcrun simctl install "$UDID" "$APP"
xcrun simctl privacy "$UDID" grant notifications "$BUNDLE" 2>/dev/null || true
START=$(date "+%Y-%m-%d %H:%M:%S")

shot() { # name screen theme
  xcrun simctl terminate "$UDID" "$BUNDLE" 2>/dev/null || true
  xcrun simctl launch --stdout="$OUT/logs/$1.out" --stderr="$OUT/logs/$1.err" \
    "$UDID" "$BUNDLE" -SZDemo YES -SZScreen "$2" -SZTheme "$3" > "$OUT/logs/$1.launch" 2>&1 || true
  sleep 5
  xcrun simctl io "$UDID" screenshot "$OUT/$1.png" >/dev/null
  if xcrun simctl spawn "$UDID" launchctl list | grep -q "$BUNDLE"; then
    echo "saved $1.png (app running)"
  else
    echo "saved $1.png (APP NOT RUNNING)" | tee -a "$OUT/logs/not-running.txt"
  fi
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

# Crash reports and the app's own log messages (fatalError text, SwiftUI runtime warnings).
cp ~/Library/Logs/DiagnosticReports/ScheduleZero* "$OUT/logs/" 2>/dev/null || true
xcrun simctl spawn "$UDID" log show --style compact --start "$START" \
  --predicate 'process == "ScheduleZero"' 2>/dev/null | tail -n 400 > "$OUT/logs/app-log.txt" || true
ls -la "$OUT/logs"
