// Mounted App, synthetic API only: Do not disturb presence (Hubchat 1.2.7 with Mail hub 2.3.0; docket
// org-presence-busy-orange-when-no-org-inbox-holde, the addition of user 2026-10-31 08:51Z: "when an org's outside
// operations are set to do not disturb, it should show a red status icon in hubchat / elsewhere"). An org whose
// outside mail is set to Do not disturb wears the presence dot filled red (one --dnd token, the danger --bad) and the
// words Do not disturb wherever presence shows: the chat list, the open chat's header (Android: its app bar), a
// mention's hover, Contact info (with what it means), the hub picker (the hub where it is on Do not disturb) and the
// Directory (its own section, after Busy now). Online, busy and offline stay as they were.
// ?platform=desktop|android
import { createRoot } from "react-dom/client";
import App from "../src/App";
import { api, type Contact, type HubStatus, type Message } from "../src/api";
import { mockApi } from "../src/lib/mock";

type Check = { name: string; ok: boolean; detail?: unknown };
declare global {
  interface Window {
    dndFixture?: { checks: Check[]; platform: string; step: (name: string) => Promise<unknown> };
    dndFixtureError?: string;
  }
}
const sleep = (ms: number) => new Promise<void>((r) => setTimeout(r, ms));
const until = async (f: () => boolean, ms: number) => { const t = performance.now(); while (performance.now() - t < ms) { if (f()) return true; await sleep(25); } return f(); };

const A = "wss://hub-a.invalid", B = "wss://hub-b.invalid";
const ME = "scratch.abcdef", DELTA = "delta.7a8b9c", ACME = "acme.4d5e6f", ALICE = "alice.111111", BETA = "beta.222222", OLLIE = "ollie.333333";
const DND_NOTE = "“Do not disturb” means its outside mail is on hold: your messages are kept, but no agent there sees them until one checks its inbox.";
const ONLINE_NOTE = "“Online” means its Orgtree is connected to the hub — not that an agent is typing.";
const NL = String.fromCharCode(10);

