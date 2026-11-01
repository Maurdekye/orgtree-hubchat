// Mounted App, synthetic API only: translation between people (Hubchat 1.3.0 with Mail hub 2.4.0, feature
// `translate`; Orgtree docket mail-hub-hubchat-automatic-translation-between-u; the user's answers 2026-11-01 12:15Z
// and "Build it" 12:16Z; design hubchat-translation-design-1-3-0). A message the hub translated for us shows its
// translation, with one small grey line under it, "Translated from Hebrew · Show original", that toggles in place
// ("Original · Show translation"); the desktop menu has the same toggle after Copy text (Copy copies what shows; not
// clicked here, as the capture guard refuses every clipboard attempt).
// A message that wasn't translated says why (too long, the hub's monthly limit, couldn't); none needed or still on
// its way: no line. Reply quotes and the chat list show the translation. Message info shows both versions. Settings ›
// Profile: "Translate messages into" (Off and the languages in their own names), saved with the profile, its caption
// from the connected hubs; disabled when none can translate.
// ?platform=desktop|android
import { createRoot } from "react-dom/client";
import App from "../src/App";
import { api, type Contact, type HubStatus, type Message } from "../src/api";
import { mockApi } from "../src/lib/mock";
import { refreshState } from "../src/lib/store";

type Check = { name: string; ok: boolean; detail?: unknown };
declare global {
  interface Window {
    trFixture?: { checks: Check[]; platform: string; step: (name: string) => Promise<unknown> };
    trFixtureError?: string;
  }
}
const sleep = (ms: number) => new Promise<void>((r) => setTimeout(r, ms));
const until = async (f: () => boolean, ms: number) => { const t = performance.now(); while (performance.now() - t < ms) { if (f()) return true; await sleep(25); } return f(); };

const A = "wss://hub-a.invalid", B = "wss://hub-b.invalid";
const ME = "scratch.abcdef", DANI = "dani.555555";
const ON_CAPTION = "Translated by your hub using Claude. Messages to you leave the hub to be translated.";

