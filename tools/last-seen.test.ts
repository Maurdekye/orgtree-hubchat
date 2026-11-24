// "Last seen" per kind (Mail hub 2.6.0, feature `last_active`; docket
// hubchat-last-seen-time-for-orgs-people-and-indep): which words show, and
// that hubs without the feature show no time (user 2026-11-24 ma46b9293f2d5).
import test from "node:test";
import assert from "node:assert/strict";
import { ACTIVE_NOW, OLD_HUBS_SHOW_LAST_CALL, lastSeenWords, withSeen, type SeenInput } from "../src/lib/lastseen.ts";

const ago = (t: number) => (t === 1000 ? "5 min ago" : t === 2000 ? "yesterday at 18:02" : "?");
const base: SeenInput = { known: true, online: true, active: false, activeAt: 0, lastCall: 0 };

test("the user's two choices", () => {
  assert.equal(ACTIVE_NOW, "active now");
  assert.equal(OLD_HUBS_SHOW_LAST_CALL, false);
});

test("a hub with last_active: active now, last seen, or nothing", () => {
  assert.deepEqual(lastSeenWords({ ...base, active: true }, ago), { words: "active now", ticks: false });
  assert.deepEqual(lastSeenWords({ ...base, activeAt: 1000 }, ago), { words: "last seen 5 min ago", ticks: true });
  assert.deepEqual(lastSeenWords({ ...base, online: false, activeAt: 2000 }, ago), { words: "last seen yesterday at 18:02", ticks: true });
  assert.equal(lastSeenWords({ ...base, activeAt: 0 }, ago), null, "no time known: none shown");
  assert.deepEqual(lastSeenWords({ ...base, online: false, active: true, activeAt: 1000 }, ago), { words: "last seen 5 min ago", ticks: true },
    "offline is never active now");
  assert.deepEqual(lastSeenWords({ ...base, activeAt: 1000, lastCall: 2000 }, ago)?.words, "last seen 5 min ago", "the old last call never stands in");
});

test("a hub without last_active: no time at all, never the last call", () => {
  const old = { ...base, known: false };
  assert.equal(lastSeenWords({ ...old, online: false, lastCall: 2000 }, ago), null);
  assert.equal(lastSeenWords({ ...old, active: true, activeAt: 1000 }, ago), null);
});

test("the words around a presence", () => {
  assert.deepEqual(withSeen("Online", false, { words: "active now" }), { text: "Online · active now", short: "online · active now" });
  assert.deepEqual(withSeen("Busy", false, { words: "last seen 5 min ago" }), { text: "Busy · last seen 5 min ago", short: "busy · last seen 5 min ago" });
  assert.deepEqual(withSeen("Do not disturb", false, null), { text: "Do not disturb", short: "do not disturb" });
  assert.deepEqual(withSeen("Offline", true, { words: "last seen 5 min ago" }), { text: "Offline · last seen 5 min ago", short: "last seen 5 min ago" });
  assert.deepEqual(withSeen("Offline", true, null), { text: "Offline", short: "offline" });
});
