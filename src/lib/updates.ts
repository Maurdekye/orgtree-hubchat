// Updates (desktop only): check at startup and every 6 hours with the updater
// plugin. A failed check is silent (there may be no release yet); an update
// shows a calm banner, and "Restart to update" installs it and relaunches.
// In the browser mock, `?update=1` pretends version 0.2.0 is out.
import { useSyncExternalStore } from "react";
import { isTauri } from "../api";
import { errText } from "./native";

export interface Available { version: string; install: () => Promise<void> }

let avail: Available | null = null;
const subs = new Set<() => void>();
const emit = () => subs.forEach((f) => f());

export function useUpdate(): Available | null {
  return useSyncExternalStore((f) => { subs.add(f); return () => { subs.delete(f); }; }, () => avail);
}

async function checkRaw(): Promise<Available | null> {
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
/** Background checks: now and every 6 hours; failures show nothing. */
export function startUpdateChecks(): void {
  if (started) return;
  started = true;
  const run = () => { checkForUpdate().catch(() => {}); };
  run();
  setInterval(run, 6 * 3600e3);
}
