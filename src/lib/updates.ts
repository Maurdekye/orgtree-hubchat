// Updates: check at startup and every 6 hours, unless turned off in Settings ›
// About. A failed check is silent (there may be no release yet).
// - Desktop, with the updater plugin: a calm banner, and "Restart to update"
//   installs and relaunches.
// - Android (user 2026-10-09 08:29Z): the shell reads the same release feed,
//   downloads the APK, checks the updater key's signature and hands it to
//   Android's installer; the banner shows each step (the first time, Android's
//   permission to install apps).
// In the browser mock, `?update=1` pretends version 0.2.0 is out.
import { useSyncExternalStore } from "react";
import { api, isTauri, type AppUpdate } from "../api";
import { errText } from "./native";

export interface Available { version: string; install: () => Promise<void> }

/** Android's install, step by step. */
export type InstallStep =
  | { k: "idle" }
  | { k: "permission" }
  | { k: "downloading"; pct: number }
  | { k: "installing" }
  | { k: "confirm" }
  | { k: "failed"; msg: string };

let avail: Available | null = null;
let step: InstallStep = { k: "idle" };
let platform: "desktop" | "android" = "desktop";
const subs = new Set<() => void>();
const emit = () => subs.forEach((f) => f());
const subscribe = (f: () => void) => { subs.add(f); return () => { subs.delete(f); }; };
const setStep = (s: InstallStep) => { step = s; emit(); };

export function useUpdate(): Available | null {
  return useSyncExternalStore(subscribe, () => avail);
}

export function useInstallStep(): InstallStep {
  return useSyncExternalStore(subscribe, () => step);
}

const AUTO = "hubchat.updates.auto";
const autoOn = () => localStorage.getItem(AUTO) !== "0";
/** Settings › About: automatic checks, on unless turned off. */
export function useAutoChecks(): boolean {
  return useSyncExternalStore(subscribe, autoOn);
}
export function setAutoChecks(on: boolean): void {
  localStorage.setItem(AUTO, on ? "1" : "0");
  emit();
}

async function androidInstall(u: AppUpdate): Promise<void> {
  setStep({ k: "downloading", pct: 0 });
  const off = await api.onAppUpdateProgress((done, total) => setStep({ k: "downloading", pct: total ? Math.min(100, Math.round((done * 100) / total)) : 0 }));
  let r: string;
  try { r = await api.appUpdateInstall(u.url, u.signature); }
  catch (e) { setStep({ k: "failed", msg: errText(e) }); return; }
  finally { off(); }
  if (r === "permission") { setStep({ k: "permission" }); return; }
  setStep({ k: "installing" });
  // Android takes over: it may show its own window, then replaces the app
  // (which ends this page); only a refusal comes back here
  for (let i = 0; i < 600; i++) {
    await new Promise((res) => setTimeout(res, 1000));
    const s = await api.appUpdateState().catch(() => "");
    if (s === "confirm" && step.k !== "confirm") setStep({ k: "confirm" });
    else if (s.startsWith("failed")) { setStep({ k: "failed", msg: s.replace(/^failed:\s*/, "") }); return; }
  }
}

async function checkRaw(): Promise<Available | null> {
  if (platform === "android") {
    // a test build may read a local feed instead (the update test; the shell
    // ignores this in the real app)
    const u = await api.appUpdateCheck(localStorage.getItem("hubchat.updates.feed"));
    return u ? { version: u.version, install: () => androidInstall(u) } : null;
  }
  if (!isTauri) {
    await new Promise((r) => setTimeout(r, 700));
    if (new URLSearchParams(location.search).get("update") !== "1") return null;
    return { version: "0.2.0", install: async () => { await new Promise((r) => setTimeout(r, 1500)); location.reload(); } };
  }
  const { check } = await import("@tauri-apps/plugin-updater");
  const u = await check();
  if (!u) return null;
  return {
    version: u.version,
    install: async () => {
      await u.downloadAndInstall();
      const { relaunch } = await import("@tauri-apps/plugin-process");
      await relaunch();
    },
  };
}

/** Check now. Returns the update, or null when up to date; throws the error text. */
export async function checkForUpdate(): Promise<Available | null> {
  try {
    const a = await checkRaw();
    avail = a; emit();
    return a;
  } catch (e) { throw errText(e); }
}

let started = false;
/** Background checks: now and every 6 hours, while automatic checks are on;
 *  failures show nothing. */
export function startUpdateChecks(p: "desktop" | "android" = "desktop"): void {
  if (started) return;
  started = true;
  platform = p;
  const run = () => { if (autoOn()) checkForUpdate().catch(() => {}); };
  run();
  setInterval(run, 6 * 3600e3);
}
