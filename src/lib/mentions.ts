// Hubchat contact mentions (docket hubchat-mentions-mention-hubchat-contacts-as-lin):
// Typing `@` in the composer opens a hint list of known Hubchat contacts/clients;
// Tab/Enter (desktop) or tap (Android) inserts `@net:<address> `, which stays
// readable as plain text on older Hubchat and Orgtree clients while rendering
// as a clickable chat-opening link in both Hubchat (`a.addr.mention`) and
// Orgtree (`HubContactChip`).
import type { ChatSummary, Contact, HubStatus } from "../api";
import { displayName, notOnYourHubs, peerHubs, presence } from "./peers";
import { getSnap, type Snap } from "./store";

const LINK_WAITING = "Waiting to be linked to a Hubchat identity";
const isLinkWaiting = (c: Contact) => /^link\.[^.]+$/.test(c.address) && c.blurb === LINK_WAITING;

export interface MentionToken {
  start: number;
  end: number;
  query: string;
}

/** Known Hubchat contacts for the current identity (directory + active chats, excluding self and link-waiting). */
export function mentionContacts(snap: Snap = getSnap()): Contact[] {
  const me = snap.state?.me?.address;
  const seen = new Set<string>();
  const out: Contact[] = [];
  for (const c of snap.directory) {
    if (!c.address || c.address === me || isLinkWaiting(c) || seen.has(c.address)) continue;
    seen.add(c.address);
    out.push(c);
  }
  for (const ch of snap.chats) {
    if (!ch.peer || ch.peer === me || seen.has(ch.peer)) continue;
    seen.add(ch.peer);
    out.push(snap.byAddr.get(ch.peer) ?? {
      address: ch.peer,
      kind: "person",
      org_name: "",
      username: "",
      blurb: "",
      online: false,
      last_seen: null,
      hubs: [],
    });
  }
  return out;
}

/** Detect an active `@` mention token ending at `caret`. Bare words without `@` never trigger. */
export function activeMentionToken(text: string, caret: number): MentionToken | null {
  const pos = Math.max(0, Math.min(caret, text.length));
  const before = text.slice(0, pos);
  const at = before.lastIndexOf("@");
  if (at < 0) return null;
  if (at > 0 && /[0-9A-Za-z._%+-]/.test(before[at - 1]!)) return null;
  const raw = before.slice(at + 1);
  const m = /^(?:(?:net|org):)?([A-Za-z0-9._-]*)$/i.exec(raw);
  if (!m) return null;
  return { start: at, end: pos, query: m[1]!.toLowerCase() };
}

/** Filter and rank contacts for the `@` hint list. */
export function filterMentionContacts(
  contacts: Contact[],
  hubs: HubStatus[],
  chats: ChatSummary[],
  query: string,
): Contact[] {
  const q = query.trim().toLowerCase().replace(/^(?:net|org):/, "");
  const chatOrder = new Map<string, number>();
  chats.forEach((c, i) => chatOrder.set(c.peer, i));

  const matched = contacts.filter((c) => {
    if (!q) return true;
    const addr = c.address.toLowerCase();
    const name = displayName(c, c.address).toLowerCase();
    const idPart = addr.split(".")[0] || "";
    return (
      addr.includes(q) ||
      name.includes(q) ||
      idPart.includes(q) ||
      c.org_name.toLowerCase().includes(q) ||
      c.username.toLowerCase().includes(q)
    );
  });

  const score = (c: Contact): number => {
    if (!q) return 0;
    const addr = c.address.toLowerCase();
    const name = displayName(c, c.address).toLowerCase();
    const idPart = addr.split(".")[0] || "";
    if (name === q || idPart === q || addr === q) return 3;
    if (name.startsWith(q) || idPart.startsWith(q) || addr.startsWith(q)) return 2;
    return 1;
  };

  return matched
    .slice()
    .sort((a, b) => {
      const sa = score(a), sb = score(b);
      if (sa !== sb) return sb - sa;
      const oa = presence(a, hubs).state === "online" ? 1 : 0;
      const ob = presence(b, hubs).state === "online" ? 1 : 0;
      if (oa !== ob) return ob - oa;
      const ca = chatOrder.get(a.address) ?? 9999;
      const cb = chatOrder.get(b.address) ?? 9999;
      if (ca !== cb) return ca - cb;
      return displayName(a, a.address).localeCompare(displayName(b, b.address)) || a.address.localeCompare(b.address);
    })
    .slice(0, 8);
}

