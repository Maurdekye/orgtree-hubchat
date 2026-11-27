// Mounted App, synthetic API only: LaTeX math in message bubbles (docket hubchat-render-latex-math-in-messages-like-orgtr,
// user 2026-11-27 m13da5b728a34). Inline ($...$, \(...\)) and display ($$...$$, \[...\]) formulas typeset with KaTeX;
// amounts, code and broken LaTeX stay as typed; the chat list and a copied selection keep the TeX source.
// No hub, no network, no real clipboard: the copy check dispatches its own ClipboardEvent with a DataTransfer.
// ?platform=desktop|android&theme=dark|light
import { createRoot } from "react-dom/client";
import App from "../src/App";
import { api, type Contact, type Message } from "../src/api";
import { mockApi } from "../src/lib/mock";

type Check = { name: string; ok: boolean; detail?: unknown };
declare global {
  interface Window {
    mathFixture?: { checks: Check[]; platform: string; step: (name: string) => Promise<unknown> };
    mathFixtureError?: string;
  }
}
const sleep = (ms: number) => new Promise<void>((r) => setTimeout(r, ms));
const until = async (f: () => boolean, ms: number) => { const t = performance.now(); while (performance.now() - t < ms) { if (f()) return true; await sleep(25); } return f(); };
const KIM = "kim.19ac02", ME = "scratch.abcdef";

export const BODIES: Record<string, string> = {
  "m-inline": "Euler: $e^{i\\pi} + 1 = 0$, and Pythagoras \\(a^2 + b^2 = c^2\\).",
  "m-display": "The integral:\n$$\n\\int_0^1 x^2\\,dx = \\frac{1}{3}\n$$\nand the sum\n\\[\\sum_{n=1}^{\\infty} \\frac{1}{n^2} = \\frac{\\pi^2}{6} = 1 + \\frac{1}{4} + \\frac{1}{9} + \\frac{1}{16} + \\frac{1}{25} + \\frac{1}{36} + \\cdots\\]",
  "m-money": "It costs $5 and $10, and `$x$` stays code.",
  "m-fence": "```\necho $HOME $x$\n```",
  "m-bad": "Broken: $\\notacommand{x}$ stays as typed.",
  "m-color": "Red: $\\color{red}{x}$ and a typed <span style=\"color:red\">tag</span>.",
  "m-last": "Last: $x^2$",
};

