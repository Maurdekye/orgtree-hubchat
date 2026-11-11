// Notifications in Hubchat Web (spec R4.1, R4.6). A phone freezes a page in the background, so a
// browser hears of new mail only through its own push: the page subscribes, under a signing key it
// makes itself, and hands the subscription and that key to the core, which registers them with every
// hub (crates/hubchat-core/src/engine_webpush.rs). Each hub then pushes its notifications there, and
// the service worker shows them (web/sw.template.js). The page never raises a notification itself.
import { workerRegistration } from "./update";

/** One command to the core (webApi.ts). */
type Call = <T>(cmd: string, args?: Record<string, unknown>) => Promise<T>;

/** What the core keeps of this browser's notifications (`hc_web_push`). */
interface Kept { endpoint: string | null; preview: boolean; resubscribe: boolean }

const b64url = (bytes: ArrayBuffer): string =>
  btoa(String.fromCharCode(...new Uint8Array(bytes))).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");

/** The web app's own address, which a notification opens. */
function appAddress(): string {
  const u = new URL("./", document.baseURI);
  u.hash = "";
  u.search = "";
  return u.href;
}

/** A new subscription, under a key made for it: what a hub needs to push to this browser. */
async function subscribe(): Promise<Record<string, unknown>> {
  const registration = await workerRegistration();
  if (!registration || !("pushManager" in registration)) throw "This browser can't show notifications for Hubchat Web.";
  // one subscription per app, tied to one key: an older one is ended first
  const old = await registration.pushManager.getSubscription();
  if (old) await old.unsubscribe().catch(() => false);
  const pair = await crypto.subtle.generateKey({ name: "ECDSA", namedCurve: "P-256" }, true, ["sign"]);
  const publicKey = await crypto.subtle.exportKey("raw", pair.publicKey);
  const privateKey = await crypto.subtle.exportKey("jwk", pair.privateKey);
  const made = await registration.pushManager.subscribe({ userVisibleOnly: true, applicationServerKey: publicKey });
  const j = made.toJSON();
  if (!j.endpoint || !j.keys?.p256dh || !j.keys?.auth || !privateKey.d) {
    await made.unsubscribe().catch(() => false);
    throw "This browser gave Hubchat no way to send it notifications.";
  }
  // `d` is the key's 32 secret bytes, base64url: what a hub signs its pushes with
  return { endpoint: j.endpoint, p256dh: b64url(base64urlBytes(j.keys.p256dh)), auth: b64url(base64urlBytes(j.keys.auth)), vapid: privateKey.d, navigate: appAddress() };
}

/** A browser's base64url text as bytes (some pad it, the hubs take it unpadded). */
function base64urlBytes(text: string): ArrayBuffer {
  const s = atob(text.replace(/-/g, "+").replace(/_/g, "/"));
  const out = new Uint8Array(s.length);
  for (let i = 0; i < s.length; i++) out[i] = s.charCodeAt(i);
  return out.buffer;
}

async function endSubscription(): Promise<void> {
  try {
    const registration = await workerRegistration();
    const now = await registration?.pushManager?.getSubscription();
    if (now) await now.unsubscribe();
  } catch { /* nothing to end */ }
}

// one at a time: two subscribes would each end the other's subscription
let turn: Promise<unknown> = Promise.resolve();
const inTurn = <T>(work: () => Promise<T>): Promise<T> => {
  const next = turn.then(work, work);
  turn = next.catch(() => { /* the caller hears of it */ });
  return next;
};

/** Turn notifications on. Call it in the tap: the browser asks the person only then. Rejects with a sentence. */
export function enablePush(call: Call, preview: boolean): Promise<void> {
  return inTurn(async () => {
    if (!("Notification" in window) || !("serviceWorker" in navigator) || !("PushManager" in window)) throw "This browser can't show notifications for Hubchat Web.";
    const answer = await Notification.requestPermission();
    if (answer !== "granted") throw "Notifications are blocked for Hubchat in this browser. Allow them for this site in the browser's settings, then turn this on again.";
    let subscription: Record<string, unknown>;
    try {
      subscription = await subscribe();
    } catch (e) {
      throw typeof e === "string" ? e : "This browser couldn't set up notifications" + (e instanceof Error && e.message ? " (" + e.message + ")." : ".");
    }
    await call("hc_web_push_set", { subscription: { ...subscription, preview } });
  });
}

