// Blocking a contact (Mail hub 2.6.0, hub feature `blocking`; Orgtree docket
// blocking-contacts-on-the-hub-hubchat-1-4-0-orgtr, user 2026-11-24 18:56Z and
// 19:06Z). The hubs keep and enforce it, both ways: a blocked contact can't
// message us and we can't message them until we unblock. Every contact menu
// offers Block / Unblock (one entry, `blockEntry`); blocking asks first
// (`BlockConfirm`, rendered once at the root). In the DM the composer gives way
// to `BlockBar`: "You blocked …" with Unblock, or the user's words when they
// block us.
import { useSyncExternalStore } from "react";
import { api, type Contact, type HubStatus } from "../api";
import type { MenuItem } from "../lib/ctxmenu";
import { Icon } from "../lib/icons";
import { displayName } from "../lib/peers";
import { errText } from "../lib/native";
import { toast } from "../lib/toast";
import { useSnap } from "../lib/store";
import { Confirm } from "./ui";

let asking: string | null = null;
const subs = new Set<() => void>();
const emit = () => subs.forEach((f) => f());

/** Can any of my hubs keep a block? (Mail hub 2.6.0 lists `blocking`.) */
export const canBlock = (hubs: HubStatus[]) => hubs.some((h) => (h.features || []).includes("blocking"));

/** The words when none can: the user's "needs an update" (plan Q8). */
export function blockNeedsUpdate(hubs: HubStatus[]): string {
  const names = hubs.map((h) => h.name).filter(Boolean);
  return (names.length === 1 ? names[0] : "Your hubs") + " needs an update before you can block contacts.";
}

export function unblock(peer: string) {
  void api.setBlocked(peer, false).catch((e) => toast(errText(e)));
}

/** Block asks first; Unblock does it at once. Disabled, with the reason, when no hub can keep it. */
export function blockEntry(peer: string, c: Contact | undefined, hubs: HubStatus[]): MenuItem {
  if (c?.blocked) return { label: "Unblock", icon: "block", run: () => unblock(peer) };
  const ok = canBlock(hubs);
  return { label: "Block", icon: "block", bad: true, run: () => { asking = peer; emit(); }, disabled: !ok, title: ok ? undefined : blockNeedsUpdate(hubs) };
}

export function askBlock(peer: string) {
  asking = peer;
  emit();
}

/** The one confirm dialog for Block, wherever it was asked. */
export function BlockConfirm() {
  const peer = useSyncExternalStore((f) => { subs.add(f); return () => { subs.delete(f); }; }, () => asking);
  const snap = useSnap();
  if (!peer) return null;
  const name = displayName(snap.byAddr.get(peer), peer);
  const close = () => { asking = null; emit(); };
  return (
    <Confirm title={"Block " + name + "?"} okLabel="Block" danger onCancel={close}
      onOk={() => { close(); void api.setBlocked(peer, true).catch((e) => toast(errText(e))); }}>
      <p>They can't message you, and you can't message them until you unblock them. Your chat history stays.</p>
    </Confirm>
  );
}

/** In the DM, in place of the composer, while either side blocks the other. */
export function BlockBar({ peer, c }: { peer: string; c: Contact | undefined }) {
  if (c?.blocked) {
    return (
      <div className="block-bar" role="status">
        <Icon name="block" /><span>You blocked {displayName(c, peer)}.</span>
        <button className="link" onClick={() => unblock(peer)}>Unblock</button>
      </div>
    );
  }
  if (c?.blocked_by) {
    return (
      <div className="block-bar bad" role="status">
        <Icon name="block" /><span>You have been blocked by @{displayName(c, peer)} and cannot message them</span>
      </div>
    );
  }
  return null;
}
