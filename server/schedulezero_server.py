#!/usr/bin/env python3
"""
ScheduleZero sync server.

* Python 3.8+ standard library only. No pip installs.
* Devices always connect TO the server; the server never contacts devices, so a phone being
  offline or unreachable is completely normal and is never logged as an error.
* The server keeps the newest version of every person's schedule (last-write-wins per day)
  and hands it to every device that asks.
* Live updates use HTTP long-polling (GET /api/state?since=REV&wait=25), which any client
  (Android, iOS) can implement with a plain HTTP library. See PROTOCOL.md.

Server-to-server sync (optional): set SZ_PEERS to one or more other ScheduleZero servers
(comma separated) and SZ_PEER_KEY to the same secret on every server. Every SZ_PEER_INTERVAL
seconds (default 20) this server pulls each peer's schedules and pushes its own, so all servers
converge on the newest copy of every day. Only ONE side needs SZ_PEERS (e.g. the server that can
reach the other one). Passwords are never shared between servers. Unreachable peers are silent.

Data directory layout (default: ./data, override with --data or SZ_DATA):
    schedules.json          every person's days + a global revision counter
    devices.json            devices that have signed in (informational only)
    passwords/<person>.json password hash + login tokens for that person.
                            Deleting this file ONLY resets that person's password: their
                            schedule and device list are untouched, and their devices will be
                            asked to create a new password the next time they connect.
"""

import argparse
import hashlib
import hmac
import json
import os
import secrets
import ssl
import sys
import tempfile
import threading
import time
from datetime import date, timedelta
from http import HTTPStatus
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from urllib.parse import parse_qs, urlparse
from urllib.request import Request, urlopen

PEOPLE = {
    "samy": {"name": "Samy", "color": "#E53935"},
    "austin": {"name": "Austin", "color": "#1E88E5"},
    "kayo": {"name": "Kayo", "color": "#9C4DFF"},
}
PROTOCOL_VERSION = 1
MAX_WAIT_SECONDS = 25
KEEP_DAYS = 16 * 7          # history kept on the server
MAX_FUTURE_DAYS = 400       # ignore absurd dates
PBKDF2_ITERATIONS = 200_000
MIN_PASSWORD_LEN = 4
MAX_BODY = 256 * 1024
WORK_MIN, WORK_MAX = 12 * 60, 26 * 60   # noon .. 2 AM next day, in work-day minutes


def now_ms() -> int:
    return int(time.time() * 1000)


def atomic_write_json(path: str, data) -> None:
    os.makedirs(os.path.dirname(path), exist_ok=True)
    fd, tmp = tempfile.mkstemp(dir=os.path.dirname(path), prefix=".tmp-", suffix=".json")
    try:
        with os.fdopen(fd, "w", encoding="utf-8") as f:
            json.dump(data, f, indent=1, sort_keys=True)
        os.replace(tmp, path)
    except BaseException:
        try:
            os.unlink(tmp)
        except OSError:
            pass
        raise


def read_json(path: str, default):
    try:
        with open(path, "r", encoding="utf-8") as f:
            return json.load(f)
    except (FileNotFoundError, json.JSONDecodeError):
        return default


