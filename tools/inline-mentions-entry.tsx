// Mounted App, synthetic API only: inline mentions of other Hubchat clients
// (user 2026-10-29, docket hubchat-inline-mentions-of-other-hubchat-clients).
// Someone one of my hubs that isn't disabled lists shows with their DM-list
// picture or letters (an agent session: its terminal icon); anyone else gets a
// small grey dot, a tooltip saying they aren't on my hubs, and a click that
// opens nothing (Android: a tap says why). No native calls, files or network.
// ?platform=desktop|android
// ?view= desktop: marks | menu | live; android: marks | tap | open
import { createRoot } from "react-dom/client";
import App from "../src/App";
import { api, type Contact, type HubStatus, type Message, type PictureMeta } from "../src/api";
import { mockApi } from "../src/lib/mock";
import { md } from "../src/lib/md";
import { refreshAll } from "../src/lib/store";

declare global { interface Window { inlineMentionsResult?: { platform: string; view: string; checks: { name: string; ok: boolean; detail?: unknown }[]; calls: string[] }; inlineMentionsError?: string } }
const settle = () => new Promise<void>((r) => setTimeout(r, 70));
const settled = async (n = 4) => { for (let i = 0; i < n; i++) await settle(); };

const A = "wss://hub-a.invalid", B = "wss://hub-b.invalid";
const ME = "scratch.abcdef", PAT = "pat.1a2b3c", ALICE = "alice.111111", ACME = "acme.222222", AGENT = "claude.333333", GHOST = "ghost.444444", STRANGER = "stranger.555555";
const INCOMING = `Ask @net:${ALICE} or @net:${ACME}, or the session @net:${AGENT}. @net:${GHOST} and @net:${STRANGER} can't hear us. Pat is @net:${PAT} and I am @net:${ME}. In code: \`@net:${ALICE}\``;
const OUTGOING = `I'll tell @net:${ALICE}.`;
const NOT_ON = " isn't on any of the mail hubs you're connected to.";

/** A picture (not letters), so the capture shows which is which. */
function picturePng(): string {
  const c = document.createElement("canvas"); c.width = c.height = 64;
  const g = c.getContext("2d")!;
  g.fillStyle = "#2f7d6d"; g.fillRect(0, 0, 64, 64);
  g.fillStyle = "#f2c14e"; g.beginPath(); g.arc(32, 25, 13, 0, Math.PI * 2); g.fill();
  g.beginPath(); g.ellipse(32, 64, 25, 19, 0, 0, Math.PI * 2); g.fill();
  return c.toDataURL("image/png").slice("data:image/png;base64,".length);
}

