// Mounted App, synthetic API only: busy presence (Hubchat 1.2.7 with Mail hub 2.3.0; user 2026-10-30 23:03Z-23:19Z,
// docket org-presence-busy-orange-when-no-org-inbox-holde). An address a hub lists online can be busy instead: an org
// none of whose org-inbox holders can start a turn, a person away from Hubchat on all their devices for an hour. It
// wears the presence dot filled amber (one --busy token, the caution --warn) and the word Busy wherever presence
// shows: the chat list, the open chat's header (Android: its app bar), a mention's hover, Contact info (with what busy
// means), the hub picker (the hub where they are busy) and the Directory (its own section, after Online now). Online
// and offline stay as they were.
// ?platform=desktop|android
import { createRoot } from "react-dom/client";
import App from "../src/App";
import { api, type Contact, type HubStatus, type Message } from "../src/api";
import { mockApi } from "../src/lib/mock";

type Check = { name: string; ok: boolean; detail?: unknown };
declare global {
  interface Window {
    busyFixture?: { checks: Check[]; platform: string; step: (name: string) => Promise<unknown> };
    busyFixtureError?: string;
  }
}
const sleep = (ms: number) => new Promise<void>((r) => setTimeout(r, ms));
const until = async (f: () => boolean, ms: number) => { const t = performance.now(); while (performance.now() - t < ms) { if (f()) return true; await sleep(25); } return f(); };

const A = "wss://hub-a.invalid", B = "wss://hub-b.invalid";
const ME = "scratch.abcdef", PAT = "pat.1a2b3c", ACME = "acme.4d5e6f", ALICE = "alice.111111", BETA = "beta.222222", OLLIE = "ollie.333333";
const PERSON_NOTE = "“Busy” means Hubchat hasn't been in front on any of their devices for an hour.";
const ORG_NOTE = "“Busy” means it is connected, but none of the agents who answer its outside mail can start a turn right now.";
const ONLINE_NOTE = "“Online” means its Orgtree is connected to the hub — not that an agent is typing.";