class Store:
    """All state lives here. One lock; the data set is tiny (3 people x a few months)."""

    def __init__(self, data_dir: str):
        self.dir = os.path.abspath(data_dir)
        self.pw_dir = os.path.join(self.dir, "passwords")
        os.makedirs(self.pw_dir, exist_ok=True)
        self.sched_path = os.path.join(self.dir, "schedules.json")
        self.dev_path = os.path.join(self.dir, "devices.json")
        self.cond = threading.Condition()
        raw = read_json(self.sched_path, {})
        self.rev = int(raw.get("rev", 0))
        self.people = {p: dict(raw.get("people", {}).get(p, {})) for p in PEOPLE}
        self.devices = read_json(self.dev_path, {})
        self._prune()

    # ---------- schedules ----------

    def _prune(self) -> None:
        cutoff = (date.today() - timedelta(days=KEEP_DAYS)).isoformat()
        for days in self.people.values():
            for d in [d for d in days if d < cutoff]:
                del days[d]

    def _save_schedules(self) -> None:
        atomic_write_json(self.sched_path, {"rev": self.rev, "people": self.people})

    def snapshot(self) -> dict:
        return {
            "rev": self.rev,
            "serverTime": now_ms(),
            "people": {p: dict(days) for p, days in self.people.items()},
        }

    def wait_for_change(self, since: int, wait: float) -> dict:
        deadline = time.monotonic() + wait
        with self.cond:
            while self.rev <= since:
                remaining = deadline - time.monotonic()
                if remaining <= 0:
                    return {"rev": self.rev, "serverTime": now_ms(), "unchanged": True}
                self.cond.wait(remaining)
            return self.snapshot()

    def merge_days(self, person: str, incoming: dict) -> int:
        """Last-write-wins per day, using each entry's `updated` timestamp."""
        applied = 0
        with self.cond:
            days = self.people[person]
            for day, entry in incoming.items():
                clean = validate_entry(day, entry)
                if clean is None:
                    continue
                current = days.get(day)
                if current is None or clean["updated"] > int(current.get("updated", 0)):
                    days[day] = clean
                    applied += 1
            if applied:
                self.rev += 1
                self._prune()
                self._save_schedules()
                self.cond.notify_all()
        return applied

    def merge_all(self, people: dict) -> int:
        total = 0
        if not isinstance(people, dict):
            return 0
        for person, days in people.items():
            if person in PEOPLE and isinstance(days, dict):
                total += self.merge_days(person, days)
        return total

    # ---------- devices ----------

    def touch_device(self, device_id: str, person: str, name: str = None, platform: str = None) -> None:
        if not device_id:
            return
        device_id = str(device_id)[:64]
        with self.cond:
            d = self.devices.get(device_id, {})
            changed = d.get("person") != person or (name and d.get("name") != name)
            d["person"] = person
            if name:
                d["name"] = str(name)[:80]
            if platform:
                d["platform"] = str(platform)[:20]
            last = d.get("lastSeen", 0)
            d["lastSeen"] = now_ms()
            self.devices[device_id] = d
            # Avoid rewriting the file on every poll: only on changes or every 10 minutes.
            if changed or d["lastSeen"] - last > 10 * 60 * 1000:
                atomic_write_json(self.dev_path, self.devices)

    # ---------- passwords / tokens ----------

    def _pw_path(self, person: str) -> str:
        return os.path.join(self.pw_dir, person + ".json")

    def has_password(self, person: str) -> bool:
        return os.path.isfile(self._pw_path(person))

    def set_password(self, person: str, password: str) -> str:
        with self.cond:
            if self.has_password(person):
                raise PermissionError("exists")
            salt = secrets.token_hex(16)
            rec = {
                "salt": salt,
                "iterations": PBKDF2_ITERATIONS,
                "hash": _hash_pw(password, salt, PBKDF2_ITERATIONS),
                "created": now_ms(),
                "tokens": [],
            }
            token = self._new_token(person, rec)
            atomic_write_json(self._pw_path(person), rec)
            return token

    def login(self, person: str, password: str):
        with self.cond:
            rec = read_json(self._pw_path(person), None)
            if not rec:
                return None
            expected = _hash_pw(password, rec["salt"], int(rec.get("iterations", PBKDF2_ITERATIONS)))
            if not hmac.compare_digest(expected, rec["hash"]):
                return None
            token = self._new_token(person, rec)
            atomic_write_json(self._pw_path(person), rec)
            return token

    def _new_token(self, person: str, rec: dict) -> str:
        token = person + "." + secrets.token_urlsafe(32)
        rec.setdefault("tokens", []).append(hashlib.sha256(token.encode()).hexdigest())
        rec["tokens"] = rec["tokens"][-50:]  # plenty for a handful of devices
        return token

    def person_for_token(self, token: str):
        if not token or "." not in token:
            return None
        person = token.split(".", 1)[0]
        if person not in PEOPLE:
            return None
        rec = read_json(self._pw_path(person), None)  # file deleted => every token is void
        if not rec:
            return None
        digest = hashlib.sha256(token.encode()).hexdigest()
        return person if any(hmac.compare_digest(digest, t) for t in rec.get("tokens", [])) else None


def _hash_pw(password: str, salt: str, iterations: int) -> str:
    return hashlib.pbkdf2_hmac("sha256", password.encode("utf-8"), bytes.fromhex(salt), iterations).hex()


