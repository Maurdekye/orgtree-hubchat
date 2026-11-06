import test from "node:test";
import assert from "node:assert/strict";
import { Outgoing, toMessage, withLocal, type Deps } from "../src/lib/outgoing.ts";
import type { Message, NewOutgoing } from "../src/api";

type Gate = { promise: Promise<void>; resolve: () => void; reject: (e: Error) => void };
const gate = (): Gate => {
  let resolve!: () => void, reject!: (e: Error) => void;
  const promise = new Promise<void>((a, b) => { resolve = a; reject = b; });
  return { promise, resolve, reject };
};
const out = (id: string, extra: Partial<NewOutgoing> = {}): NewOutgoing => ({ id, peer: "p.abc", body: "hello " + id, ...extra });
const stored = (id: string): Message =>
  ({ id, peer: "p.abc", outgoing: true, hub: null, body: "b", kind: null, reply_to: null, sent_at: null, received_at: null, created_at: "2026-11-06T16:00:00Z", state: "queued",
     fetched_at: null, delivered_at: null, read_at: null, error: null, seen: true, attachments: [] });

function rig() {
  const gates: Gate[] = [];
  const sends: string[] = [];
  const drafts: string[] = [];
  const changes = { n: 0 };
  const events: string[] = [];
  const deps: Deps = {
    saveDraft: async (_p, body) => { events.push("draft"); drafts.push(body); },
    send: (m) => { events.push("send"); sends.push(m.id); const g = gate(); gates.push(g); return g.promise; },
    sentOk: async () => {},
    now: () => "2026-11-06T16:00:00Z",
    changed: () => { events.push("changed"); changes.n++; },
  };
  return { o: new Outgoing(deps), gates, sends, drafts, changes, events };
}
const tick = () => new Promise((r) => setTimeout(r, 0));

test("OB1 Enter puts the bubble in the list before the send promise resolves", async () => {
  const { o, gates, sends, changes } = rig();
  o.submit(out("m1"));
  assert.equal(o.rows("p.abc").length, 1, "the row exists the moment submit returns");
  assert.equal(o.rows("p.abc")[0].phase, "sending");
  assert.ok(changes.n >= 1, "the UI was told");
  assert.deepEqual(withLocal([], o.rows("p.abc")).map((m) => [m.id, m.state, m.body]), [["m1", "queued", "hello m1"]]);
  await tick(); await tick();
  assert.deepEqual(sends, ["m1"], "the send ran after the render, not before");
  assert.equal(gates.length, 1, "and is still pending");
  assert.equal(o.rows("p.abc")[0].sent, false);
});

test("OB2 a reply bubble carries its quote ids at once (reply_to, replies, answers) and attachments by name", () => {
  const { o } = rig();
  o.submit(out("m2", { reply_to: "a1", replies: ["a1", "a2"], answers: [{ question: "q1", picked: ["yes"] }], attachments: [{ name: "pic.png", bytes: 9, source: "C:\\x\\pic.png" }] }));
  const m = withLocal([], o.rows("p.abc"))[0];
  assert.equal(m.reply_to, "a1");
  assert.deepEqual(m.replies, ["a1", "a2"]);
  assert.deepEqual(m.answers, [{ question: "q1", picked: ["yes"] }]);
  assert.deepEqual(m.attachments.map((a) => [a.name, a.bytes, a.source, a.state]), [["pic.png", 9, "C:\\x\\pic.png", "pending"]]);
  assert.equal(m.outgoing, true);
});

test("OB3 when the core has it and a read returns it, the stored row replaces the local one: no duplicate, no flicker", async () => {
  const { o, gates } = rig();
  o.submit(out("m3"));
  await tick(); await tick();
  gates[0].resolve(); await tick(); await tick();
  assert.equal(o.rows("p.abc")[0].sent, true, "sent but still drawn until a read returns it");
  const asRead = [stored("m1"), stored("m3")];
  assert.deepEqual(withLocal(asRead, o.rows("p.abc")).map((m) => m.id), ["m1", "m3"], "an id the read has is not drawn twice");
  assert.equal(withLocal(asRead, o.rows("p.abc")), asRead, "and the stored list is returned as it is");
  o.settle("p.abc", ["m1", "m3"]);
  assert.equal(o.rows("p.abc").length, 0);
  o.submit(out("m4")); await tick(); await tick();
  o.settle("p.abc", ["m4"]);
  assert.equal(o.rows("p.abc").length, 1, "a row whose send has not resolved is never settled by a read that happens to name its id");
});

