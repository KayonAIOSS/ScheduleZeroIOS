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

Web app (optional): if a `web/` folder sits next to this file (or SZ_WEB points at one), every
non-/api path serves the ScheduleZero web app from it. Without that folder the server is sync-only.

Web push (for the web app's reminders): the browser registers a push subscription and uploads
the reminders it wants; this server sends them at the right time with VAPID (RFC 8292). Pushes
carry no payload: the service worker fetches the text from /api/push/inbox. Keys are made on
first start and stored in the data directory.

Data directory layout (default: ./data, override with --data or SZ_DATA):
    schedules.json          every person's days + a global revision counter
    devices.json            devices that have signed in (informational only)
    push.json               web-push subscriptions and their queued reminders
    vapid.json              this server's web-push key pair
    passwords/<person>.json password hash + login tokens for that person.
                            Deleting this file ONLY resets that person's password: their
                            schedule and device list are untouched, and their devices will be
                            asked to create a new password the next time they connect.
"""

import argparse
import base64
import gzip
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
from urllib.error import HTTPError
from urllib.parse import parse_qs, unquote, urlparse
from urllib.request import Request, urlopen

VERSION = "2.0.0"

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


# ---------------------------------------------------------------------------------------------
# Web push. VAPID needs ES256 (ECDSA on P-256 with SHA-256). The standard library has no ECDSA,
# so this is a small, plain implementation; it only ever signs this server's own short JWTs.
# ---------------------------------------------------------------------------------------------

P256_P = 0xFFFFFFFF00000001000000000000000000000000FFFFFFFFFFFFFFFFFFFFFFFF
P256_A = P256_P - 3
P256_B = 0x5AC635D8AA3A93E7B3EBBD55769886BC651D06B0CC53B0F63BCE3C3E27D2604B
P256_N = 0xFFFFFFFF00000000FFFFFFFFFFFFFFFFBCE6FAADA7179E84F3B9CAC2FC632551
P256_G = (0x6B17D1F2E12C4247F8BCE6E563A440F277037D812DEB33A0F4A13945D898C296,
          0x4FE342E2FE1A7F9B8EE7EB4A7C0F9E162BCE33576B315ECECBB6406837BF51F5)


def _ec_add(p1, p2):
    if p1 is None:
        return p2
    if p2 is None:
        return p1
    (x1, y1), (x2, y2) = p1, p2
    if x1 == x2:
        if (y1 + y2) % P256_P == 0:
            return None
        lam = (3 * x1 * x1 + P256_A) * pow(2 * y1, -1, P256_P) % P256_P
    else:
        lam = (y2 - y1) * pow(x2 - x1, -1, P256_P) % P256_P
    x3 = (lam * lam - x1 - x2) % P256_P
    return x3, (lam * (x1 - x3) - y1) % P256_P


def _ec_mul(k: int, point=P256_G):
    result = None
    while k:
        if k & 1:
            result = _ec_add(result, point)
        point = _ec_add(point, point)
        k >>= 1
    return result


def b64url(data: bytes) -> str:
    return base64.urlsafe_b64encode(data).rstrip(b"=").decode("ascii")


def es256_sign(private_key: int, message: bytes) -> bytes:
    """ECDSA P-256 / SHA-256 signature as the 64-byte r||s that JWTs use."""
    e = int.from_bytes(hashlib.sha256(message).digest(), "big")
    while True:
        k = secrets.randbelow(P256_N - 1) + 1
        r = _ec_mul(k)[0] % P256_N
        if r == 0:
            continue
        s = pow(k, -1, P256_N) * (e + r * private_key) % P256_N
        if s:
            return r.to_bytes(32, "big") + s.to_bytes(32, "big")


def es256_verify(public_point, message: bytes, sig: bytes) -> bool:
    r, s = int.from_bytes(sig[:32], "big"), int.from_bytes(sig[32:], "big")
    if not (0 < r < P256_N and 0 < s < P256_N):
        return False
    e = int.from_bytes(hashlib.sha256(message).digest(), "big")
    w = pow(s, -1, P256_N)
    pt = _ec_add(_ec_mul(e * w % P256_N), _ec_mul(r * w % P256_N, public_point))
    return pt is not None and pt[0] % P256_N == r


class Vapid:
    def __init__(self, path: str):
        rec = read_json(path, None)
        if not rec or "d" not in rec:
            rec = {"d": "%064x" % (secrets.randbelow(P256_N - 1) + 1)}
            atomic_write_json(path, rec)
        self.d = int(rec["d"], 16)
        x, y = _ec_mul(self.d)
        self.public_point = (x, y)
        self.public_key = b64url(b"\x04" + x.to_bytes(32, "big") + y.to_bytes(32, "big"))

    def auth_header(self, endpoint: str, contact: str) -> str:
        u = urlparse(endpoint)
        header = b64url(json.dumps({"typ": "JWT", "alg": "ES256"}, separators=(",", ":")).encode())
        claims = b64url(json.dumps({"aud": f"{u.scheme}://{u.netloc}", "exp": int(time.time()) + 12 * 3600,
                                    "sub": contact}, separators=(",", ":")).encode())
        signing_input = f"{header}.{claims}".encode("ascii")
        jwt = f"{header}.{claims}.{b64url(es256_sign(self.d, signing_input))}"
        return f"vapid t={jwt}, k={self.public_key}"


class PushStore:
    """Web-push subscriptions (one per browser install) and the reminders each one asked for.

    Each reminder carries a `check` so a stale one is dropped instead of sent: a shift reminder only
    fires if that day is still a work day starting at the same time; the weekly prompt only fires if
    that week is still empty. Same rules as the Android app's alarms.
    """

    MAX_ITEMS = 80
    MAX_INBOX = 10

    def __init__(self, data_dir: str, store: "Store"):
        self.path = os.path.join(data_dir, "push.json")
        self.store = store
        self.lock = threading.Lock()
        self.devices = read_json(self.path, {}).get("devices", {})
        self.vapid = Vapid(os.path.join(data_dir, "vapid.json"))

    def _save(self) -> None:
        atomic_write_json(self.path, {"devices": self.devices})

    def subscribe(self, device_id: str, person: str, sub: dict) -> str:
        endpoint = str(sub.get("endpoint", ""))
        if not endpoint.startswith("https://"):
            raise ValueError("bad endpoint")
        with self.lock:
            d = self.devices.get(device_id) or {"items": [], "inbox": [], "key": secrets.token_urlsafe(24)}
            if d.get("person") != person:
                d["items"], d["inbox"] = [], []
            d.update(person=person, endpoint=endpoint, updated=now_ms())
            self.devices[device_id] = d
            self._save()
            return d["key"]

    def unsubscribe(self, device_id: str) -> None:
        with self.lock:
            if self.devices.pop(device_id, None) is not None:
                self._save()

    def set_items(self, device_id: str, person: str, items) -> int:
        clean = []
        for it in (items if isinstance(items, list) else [])[: self.MAX_ITEMS]:
            if not isinstance(it, dict):
                continue
            try:
                clean.append({
                    "id": str(it["id"])[:40], "at": int(it["at"]),
                    "until": int(it.get("until") or 0),
                    "title": str(it.get("title", ""))[:120], "body": str(it.get("body", ""))[:400],
                    "check": it.get("check") if isinstance(it.get("check"), dict) else {},
                    "snooze": bool(it.get("snooze", False)),
                })
            except (KeyError, TypeError, ValueError):
                continue
        with self.lock:
            d = self.devices.get(device_id)
            if not d or d.get("person") != person:
                return -1
            # Snoozes are made by the server from a notification button; the app doesn't know them.
            d["items"] = clean + [i for i in d.get("items", []) if i.get("id", "").endswith("~snooze")]
            self._save()
            return len(clean)

    def enqueue_now(self, device_id: str, person: str, title: str, body: str) -> bool:
        with self.lock:
            d = self.devices.get(device_id)
            if not d or d.get("person") != person:
                return False
            d.setdefault("items", []).append({"id": "test-%d" % now_ms(), "at": now_ms(), "until": 0,
                                              "title": title, "body": body, "check": {}, "snooze": False})
            self._save()
        return True

    def _device_for_key(self, device_id: str, key: str):
        d = self.devices.get(device_id)
        if d and key and hmac.compare_digest(str(d.get("key", "")), str(key)):
            return d
        return None

    def pop_inbox(self, device_id: str, key: str):
        with self.lock:
            d = self._device_for_key(device_id, key)
            if not d or not d.get("inbox"):
                return None
            msg = d["inbox"].pop(0)
            self._save()
            return msg

    def snooze(self, device_id: str, key: str, item_id: str, minutes: int = 15) -> bool:
        with self.lock:
            d = self._device_for_key(device_id, key)
            if not d:
                return False
            src = next((m for m in d.get("sent", []) if m.get("id") == item_id), None)
            if not src:
                return False
            base = item_id.split("~")[0]
            d["items"] = [i for i in d.get("items", []) if i.get("id") != base + "~snooze"]
            d["items"].append({**src, "id": base + "~snooze", "at": now_ms() + minutes * 60_000, "snooze": True})
            self._save()
            return True

    def _still_valid(self, person: str, item: dict, now: int) -> bool:
        if item.get("until") and now >= item["until"]:
            return False
        check = item.get("check") or {}
        days = self.store.people.get(person, {})
        if check.get("type") == "shift":
            e = days.get(str(check.get("day")))
            return bool(e) and e.get("type") == "work" and e.get("start") == check.get("start")
        if check.get("type") == "weekly":
            try:
                start = date.fromisoformat(str(check.get("week")))
            except ValueError:
                return False
            for i in range(7):
                e = days.get((start + timedelta(days=i)).isoformat())
                if e and e.get("type") in ("work", "off"):
                    return False
        return True

    def due(self, now: int):
        """Moves due reminders to their device's inbox. Returns [(device_id, endpoint)] to push."""
        out = []
        with self.lock:
            changed = False
            for dev_id, d in self.devices.items():
                keep = []
                for item in d.get("items", []):
                    if item.get("at", 0) > now:
                        keep.append(item)
                        continue
                    changed = True
                    if now - item.get("at", 0) > 6 * 3600 * 1000 or not self._still_valid(d.get("person"), item, now):
                        continue
                    msg = {k: item[k] for k in ("id", "title", "body")}
                    msg["tag"] = item["id"].split("~")[0]
                    msg["snoozable"] = item["id"].startswith("shift-")
                    d["inbox"] = (d.get("inbox", []) + [msg])[-self.MAX_INBOX:]
                    d["sent"] = (d.get("sent", []) + [{**item, "id": item["id"]}])[-20:]
                    out.append((dev_id, d.get("endpoint")))
                d["items"] = keep
            if changed:
                self._save()
        return out

    def send(self, endpoint: str, contact: str) -> int:
        req = Request(endpoint, data=b"", method="POST", headers={
            "Authorization": self.vapid.auth_header(endpoint, contact),
            "TTL": "21600", "Urgency": "high", "Content-Length": "0",
        })
        try:
            with urlopen(req, timeout=15) as r:
                return r.status
        except HTTPError as e:
            return e.code


def push_loop(push: PushStore, contact: str, verbose: bool) -> None:
    while True:
        try:
            for dev_id, endpoint in push.due(now_ms()):
                status = push.send(endpoint, contact) if endpoint else 0
                if verbose:
                    sys.stderr.write("push %s -> %s\n" % (dev_id[:8], status))
                if status in (404, 410):  # the browser dropped this subscription
                    push.unsubscribe(dev_id)
        except Exception as e:  # network trouble: try again next round
            if verbose:
                sys.stderr.write("push loop: %s\n" % e)
        time.sleep(10)


# ---------------------------------------------------------------------------------------------
# Static files for the web app.
# ---------------------------------------------------------------------------------------------

STATIC_TYPES = {
    ".html": "text/html; charset=utf-8", ".js": "text/javascript; charset=utf-8",
    ".mjs": "text/javascript; charset=utf-8", ".wasm": "application/wasm",
    ".css": "text/css; charset=utf-8", ".json": "application/json",
    ".webmanifest": "application/manifest+json", ".png": "image/png", ".svg": "image/svg+xml",
    ".ico": "image/x-icon", ".txt": "text/plain; charset=utf-8",
}
COMPRESSIBLE = {".html", ".js", ".mjs", ".wasm", ".css", ".json", ".webmanifest", ".svg", ".txt"}


class StaticFiles:
    def __init__(self, root: str):
        self.root = os.path.abspath(root)
        self.cache = {}
        self.lock = threading.Lock()

    def resolve(self, url_path: str):
        rel = unquote(url_path).lstrip("/")
        path = os.path.abspath(os.path.join(self.root, rel))
        if path != self.root and not path.startswith(self.root + os.sep):
            return None
        if os.path.isfile(path):
            return path
        # Single-page app: unknown paths without a file extension get index.html.
        if "." not in os.path.basename(rel):
            index = os.path.join(self.root, "index.html")
            return index if os.path.isfile(index) else None
        return None

    def load(self, path: str, want_gzip: bool):
        """Returns (bytes, content_type, gzipped, cache_control)."""
        ext = os.path.splitext(path)[1].lower()
        mtime = os.path.getmtime(path)
        with self.lock:
            hit = self.cache.get(path)
            if not hit or hit[0] != mtime:
                with open(path, "rb") as f:
                    raw = f.read()
                gz = gzip.compress(raw, 9) if ext in COMPRESSIBLE and len(raw) > 512 else None
                hit = (mtime, raw, gz)
                self.cache[path] = hit
        _, raw, gz = hit
        name = os.path.basename(path)
        if name in ("index.html", "sw.js", "manifest.webmanifest") or ext == ".html":
            cache = "no-cache"
        elif "-" in name and ext in (".js", ".wasm", ".css"):  # hashed by the build
            cache = "public, max-age=31536000, immutable"
        else:
            cache = "public, max-age=3600"
        ctype = STATIC_TYPES.get(ext, "application/octet-stream")
        if want_gzip and gz is not None:
            return gz, ctype, True, cache
        return raw, ctype, False, cache


class QuietServer(ThreadingHTTPServer):
    daemon_threads = True
    allow_reuse_address = True

    def handle_error(self, request, client_address):
        # Devices dropping off mid-request (sleep, lost signal, app closed) is normal. Stay quiet.
        exc = sys.exc_info()[1]
        if isinstance(exc, (ConnectionError, TimeoutError, OSError)):
            return
        super().handle_error(request, client_address)


def make_handler(store: Store, verbose: bool, push: "PushStore" = None, static: "StaticFiles" = None):
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

        def _static(self, url, head: bool = False) -> None:
            path = static.resolve(url.path) if static else None
            if not path:
                return self._send(404, {"error": "not found"})
            want_gzip = "gzip" in (self.headers.get("Accept-Encoding") or "")
            body, ctype, gz, cache = static.load(path, want_gzip)
            try:
                self.send_response(200)
                self.send_header("Content-Type", ctype)
                self.send_header("Content-Length", str(len(body)))
                self.send_header("Cache-Control", cache)
                self.send_header("Vary", "Accept-Encoding")
                self.send_header("X-Content-Type-Options", "nosniff")
                if gz:
                    self.send_header("Content-Encoding", "gzip")
                if os.path.basename(path) == "sw.js":
                    self.send_header("Service-Worker-Allowed", "/")
                self.end_headers()
                if not head:
                    self.wfile.write(body)
            except (ConnectionError, TimeoutError, OSError):
                pass

        def do_HEAD(self):
            url = urlparse(self.path)
            if url.path.startswith("/api/"):
                return self._send(405, {"error": "use GET"})
            return self._static(url, head=True)

        # ---- routes ----
        def do_GET(self):
            url = urlparse(self.path)
            q = parse_qs(url.query)
            if not url.path.startswith("/api/"):
                return self._static(url)
            if url.path == "/api/ping":
                return self._send(200, {
                    "ok": True, "app": "schedulezero", "protocol": PROTOCOL_VERSION,
                    "version": VERSION, "serverTime": now_ms(),
                    "web": static is not None, "push": push is not None,
                    "people": [{"id": k, **v} for k, v in PEOPLE.items()],
                })
            if url.path == "/api/push/key":
                if not push:
                    return self._send(404, {"error": "push disabled"})
                return self._send(200, {"publicKey": push.vapid.public_key})
            if url.path == "/api/push/inbox":
                if not push:
                    return self._send(404, {"error": "push disabled"})
                msg = push.pop_inbox((q.get("device") or [""])[0], (q.get("key") or [""])[0])
                return self._send(200, {"message": msg})
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
            if url.path.startswith("/api/push/"):
                return self._push_post(url.path, body)
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

        def _push_post(self, path: str, body: dict):
            if not push:
                return self._send(404, {"error": "push disabled"})
            device = str(body.get("deviceId") or self.headers.get("X-Device-Id") or "")[:64]
            if not device:
                return self._send(400, {"error": "deviceId required"})
            if path == "/api/push/snooze":  # from the service worker: inbox key instead of a token
                ok = push.snooze(device, str(body.get("key", "")), str(body.get("id", "")))
                return self._send(200 if ok else 404, {"ok": ok})
            person = self._auth()
            if not person:
                return
            if path == "/api/push/subscribe":
                sub = body.get("subscription")
                if not isinstance(sub, dict):
                    return self._send(400, {"error": "subscription required"})
                try:
                    key = push.subscribe(device, person, sub)
                except ValueError:
                    return self._send(400, {"error": "bad subscription"})
                return self._send(200, {"key": key})
            if path == "/api/push/unsubscribe":
                push.unsubscribe(device)
                return self._send(200, {"ok": True})
            if path == "/api/push/schedule":
                n = push.set_items(device, person, body.get("items"))
                if n < 0:
                    return self._send(404, {"error": "not subscribed"})
                return self._send(200, {"scheduled": n})
            if path == "/api/push/test":
                ok = push.enqueue_now(device, person, "Test reminder",
                                      "This is what a shift reminder will look like.")
                return self._send(200 if ok else 404, {"ok": ok})
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
    ap.add_argument("--web", default=os.environ.get("SZ_WEB", os.path.join(os.path.dirname(os.path.abspath(__file__)), "web")),
                    help="folder with the built web app (served at /). Missing folder = sync-only server")
    args = ap.parse_args()

    store = Store(args.data)
    push = PushStore(store.dir, store) if os.environ.get("SZ_PUSH", "1") != "0" else None
    static = StaticFiles(args.web) if os.path.isfile(os.path.join(args.web, "index.html")) else None
    server = QuietServer((args.host, args.port), make_handler(store, args.verbose, push, static))
    if push:
        contact = os.environ.get("SZ_PUSH_CONTACT", "https://github.com/KayonAIOSS/ScheduleZeroWeb")
        threading.Thread(target=push_loop, args=(push, contact, args.verbose), daemon=True).start()
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

    print(f"ScheduleZero server {VERSION} on {scheme}://{args.host}:{args.port}  data={store.dir}"
          + (f"  web={static.root}" if static else "  (sync only, no web app)")
          + (f"  peers={','.join(peers)}" if peers and peer_key else ""), flush=True)
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        pass


if __name__ == "__main__":
    main()
