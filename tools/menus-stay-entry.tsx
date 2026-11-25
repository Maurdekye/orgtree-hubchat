// Mounted App, synthetic API only: an arriving message, the typing dots, a read check or a presence change never
// closes an open context menu (Hubchat 1.4.0; docket chats-an-arriving-message-never-closes-an-open-c, user
// 2026-11-25 m1f747c2836f9 "make sure new messages in either hubchat or the new inbox dont close any context menu").
// Events come through the mocked api.onEvent, as the core sends them. Right-click (desktop) and the long-press's
// contextmenu (Android) are DOM events dispatched in this page; nothing reaches the clipboard. ?platform=desktop|android
import { createRoot } from "react-dom/client";
import App from "../src/App";
import { api, type Contact, type HcEvent, type Message } from "../src/api";
import { mockApi } from "../src/lib/mock";

type Check = { name: string; ok: boolean; detail?: unknown };
declare global {
  interface Window {
    menusFixture?: { checks: Check[]; platform: string; step: (name: string) => Promise<unknown> };
    menusFixtureError?: string;
  }
}
const sleep = (ms: number) => new Promise<void>((r) => setTimeout(r, ms));
const until = async (f: () => boolean, ms: number) => { const t = performance.now(); while (performance.now() - t < ms) { if (f()) return true; await sleep(25); } return f(); };
async function jpeg(): Promise<ArrayBuffer> {
  const c = document.createElement("canvas"); c.width = 64; c.height = 48;
  const g = c.getContext("2d")!; g.fillStyle = "#2a6"; g.fillRect(0, 0, 64, 48); g.fillStyle = "#fd3"; g.fillRect(8, 8, 24, 16);
  return await new Promise<ArrayBuffer>((ok) => c.toBlob((b) => void b!.arrayBuffer().then(ok), "image/jpeg", 0.9));
}
const KIM = "kim.19ac02", LEE = "lee.55d0e1", ME = "scratch.abcdef", HUB = "wss://hub-a.invalid";
Object.defineProperty(navigator, "clipboard", { configurable: true, value: {
  write: async () => {}, writeText: async () => {},
  read: async () => { throw new Error("stub: no reading"); }, readText: async () => { throw new Error("stub: no reading"); },
} });

