// What this browser is, as far as Hubchat Web has to know.

/** An iPhone, iPad or iPod touch, in any browser (they are all WebKit there). An iPad may say it is a
 *  Mac: a Mac with a touch screen is an iPad. */
export function isApplePhoneOrPad(ua = navigator.userAgent, touch = navigator.maxTouchPoints): boolean {
  return /iPhone|iPad|iPod/.test(ua) || (/Macintosh/.test(ua) && touch > 1);
}

/** Opened from the Home Screen: its own window and, on an iPhone, its own storage (spec R3.6). */
export function isInstalled(): boolean {
  return (navigator as { standalone?: boolean }).standalone === true || window.matchMedia("(display-mode: standalone)").matches;
}

/** The layout, chosen once at start (spec section 8): the phone layout below 900 px, else the desktop one. */
export function layout(): "android" | "desktop" {
  return window.matchMedia("(max-width: 899px)").matches ? "android" : "desktop";
}

/** What this browser is called among the user's devices: "Safari on iPhone". */
export function deviceName(ua = navigator.userAgent, touch = navigator.maxTouchPoints): string {
  const browser = /Edg(e|A|iOS)?\//.test(ua) ? "Edge" : /OPR\//.test(ua) ? "Opera" : /Firefox\/|FxiOS\//.test(ua) ? "Firefox"
    : /Chrome\/|CriOS\//.test(ua) ? "Chrome" : /Safari\//.test(ua) ? "Safari" : "Browser";
  const os = /iPhone|iPod/.test(ua) ? "iPhone" : /iPad/.test(ua) || (/Macintosh/.test(ua) && touch > 1) ? "iPad" : /Android/.test(ua) ? "Android"
    : /Windows/.test(ua) ? "Windows" : /Macintosh|Mac OS X/.test(ua) ? "Mac" : /CrOS/.test(ua) ? "ChromeOS" : /Linux/.test(ua) ? "Linux" : "";
  return os ? browser + " on " + os : browser;
}

/** Why this browser cannot run Hubchat Web at all (spec R3.8), or null when it can. */
export function unsupported(): { title: string; text: string } | null {
  if (!window.isSecureContext) return { title: "Hubchat Web needs a secure address", text: "Open it over https://." };
  const has = typeof Worker !== "undefined" && typeof WebAssembly !== "undefined" && typeof BroadcastChannel !== "undefined"
    && "locks" in navigator && "indexedDB" in window && !!crypto?.subtle && typeof navigator.storage?.getDirectory === "function";
  return has ? null : { title: "Hubchat Web needs a newer browser", text: "Use a current Safari, Chrome, Edge or Firefox." };
}

/** True when this window gives the page no file storage: a private window, or storage turned off. */
export async function storageRefused(): Promise<boolean> {
  try { await navigator.storage.getDirectory(); return false; } catch { return true; }
}

/** An iPhone zooms the page in when a field's text is under 16 px, and never zooms back. With a largest
 *  scale of 1 it leaves the page alone (pinching still zooms there). Only for those devices: elsewhere
 *  the same setting would take pinch zoom away. */
export function keepScale(): void {
  const meta = document.querySelector<HTMLMetaElement>('meta[name="viewport"]');
  if (meta && !/maximum-scale/.test(meta.content)) meta.content += ", maximum-scale=1";
}
