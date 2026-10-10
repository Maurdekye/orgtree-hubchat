// Tailscale switches itself off now and then on Android (user 2026-10-10
// 06:44Z): after a restart, for another VPN app, to save battery. A hub that
// is only reachable through it then just looks down. Hubchat can't turn
// Tailscale back on, but the hub notice can say why (Banners.tsx): a hub that
// can't be reached and needs Tailscale, while Android reports no VPN.
// A hub needs Tailscale when its address is a tailnet one (100.64.0.0/10,
// fd7a:115c:a1e0::/48 or a *.ts.net name), or when this device has only ever
// connected to it with a VPN on (remembered here, per hub). A hub this device
// has connected to without a VPN never counts, whatever its address.
import { useSyncExternalStore } from "react";
import { api, type HubState } from "../api";
import { openLink } from "./native";
import { getSnap, subscribeSnap, useSnap } from "./store";

/** A tailnet address: 100.64.0.0/10, fd7a:115c:a1e0::/48 or *.ts.net. */
export function tailnetAddress(url: string): boolean {
  let host: string;
  try { host = new URL(/^[a-z][a-z0-9+.-]*:\/\//i.test(url) ? url : "http://" + url).hostname.toLowerCase(); } catch { return false; }
  host = host.replace(/^\[|\]$/g, "").replace(/\.$/, "");
  if (host.endsWith(".ts.net")) return true;
  const v4 = host.match(/^(\d+)\.(\d+)\.\d+\.\d+$/);
  if (v4) return v4[1] === "100" && Number(v4[2]) >= 64 && Number(v4[2]) <= 127;
  return host.startsWith("fd7a:115c:a1e0:");
}

// per hub URL: "vpn" (connected only with a VPN on, so far) or "direct"
// (connected without one at least once: never blamed on Tailscale)
const KEY = "hubchat.hubs.reach";
let reach: Record<string, "vpn" | "direct"> = {};
try { reach = JSON.parse(localStorage.getItem(KEY) || "{}") || {}; } catch { reach = {}; }
function remember(url: string, viaVpn: boolean) {
  const v = viaVpn ? "vpn" : "direct";
  if (reach[url] === "direct" || reach[url] === v) return;
  reach = { ...reach, [url]: v };
  localStorage.setItem(KEY, JSON.stringify(reach));
}

/** Whether the hub at `url` is only reachable through Tailscale, as far as this device knows. */
export function needsTailscale(url: string): boolean {
  return reach[url] !== "direct" && (reach[url] === "vpn" || tailnetAddress(url));
}

// Android's answer, last asked: true or false; null where it can't be told
let vpn: boolean | null = null;
const subs = new Set<() => void>();
const subscribe = (f: () => void) => { subs.add(f); return () => { subs.delete(f); }; };
async function askVpn(): Promise<boolean | null> {
  const v = await api.vpnActive().catch(() => null);
  if (v !== vpn) { vpn = v; subs.forEach((f) => f()); }
  return v;
}
const tailnetHubDown = () => (getSnap().state?.hubs || []).some((h) => h.state === "disconnected" && needsTailscale(h.url));

let started = false;
/** Android: notes how each hub connects, and keeps the VPN answer fresh while a Tailscale hub is down. */
export function startTailnetWatch(): void {
  if (started) return;
  started = true;
  const was = new Map<string, HubState>();
  subscribeSnap(() => {
    for (const h of getSnap().state?.hubs || []) {
      const before = was.get(h.url);
      if (before === h.state) continue;
      was.set(h.url, h.state);
      // a fresh connection: did it go through a VPN?
      if (h.state === "connected") void askVpn().then((v) => { if (v != null) remember(h.url, v); });
      else if (h.state === "disconnected" && needsTailscale(h.url)) void askVpn();
    }
  });
  // the notice follows Tailscale within a few seconds, and at once when
  // Hubchat is back in front (from Tailscale's app, say)
  setInterval(() => { if (tailnetHubDown()) void askVpn(); }, 5000);
  document.addEventListener("visibilitychange", () => { if (document.visibilityState === "visible" && tailnetHubDown()) void askVpn(); });
}

/** The hub the notice blames on Tailscale: it can't be reached and needs
 *  Tailscale, while Android reports no VPN. Null otherwise (and off Android). */
export function useTailscaleOff() {
  const v = useSyncExternalStore(subscribe, () => vpn);
  const hubs = useSnap().state?.hubs || [];
  if (v !== false) return null;
  return hubs.find((h) => h.state === "disconnected" && needsTailscale(h.url)) || null;
}

/** The Tailscale app, or its Play Store page when it isn't installed. */
export async function openTailscale(): Promise<void> {
  for (const what of ["open_tailscale", "get_tailscale"] as const) {
    if (await api.openApp(what).catch(() => false)) return;
  }
  await openLink("https://play.google.com/store/apps/details?id=com.tailscale.ipn");
}
