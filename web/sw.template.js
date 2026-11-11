// Hubchat Web's service worker (spec R6.1, "Updates" and "Offline").
// tools/web-sw.mjs writes sw.js from this file at build time, with the build's
// version, a hash of its files, and their list.
//
// It keeps the app's files so Hubchat opens without a network, and holds a
// newer build back until the person taps Update. It never sees the identity
// and never calls a hub (R3.3): requests to other origins pass untouched.
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

self.addEventListener("message", (e) => {
  if (e.data === "update") void self.skipWaiting();
  else if (e.data === "version" && e.ports[0]) e.ports[0].postMessage({ version: VERSION, build: BUILD });
});