/** Canonical wire token inserted when completing a contact mention. */
export function mentionWireToken(contact: Contact | string): string {
  const addr = (typeof contact === "string" ? contact : contact.address).trim().replace(/^@?(?:net|org):/i, "");
  return "@net:" + addr;
}

/** Replace the active `@...` token with `@net:<address> ` and return the new text and caret. */
export function completeMentionText(
  text: string,
  token: MentionToken,
  contact: Contact | string,
): { text: string; caret: number } {
  const wire = mentionWireToken(contact);
  const after = text.slice(token.end);
  const needsSpace = !after.startsWith(" ");
  const inserted = wire + (needsSpace ? " " : "");
  const next = text.slice(0, token.start) + inserted + after;
  return { text: next, caret: token.start + inserted.length };
}

/** How a mention of `address` shows (user 2026-10-29, docket
 *  hubchat-inline-mentions-of-other-hubchat-clients): "on" when one of my hubs
 *  that isn't disabled lists it in its directory (the composer's own test) or
 *  it is mine; "off" when none does; "unknown" before any directory arrived
 *  (like the composer, it blocks nothing then). `title` is the link's tooltip. */
export interface MentionReach { state: "on" | "off" | "unknown"; contact: Contact | undefined; me: boolean; name: string; title: string }
export function mentionReach(address: string, snap: Snap = getSnap()): MentionReach {
  const me = snap.state?.me;
  const mine = !!me && me.address === address;
  const contact = snap.byAddr.get(address);
  const name = mine ? me!.name || address : displayName(contact, address);
  const listed = !!contact && peerHubs(contact, snap.state?.hubs ?? []).some((h) => h.state !== "disabled");
  const state = mine || listed ? "on" : snap.directory.length ? "off" : "unknown";
  const title = state === "off" ? notOnYourHubs(name) : mine || contact ? `${name} · @net:${address}` : `@net:${address}`;
  return { state, contact, me: mine, name, title };
}

/** Resolve `@net:<addr>`, `@org:<slug>`, or `@<known-contact>` in message Markdown. */
export function resolveMentionTarget(
  raw: string,
  snap: Snap = getSnap(),
): { slug: string; label: string; title: string } | null {
  const netMatch = /^@net:([a-z0-9](?:[a-z0-9._-]*[a-z0-9])?)$/i.exec(raw);
  if (netMatch) {
    const slug = netMatch[1]!.toLowerCase();
    const c = snap.byAddr.get(slug);
    const title = c ? `${displayName(c, slug)} · @net:${slug}` : `@net:${slug}`;
    return { slug, label: raw, title };
  }
  const orgMatch = /^@org:([a-z0-9](?:[a-z0-9._-]*[a-z0-9])?)$/i.exec(raw);
  if (orgMatch) {
    const target = orgMatch[1]!.toLowerCase();
    const contacts = mentionContacts(snap);
    const found = contacts.find((c) => {
      const addr = c.address.toLowerCase();
      return (
        addr === target ||
        (c.kind === "org" && (addr.startsWith(target + ".") || c.org_name.toLowerCase() === target))
      );
    });
    const slug = found ? found.address : target;
    const title = found ? `${displayName(found, slug)} · @net:${slug}` : raw;
    return { slug, label: raw, title };
  }
  const bareMatch = /^@([a-z0-9](?:[a-z0-9._-]*[a-z0-9])?)$/i.exec(raw);
  if (bareMatch) {
    const target = bareMatch[1]!.toLowerCase();
    const contacts = mentionContacts(snap);
    const exactAddr = contacts.find((c) => c.address.toLowerCase() === target);
    if (exactAddr) {
      return {
        slug: exactAddr.address,
        label: raw,
        title: `${displayName(exactAddr, exactAddr.address)} · @net:${exactAddr.address}`,
      };
    }
    const byName = contacts.filter((c) => {
      const idPart = c.address.toLowerCase().split(".")[0] || "";
      return (
        idPart === target ||
        c.username.toLowerCase() === target ||
        c.org_name.toLowerCase() === target ||
        displayName(c, c.address).toLowerCase() === target
      );
    });
    if (byName.length === 1) {
      const c = byName[0]!;
      return {
        slug: c.address,
        label: raw,
        title: `${displayName(c, c.address)} · @net:${c.address}`,
      };
    }
  }
  return null;
}
