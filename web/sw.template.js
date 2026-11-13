// Hubchat Web's service worker (spec R6.1, "Updates" and "Offline").
// tools/web-sw.mjs writes sw.js from this file at build time, with the build's
// version, a hash of its files, and their list.
//
// It keeps the app's files so Hubchat opens without a network, and holds a
// newer build back until the person taps Update. It never sees the identity
// and never calls a hub (R3.3): requests to other origins pass untouched.
// It also shows the notifications a hub pushes (R4.6): what to show comes
// in the push itself, already worded by the hub.
const VERSION = "__VERSION__";
const BUILD = "__BUILD__";
const FILES = __FILES__;
const CACHE = "hubchat-web-" + BUILD;
const at = (path) => new URL(path, self.registration.scope).href;

self.addEventListener("install", (e) => {
  // no skipWaiting: a newer build waits until the app asks for it
  e.waitUntil(caches.open(CACHE).then((c) => c.addAll(FILES.map(at))));
});

self.addEventListener("activate", (e) => {
  e.waitUntil((async () => {
    for (const k of await caches.keys()) if (k.startsWith("hubchat-web-") && k !== CACHE) await caches.delete(k);
    await self.clients.claim();
  })());
});

/** The server's settings: fresh when it answers, else the last copy. */
async function settings(request) {
  const c = await caches.open(CACHE);
  try {
    const r = await fetch(request);
    if (r.ok) await c.put(at("config.json"), r.clone());
    return r;
  } catch {
    return (await c.match(at("config.json"))) || Response.error();
  }
}

self.addEventListener("fetch", (e) => {
  const r = e.request;
  if (r.method !== "GET") return;
  const scope = new URL(self.registration.scope);
  const u = new URL(r.url);
  if (u.origin !== scope.origin || !u.pathname.startsWith(scope.pathname)) return;
  const rel = u.pathname.slice(scope.pathname.length);
  if (rel === "config.json") { e.respondWith(settings(r)); return; }
  // the app is one page: every navigation inside it is index.html
  const file = r.mode === "navigate" || rel === "" ? "index.html" : rel;
  if (!FILES.includes(file)) return;
  e.respondWith(caches.open(CACHE).then(async (c) => (await c.match(at(file))) || fetch(r)));
});

/** What a push says: the hub's JSON (docs: Mail hub, "Web Push for browsers"). Anything else, or
 *  nothing: a plain notice. A push ALWAYS shows a notification: a browser takes the subscription
 *  away from an app whose pushes show nothing. */
function notice(data) {
  let n = null, badge = null;
  try {
    const p = data ? data.json() : null;
    if (p && p.web_push === 8030 && p.notification && typeof p.notification.title === "string") { n = p.notification; badge = p.app_badge; }
  } catch { /* not JSON */ }
  const scope = self.registration.scope;
  // only this app's own pages are opened, whatever the push says
  const to = n && typeof n.navigate === "string" && n.navigate.startsWith(scope) ? n.navigate : scope;
  return {
    title: n ? n.title : "Hubchat",
    options: {
      body: n && typeof n.body === "string" ? n.body : "New message",
      tag: n && typeof n.tag === "string" ? n.tag : "hc-new",
      // a chat's next message replaces its notification (the tag); it alerts again, where a browser
      // would otherwise swap it in silently, with no banner and no sound
      renotify: true,
      icon: at("icons/icon-192.png"),
      data: { navigate: to },
    },
    badge: typeof badge === "number" ? badge : null,
  };
}

self.addEventListener("push", (e) => {
  const n = notice(e.data);
  e.waitUntil((async () => {
    await self.registration.showNotification(n.title, n.options);
    try { if (n.badge !== null && self.navigator.setAppBadge) await self.navigator.setAppBadge(n.badge); } catch { /* no badge here */ }
  })());
});

// A tapped notification: the open app comes to the front and opens the chat; else the app opens there.
self.addEventListener("notificationclick", (e) => {
  e.notification.close();
  const to = (e.notification.data && e.notification.data.navigate) || self.registration.scope;
  e.waitUntil((async () => {
    const open = await self.clients.matchAll({ type: "window", includeUncontrolled: true });
    const mine = open.find((c) => c.url.startsWith(self.registration.scope));
    if (mine) {
      mine.postMessage({ t: "open", navigate: to });
      try { await mine.focus(); } catch { /* the browser decides */ }
      return;
    }
    await self.clients.openWindow(to);
  })());
});

// The browser replaced or dropped the subscription: an open app makes a new one and tells the
// hubs; a closed one does at its next start (src/web/push.ts).
self.addEventListener("pushsubscriptionchange", (e) => {
  e.waitUntil((async () => {
    for (const c of await self.clients.matchAll({ type: "window", includeUncontrolled: true })) c.postMessage({ t: "push-changed" });
  })());
});

self.addEventListener("message", (e) => {
  if (e.data === "update") void self.skipWaiting();
  else if (e.data === "version" && e.ports[0]) e.ports[0].postMessage({ version: VERSION, build: BUILD });
});