def validate_entry(day: str, entry):
    try:
        d = date.fromisoformat(day)
    except (TypeError, ValueError):
        return None
    if d > date.today() + timedelta(days=MAX_FUTURE_DAYS):
        return None
    if not isinstance(entry, dict):
        return None
    try:
        updated = int(entry.get("updated"))
    except (TypeError, ValueError):
        return None
    kind = entry.get("type")
    if kind == "work":
        try:
            start, end = int(entry["start"]), int(entry["end"])
        except (KeyError, TypeError, ValueError):
            return None
        if not (WORK_MIN <= start < end <= WORK_MAX):
            return None
        return {"type": "work", "start": start, "end": end, "updated": updated}
    if kind in ("off", "none"):
        return {"type": kind, "updated": updated}
    return None


class QuietServer(ThreadingHTTPServer):
    daemon_threads = True
    allow_reuse_address = True

    def handle_error(self, request, client_address):
        # Devices dropping off mid-request (sleep, lost signal, app closed) is normal. Stay quiet.
        exc = sys.exc_info()[1]
        if isinstance(exc, (ConnectionError, TimeoutError, OSError)):
            return
        super().handle_error(request, client_address)


def make_handler(store: Store, verbose: bool):
    class Handler(BaseHTTPRequestHandler):
        server_version = "ScheduleZero/1"
        protocol_version = "HTTP/1.1"

        def log_message(self, fmt, *args):
            if verbose:
                sys.stderr.write("%s %s\n" % (self.address_string(), fmt % args))

        # ---- helpers ----
        def _send(self, status: int, payload) -> None:
            body = json.dumps(payload).encode("utf-8")
            try:
                self.send_response(status)
                self.send_header("Content-Type", "application/json; charset=utf-8")
                self.send_header("Content-Length", str(len(body)))
                self.send_header("Cache-Control", "no-store")
                self.end_headers()
                self.wfile.write(body)
            except (ConnectionError, TimeoutError, OSError):
                pass  # client went away; nothing to do

        def _body(self) -> dict:
            length = int(self.headers.get("Content-Length") or 0)
            if length <= 0 or length > MAX_BODY:
                return {}
            try:
                data = json.loads(self.rfile.read(length).decode("utf-8"))
                return data if isinstance(data, dict) else {}
            except (ValueError, UnicodeDecodeError):
                return {}

        def _auth(self):
            h = self.headers.get("Authorization", "")
            token = h[7:].strip() if h.startswith("Bearer ") else ""
            person = store.person_for_token(token)
            if person is None:
                self._send(HTTPStatus.UNAUTHORIZED, {"error": "unauthorized"})
            return person

        def _peer_ok(self) -> bool:
            key = os.environ.get("SZ_PEER_KEY", "")
            given = self.headers.get("X-Peer-Key", "")
            if not key or not hmac.compare_digest(key, given):
                self._send(HTTPStatus.FORBIDDEN, {"error": "forbidden"})
                return False
            return True

        def _device(self, person: str, body: dict = None) -> None:
            body = body or {}
            store.touch_device(
                self.headers.get("X-Device-Id") or body.get("deviceId"),
                person,
                self.headers.get("X-Device-Name") or body.get("deviceName"),
                self.headers.get("X-Platform") or body.get("platform"),
            )

        # ---- routes ----
        def do_GET(self):
            url = urlparse(self.path)
            q = parse_qs(url.query)
            if url.path == "/api/ping":
                return self._send(200, {
                    "ok": True, "app": "schedulezero", "protocol": PROTOCOL_VERSION,
                    "serverTime": now_ms(),
                    "people": [{"id": k, **v} for k, v in PEOPLE.items()],
                })
            if url.path == "/api/auth/status":
                person = (q.get("person") or [""])[0].lower()
                if person not in PEOPLE:
                    return self._send(400, {"error": "unknown person"})
                return self._send(200, {"person": person, "hasPassword": store.has_password(person)})
            if url.path == "/api/state":
                person = self._auth()
                if not person:
                    return
                self._device(person)
                try:
                    since = int((q.get("since") or ["-1"])[0])
                    wait = min(max(float((q.get("wait") or ["0"])[0]), 0), MAX_WAIT_SECONDS)
                except ValueError:
                    since, wait = -1, 0
                result = store.wait_for_change(since, wait) if wait > 0 else (
                    store.snapshot() if store.rev > since else
                    {"rev": store.rev, "serverTime": now_ms(), "unchanged": True})
                result["you"] = person
                return self._send(200, result)
            if url.path == "/api/peer/state":
                if not self._peer_ok():
                    return
                return self._send(200, store.snapshot())
            if url.path == "/api/devices":
                person = self._auth()
                if not person:
                    return
                mine = {k: v for k, v in store.devices.items() if v.get("person") == person}
                return self._send(200, {"devices": mine})
            self._send(404, {"error": "not found"})

        def do_POST(self):
            url = urlparse(self.path)
            body = self._body()
            if url.path in ("/api/auth/setup", "/api/auth/login"):
                person = str(body.get("person", "")).lower()
                password = str(body.get("password", ""))
                if person not in PEOPLE:
                    return self._send(400, {"error": "unknown person"})
                if url.path.endswith("setup"):
                    if len(password) < MIN_PASSWORD_LEN:
                        return self._send(400, {"error": "password too short", "min": MIN_PASSWORD_LEN})
                    try:
                        token = store.set_password(person, password)
                    except PermissionError:
                        return self._send(409, {"error": "password already set"})
                else:
                    token = store.login(person, password)
                    if not token:
                        time.sleep(1.0)  # slow down guessing
                        status = 401 if store.has_password(person) else 404
                        return self._send(status, {"error": "wrong password" if status == 401 else "no password set"})
                self._device(person, body)
                return self._send(200, {"token": token, "person": person})
            if url.path == "/api/peer/merge":
                if not self._peer_ok():
                    return
                applied = store.merge_all(body.get("people"))
                return self._send(200, {"applied": applied, "rev": store.rev})
            if url.path == "/api/schedule":
                person = self._auth()
                if not person:
                    return
                self._device(person, body)
                days = body.get("days")
                if not isinstance(days, dict):
                    return self._send(400, {"error": "days must be an object"})
                applied = store.merge_days(person, days)
                return self._send(200, {"applied": applied, "rev": store.rev})
            self._send(404, {"error": "not found"})

    return Handler


