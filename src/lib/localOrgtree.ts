// The Orgtree install on this PC (user 2026-10-29): Contact info's "Orgtree
// org" badge opens an org it hosts. Which orgs those are is read natively
// (src-tauri/src/local_orgtree.rs) and shared here: once a minute at most, and
// again when the window comes back after that. Desktop only: an Android
// phone hosts no Orgtree.
import { useEffect, useSyncExternalStore } from "react";
import { api, type LocalOrgtreeOrg } from "../api";

const FRESH_MS = 60_000;
let orgs: LocalOrgtreeOrg[] = [];
let checked = 0;
let reading = false;
const subs = new Set<() => void>();
const emit = () => subs.forEach((f) => f());

function refresh(): void {
  if (reading || Date.now() - checked < FRESH_MS) return;
  reading = true;
  api.localOrgtreeOrgs().then((next) => { orgs = next; }, () => { orgs = []; })
    .finally(() => { checked = Date.now(); reading = false; emit(); });
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

/** Orgtree couldn't be started on it: a plain badge until the next check. */
export function forgetLocalOrgtreeOrg(slug: string): void {
  orgs = orgs.filter((o) => o.slug !== slug);
  emit();
}
