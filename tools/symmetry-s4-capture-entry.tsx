// Symmetry S4 captures: each scene mounts the real components with a synthetic API, in the real stylesheets, and the
// Electron driver (symmetry-s4-capture.cjs) takes the pictures. No hub, native file, network or clipboard.
import { createElement as h, useState } from "react";
import { createRoot } from "react-dom/client";
import { api } from "../src/api";
import * as store from "../src/lib/store";
import { PlatformCtx, Toasts } from "../src/components/ui";
import { CtxMenu } from "../src/components/CtxMenu";
import { NewChatResults } from "../src/components/NewChat";
import { ContactInfo } from "../src/components/ContactInfo";
import { TransfersPanel } from "../src/components/Transfers";
import { SettingsModal, SettingsSection, type SetTab } from "../src/components/Settings";
import { Conversation } from "../src/components/Conversation";

declare global { interface Window { scene?: Record<string, () => Promise<void> | void>; sceneReady?: boolean; sceneError?: string; sceneFacts?: Record<string, unknown> } }
const q = new URLSearchParams(location.search);
const platform = q.get("platform") === "android" ? "android" : "desktop";
const scene = q.get("scene") || "";
const A = "https://office.invalid", B = "https://lab.invalid";
const hubs = [
  { url: A, name: "office", state: "connected", error: null, retry_at_ms: null, max_attachment_bytes: 1e9, features: ["v2"] },
  { url: B, name: "lab", state: "disconnected", error: "connection refused", retry_at_ms: null, max_attachment_bytes: 25e6, features: [] },
];
const person = (address: string, name: string, blurb: string, online: boolean, on: string[]) => ({ address, kind: "person", org_name: "", username: name, blurb, online, last_seen: new Date(Date.now() - 3 * 3600e3).toISOString(), hubs: on });
const maya = person("maya.e71f2b", "Maya Lin", "Design · Android", false, [A, B]);
const pat = person("pat.4d5e6f", "Pat Peer", "Started on this device today", true, [A, B]);
const kim = person("kim.19ac02", "Kim Okafor", "", false, [A]);
const msg = (id: string, peer: string, body: string, extra: object = {}) => ({ id, peer, outgoing: false, hub: null, body, kind: null, reply_to: null, created_at: "2026-10-29T10:00:00Z", sent_at: null, received_at: null, state: "received", attachments: [], error: null, seen: true, fetched_at: null, delivered_at: null, read_at: null, ...extra });
const done = () => { window.sceneReady = true; };
const settle = (n = 8) => new Promise<void>((r) => { let i = 0; const f = () => (++i >= n ? r() : setTimeout(f, 20)); f(); });

let receipts = false;
async function seed(extra: Record<string, unknown> = {}) {
  Object.assign(api, {
    state: async () => ({ me: { id: "me", address: "scratch.123456", name: "Me", about: "hello" }, hubs, platform, recovery_saved: true, read_receipts: receipts, file_background_hint: null, push: null, stay_connected: null }),
    chats: async () => [{ peer: maya.address, last: null, unread: 0 }], directory: async () => [maya, pat, kim], transfers: async () => ({ rows: [], next: null }),
    onEvent: async () => () => {}, onSettingsChanged: () => Promise.resolve(() => {}), draft: async () => "", setDraft: async () => {}, draftAnswers: async () => ({ replies: [], picks: {} }),
    sendRoute: async () => ({ pinned: null, automatic: null, next: null, hubs: [] }), markRead: async () => {}, openDm: async () => {}, setupStatus: async () => null,
    loadOlder: async () => ({ added: 0, more: [], unreachable: [] }), savePasted: async (name: string) => "C:/pasted/" + name, fileInfo: async (s: string) => ({ name: s.split("/").pop(), bytes: 248000 }),
    setReadReceipts: async (v: boolean) => { receipts = v; }, ...extra,
  });
  await store.refreshAll();
}

