// Mounted App, synthetic API only: your status (Hubchat 1.2.7 with Mail hub 2.3.0; docket
// org-presence-busy-orange-when-no-org-inbox-holde, user 2026-10-31 08:52Z: "users on hubchat should be able to
// manually set their status (do not disturb prevents notifications from appearing on their desktop / phone)").
// Your own avatar (desktop: the chat list's head; Android: the Chats screen's app bar) wears the dot others see and
// opens the picker, made like the hub picker (desktop a popover, Android a bottom sheet): Automatic, Online, Busy and
// Do not disturb, each with what it does and its dot, the one in use checked; then Profile. A pick goes to the core
// (hc_set_status) and the dot follows it; with no hub connected the dot says offline (offline wins, 09:03Z). On Do
// not disturb, Settings › Notifications says nothing will show or sound, with a way to turn it off. A person on Do
// not disturb: Contact info says what it means.
// ?platform=desktop|android
import { createRoot } from "react-dom/client";
import App from "../src/App";
import { api, type Contact, type HubStatus, type Message } from "../src/api";
import { mockApi } from "../src/lib/mock";
import { refreshState } from "../src/lib/store";

type Check = { name: string; ok: boolean; detail?: unknown };
declare global {
  interface Window {
    statusFixture?: { checks: Check[]; platform: string; step: (name: string) => Promise<unknown> };
    statusFixtureError?: string;
  }
}
const sleep = (ms: number) => new Promise<void>((r) => setTimeout(r, ms));
const until = async (f: () => boolean, ms: number) => { const t = performance.now(); while (performance.now() - t < ms) { if (f()) return true; await sleep(25); } return f(); };

const A = "wss://hub-a.invalid", B = "wss://hub-b.invalid";
const ME = "scratch.abcdef", PIA = "pia.444444", ALICE = "alice.111111", ACME = "acme.4d5e6f";
const CHOICES = [
  { label: "Automatic", sub: "Online while you're here, busy after an hour away", dot: null, checked: true },
  { label: "Online", sub: "Online even while you're away", dot: "hdot online", checked: false },
  { label: "Busy", sub: "Shown busy to everyone", dot: "hdot busy", checked: false },
  { label: "Do not disturb", sub: "No notifications on any of your devices", dot: "hdot dnd", checked: false },
];
const PERSON_DND_NOTE = "“Do not disturb” means they've turned off notifications on all their devices: your messages still arrive, and they'll see them when they look.";