test("OB4 a send that fails leaves a failed bubble; Retry sends once more with the same id; no duplicate when it works", async () => {
  const { o, gates, sends } = rig();
  o.submit(out("m5"));
  await tick(); await tick();
  gates[0].reject(new Error("no space"));
  await tick(); await tick();
  const failed = o.rows("p.abc")[0];
  assert.equal(failed.phase, "failed");
  assert.equal(failed.error, "no space");
  const m = withLocal([], o.rows("p.abc"))[0];
  assert.deepEqual([m.state, m.error], ["failed", "no space"]);
  o.retry("m5"); o.retry("m5");   // a second click while it is sending does nothing
  await tick(); await tick();
  assert.deepEqual(sends, ["m5", "m5"], "exactly one more send, with the same id");
  assert.equal(o.rows("p.abc").length, 1, "still one bubble");
  assert.equal(o.rows("p.abc")[0].phase, "sending");
  gates[1].resolve(); await tick(); await tick();
  o.settle("p.abc", ["m5"]);
  assert.equal(o.rows("p.abc").length, 0);
});

test("OB5 Discard removes the bubble and hands back the message (text, reply target, attachments) for an empty composer", async () => {
  const { o, gates } = rig();
  const msg = out("m6", { reply_to: "a1", attachments: [{ name: "f", bytes: 1, source: "s" }] });
  o.submit(msg);
  await tick(); await tick();
  gates[0].reject(new Error("x")); await tick(); await tick();
  assert.deepEqual(o.discard("m6"), msg);
  assert.equal(o.rows("p.abc").length, 0);
  assert.equal(o.discard("m6"), null, "a second discard finds nothing");
  o.retry("m6");
  await tick();
  assert.equal(o.rows("p.abc").length, 0, "and a retry of a discarded message does nothing");
});

test("OB6 a late failure of a discarded or cleared message touches nothing", async () => {
  const { o, gates } = rig();
  o.submit(out("m7"));
  await tick(); await tick();
  o.discard("m7");
  gates[0].reject(new Error("late"));
  await tick(); await tick();
  assert.equal(o.rows("p.abc").length, 0);
  o.submit(out("m8")); await tick(); await tick();
  o.clear();
  gates[1].resolve(); await tick(); await tick();
  assert.equal(o.rows("p.abc").length, 0, "an identity switch drops local rows");
});

test("OB7 rows survive switching chats, per chat; the list reference is stable until it changes", async () => {
  const { o } = rig();
  o.submit(out("m9"));
  o.submit({ ...out("n1"), peer: "q.def" });
  const a = o.rows("p.abc");
  assert.equal(o.rows("p.abc"), a, "same reference");
  assert.equal(o.rows("q.def").length, 1);
  assert.equal(o.rows("none").length, 0);
  assert.equal(withLocal([], o.rows("none")).length, 0);
  const empty: Message[] = [];
  assert.equal(withLocal(empty, o.rows("none")), empty, "no locals: the same array back");
});

test("OB8 submitting the same id twice makes one bubble and one send", async () => {
  const { o, sends } = rig();
  o.submit(out("m10")); o.submit(out("m10"));
  await tick(); await tick();
  assert.equal(o.rows("p.abc").length, 1);
  assert.deepEqual(sends, ["m10"]);
});

test("OB9 the UI is told before any core call; the draft is saved before the send (crash recovery)", async () => {
  const { o, drafts, gates, events } = rig();
  o.submit(out("m11"));
  await tick(); await tick();
  assert.deepEqual(events.slice(0, 3), ["changed", "draft", "send"], "told first, then draft, then send");
  assert.deepEqual(drafts, ["hello m11"]);
  assert.equal(gates.length, 1);
  assert.equal(toMessage(o.rows("p.abc")[0]).created_at, "2026-11-06T16:00:00Z");
});

test("OB10 while a local row holds a message, the recovery draft that is that same text is not restored on chat return; other text still is", async () => {
  const { o, gates } = rig();
  o.submit(out("m12", { body: "pending text" }));
  assert.equal(o.holdsBody("p.abc", "pending text"), true, "the saved recovery draft is the sent text: it must not refill the box");
  assert.equal(o.holdsBody("p.abc", "something new typed since"), false, "a newer draft is restored as before");
  assert.equal(o.holdsBody("q.def", "pending text"), false, "per chat");
  await tick(); await tick();
  gates[0].resolve(); await tick(); await tick();
  assert.equal(o.holdsBody("p.abc", "pending text"), true, "still held after the core has it, until a read returns it");
  o.settle("p.abc", ["m12"]);
  assert.equal(o.holdsBody("p.abc", "pending text"), false, "the row is gone: the draft was cleared with it");
  o.submit(out("m13", { body: "again" })); await tick(); await tick();
  gates[1].reject(new Error("x")); await tick(); await tick();
  assert.equal(o.holdsBody("p.abc", "again"), true, "a failed row still holds it (Discard restores it deliberately)");
  o.discard("m13");
  assert.equal(o.holdsBody("p.abc", "again"), false);
});