async function main() {
  const root = createRoot(document.getElementById("root")!);
  const wrap = (...kids: unknown[]) => h(PlatformCtx.Provider, { value: platform }, h(CtxMenu), ...(kids as never[]), h(Toasts));
  const noop = () => {};
  window.scene = {};
  if (scene === "newchat") {
    await seed();
    const r = { valid: true, address: "maya", exact: null, matches: [maya, pat, kim], is_me: false };
    root.render(wrap(h("div", { style: { padding: 12, maxWidth: platform === "desktop" ? 560 : undefined }, id: "stage" }, h(NewChatResults, { q: "m", r: r as never, onOpen: noop, onInfo: noop, onDirectory: noop }))));
    window.scene.menu = async () => { const row = document.querySelector(".nc-row")!; row.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, cancelable: true, clientX: 180, clientY: 120 })); await settle(); };
  } else if (scene.startsWith("files-")) {
    const mode = scene.slice(6);
    const peer = "casey.abcdef";
    const att = (id: string, name: string, bytes: number, st = "remote") => ({ local_id: id, name, bytes, hub_id: id, source: null, local_path: null, state: st, error: null });
    await seed({
      chat: async () => {
        if (mode === "loading") return new Promise(() => {});
        if (mode === "error") throw "The conversation could not be read (disk is busy).";
        return mode === "none" ? [msg("m0", peer, "hi")] : [msg("m1", peer, "files", { attachments: [att("a", "report.pdf", 1200000), att("b", "notes.txt", 3400), att("c", "release.zip", 48000000)] })];
      },
      directory: async () => [person(peer, "Casey", "Runs the lab hub", true, [A])],
    });
    root.render(wrap(h("div", { id: "stage", style: platform === "desktop" ? { width: 360, height: 700 } : { height: "100vh" } }, h(ContactInfo, { peer, onClose: noop, onShowMessage: noop }))));
    // the Files section is the last one of a long panel: bring it into view for the picture
    window.scene.scrollFiles = async () => {
      await settle();
      const head = [...document.querySelectorAll("h4, .sec-h")].find((e) => e.textContent?.trim() === "Files");
      head?.scrollIntoView({ block: "start" });
      await settle();
    };
  } else if (scene === "transfers" || scene === "transfers-error") {
    const row = (id: string, extra: object = {}) => ({ local_id: id, message_id: "m-" + id, created_at: "2026-10-17T00:00:00Z", peer: maya.address, name: id + ".pdf", upload: false, state: "done", total: 2400000, error: null, available: true, can_redownload: true, ...extra });
    // the first scene is one finished page; the second has an older page whose first read fails (the list loads it by
    // itself as soon as its end shows, so the picture is of the error that leaves behind, then of Retry)
    const withMore = scene === "transfers-error";
    let fail = true;
    await seed({
      transfers: async () => ({ rows: [{ local_id: "act", message_id: "m-act", created_at: "now", peer: pat.address, name: "design.fig", upload: true, state: "running", done: 3400000, total: 9100000, error: null }], next: null }),
      transferHistory: async (after: string | null = null) => {
        if (after && fail) { fail = false; throw "The transfer history could not be read."; }
        return after ? { rows: [row("older-1"), row("older-2", { upload: true, can_redownload: false })], next: null } : { rows: [row("report"), row("photo", { upload: true, can_redownload: false }), row("archive", { peer: kim.address })], next: withMore ? "p2" : null };
      },
    });
    await store.refreshTransfers();
    root.render(wrap(h("div", { id: "stage", style: platform === "desktop" ? { width: 620, padding: 10 } : {} }, h(TransfersPanel, { onOpen: noop }))));
    window.scene.retry = async () => { ([...document.querySelectorAll("button")].find((b) => b.textContent?.trim() === "Retry") as HTMLButtonElement).click(); await settle(); };
  } else if (scene === "settings") {
    await seed();
    const Host = () => { const [tab, setTab] = useState<SetTab>("privacy"); return platform === "desktop" ? h(SettingsModal, { tab, setTab, onClose: noop }) : h("div", { id: "stage" }, h(SettingsSection, { tab: "privacy" })); };
    root.render(wrap(h(Host)));
    window.scene.focusTab = async () => { (document.querySelector('[role="tab"][aria-selected="true"]') as HTMLElement | null)?.focus(); await settle(); };
    window.scene.toggle = async () => { (document.querySelector('[role="switch"]') as HTMLButtonElement).click(); await settle(30); };
  } else if (scene === "composer") {
    localStorage.removeItem("hubchat.enter-key"); // the profile keeps localStorage between scenes
    const peer = pat.address;
    await seed({ chats: async () => [{ peer, last: msg("m9", peer, "Can you send me the report?"), unread: 0 }], chat: async () => [msg("m9", peer, "Can you send me the report?")] });
    root.render(wrap(h("div", { id: "stage", style: { height: "100vh", display: "flex", flexDirection: "column" } }, h(Conversation, { peer, onBack: noop, onInfo: noop, onOpenAddr: noop, onContactInfo: noop, onContact: noop }))));
    window.scene.setNewline = async () => { localStorage.setItem("hubchat.enter-key", "newline"); window.dispatchEvent(new Event("storage")); const { setEnterKey } = await import("../src/lib/enterKey"); setEnterKey("newline"); await settle(); };
    window.scene.paste = async () => {
      const ta = document.querySelector("textarea")!;
      const f = new File([new Uint8Array(8)], "report.pdf", { type: "application/pdf" });
      const e = new Event("paste", { bubbles: true, cancelable: true });
      Object.defineProperty(e, "clipboardData", { value: { types: ["Files"], files: [f], getData: () => "" } });
      ta.dispatchEvent(e); await settle(30);
    };
  } else if (scene === "mention") {
    const peer = "casey.abcdef";
    await seed({ chats: async () => [{ peer, last: null, unread: 0 }], chat: async () => [msg("m1", peer, "Ask @net:" + maya.address + " about the Android build.")] });
    root.render(wrap(h("div", { id: "stage", style: { height: "100vh", display: "flex", flexDirection: "column" } }, h(Conversation, { peer, onBack: noop, onInfo: noop, onOpenAddr: noop, onContactInfo: noop, onContact: noop }))));
    window.scene.menu = async () => { const a = document.querySelector("a.addr")!; a.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, cancelable: true, clientX: 260, clientY: 140 })); await settle(); };
  }
  await settle(30);
  done();
}
main().catch((e) => { window.sceneError = String(e?.stack || e); });