async function run() {
  const platform = new URLSearchParams(location.search).get("platform") === "android" ? "android" : "desktop";
  const checks: Check[] = [];
  const check = (name: string, ok: unknown, detail?: unknown) => { checks.push({ name, ok: !!ok, ...(ok ? {} : { detail }) }); };
  let status = "auto", connected = true;
  const picks: string[] = [];
  const contact = (address: string, kind: "person" | "org", name: string, online: boolean, busy: boolean, dnd: boolean): Contact =>
    ({ address, kind, org_name: kind === "org" ? name : "", username: kind === "org" ? "" : name, blurb: "", online, busy, dnd,
      last_seen: null, hubs: [A], picture: null }) as Contact;
  const directory: Contact[] = [
    contact(ME, "person", "Scratch", true, false, false),
    contact(PIA, "person", "Pia", true, true, true), // a person who picked Do not disturb
    contact(ALICE, "person", "Alice", true, false, false),
    contact(ACME, "org", "Acme Labs", true, false, false),
  ];
  const hub = (url: string, name: string): HubStatus => ({ url, name, state: connected ? "connected" : "disconnected", error: null, retry_at_ms: null,
    max_attachment_bytes: 1 << 30, features: ["v2", "busy", "dnd", "status"] });
  let n = 0;
  const message = (peer: string, body: string, outgoing = false): Message => ({ id: peer + "-" + n, peer, outgoing, hub: A, body, kind: null, reply_to: null,
    created_at: new Date(1700000000000 + n++ * 60000).toISOString(), sent_at: null, received_at: null, state: outgoing ? "delivered" : "received", fetched_at: null, delivered_at: null, read_at: null,
    error: null, seen: true, attachments: [] } as Message);
  const chats: Record<string, Message[]> = {
    [PIA]: [message(PIA, "Heads-down on the release today"), message(PIA, "Good luck!", true)],
    [ALICE]: [message(ALICE, "Hello")],
    [ACME]: [message(ACME, "We'll get back to you")],
  };
  const all = Object.values(chats).flat();
  Object.assign(api, mockApi, {
    state: async () => ({ ...await mockApi.state(), me: { id: "scratch", address: ME, name: "Scratch", about: "" }, hubs: [hub(A, "Hub A"), hub(B, "Hub B")],
      platform, recovery_saved: true, file_background_hint: null, status }),
    setStatus: async (s: string) => { picks.push(s); status = s; },
    directory: async () => directory,
    chats: async () => Object.keys(chats).map((p) => ({ peer: p, last: chats[p].at(-1), unread: 0 })),
    chat: async (p: string) => chats[p] ?? [],
    message: async (id: string) => all.find((m) => m.id === id) ?? null,
    picture: async () => null,
    transfers: async () => ({ rows: [], next: null }), onEvent: async () => () => {},
    openDm: async () => {}, markRead: async () => {}, draft: async () => "", setDraft: async () => {}, draftAnswers: async () => ({ replies: [], picks: {} }),
    setupStatus: async () => null,
    sendRoute: async () => ({ pinned: null, automatic: A, next: A, hubs: [] }),
    loadOlder: async () => ({ added: 0, more: [], unreachable: [] }),
  });
  // each view starts clean: the app reopens the last open chat from localStorage (useLastChat), and the views share
  // one Electron session
  localStorage.clear(); sessionStorage.clear();
  const root = createRoot(document.getElementById("root")!); root.render(<App />);
  await sleep(300);

  const settle = () => sleep(250);
  const state = (e: Element | null | undefined) => (e ? [...e.classList].filter((c) => c.startsWith("p-")).join(" ") : null);
  const me = () => document.querySelector<HTMLElement>(platform === "desktop" ? ".side-head .me-status" : ".appbar .me-status");
  const mine = () => ({ dot: state(me()?.querySelector(".pres")), label: me()?.getAttribute("aria-label") ?? null, title: me()?.title ?? null });
  const menu = () => document.querySelector<HTMLElement>('[role="menu"][aria-label="Your status"]');
  const rows = () => [...(menu()?.querySelectorAll<HTMLElement>(".statusopt") ?? [])];
  const choices = () => rows().map((o) => ({ label: o.querySelector("b")?.textContent ?? "", sub: o.querySelector("small")?.textContent ?? "",
    dot: o.querySelector(".hdot")?.className ?? null, checked: o.getAttribute("aria-checked") === "true" }));
  const openMenu = async () => { me()?.click(); await until(() => rows().length === 4, 3000); await settle(); };
  const closeMenu = async () => {
    if (platform === "desktop") me()?.click(); else document.querySelector<HTMLElement>(".sheet-scrim")?.click();
    await until(() => !menu(), 3000); await settle();
  };
  const pick = async (label: string) => { rows().find((o) => o.querySelector("b")?.textContent === label)?.click(); await until(() => !menu(), 3000); await settle(); };
  const row = (name: string) => [...document.querySelectorAll<HTMLElement>(".crow")].find((e) => e.querySelector(".crow-name")?.textContent === name);

  const steps: Record<string, () => Promise<unknown>> = {
    "me": async () => {
      await until(() => !!me() && !!row("Pia"), 3000); await settle();
      const m = mine();
      check("your own avatar wears the dot others see: Automatic shows online", m.dot === "p-online" && m.label === "Your status: Automatic" && m.title === "Your status: Automatic", m);
      return m;
    },
    "open": async () => {
      await openMenu();
      const m = menu(), c = choices();
      check("the picker: Automatic, Online, Busy and Do not disturb, each with what it does and the dot others see; Automatic checked",
        JSON.stringify(c) === JSON.stringify(CHOICES), c);
      check("each choice is a radio item, as in the hub picker", rows().length === 4 && rows().every((o) => o.getAttribute("role") === "menuitemradio" && o.classList.contains("hubopt")), rows().map((o) => o.outerHTML.slice(0, 120)));
      const items = [...(m?.querySelectorAll<HTMLElement>(".mi") ?? [])];
      check("the picker ends with Profile", items.at(-1)?.textContent === "Profile" && items.at(-1)?.getAttribute("role") === "menuitem", items.map((i) => i.textContent));
      check(platform === "desktop" ? "desktop: a popover under your avatar, headed Your status" : "Android: a bottom sheet with its grip, headed Your status",
        platform === "desktop" ? !!m?.classList.contains("pop") && m.querySelector(".pop-h")?.textContent === "Your status" && (m.getBoundingClientRect().top >= (me()?.getBoundingClientRect().bottom ?? 1e9))
          : !!m?.classList.contains("sheet") && !!m.querySelector(".grip") && m.querySelector("h4")?.textContent === "Your status",
        { cls: m?.className, head: m?.querySelector(".pop-h, h4")?.textContent });
      return c;
    },
    "pick-dnd": async () => {
      await pick("Do not disturb");
      await until(() => mine().dot === "p-dnd", 3000);
      const m = mine();
      check("Do not disturb picked: the core is told, the picker closes and your dot turns red", picks.at(-1) === "dnd" && !menu() && m.dot === "p-dnd" && m.label === "Your status: Do not disturb", { picks, open: !!menu(), m });
      return m;
    },
    "reopen": async () => {
      await openMenu();
      const on = choices().filter((c) => c.checked).map((c) => c.label);
      check("reopened: Do not disturb is the one checked", JSON.stringify(on) === JSON.stringify(["Do not disturb"]), on);
      return on;
    },
    "close": async () => { await closeMenu(); return !menu(); },
    "notifications": async () => {
      if (platform === "desktop") {
        document.querySelector<HTMLElement>('.side-head [aria-label="Settings"]')?.click();
        await until(() => !!document.querySelector(".settings"), 3000); await settle();
        [...document.querySelectorAll<HTMLElement>(".settings .tab, .settings [role=tab], .settings nav button")].find((b) => b.textContent?.trim() === "Notifications")?.click();
      } else {
        document.querySelector<HTMLElement>('.appbar [aria-label="Settings"]')?.click(); await settle();
        [...document.querySelectorAll<HTMLElement>('.li[role="button"]')].find((r) => r.querySelector(".t1")?.textContent === "Notifications")?.click();
      }
      await until(() => !!document.querySelector(".note-card.dnd-note"), 3000); await settle();
      const note = document.querySelector<HTMLElement>(".note-card.dnd-note");
      const text = note?.textContent ?? "";
      check("Settings › Notifications on Do not disturb: nothing shows or sounds on any device, messages still arrive",
        text.startsWith("Do not disturb is on.") && text.includes("Hubchat shows no notifications and plays no sound on any of your devices.") && text.includes("Messages still arrive."), text);
      check("…with a way to turn it off", note?.querySelector("button")?.textContent === "Turn it off", note?.innerHTML.slice(0, 300));
      return text;
    },
    "turn-off": async () => {
      document.querySelector<HTMLElement>(".note-card.dnd-note button")?.click();
      await until(() => !document.querySelector(".note-card.dnd-note"), 3000); await settle();
      check("Turn it off: Automatic again, and the note goes", picks.at(-1) === "auto" && !document.querySelector(".note-card.dnd-note"), picks);
      if (platform === "desktop") window.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape" }));
      else { document.querySelector<HTMLElement>('.appbar [aria-label="Back"]')?.click(); await settle(); document.querySelector<HTMLElement>('.appbar [aria-label="Back"]')?.click(); }
      await until(() => !!me(), 3000); await settle();
      return mine();
    },
    "profile": async () => {
      await openMenu();
      [...(menu()?.querySelectorAll<HTMLElement>(".mi") ?? [])].find((i) => i.textContent === "Profile")?.click();
      await until(() => !menu() && !!document.querySelector(platform === "desktop" ? ".settings" : ".scr"), 3000); await settle();
      const title = platform === "desktop" ? document.querySelector(".settings .tab.on, .settings [aria-selected=true]")?.textContent?.trim() ?? null : [...document.querySelectorAll(".appbar .title")].at(-1)?.textContent ?? null;
      check("Profile in the picker opens your profile, as your avatar did", title === "Profile", title);
      if (platform === "desktop") window.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape" }));
      else document.querySelector<HTMLElement>('.appbar [aria-label="Back"]')?.click();
      await until(() => !!me(), 3000); await settle();
      return title;
    },
    "person-dnd": async () => {
      row("Pia")?.click(); await settle();
      (document.querySelector(platform === "desktop" ? '[aria-label="Contact info"]' : ".appbar .who") as HTMLElement | null)?.click();
      const info = () => platform === "desktop" ? document.querySelector<HTMLElement>('aside.info[aria-label="Contact info"]') : document.querySelector<HTMLElement>(".scr-body");
      await until(() => !!info()?.querySelector(".pres"), 3000); await settle();
      const words = info()?.textContent ?? "";
      check("Contact info of a person on Do not disturb: the red dot, and what it means for a person",
        state(info()?.querySelector(".pres")) === "p-dnd" && info()?.querySelector(".ptext")?.textContent === "Do not disturb" && words.includes(PERSON_DND_NOTE), words.slice(0, 600));
      return words.slice(0, 300);
    },
    "person-close": async () => {
      if (platform === "desktop") document.querySelector<HTMLElement>('.info-head [aria-label="Close"]')?.click();
      else { document.querySelector<HTMLElement>('.appbar [aria-label="Back"]')?.click(); await settle(); document.querySelector<HTMLElement>('.appbar [aria-label="Back"]')?.click(); }
      await until(() => !!me(), 3000); await settle();
      return !!me();
    },
    "offline": async () => {
      // busy, then no hub connected: offline wins over your status (user 2026-10-31 09:03Z, "Offline for both")
      status = "busy"; await refreshState(); await until(() => mine().dot === "p-busy", 3000);
      const busy = mine();
      connected = false; await refreshState(); await until(() => mine().dot === "p-offline", 3000); await settle();
      const off = mine();
      check("your dot follows your status (Busy), and says offline while none of your hubs is connected",
        busy.dot === "p-busy" && busy.label === "Your status: Busy" && off.dot === "p-offline" && off.title === "Your status: Busy · offline: none of your hubs is connected", { busy, off });
      connected = true; status = "auto"; await refreshState(); await settle();
      return { busy, off };
    },
  };
  window.statusFixture = { checks, platform, step: async (name) => { const f = steps[name]; if (!f) throw Error("no step " + name); return f(); } };
}
run().catch((e) => { window.statusFixtureError = String(e?.stack || e); });