async function run() {
  const q = new URLSearchParams(location.search);
  const platform = q.get("platform") === "android" ? "android" : "desktop";
  const theme = q.get("theme") === "light" ? "light" : "dark";
  document.documentElement.classList.remove("theme-dark", "theme-light");
  document.documentElement.classList.add("theme-" + theme);
  document.documentElement.dataset.theme = theme;
  const checks: Check[] = [];
  const check = (name: string, ok: unknown, detail?: unknown) => { checks.push({ name, ok: !!ok, ...(ok ? {} : { detail }) }); };
  const directory: Contact[] = [{ address: KIM, kind: "person", org_name: "", username: "Kim", blurb: "", online: true, busy: false, last_seen: null, hubs: ["wss://hub-a.invalid"], picture: null } as unknown as Contact];
  const ids = Object.keys(BODIES);
  const msg = (id: string, i: number): Message => ({ id, peer: KIM, outgoing: false, hub: "wss://hub-a.invalid", body: BODIES[id], kind: null, reply_to: null,
    created_at: new Date(1700000000000 + i * 60000).toISOString(), sent_at: null, received_at: null, state: "received", fetched_at: null, delivered_at: null, read_at: null,
    error: null, seen: true, attachments: [] } as unknown as Message);
  const chat: Message[] = ids.map(msg);
  Object.assign(api, mockApi, {
    state: async () => ({ ...await mockApi.state(), me: { id: "scratch", address: ME, name: "Scratch", about: "" }, hubs: [{ url: "wss://hub-a.invalid", name: "Hub A", state: "connected", error: null, retry_at_ms: null, max_attachment_bytes: 1 << 30, features: ["v2"] }], platform, recovery_saved: true, file_background_hint: null }),
    directory: async () => directory,
    chats: async () => [{ peer: KIM, last: chat.at(-1), unread: 0 }],
    chat: async () => chat,
    message: async (id: string) => chat.find((m) => m.id === id) ?? null,
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
  const row = () => [...document.querySelectorAll<HTMLElement>(".crow")].find((e) => e.querySelector(".crow-name")?.textContent === "Kim");
  await until(() => !!row(), 3000);
  const preview = row()?.querySelector(".crow-prev")?.textContent ?? "";
  row()?.click();
  const bubble = (id: string) => document.querySelector<HTMLElement>(`.msg[data-id="${id}"] .bubble`);
  await until(() => ids.every((id) => !!bubble(id)), 4000);
  await document.fonts.ready; await sleep(250);
  // the chat scrolls to its newest message: start at the top so the pictures show the formulas
  const scroller = document.querySelector<HTMLElement>(".timeline");
  if (scroller) scroller.scrollTop = 0;
  await sleep(200);

  const steps: Record<string, () => Promise<unknown>> = {
    "render": async () => {
      const b = (id: string) => bubble(id);
      const inl = b("m-inline"), disp = b("m-display"), money = b("m-money"), fence = b("m-fence"), bad = b("m-bad"), color = b("m-color");
      const visible = (el: HTMLElement | null) => [...(el?.querySelectorAll<HTMLElement>(".katex-html") ?? [])].map((e) => e.textContent).join(" ");
      const shown = (el: HTMLElement | null) => { const c = el?.cloneNode(true) as HTMLElement | undefined; c?.querySelectorAll(".katex-mathml").forEach((e) => e.remove()); return c?.textContent ?? ""; };
      check("inline math typesets: $...$ and \\(...\\)", inl?.querySelectorAll(".katex").length === 2 && !inl.querySelector(".katex-display")
        && !/[$\\]/.test(shown(inl)) && !visible(inl).includes("pi"), inl?.innerHTML.slice(0, 400));
      const ds = [...(disp?.querySelectorAll<HTMLElement>(".katex-display") ?? [])];
      check("display math typesets: $$...$$ and \\[...\\] on their own lines", ds.length === 2, disp?.innerHTML.slice(0, 400));
      const box = disp?.getBoundingClientRect(), wide = ds[1];
      check("a long display formula scrolls inside its bubble, never past it", !!wide && !!box && wide.getBoundingClientRect().right <= box.right + 0.5
        && getComputedStyle(wide).overflowX === "auto", { box: box?.right, wide: wide?.getBoundingClientRect().right, overflow: wide && getComputedStyle(wide).overflowX });
      check("dollar amounts and code spans stay as typed", !!money && !money.querySelector(".katex") && (money.textContent ?? "").includes("It costs $5 and $10")
        && money.querySelector("code")?.textContent === "$x$", money?.innerHTML);
      check("fenced code keeps its dollars", !!fence && !fence.querySelector(".katex") && (fence.querySelector("pre code")?.textContent ?? "").includes("echo $HOME $x$"), fence?.innerHTML);
      check("broken LaTeX shows as typed, with no error box", !!bad && !bad.querySelector(".katex, .katex-error") && (bad.textContent ?? "").includes("Broken: $\\notacommand{x}$ stays as typed."), bad?.innerHTML);
      const red = [...(color?.querySelectorAll<HTMLElement>(".katex [style]") ?? [])].some((e) => /^rgb\(255, 0, 0\)$/.test(getComputedStyle(e).color));
      const strayStyle = [...(document.querySelectorAll<HTMLElement>(".bubble [style]"))].filter((e) => !e.closest(".katex")).length;
      check("a formula keeps its colour; a typed tag stays text", red && strayStyle === 0 && (color?.textContent ?? "").includes('<span style="color:red">tag</span>'), { red, strayStyle, html: color?.innerHTML.slice(0, 300) });
      const faces = [...document.fonts].filter((f) => f.family.replace(/["']/g, "") === "KaTeX_Main");
      check("KaTeX's fonts load from the app itself", faces.some((f) => f.status === "loaded"), faces.map((f) => [f.weight, f.style, f.status]));
      check("the chat list shows the formula's source", preview.includes("Last: $x^2$"), preview);
      return { faces: faces.length };
    },
    "copy": async () => {
      const el = bubble("m-inline")?.querySelector<HTMLElement>(".mtext, .mbody") ?? bubble("m-inline");
      const sel = getSelection()!; sel.removeAllRanges(); if (el) sel.selectAllChildren(el);
      const dt = new DataTransfer();
      document.dispatchEvent(new ClipboardEvent("copy", { bubbles: true, cancelable: true, clipboardData: dt }));
      const text = dt.getData("text/plain");
      sel.removeAllRanges();
      check("copying a selected formula copies its TeX source", text.includes("$e^{i\\pi} + 1 = 0$") && text.includes("$a^2 + b^2 = c^2$") && text.startsWith("Euler:"), text);
      return text;
    },
  };
  window.mathFixture = { checks, platform, step: async (name) => { const f = steps[name]; if (!f) throw Error("no step " + name); return f(); } };
}
run().catch((e) => { window.mathFixtureError = String(e?.stack || e); });
