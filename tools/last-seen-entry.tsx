// Mounted App, synthetic API only: last seen per kind (Hubchat 1.4.0 with Mail hub 2.6.0, feature `last_active`;
// docket hubchat-last-seen-time-for-orgs-people-and-indep, user 2026-11-24 11:44Z, choices ma46b9293f2d5). Presence
// words also say "active now" or "last seen ..." when a connected hub that lists them has the feature: the open
// chat's header (Android: its app bar), Contact info and the Directory rows. A hub without it shows no time at all,
// never the old last-call time.
// ?platform=desktop|android
import { createRoot } from "react-dom/client";
import App from "../src/App";
import { api, type Contact, type HubStatus, type Message } from "../src/api";
import { mockApi } from "../src/lib/mock";

type Check = { name: string; ok: boolean; detail?: unknown };
declare global {
  interface Window {
    seenFixture?: { checks: Check[]; platform: string; step: (name: string) => Promise<unknown> };
    seenFixtureError?: string;
  }
}
const sleep = (ms: number) => new Promise<void>((r) => setTimeout(r, ms));
const until = async (f: () => boolean, ms: number) => { const t = performance.now(); while (performance.now() - t < ms) { if (f()) return true; await sleep(25); } return f(); };

const A = "wss://hub-a.invalid", B = "wss://hub-b.invalid";
const ME = "scratch.abcdef", ACME = "acme.4d5e6f", BETA = "beta.222222", PAT = "pat.1a2b3c", ALICE = "alice.111111", OLLIE = "ollie.333333", SESS = "sess.444444", BO = "bo.555555", CY = "cy.666666";