def peer_sync_loop(store: Store, peers, key: str, interval: float, verbose: bool) -> None:
    """Pull every peer's schedules, merge them, then push ours. Failures are expected and silent."""
    while True:
        for base in peers:
            try:
                req = Request(base.rstrip("/") + "/api/peer/state", headers={"X-Peer-Key": key})
                with urlopen(req, timeout=15) as r:
                    theirs = json.loads(r.read().decode("utf-8"))
                store.merge_all(theirs.get("people"))
                mine = json.dumps({"people": store.snapshot()["people"]}).encode("utf-8")
                req = Request(base.rstrip("/") + "/api/peer/merge", data=mine, method="POST",
                              headers={"X-Peer-Key": key, "Content-Type": "application/json"})
                with urlopen(req, timeout=15) as r:
                    r.read()
            except Exception as e:  # unreachable / not set up yet / wrong key: just try again later
                if verbose:
                    sys.stderr.write("peer %s: %s\n" % (base, e))
        time.sleep(interval)


def main():
    ap = argparse.ArgumentParser(description="ScheduleZero sync server")
    ap.add_argument("--host", default=os.environ.get("SZ_HOST", "0.0.0.0"))
    ap.add_argument("--port", type=int, default=int(os.environ.get("SZ_PORT", "8787")))
    ap.add_argument("--data", default=os.environ.get("SZ_DATA", os.path.join(os.path.dirname(os.path.abspath(__file__)), "data")))
    ap.add_argument("--verbose", action="store_true", help="log every request")
    ap.add_argument("--cert", default=os.environ.get("SZ_CERT"), help="TLS certificate (PEM) to serve HTTPS directly")
    ap.add_argument("--key", default=os.environ.get("SZ_KEY"), help="TLS private key (PEM)")
    args = ap.parse_args()

    store = Store(args.data)
    server = QuietServer((args.host, args.port), make_handler(store, args.verbose))
    scheme = "http"
    if args.cert and args.key:
        ctx = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
        ctx.load_cert_chain(args.cert, args.key)
        server.socket = ctx.wrap_socket(server.socket, server_side=True)
        scheme = "https"

    peers = [p.strip() for p in os.environ.get("SZ_PEERS", "").split(",") if p.strip()]
    peer_key = os.environ.get("SZ_PEER_KEY", "")
    if peers and peer_key:
        interval = float(os.environ.get("SZ_PEER_INTERVAL", "20"))
        threading.Thread(target=peer_sync_loop, args=(store, peers, peer_key, interval, args.verbose), daemon=True).start()

    print(f"ScheduleZero server on {scheme}://{args.host}:{args.port}  data={store.dir}"
          + (f"  peers={','.join(peers)}" if peers and peer_key else ""), flush=True)
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        pass


if __name__ == "__main__":
    main()
