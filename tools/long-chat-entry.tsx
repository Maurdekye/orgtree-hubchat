// Loaded only by tools/long-chat-bench.cjs (Electron, offscreen, Session 0):
// the real Conversation with a stubbed api and a synthetic chat of N
// messages (text, mentions, images, files, question cards, replies). Measures
// what a long loaded history costs: opening, loading older pages, a keystroke
// in the composer, the peer's typing heartbeat and a new incoming message.
// Nothing is opened: no network, no clipboard, no media, no real core.
import { createElement, Profiler, type ProfilerOnRenderCallback } from "react";
import { createRoot } from "react-dom/client";
import { api, type HcEvent, type Message } from "../src/api";
import { Conversation } from "../src/components/Conversation";
import { PlatformCtx } from "../src/components/ui";
import * as store from "../src/lib/store";

const PEER = "bench.org.1a2b3c";
const ME = "bench.me.123456";
const T0 = Date.parse("2026-01-01T00:00:00Z");

/** Message i of the synthetic chat: a fixed mix, the same on every run. */
function make(i: number): Message {
  const kind = i % 20;
  const outgoing = i % 3 === 0;
  const id = "m" + String(i).padStart(6, "0");
  const at = new Date(T0 + i * 37_000).toISOString();
  const base: Message = {
    id, peer: PEER, outgoing, hub: "http://hub.invalid", kind: null, reply_to: null, sent_at: at, received_at: at, created_at: at,
    state: outgoing ? "read" : "received", fetched_at: null, delivered_at: at, read_at: at, error: null, seen: true, attachments: [],
    body: `Message ${i}: ` + (kind % 4 === 0
      ? "a longer paragraph about **the build**, with `code`, a link https://example.invalid/x and a mention of @net:bench.other.9z9z9z. ".repeat(3)
      : kind % 4 === 1 ? "short reply" : kind % 4 === 2 ? "- one\n- two\n- three" : "See @item:bench/some-item please"),
  };
  if (kind === 5) base.attachments = [{ local_id: "a" + i, hub_id: "h" + i, name: `photo-${i}.png`, bytes: 120_000, source: null, local_path: null, state: "remote", error: null }];
  if (kind === 9) base.attachments = [{ local_id: "a" + i, hub_id: "h" + i, name: `report-${i}.pdf`, bytes: 3_400_000, source: null, local_path: null, state: "remote", error: null }];
  if (kind === 13 && !outgoing) { base.kind = "question"; base.question = { options: ["Yes", "No", "Later"], multiple: false } as Message["question"]; }
  if (kind === 17 && i > 10) base.reply_to = "m" + String(i - 7).padStart(6, "0");
  return base;
}

type Sample = { ms: number; commits: number; renderMs: number };
const state = { commits: 0, renderMs: 0 };
const onRender: ProfilerOnRenderCallback = (_id, _phase, actual) => { state.commits++; state.renderMs += actual; };
// production React has no act(): run, then let effects and microtasks settle
const act = async (f: () => void | Promise<void>) => { await f(); await new Promise<void>((r) => setTimeout(r, 0)); };
// rows re-rendered because a prop changed, by prop, since the last call
type Misses = Record<string, number>;
const takeMisses = (): Misses => { const w = window as unknown as { __memoMiss?: Misses }; const m = w.__memoMiss ?? {}; w.__memoMiss = {}; return m; };
const total = (m: Misses) => Object.values(m).reduce((a, x) => a + x, 0);
const frame = () => new Promise<void>((r) => requestAnimationFrame(() => requestAnimationFrame(() => r())));
const sleep = (ms: number) => new Promise<void>((r) => setTimeout(r, ms));
const quantile = (xs: number[], q: number) => { const s = [...xs].sort((a, b) => a - b); return s[Math.min(s.length - 1, Math.floor(q * s.length))]; };
const summary = (xs: Sample[]) => ({
  n: xs.length,
  median_ms: +quantile(xs.map((x) => x.ms), 0.5).toFixed(2), p95_ms: +quantile(xs.map((x) => x.ms), 0.95).toFixed(2),
  median_render_ms: +quantile(xs.map((x) => x.renderMs), 0.5).toFixed(2), commits: xs.reduce((a, x) => a + x.commits, 0),
});
/** Time `f` until two frames have painted after it, with React's commit cost. */
async function measure(f: () => void | Promise<void>, settle = 0): Promise<Sample> {
  state.commits = 0; state.renderMs = 0;
  const t = performance.now();
  await f();
  if (settle) await sleep(settle);
  await frame();
  return { ms: performance.now() - t, commits: state.commits, renderMs: state.renderMs };
}

