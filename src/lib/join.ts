// A signed-in device joining another device's link (user 19:21-19:22Z): the
// request lives here so a scan, a deep link or Settings › Devices can start it
// and the platform layout shows it (desktop: a modal; Android: a screen).
// Routing by the link's role: "give" (a signed-in device offers its identity)
// means join; "take" (a new device asks for one) means approve; a typed code
// (no role) keeps the caller's own meaning.
import { useSyncExternalStore } from "react";
import { api, type ParsedLink } from "../api";

export interface JoinReq { code: string; hub: string | null; hubs: string[]; hubName: string | null; n: number }

let cur: JoinReq | null = null;
let n = 0;
const subs = new Set<() => void>();
const changed = () => subs.forEach((f) => f());

/** Join `code` through `hub` (null: ask which hub); `hubs`: every address the
 *  link names for it, `hubName`: the name it must answer with. */
export function startJoin(code: string, hub: string | null, hubs: string[] = [], hubName: string | null = null): void { cur = { code, hub, hubs, hubName, n: ++n }; changed(); }
export function endJoin(): void { cur = null; changed(); }
export function useJoin(): JoinReq | null {
  return useSyncExternalStore((f) => { subs.add(f); return () => { subs.delete(f); }; }, () => cur);
}

export type LinkRoute = { k: "join"; p: ParsedLink } | { k: "approve"; p: ParsedLink; input: string };

/** What a scanned, pasted or deep-linked link asks of this device.
 *  `typed`: what a code without a role means where it was entered. */
export async function routeLink(input: string, typed: "join" | "approve"): Promise<LinkRoute> {
  const t = input.trim();
  const p = await api.parseLink(t);
  const k = p.role === "give" ? "join" : p.role === "take" ? "approve" : typed;
  return k === "join" ? { k, p } : { k, p, input: t };
}
