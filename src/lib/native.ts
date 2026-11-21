// Plugin calls (file pickers, opener, clipboard, QR scanner, autostart, app
// version) with browser fallbacks so the mock build works in a normal browser.
// Hubchat Web (`isWeb`) is neither: what a page cannot do yet says so instead
// of pretending, as the mock may.
import { api, isTauri, isWeb } from "../api";
import { toast } from "./toast";

const sleep = (n: number) => new Promise((r) => setTimeout(r, n));

/** What Hubchat Web answers for something it does not do yet (the core says the same). */
const NOT_ON_WEB = "This isn't available in Hubchat Web yet.";

/** What only a page can do for Hubchat Web: the camera, the browser's file chooser and its downloads.
 *  src/web fills it in at start (webApi.ts); nothing here imports from src/web. */
export interface SuggestedHubs { listed: boolean; addresses: string[] }
export interface WebNative {
  /** Scan one QR code with the camera. Null: the user closed it. */
  scanQr(): Promise<string | null>;
  /** Choose files to send: what stands for each as an attachment's source (the core keeps its copy). */
  pickFiles(): Promise<string[]>;
  /** Choose a key file: what stands for it in `api.keyFileImport`. */
  pickKeyFile(): Promise<string | null>;
  /** What stands for "where to save the key file" in `api.keyFileExport`: the browser decides where. */
  saveKeyFileTo(): Promise<string | null>;
  /** Hub addresses to offer for the first hub (spec R6.2a). `listed`: the server named them, so each
   *  is shown whatever its check finds; else they are guesses, shown only when a hub answers. */
  suggestedHubs(): Promise<SuggestedHubs>;
  /** The label the server's operator gave this Hubchat Web, for the welcome screen. Empty: none. */
  serverName(): Promise<string>;
  /** A newer build of the web app waiting to load (`install` reloads into it). Null: none. */
  newerBuild(): Promise<{ version: string; install: () => Promise<void> } | null>;
}
let webNative: WebNative | null = null;
export function installWebNative(w: WebNative): void { webNative = w; }
/** Hub addresses to offer someone adding their first hub. The apps have none: only Hubchat Web is
 *  served from somewhere a hub may run too. */
export async function suggestedHubs(): Promise<SuggestedHubs> {
  const none: SuggestedHubs = { listed: false, addresses: [] };
  return isWeb && webNative ? webNative.suggestedHubs().catch(() => none) : none;
}
/** The label of the Hubchat Web server this page came from. The apps come from no server: "". */
export async function serverName(): Promise<string> {
  return isWeb && webNative ? webNative.serverName().catch(() => "") : "";
}
/** Hubchat Web's update: a newer build the server has, fetched and waiting. */
export async function webUpdate(): Promise<{ version: string; install: () => Promise<void> } | null> {
  return webNative ? webNative.newerBuild() : null;
}
function onWeb(): WebNative {
  if (!webNative) throw NOT_ON_WEB;
  return webNative;
}

/** Pick files to attach: paths (desktop) or content:// URIs (Android). */
export async function pickFiles(): Promise<string[]> {
  if (isWeb) return onWeb().pickFiles();
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
  if (isWeb) throw NOT_ON_WEB;
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
  if (isWeb) return onWeb().pickKeyFile();
  if (!isTauri) { await sleep(200); return MOCK_KEY_FILE; }
  const { open }= await import("@tauri-apps/plugin-dialog");
  const r = await open({ multiple: false, directory: false });
  return typeof r === "string" ? r : null;
}

/** Where to save the key file (a path, or on Android a content:// URI). */
export async function saveKeyFileTo(): Promise<string | null> {
  if (isWeb) return onWeb().saveKeyFileTo();
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
  if (isWeb) return onWeb().scanQr();
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
  if (isWeb) return (await import("../../package.json")).version;
  if (!isTauri) return "1.3.11";
  const { getVersion } = await import("@tauri-apps/api/app");
  return await getVersion();
}

/** Start with Windows (desktop): the autostart plugin, or localStorage in the mock. */
export const autostart = {
  async get(): Promise<boolean> {
    if (isWeb) return false;
    if (!isTauri) return localStorage.getItem("hubchat.mock.autostart") === "1";
    const { isEnabled } = await import("@tauri-apps/plugin-autostart");
    return await isEnabled();
  },
  /** Through the shell, so the tray's tick follows; the value now in effect. */
  async set(on: boolean): Promise<boolean> {
    if (isWeb) return false;
    if (!isTauri) { localStorage.setItem("hubchat.mock.autostart", on ? "1" : "0"); return on; }
    return await api.setAutostart(on);
  },
};

/** Exit on close (desktop): closing the window quits Hubchat instead of
 *  keeping it in the tray. The tray has the same switch. */
export const exitOnClose = {
  get: (): Promise<boolean> => api.exitOnClose(),
  /** The value now in effect. */
  set: (on: boolean): Promise<boolean> => api.setExitOnClose(on),
};

/** Only http(s) links leave the app. */
export async function openLink(url: string): Promise<void> {
  if (!/^https?:\/\//i.test(url)) return;
  if (!isTauri) { window.open(url, "_blank", "noopener"); return; }
  const { invoke } = await import("@tauri-apps/api/core");
  await invoke<void>("hc_open_url", { url });
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