async function run() {
  const params = new URLSearchParams(location.search);
  const platform = params.get("platform") === "android" ? "android" : "desktop";
  const view = params.get("view") || "marks";
  const checks: { name: string; ok: boolean; detail?: unknown }[] = [], calls: string[] = [];
  const check = (name: string, ok: unknown, detail?: unknown) => checks.push({ name, ok: !!ok, ...(ok ? {} : { detail }) });

  const png = picturePng();
  const patPicture: PictureMeta = { picture_owner: null, revision: "r1", sha256: "fixture-sha", width: 64, height: 64, bytes: png.length };
  const contact = (address: string, kind: string, name: string, hubs: string[], picture: PictureMeta | null = null): Contact =>
    ({ address, kind, org_name: kind === "org" ? name : "", username: kind === "org" ? "" : name, blurb: "", online: true, last_seen: null, hubs, picture });
  let directory: Contact[] = [
    contact(ME, "person", "Scratch", [A]), contact(PAT, "person", "Pat", [A], patPicture), contact(ALICE, "person", "Alice", [A]),
    // the hub's description of Acme, and a session that is offline, for the hover's lines
    { ...contact(ACME, "org", "Acme Org", [A]), blurb: "Acme's agents answer here." }, { ...contact(AGENT, "chat", "Claude session", [A]), online: false },
    // listed only by a hub that is disabled on this device: not accessible
    contact(GHOST, "person", "Ghost", [B]),
  ];
  const hub = (url: string, name: string, state: HubStatus["state"]): HubStatus => ({ url, name, state, error: null, retry_at_ms: null, max_attachment_bytes: 1 << 30, features: ["v2"] });
  let hubs: HubStatus[] = [hub(A, "Hub A", "connected"), hub(B, "Hub B", "disabled")];
  const message = (peer: string, i: number, body: string, outgoing: boolean): Message => ({ id: peer + "-" + i, peer, outgoing, hub: A, body, kind: null, reply_to: null,
    created_at: new Date(1700000000000 + i * 60000).toISOString(), sent_at: null, received_at: null, state: outgoing ? "delivered" : "received", fetched_at: null, delivered_at: null, read_at: null, error: null, attachments: [] } as Message);
  const chat = (peer: string): Message[] => peer === PAT ? [message(PAT, 0, INCOMING, false), message(PAT, 1, OUTGOING, true)] : [message(peer, 0, "Hello", false)];
  Object.assign(api, mockApi, {
    state: async () => ({ ...await mockApi.state(), me: { id: "scratch", address: ME, name: "Scratch", about: "" }, hubs, platform, recovery_saved: true, file_background_hint: null }),
    directory: async () => directory,
    chats: async () => [PAT, ALICE, ACME, AGENT].map((p) => ({ peer: p, last: chat(p).at(-1), unread: 0 })),
    chat: async (p: string) => chat(p),
    message: async () => null,
    picture: async (p: string) => (p === PAT ? { ...patPicture, data: png } : null),
    transfers: async () => ({ rows: [], next: null }), onEvent: async () => () => {},
    openDm: async (p: string) => { calls.push("open:" + p); }, markRead: async () => {}, draft: async () => "", setDraft: async () => {}, draftAnswers: async () => ({ replies: [], picks: {} }),
    setupStatus: async () => null, sendRoute: async () => ({ pinned: null, automatic: A, next: A, hubs: [] }),
    loadOlder: async () => ({ added: 0, more: [], unreachable: [] }),
  });
  const root = createRoot(document.getElementById("root")!); root.render(<App />); await settled(2);
  const click = async (e: Element | null | undefined) => { (e as HTMLElement | null | undefined)?.click(); await settled(2); };
  const row = (name: string) => [...document.querySelectorAll(".crow")].find((e) => e.querySelector(".crow-name")?.textContent === name);
  const openPat = () => click(row("Pat"));
  const header = () => (platform === "desktop" ? document.querySelector(".conv-head .name") : document.querySelector(".appbar .who .n .ell"))?.textContent ?? "";
  await openPat(); await settled(4);

  const anchor = (addr: string, dir = "in") => document.querySelector<HTMLElement>(`.msg.${dir} .mtext a.addr.mention[data-slug="${addr}"]`);
  const mark = (a: HTMLElement | null) => a?.querySelector<HTMLElement>(".mmark") ?? null;
  const letters = (e: Element | null) => (e ? getComputedStyle(e, "::before").content : "");
  const dmAvatar = (name: string) => row(name)?.querySelector<HTMLElement>(".av") ?? null;
  const looks = (e: Element | null) => { if (!e) return null; const s = getComputedStyle(e); return { bg: s.backgroundColor, fg: s.color, radius: s.borderRadius }; };
  const grey = (c: string) => { const m = /rgba?\((\d+), (\d+), (\d+)/.exec(c); return !!m && Math.abs(+m[1] - +m[2]) < 12 && Math.abs(+m[2] - +m[3]) < 12; };
  const isDot = (e: HTMLElement | null) => !!e && e.classList.contains("mdot") && !e.querySelector("img, svg") && e.offsetWidth >= 4 && e.offsetWidth <= 8 && grey(getComputedStyle(e).backgroundColor) && !/^"/.test(letters(e));
  const isAvatarLike = (e: HTMLElement | null, dm: HTMLElement | null) => !!e && !!dm && e.classList.contains("av") && JSON.stringify(looks(e)) === JSON.stringify(looks(dm));
  const toasts = () => [...document.querySelectorAll(".toast, .snack")].map((t) => t.textContent ?? "");
  const menuItems = () => [...document.querySelectorAll(".pop.ctx .mi")].map((b) => b.textContent ?? "");
  const rightClick = async (e: HTMLElement | null) => { if (!e) return; const r = e.getBoundingClientRect(); e.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, cancelable: true, clientX: r.x + 4, clientY: r.y + 4, button: 2 })); await settled(2); };
  const closeMenu = async () => { window.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true })); await settled(1); };

  check("the conversation with Pat is open", header() === "Pat", header());
  if (view === "marks" && platform === "desktop") {
    const alice = mark(anchor(ALICE)), alicesDm = dmAvatar("Alice");
    check("an accessible person's mention shows their DM-list letters and colours", isAvatarLike(alice, alicesDm) && alice!.classList.contains("k-person")
      && letters(alice) === JSON.stringify(alicesDm?.firstChild?.textContent ?? "") && alice!.textContent === "", { mark: alice?.outerHTML, dm: alicesDm?.outerHTML, letters: letters(alice) });
    const pat = mark(anchor(PAT)), patsDm = dmAvatar("Pat")?.querySelector("img");
    check("a picture mention shows the DM list's picture", !!pat?.querySelector("img") && !!patsDm && pat!.querySelector("img")!.getAttribute("src") === patsDm.getAttribute("src"), { mark: pat?.outerHTML.slice(0, 200) });
    const acme = mark(anchor(ACME)), acmesDm = dmAvatar("Acme Org");
    check("an org's mention shows its DM-list letters, colours and shape", isAvatarLike(acme, acmesDm) && acme!.classList.contains("k-org")
      && letters(acme) === JSON.stringify(acmesDm?.firstChild?.textContent ?? ""), { mark: acme?.outerHTML, letters: letters(acme) });
    const agent = mark(anchor(AGENT)), agentsDm = dmAvatar("Claude session");
    check("an agent session's mention shows the DM list's terminal icon", isAvatarLike(agent, agentsDm) && !!agent!.querySelector("svg path")
      && agent!.querySelector("svg path")!.getAttribute("d") === agentsDm?.querySelector("svg path")?.getAttribute("d"), { mark: agent?.outerHTML });
    const me = mark(anchor(ME)), mine = document.querySelector<HTMLElement>(".av.k-me:not(.mmark)");
    check("a mention of yourself shows your own avatar", isAvatarLike(me, mine) && letters(me) === JSON.stringify(mine?.firstChild?.textContent ?? ""), { mark: me?.outerHTML, mine: mine?.outerHTML });
    check("a sent message's mention shows its mark too", isAvatarLike(mark(anchor(ALICE, "out")), alicesDm), mark(anchor(ALICE, "out"))?.outerHTML);
    const ghost = anchor(GHOST), stranger = anchor(STRANGER);
    check("inaccessible mentions show a small grey dot and no picture", isDot(mark(ghost)) && isDot(mark(stranger)), { ghost: mark(ghost)?.outerHTML, stranger: mark(stranger)?.outerHTML });
    check("their tooltip says they aren't on your hubs", ghost?.title === "Ghost" + NOT_ON && stranger?.title === STRANGER + NOT_ON, { ghost: ghost?.title, stranger: stranger?.title });
    // Orgtree's lines for the same address (symmetry audit #2): presence and type, name, description, address, action
    const hovers = [anchor(ALICE)?.title, anchor(ACME)?.title, anchor(AGENT)?.title, anchor(ME)?.title];
    check("an accessible mention's tooltip reads as Orgtree's: presence and type, name, description, address, action", JSON.stringify(hovers) === JSON.stringify([
      `Online · person\nAlice\n@net:${ALICE}\nOpen chat`,
      `Online · org\nAcme Org\nAcme's agents answer here.\n@net:${ACME}\nOpen chat`,
      `Offline · independent chat\nClaude session\n@net:${AGENT}\nOpen chat`,
      `You\nScratch\n@net:${ME}\nOpen chat`,
    ]), hovers);
    const off = ghost && getComputedStyle(ghost), on = anchor(ALICE) && getComputedStyle(anchor(ALICE)!);
    check("an inaccessible mention's text is muted, without a pointer", !!off && !!on && off.color !== on.color && off.cursor !== "pointer", { off: off && [off.color, off.cursor], on: on && [on.color, on.cursor] });
    const code = [...document.querySelectorAll(".msg.in .mtext code")].find((c) => c.textContent === `@net:${ALICE}`);
    check("a mention in code stays code, without a mark", !!code && !code.querySelector(".mmark, a"), code?.outerHTML);
    const text = document.querySelector(".msg.in .mtext");
    const sel = getSelection()!; const range = document.createRange(); if (text) range.selectNodeContents(text); sel.removeAllRanges(); sel.addRange(range);
    const copied = sel.toString().replace(/\s+/g, " "); sel.removeAllRanges();
    check("a selection copies the words, not the letters", copied.includes(`Ask @net:${ALICE} or @net:${ACME}, or the session @net:${AGENT}. @net:${GHOST} and @net:${STRANGER} can't hear us.`), copied);
    check("the message HTML itself carries no mark (the sanitizer is unchanged)", !md(INCOMING).includes("mmark") && !md(INCOMING).includes("<img"));
  } else if (view === "menu" && platform === "desktop") {
    await click(anchor(GHOST));
    check("clicking an inaccessible mention opens nothing and says nothing", header() === "Pat" && !calls.some((c) => c === "open:" + GHOST) && !toasts().length, { header: header(), calls, toasts: toasts() });
    if (header() !== "Pat") { await openPat(); await settled(2); }
    await rightClick(anchor(ALICE));
    const accessibleMenu = menuItems(); await closeMenu();
    check("right-click on an accessible mention offers Open chat and Copy address", accessibleMenu.includes("Open chat") && accessibleMenu.includes("Copy address"), accessibleMenu);
    await click(anchor(ACME));
    check("clicking an accessible mention opens its chat", calls.includes("open:" + ACME) && header() === "Acme Org", { calls, header: header() });
    await openPat(); await settled(2);
    // left open for the capture
    await rightClick(anchor(GHOST));
    const offMenu = menuItems();
    check("right-click on an inaccessible mention offers Copy address, not Open chat", offMenu.includes("Copy address") && !offMenu.includes("Open chat"), offMenu);
  } else if (view === "live" && platform === "desktop") {
    const before = anchor(GHOST);
    hubs = [hub(A, "Hub A", "connected"), hub(B, "Hub B", "connected")];
    await refreshAll(); await settled(4);
    const ghost = anchor(GHOST);
    check("enabling a hub turns a mention into its avatar, live, without re-rendering the message", ghost === before && !!mark(ghost)?.classList.contains("av")
      && letters(mark(ghost)) === '"G"' && ghost?.title === `Online · person\nGhost\n@net:${GHOST}\nOpen chat`, { same: ghost === before, mark: mark(ghost)?.outerHTML, title: ghost?.title });
    directory = directory.filter((c) => c.address !== ALICE);
    await refreshAll(); await settled(4);
    check("a contact leaving the directory turns its mention into the dot, live", isDot(mark(anchor(ALICE))) && anchor(ALICE)?.title === ALICE + NOT_ON, { mark: mark(anchor(ALICE))?.outerHTML, title: anchor(ALICE)?.title });
  } else if (platform === "android" && view === "marks") {
    const alice = mark(anchor(ALICE));
    check("an accessible mention shows its DM-list avatar", !!alice?.classList.contains("av") && alice.classList.contains("k-person") && letters(alice) === '"A"' && alice.textContent === "", alice?.outerHTML);
    check("an inaccessible mention shows the grey dot", isDot(mark(anchor(GHOST))), mark(anchor(GHOST))?.outerHTML);
    check("its tooltip says they aren't on your hubs", anchor(GHOST)?.title === "Ghost" + NOT_ON, anchor(GHOST)?.title);
  } else if (platform === "android" && view === "tap") {
    await click(anchor(GHOST));
    // left showing for the capture
    check("tapping an inaccessible mention says why once, and stays in the chat", header() === "Pat" && JSON.stringify(toasts()) === JSON.stringify(["Ghost" + NOT_ON]) && !calls.includes("open:" + GHOST), { header: header(), toasts: toasts(), calls });
  } else if (platform === "android" && view === "open") {
    await click(anchor(ACME)); await settled(2);
    check("tapping an accessible mention opens its chat", header() === "Acme Org", header());
  }
  window.inlineMentionsResult = { platform, view, checks, calls };
}
run().catch((e) => { window.inlineMentionsError = String(e?.stack || e); });
