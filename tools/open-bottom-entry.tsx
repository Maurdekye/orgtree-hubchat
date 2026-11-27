// Mounted App, synthetic API only: opening a chat lands at its bottom and stays there while late content (pictures)
// settles (docket hubchat-some-chats-open-scrolled-a-little-above-t, user 2026-11-27 m50385a018301 "when i click onto
// certain chats it always starts scrolled a little bit up from the bottom"). No hub, network or real clipboard.
// ?platform=desktop|android
import { createRoot } from "react-dom/client";
import App from "../src/App";
import { api, type Contact, type Message } from "../src/api";
import { mockApi } from "../src/lib/mock";

type Check = { name: string; ok: boolean; detail?: unknown };
declare global {
  interface Window {
    openFixture?: { checks: Check[]; platform: string; step: (name: string) => Promise<unknown> };
    openFixtureError?: string;
  }
}
const sleep = (ms: number) => new Promise<void>((r) => setTimeout(r, ms));
const frame = () => new Promise<void>((r) => requestAnimationFrame(() => r()));
const until = async (f: () => boolean, ms: number) => { const t = performance.now(); while (performance.now() - t < ms) { if (f()) return true; await sleep(25); } return f(); };
const KIM = "kim.19ac02", LEE = "lee.33bd01", ME = "scratch.abcdef";
const HUB = "wss://hub-a.invalid";

// a tall picture (portrait), so each one grows its bubble by a few hundred pixels when it decodes
async function jpeg(w: number, h: number, hue: number): Promise<ArrayBuffer> {
  const c = document.createElement("canvas"); c.width = w; c.height = h;
  const g = c.getContext("2d")!; g.fillStyle = `hsl(${hue} 60% 45%)`; g.fillRect(0, 0, w, h); g.fillStyle = "#fd3"; g.fillRect(w / 4, h / 4, w / 2, h / 6);
  return await new Promise<ArrayBuffer>((ok) => c.toBlob((b) => void b!.arrayBuffer().then(ok), "image/jpeg", 0.9));
}

