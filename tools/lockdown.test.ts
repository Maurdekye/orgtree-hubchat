// Lockdown (Mail hub 2.6.0; Orgtree docket org-mailbox-lockdown-allow-list-mode-that-denies, user 2026-11-25
// 13:26Z: "a little lock icon in the corner of their status while enabled to anyone with access, and to anyone
// without access, their whole status symbol is replaced with a lock, and they cannot be messaged").
import test from "node:test";
import assert from "node:assert/strict";
import { LOCKED_TEXT, answerOptionsShown, canMessage, lockBadge, lockBarText, lockedPresence, ownLockdownText } from "../src/lib/lockdown.ts";

test("closed: the lock replaces the status, and they can't be messaged", () => {
  assert.deepEqual(lockedPresence({ locked: "closed" }), { state: "locked", text: LOCKED_TEXT, short: "in lockdown", ticks: false });
  assert.equal(canMessage({ locked: "closed" }), false);
  assert.equal(lockBadge({ locked: "closed" }), false, "no corner lock: the whole symbol is the lock");
});

test("open: the status stays, with a small lock in the corner", () => {
  assert.equal(lockedPresence({ locked: "open" }), null);
  assert.equal(canMessage({ locked: "open" }), true);
  assert.equal(lockBadge({ locked: "open" }), true);
});

test("not in lockdown, or unknown: as before", () => {
  for (const c of [{}, { locked: null }, undefined]) {
    assert.equal(lockedPresence(c), null);
    assert.equal(canMessage(c), true);
    assert.equal(lockBadge(c), false);
  }
});

test("the words: the bar names them as the hub's refusal does; our own lockdown names its hubs", () => {
  assert.equal(LOCKED_TEXT, "In lockdown: you can't message them");
  assert.equal(lockBarText("Acme"), "@Acme is in lockdown and only takes messages from addresses on its list");
  assert.equal(ownLockdownText([]), null);
  assert.equal(ownLockdownText(["Home hub"]), "Your address is in lockdown on Home hub: only addresses on its list can message you.");
  assert.equal(ownLockdownText(["A", "B"]), "Your address is in lockdown on A and B: only addresses on its list can message you.");
});

test("a closed contact's open question shows no answer buttons; an answered one still shows what was picked", () => {
  // review I8 + capture C2 (@integ-sol-d m1066d517fa86: disabled buttons are still buttons)
  assert.equal(answerOptionsShown({ settled: false, closed: true }), false);
  assert.equal(answerOptionsShown({ settled: true, closed: true }), true);
  assert.equal(answerOptionsShown({ settled: false, closed: false }), true);
});