/** Turn them off: the browser's subscription ends and every hub forgets it. `now`: the hubs are told
 *  before this settles (Start over: nothing is left afterwards to tell them with). */
export function disablePush(call: Call, now: boolean): Promise<void> {
  return inTurn(async () => {
    await endSubscription();
    await (now ? call("hc_web_push_off") : call("hc_web_push_set", { subscription: null }));
  });
}

/** Bring the browser's subscription and what the hubs were handed in line: at each start, when the
 *  browser says the subscription changed, and (`onlyWhenOwed`) when a removed hub could not be told
 *  to forget it, so a new subscription must replace the one it still holds. */
export function syncPush(call: Call, onlyWhenOwed = false): Promise<void> {
  return inTurn(async () => {
    const kept = await call<Kept>("hc_web_push");
    if (!kept.endpoint || (onlyWhenOwed && !kept.resubscribe)) return;
    const registration = await workerRegistration();
    const now = await registration?.pushManager?.getSubscription().catch(() => null);
    const allowed = "Notification" in window && Notification.permission === "granted";
    if (allowed && now && now.endpoint === kept.endpoint && !kept.resubscribe) return;
    if (!allowed || !registration) {
      // the person took the permission back in the browser: the hubs stop sending
      await endSubscription();
      await call("hc_web_push_set", { subscription: null });
      return;
    }
    const subscription = await subscribe();
    await call("hc_web_push_set", { subscription: { ...subscription, preview: kept.preview } });
  });
}

// ---- a tapped notification names a chat

let pending: string | null = null;
/** The chat a `navigate` address names (`.../#/chat/<address>`), if it names one. */
function chatOf(address: string): string | null {
  const m = /#\/chat\/([a-z0-9][a-z0-9._-]{0,80})$/i.exec(address);
  return m ? m[1] : null;
}
// opened by a tapped notification: the address carries the chat
pending = chatOf(location.href);
if (pending) history.replaceState(null, "", location.pathname + location.search);

/** Whether a tapped notification is waiting to open its chat. */
export function pendingChat(): boolean {
  return pending !== null;
}
/** The chat a tapped notification asked for, once (the apps' `hc_take_pending_chat`). */
export function takePendingChat(): string | null {
  const p = pending;
  pending = null;
  return p;
}

/** Hear the service worker: a notification tapped while the app is open, a subscription the browser changed. */
export function watchPush(call: Call): void {
  if (!("serviceWorker" in navigator)) return;
  navigator.serviceWorker.addEventListener("message", (e: MessageEvent<{ t?: string; navigate?: string } | null>) => {
    const m = e.data;
    if (m?.t === "open" && typeof m.navigate === "string") {
      pending = chatOf(m.navigate);
      if (pending) window.dispatchEvent(new Event("hc-pending"));
    } else if (m?.t === "push-changed") {
      void syncPush(call).catch(() => { /* next start */ });
    }
  });
}

// ---- the unread count, while the app is open (R4.6, R4.7)

/** Show the unread count on the app's icon and in the tab's title: `(3) Hubchat`. */
export function showUnread(count: number): void {
  document.title = count > 0 ? "(" + (count > 999 ? "999+" : count) + ") Hubchat" : "Hubchat";
  const n = navigator as Navigator & { setAppBadge?: (n?: number) => Promise<void>; clearAppBadge?: () => Promise<void> };
  try {
    if (count > 0) void n.setAppBadge?.(count)?.catch(() => { /* no badge here */ });
    else void n.clearAppBadge?.()?.catch(() => { /* no badge here */ });
  } catch { /* no badge here */ }
}