async function run() {
  const platform = new URLSearchParams(location.search).get("platform") === "android" ? "android" : "desktop";
  const checks: Check[] = [];
  const picture = await jpeg();
  const check = (name: string, ok: unknown, detail?: unknown) => { checks.push({ name, ok: !!ok, ...(ok ? {} : { detail }) }); };
  const person = (address: string, username: string, online: boolean) => ({ address, kind: "person", org_name: "", username, blurb: "", online, busy: false,
    last_seen: null, hubs: [HUB], picture: null }) as unknown as Contact;
  const directory: Contact[] = [person(KIM, "Kim", true), person(LEE, "Lee", true)];
  let clock = 1700000000000;
  const msg = (id: string, peer: string, body: string, outgoing = false, extra: Partial<Message> = {}): Message => ({ id, peer, outgoing, hub: HUB, body, kind: null,
    reply_to: null, created_at: new Date(clock += 60000).toISOString(), sent_at: null, received_at: null, state: outgoing ? "delivered" : "received",
    fetched_at: null, delivered_at: null, read_at: null, error: null, seen: true, attachments: [], ...extra } as Message);
  const chats: Record<string, Message[]> = {
    [LEE]: [msg("l-1", LEE, "See you tomorrow.")],
    [KIM]: [...Array.from({ length: 40 }, (_, i) => msg(`k-${i}`, KIM, `Note ${i}: the figures for the north region, row ${i}, are in.`)),
      msg("k-pic", KIM, "", false, { attachments: [{ local_id: "att-1", hub_id: "h-1", name: "photo.jpg", bytes: picture.byteLength, source: null, local_path: null, state: "remote", error: null }] as Message["attachments"] }),
      msg("k-mine", KIM, "Thanks, I will read them tonight.", true, { delivered_at: new Date(clock).toISOString() })],
  };
  let emit: ((e: HcEvent) => void) | null = null;
  const send = (e: HcEvent) => { emit?.(e); };
  Object.assign(api, mockApi, {
    state: async () => ({ ...await mockApi.state(), me: { id: "scratch", address: ME, name: "Scratch", about: "" }, hubs: [{ url: HUB, name: "Hub A", state: "connected", error: null, retry_at_ms: null, max_attachment_bytes: 1 << 30, features: ["v2", "typing"] }], platform, recovery_saved: true, file_background_hint: null }),
    directory: async () => directory.map((c) => ({ ...c })),
    chats: async () => Object.entries(chats).map(([peer, list]) => ({ peer, last: list.at(-1), unread: 0 }))
      .sort((a, b) => (b.last!.created_at > a.last!.created_at ? 1 : -1)),
    chat: async (peer: string) => (chats[peer] ?? []).map((m) => ({ ...m })),
    message: async (id: string) => Object.values(chats).flat().find((m) => m.id === id) ?? null,
    picture: async () => null,
    attachmentPreview: async () => picture.slice(0),
    transfers: async () => ({ rows: [], next: null }),
    onEvent: async (f: (e: HcEvent) => void) => { emit = f; return () => { emit = null; }; },
    openDm: async () => {}, markRead: async () => {}, draft: async () => "", setDraft: async () => {}, draftAnswers: async () => ({ replies: [], picks: {} }),
    setupStatus: async () => null,
    sendRoute: async () => ({ pinned: null, automatic: HUB, next: HUB, hubs: [] }),
    loadOlder: async () => ({ added: 0, more: [], unreachable: [] }),
  });
  localStorage.clear(); sessionStorage.clear();
  createRoot(document.getElementById("root")!).render(<App />);
  await sleep(300);
  const settle = () => sleep(250);
  const row = (name: string) => [...document.querySelectorAll<HTMLElement>(".crow")].find((e) => e.querySelector(".crow-name")?.textContent === name);
  const rowOrder = () => [...document.querySelectorAll<HTMLElement>(".crow .crow-name")].map((e) => e.textContent);
  const bubble = (id: string) => document.querySelector<HTMLElement>(`.msg[data-id="${id}"] .bubble`);
  const menuEl = () => document.querySelector<HTMLElement>(platform === "desktop" ? ".pop.ctx" : ".sheet.contact-file-menu");
  const labels = () => [...(menuEl()?.querySelectorAll(".mi") ?? [])].map((b) => b.textContent?.trim() ?? "");
  const ctx = async (el: Element | null) => {
    const r = el?.getBoundingClientRect();
    el?.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, cancelable: true, clientX: (r?.left ?? 0) + Math.min(40, (r?.width ?? 0) / 2),
      clientY: (r?.top ?? 0) + (r?.height ?? 0) / 2, button: 2 }));
    await settle();
  };
  // Android has no message menu: a long-press SELECTS the message (the selection app bar); a picture's opens a sheet
  const pressEvents = (el: Element, type: string, x: number, y: number) =>
    el.dispatchEvent(new PointerEvent(type, { bubbles: true, cancelable: true, pointerId: 7, pointerType: "touch", isPrimary: true, clientX: x, clientY: y }));
  const longPress = async (el: Element | null) => {
    if (!el) return;
    const r = el.getBoundingClientRect(), x = r.left + Math.min(40, r.width / 2), y = r.top + r.height / 2;
    pressEvents(el, "pointerdown", x, y); await sleep(600); pressEvents(el, "pointerup", x, y); await settle();
  };
  const selected = () => document.querySelector(".appbar.select .title")?.textContent ?? null;
  /** a message's menu (desktop) or its selection (Android) */
  const hold = async (id: string) => { if (platform === "desktop") await ctx(bubble(id)); else await longPress(bubble(id)); };
  const held = () => (platform === "desktop" ? !!menuEl() : selected() === "1 selected");
  const letGo = async () => { if (platform === "desktop") await escape(); else { (document.querySelector('.appbar.select [aria-label="Cancel"]') as HTMLElement | null)?.click(); await settle(); } };
  const pic = () => document.querySelector<HTMLImageElement>('.msg[data-id="k-pic"] .att-img img');
  const escape = async () => { (document.activeElement ?? document.body).dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true, cancelable: true })); await settle(); };
  const timeline = () => document.querySelector<HTMLElement>(".timeline");
  const atEnd = () => { const t = timeline(); return !!t && t.scrollHeight - t.scrollTop - t.clientHeight < 4; };
  const arrive = (peer: string, id: string, body: string) => {
    chats[peer]!.push(msg(id, peer, body));
    send({ type: "incoming", peer, id, preview: body }); send({ type: "chat", peer });
  };
  let n = 0;

  await until(() => !!row("Kim"), 3000); row("Kim")?.click();
  await until(() => !!bubble("k-mine"), 4000); await sleep(600);

  const steps: Record<string, () => Promise<unknown>> = {
    // the chat sits at its newest message; the menu is on a message above it
    "message": async () => {
      check("POSITIVE CONTROL: the chat is at its newest message", atEnd(), { top: timeline()?.scrollTop, h: timeline()?.scrollHeight, c: timeline()?.clientHeight });
      await hold("k-37");
      const open = held(), had = labels();
      arrive(KIM, `k-new-${++n}`, "A new message: the south region is in too.");
      const drawn = await until(() => !!bubble(`k-new-${n}`), 3000); await settle();
      check("an arriving message keeps a message's menu open (Android: its selection)", open && drawn && held() && JSON.stringify(labels()) === JSON.stringify(had),
        { open, drawn, held: held(), had, now: labels(), selected: selected() });
      return { open, drawn, held: held(), atEnd: atEnd() };
    },
    "typing": async () => {
      if (!held()) await hold("k-37");
      send({ type: "typing", peer: KIM });
      const dots = await until(() => !!document.querySelector(".typing-row"), 2000); await sleep(100);
      check("the typing dots keep a message's menu open", dots && held(), { dots, held: held() });
      const gone = await until(() => !document.querySelector(".typing-row"), 4000);
      check("the typing dots going away keep it open", gone && held(), { gone, held: held() });
      return null;
    },
    "tick": async () => {
      if (!held()) await hold("k-37");
      const mine = chats[KIM]!.find((m) => m.id === "k-mine")!;
      const tick = () => document.querySelector('.msg[data-id="k-mine"] .tick')?.className;
      const before = tick();
      Object.assign(mine, { state: "read", read_at: new Date().toISOString() });
      send({ type: "chat", peer: KIM });
      const changed = await until(() => tick() !== before, 3000); await settle();
      check("a read check arriving keeps a message's menu open", changed && held(), { before, after: tick(), held: held() });
      return null;
    },
    "presence": async () => {
      if (!held()) await hold("k-37");
      directory[0]!.online = false;
      send({ type: "directory" });
      await sleep(800);
      check("a presence change keeps a message's menu open", held(), { held: held() });
      return null;
    },
    "own-message-menu": async () => {
      await letGo();
      await hold("k-mine");
      const open = held();
      arrive(KIM, `k-new-${++n}`, "And the west region.");
      const drawn = await until(() => !!bubble(`k-new-${n}`), 3000); await settle();
      check("an arriving message keeps the menu of the user's own message open", open && drawn && held(), { open, drawn, held: held() });
      return null;
    },
    // a picture's menu: desktop's right-click menu, Android's long-press sheet
    "picture": async () => {
      await letGo();
      await until(() => !!pic()?.complete, 3000);
      // Android: the WebView turns the long-press on a picture into this contextmenu (tools/copy-image-entry.tsx)
      await ctx(pic());
      const open = !!menuEl();
      arrive(KIM, `k-new-${++n}`, "One more: the east region.");
      const drawn = await until(() => !!bubble(`k-new-${n}`), 3000); await settle();
      check("an arriving message keeps a picture's menu open (Android: its sheet)", open && drawn && !!menuEl(), { open, drawn, menu: !!menuEl(), labels: labels() });
      return null;
    },
    "row": async () => {
      await escape();
      if (platform === "android") { history.back(); await until(() => !!row("Lee"), 3000); await settle(); }
      const before = rowOrder();
      await ctx(row("Lee"));
      const open = !!menuEl();
      arrive(LEE, "l-new", "Actually, can we move it to Friday?");
      const moved = await until(() => JSON.stringify(rowOrder()) !== JSON.stringify(before), 3000); await settle();
      check("a message arriving moves a chat row and keeps its menu open", open && moved && !!menuEl(), { open, before, after: rowOrder(), menu: !!menuEl() });
      return { before, after: rowOrder() };
    },
    "escape": async () => {
      const open = !!menuEl();
      await escape();
      check("POSITIVE CONTROL: Escape still closes the menu", open && !menuEl(), { open, menu: !!menuEl() });
      return null;
    },
  };
  window.menusFixture = { checks, platform, step: async (name) => { const f = steps[name]; if (!f) throw Error("no step " + name); return f(); } };
}
run().catch((e) => { window.menusFixtureError = String(e?.stack || e); });
