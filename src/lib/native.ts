// Plugin calls (file pickers, opener, clipboard, QR scanner, autostart, app
// version) with browser fallbacks so the mock build works in a normal browser.
import { api, isTauri } from "../api";
import { toast } from "./toast";

const sleep = (n: number) => new Promise((r) => setTimeout(r, n));

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

/** A downloaded attachment, through the core (on Android local_path may be a
 *  content:// URI, which only the core knows how to hand to another app). */
export async function openAttachment(messageId: string, localId: string, reveal: boolean): Promise<void> {
  await api.openAttachment(messageId, localId, reveal);
}

/** Pick a key file to import: a path, or on Android a content:// URI. */
export async function pickKeyFile(): Promise<string | null> {
  if (!isTauri) { await sleep(200); return MOCK_KEY_FILE; }
  const { open }= await import("@tauri-apps/plugin-dialog");
  const r = await open({ multiple: false, directory: false });
  return typeof r === "string" ? r : null;
}

/** Where to save the key file (a path, or on Android a content:// URI). */
export async function saveKeyFileTo(): Promise<string | null> {
  if (!isTauri) { await sleep(200); return MOCK_KEY_FILE; }
  const { save }= await import("@tauri-apps/plugin-dialog");
  return await save({ defaultPath: "hubchat-key.hubchat-key" });
}

/** The file name at the end of a path or content URI, for display. */
export function baseName(source: string): string {
  let s = source;
  try { s = decodeURIComponent(source); } catch { /* keep it raw */ }
  return s.split(/[\\/:]/).filter(Boolean).pop() || source;
}
const MOCK_KEY_FILE = "C:\\Users\\alex\\Documents\\hubchat-key.hubchat-key";

/** Scan one QR code with the camera (Android). Null when the user backs out. */
export async function scanQr(mockText: string): Promise<string | null> {
  if (!isTauri) {
    await sleep(900);
    return new URLSearchParams(location.search).get("scan") ?? mockText;
  }
  const bs = await import("@tauri-apps/plugin-barcode-scanner");
  let perm = await bs.checkPermissions();
  if (perm !== "granted") perm = await bs.requestPermissions();
  if (perm !== "granted") throw "Hubchat needs the camera to scan a code. Allow it in Android's settings for Hubchat.";
  try {
    const r = await bs.scan({ windowed: false, formats: [bs.Format.QRCode] });
    return r.content || null;
  } catch (e) {
    const t = errText(e);
    if (/cancel/i.test(t)) return null;
    throw t;
  }
}

/** The app's version (tauri.conf.json). */
export async function appVersion(): Promise<string> {
  if (!isTauri) return "1.0.0";
  const { getVersion } = await import("@tauri-apps/api/app");
  return await getVersion();
}

/** Start with Windows (desktop): the autostart plugin, or localStorage in the mock. */
export const autostart = {
  async get(): Promise<boolean> {
    if (!isTauri) return localStorage.getItem("hubchat.mock.autostart") === "1";
    const { isEnabled } = await import("@tauri-apps/plugin-autostart");
    return await isEnabled();
  },
  async set(on: boolean): Promise<void> {
    if (!isTauri) { localStorage.setItem("hubchat.mock.autostart", on ? "1" : "0"); return; }
    const a = await import("@tauri-apps/plugin-autostart");
    await (on ? a.enable() : a.disable());
  },
};

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
