#!/usr/bin/env python3
"""End-to-end check + screenshots, run by CI.

Starts the real server with the built web app, seeds Samy's and Austin's weeks through the API,
then drives the app like a person would: pick Kayo's dot, sign in, look around, edit a day.
iPhone layout in WebKit (Safari's engine) and desktop layout in Chromium, dark and light.
Fails if the page throws an error. Usage: screenshots.py <dist-dir> <out-dir>
"""

import json
import os
import subprocess
import sys
import tempfile
import time
import urllib.request
from datetime import date, datetime, timedelta
from zoneinfo import ZoneInfo

from playwright.sync_api import sync_playwright

DIST, OUT = sys.argv[1], sys.argv[2]
PORT = 8799
BASE = f"http://127.0.0.1:{PORT}"
TZ = "America/Chicago"
os.makedirs(os.path.join(OUT, "logs"), exist_ok=True)


def api(method, path, body=None, token=None):
    headers = {"Content-Type": "application/json", "X-Device-Id": "ci-seed"}
    if token:
        headers["Authorization"] = "Bearer " + token
    data = json.dumps(body).encode() if body is not None else None
    with urllib.request.urlopen(urllib.request.Request(BASE + path, data=data, method=method, headers=headers), timeout=10) as r:
        return json.loads(r.read())


def seed(today: date):
    monday = today - timedelta(days=today.weekday())
    w = lambda s, e: {"type": "work", "start": s, "end": e, "updated": 1}
    off = {"type": "off", "updated": 1}
    weeks = {
        "samy": {0: w(900, 1380), 2: w(1020, 1440), 4: off, "today": w(1020, 1500)},
        "austin": {0: w(960, 1440), 3: w(1080, 1560), "today": w(840, 1320)},
        "kayo": {0: w(960, 1440), 1: off, 2: w(1020, 1530), 3: w(960, 1440), 4: w(1080, 1560), 5: off, "today": w(960, 1470)},
    }
    for person, days in weeks.items():
        token = api("POST", "/api/auth/setup", {"person": person, "password": "ci-pass", "deviceId": "ci-seed"})["token"]
        payload = {}
        for k, v in days.items():
            d = today if k == "today" else monday + timedelta(days=k)
            payload[d.isoformat()] = v
        api("POST", "/api/schedule", {"days": payload}, token)


def main():
    data_dir = tempfile.mkdtemp()
    server = subprocess.Popen(
        [sys.executable, "server/schedulezero_server.py", "--port", str(PORT), "--data", data_dir, "--web", DIST],
        stdout=open(os.path.join(OUT, "logs", "server.log"), "w"), stderr=subprocess.STDOUT,
    )
    try:
        for _ in range(50):
            try:
                api("GET", "/api/ping")
                break
            except Exception:
                time.sleep(0.2)
        now = datetime.now(ZoneInfo(TZ)).replace(hour=14, minute=5, second=0, microsecond=0)
        seed(now.date())
        errors = []
        with sync_playwright() as p:
            iphone = p.devices.get("iPhone 15") or p.devices["iPhone 13"]
            runs = [
                ("iphone", p.webkit, dict(iphone)),
                ("desktop", p.chromium, {"viewport": {"width": 1440, "height": 900}}),
            ]
            for name, engine, opts in runs:
                browser = engine.launch()
                ctx = browser.new_context(timezone_id=TZ, locale="en-US", color_scheme="dark", **opts)
                page = ctx.new_page()
                page.clock.set_fixed_time(now)
                log = open(os.path.join(OUT, "logs", f"console-{name}.txt"), "w")
                page.on("console", lambda m, log=log: log.write(f"[{m.type}] {m.text}\n"))
                page.on("pageerror", lambda e, name=name: errors.append(f"{name}: {e}"))
                run_flow(page, name)
                log.close()
                browser.close()
        if errors:
            print("PAGE ERRORS:\n" + "\n".join(errors))
            sys.exit(1)
        print("screenshots ok")
    finally:
        server.terminate()


def shot(page, name):
    page.wait_for_timeout(450)
    page.screenshot(path=os.path.join(OUT, name + ".png"), full_page=False)
    print("saved", name)


def run_flow(page, n):
    page.goto(BASE + "/")
    page.wait_for_selector("[data-act=pick]", timeout=20000)
    shot(page, f"{n}-01-picker")
    page.click("[data-act=pick][data-arg=kayo]")
    page.wait_for_selector("#pw", timeout=15000)
    shot(page, f"{n}-02-password")
    page.fill("#pw", "wrong-pass")
    page.click("[data-act=pw-submit]")
    page.wait_for_selector(".error", timeout=15000)
    page.fill("#pw", "ci-pass")
    page.click("[data-act=pw-submit]")
    page.wait_for_selector(".hero", timeout=15000)
    page.wait_for_timeout(1500)  # first sync round
    shot(page, f"{n}-03-home")
    if n == "iphone":
        page.evaluate("window.scrollBy(0, 700)")
        shot(page, f"{n}-04-home-scrolled")
        page.evaluate("window.scrollBy(0, -2000)")
    page.click(".hero [data-act=compare]") if page.locator(".hero [data-act=compare]").count() else None
    shot(page, f"{n}-05-compare")
    page.keyboard.press("Escape")
    page.click("[data-act=nav][data-arg^='week:']" if n == "iphone" else "[data-act=side][data-arg^='week:']")
    page.wait_for_timeout(300)
    shot(page, f"{n}-06-week")
    if n == "iphone":
        page.click("[data-act=menu]")
        shot(page, f"{n}-07-week-menu")
        page.click(".menu-scrim")
    page.locator(".day-card, .tile-day").nth(1).click()
    page.wait_for_selector(".overlay", timeout=5000)
    shot(page, f"{n}-08-editor")
    page.click("[data-act=ed-mode][data-arg='0']")
    page.click("[data-act=ed-pick][data-arg=start]")
    shot(page, f"{n}-09-time-picker")
    page.click("[data-act=ed-slot][data-arg='1050']")
    page.click("[data-act=ed-save]")
    page.wait_for_timeout(800)
    shot(page, f"{n}-10-week-after-edit")
    page.click("[data-act=back]" if n == "iphone" else "[data-act=side][data-arg=home]")
    page.click("[data-act=nav][data-arg=settings]" if n == "iphone" else "[data-act=side][data-arg=settings]")
    page.wait_for_timeout(300)
    shot(page, f"{n}-11-settings")
    if n == "iphone":
        page.evaluate("window.scrollBy(0, 900)")
        shot(page, f"{n}-12-settings-scrolled")
        page.evaluate("window.scrollBy(0, -3000)")
    page.click("[data-act=s-dark]")
    page.wait_for_timeout(300)
    shot(page, f"{n}-13-settings-light")
    page.click("[data-act=back]" if n == "iphone" else "[data-act=side][data-arg=home]")
    page.wait_for_timeout(300)
    shot(page, f"{n}-14-home-light")
    # The edit made above must have reached the server.
    state = api("POST", "/api/auth/login", {"person": "kayo", "password": "ci-pass", "deviceId": "ci-check"})
    st = api("GET", "/api/state?since=-1&wait=0", token=state["token"])
    starts = [d.get("start") for d in st["people"]["kayo"].values()]
    if 1050 not in starts:
        raise SystemExit(f"{n}: edited shift (start 1050) never reached the server: {starts}")
    print(n, "edit synced to server")


if __name__ == "__main__":
    main()
