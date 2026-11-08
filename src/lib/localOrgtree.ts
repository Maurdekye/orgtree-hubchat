// The Orgtree install on this PC (user 2026-10-29): Contact info's "Orgtree
// org" badge opens an org it hosts. Which orgs those are is read natively
// (src-tauri/src/local_orgtree.rs) and shared here: once a minute at most, and
// again when the window comes back after that. Desktop only: an Android
// phone hosts no Orgtree.
import { useEffect, useSyncExternalStore } from "react";
import { api, type LocalOrgtreeOrg } from "../api";

const FRESH_MS = 60_000;
let orgs: LocalOrgtreeOrg[] = [];
// it opens orgtree:// links (Orgtree 4.5.9 or later): Orgtree mention chips may open
let links = false;
let snapshot: { orgs: LocalOrgtreeOrg[]; links: boolean } = { orgs, links };
let checked = 0;
let reading = false;
const subs = new Set<() => void>();
const emit = () => subs.forEach((f) => f());

function refresh(): void {
  if (reading || Date.now() - checked < FRESH_MS) return;
  reading = true;
  Promise.all([
    api.localOrgtreeOrgs().then((next) => { orgs = next; }, () => { orgs = []; }),
    api.orgtreeLinks().then((next) => { links = next === true; }, () => { links = false; }),
  ]).finally(() => { checked = Date.now(); reading = false; snapshot = { orgs, links }; emit(); });
}

/** The org the Orgtree install on this PC hosts at this address, if any. */
export function useLocalOrgtreeOrg(address: string, enabled: boolean): LocalOrgtreeOrg | undefined {
  useEffect(() => {
    if (!enabled) return;
    refresh();
    window.addEventListener("focus", refresh);
    return () => window.removeEventListener("focus", refresh);
  }, [enabled]);
  const list = useSyncExternalStore((f) => { subs.add(f); return () => { subs.delete(f); }; }, () => orgs);
  return enabled ? list.find((o) => o.address === address) : undefined;
}

/** This PC's Orgtree: the orgs it hosts and whether it opens orgtree:// links
 *  (Orgtree mention chips, lib/orgtreeRefs.ts). Nothing while disabled. */
export function useLocalOrgtree(enabled: boolean): { orgs: LocalOrgtreeOrg[]; links: boolean } {
  useEffect(() => {
    if (!enabled) return;
    refresh();
    window.addEventListener("focus", refresh);
    return () => window.removeEventListener("focus", refresh);
  }, [enabled]);
  const now = useSyncExternalStore((f) => { subs.add(f); return () => { subs.delete(f); }; }, () => snapshot);
  return enabled ? now : NONE;
}
const NONE = { orgs: [] as LocalOrgtreeOrg[], links: false };

/** Orgtree couldn't be started on it: a plain badge until the next check. */
export function forgetLocalOrgtreeOrg(slug: string): void {
  orgs = orgs.filter((o) => o.slug !== slug);
  snapshot = { orgs, links };
  emit();
}