async function run() {
  const platform = new URLSearchParams(location.search).get("platform") === "android" ? "android" : "desktop";
  const checks: Check[] = [];
  const check = (name: string, ok: unknown, detail?: unknown) => { checks.push({ name, ok: !!ok, ...(ok ? {} : { detail }) }); };
  const minAgo = (m: number) => new Date(Date.now() - m * 60000).toISOString();
  const contact = (address: string, kind: "person" | "org" | "chat", name: string, more: Partial<Contact>): Contact =>
    ({ address, kind, org_name: kind === "person" ? "" : name, username: kind === "person" ? name : "", blurb: "", online: true, busy: false,
      last_seen: null, hubs: [A], picture: null, ...more }) as Contact;
  const directory: Contact[] = [
    contact(ME, "person", "Scratch", {}),
    contact(ACME, "org", "Acme Labs", { active: true, active_at: minAgo(12) }), // an agent in charge is mid-turn
    contact(BETA, "org", "Beta Works", { active: false, active_at: minAgo(12) }), // its last turn ended 12 min ago
    contact(PAT, "person", "Pat", { busy: true, active: true, active_at: minAgo(1) }), // busy, Hubchat in front
    contact(ALICE, "person", "Alice", { online: false, active_at: minAgo(30), last_seen: minAgo(3) }), // last visit 30 min ago, last call 3 min ago
    contact(OLLIE, "person", "Ollie", { online: false, hubs: [B], last_seen: minAgo(2 * 1440), active_at: minAgo(20) }), // hub B has no last_active
    contact(SESS, "chat", "Night shift", { active_at: minAgo(5) }), // an agent session's last message sent
    // blocking (Mail hub 2.6.0, docket blocking-contacts-on-the-hub-hubchat-1-4-0-orgtr): in the DM a block wins over
    // active now / last seen (@coordinator-opus 2026-11-24 m4645f83a565d)
    contact(BO, "person", "Bo", { active: true, active_at: minAgo(1), blocked: true }), // we block them
    contact(CY, "person", "Cy", { active: true, active_at: minAgo(1), blocked_by: true }), // they block us
  ];
  const hub = (url: string, name: string, features: string[]): HubStatus => ({ url, name, state: "connected", error: null, retry_at_ms: null, max_attachment_bytes: 1 << 30, features });
  const hubs: HubStatus[] = [hub(A, "Hub A", ["v2", "busy", "last_active"]), hub(B, "Hub B", ["v2", "busy"])];
  let n = 0;
  const message = (peer: string, body: string): Message => ({ id: peer + "-" + n, peer, outgoing: false, hub: peer === OLLIE ? B : A, body, kind: null, reply_to: null,
    created_at: new Date(1700000000000 + n++ * 60000).toISOString(), sent_at: null, received_at: null, state: "received", fetched_at: null, delivered_at: null, read_at: null,
    error: null, seen: true, attachments: [] } as Message);
  const chats: Record<string, Message[]> = Object.fromEntries([ACME, BETA, PAT, ALICE, OLLIE, SESS, BO, CY].map((p) => [p, [message(p, "Hello")]]));
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
    sendRoute: async (p: string) => ({ pinned: null, automatic: p === OLLIE ? B : A, next: p === OLLIE ? B : A, hubs: [] }),
    loadOlder: async () => ({ added: 0, more: [], unreachable: [] }),
  });
  // each view starts clean: the app reopens the last open chat from localStorage (useLastChat)
  localStorage.clear(); sessionStorage.clear();
  const root = createRoot(document.getElementById("root")!); root.render(<App />);
  await sleep(300);

  const settle = () => sleep(250);
  const row = (name: string) => [...document.querySelectorAll<HTMLElement>(".crow")].find((e) => e.querySelector(".crow-name")?.textContent === name);
  const header = () => (platform === "desktop" ? document.querySelector(".conv-head .name") : document.querySelector(".appbar .who .n .ell"))?.textContent ?? "";
  const backToList = async () => { if (platform === "android" && document.querySelector(".appbar .who")) { (document.querySelector('.appbar [aria-label="Back"]') as HTMLElement | null)?.click(); await settle(); } };
  const open = async (name: string) => { await backToList(); row(name)?.click(); await until(() => header() === name, 3000); await settle(); };
  // desktop: the header's presence words; Android: the app bar's subtitle (a hub name may follow)
  const words = () => (platform === "desktop" ? document.querySelector(".conv-head .ptext")?.textContent ?? null : document.querySelector(".appbar .who .s")?.textContent ?? null);
  const infoRoot = () => (platform === "desktop" ? document.querySelector<HTMLElement>('aside.info[aria-label="Contact info"]') : document.querySelector<HTMLElement>(".scr-body"));
  // desktop: the full words; Android: the short words, "· <hub>" may follow
  const fits = (w: string | null, text: string, short: string) => (platform === "desktop" ? w === text : !!w && (w === short || w.startsWith(short + " · ")));

  const heads: [string, string, string, string][] = [
    ["Acme Labs", "an org with an agent in charge mid-turn: active now", "Online · active now", "online · active now"],
    ["Beta Works", "an org: last seen when its last turn ended", "Online · last seen 12 min ago", "online · last seen 12 min ago"],
    ["Pat", "a busy person with Hubchat in front: busy, active now", "Busy · active now", "busy · active now"],
    ["Alice", "an offline person: last seen at their last visit, not their last call", "Offline · last seen 30 min ago", "last seen 30 min ago"],
    ["Ollie", "a hub without last_active: no time at all", "Offline", "offline"],
    ["Night shift", "an agent session: last seen at its last message, never active now", "Online · last seen 5 min ago", "online · last seen 5 min ago"],
    ["Bo", "a contact we block: Blocked, not active now", "Blocked", "blocked"],
    ["Cy", "a contact who blocks us: Has blocked you, not active now", "Has blocked you", "has blocked you"],
  ];
  const steps: Record<string, () => Promise<unknown>> = {
    "headers": async () => {
      await until(() => heads.every(([name]) => !!row(name)), 3000);
      const seen: Record<string, string | null> = {};
      for (const [name, what, text, short] of heads) {
        await open(name);
        seen[name] = words();
        check("the open chat's header: " + what, fits(seen[name], text, short), { name, got: seen[name], text, short });
      }
      return seen;
    },
    "list-avatars": async () => {
      // the chats list row's picture carries the same red cancel as the open chat's header (USER 2026-11-25 09:05Z
      // m2d43462b28f6 "the dm is missing the red cancel icon in the dms list, but it appears in the chat header")
      await backToList(); await until(() => !!row("Bo") && !!row("Cy"), 3000); await settle();
      const dot = (name: string) => row(name)?.querySelector(".av .pres")?.className ?? null;
      const got = { Bo: dot("Bo"), Cy: dot("Cy"), Pat: dot("Pat") };
      check("the chats list row's picture shows the block's red cancel, as the header does", got.Bo === "pres p-blocked" && got.Cy === "pres p-blocked" && got.Pat === "pres p-busy", got);
      return got;
    },
    "header-org": async () => { await open("Acme Labs"); return words(); },
    "info-org": async () => {
      (document.querySelector(platform === "desktop" ? '[aria-label="Contact info"]' : ".appbar .who") as HTMLElement | null)?.click();
      await until(() => !!infoRoot()?.querySelector(".ptext"), 3000); await settle();
      const t = infoRoot()?.querySelector(".ptext")?.textContent ?? null;
      check("Contact info of an org mid-turn says Online · active now", t === "Online · active now", t);
      return t;
    },
    "directory": async () => {
      (document.querySelector(platform === "desktop" ? '.info-head [aria-label="Close"]' : '.appbar [aria-label="Back"]') as HTMLElement | null)?.click();
      await settle(); await backToList();
      (document.querySelector('[aria-label="Directory"]') as HTMLElement | null)?.click();
      await until(() => document.querySelectorAll(".dir-row").length > 0, 3000); await settle();
      const rows = Object.fromEntries([...document.querySelectorAll<HTMLElement>(".dir-row")].map((r) => [r.querySelector(".t1")?.textContent?.trim().split(" ")[0] ?? "", r.querySelector(".r span")?.textContent ?? ""]));
      check("the Directory rows say active now or last seen per kind",
        rows.Acme === "online · active now" && rows.Beta === "online · last seen 12 min ago" && rows.Pat === "busy · active now"
        && rows.Alice === "last seen 30 min ago" && rows.Ollie === "offline", rows);
      return rows;
    },
  };
  window.seenFixture = { checks, platform, step: async (name) => { const f = steps[name]; if (!f) throw Error("no step " + name); return f(); } };
}
run().catch((e) => { window.seenFixtureError = String(e?.stack || e); });
