#!/usr/bin/env python3
"""Tests for schedulezero_server.py. Run: python3 test_server.py   (standard library only)."""

import base64
import gzip
import json
import os
import shutil
import sys
import tempfile
import threading
import time
import unittest
from datetime import date, timedelta
from http.server import BaseHTTPRequestHandler, HTTPServer
from urllib.error import HTTPError
from urllib.request import Request, urlopen

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import schedulezero_server as sz  # noqa: E402


def unb64(s: str) -> bytes:
    return base64.urlsafe_b64decode(s + "=" * (-len(s) % 4))


class CurveTests(unittest.TestCase):
    def test_generator_on_curve_and_order(self):
        x, y = sz.P256_G
        self.assertEqual((y * y - (x ** 3 + sz.P256_A * x + sz.P256_B)) % sz.P256_P, 0)
        self.assertIsNone(sz._ec_mul(sz.P256_N))

    def test_sign_verify(self):
        d = 0x1234567890ABCDEF
        pub = sz._ec_mul(d)
        sig = sz.es256_sign(d, b"hello")
        self.assertEqual(len(sig), 64)
        self.assertTrue(sz.es256_verify(pub, b"hello", sig))
        self.assertFalse(sz.es256_verify(pub, b"hellO", sig))

    def test_vapid_header(self):
        tmp = tempfile.mkdtemp()
        try:
            v = sz.Vapid(os.path.join(tmp, "vapid.json"))
            again = sz.Vapid(os.path.join(tmp, "vapid.json"))
            self.assertEqual(v.public_key, again.public_key)  # stable across restarts
            self.assertEqual(len(unb64(v.public_key)), 65)
            h = v.auth_header("https://web.push.apple.com/abc", "https://example.com")
            self.assertTrue(h.startswith("vapid t="))
            jwt = h[len("vapid t="):].split(",")[0]
            head, claims, sig = jwt.split(".")
            c = json.loads(unb64(claims))
            self.assertEqual(c["aud"], "https://web.push.apple.com")
            self.assertTrue(sz.es256_verify(v.public_point, f"{head}.{claims}".encode(), unb64(sig)))
        finally:
            shutil.rmtree(tmp)


class ServerTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.tmp = tempfile.mkdtemp()
        web = os.path.join(cls.tmp, "web")
        os.makedirs(web)
        with open(os.path.join(web, "index.html"), "w") as f:
            f.write("<!doctype html><title>SZ</title>" + "x" * 2000)
        with open(os.path.join(web, "app-abc123.wasm"), "wb") as f:
            f.write(b"\0asm" + b"\1" * 3000)
        with open(os.path.join(cls.tmp, "secret.txt"), "w") as f:
            f.write("nope")
        sz.BASE_PATH = "/ScheduleZero"
        cls.store = sz.Store(os.path.join(cls.tmp, "data"))
        cls.push = sz.PushStore(cls.store.dir, cls.store)
        cls.store.on_new_message = cls.push.notify_message
        static = sz.StaticFiles(web)
        cls.httpd = sz.QuietServer(("127.0.0.1", 0), sz.make_handler(cls.store, False, cls.push, static))
        cls.base = "http://127.0.0.1:%d" % cls.httpd.server_address[1]
        threading.Thread(target=cls.httpd.serve_forever, daemon=True).start()

    @classmethod
    def tearDownClass(cls):
        cls.httpd.shutdown()
        shutil.rmtree(cls.tmp)

    def req(self, method, path, body=None, token=None, headers=None):
        h = {"X-Device-Id": "dev-1", **(headers or {})}
        data = None
        if body is not None:
            data = json.dumps(body).encode()
            h["Content-Type"] = "application/json"
        if token:
            h["Authorization"] = "Bearer " + token
        try:
            with urlopen(Request(self.base + path, data=data, method=method, headers=h), timeout=10) as r:
                return r.status, r.read(), r.headers
        except HTTPError as e:
            return e.code, e.read(), e.headers

    def token(self, person="kayo"):
        st, raw, _ = self.req("GET", "/api/auth/status?person=" + person)
        has = json.loads(raw)["hasPassword"]
        st, raw, _ = self.req("POST", "/api/auth/login" if has else "/api/auth/setup",
                              {"person": person, "password": "secret1", "deviceId": "dev-1"})
        self.assertEqual(st, 200)
        return json.loads(raw)["token"]

    def test_static_index_spa_and_gzip(self):
        st, body, h = self.req("GET", "/")
        self.assertEqual(st, 200)
        self.assertIn(b"<title>SZ</title>", body)
        self.assertEqual(h["Cache-Control"], "no-cache")
        st, body, _ = self.req("GET", "/week/next")  # SPA route
        self.assertIn(b"<title>SZ</title>", body)
        st, body, h = self.req("GET", "/app-abc123.wasm", headers={"Accept-Encoding": "gzip"})
        self.assertEqual(h["Content-Type"], "application/wasm")
        self.assertEqual(h["Content-Encoding"], "gzip")
        self.assertIn("immutable", h["Cache-Control"])
        self.assertTrue(gzip.decompress(body).startswith(b"\0asm"))

    def test_static_blocks_traversal_and_missing(self):
        self.assertEqual(self.req("GET", "/../secret.txt")[0], 404)
        self.assertEqual(self.req("GET", "/%2e%2e/secret.txt")[0], 404)
        self.assertEqual(self.req("GET", "/missing.js")[0], 404)
        self.assertEqual(self.req("GET", "/api/nothing")[0], 404)

    def test_ping(self):
        st, raw, _ = self.req("GET", "/api/ping")
        p = json.loads(raw)
        self.assertTrue(p["web"] and p["push"])
        self.assertEqual(p["version"], sz.VERSION)

    def test_schedule_roundtrip(self):
        t = self.token("samy")
        day = date.today().isoformat()
        st, raw, _ = self.req("POST", "/api/schedule", {"days": {day: {"type": "work", "start": 960, "end": 1440, "updated": 5}}}, t)
        self.assertEqual(json.loads(raw)["applied"], 1)
        st, raw, _ = self.req("GET", "/api/state?since=-1&wait=0", token=t)
        self.assertEqual(json.loads(raw)["people"]["samy"][day]["start"], 960)

    def test_later_edit_wins_even_if_older_one_uploads_last(self):
        """Phone A edits offline at 10:00; phone B edits the same day online at 10:05;
        A reconnects at 10:10 and uploads. B's later edit must stay."""
        t = self.token("austin")
        day = (date.today() + timedelta(days=3)).isoformat()
        now = sz.now_ms()
        a_edit = {"type": "off", "updated": now - 10 * 60_000}
        b_edit = {"type": "work", "start": 1000, "end": 1400, "updated": now - 5 * 60_000}
        st, raw, _ = self.req("POST", "/api/schedule", {"days": {day: b_edit}}, t)
        self.assertEqual(json.loads(raw)["applied"], 1)
        st, raw, _ = self.req("POST", "/api/schedule", {"days": {day: a_edit}}, t)
        self.assertEqual(json.loads(raw)["applied"], 0)
        self.assertIn("serverTime", json.loads(raw))
        st, raw, _ = self.req("GET", "/api/state?since=-1&wait=0", token=t)
        self.assertEqual(json.loads(raw)["people"]["austin"][day]["start"], 1000)

    def test_device_clock_in_future_is_clamped(self):
        t = self.token("austin")
        day = (date.today() + timedelta(days=4)).isoformat()
        future = sz.now_ms() + 24 * 3600 * 1000  # phone clock a day ahead
        self.req("POST", "/api/schedule", {"days": {day: {"type": "off", "updated": future}}}, t)
        st, raw, _ = self.req("GET", "/api/state?since=-1&wait=0", token=t)
        self.assertLessEqual(json.loads(raw)["people"]["austin"][day]["updated"], sz.now_ms())
        # So a normal edit made a minute later still wins.
        later = {"type": "work", "start": 900, "end": 1300, "updated": sz.now_ms() + 1000}
        st, raw, _ = self.req("POST", "/api/schedule", {"days": {day: later}}, t)
        self.assertEqual(json.loads(raw)["applied"], 1)

    def test_push_flow(self):
        t = self.token("kayo")
        st, raw, _ = self.req("GET", "/api/push/key")
        self.assertEqual(len(unb64(json.loads(raw)["publicKey"])), 65)
        sub = {"endpoint": "https://push.example.invalid/abc", "keys": {"p256dh": "x", "auth": "y"}}
        self.assertEqual(self.req("POST", "/api/push/subscribe", {"deviceId": "dev-1", "subscription": {"endpoint": "http://bad"}}, t)[0], 400)
        st, raw, _ = self.req("POST", "/api/push/subscribe", {"deviceId": "dev-1", "subscription": sub}, t)
        key = json.loads(raw)["key"]

        day = date.today() + timedelta(days=1)
        self.req("POST", "/api/schedule", {"days": {day.isoformat(): {"type": "work", "start": 1000, "end": 1400, "updated": 9}}}, t)
        now = sz.now_ms()
        items = [
            {"id": "shift-" + day.isoformat(), "at": now - 1000, "until": now + 10 ** 7,
             "title": "Work at 4:40 PM", "body": "Leave by 4:10 PM",
             "check": {"type": "shift", "day": day.isoformat(), "start": 1000}},
            # Stale: the day now starts at 1000, not 960, so this must be dropped.
            {"id": "shift-stale", "at": now - 1000, "title": "old", "body": "",
             "check": {"type": "shift", "day": day.isoformat(), "start": 960}},
            {"id": "later", "at": now + 10 ** 8, "title": "later", "body": ""},
        ]
        st, raw, _ = self.req("POST", "/api/push/schedule", {"deviceId": "dev-1", "items": items}, t)
        self.assertEqual(json.loads(raw)["scheduled"], 3)
        due = self.push.due(sz.now_ms())
        self.assertEqual(due, [("dev-1", sub["endpoint"])])
        st, raw, _ = self.req("GET", "/api/push/inbox?device=dev-1&key=wrong")
        self.assertIsNone(json.loads(raw)["message"])
        st, raw, _ = self.req("GET", "/api/push/inbox?device=dev-1&key=" + key)
        msg = json.loads(raw)["message"]
        self.assertEqual(msg["title"], "Work at 4:40 PM")
        self.assertTrue(msg["snoozable"])
        self.assertIsNone(json.loads(self.req("GET", "/api/push/inbox?device=dev-1&key=" + key)[1])["message"])

        # Snooze from the notification button: comes back 15 minutes later, and survives a re-upload.
        st, raw, _ = self.req("POST", "/api/push/snooze", {"deviceId": "dev-1", "key": key, "id": msg["id"]})
        self.assertEqual(st, 200)
        self.req("POST", "/api/push/schedule", {"deviceId": "dev-1", "items": []}, t)
        self.assertEqual(self.push.due(sz.now_ms() + 16 * 60_000), [("dev-1", sub["endpoint"])])

        # Weekly prompt only when the week is still empty.
        week = date.today() + timedelta(days=60)
        self.req("POST", "/api/push/schedule", {"deviceId": "dev-1", "items": [
            {"id": "weekly", "at": sz.now_ms() - 1, "title": "Next week", "body": "",
             "check": {"type": "weekly", "week": week.isoformat()}}]}, t)
        self.assertEqual(len(self.push.due(sz.now_ms())), 1)
        self.req("POST", "/api/schedule", {"days": {week.isoformat(): {"type": "off", "updated": 9}}}, t)
        self.req("POST", "/api/push/schedule", {"deviceId": "dev-1", "items": [
            {"id": "weekly", "at": sz.now_ms() - 1, "title": "Next week", "body": "",
             "check": {"type": "weekly", "week": week.isoformat()}}]}, t)
        self.assertEqual(self.push.due(sz.now_ms()), [])

        self.assertEqual(self.req("POST", "/api/push/test", {"deviceId": "dev-1"}, t)[0], 200)
        self.assertEqual(len(self.push.due(sz.now_ms())), 1)

    def test_push_send_headers(self):
        got = {}

        class Capture(BaseHTTPRequestHandler):
            def do_POST(self):
                got.update({k.lower(): v for k, v in self.headers.items()})
                self.send_response(201)
                self.send_header("Content-Length", "0")
                self.end_headers()

            def log_message(self, *a):
                pass

        srv = HTTPServer(("127.0.0.1", 0), Capture)
        threading.Thread(target=srv.handle_request, daemon=True).start()
        status = self.push.send("http://127.0.0.1:%d/push/x" % srv.server_address[1], "https://example.com")
        srv.server_close()
        self.assertEqual(status, 201)
        self.assertTrue(got["authorization"].startswith("vapid t="))
        self.assertEqual(got["ttl"], "21600")

    # ---- web address prefix, CORS ----

    def test_works_under_schedulezero_prefix_or_stripped(self):
        self.assertIn(b"<title>SZ</title>", self.req("GET", "/ScheduleZero/")[1])
        self.assertIn(b"<title>SZ</title>", self.req("GET", "/schedulezero/")[1])  # any case
        self.assertTrue(json.loads(self.req("GET", "/ScheduleZero/api/ping")[1])["ok"])
        self.assertTrue(json.loads(self.req("GET", "/api/ping")[1])["ok"])  # proxy stripped it
        conn = __import__("http.client").client.HTTPConnection("127.0.0.1", self.httpd.server_address[1])
        conn.request("GET", "/ScheduleZero")
        r = conn.getresponse()
        self.assertEqual((r.status, r.getheader("Location")), (301, "/ScheduleZero/"))
        conn.close()

    def test_cors_for_backup_server_use(self):
        st, _, h = self.req("OPTIONS", "/ScheduleZero/api/state")
        self.assertEqual(st, 204)
        self.assertEqual(h["Access-Control-Allow-Origin"], "*")
        self.assertIn("Authorization", h["Access-Control-Allow-Headers"])
        self.assertEqual(self.req("GET", "/api/ping")[2]["Access-Control-Allow-Origin"], "*")

    # ---- carpools & inbox ----

    def carpool(self, token, **body):
        st, raw, _ = self.req("POST", "/ScheduleZero/api/carpool", body, token)
        return st, json.loads(raw)

    def inbox(self, token):
        st, raw, _ = self.req("GET", "/api/state?since=-1&wait=0", token=token)
        return json.loads(raw)

    def test_carpool_request_answer_cancel_and_inbox(self):
        kayo, samy, austin = self.token("kayo"), self.token("samy"), self.token("austin")
        day = (date.today() + timedelta(days=2)).isoformat()
        self.req("POST", "/api/schedule", {"days": {day: {"type": "work", "start": 960, "end": 1440, "updated": 1}}}, kayo)
        st, r = self.carpool(kayo, action="create", kind="request", day=day, dir="to", to=["samy", "austin"], note="pls")
        self.assertEqual(st, 200)
        cid = r["carpool"]["id"]
        self.assertEqual(r["carpool"]["to"], ["austin", "samy"])
        # Both got a message; it names the start time.
        s_inbox = [m for m in self.inbox(samy)["inbox"] if m["carpool"] == cid]
        self.assertEqual(len(s_inbox), 1)
        self.assertEqual(s_inbox[0]["title"], "Kayo asks for a ride to work")
        self.assertIn("starts 4:00 PM", s_inbox[0]["body"])
        self.assertEqual(s_inbox[0]["read"], 0)
        self.assertTrue(any(m["carpool"] == cid for m in self.inbox(austin)["inbox"]))
        self.assertFalse(any(m["carpool"] == cid for m in self.inbox(kayo)["inbox"]))
        # Everyone sees the carpool itself.
        self.assertTrue(any(c["id"] == cid for c in self.inbox(austin)["carpools"]))

        # Only recipients can answer; answering notifies the sender.
        self.assertEqual(self.carpool(kayo, action="respond", id=cid, answer="yes")[0], 400)
        st, r = self.carpool(samy, action="respond", id=cid, answer="yes")
        self.assertEqual(r["carpool"]["responses"]["samy"]["answer"], "yes")
        k_msgs = [m for m in self.inbox(kayo)["inbox"] if m["carpool"] == cid]
        self.assertEqual(k_msgs[0]["title"], "Samy will drive you to work")

        # Mark as read: one message, then all.
        st, raw, _ = self.req("POST", "/api/inbox/read", {"ids": [s_inbox[0]["id"]]}, samy)
        self.assertEqual(json.loads(raw)["marked"], 1)
        self.assertTrue(all(m["read"] for m in self.inbox(samy)["inbox"] if m["carpool"] == cid))
        self.req("POST", "/api/inbox/read", {"all": True}, kayo)
        self.assertTrue(all(m["read"] for m in self.inbox(kayo)["inbox"]))

        # Only the sender can cancel; recipients get told.
        self.assertEqual(self.carpool(samy, action="cancel", id=cid)[0], 400)
        st, r = self.carpool(kayo, action="cancel", id=cid)
        self.assertTrue(r["carpool"]["cancelled"])
        titles = [m["title"] for m in self.inbox(austin)["inbox"] if m["carpool"] == cid]
        self.assertIn("Kayo cancelled the carpool to work", titles)
        self.assertEqual(self.carpool(austin, action="respond", id=cid, answer="yes")[0], 400)

    def test_carpool_validation(self):
        kayo = self.token("kayo")
        day = date.today().isoformat()
        for bad in [dict(kind="x", dir="to", to=["samy"]), dict(kind="offer", dir="up", to=["samy"]),
                    dict(kind="offer", dir="to", to=[]), dict(kind="offer", dir="to", to=["kayo"])]:
            self.assertEqual(self.carpool(kayo, action="create", day=day, **bad)[0], 400, bad)
        st, r = self.carpool(kayo, action="create", kind="offer", day=day, dir="from", to=["samy"])
        self.assertEqual(st, 200)
        samy = self.token("samy")
        msg = [m for m in self.inbox(samy)["inbox"] if m["carpool"] == r["carpool"]["id"]][0]
        self.assertEqual(msg["title"], "Kayo can drive you home from work")

    def test_new_message_is_pushed_with_mark_as_read(self):
        samy = self.token("samy")
        sub = {"endpoint": "https://push.example.invalid/samy", "keys": {}}
        st, raw, _ = self.req("POST", "/api/push/subscribe", {"deviceId": "dev-s", "subscription": sub}, samy)
        key = json.loads(raw)["key"]
        self.push.due(sz.now_ms())  # flush anything earlier
        kayo = self.token("kayo")
        st, r = self.carpool(kayo, action="create", kind="offer", day=date.today().isoformat(), dir="to", to=["samy"])
        due = self.push.due(sz.now_ms())
        self.assertIn(("dev-s", sub["endpoint"]), due)
        st, raw, _ = self.req("GET", "/api/push/inbox?device=dev-s&key=" + key)
        n = json.loads(raw)["message"]
        self.assertTrue(n["message"])
        st, raw, _ = self.req("POST", "/api/push/read", {"deviceId": "dev-s", "key": key, "id": n["message"]})
        self.assertEqual(st, 200)
        self.assertTrue(all(m["read"] for m in self.inbox(samy)["inbox"] if m["id"] == n["message"]))
        # Carpool notifications switched off for that browser: no push.
        self.req("POST", "/api/push/schedule", {"deviceId": "dev-s", "items": [], "messages": False}, samy)
        self.carpool(kayo, action="create", kind="offer", day=date.today().isoformat(), dir="from", to=["samy"])
        self.assertNotIn(("dev-s", sub["endpoint"]), self.push.due(sz.now_ms()))