async function run() {
  const platform = new URLSearchParams(location.search).get("platform") === "android" ? "android" : "desktop";
  const checks: Check[] = [];
  const check = (name: string, ok: unknown, detail?: unknown) => { checks.push({ name, ok: !!ok, ...(ok ? {} : { detail }) }); };
  const day = 864e5;
  const contact = (address: string, kind: "person" | "org", name: string, online: boolean, busy: boolean, dnd: boolean, seenDaysAgo: number | null = null): Contact =>
    ({ address, kind, org_name: kind === "org" ? name : "", username: kind === "org" ? "" : name, blurb: "", online, busy, dnd,
      last_seen: seenDaysAgo === null ? null : new Date(Date.now() - seenDaysAgo * day).toISOString(), hubs: [A], picture: null }) as Contact;
  const directory: Contact[] = [
    contact(ME, "person", "Scratch", true, false, false),
    { ...contact(DELTA, "org", "Delta Org", true, true, true), hubs: [A, B] }, // on Do not disturb on hub A, offline on hub B
    contact(ACME, "org", "Acme Labs", true, true, false),
    contact(ALICE, "person", "Alice", true, false, false),
    contact(BETA, "org", "Beta Works", true, false, false),
    contact(OLLIE, "person", "Ollie", false, false, false, 2),
  ];
  const hub = (url: string, name: string): HubStatus => ({ url, name, state: "connected", error: null, retry_at_ms: null, max_attachment_bytes: 1 << 30, features: ["v2", "busy", "dnd"] });
  const hubs: HubStatus[] = [hub(A, "Hub A"), hub(B, "Hub B")];
  let n = 0;
  const message = (peer: string, body: string, outgoing = false): Message => ({ id: peer + "-" + n, peer, outgoing, hub: A, body, kind: null, reply_to: null,
    created_at: new Date(1700000000000 + n++ * 60000).toISOString(), sent_at: null, received_at: null, state: outgoing ? "delivered" : "received", fetched_at: null, delivered_at: null, read_at: null,
    error: null, seen: true, attachments: [] } as Message);
  const chats: Record<string, Message[]> = {
    [DELTA]: [message(DELTA, "We're heads-down this week; write to @net:" + DELTA + " and we'll read it after."), message(DELTA, "Understood", true)],
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
    sendRoute: async (p: string) => ({ pinned: null, automatic: A, next: A, hubs: p === DELTA ? [{ url: A, online: true, busy: true, dnd: true }, { url: B, online: false, busy: false, dnd: false }] : [] }),
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

  const pose = async (wd: number | null) => {
    const n2 = document.querySelector<HTMLElement>(".conv-head .n2"); if (!n2) return null;
    n2.style.flex = wd === null ? "" : "none"; n2.style.width = wd === null ? "" : wd + "px";
    for (let i = 0; i < 3; i++) await new Promise((r) => requestAnimationFrame(() => r(null)));
    return [...n2.children].filter((e) => getComputedStyle(e).visibility !== "hidden" && e.getBoundingClientRect().left < n2.getBoundingClientRect().right - 0.5).map((e) => e.className);
  };

  const steps: Record<string, () => Promise<unknown>> = {
    "list": async () => {
      await until(() => !!row("Delta Org") && !!row("Acme Labs") && !!row("Ollie"), 3000);
      const seen = Object.fromEntries(["Delta Org", "Acme Labs", "Alice", "Ollie"].map((name) => [name, { dot: state(dotOf(row(name))), title: dotOf(row(name))?.title ?? null }]));
      check("the chat list: an org on Do not disturb wears the red dot, titled Do not disturb", seen["Delta Org"].dot === "p-dnd" && seen["Delta Org"].title === "Do not disturb", seen);
      check("the chat list: online, busy and offline stay as they were", seen.Alice.dot === "p-online" && seen.Alice.title === "Online" && seen["Acme Labs"].dot === "p-busy"
        && seen["Acme Labs"].title === "Busy" && seen.Ollie.dot === "p-offline", seen);
      return seen;
    },
    "header": async () => {
      await open("Delta Org");
      const head = document.querySelector(platform === "desktop" ? ".conv-head" : ".appbar .who");
      const text = platform === "desktop" ? head?.querySelector(".ptext")?.textContent ?? null : head?.querySelector(".s")?.textContent ?? null;
      check("the open chat's header says Do not disturb, dot and words", state(dotOf(head)) === "p-dnd" && (platform === "desktop" ? text === "Do not disturb" : !!text?.startsWith("do not disturb")),
        { dot: state(dotOf(head)), text });
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
      check("the hub picker: the hub where it is on Do not disturb says so, dot and words", a?.dot === "hdot dnd" && a?.sub === "Delta Org is on Do not disturb here", opts);
      check("the hub picker: a hub where it is offline says so as before", b?.dot === "hdot offline" && b?.sub === "Delta Org is offline here", opts);
      return opts;
    },
    "picker-close": async () => {
      (document.querySelector(platform === "desktop" ? ".conv-head .hubchip.pick" : ".sheet-scrim") as HTMLElement | null)?.click();
      await until(() => !document.querySelector(".hubopt"), 3000); await settle();
      return !document.querySelector(".hubopt");
    },
    "mention": async () => {
      const sel = 'a.addr.mention[data-slug="' + DELTA + '"]';
      await until(() => !!document.querySelector<HTMLElement>(sel)?.title, 3000);
      const a = document.querySelector<HTMLElement>(sel);
      check("a mention of an org on Do not disturb: its hover leads with Do not disturb", !!a && a.title.startsWith("Do not disturb · org" + NL + "Delta Org"), a?.title ?? null);
      return a?.title ?? null;
    },
    "info-org": async () => {
      await openInfo();
      const i = infoOf();
      check("Contact info of an org on Do not disturb: the red dot and the words Do not disturb", i.dot === "p-dnd" && i.text === "Do not disturb", { dot: i.dot, text: i.text });
      check("Contact info of an org on Do not disturb says what it means, after what online means",
        i.words.includes(ONLINE_NOTE) && i.words.includes(DND_NOTE) && i.words.indexOf(ONLINE_NOTE) < i.words.indexOf(DND_NOTE), i.words.slice(0, 700));
      return { dot: i.dot, text: i.text };
    },
    "header-fit": async () => {
      // the chat header's status line beside an open Contact info (desktop, 1000 px): the state keeps its whole width
      // up to the line's own, then ends in an ellipsis, never cut mid-word ("Do not disturl"; coordinator 2026-10-31)
      const n2 = document.querySelector<HTMLElement>(".conv-head .n2"), p = n2?.querySelector<HTMLElement>(".ptext");
      const r = p?.getBoundingClientRect(), row = n2?.getBoundingClientRect(), cs = p ? getComputedStyle(p) : null;
      const facts = { text: p?.textContent ?? null, right: r?.right ?? null, rowRight: row?.right ?? null, scroll: p?.scrollWidth ?? null,
        client: p?.clientWidth ?? null, overflow: cs?.overflowX ?? null, textOverflow: cs?.textOverflow ?? null };
      check("the chat header's state ends in an ellipsis inside its line, never cut mid-word (Contact info open, 1000 px)",
        !!r && !!row && r.right <= row.right + 0.5 && cs?.textOverflow === "ellipsis" && cs?.overflowX === "hidden", facts);
      return facts;
    },
    "header-parts": async () => {
      // the same line at every width from 40 to 520 px: after the state it shows whole parts, each with its separator,
      // or neither, so it never ends in a "•" (coordinator 2026-10-31): the address at least 30 px, the hub chip whole
      const n2 = document.querySelector<HTMLElement>(".conv-head .n2");
      const bad: unknown[] = [], seen = new Set<string>();
      const frame = () => new Promise((r) => requestAnimationFrame(() => r(null)));
      for (let wd = 40; n2 && wd <= 520; wd += 3) {
        n2.style.flex = "none"; n2.style.width = wd + "px";
        await frame(); await frame(); await frame();
        const row = n2.getBoundingClientRect();
        const vis = [...n2.children].filter((e): e is HTMLElement => e instanceof HTMLElement && getComputedStyle(e).visibility !== "hidden"
          && e.getBoundingClientRect().width > 0 && e.getBoundingClientRect().left < row.right - 0.5);
        const tag = (e: HTMLElement) => e.classList.contains("sep") ? "•" : e.classList.contains("ptext") ? "S" : e.classList.contains("hubchip") ? "H" : "A";
        seen.add(vis.map(tag).join(""));
        vis.forEach((e, i) => {
          if (tag(e) !== "•") return;
          const next = vis[i + 1], r = next?.getBoundingClientRect();
          const ok = !!next && !!r && (tag(next) === "H" ? r.right <= row.right + 0.5 : tag(next) === "A" && Math.min(r.right, row.right) - r.left >= 30);
          if (!ok) bad.push({ width: wd, line: vis.map(tag).join(""), next: next ? tag(next) : null, shown: r ? Math.round(Math.min(r.right, row.right) - r.left) : null });
        });
      }
      if (n2) { n2.style.width = ""; n2.style.flex = ""; }
      const facts = { bad: bad.slice(0, 8), badCount: bad.length, seen: [...seen] };
      check("the chat header's status line never ends in a separator: each • is followed by the whole hub chip or at least 30 px of the address, at every width from 40 to 520 px",
        !!n2 && bad.length === 0 && ["S", "S•H", "S•A•H"].every((l) => seen.has(l)), facts);
      return facts;
    },
    // posed widths of the same line for the captures (115 px: the old line ended "Do not disturb •"; 220 px)
    "pose-115": async () => pose(115),
    "pose-220": async () => pose(220),
    "unpose": async () => pose(null),
    "directory": async () => {
      await closeInfo(); await backToList();
      (document.querySelector('[aria-label="Directory"]') as HTMLElement | null)?.click();
      await until(() => document.querySelectorAll(".nc-sec").length > 0, 3000); await settle();
      const secs = [...document.querySelectorAll(".nc-sec")].map((e) => e.textContent);
      check("the Directory: Online now, then Busy now, then Do not disturb, then the people seen lately",
        JSON.stringify(secs) === JSON.stringify(["Online now · 2", "Busy now · 1", "Do not disturb · 1", "Seen in the last 7 days · 1"]), secs);
      const rows = [...document.querySelectorAll<HTMLElement>(".dir-row")].map((r) => ({ name: r.querySelector(".t1")?.textContent?.trim() ?? "", dot: state(dotOf(r)), word: r.querySelector(".r span")?.textContent ?? "" }));
      const delta = rows.filter((r) => r.name.startsWith("Delta Org"));
      check("the Directory: the row on Do not disturb wears the red dot and says do not disturb", delta.length === 1 && delta[0].dot === "p-dnd" && delta[0].word === "do not disturb", rows);
      const text = document.body.textContent ?? "";
      check("the Directory counts as online only who is online and free", text.includes("· 2 online") && !text.includes("· 3 online"), text.slice(0, 300));
      return { secs, rows };
    },
  };
  window.dndFixture = { checks, platform, step: async (name) => { const f = steps[name]; if (!f) throw Error("no step " + name); return f(); } };
}
run().catch((e) => { window.dndFixtureError = String(e?.stack || e); });
