// node --test tools/web-sw-push.test.mjs
// What Hubchat Web's service worker (web/sw.template.js) does with a push:
// the worker's own script, run here with a stand-in for the browser around
// it. The browser test (tools/web-e2e.mjs) proves the hub sends the push;
// its headless browser shows no notifications, so the showing is proven
// here.
import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import vm from "node:vm";

const SCOPE = "https://chat.example/app/";
const source = readFileSync(new URL("../web/sw.template.js", import.meta.url), "utf8")
  .replace("__VERSION__", "1.0.0").replace("__BUILD__", "test").replace("__FILES__", '["index.html"]');

/** The worker, started: its listeners, and what it showed, opened and told its pages. */
function worker(windows = []) {
  const handlers = {}, shown = [], opened = [], badges = [];
  const self = {
    addEventListener: (name, f) => { handlers[name] = f; },
    registration: { scope: SCOPE, showNotification: async (title, options) => { shown.push({ title, ...options }); } },
    clients: { matchAll: async () => windows, openWindow: async (url) => { opened.push(url); }, claim: async () => {} },
    navigator: { setAppBadge: async (n) => { badges.push(n); } },
    skipWaiting: async () => {},
  };
  vm.runInNewContext(source, { self, URL, caches: {}, fetch: async () => { throw new Error("no network in this test"); }, Response: { error: () => null } });
  /** Fire one event and wait for what it asked the browser to wait for. */
  const fire = async (name, event) => { let waited = Promise.resolve(); handlers[name]({ ...event, waitUntil: (p) => { waited = p; } }); await waited; };
  return { fire, shown, opened, badges };
}
// what the worker made lives in its own realm: compared as plain data
const plain = (v) => JSON.parse(JSON.stringify(v));
const data = (value) => ({ json: () => { if (typeof value === "string") throw new SyntaxError("not JSON"); return value; } });
const HUB = { web_push: 8030, notification: { title: "Ann Example", body: "Are we still on for 3?", navigate: SCOPE + "#/chat/ann.3f9c2a", tag: "hc-ann.3f9c2a", silent: false }, app_badge: 3 };

test("a hub's push shows as the notification it describes, and sets the badge", async () => {
  const w = worker();
  await w.fire("push", { data: data(HUB) });
  assert.deepEqual(plain(w.shown), [{ title: "Ann Example", body: "Are we still on for 3?", tag: "hc-ann.3f9c2a", icon: SCOPE + "icons/icon-192.png", data: { navigate: SCOPE + "#/chat/ann.3f9c2a" } }]);
  assert.deepEqual(w.badges, [3]);
});

test("every push shows a notification: one it cannot read, an empty one, one that is not a hub's", async () => {
  for (const d of [data("garbage"), null, data({ hello: 1 }), data({ web_push: 8030 }), data({ web_push: 1, notification: HUB.notification })]) {
    const w = worker();
    await w.fire("push", { data: d });
    assert.deepEqual(plain(w.shown), [{ title: "Hubchat", body: "New message", tag: "hc-new", icon: SCOPE + "icons/icon-192.png", data: { navigate: SCOPE } }]);
    assert.deepEqual(w.badges, []);
  }
});

test("a notification opens only this app's own pages, whatever the push names", async () => {
  for (const elsewhere of ["https://evil.example/", "https://chat.example/other/", "javascript:alert(1)", 7]) {
    const w = worker();
    await w.fire("push", { data: data({ ...HUB, notification: { ...HUB.notification, navigate: elsewhere } }) });
    assert.equal(w.shown[0].data.navigate, SCOPE);
    assert.equal(w.shown[0].title, "Ann Example");
  }
});

test("tapped with the app open: that window is told which chat, and comes to the front", async () => {
  const told = [];
  let focused = 0;
  const other = { url: "https://chat.example/other/", postMessage: () => { throw new Error("not this app"); }, focus: async () => {} };
  const mine = { url: SCOPE, postMessage: (m) => told.push(m), focus: async () => { focused++; } };
  const w = worker([other, mine]);
  let closed = 0;
  await w.fire("notificationclick", { notification: { close: () => { closed++; }, data: { navigate: SCOPE + "#/chat/ann.3f9c2a" } } });
  assert.deepEqual(plain(told), [{ t: "open", navigate: SCOPE + "#/chat/ann.3f9c2a" }]);
  assert.equal(focused, 1);
  assert.equal(closed, 1);
  assert.deepEqual(w.opened, []);
});

test("tapped with the app closed: the app opens at that chat", async () => {
  const w = worker([]);
  await w.fire("notificationclick", { notification: { close: () => {}, data: { navigate: SCOPE + "#/chat/ann.3f9c2a" } } });
  assert.deepEqual(w.opened, [SCOPE + "#/chat/ann.3f9c2a"]);
  const bare = worker([]);
  await bare.fire("notificationclick", { notification: { close: () => {}, data: null } });
  assert.deepEqual(bare.opened, [SCOPE]);
});

test("a subscription the browser changed: every open window is told", async () => {
  const told = [];
  const w = worker([{ url: SCOPE, postMessage: (m) => told.push(m) }, { url: SCOPE + "?2", postMessage: (m) => told.push(m) }]);
  await w.fire("pushsubscriptionchange", {});
  assert.deepEqual(plain(told), [{ t: "push-changed" }, { t: "push-changed" }]);
});
