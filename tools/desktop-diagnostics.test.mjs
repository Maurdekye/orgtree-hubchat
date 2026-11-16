// Headless real-module tests; read dependencies in place, with no links/copies.
import test from "node:test";
import assert from "node:assert/strict";
import { createRequire } from "node:module";
import vm from "node:vm";
import path from "node:path";
const require = createRequire(path.resolve(process.env.HUBCHAT_DEPENDENCY_ROOT || ".", "package.json"));
const { build } = require("esbuild");

async function page(entry, { android = false, web = false } = {}) {
  const calls = [], timers = [], listeners = new Map();
  const document = { visibilityState: "visible", addEventListener: (n, f) => listeners.set(n, f) };
  const fixture = { calls, check: async () => null, androidCheck: async () => null };
  const result = await build({
    entryPoints: [entry], bundle: true, write: false, format: "cjs", platform: "node",
    plugins: [{ name: "native-fixture", setup(b) {
      b.onResolve({ filter: /^(react|.*\/api|.*\/native|@tauri-apps\/.*)$/ }, a => ({ path: a.path, namespace: "fixture" }));
      b.onLoad({ filter: /.*/, namespace: "fixture" }, ({ path: p }) => ({ contents:
        p === "react" ? "export const useSyncExternalStore = () => {};" :
        p === "@tauri-apps/api/core" ? "export const invoke = (c, a) => { fixture.calls.push([c, a]); return Promise.reject('ignored'); };" :
        p === "@tauri-apps/api/app" ? "export const getIdentifier = async () => 'dev.hubchat';" :
        p === "@tauri-apps/plugin-updater" ? "export const check = () => fixture.check();" :
        p.endsWith("/api") ? `export const isTauri = true, isWeb = ${web}; export const api = { appUpdateCheck: () => fixture.androidCheck() };` :
        p.endsWith("/native") ? "export const errText = e => e instanceof Error ? e.message : String(e); export const webUpdate = () => fixture.check();" :
        "export const relaunch = async () => {};"
      }));
    } }],
  });
  const module = { exports: {} };
  vm.runInNewContext(result.outputFiles[0].text, {
    module, exports: module.exports, fixture, document, Error,
    localStorage: { getItem: () => "0" },
    setInterval: (f, ms) => { timers.push({ f, ms }); return timers.length; },
  });
  return { api: module.exports, calls, timers, fixture, document, hide() { document.visibilityState = "hidden"; listeners.get("visibilitychange")?.(); }, show() { document.visibilityState = "visible"; listeners.get("visibilitychange")?.(); }, android };
}

test("updateFailureClass uses fixed classes and first-match order", async () => {
  const { api } = await page("src/lib/updates.ts");
  for (const text of ["SIGNATURE status code 503", "minisign", "verify JSON"]) assert.equal(api.updateFailureClass(text), "signature");
  for (const text of ["status code unavailable JSON", "HTTP 404 network"]) assert.equal(api.updateFailureClass(text), "status");
  for (const text of ["deserialize", "parse timeout", "JSON", "invalid type", "missing field"]) assert.equal(api.updateFailureClass(text), "feed");
  for (const text of ["error sending request", "connect", "dns", "timed out", "timeout", "network", "unreachable", "os error"]) assert.equal(api.updateFailureClass(new Error(text)), "offline");
  assert.equal(api.updateFailureClass("unknown private words"), "other");
  assert.equal(api.updateFailureClass(null), "other");
  assert.equal(api.updateFailureClass("14040"), "other");
});

test("failed desktop checks log once, coalesced asked-for checks say asked, success is silent", async () => {
  const p = await page("src/lib/updates.ts");
  let reject;
  p.fixture.check = () => new Promise((_, no) => { reject = no; });
  const first = p.api.checkForUpdate(true);
  const second = p.api.checkForUpdate();
  assert.equal(first, second);
  for (let i = 0; i < 20 && !reject; i++) await Promise.resolve();
  reject(new Error("signature private text"));
  await assert.rejects(first);
  assert.equal(JSON.stringify(p.calls), JSON.stringify([["log_update_check_failed", { class: "signature", asked: true }]]));
  p.fixture.check = async () => { throw "network private text"; };
  await assert.rejects(p.api.checkForUpdate(true));
  assert.equal(JSON.stringify(p.calls[1]), JSON.stringify(["log_update_check_failed", { class: "offline", asked: false }]));
  p.fixture.check = async () => null;
  await p.api.checkForUpdate();
  assert.equal(p.calls.length, 2);
});

test("Android and Web failed checks never call desktop logging", async () => {
  for (const opts of [{ android: true }, { web: true }]) {
    const p = await page("src/lib/updates.ts", opts);
    p.api.startUpdateChecks(opts.android ? "android" : "desktop");
    p.fixture.check = p.fixture.androidCheck = async () => { throw "network"; };
    await assert.rejects(p.api.checkForUpdate());
    assert.equal(p.calls.length, 0);
  }
});

test("window beat ticks every 5 s while visible and sends one hidden beat", async () => {
  const p = await page("src/lib/beat.ts");
  p.api.startBeats("desktop");
  p.api.startBeats("desktop");
  assert.equal(p.timers.length, 1);
  assert.equal(p.timers[0].ms, 5000);
  p.timers[0].f();
  assert.equal(p.calls.at(-1)[0], "ui_beat");
  assert.equal(p.calls.at(-1)[1].visible, true);
  p.hide();
  const n = p.calls.length;
  assert.equal(p.calls.at(-1)[1].visible, false);
  p.timers[0].f(); p.timers[0].f();
  assert.equal(p.calls.length, n);
  p.show();
  assert.equal(p.calls.length, n + 1);
  assert.equal(p.calls.at(-1)[1].visible, true);
  await Promise.resolve(); // rejected invokes must be swallowed
});

test("window beat never starts on Android or Web", async () => {
  for (const opts of [{ android: true }, { web: true }]) {
    const p = await page("src/lib/beat.ts", opts);
    p.api.startBeats(opts.android ? "android" : "desktop");
    p.hide(); p.show();
    assert.equal(p.timers.length, 0);
    assert.equal(p.calls.length, 0);
  }
});
