// Mounted App, synthetic API only: Copy image in a chat picture's menu and a menu on the zoomed picture (Hubchat 1.4.0;
// docket hubchat-copy-image-in-image-context-menus-and-co, user 2026-11-25 mee317e7dbd9a). The clipboard is a stub set
// before the app loads: nothing here reaches the real one. Right-click (desktop) and the long-press's contextmenu
// (Android) are DOM events dispatched in this page. ?platform=desktop|android
import { createRoot } from "react-dom/client";
import App from "../src/App";
import { api, type Contact, type Message } from "../src/api";
import { mockApi } from "../src/lib/mock";

type Check = { name: string; ok: boolean; detail?: unknown };
declare global {
  interface Window {
    copyFixture?: { checks: Check[]; platform: string; step: (name: string) => Promise<unknown> };
    copyFixtureError?: string;
  }
}
const sleep = (ms: number) => new Promise<void>((r) => setTimeout(r, ms));
const until = async (f: () => boolean, ms: number) => { const t = performance.now(); while (performance.now() - t < ms) { if (f()) return true; await sleep(25); } return f(); };
const KIM = "kim.19ac02", ME = "scratch.abcdef", W = 64, H = 48;

// the clipboard stub: what was written, decoded
type Wrote = { types: string[]; type: string; size: number; w: number; h: number; png: boolean };
const writes: Wrote[] = [], texts: string[] = [];
Object.defineProperty(navigator, "clipboard", { configurable: true, value: {
  write: async (items: ClipboardItem[]) => {
    for (const it of items) {
      const b = await it.getType(it.types[0]!);
      const head = new Uint8Array(await b.slice(0, 8).arrayBuffer());
      const bmp = await createImageBitmap(b);
      writes.push({ types: [...it.types], type: b.type, size: b.size, w: bmp.width, h: bmp.height, png: head[1] === 0x50 && head[2] === 0x4e && head[3] === 0x47 });
      bmp.close();
    }
  },
  writeText: async (t: string) => { texts.push(t); },
  read: async () => { throw new Error("stub: no reading"); },
  readText: async () => { throw new Error("stub: no reading"); },
} });

async function jpeg(): Promise<ArrayBuffer> {
  const c = document.createElement("canvas"); c.width = W; c.height = H;
  const g = c.getContext("2d")!; g.fillStyle = "#2a6"; g.fillRect(0, 0, W, H); g.fillStyle = "#fd3"; g.fillRect(8, 8, 24, 16);
  return await new Promise<ArrayBuffer>((ok) => c.toBlob((b) => void b!.arrayBuffer().then(ok), "image/jpeg", 0.9));
}