async function run() {
  const platform = new URLSearchParams(location.search).get("platform") === "android" ? "android" : "desktop";
  const checks: Check[] = [];
  const check = (name: string, ok: unknown, detail?: unknown) => { checks.push({ name, ok: !!ok, ...(ok ? {} : { detail }) }); };
  const day = 864e5;
  const contact = (address: string, kind: "person" | "org", name: string, online: boolean, busy: boolean, seenDaysAgo: number | null = null): Contact =>
    ({ address, kind, org_name: kind === "org" ? name : "", username: kind === "org" ? "" : name, blurb: "", online, busy,
      last_seen: seenDaysAgo === null ? null : new Date(Date.now() - seenDaysAgo * day).toISOString(), hubs: [A], picture: null }) as Contact;
  const directory: Contact[] = [
    contact(ME, "person", "Scratch", true, false),
    { ...contact(PAT, "person", "Pat", true, true), hubs: [A, B] }, // busy on hub A, offline on hub B
    contact(ACME, "org", "Acme Labs", true, true),
    contact(ALICE, "person", "Alice", true, false),
    contact(BETA, "org", "Beta Works", true, false),
    contact(OLLIE, "person", "Ollie", false, false, 2),
  ];
  const hub = (url: string, name: string): HubStatus => ({ url, name, state: "connected", error: null, retry_at_ms: null, max_attachment_bytes: 1 << 30, features: ["v2", "busy"] });
  const hubs: HubStatus[] = [hub(A, "Hub A"), hub(B, "Hub B")];
  let n = 0;
  const message = (peer: string, body: string, outgoing = false): Message => ({ id: peer + "-" + n, peer, outgoing, hub: A, body, kind: null, reply_to: null,
    created_at: new Date(1700000000000 + n++ * 60000).toISOString(), sent_at: null, received_at: null, state: outgoing ? "delivered" : "received", fetched_at: null, delivered_at: null, read_at: null,
    error: null, seen: true, attachments: [] } as Message);
  const chats: Record<string, Message[]> = {
    [PAT]: [message(PAT, "Did you ask @net:" + ACME + " about it?"), message(PAT, "Talk later", true)],
    [ACME]: [message(ACME, "We'll get back to you")],
    [ALICE]: [message(ALICE, "Hello")],
    [OLLIE]: [message(OLLIE, "See you")],
  };
  const all = Object.values(chats).flat();
  Object.assign(api, mockApi, {
    state: async () => ({ ...await mockApi.state(), me: { id: "scratch", address: ME, name: "Scratch", about: "" }, hubs, platform, recovery_saved: true, file_background_hint: null }),
    directory: async () => directory,
    chats: async () => Object.keys(chats).map((p) => ({ peer: p, last: chats[p].at(-1), unread: 0 })),
    chat: async (p: string) => chats[p] ?? [],
    message: async (id: string) => all.find((m) => m.id === id) ?? null,
    picture: async () => null,
    transfers: async () => ({ rows: [], next: null }), onEvent: async () => () => {},
    openDm: async () => {}, markRead: async () => {}, draft: async () => "", setDraft: async () => {}, draftAnswers: async () => ({ replies: [], picks: {} }),
    setupStatus: async () => null,
    sendRoute: async (p: string) => ({ pinned: null, automatic: A, next: A, hubs: p === PAT ? [{ url: A, online: true, busy: true }, { url: B, online: false, busy: false }] : [] }),
    loadOlder: async () => ({ added: 0, more: [], unreachable: [] }),
  });
  // each view starts clean: the app reopens the last open chat from localStorage (useLastChat), and the views share
  // one Electron session
  localStorage.clear(); sessionStorage.clear();
  const root = createRoot(document.getElementById("root")!); root.render(<App />);
  await sleep(300);

  const settle = () => sleep(250);
  const row = (name: string) => [...document.querySelectorAll<HTMLElement>(".crow")].find((e) => e.querySelector(".crow-name")?.textContent === name);
  const dotOf = (e: Element | null | undefined) => e?.querySelector<HTMLElement>(".pres") ?? null;
  const state = (e: Element | null | undefined) => (e ? [...e.classList].filter((c) => c.startsWith("p-")).join(" ") : null);
  const header = () => (platform === "desktop" ? document.querySelector(".conv-head .name") : document.querySelector(".appbar .who .n .ell"))?.textContent ?? "";
  const open = async (name: string) => { row(name)?.click(); await until(() => header() === name, 3000); await settle(); };
  const infoRoot = () => (platform === "desktop" ? document.querySelector<HTMLElement>('aside.info[aria-label="Contact info"]') : document.querySelector<HTMLElement>(".scr-body"));
  const openInfo = async () => {
    (document.querySelector(platform === "desktop" ? '[aria-label="Contact info"]' : ".appbar .who") as HTMLElement | null)?.click();
    await until(() => !!infoRoot()?.querySelector(".pres"), 3000); await settle();
  };
  const closeInfo = async () => {
    (document.querySelector(platform === "desktop" ? '.info-head [aria-label="Close"]' : '.appbar [aria-label="Back"]') as HTMLElement | null)?.click();
    await settle();
  };
  const backToList = async () => { if (platform === "android") { (document.querySelector('.appbar [aria-label="Back"]') as HTMLElement | null)?.click(); await settle(); } };
  const infoOf = () => { const r = infoRoot(); return { dot: state(dotOf(r)), text: r?.querySelector(".ptext")?.textContent ?? null, words: r?.textContent ?? "" }; };

  const steps: Record<string, () => Promise<unknown>> = {
    "list": async () => {
      await until(() => !!row("Pat") && !!row("Acme Labs") && !!row("Ollie"), 3000);
      const seen = Object.fromEntries(["Pat", "Acme Labs", "Alice", "Ollie"].map((name) => [name, { dot: state(dotOf(row(name))), title: dotOf(row(name))?.title ?? null }]));
      check("the chat list: a busy person wears the busy dot, titled Busy", seen.Pat.dot === "p-busy" && seen.Pat.title === "Busy", seen);
      check("the chat list: a busy org wears the busy dot, titled Busy", seen["Acme Labs"].dot === "p-busy" && seen["Acme Labs"].title === "Busy", seen);
      check("the chat list: online and offline stay as they were", seen.Alice.dot === "p-online" && seen.Alice.title === "Online" && seen.Ollie.dot === "p-offline", seen);
      return seen;
    },
    "header": async () => {
      await open("Pat");
      const head = document.querySelector(platform === "desktop" ? ".conv-head" : ".appbar .who");
      const text = platform === "desktop" ? head?.querySelector(".ptext")?.textContent ?? null : head?.querySelector(".s")?.textContent ?? null;
      check("the open chat's header says busy, dot and word", state(dotOf(head)) === "p-busy" && (platform === "desktop" ? text === "Busy" : !!text?.startsWith("busy")), { dot: state(dotOf(head)), text });
      return { dot: state(dotOf(head)), text };
    },
    "picker": async () => {
      if (platform === "desktop") (document.querySelector(".conv-head .hubchip.pick") as HTMLElement | null)?.click();
      else {
        (document.querySelector('.appbar [aria-label="More"]') as HTMLElement | null)?.click(); await settle();
        [...document.querySelectorAll<HTMLElement>(".sheet .mi")].find((b) => b.textContent?.startsWith("Send through"))?.click();
      }
      await until(() => document.querySelectorAll(".hubopt").length >= 3, 3000); await settle();
      const opts = [...document.querySelectorAll<HTMLElement>(".hubopt")].map((o) => ({ label: o.querySelector("b")?.textContent ?? "", dot: o.querySelector(".hdot")?.className ?? null, sub: o.querySelector("small")?.textContent ?? "" }));
      const a = opts.find((o) => o.label === "Hub A"), b = opts.find((o) => o.label === "Hub B");
      check("the hub picker: the hub where they are busy says so, dot and words", a?.dot === "hdot busy" && a?.sub === "Pat is busy here", opts);
      check("the hub picker: a hub where they are offline says so as before", b?.dot === "hdot offline" && b?.sub === "Pat is offline here", opts);
      return opts;
    },
    "picker-close": async () => {
      (document.querySelector(platform === "desktop" ? ".conv-head .hubchip.pick" : ".sheet-scrim") as HTMLElement | null)?.click();
      await until(() => !document.querySelector(".hubopt"), 3000); await settle();
      return !document.querySelector(".hubopt");
    },
    "mention": async () => {
      const sel = 'a.addr.mention[data-slug="' + ACME + '"]';
      await until(() => !!document.querySelector<HTMLElement>(sel)?.title, 3000);
      const a = document.querySelector<HTMLElement>(sel);
      check("a mention of a busy org: its hover leads with Busy", !!a && a.title.startsWith("Busy · org\nAcme Labs"), a?.title ?? null);
      return a?.title ?? null;
    },
    "info-person": async () => {
      await openInfo();
      const i = infoOf();
      check("Contact info of a busy person: the busy dot and the word Busy", i.dot === "p-busy" && i.text === "Busy", { dot: i.dot, text: i.text });
      check("Contact info of a busy person says what busy means", i.words.includes(PERSON_NOTE), i.words.slice(0, 600));
      return { dot: i.dot, text: i.text };
    },
    "info-org": async () => {
      await closeInfo(); await backToList();
      await open("Acme Labs");
      await openInfo();
      const i = infoOf();
      check("Contact info of a busy org: the busy dot and the word Busy", i.dot === "p-busy" && i.text === "Busy", { dot: i.dot, text: i.text });
      check("Contact info of a busy org says what busy means, after what online means", i.words.includes(ONLINE_NOTE) && i.words.includes(ORG_NOTE) && i.words.indexOf(ONLINE_NOTE) < i.words.indexOf(ORG_NOTE), i.words.slice(0, 600));
      return { dot: i.dot, text: i.text };
    },
    "directory": async () => {
      await closeInfo(); await backToList();
      (document.querySelector('[aria-label="Directory"]') as HTMLElement | null)?.click();
      await until(() => document.querySelectorAll(".nc-sec").length > 0, 3000); await settle();
      const secs = [...document.querySelectorAll(".nc-sec")].map((e) => e.textContent);
      check("the Directory: Online now, then Busy now, then the people seen lately", JSON.stringify(secs) === JSON.stringify(["Online now · 2", "Busy now · 2", "Seen in the last 7 days · 1"]), secs);
      const rows = [...document.querySelectorAll<HTMLElement>(".dir-row")].map((r) => ({ name: r.querySelector(".t1")?.textContent?.trim() ?? "", dot: state(dotOf(r)), word: r.querySelector(".r span")?.textContent ?? "" }));
      const busyRows = rows.filter((r) => /^(Acme Labs|Pat)/.test(r.name));
      check("the Directory: the busy rows wear the busy dot and say busy", busyRows.length === 2 && busyRows.every((r) => r.dot === "p-busy" && r.word === "busy"), rows);
      check("the Directory counts as online only who is online and not busy", (document.body.textContent ?? "").includes("· 2 online"), (document.body.textContent ?? "").match(/·\s*\d+ online/)?.[0] ?? null);
      return { secs, rows };
    },
  };
  window.busyFixture = { checks, platform, step: async (name) => { const f = steps[name]; if (!f) throw Error("no step " + name); return f(); } };
}
run().catch((e) => { window.busyFixtureError = String(e?.stack || e); });
