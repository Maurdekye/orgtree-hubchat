// Scan setup code (user 2026-10-09 10:38Z, mockup v2): Orgtree on a PC
// shows a hubchat://setup QR; Hubchat checks the phone side, joins the PC's
// hub and messages the org with the code. The request lives here so the
// welcome screen, New chat, Settings › Hubs or a deep link can start it and
// App shows it (Android: a screen; desktop: a modal).
import { useEffect, useRef, useSyncExternalStore } from "react";
import { api, isTauri } from "../api";
import { errText, openLink, scanQr } from "./native";
import { checkForUpdate } from "./updates";
import { toast } from "./toast";

export interface SetupReq { input: string; n: number }

let cur: SetupReq | null = null;
let n = 0;
const subs = new Set<() => void>();
const changed = () => subs.forEach((f) => f());

export function startSetup(input: string): void { cur = { input, n: ++n }; changed(); }
export function endSetup(): void { cur = null; changed(); }
export function useSetup(): SetupReq | null {
  return useSyncExternalStore((f) => { subs.add(f); return () => { subs.delete(f); }; }, () => cur);
}

/** A hubchat://setup link (a deep link from the phone's camera, or a scan). */
export const isSetupLink = (t: string) => /^hubchat:\/\/setup(?:[/?#]|$)/i.test(t.trim());

export const NOT_SETUP = "That isn't a setup code. On your PC, open Orgtree › App settings › Mail hub › Connect your phone.";

/** What the browser mock's camera reads by default: Orgtree's setup QR. */
export const MOCK_SETUP_QR = "hubchat://setup?v=1&hub=" + encodeURIComponent("http://100.101.102.103:7371") +
  "&org=my-org.alex.3f9c2a&orgname=" + encodeURIComponent("My Org") + "&pc=home-pc&ts=" + encodeURIComponent("alex@gmail.com") +
  "&code=K7QD-4MXP&net=tailscale&hubname=home-pc";

/** Scan Orgtree's setup QR and start the flow. */
export async function scanSetup(): Promise<void> {
  let t: string | null;
  try { t = (await scanQr(MOCK_SETUP_QR))?.trim() ?? null; } catch (e) { toast(errText(e)); return; }
  if (!t) return;
  if (!isSetupLink(t)) { toast(NOT_SETUP); return; }
  startSetup(t);
}

// The chat to open once the flow is done: the layout may only mount then
// (a new identity leaves onboarding first).
let chatToOpen: string | null = null;
const chatSubs = new Set<() => void>();
export function openAfterSetup(peer: string): void { chatToOpen = peer; chatSubs.forEach((f) => f()); }
/** The layout opens the chat a finished setup asks for (now, or when it comes). */
export function useSetupChat(open: (peer: string) => void): void {
  const ref = useRef(open);
  ref.current = open;
  useEffect(() => {
    const take = () => { const p = chatToOpen; chatToOpen = null; if (p) ref.current(p); };
    take();
    chatSubs.add(take);
    return () => { chatSubs.delete(take); };
  }, []);
}

/** Android's fixed-name APK: what Update opens until the in-app updater lands. */
const APK = "https://github.com/Maurdekye/orgtree-hubchat/releases/latest/download/Hubchat-android.apk";

/** "This code needs a newer Hubchat" › Update (hubchat-opus 11:38Z): the
 *  updates module where it can install; on Android, until its updater
 *  lands, the newest APK in the browser. */
export async function updateHubchat(platform: "desktop" | "android"): Promise<void> {
  if (platform === "android" || !isTauri) { await openLink(APK); return; }
  const a = await checkForUpdate();
  if (a) await a.install();
  else toast("No newer Hubchat is out yet.");
}

/** Open Tailscale or the Wi-Fi settings (Android); false when nothing opened. */
export async function openOther(what: "get_tailscale" | "open_tailscale" | "wifi_settings"): Promise<void> {
  let ok = false;
  try { ok = await api.openApp(what); } catch { /* no such app */ }
  if (!ok && what === "get_tailscale") await openLink("https://play.google.com/store/apps/details?id=com.tailscale.ipn");
  else if (!ok) toast(what === "open_tailscale" ? "Couldn't open Tailscale." : "Couldn't open the Wi-Fi settings.");
}

// "Already use Hubchat on another device? Link this phone instead." on the
// name step (gap 4: a second device gets no second identity): onboarding
// opens today's device linking.
const insteadSubs = new Set<() => void>();
export function wantLinkInstead(): void { insteadSubs.forEach((f) => f()); }
export function useLinkInstead(f: () => void): void {
  const ref = useRef(f);
  ref.current = f;
  useEffect(() => {
    const g = () => ref.current();
    insteadSubs.add(g);
    return () => { insteadSubs.delete(g); };
  }, []);
}
