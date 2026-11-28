// Small browser glue that the WebAssembly app calls by name (see src/browser.rs `glue`).
// The app itself (screens, sync, reminders logic) is Rust; this file only wraps APIs that are
// awkward to reach from Rust: service worker registration, push subscriptions and install prompts.
(function () {
  'use strict';
  let installEvent = null;
  window.addEventListener('beforeinstallprompt', (e) => { e.preventDefault(); installEvent = e; });
  window.addEventListener('appinstalled', () => { installEvent = null; });

  window.szRegisterServiceWorker = () => {
    if ('serviceWorker' in navigator) navigator.serviceWorker.register('/sw.js', { scope: '/' }).catch(() => {});
  };
  window.szIsStandalone = () =>
    window.matchMedia('(display-mode: standalone)').matches || window.navigator.standalone === true;
  window.szIsIOS = () =>
    /iPhone|iPad|iPod/.test(navigator.userAgent) || (navigator.platform === 'MacIntel' && navigator.maxTouchPoints > 1);
  window.szPushSupported = () => 'serviceWorker' in navigator && 'PushManager' in window && 'Notification' in window;
  window.szPermission = () => ('Notification' in window ? Notification.permission : 'unsupported');
  window.szCanInstall = () => installEvent !== null;
  window.szInstall = () => {
    if (!installEvent) return;
    installEvent.prompt();
    installEvent = null;
  };

  function keyBytes(b64) {
    const s = (b64 + '='.repeat((4 - (b64.length % 4)) % 4)).replace(/-/g, '+').replace(/_/g, '/');
    const raw = atob(s);
    const out = new Uint8Array(raw.length);
    for (let i = 0; i < raw.length; i++) out[i] = raw.charCodeAt(i);
    return out;
  }

  function sameKey(a, b) {
    if (!a || !b) return false;
    const x = new Uint8Array(a), y = new Uint8Array(b);
    return x.length === y.length && x.every((v, i) => v === y[i]);
  }

  // Must run inside a tap: asks for permission first (synchronously), then subscribes.
  // Resolves to the subscription as JSON, or '' if refused/unavailable.
  window.szSubscribe = (publicKey) => {
    if (!window.szPushSupported()) return Promise.resolve('');
    const asked = Notification.requestPermission();
    return (async () => {
      const perm = await asked;
      if (perm !== 'granted') return '';
      let key = publicKey;
      if (!key) {
        const r = await fetch('/api/push/key', { cache: 'no-store' });
        key = (await r.json()).publicKey;
      }
      const reg = await navigator.serviceWorker.ready;
      const wanted = keyBytes(key);
      let sub = await reg.pushManager.getSubscription();
      if (sub && !sameKey(sub.options && sub.options.applicationServerKey, wanted.buffer)) {
        await sub.unsubscribe();
        sub = null;
      }
      if (!sub) sub = await reg.pushManager.subscribe({ userVisibleOnly: true, applicationServerKey: wanted });
      return JSON.stringify(sub.toJSON());
    })().catch(() => '');
  };

  window.szCurrentSubscription = async () => {
    try {
      if (!window.szPushSupported()) return '';
      const reg = await navigator.serviceWorker.getRegistration('/');
      const sub = reg && (await reg.pushManager.getSubscription());
      return sub ? JSON.stringify(sub.toJSON()) : '';
    } catch (_) {
      return '';
    }
  };

  // The service worker reads this to fetch reminder text when a push arrives.
  window.szSaveSession = async (json) => {
    try {
      const c = await caches.open('sz-session');
      await c.put('/__sz/session', new Response(json, { headers: { 'Content-Type': 'application/json' } }));
    } catch (_) {}
    return '';
  };
})();
