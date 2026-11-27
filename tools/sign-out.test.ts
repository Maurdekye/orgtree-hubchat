// node --test tools/sign-out.test.ts
// Settings › Devices › Sign out of this device (src/lib/signOut.ts): what the confirmation says, and the
// order Hubchat Web leaves in. The hubs are told to forget the push subscription first, while the key
// can still ask; a hub that can't be told never stops the sign-out.
import test from "node:test";
import assert from "node:assert/strict";
import { afterForgettingPush, pushOffTellsHubs, signOutText, stoppedText, unreachedText } from "../src/lib/signOut.ts";

test("the confirmation names what goes, what stays, and the way back", () => {
  const t = signOutText("PC");
  assert.equal(t.action, "Sign out of this device");
  assert.equal(t.title, "Sign out of this device?");
  assert.equal(t.body, "This removes this PC's chats, hubs and settings. Your address keeps working on your other devices, and you can bring it back here later by linking.");
  assert.equal(t.unsaved, "Your recovery words aren't marked as saved. If no other device has your identity, save them first in Settings › Recovery words, or it can't come back.");
  assert.equal(t.ok, "Sign out");
  assert.equal(t.busy, "Signing out…");
  assert.equal(t.cancel, "Keep them");
  assert.match(signOutText("phone").body, /^This removes this phone's chats/);
});

test("the hubs forget the push subscription before the device signs out", async () => {
  const seen: string[] = [];
  await afterForgettingPush(async () => { seen.push("push off"); }, async () => { seen.push("sign out"); });
  assert.deepEqual(seen, ["push off", "sign out"]);
});

test("a hub that can't be told doesn't stop the sign-out", async () => {
  const seen: string[] = [];
  const r = await afterForgettingPush(async () => { throw new Error("hub unreachable"); }, async () => { seen.push("sign out"); return 7; });
  assert.deepEqual(seen, ["sign out"]);
  assert.equal(r, 7);
});

test("a sign-out that fails says so", async () => {
  await assert.rejects(afterForgettingPush(async () => {}, async () => { throw new Error("the key store refused"); }), /the key store refused/);
});

test("hubs that couldn't be told are named after signing out; none, nothing is said", () => {
  assert.equal(unreachedText([], "PC"), null);
  assert.equal(unreachedText(["home"], "PC"), "Signed out. Hubchat couldn't reach home, so it may still list this PC.");
  assert.equal(unreachedText(["home", "office", "https://hub.example"], "phone"),
    "Signed out. Hubchat couldn't reach home, office and https://hub.example, so they may still list this phone.");
});

test("a sign-out that stopped partway offers to finish it", () => {
  const t = stoppedText("browser");
  assert.equal(t.title, "Hubchat couldn't finish signing out");
  assert.equal(t.body, "Signing out of this browser stopped partway: some of its key, chats or settings are still here. Try again to remove them.");
  assert.equal(t.retry, "Try again");
});

test("when nobody knows which hubs heard, it never reads as all told", () => {
  assert.equal(unreachedText([""], "PC"), "Signed out. Hubchat couldn't tell whether its hubs heard, so they may still list this PC.");
});

test("Sign out leaves telling the hubs about push to its budgeted hub step; Start over tells them first", () => {
  assert.equal(pushOffTellsHubs("hc_sign_out"), false);
  assert.equal(pushOffTellsHubs("hc_start_over"), true);
});