class PeerMergeTests(unittest.TestCase):
    def test_carpools_and_messages_merge_between_servers(self):
        a_dir, b_dir = tempfile.mkdtemp(), tempfile.mkdtemp()
        try:
            a, b = sz.Store(a_dir), sz.Store(b_dir)
            announced = []
            b.on_new_message = announced.append
            day = (date.today() + timedelta(days=1)).isoformat()
            rec = a.carpool_create("kayo", "request", day, "from", ["samy", "austin"])
            snap = a.snapshot()
            b.merge_all(snap["people"], snap["carpools"], snap["messages"])
            self.assertIn(rec["id"], b.carpools)
            self.assertEqual(len(announced), 2)  # fresh messages get pushed on the other server too
            # Samy answers on A, Austin on B at the same time; both answers survive both ways.
            a.carpool_respond("samy", rec["id"], "yes")
            b.carpool_respond("austin", rec["id"], "no")
            sa, sb = a.snapshot(), b.snapshot()
            a.merge_all(sb["people"], sb["carpools"], sb["messages"])
            b.merge_all(sa["people"], sa["carpools"], sa["messages"])
            for s in (a, b):
                self.assertEqual({k: v["answer"] for k, v in s.carpools[rec["id"]]["responses"].items()},
                                 {"samy": "yes", "austin": "no"})
                self.assertEqual(len([m for m in s.messages.values() if m["to"] == "kayo"]), 2)
            # Read on one server -> read on the other.
            a.mark_read("kayo")
            sa = a.snapshot()
            b.merge_all(sa["people"], sa["carpools"], sa["messages"])
            self.assertTrue(all(m["read"] for m in b.messages.values() if m["to"] == "kayo"))
            # Restart keeps it all.
            self.assertIn(rec["id"], sz.Store(b_dir).carpools)
        finally:
            shutil.rmtree(a_dir)
            shutil.rmtree(b_dir)

    def test_password_reset_from_setting_runs_once(self):
        d = tempfile.mkdtemp()
        try:
            s = sz.Store(d)
            s.set_password("kayo", "abcd")
            sz.reset_passwords(s, "kayo")
            self.assertFalse(s.has_password("kayo"))
            s.set_password("kayo", "efgh")
            sz.reset_passwords(s, "kayo")  # same setting again (restart): nothing happens
            self.assertTrue(s.has_password("kayo"))
            sz.reset_passwords(s, "kayo #2")
            self.assertFalse(s.has_password("kayo"))
        finally:
            shutil.rmtree(d)


if __name__ == "__main__":
    unittest.main(verbosity=2)
