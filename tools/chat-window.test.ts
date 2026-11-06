import test from "node:test";
import assert from "node:assert/strict";
import { mergeWindow } from "../src/lib/chatwindow.ts";
import type { Message } from "../src/api";

const msg = (i: number, state = "sent"): Message =>
  ({ id: `m${String(i).padStart(3, "0")}`, peer: "p.abcdef", outgoing: true, created_at: `2026-11-01T00:00:${String(i).padStart(2, "0")}Z`, state, body: `b${i}` }) as unknown as Message;
const range = (a: number, b: number, state = "sent") => Array.from({ length: b - a + 1 }, (_, k) => msg(a + k, state));

test("a new row is appended and rows older than the window are the same objects", () => {
  const prev = range(1, 10);
  const out = mergeWindow(prev, range(8, 11), 4);
  assert.deepEqual(out.map((m) => m.id), range(1, 11).map((m) => m.id));
  for (let i = 0; i < 7; i++) assert.equal(out[i], prev[i]);
});

test("a receipt on a recent row replaces it", () => {
  const prev = range(1, 10);
  const out = mergeWindow(prev, [...range(7, 9), msg(10, "read")], 4);
  assert.equal(out[9].state, "read");
  assert.equal(out.length, 10);
});

test("a row deleted inside the window disappears; older rows stay", () => {
  const prev = range(1, 10);
  const out = mergeWindow(prev, [msg(7), msg(8), msg(10)].concat(), 4);
  // fewer than the limit came back: that is the whole chat
  assert.deepEqual(out.map((m) => m.id), ["m007", "m008", "m010"]);
  const full = mergeWindow(prev, [msg(6), msg(7), msg(8), msg(10)], 4);
  assert.deepEqual(full.map((m) => m.id), ["m001", "m002", "m003", "m004", "m005", "m006", "m007", "m008", "m010"]);
});

test("fewer rows than the limit are the whole chat", () => {
  const out = mergeWindow(range(1, 3), range(1, 2), 4);
  assert.deepEqual(out.map((m) => m.id), ["m001", "m002"]);
});
