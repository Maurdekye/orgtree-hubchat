// Settings › Changelog, mounted in the real App on synthetic state only (no hub, no network, no native writes).
// Red at the base: there is no Changelog tab, so every changelog check fails and the start check passes.
import { createRoot } from "react-dom/client";
import App from "../src/App";
import { api } from "../src/api";
import { mockApi } from "../src/lib/mock";
import { setThemePref } from "../src/lib/theme";

declare global { interface Window { changelogResult?: { platform: string; checks: { name: string; ok: boolean }[] }; changelogError?: string; captureLight?: () => Promise<void>; } }
const pause = () => new Promise<void>((r) => setTimeout(r, 80));
// the versions the repo ships notes for (the proof host passes them: the base has no releaseNotes.ts to import)
const want: string[] = JSON.parse(decodeURIComponent(new URLSearchParams(location.search).get("versions") || "[]"));
async function run() {
  localStorage.clear(); sessionStorage.clear();
  setThemePref("dark");
  const platform = new URLSearchParams(location.search).get("platform") === "android" ? "android" : "desktop";
  const checks: { name: string; ok: boolean }[] = [];
  const check = (name: string, ok: unknown) => checks.push({ name, ok: !!ok });
  let fetched = 0;
  const realFetch = window.fetch;
  window.fetch = (...a: Parameters<typeof fetch>) => { fetched++; return realFetch(...a); };
  Object.assign(api, mockApi, {
    state: async () => ({ ...await mockApi.state(), me: { id: "scratch", address: "scratch.abcdef", name: "Scratch", about: "" }, hubs: [], platform, recovery_saved: true, file_background_hint: null }),
    directory: async () => [], chats: async () => [], chat: async () => [],
    onEvent: async () => () => {}, setupStatus: async () => null,
  });
  createRoot(document.getElementById("root")!).render(<App />);
  await pause(); await pause(); await pause();
  const click = async (e: Element | null | undefined) => { (e as HTMLElement | null)?.click(); await pause(); await pause(); };
  check("not shown at start", !document.querySelector(".changelog") && !document.body.textContent?.includes("This version"));

  await click(document.querySelector('[aria-label="Settings"]'));
  const labels = platform === "desktop"
    ? [...document.querySelectorAll(".set-nav [role=tab]")].map((e) => e.textContent?.trim() ?? "")
    : [...document.querySelectorAll(".scr-body .li .t1")].map((e) => e.textContent?.trim() ?? "");
  const at = labels.indexOf("Changelog");
  check("a Changelog tab between Help and About", at > 0 && labels[at - 1] === "Help" && labels[at + 1] === "About");
  check("not opened by opening Settings", !document.querySelector(".changelog"));
  // 13 sections: in a short window the desktop list scrolls, so About and Quit Hubchat stay reachable
  const nav = document.querySelector<HTMLElement>(".set-nav");
  check("every section reachable", platform === "android" || (!!nav && (nav.scrollHeight <= nav.clientHeight + 1 || /auto|scroll/.test(getComputedStyle(nav).overflowY))));
  const entry = platform === "desktop"
    ? document.querySelector("#settab-changelog")
    : [...document.querySelectorAll(".scr-body .li")].find((e) => e.querySelector(".t1")?.textContent === "Changelog");
  await click(entry);
  const view = document.querySelector(".changelog");
  const title = platform === "desktop" ? document.querySelector(".set-main .modal-h h3")?.textContent : document.querySelector(".appbar .title")?.textContent;
  check("the tab opens under its own title", !!view && title === "Changelog");
  const versions = [...(view?.querySelectorAll(".changelog-v") ?? [])].map((e) => e.firstChild?.textContent ?? "");
  check("every version, newest first", want.length >= 30 && JSON.stringify(versions) === JSON.stringify(want));
  check("the test build says its notes come with its release",
    view?.querySelector(".changelog-pending")?.textContent === "You're running 1.4.0-alpha.1, a test build. Its notes come with the 1.4.0 release."
    && !view?.querySelector(".changelog-v .chip"));
  check("no download sections", !!view && ![...view.querySelectorAll("h1,h2,h3,h4,h5,h6")].some((h) => /^(Downloads|First run on macOS and Linux)$/.test(h.textContent ?? ""))
    && ![...view.querySelectorAll("th")].some((th) => th.textContent === "File"));
  check("sanitized: no script, frame or live link", !!view && !view.querySelector("script,iframe,object,embed,a[href]") && view.querySelectorAll("[data-href]").length > 0);
  check("bundled: no network", fetched === 0);
  window.captureLight = async () => { setThemePref("light"); await pause(); await pause(); };
  window.changelogResult = { platform, checks };
}
run().catch((e) => { window.changelogError = String(e?.stack || e); });
