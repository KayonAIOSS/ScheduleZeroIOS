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
        cls.store = sz.Store(os.path.join(cls.tmp, "data"))
        cls.push = sz.PushStore(cls.store.dir, cls.store)
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


if __name__ == "__main__":
    unittest.main(verbosity=2)
