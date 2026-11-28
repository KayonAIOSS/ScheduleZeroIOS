// ScheduleZero service worker: works offline and shows reminder notifications.
// Pushes from the server carry no text; the text is fetched from /api/push/inbox (one per push).
'use strict';
const SHELL = 'sz-shell-v2';
// Everything is relative to where the app lives (e.g. https://example.com/ScheduleZero/).
const BASE = new URL(self.registration.scope).pathname; // ends with '/'
const at = (p) => BASE + p;
const MAX_ENTRIES = 40;

self.addEventListener('install', (e) => {
  self.skipWaiting();
  e.waitUntil(caches.open(SHELL).then((c) => c.add(BASE)).catch(() => {}));
});

self.addEventListener('activate', (e) => {
  e.waitUntil(
    caches.keys()
      .then((keys) => Promise.all(keys.filter((k) => k.startsWith('sz-shell-') && k !== SHELL).map((k) => caches.delete(k))))
      .then(() => self.clients.claim())
  );
});

async function trim(cache) {
  const keys = await cache.keys();
  for (let i = 0; i < keys.length - MAX_ENTRIES; i++) await cache.delete(keys[i]);
}

self.addEventListener('fetch', (e) => {
  const req = e.request;
  const url = new URL(req.url);
  if (req.method !== 'GET' || url.origin !== self.location.origin || url.pathname.startsWith(at('api/')) || url.pathname.startsWith(at('__sz/'))) return;
  if (req.mode === 'navigate') {
    // Newest page when online; the saved one when offline.
    e.respondWith(
      fetch(req)
        .then((resp) => {
          const copy = resp.clone();
          caches.open(SHELL).then((c) => c.put(BASE, copy));
          return resp;
        })
        .catch(() => caches.match(BASE))
    );
    return;
  }
  // Built files have content hashes in their names, so a saved copy is always right.
  e.respondWith(
    caches.match(req).then((hit) => {
      const net = fetch(req).then((resp) => {
        if (resp.ok) {
          const copy = resp.clone();
          caches.open(SHELL).then((c) => c.put(req, copy).then(() => trim(c)));
        }
        return resp;
      });
      return hit || net;
    })
  );
});

async function session() {
  try {
    const c = await caches.open('sz-session');
    const r = await c.match(at('__sz/session'));
    return r ? await r.json() : null;
  } catch (_) {
    return null;
  }
}

async function onPush() {
  let msg = null;
  const s = await session();
  if (s) {
    try {
      const q = '?device=' + encodeURIComponent(s.deviceId) + '&key=' + encodeURIComponent(s.key);
      const r = await fetch(at('api/push/inbox') + q, { cache: 'no-store' });
      msg = (await r.json()).message;
    } catch (_) {}
  }
  if (!msg) msg = { id: 'sz', title: 'ScheduleZero', body: 'You have a reminder. Open ScheduleZero to see it.', tag: 'sz' };
  const opts = { body: msg.body, tag: msg.tag || msg.id, icon: at('icon-192.png'), badge: at('icon-192.png'), data: { id: msg.id, message: msg.message } };
  if (msg.snoozable) opts.actions = [{ action: 'snooze', title: 'Snooze 15 min' }];
  // Inbox messages: "Mark as read" only. Confirm/deny always happens in the app.
  if (msg.message) opts.actions = [{ action: 'read', title: 'Mark as read' }];
  await self.registration.showNotification(msg.title, opts);
}

self.addEventListener('push', (e) => e.waitUntil(onPush()));

self.addEventListener('notificationclick', (e) => {
  e.notification.close();
  const data = e.notification.data || {};
  if (e.action === 'snooze') {
    e.waitUntil(
      session().then((s) => s && fetch(at('api/push/snooze'), {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ deviceId: s.deviceId, key: s.key, id: data.id }),
      })).catch(() => {})
    );
    return;
  }
  if (e.action === 'read') {
    e.waitUntil(
      session().then((s) => s && fetch(at('api/push/read'), {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ deviceId: s.deviceId, key: s.key, id: data.message }),
      })).catch(() => {})
    );
    return;
  }
  // Tapping a message opens the inbox; tapping a reminder opens the app.
  const target = at(data.message ? '#inbox' : '');
  e.waitUntil(
    self.clients.matchAll({ type: 'window', includeUncontrolled: true }).then((list) => {
      for (const c of list) {
        if ('focus' in c) {
          if (data.message) c.postMessage({ open: 'inbox' });
          return c.focus();
        }
      }
      return self.clients.openWindow(target);
    })
  );
});