let listener: ((e: HcEvent) => void) | null = null;
let all: Message[] = [];
const calls = { chat: 0, rowsReturned: 0 };

function install(n: number) {
  all = Array.from({ length: n }, (_, i) => make(i));
  const later = (a: Message, b: { at: string; id: string }) => a.created_at > b.at || (a.created_at === b.at && a.id >= b.id);
  const fallback = async () => null;
  for (const k of Object.keys(api) as (keyof typeof api)[]) (api as Record<string, unknown>)[k] = fallback;
  Object.assign(api, {
    state: async () => ({ me: { address: ME, username: "me" }, hubs: [{ url: "http://hub.invalid", name: "Bench hub", state: "connected", enabled: true }], platform: "desktop" }),
    uiState: async () => ({}),
    chats: async () => [{ peer: PEER, last: all[all.length - 1], unread: 0 }],
    directory: async () => [{ address: PEER, kind: "org", org_name: "Bench Org", username: "", blurb: "", online: true, last_seen: null, hubs: ["http://hub.invalid"] }],
    transfers: async () => ({ rows: [], next: null }), transferHistory: async () => ({ rows: [], next: null }),
    onEvent: async (f: (e: HcEvent) => void) => { listener = f; return () => { listener = null; }; },
    onSettingsChanged: async () => () => {}, onTray: async () => () => {}, onLink: async () => () => {}, onPendingLink: async () => () => {}, onAppUpdateProgress: async () => () => {},
    draft: async () => "", setDraft: async () => {}, draftAnswers: async () => ({ replies: [], picks: {} }), setDraftAnswers: async () => {},
    cardAnswers: async () => [], setupStatus: async () => null, composing: async () => {}, markRead: async () => {},
    sendRoute: async () => ({ pinned: null, automatic: "http://hub.invalid", next: "http://hub.invalid", hubs: ["http://hub.invalid"] }),
    loadOlder: async () => ({ added: 0, more: [], unreachable: [] }),
    localOrgtreeOrgs: async () => [], orgtreeLinks: async () => false, appInstalled: async () => false, vpnActive: async () => false,
    devices: async () => [], message: async (id: string) => structuredClone(all.find((m) => m.id === id) ?? null),
    // the store's paging: the newest `limit` before a message, or everything from one on
    chat: async (_p: string, o: { before?: Message | null; from?: Message | null; limit?: number } = {}) => {
      calls.chat++;
      let v = all;
      if (o.before) { const b = { at: o.before.created_at, id: o.before.id }; v = v.filter((m) => !later(m, b)); }
      if (o.from) { const f = { at: o.from.created_at, id: o.from.id }; v = v.filter((m) => later(m, f)); }
      const out = v.slice(Math.max(0, v.length - (o.limit ?? 50)));
      calls.rowsReturned += out.length;
      // like the core over IPC: new objects on every read
      return structuredClone(out);
    },
  });
}

