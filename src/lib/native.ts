// Plugin calls (file picker, opener, clipboard) with browser fallbacks so the
// mock build works in a normal browser.
import { isTauri } from "../api";
import { toast } from "./toast";

/** Pick files to attach: paths (desktop) or content:// URIs (Android). */
export async function pickFiles(): Promise<string[]> {
  if (!isTauri) {
    const all = ["C:\\Users\\alex\\Documents\\nightly-logs.zip", "C:\\Users\\alex\\Pictures\\screenshot-rc3.png", "C:\\Users\\alex\\Documents\\release-notes.md"];
    return [all[Math.floor(Math.random() * all.length)]];
  }
  const { open } = await import("@tauri-apps/plugin-dialog");
  const r = await open({ multiple: true });
  if (!r) return [];
  return Array.isArray(r) ? r : [r];
}

export async function openFile(path: string): Promise<void> {
  if (!isTauri) { toast("Opening " + path.split(/[\\/]/).pop()); return; }
  const { openPath } = await import("@tauri-apps/plugin-opener");
  await openPath(path);
}

export async function revealFile(path: string): Promise<void> {
  if (!isTauri) { toast("Showing " + path.split(/[\\/]/).pop() + " in its folder"); return; }
  const { revealItemInDir } = await import("@tauri-apps/plugin-opener");
  await revealItemInDir(path);
}

/** Only http(s) links leave the app. */
export async function openLink(url: string): Promise<void> {
  if (!/^https?:\/\//i.test(url)) return;
  if (!isTauri) { window.open(url, "_blank", "noopener"); return; }
  const { openUrl } = await import("@tauri-apps/plugin-opener");
  await openUrl(url);
}

export async function copyText(text: string, confirm?: string): Promise<void> {
  try {
    await navigator.clipboard.writeText(text);
  } catch {
    const ta = document.createElement("textarea");
    ta.value = text; ta.style.position = "fixed"; ta.style.opacity = "0";
    document.body.appendChild(ta); ta.select();
    try { document.execCommand("copy"); } catch { /* nothing else to try */ }
    ta.remove();
  }
  if (confirm) toast(confirm);
}

/** An error from invoke() is a string; anything else gets stringified. */
export const errText = (e: unknown) => (typeof e === "string" ? e : e instanceof Error ? e.message : String(e));
