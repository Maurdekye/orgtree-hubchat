// Desktop only: a stalled page cannot send beats; Rust watches off the UI thread.
import { invoke } from "@tauri-apps/api/core";
import { isTauri, isWeb } from "../api";

let started = false;
export function startBeats(platform: "desktop" | "android"): void {
  if (started || !isTauri || isWeb || platform !== "desktop") return;
  started = true;
  const beat = (visible: boolean) => { void invoke("ui_beat", { visible }).catch(() => {}); };
  document.addEventListener("visibilitychange", () => beat(document.visibilityState === "visible"));
  setInterval(() => { if (document.visibilityState === "visible") beat(true); }, 5000);
}