async function run(n: number, back = n) {
  install(n);
  await act(async () => { await store.startStore(); });
  const host = document.getElementById("root")!;
  const root = createRoot(host);
  const rows = () => host.querySelectorAll(".timeline [id^='m-']").length;
  const tl = () => host.querySelector(".timeline") as HTMLElement;
  const r: Record<string, unknown> = { n, back };
  // open: what the chat draws first (the newest page)
  const open = await measure(async () => {
    await act(async () => {
      root.render(createElement(PlatformCtx.Provider, { value: "desktop" }, createElement(Profiler, { id: "c", onRender },
        createElement(Conversation, { peer: PEER, onBack: () => {}, onInfo: () => {}, onOpenAddr: () => {}, onContact: () => {} }))));
    });
    for (let i = 0; i < 200 && !rows(); i++) await act(async () => { await sleep(10); });
  }, 50);
  r.open = { ...open, rows: rows() };
  // scrolled back through the history (all of it, or the newest `back` rows): older pages as the top comes near
  const t = performance.now(); let pages = 0;
  while (rows() < Math.min(n, back) && pages < 2000) {
    const before = rows();
    await act(async () => { const el = tl(); el.scrollTop = 0; el.dispatchEvent(new Event("scroll")); await sleep(0); });
    for (let i = 0; i < 100 && rows() === before; i++) await act(async () => { await sleep(5); });
    if (rows() === before) break;
    pages++;
  }
  r.scrollback = { ms: +(performance.now() - t).toFixed(1), pages, rows: rows(), dom_nodes: host.querySelectorAll("*").length };
  // typing in the composer with the whole history loaded (still scrolled back)
  const box = host.querySelector("textarea") as HTMLTextAreaElement;
  const setter = Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, "value")!.set!;
  box.focus();
  takeMisses();
  const keys: Sample[] = [];
  for (let i = 0; i < 40; i++) {
    keys.push(await measure(() => act(async () => {
      box.dispatchEvent(new KeyboardEvent("keydown", { key: "a", bubbles: true }));
      setter.call(box, box.value + "a");
      box.dispatchEvent(new Event("input", { bubbles: true }));
      box.dispatchEvent(new KeyboardEvent("keyup", { key: "a", bubbles: true }));
    })));
  }
  const keyMiss = takeMisses();
  r.keystroke = { ...summary(keys.slice(5)), rows_rerendered_per_key: total(keyMiss) / keys.length, by_prop: keyMiss };
  // back to the newest message
  const toNewest = await measure(async () => {
    await act(async () => { const el = tl(); el.scrollTop = el.scrollHeight; el.dispatchEvent(new Event("scroll")); await sleep(20); });
    await act(async () => { const el = tl(); el.scrollTop = el.scrollHeight; el.dispatchEvent(new Event("scroll")); await sleep(20); });
  });
  const tlEl = tl();
  r.return_to_newest = { ...toNewest, rows: rows(), dom_nodes: host.querySelectorAll("*").length, at_bottom: tlEl.scrollHeight - tlEl.scrollTop - tlEl.clientHeight < 80 };
  // typing there
  const keys2: Sample[] = [];
  for (let i = 0; i < 20; i++) {
    keys2.push(await measure(() => act(async () => {
      setter.call(box, box.value + "b");
      box.dispatchEvent(new Event("input", { bubbles: true }));
    })));
  }
  r.keystroke_at_newest = summary(keys2.slice(3));
  takeMisses();
  // the peer's typing heartbeat (about once a second while they write)
  const typing: Sample[] = [];
  for (let i = 0; i < 15; i++) typing.push(await measure(() => act(async () => { listener?.({ type: "typing", peer: PEER }); await sleep(0); })));
  const typingMiss = takeMisses();
  r.typing_event = { ...summary(typing.slice(3)), rows_rerendered_per_event: total(typingMiss) / typing.length, by_prop: typingMiss };
  // a new incoming message: the store reports a chat change, the chat re-reads its newest rows
  const incoming: Sample[] = [];
  for (let i = 0; i < 10; i++) {
    const m = make(n + i); m.outgoing = false; m.state = "received"; all.push(m);
    incoming.push(await measure(() => act(async () => { listener?.({ type: "incoming", peer: PEER, id: m.id, preview: "x" }); for (let k = 0; k < 40 && !document.getElementById("m-" + m.id); k++) await sleep(5); })));
  }
  const inMiss = takeMisses();
  r.incoming = { ...summary(incoming.slice(2)), rows: rows(), rows_rerendered_per_message: total(inMiss) / incoming.length, by_prop: inMiss };
  // the gate (red on 1.3.12): typing re-renders no message row, whatever the history
  r.checks = [
    { name: "a keystroke re-renders no message row", ok: total(keyMiss) === 0 },
    { name: "back at the newest message, at most 300 rows stay mounted, still at the bottom", ok: (r.return_to_newest as { rows: number }).rows <= 300 && (r.return_to_newest as { at_bottom: boolean }).at_bottom },
  ];
  // the window gains focus (VR overlays and alt-tab do this often): the open chat re-reads all it shows
  // an offscreen window never has the OS focus: the page's own answer follows these events
  let focused = true;
  document.hasFocus = () => focused;
  takeMisses();
  const reads0 = calls.chat;
  const focus: Sample[] = [];
  for (let i = 0; i < 5; i++) {
    const before = calls.chat;
    focus.push(await measure(async () => {
      focused = false; window.dispatchEvent(new Event("blur"));
      focused = true; window.dispatchEvent(new Event("focus"));
      for (let k = 0; k < 400 && calls.chat === before; k++) await sleep(5);
      await sleep(30);
    }));
  }
  const focusMiss = takeMisses();
  r.focus = { ...summary(focus), reads: calls.chat - reads0, rows_rerendered_per_focus: total(focusMiss) / focus.length, by_prop: focusMiss };
  (r.checks as { name: string; ok: boolean }[]).push(
    { name: "a new incoming message re-renders at most 2 older rows", ok: total(inMiss) <= 2 * incoming.length },
    { name: "a focus gain's re-read re-renders no row whose message didn't change", ok: calls.chat - reads0 >= focus.length && total(focusMiss) === 0 },
  );
  r.calls = { ...calls };
  await act(async () => { root.unmount(); });
  return r;
}

declare global { interface Window { longChat?: { run: (n: number, back?: number) => Promise<unknown> }; longChatError?: string } }
window.longChat = { run };
window.addEventListener("error", (e) => { window.longChatError = String(e.message); });
