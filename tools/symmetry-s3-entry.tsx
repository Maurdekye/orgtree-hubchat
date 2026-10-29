// Mounted real App on the synthetic API only (no hub, file, opener or network): the Hubchat labels, hovers and
// keyboard access that the UX symmetry audit found missing next to their siblings (docket
// symmetry-fixes-s3-hubchat-labels-hovers-and-keyb; audit #5, 8, 16, 17, 18, 22, 29, 30, 36, 42, 44, 47, 50).
// run("desktop" | "android") returns every check by name; the contained driver tools/proof-symmetry-s3.mjs mounts it in
// a DOM with no window, and tools/symmetry-s3-electron.cjs mounts it in an offscreen window for the captures.
import { createRoot } from "react-dom/client";
import App from "../src/App";
import { api, type Contact, type HubStatus, type Message, type TransferView } from "../src/api";
import { mockApi } from "../src/lib/mock";
import { setThemePref } from "../src/lib/theme";

export interface Result { platform: string; checks: { name: string; ok: boolean }[]; notes: Record<string, string | null> }
/** The capture host's hook: take a screenshot named `name` now (Electron only; the DOM proof passes none). */
export type Shot = (name: string) => Promise<void>;
declare global { interface Window { symmetryS3Run?: (platform: "desktop" | "android") => Promise<Result>; symmetryS3Result?: Result; symmetryShot?: string | null; symmetryShotDone?: () => void } }

const settle = () => new Promise<void>((r) => setTimeout(r, 60));
const q = (s: string, r: ParentNode = document) => r.querySelector<HTMLElement>(s);
const qa = (s: string, r: ParentNode = document) => [...r.querySelectorAll<HTMLElement>(s)];
const textOf = (e: Element | null | undefined) => e?.textContent?.trim() ?? "";
const click = async (e: Element | null | undefined) => { (e as HTMLElement | null | undefined)?.click(); await settle(); await settle(); };
const press = async (e: Element | null | undefined, key: string) => {
  e?.dispatchEvent(new KeyboardEvent("keydown", { key, bubbles: true, cancelable: true }));
  await settle(); await settle();
};

const ME = "scratch.abcdef", ALEX = "alexandria.a1b2c3", CASEY = "casey.abcdef";
const HUB = "wss://fixture.invalid";
const LONG_NAME = "Alexandria Montgomery-Featherstonehaugh of the Northern Reading Circle";
const LONG_ABOUT = "Keeps the shared notes, the long agenda and every follow-up from the quarterly review in one place for everybody";
const hub: HubStatus = { url: HUB, name: "Fixture Hub", state: "connected", error: null, retry_at_ms: null, max_attachment_bytes: 1048576, features: [] };
const contact = (address: string, username: string, blurb: string): Contact => ({ address, kind: "person", org_name: "", username, blurb, online: true, last_seen: null, hubs: [HUB] });
const message = (id: string, outgoing: boolean, body: string, at: string, reply_to: string | null = null): Message => ({
  id, peer: ALEX, outgoing, hub: HUB, body, kind: null, reply_to, sent_at: at, received_at: at, created_at: at,
  state: outgoing ? "sent" : "received", fetched_at: null, delivered_at: null, read_at: null, error: null, seen: true, attachments: [],
} as Message);