async function run() {
  const platform = new URLSearchParams(location.search).get("platform") === "android" ? "android" : "desktop";
  const checks: Check[] = [];
  const check = (name: string, ok: unknown, detail?: unknown) => { checks.push({ name, ok: !!ok, ...(ok ? {} : { detail }) }); };
  const pics = await Promise.all([jpeg(300, 420, 10), jpeg(300, 400, 140), jpeg(320, 440, 220), jpeg(300, 380, 300)]);
  const contact = (address: string, username: string) => ({ address, kind: "person", org_name: "", username, blurb: "", online: true, busy: false, last_seen: null, hubs: [HUB], picture: null } as unknown as Contact);
  const msg = (peer: string, id: string, i: number, body: string, pic?: number): Message => ({ id, peer, outgoing: i % 3 === 0, hub: HUB, body, kind: null, reply_to: null,
    created_at: new Date(1700000000000 + i * 60000).toISOString(), sent_at: null, received_at: null, state: "received", fetched_at: null, delivered_at: null, read_at: null,
    error: null, seen: true,
    attachments: pic == null ? [] : [{ local_id: "att-" + id, hub_id: "h-" + id, name: `photo-${pic}.jpg`, bytes: pics[pic]!.byteLength, source: null, local_path: null, state: "remote", error: null }] } as unknown as Message);
  // Kim: a long text history that ends in four pictures; Lee: a short chat to switch away to
  const kim: Message[] = [];
  for (let i = 0; i < 30; i++) kim.push(msg(KIM, "k" + i, i, `Message ${i}: ` + "a line of chat text that wraps on a narrow window. ".repeat(1 + (i % 3))));
  for (let p = 0; p < 4; p++) kim.push(msg(KIM, "kp" + p, 30 + p, p % 2 ? "" : "and this one", p));
  kim.push(msg(KIM, "k-last", 40, "the last one"));
  const lee: Message[] = [msg(LEE, "l0", 1, "hi"), msg(LEE, "l1", 2, "hello")];
  const chats: Record<string, Message[]> = { [KIM]: kim, [LEE]: lee };
  // each preview arrives a little after the one before, as the native side answers them one by one
  let delay = 0;
  Object.assign(api, mockApi, {
    state: async () => ({ ...await mockApi.state(), me: { id: "scratch", address: ME, name: "Scratch", about: "" }, hubs: [{ url: HUB, name: "Hub A", state: "connected", error: null, retry_at_ms: null, max_attachment_bytes: 1 << 30, features: ["v2"] }], platform, recovery_saved: true, file_background_hint: null }),
    directory: async () => [contact(KIM, "Kim"), contact(LEE, "Lee")],
    chats: async () => [{ peer: KIM, last: kim.at(-1), unread: 0 }, { peer: LEE, last: lee.at(-1), unread: 0 }],
    chat: async (peer: string) => chats[peer] ?? [],
    message: async (id: string) => [...kim, ...lee].find((m) => m.id === id) ?? null,
    attachmentPreview: async (id: string) => { const p = Number(id.slice(2)); delay += 40; await sleep(delay); return pics[p]!.slice(0); },
    picture: async () => null,
    transfers: async () => ({ rows: [], next: null }), onEvent: async () => () => {},
    openDm: async () => {}, markRead: async () => {}, draft: async () => "", setDraft: async () => {}, draftAnswers: async () => ({ replies: [], picks: {} }),
    setupStatus: async () => null,
    sendRoute: async () => ({ pinned: null, automatic: HUB, next: HUB, hubs: [] }),
    loadOlder: async () => ({ added: 0, more: [], unreachable: [] }),
  });
  localStorage.clear(); sessionStorage.clear();
  createRoot(document.getElementById("root")!).render(<App />);
  await sleep(300);
  const row = (name: string) => [...document.querySelectorAll<HTMLElement>(".crow")].find((e) => e.querySelector(".crow-name")?.textContent === name);
  const back = () => (document.querySelector('.appbar [aria-label="Back"]') as HTMLElement | null)?.click();
  const tl = () => document.querySelector<HTMLElement>(".timeline");
  const gap = () => { const el = tl(); return el ? Math.round(el.scrollHeight - el.scrollTop - el.clientHeight) : -1; };
  const open = async (name: string, id: string) => {
    if (platform === "android") { back(); await until(() => !!row(name), 3000); }
    row(name)?.click();
    await until(() => !!document.querySelector(`.msg[data-id="${id}"]`), 3000);
  };
  const settled = async () => { // every picture shown and decoded, then a few frames
    await until(() => [...document.querySelectorAll<HTMLImageElement>(".att-img img")].length === 4
      && [...document.querySelectorAll<HTMLImageElement>(".att-img img")].every((i) => i.complete && i.naturalWidth > 0), 5000);
    for (let i = 0; i < 6; i++) await frame();
    await sleep(150);
  };
  await until(() => !!row("Kim"), 3000);

  const steps: Record<string, () => Promise<unknown>> = {
    // the user's case: open the chat again and again; it lands at the bottom every time
    "opens": async () => {
      const gaps: number[] = [];
      for (let i = 0; i < 8; i++) {
        await open("Lee", "l1"); await sleep(120);
        delay = 0;
        await open("Kim", "k-last"); await settled();
        gaps.push(gap());
      }
      check("every open of a chat that ends in pictures lands at its bottom", gaps.every((g) => g <= 1), gaps);
      return gaps;
    },
    // what goes wrong, without timing luck: a scroll event that arrives after the content below grew
    // (the pin's own programmatic scroll, dispatched a frame later) must not count as the user scrolling up
    "late-growth": async () => {
      const el = tl()!; el.scrollTop = el.scrollHeight; await frame(); await frame();
      const last = document.querySelector<HTMLElement>('.msg[data-id="k-last"] .bubble')!;
      const pad = document.createElement("div"); pad.style.height = "240px"; pad.className = "fixture-growth";
      last.appendChild(pad);
      el.dispatchEvent(new Event("scroll"));
      for (let i = 0; i < 4; i++) await frame();
      const g = gap();
      pad.remove(); for (let i = 0; i < 3; i++) await frame();
      check("content that grows after the pin keeps the chat at its bottom", g <= 1, g);
      return g;
    },
    // and the user's own scroll still wins: scrolled up, growth below never pulls the view down
    "user-up": async () => {
      const el = tl()!; el.scrollTop = el.scrollHeight; await frame(); await frame();
      el.scrollTop = Math.max(0, el.scrollTop - 500); el.dispatchEvent(new Event("scroll"));
      await frame(); await frame();
      const before = el.scrollTop;
      const last = document.querySelector<HTMLElement>('.msg[data-id="k-last"] .bubble')!;
      const pad = document.createElement("div"); pad.style.height = "240px";
      last.appendChild(pad); el.dispatchEvent(new Event("scroll"));
      for (let i = 0; i < 4; i++) await frame();
      const after = el.scrollTop, g = gap();
      pad.remove();
      check("a user who scrolled up stays where they are while content grows below", Math.abs(after - before) <= 1 && g >= 500, { before, after, gap: g });
      return { before, after };
    },
  };
  window.openFixture = { checks, platform, step: async (name) => { const f = steps[name]; if (!f) throw Error("no step " + name); return f(); } };
}
run().catch((e) => { window.openFixtureError = String(e?.stack || e); });