async function run() {
  const platform = new URLSearchParams(location.search).get("platform") === "android" ? "android" : "desktop";
  const checks: Check[] = [];
  const check = (name: string, ok: unknown, detail?: unknown) => { checks.push({ name, ok: !!ok, ...(ok ? {} : { detail }) }); };
  const picture = await jpeg();
  const directory: Contact[] = [{ address: KIM, kind: "person", org_name: "", username: "Kim", blurb: "", online: true, busy: false, last_seen: null, hubs: ["wss://hub-a.invalid"], picture: null } as unknown as Contact];
  const msg = (id: string, body: string, attachments: Message["attachments"]): Message => ({ id, peer: KIM, outgoing: false, hub: "wss://hub-a.invalid", body, kind: null, reply_to: null,
    created_at: new Date(1700000000000 + attachments.length * 60000).toISOString(), sent_at: null, received_at: null, state: "received", fetched_at: null, delivered_at: null, read_at: null,
    error: null, seen: true, attachments } as Message);
  const chat: Message[] = [
    msg("m-text", "Morning! The layout from yesterday:", []),
    msg("m-pic", "", [{ local_id: "att-1", hub_id: "h-1", name: "photo.jpg", bytes: picture.byteLength, source: null, local_path: null, state: "remote", error: null }]),
  ];
  Object.assign(api, mockApi, {
    state: async () => ({ ...await mockApi.state(), me: { id: "scratch", address: ME, name: "Scratch", about: "" }, hubs: [{ url: "wss://hub-a.invalid", name: "Hub A", state: "connected", error: null, retry_at_ms: null, max_attachment_bytes: 1 << 30, features: ["v2"] }], platform, recovery_saved: true, file_background_hint: null }),
    directory: async () => directory,
    chats: async () => [{ peer: KIM, last: chat.at(-1), unread: 0 }],
    chat: async () => chat,
    message: async (id: string) => chat.find((m) => m.id === id) ?? null,
    attachmentPreview: async () => picture.slice(0),
    picture: async () => null,
    transfers: async () => ({ rows: [], next: null }), onEvent: async () => () => {},
    openDm: async () => {}, markRead: async () => {}, draft: async () => "", setDraft: async () => {}, draftAnswers: async () => ({ replies: [], picks: {} }),
    setupStatus: async () => null,
    sendRoute: async () => ({ pinned: null, automatic: "wss://hub-a.invalid", next: "wss://hub-a.invalid", hubs: [] }),
    loadOlder: async () => ({ added: 0, more: [], unreachable: [] }),
  });
  localStorage.clear(); sessionStorage.clear();
  createRoot(document.getElementById("root")!).render(<App />);
  await sleep(300);
  const settle = () => sleep(250);
  const row = () => [...document.querySelectorAll<HTMLElement>(".crow")].find((e) => e.querySelector(".crow-name")?.textContent === "Kim");
  await until(() => !!row(), 3000); row()?.click();
  const chatPic = () => document.querySelector<HTMLImageElement>('.msg[data-id="m-pic"] .att-img img');
  await until(() => !!chatPic()?.complete && (chatPic()?.naturalWidth ?? 0) > 0, 4000); await settle();

  const menuEl = () => document.querySelector<HTMLElement>(platform === "desktop" ? ".pop.ctx" : ".sheet.contact-file-menu");
  const labels = () => [...(menuEl()?.querySelectorAll(".mi") ?? [])].map((b) => b.textContent?.trim() ?? "");
  const item = (label: string) => [...(menuEl()?.querySelectorAll<HTMLElement>(".mi") ?? [])].find((b) => b.textContent?.trim() === label) ?? null;
  const ctx = async (el: Element | null) => {
    const r = el?.getBoundingClientRect();
    el?.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, cancelable: true, clientX: (r?.left ?? 0) + (r?.width ?? 0) / 2, clientY: (r?.top ?? 0) + (r?.height ?? 0) / 2, button: 2 }));
    await settle();
  };
  // the menu is on top: the point at its middle belongs to it
  const onTop = () => { const m = menuEl(); if (!m) return false; const r = m.getBoundingClientRect(); return m.contains(document.elementFromPoint(r.left + r.width / 2, r.top + Math.min(r.height / 2, 30))); };
  const viewer = () => document.querySelector<HTMLElement>(".imgview");
  const viewerPic = () => viewer()?.querySelector<HTMLImageElement>("img") ?? null;
  const copied = (n: number) => until(() => writes.length >= n, 3000);
  const pressEvents = (el: Element, type: string, x: number, y: number) =>
    el.dispatchEvent(new PointerEvent(type, { bubbles: true, cancelable: true, pointerId: 7, pointerType: "touch", isPrimary: true, clientX: x, clientY: y }));
  const longPress = async (el: Element | null) => {
    if (!el) return;
    const r = el.getBoundingClientRect(), x = r.left + r.width / 2, y = r.top + r.height / 2;
    pressEvents(el, "pointerdown", x, y); await sleep(600); pressEvents(el, "pointerup", x, y); await settle();
  };
  let chatLabels: string[] = [];

  const steps: Record<string, () => Promise<unknown>> = {
    "chat-menu": async () => {
      if (platform === "android") {
        await longPress(chatPic());
        check("Android: a long-press on a picture doesn't select its message", !document.querySelector(".appbar.select"), document.querySelector(".appbar.select .title")?.textContent);
        (document.querySelector('.appbar.select [aria-label="Cancel"]') as HTMLElement | null)?.click(); await settle();
        await longPress(document.querySelector('.msg[data-id="m-text"] .bubble'));
        check("Android: a long-press on a message's text still selects it", document.querySelector(".appbar.select .title")?.textContent === "1 selected");
        (document.querySelector('.appbar.select [aria-label="Cancel"]') as HTMLElement | null)?.click(); await settle();
      }
      await ctx(chatPic());
      chatLabels = labels();
      const at = chatLabels.indexOf("Copy image");
      check("the chat picture's menu offers Copy image, after View", at > 0 && chatLabels[at - 1] === "View", chatLabels);
      check("the chat picture's menu is on top", onTop());
      return chatLabels;
    },
    "chat-copy": async () => {
      item("Copy image")?.click();
      await copied(1);
      const w = writes[0];
      check("Copy image puts the picture on the clipboard as PNG", !!w && w.png && w.type === "image/png" && JSON.stringify(w.types) === '["image/png"]' && w.w === W && w.h === H && !texts.length, { writes, texts });
      await until(() => !!document.body.textContent?.includes("Image copied"), 2000);
      check("it says Image copied", document.body.textContent?.includes("Image copied"));
      return w;
    },
    "viewer-menu": async () => {
      chatPic()?.click();
      await until(() => !!viewerPic()?.complete, 3000); await settle();
      await ctx(viewerPic());
      const got = labels();
      check("the zoomed picture has a menu: the chat's picture actions without View", got.length > 0 && JSON.stringify(got) === JSON.stringify(chatLabels.filter((l) => l !== "View").slice(0, got.length))
        && got.includes("Copy image") && !got.includes("View") && !got.includes("Reply"), { got, chatLabels });
      check("the zoomed picture's menu is on top of the viewer", onTop());
      return got;
    },
    "viewer-copy": async () => {
      item("Copy image")?.click();
      await copied(2);
      const w = writes[1];
      check("the viewer's Copy image copies the picture too", !!w && w.png && w.w === W && w.h === H, writes);
      return w;
    },
    "dismiss": async () => {
      await ctx(viewerPic());
      const open = !!menuEl();
      if (platform === "desktop") {
        const v = viewer()!; const r = v.getBoundingClientRect(), x = r.left + 30, y = r.top + 30;
        v.dispatchEvent(new PointerEvent("pointerdown", { bubbles: true, clientX: x, clientY: y, pointerId: 3, pointerType: "mouse" }));
        v.dispatchEvent(new MouseEvent("mousedown", { bubbles: true, clientX: x, clientY: y })); // what a real click sends, in order
        v.dispatchEvent(new PointerEvent("pointerup", { bubbles: true, clientX: x, clientY: y, pointerId: 3, pointerType: "mouse" }));
        v.click();
      } else (document.querySelector(".sheet-scrim") as HTMLElement | null)?.click();
      await settle();
      check("dismissing the picture's menu leaves the viewer open", open && !menuEl() && !!viewer(), { open, menu: !!menuEl(), viewer: !!viewer() });
      // and a plain click on the backdrop still closes it, as before
      viewer()?.click(); await settle();
      check("a click on the backdrop still closes the viewer", !viewer());
      return null;
    },
    // Escape (desktop) or the system Back (Android) closes the menu first and the viewer only the next time
    // (@build-sol, round 1 bounce: both closed at once on desktop; on Android Back closed the viewer under the sheet)
    "escape-back": async () => {
      chatPic()?.click();
      await until(() => !!viewerPic()?.complete, 3000); await settle();
      await ctx(viewerPic());
      const open = !!menuEl() && !!viewer();
      const away = async () => {
        if (platform === "desktop") {
          (document.activeElement ?? document.body).dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true, cancelable: true }));
          await settle();
        } else {
          const popped = new Promise<void>((ok) => window.addEventListener("popstate", () => ok(), { once: true }));
          history.back();
          await Promise.race([popped, sleep(2000)]); await settle();
        }
      };
      await away();
      const first = { menu: !!menuEl(), viewer: !!viewer() };
      check("the first Escape or Back closes the picture's menu and keeps the viewer", open && !first.menu && first.viewer, { open, first });
      await away();
      check("the next Escape or Back closes the viewer", !viewer() && !menuEl(), { menu: !!menuEl(), viewer: !!viewer() });
      return first;
    },
  };
  window.copyFixture = { checks, platform, step: async (name) => { const f = steps[name]; if (!f) throw Error("no step " + name); return f(); } };
}
run().catch((e) => { window.copyFixtureError = String(e?.stack || e); });