async function run() {
  const platform = new URLSearchParams(location.search).get("platform") === "android" ? "android" : "desktop";
  const checks: Check[] = [];
  const check = (name: string, ok: unknown, detail?: unknown) => { checks.push({ name, ok: !!ok, ...(ok ? {} : { detail }) }); };
  let language = "", hubsMode: "mixed" | "off" | "none" = "mixed";
  const told: string[] = [];
  const hub = (url: string, name: string): HubStatus => {
    const translate = hubsMode === "none" ? false : url === A;
    return ({ url, name, state: "connected", error: null, retry_at_ms: null, max_attachment_bytes: 1 << 30,
      features: translate ? ["v2", "status", "translate"] : ["v2", "status"], translation: translate ? (hubsMode === "off" ? "off" : "ok") : null });
  };
  const directory: Contact[] = [
    { address: DANI, kind: "person", org_name: "", username: "Dani", blurb: "", online: true, busy: false, dnd: false, last_seen: null, hubs: [A], picture: null } as Contact,
  ];
  let n = 0;
  const message = (id: string, body: string, extra: Partial<Message> = {}): Message => ({ id, peer: DANI, outgoing: false, hub: A, body, kind: null, reply_to: null,
    created_at: new Date(1700000000000 + n++ * 60000).toISOString(), sent_at: null, received_at: null, state: "received", fetched_at: null, delivered_at: null,
    read_at: null, error: null, seen: true, attachments: [], ...extra } as Message);
  const msgs: Message[] = [
    message("t-hi", "שלום, מה שלומך?", { translation: { lang: "en", source_lang: "he", status: "translated", body: "Hi, how are you?" } }),
    message("t-long", "מכתב ארוך", { translation: { lang: "en", status: "too_long" } }),
    message("t-limit", "עוד הודעה", { translation: { lang: "en", status: "limit" } }),
    message("t-failed", "הודעה שלישית", { translation: { lang: "en", status: "failed" } }),
    message("t-pending", "רגע אחד", { translation: { lang: "en", status: "pending" } }),
    message("t-plain", "Same language, nothing to translate"),
    message("t-mine", "I'm fine, thanks", { outgoing: true, state: "delivered", translation: { lang: "he", status: "translated", body: "should never show" } }),
    message("t-reply", "תודה!", { reply_to: "t-hi", translation: { lang: "en", source_lang: "he", status: "translated", body: "Thanks!" } }),
  ];
  Object.assign(api, mockApi, {
    state: async () => ({ ...await mockApi.state(), me: { id: "scratch", address: ME, name: "Scratch", about: "" }, hubs: [hub(A, "Hub A"), hub(B, "Hub B")],
      platform, recovery_saved: true, file_background_hint: null, status: "auto", language }),
    setLanguage: async (l: string) => { told.push(l); language = l; },
    setProfile: async () => { throw Error("only the language changed: the profile is not sent"); },
    directory: async () => directory,
    chats: async () => [{ peer: DANI, last: msgs.at(-1), unread: 0 }],
    chat: async (p: string) => (p === DANI ? msgs : []),
    message: async (id: string) => msgs.find((m) => m.id === id) ?? null,
    picture: async () => null,
    transfers: async () => ({ rows: [], next: null }), onEvent: async () => () => {},
    openDm: async () => {}, markRead: async () => {}, draft: async () => "", setDraft: async () => {}, draftAnswers: async () => ({ replies: [], picks: {} }),
    setupStatus: async () => null,
    sendRoute: async () => ({ pinned: null, automatic: A, next: A, hubs: [] }),
    loadOlder: async () => ({ added: 0, more: [], unreachable: [] }),
  });
  // each view starts clean (useLastChat reopens the last chat from localStorage; the views share one session)
  localStorage.clear(); sessionStorage.clear();
  const root = createRoot(document.getElementById("root")!); root.render(<App />);
  await sleep(300);

  const settle = () => sleep(250);
  const bubble = (id: string) => document.querySelector<HTMLElement>(`#m-${id} .bubble`);
  // the message's own words: its time and ticks (spliced into the text's last line) left out
  const text = (id: string) => {
    const t = bubble(id)?.querySelector(".mtext, .mbody")?.cloneNode(true) as HTMLElement | undefined;
    t?.querySelectorAll(".meta, .meta-line").forEach((e) => e.remove());
    return t ? (t.textContent ?? "").trim() : null;
  };
  const line = (id: string) => bubble(id)?.querySelector(".tr-line")?.textContent ?? null;
  const row = () => [...document.querySelectorAll<HTMLElement>(".crow")].find((e) => e.querySelector(".crow-name")?.textContent === "Dani");
  const me = () => document.querySelector<HTMLElement>(platform === "desktop" ? ".side-head .me-status" : ".appbar .me-status");
  const back = async () => {
    if (platform === "desktop") window.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape" }));
    else document.querySelector<HTMLElement>('.appbar [aria-label="Back"]')?.click();
    await settle();
  };
  const menuItems = () => [...document.querySelectorAll<HTMLElement>('.ctx [role="menuitem"], [role="menu"] .mi')].map((b) => b.textContent?.trim() ?? "");
  const openCtx = async (id: string) => {
    const b = bubble(id)!.querySelector<HTMLElement>(".mtext, .mbody") ?? bubble(id)!;
    const r = b.getBoundingClientRect();
    b.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, cancelable: true, clientX: r.left + 5, clientY: r.top + 5 }));
    await until(() => menuItems().length > 0, 3000); await settle();
  };
  const clickItem = async (label: string) => {
    [...document.querySelectorAll<HTMLElement>('[role="menuitem"]')].find((b) => b.textContent?.trim() === label)?.click();
    await settle();
  };
  const select = () => document.querySelector<HTMLSelectElement>("#pf-lang");
  const help = () => select()?.closest(".field")?.querySelector(".help")?.textContent ?? null;
  const openProfile = async () => {
    me()?.click(); await until(() => !!document.querySelector('[role="menu"][aria-label="Your status"]'), 3000); await settle();
    [...document.querySelectorAll<HTMLElement>('[role="menu"][aria-label="Your status"] .mi')].find((i) => i.textContent === "Profile")?.click();
    await until(() => !!document.querySelector("#pf-name"), 3000); await settle();
  };

  const steps: Record<string, () => Promise<unknown>> = {
    "chat": async () => {
      await until(() => !!row(), 3000); await settle();
      const prev = row()?.querySelector(".crow-prev")?.textContent ?? null;
      check("the chat list shows the last message's translation", prev === "Thanks!", prev);
      row()!.click();
      await until(() => !!bubble("t-reply"), 3000); await settle();
      const hi = { text: text("t-hi"), line: line("t-hi") };
      check("a translated message shows its translation, with one grey line: Translated from Hebrew · Show original",
        hi.text === "Hi, how are you?" && hi.line === "Translated from Hebrew · Show original", hi);
      const why = ["t-long", "t-limit", "t-failed"].map(line);
      check("a message that wasn't translated says why, and shows its original",
        JSON.stringify(why) === JSON.stringify(["Not translated: too long", "Not translated: the hub's monthly translation limit is reached", "Couldn't translate"])
          && text("t-long") === "מכתב ארוך", { why, long: text("t-long") });
      const none = ["t-pending", "t-plain", "t-mine"].map((id) => ({ id, line: line(id), text: text(id) }));
      check("no line when none was needed, while it is on its way, or on our own message (its own text shows)",
        none.every((x) => x.line === null) && none[2].text === "I'm fine, thanks" && none[0].text === "רגע אחד", none);
      const q = bubble("t-reply")?.querySelector(".quote .q-txt")?.textContent ?? null;
      check("a reply's quote shows the reader's translation", q === "Hi, how are you?" && text("t-reply") === "Thanks!", { q, t: text("t-reply") });
      return { hi, why, q };
    },
    "original": async () => {
      bubble("t-hi")?.querySelector<HTMLElement>(".tr-toggle")?.click(); await settle();
      const o = { text: text("t-hi"), line: line("t-hi"), dir: bubble("t-hi")?.querySelector(".mtext")?.getAttribute("dir") ?? null };
      check("Show original: the original in place, the line now Original · Show translation", o.text === "שלום, מה שלומך?" && o.line === "Original · Show translation" && o.dir === "auto", o);
      return o;
    },
    "translation": async () => {
      bubble("t-hi")?.querySelector<HTMLElement>(".tr-toggle")?.click(); await settle();
      const t = { text: text("t-hi"), line: line("t-hi") };
      check("Show translation: back to the translation", t.text === "Hi, how are you?" && t.line === "Translated from Hebrew · Show original", t);
      return t;
    },
    "menu": async () => {
      await openCtx("t-hi");
      const items = menuItems();
      const at = items.indexOf("Copy text");
      check("desktop menu: Show original right after Copy text", at >= 0 && items[at + 1] === "Show original", items);
      // (Copy text is not clicked: the capture guard counts any clipboard attempt as a failure)
      await clickItem("Show original");
      const shown = text("t-hi");
      await openCtx("t-hi");
      const items2 = menuItems();
      await clickItem("Show translation");
      check("the menu's Show original shows the original, and the menu then offers Show translation",
        shown === "שלום, מה שלומך?" && items2.includes("Show translation") && !items2.includes("Show original") && text("t-hi") === "Hi, how are you?", { shown, items2, after: text("t-hi") });
      return items2;
    },
    "info": async () => {
      await openCtx("t-hi"); await clickItem("Message info");
      const sec = () => [...document.querySelectorAll<HTMLElement>(".info-sec, .pad")].find((s) => [...s.querySelectorAll("h4, .sec-h")].some((h) => h.textContent === "Translation"));
      await until(() => !!sec(), 3000); await settle();
      const kv = [...(sec()?.querySelectorAll(".kv") ?? [])].map((k) => [k.querySelector(".k")?.textContent, k.querySelector(".v")?.textContent]);
      check("Message info shows both versions and the languages",
        JSON.stringify(kv.slice(-3)) === JSON.stringify([["Translated", "From Hebrew into English, by your hub"], ["Original", "שלום, מה שלומך?"], ["Translation", "Hi, how are you?"]]), kv);
      return kv;
    },
    "info-close": async () => { await back(); await settle(); return true; },
    "profile": async () => {
      if (platform === "android") { await back(); await until(() => !!me(), 3000); }
      await openProfile();
      const s = select();
      const opts = [...(s?.options ?? [])].map((o) => o.textContent);
      check("Settings › Profile: Translate messages into, Off then the languages in their own names; Off chosen",
        !!s && s.value === "" && opts[0] === "Off" && opts.length === 33 && opts.includes("עברית") && opts.includes("English") && opts.includes("Español")
          && s.closest(".field")?.querySelector("label")?.textContent === "Translate messages into", { value: s?.value, n: opts.length, first: opts.slice(0, 4) });
      check("its caption: the hub translates, text leaves it; a connected hub that can't is named",
        help() === ON_CAPTION + " · Hub B can't translate yet" && !s?.disabled, help());
      return help();
    },
    "pick": async () => {
      const s = select();
      if (s) {
        // React's own value setter, so its change handler sees the pick
        Object.getOwnPropertyDescriptor(HTMLSelectElement.prototype, "value")!.set!.call(s, "he");
        s.dispatchEvent(new Event("change", { bubbles: true })); await settle();
      }
      const saveBtn = [...document.querySelectorAll<HTMLButtonElement>("button")].find((b) => b.textContent === "Save");
      const enabled = !!saveBtn && !saveBtn.disabled;
      saveBtn?.click();
      await until(() => told.length > 0, 3000); await settle();
      check("picked and saved with the Profile's Save: the language alone is told", enabled && JSON.stringify(told) === JSON.stringify(["he"]) && select()?.value === "he", { enabled, told, value: select()?.value });
      return told;
    },
    "hub-off": async () => {
      hubsMode = "off"; await refreshState(); await settle();
      check("a hub with the feature that doesn't translate: says so", help() === "Your hub doesn't translate. · Hub B can't translate yet", help());
      return help();
    },
    "no-hub": async () => {
      hubsMode = "none"; language = ""; await back(); await until(() => !!me(), 3000); await refreshState(); await openProfile();
      check("no connected hub can translate: the row is disabled and names them", select()?.disabled === true && help() === "Hub A, Hub B can't translate yet.", { disabled: select()?.disabled, help: help() });
      return help();
    },
    "no-hub-end": async () => { hubsMode = "mixed"; await refreshState(); await settle(); return true; },
  };
  window.trFixture = { checks, platform, step: async (name) => { const f = steps[name]; if (!f) throw Error("no step " + name); return f(); } };
}
run().catch((e) => { window.trFixtureError = String(e?.stack || e); });