export async function run(platform: "desktop" | "android", shot?: Shot): Promise<Result> {
  localStorage.clear(); sessionStorage.clear();
  if (!Element.prototype.scrollIntoView) Element.prototype.scrollIntoView = function () {};
  const checks: Result["checks"] = [], notes: Result["notes"] = {};
  const check = (name: string, ok: unknown) => checks.push({ name, ok: !!ok });
  const calls: string[] = [];
  // a screenshot of the current state, optionally with the keyboard focus on one element (and whether Chromium draws it as :focus-visible)
  const snap = async (name: string, focus?: HTMLElement | null) => {
    if (focus) { focus.focus(); await settle(); try { notes["focus-visible:" + name] = String(focus.matches(":focus-visible")); } catch { notes["focus-visible:" + name] = "unsupported"; } }
    await shot?.(name);
  };
  const first = message("m-1", false, "Where should the notes go?", "2026-10-27T09:05:07Z");
  const reply = message("m-2", true, "Into the shared folder.", "2026-10-28T13:25:12Z", "m-1");
  const active: TransferView = { local_id: "active", message_id: "message", created_at: "2026-10-28T00:00:00Z", peer: CASEY, name: "archive.zip", upload: false, state: "running", done: 40, total: 100, error: null };
  Object.assign(api, mockApi, {
    state: async () => ({ ...await mockApi.state(), me: { id: "scratch", address: ME, name: "Scratch", about: "" }, hubs: [hub], platform, recovery_saved: true, file_background_hint: null }),
    directory: async () => [contact(ALEX, LONG_NAME, LONG_ABOUT), contact(CASEY, "Casey", "")],
    chats: async () => [{ peer: ALEX, last: reply, unread: 3 }, { peer: CASEY, last: null, unread: 0 }],
    chat: async (p: string) => (p === ALEX ? [first, reply] : []),
    message: async () => null,
    transfers: async () => ({ rows: [active], next: null }),
    transferHistory: async () => ({ rows: [], next: null }),
    cancelTransfer: async (id: string) => { calls.push("cancel:" + id); },
    onEvent: async () => () => {}, openDm: async () => {}, markRead: async () => {}, draft: async () => "", setDraft: async () => {},
    draftAnswers: async () => ({ replies: [], picks: {} }), setupStatus: async () => null,
    sendRoute: async () => ({ pinned: null, automatic: HUB, next: HUB, hubs: [] }),
    loadOlder: async () => ({ added: 0, more: [], unreachable: [] }),
  });
  createRoot(document.getElementById("root")!).render(<App />); await settle(); await settle(); await settle();
  const settings = () => click(q('[aria-label="Settings"]'));

  // ---- the chat list: #16 the full name on hover, #22 "N unread", #44 the rail's hub button (desktop)
  const row = () => qa(".crow").find((e) => textOf(q(".crow-name", e)) === LONG_NAME);
  check("the chat list shows the long name", !!row());
  check("#16 the expanded row's name has the full-name hover", q(".crow-name", row())?.getAttribute("title") === LONG_NAME);
  const badge = q(".badge", row());
  check("the chat row keeps its visible unread number", textOf(badge) === "3");
  check("#22 the expanded row's unread badge is labelled \"3 unread\"", badge?.getAttribute("aria-label") === "3 unread");
  await snap(platform + "-1-chats");
  if (platform === "desktop") {
    const summary = textOf(q(".hubpill span:not(.dot)"));
    check("the expanded hub button shows the hub summary", summary === "Connected to Fixture Hub");
    await click(q('[aria-label="Shrink the chat list"]'));
    const rail = q(".rail-hub"), rrow = qa(".rrow")[0];
    check("the rail row names the chat and its unread count (control)", rrow?.getAttribute("title") === LONG_NAME && rrow?.getAttribute("aria-label") === LONG_NAME + ", 3 unread");
    check("#44 the collapsed hub button is announced with the hub summary", rail?.getAttribute("aria-label") === summary + " · Hub connections");
    check("the collapsed hub button keeps its hover (control)", rail?.getAttribute("title") === summary + " · Hub connections (F4)");
    await snap("desktop-2-rail");
    await click(q('[aria-label="Show the chat list"]'));

    // ---- the directory: #18 full address and description on hover
    await click(q('[aria-label="Directory"]'));
    const dir = qa(".dir-row").find((e) => textOf(q(".t1", e)).startsWith("Alexandria"));
    check("the directory lists the contact", !!dir);
    check("#18 a directory address has the full-text hover", q(".t2", dir)?.getAttribute("title") === ALEX);
    check("#18 a directory description has the full-text hover", q(".t3", dir)?.getAttribute("title") === LONG_ABOUT);
    await snap("desktop-3-directory");
    await click(dir);
  } else {
    await click(row());
  }

  // ---- the conversation: #42 header picture hint, #50 message time hover, #36 quote by keyboard, #17 attachment chip
  check("the conversation opens", !!q(".composer"));
  if (platform === "desktop") {
    check("#42 the conversation header picture says Contact info", q(".conv-av")?.getAttribute("title") === "Contact info");
    check("the header's name keeps its Contact info hover (control)", q(".conv-head .who")?.getAttribute("title") === "Contact info");
  }
  const times = qa(".bubble .mt");
  check("the conversation shows two message times", times.length === 2);
  const full = /^\d{4}-\d\d-\d\d \d\d:\d\d:\d\d(\s.+)?$/;
  check("#50 a message time has a full date-and-time hover", times.length === 2 && times.every((t) => full.test(t.getAttribute("title") || "")));
  check("#50 the full time agrees with the visible clock", times.length === 2 && times.every((t) => (t.getAttribute("title") || "").slice(11, 16) === textOf(t)));
  check("#50 the time is announced with its full date and time", times.length === 2 && times.every((t) => full.test(t.getAttribute("aria-label") || "")));
  const quote = q(".bubble .quote");
  check("the reply shows its quote", !!quote && textOf(quote).includes("Where should the notes go?"));
  check("#36 the quote is focusable and announced as a button", quote?.tabIndex === 0 && quote?.getAttribute("role") === "button");
  check("the quote keeps its Show the original message hover (control)", quote?.getAttribute("title") === "Show the original message");
  const target = () => document.getElementById("m-m-1");
  target()?.classList.remove("flash");
  await press(quote, "Enter");
  check("#36 Enter on the quote jumps to the original", !!target()?.classList.contains("flash"));
  target()?.classList.remove("flash");
  await press(quote, " ");
  check("#36 Space on the quote jumps to the original", !!target()?.classList.contains("flash"));
  target()?.classList.remove("flash");
  await press(quote, "a");
  check("another key on the quote does nothing (control)", !target()?.classList.contains("flash"));
  const random = Math.random; Math.random = () => 0;
  await click(q('[aria-label="Attach files"]'));
  Math.random = random;
  const chip = q(".attchip");
  check("a non-image attachment waits in the composer as a chip", !!chip && textOf(q(".ell", chip)) === "nightly-logs.zip");
  check("#17 the chip's hover is the filename and size", !!chip && chip.getAttribute("title") === "nightly-logs.zip · " + textOf(q(".s", chip)));
  await snap(platform + "-" + (platform === "desktop" ? "4" : "2") + "-conversation", quote);
  await click(q(".attchip [aria-label^=\"Remove\"]"));
  if (platform === "android") await click(q('.appbar [aria-label="Back"]'));

  // ---- Settings
  await settings();
  if (platform === "desktop") {
    // #8 an active transfer row opens its chat from the keyboard, Cancel stays its own control
    await click(qa(".set-nav button").find((e) => textOf(e).includes("Transfers")));
    const panel = () => q(".transfers-panel");
    const xrow = () => q(".transfers-panel .xrow:not(.xhistory-row)");
    check("settings lists the active transfer", textOf(xrow()).includes("archive.zip"));
    check("#8 the active transfer row is focusable", xrow()?.tabIndex === 0);
    await snap("desktop-5-transfers", xrow());
    await press(q('.xrow:not(.xhistory-row) [aria-label="Cancel"]', panel() ?? document), "Enter");
    check("a key on Cancel does not open the chat (control)", !!panel() && !calls.includes("cancel:active"));
    await click(q('.xrow:not(.xhistory-row) [aria-label="Cancel"]'));
    check("clicking Cancel still cancels (control)", calls.includes("cancel:active") && !!panel());
    await press(xrow(), "Enter");
    check("#8 Enter on the active transfer row opens its conversation", !!q(".timeline") && !panel() && textOf(q(".conv-head .name")) === "Casey");
    await settings(); await click(qa(".set-nav button").find((e) => textOf(e).includes("Transfers")));
    await press(xrow(), " ");
    check("#8 Space on the active transfer row opens its conversation", !!q(".timeline") && !panel() && textOf(q(".conv-head .name")) === "Casey");
    // #47 the system theme option
    await settings(); await click(qa(".set-nav button").find((e) => textOf(e).includes("Appearance")));
    const labels = qa(".rcard .t1").map(textOf);
    notes.systemLabel = labels[2] ?? null;
    await snap("desktop-6-appearance");
    check("desktop calls the system theme option Match system (control)", labels.join("|") === "Dark|Light|Match system");
  } else {
    // ---- Android settings list: #30 the profile card by keyboard
    const card = q(".profile-card");
    check("the settings list shows the profile card", !!card);
    check("#30 the profile card is focusable and announced as a button", card?.tabIndex === 0 && card?.getAttribute("role") === "button");
    await snap("android-3-settings", card);
    await press(q(".profile-card [aria-label=\"Copy address\"]"), "Enter");
    check("a key on the card's Copy address does not open Profile (control)", textOf(q(".appbar .title")) === "Settings");
    await press(card, "Enter");
    check("#30 Enter on the profile card opens Profile", textOf(q(".appbar .title")) === "Profile");
    // (leave Profile only if it opened, so a missing key handler cannot send the next steps back to the chat list)
    if (textOf(q(".appbar .title")) === "Profile") await click(q('.appbar [aria-label="Back"]'));
    await press(q(".profile-card"), " ");
    check("#30 Space on the profile card opens Profile", textOf(q(".appbar .title")) === "Profile");
    if (textOf(q(".appbar .title")) === "Profile") await click(q('.appbar [aria-label="Back"]'));
    // ---- #29 the theme choices are real radios, #47 one label for the system option
    await click(qa(".li").find((e) => textOf(q(".t1", e)) === "Appearance"));
    check("Appearance opens", textOf(q(".appbar .title")) === "Appearance");
    const radios = qa('input[type="radio"]') as HTMLInputElement[];
    check("#29 the theme choices are native radio buttons", radios.length === 3);
    check("#29 they form one named radio group", radios.length === 3 && new Set(radios.map((r) => r.name)).size === 1 && !!radios[0].name && !!q('[role="radiogroup"]'));
    const names = radios.map((r) => textOf(q(".t1", r.closest("label") ?? r)));
    check("#29 each radio is named by its option", radios.length === 3 && names.join("|") === "Dark|Light|Match system");
    check("#29 Dark is the checked radio by default", radios[0]?.checked === true && radios[1]?.checked === false);
    check("#29 no clickable div claims the radio role", qa('div[role="radio"]').length === 0);
    await snap("android-4-appearance-focus", radios[1]);
    notes.systemLabel = names[2] ?? textOf(qa(".li .t1")[2]) ?? null;
    // arrow keys move the focus together with the choice, a script click() does not: move it first, as the keys would
    radios[2]?.focus();
    await click(radios[2]);
    setThemePref("system"); await settle(); // the same choice, whether or not the radios could make it
    const after = qa('input[type="radio"]') as HTMLInputElement[];
    check("#29 the focus follows the choice: the chosen radio holds it", after.length === 3 && document.activeElement === after[2]);
    check("#29 choosing Match system checks that radio and only it", after.length === 3 && after[2].checked && !after[0].checked && !after[1].checked);
    check("choosing a theme marks its visual radio (control)", qa(".radio.on").length === 1);
    await snap("android-5-appearance-system", after[2]);
    await click(q('.appbar [aria-label="Back"]'));
    const appearance = qa(".li").find((e) => textOf(q(".t1", e)) === "Appearance");
    check("#47 the settings list calls the system theme option Match system", textOf(q(".t2", appearance)) === "Match system");
  }
  return { platform, checks, notes };
}

// the capture host starts a run and polls window.symmetryShot / window.symmetryS3Result (tools/symmetry-s3-electron.cjs)
if (typeof window !== "undefined") {
  window.symmetryS3Run = (platform) => run(platform, async (name) => {
    window.symmetryShot = name;
    await new Promise<void>((done) => { window.symmetryShotDone = done; });
  }).then((result) => (window.symmetryS3Result = result));
}
