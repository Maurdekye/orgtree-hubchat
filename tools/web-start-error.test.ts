// node --test tools/web-start-error.test.ts
// What Hubchat Web's start screen says when its core does not start
// (src/web/startError.ts): a database from a newer build asks for the
// server to be updated; anything else is the plain failure with Reload.
import test from "node:test";
import assert from "node:assert/strict";
import { startFailure } from "../src/web/startError.ts";

const NEWER = "local store: this database was written by a newer Hubchat (its schema is 21, this build knows 19)";

test("a database from a newer build: update the server, nothing is lost", () => {
  const f = startFailure(NEWER, "https://chat.example", "1.4.0");
  assert.equal(f.title, "Update the Hubchat Web server");
  assert.equal(f.text,
    "This browser last used a newer Hubchat Web than the one https://chat.example serves now (version 1.4.0). "
    + "Ask whoever runs that server to update it, then reload. Your identity and messages are still here, untouched.");
  assert.equal(f.bad, false);
  assert.equal(f.action, "Reload");
});

test("it is known by the core's own words, wherever they stand in the message", () => {
  for (const m of [NEWER, "Error: " + NEWER, new Error(NEWER)]) {
    assert.equal(startFailure(m, "https://chat.example", "1.4.0").title, "Update the Hubchat Web server");
  }
});

test("without a version the sentence still reads", () => {
  assert.equal(startFailure(NEWER, "https://chat.example", "").text,
    "This browser last used a newer Hubchat Web than the one https://chat.example serves now. "
    + "Ask whoever runs that server to update it, then reload. Your identity and messages are still here, untouched.");
});

test("anything else is the plain failure", () => {
  for (const m of ["This browser didn't give Hubchat its storage (NoModificationAllowedError).", "local store: disk I/O error", new Error("boom")]) {
    const f = startFailure(m, "https://chat.example", "1.4.0");
    assert.equal(f.title, undefined);
    assert.equal(f.text, "Hubchat couldn't start its core: " + (m instanceof Error ? m.message : m));
    assert.equal(f.bad, true);
    assert.equal(f.action, "Reload");
  }
});
