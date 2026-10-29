// Mounted Contact info, synthetic API only: the "Orgtree org" badge of an org
// the Orgtree install on this PC hosts opens it in Orgtree; every other badge
// stays plain. No native opener, registry, files or network: the two native
// calls are recorded here.
// ?platform=desktop|android  ?view=local (an org on this PC) | remote (an org
// elsewhere) | fail (Orgtree cannot be started)
import { createRoot } from "react-dom/client";
import App from "../src/App";
import { api, type Message } from "../src/api";
import { mockApi } from "../src/lib/mock";

declare global { interface Window { orgtreeBadgeResult?: { platform: string; view: string; checks: { name: string; ok: boolean }[]; calls: string[] }; orgtreeBadgeError?: string } }
const settle = () => new Promise<void>((r) => setTimeout(r, 70));

async function run() {
  const params = new URLSearchParams(location.search);
  const platform = params.get("platform") === "android" ? "android" : "desktop";
  const view = params.get("view") === "remote" ? "remote" : params.get("view") === "fail" ? "fail" : "local";
  const local = "alpha-works.someone.1a2b3c", remote = "remote-org.other.9f8e7d";
  const peer = view === "remote" ? remote : local;
  const checks: { name: string; ok: boolean }[] = [], calls: string[] = [];
  const check = (name: string, ok: unknown) => checks.push({ name, ok: !!ok });
  const message = (to: string, i: number): Message => ({ id: to + "-" + i, peer: to, outgoing: false, hub: "https://fixture.invalid", body: "Hello " + i, kind: null, reply_to: null,
    created_at: new Date(1700000000000 + i * 60000).toISOString(), sent_at: null, received_at: null, state: "received", fetched_at: null, delivered_at: null, read_at: null, error: null, attachments: [] } as Message);
  const contact = (address: string, name: string) => ({ address, kind: "org", org_name: name, username: "", blurb: "", online: true, last_seen: null, hubs: [] });
  Object.assign(api, mockApi, {
    state: async () => ({ ...await mockApi.state(), me: { id: "scratch", address: "scratch.abcdef", name: "Scratch", about: "" }, hubs: [], platform, recovery_saved: true, file_background_hint: null }),
    directory: async () => [contact(local, "Alpha Works"), contact(remote, "Remote Org")],
    chats: async () => [{ peer: local, last: message(local, 1), unread: 0 }, { peer: remote, last: message(remote, 0), unread: 0 }],
    chat: async (p: string) => [message(p, 0), message(p, 1)],
    message: async () => null,
    transfers: async () => ({ rows: [], next: null }), onEvent: async () => () => {},
    openDm: async () => {}, markRead: async () => {}, draft: async () => "", setDraft: async () => {}, draftAnswers: async () => ({ replies: [], picks: {} }),
    setupStatus: async () => null, sendRoute: async () => ({ pinned: null, automatic: null, next: null, hubs: [] }),
    loadOlder: async () => ({ added: 0, more: [], unreachable: [] }),
    // the native side: which orgs this PC's Orgtree hosts, and starting it on one
    localOrgtreeOrgs: async () => { calls.push("list"); return [{ slug: "alpha-works", name: "Alpha Works", address: local }]; },
    openOrgtreeOrg: async (slug: string) => { calls.push("open:" + slug); if (view === "fail") throw "Orgtree could not be started"; },
  });
  const root = createRoot(document.getElementById("root")!); root.render(<App />); await settle(); await settle();
  const click = async (e: Element | null | undefined) => { (e as HTMLElement | null | undefined)?.click(); await settle(); await settle(); };
  await click([...document.querySelectorAll(".crow")].find((e) => e.textContent?.includes(view === "remote" ? "Remote Org" : "Alpha Works")));
  await click(document.querySelector(platform === "desktop" ? 'button[aria-label="Contact info"]' : ".appbar .who"));
  const heroSel = platform === "desktop" ? ".info-hero" : ".hero";
  const chip = () => document.querySelector<HTMLElement>(heroSel + " .chip.k-org");
  check("contact info shows the Orgtree org badge", chip()?.textContent?.includes("Orgtree org"));
  if (platform === "desktop") {
    // the conversation header's chip stays plain (coordinator 2026-10-29)
    check("the conversation header's badge stays plain", document.querySelector(".conv-head .chip.k-org")?.tagName === "SPAN");
  }
  if (platform === "android") {
    check("Android never asks for local orgs", !calls.includes("list"));
    check("Android keeps the plain badge", chip()?.tagName === "SPAN" && !chip()?.hasAttribute("title"));
  } else if (view === "remote") {
    check("asks which orgs this PC's Orgtree hosts", calls.includes("list"));
    check("an org on another computer keeps the plain badge", chip()?.tagName === "SPAN" && !chip()?.hasAttribute("title"));
  } else {
    const b = chip();
    check("asks which orgs this PC's Orgtree hosts", calls.includes("list"));
    check("a local org's badge is a button", b?.tagName === "BUTTON" && (b as HTMLButtonElement).type === "button");
    check("its tooltip says Open in Orgtree", b?.getAttribute("title") === "Open in Orgtree");
    check("it is announced with the org's name", b?.getAttribute("aria-label") === "Open Alpha Works in Orgtree");
    const look = b && getComputedStyle(b);
    check("it keeps the chip's size and colours", look && look.height === "22px" && look.fontSize === "11.5px" && look.cursor === "pointer"
      && look.backgroundColor !== "rgba(0, 0, 0, 0)" && look.borderRadius === "11px");
    await click(b);
    check("clicking starts Orgtree on that org", calls.includes("open:alpha-works"));
    if (view === "fail") {
      check("a failed start says so once", [...document.querySelectorAll(".toast,.snack")].filter((t) => t.textContent?.includes("Orgtree couldn't be opened")).length === 1);
      check("and the badge turns plain", chip()?.tagName === "SPAN");
    } else {
      check("a successful start shows no message", ![...document.querySelectorAll(".toast,.snack")].some((t) => t.textContent?.includes("Orgtree")));
      check("the badge stays a link", chip()?.tagName === "BUTTON");
    }
  }
  window.orgtreeBadgeResult = { platform, view, checks, calls };
}
run().catch((e) => { window.orgtreeBadgeError = String(e?.stack || e); });
