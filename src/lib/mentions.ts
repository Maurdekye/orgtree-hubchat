// Hubchat contact mentions (docket hubchat-mentions-mention-hubchat-contacts-as-lin):
// Typing `@` in the composer opens a hint list of known Hubchat contacts/clients;
// Tab/Enter (desktop) or tap (Android) inserts `@net:<address> `, which stays
// readable as plain text on older Hubchat and Orgtree clients while rendering
// as a clickable chat-opening link in both Hubchat (`a.addr.mention`) and
// Orgtree (`HubContactChip`).
import { api, type ChatSummary, type Contact, type HubStatus, type Message } from "../api";
import { useEffect, useMemo, useState } from "react";
import { ms } from "./format";
import { displayName, kindOf, msgTime, notOnYourHubs, peerHubs, presence } from "./peers";
import { getSnap, type Snap } from "./store";

const LINK_WAITING = "Waiting to be linked to a Hubchat identity";
const isLinkWaiting = (c: Contact) => /^link\.[^.]+$/.test(c.address) && c.blurb === LINK_WAITING;

/** Maximum outgoing messages scanned per chat when computing account-wide mention recency. */
export const MENTION_SCAN_LIMIT = 200;
/** Page step for the bottom-up mention menu, matching Orgtree's 20-row mention page size. */
export const MENU_STEP = 20;

export interface MentionToken {
  start: number;
  end: number;
  query: string;
}

/** Replace fenced code blocks and inline code spans with spaces so mentions inside code are ignored. */
export function stripMarkdownCode(text: string): string {
  const chars = text.split("");
  const len = text.length;
  let i = 0;
  let lineStart = true;

  while (i < len) {
    if (lineStart) {
      let p = i;
      while (p < len && (text[p] === " " || text[p] === "\t")) p++;
      const ch = text[p];
      if (ch === "`" || ch === "~") {
        let fenceLen = 0;
        while (p + fenceLen < len && text[p + fenceLen] === ch) fenceLen++;
        if (fenceLen >= 3) {
          let closeStart = text.indexOf("\n", p + fenceLen);
          let end = len;
          while (closeStart !== -1) {
            let cp = closeStart + 1;
            while (cp < len && (text[cp] === " " || text[cp] === "\t")) cp++;
            let cLen = 0;
            while (cp + cLen < len && text[cp + cLen] === ch) cLen++;
            let after = cp + cLen;
            while (after < len && (text[after] === " " || text[after] === "\t" || text[after] === "\r")) after++;
            if (cLen >= fenceLen && (after >= len || text[after] === "\n")) {
              end = after < len ? after + 1 : len;
              break;
            }
            closeStart = text.indexOf("\n", closeStart + 1);
          }
          for (let k = i; k < end; k++) {
            if (chars[k] !== "\n") chars[k] = " ";
          }
          i = end;
          lineStart = true;
          continue;
        }
      }
    }

    if (text[i] === "`") {
      let tickLen = 0;
      while (i + tickLen < len && text[i + tickLen] === "`") tickLen++;
      let j = i + tickLen;
      let found = -1;
      while (j < len) {
        if (text[j] === "`") {
          let closeLen = 0;
          while (j + closeLen < len && text[j + closeLen] === "`") closeLen++;
          if (closeLen === tickLen) {
            found = j + closeLen;
            break;
          }
          j += closeLen;
        } else {
          j++;
        }
      }
      if (found !== -1) {
        for (let k = i; k < found; k++) {
          if (chars[k] !== "\n") chars[k] = " ";
        }
        i = found;
        lineStart = false;
        continue;
      }
      i += tickLen;
      lineStart = false;
      continue;
    }

    lineStart = text[i] === "\n";
    i++;
  }

  return chars.join("");
}

const TRANSCRIPT_MENTION_RE = /(?:^|[^0-9A-Za-z._%+-])(@(?:(?:net|org):)?[A-Za-z0-9](?:(?:\.|[_-]+)?[A-Za-z0-9])*)/gi;

/** Extract lowercased contact addresses mentioned in a message body (excluding Markdown code). */
export function extractMentionSlugs(
  rawText: string,
  contacts: Contact[] = mentionContacts(),
  snap: Snap = getSnap(),
): string[] {
  if (!rawText) return [];
  const text = stripMarkdownCode(rawText);
  const out: string[] = [];
  TRANSCRIPT_MENTION_RE.lastIndex = 0;
  let m: RegExpExecArray | null;
  while ((m = TRANSCRIPT_MENTION_RE.exec(text)) !== null) {
    const raw = m[1]!;
    const at = m.index + (m[0].length - raw.length);
    const next = text[at + raw.length];
    if (next && /[0-9A-Za-z@]/.test(next)) continue;
    const tokenSlugs = new Set<string>();
    const resolved = resolveMentionTarget(raw, snap, contacts);
    if (resolved) {
      tokenSlugs.add(resolved.slug.toLowerCase());
    }
    const prefixed = /^@(?:net|org):([A-Za-z0-9._-]+)$/i.exec(raw);
    if (prefixed) {
      const token = prefixed[1]!.toLowerCase().replace(/[._-]+$/, "");
      if (token) {
        for (const c of contacts) {
          const slug = c.address.toLowerCase();
          if (slug === token || slug.startsWith(token + ".")) {
            tokenSlugs.add(slug);
          }
        }
      }
    }
    for (const slug of tokenSlugs) {
      out.push(slug);
    }
  }
  return out;
}

/** Extract in-conversation mention recency (Bucket 1): maps lowercased address -> sequence number (higher = more recent). */
export function extractMentionRecency(
  messages: readonly (Message | string)[],
  contacts: Contact[] = mentionContacts(),
  snap: Snap = getSnap(),
): Map<string, number> {
  const recency = new Map<string, number>();
  let seq = 0;
  for (const item of messages) {
    const body = typeof item === "string" ? item : item?.body ?? "";
    if (!body) continue;
    for (const slug of extractMentionSlugs(body, contacts, snap)) {
      seq += 1;
      recency.set(slug, seq);
    }
  }
  return recency;
}

/** Extract latest mention timestamp (ms) per contact from this account's outgoing messages (capped at MENTION_SCAN_LIMIT). */
export function extractOutgoingMentionTimes(
  messages: readonly Message[],
  contacts: Contact[] = mentionContacts(),
  snap: Snap = getSnap(),
): Map<string, number> {
  const mentioned = new Map<string, number>();
  const outgoing = messages.filter((m) => m && m.outgoing && m.body);
  const capped = outgoing.length > MENTION_SCAN_LIMIT ? outgoing.slice(-MENTION_SCAN_LIMIT) : outgoing;
  for (const m of capped) {
    const t = msgTime(m) || ms(m.created_at);
    if (!t) continue;
    for (const slug of extractMentionSlugs(m.body, contacts, snap)) {
      if (t > (mentioned.get(slug) ?? 0)) {
        mentioned.set(slug, t);
      }
    }
  }
  return mentioned;
}

interface CachedChatOutgoing {
  lastId: string;
  outgoing: Message[];
}
const chatOutgoingCache = new Map<string, CachedChatOutgoing>();
let cachedMeAddress = "";

function ensureCacheIdentity(meAddress: string) {
  if (meAddress !== cachedMeAddress) {
    cachedMeAddress = meAddress;
    chatOutgoingCache.clear();
  }
}

/** Compute and cache account-wide outgoing mention timestamps without calling api.chat per keystroke. */
export function useAccountMentions(
  active: boolean,
  chats: ChatSummary[],
  contacts: Contact[],
  openPeer: string,
  openMsgs?: readonly Message[],
  meAddress = getSnap().state?.me?.address ?? "",
): ReadonlyMap<string, number> {
  ensureCacheIdentity(meAddress);
  const [cacheVer, setCacheVer] = useState(0);

  // Update the open peer's cache entry from already-loaded messages without calling api.chat.
  useEffect(() => {
    if (!openPeer || !openMsgs || !openMsgs.length) return;
    ensureCacheIdentity(meAddress);
    const lastId = openMsgs[openMsgs.length - 1]?.id ?? "";
    const outgoing = openMsgs.filter((m) => m.outgoing && m.body).slice(-MENTION_SCAN_LIMIT);
    const prev = chatOutgoingCache.get(openPeer);
    if (!prev || prev.lastId !== lastId || prev.outgoing.length !== outgoing.length) {
      chatOutgoingCache.set(openPeer, { lastId, outgoing });
      setCacheVer((v) => v + 1);
    }
  }, [openPeer, openMsgs, meAddress]);

  // When the menu opens (or chats change while open), scan up to MENTION_SCAN_LIMIT messages for chats not yet cached.
  useEffect(() => {
    if (!active) return;
    ensureCacheIdentity(meAddress);
    let live = true;
    for (const ch of chats) {
      const peer = ch.peer;
      const lastId = ch.last?.id ?? "";
      if (!peer || !lastId) continue;
      if (peer === openPeer && openMsgs && openMsgs.length > 0) continue;
      const cached = chatOutgoingCache.get(peer);
      if (cached && cached.lastId === lastId) continue;
      chatOutgoingCache.set(peer, { lastId, outgoing: cached?.outgoing ?? (ch.last?.outgoing ? [ch.last] : []) });
      void api.chat(peer, { limit: MENTION_SCAN_LIMIT }).then(
        (msgs) => {
          if (!live || cachedMeAddress !== meAddress) return;
          const outgoing = (msgs ?? []).filter((m) => m.outgoing && m.body).slice(-MENTION_SCAN_LIMIT);
          chatOutgoingCache.set(peer, { lastId, outgoing });
          setCacheVer((v) => v + 1);
        },
        () => {},
      );
    }
    return () => {
      live = false;
    };
  }, [active, chats, openPeer, openMsgs, meAddress]);

  return useMemo(() => {
    const snap = getSnap();
    const out = new Map<string, number>();
    const mergeFrom = (msgs: readonly Message[]) => {
      for (const [slug, t] of extractOutgoingMentionTimes(msgs, contacts, snap)) {
        if (t > (out.get(slug) ?? 0)) out.set(slug, t);
      }
    };
    for (const ch of chats) {
      if (ch.last?.outgoing && ch.last.body) mergeFrom([ch.last]);
      const cached = chatOutgoingCache.get(ch.peer);
      if (cached?.outgoing.length) mergeFrom(cached.outgoing);
    }
    if (openMsgs?.length) mergeFrom(openMsgs);
    return out;
  }, [chats, contacts, openMsgs, cacheVer]);
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

/** Filter and rank contacts for the `@` hint list (best match at index 0; caller reverses for bottom-up display):
 *  - Word-substring filtering on `/[-_./\s]+/` (typing narrows and never reorders).
 *  - Bucket 1 (`inTranscript`): contacts mentioned in the open chat (`recency`), newest first (`b.seq - a.seq`).
 *  - Bucket 2 (`rest`): remaining contacts ordered by `max(lastChatted, lastMentionedByMe)` descending,
 *    then lowercased display name, kind, and address ascending (mirroring Orgtree's mentions menu). */
export function filterMentionContacts(
  contacts: Contact[],
  _hubs: HubStatus[],
  chats: ChatSummary[],
  query: string,
  recency?: ReadonlyMap<string, number>,
  mentioned?: ReadonlyMap<string, number>,
): Contact[] {
  const q = query.trim().toLowerCase().replace(/^(?:net|org):/, "");
  const words = q.split(/[-_./\s]+/).filter(Boolean);

  const chatAt = new Map<string, number>();
  const mentionedAt = new Map<string, number>(mentioned);
  for (const ch of chats) {
    if (!ch.peer || !ch.last) continue;
    const t = msgTime(ch.last) || ms(ch.last.created_at);
    const peerKey = ch.peer.toLowerCase();
    if (t > (chatAt.get(peerKey) ?? 0)) chatAt.set(peerKey, t);
    if (ch.last.outgoing && ch.last.body && t > 0) {
      for (const slug of extractMentionSlugs(ch.last.body, contacts)) {
        if (t > (mentionedAt.get(slug) ?? 0)) mentionedAt.set(slug, t);
      }
    }
  }

  const inTranscript: { c: Contact; seq: number; name: string; kind: string; addr: string }[] = [];
  const rest: { c: Contact; at: number; name: string; kind: string; addr: string }[] = [];

  for (const c of contacts) {
    const addr = c.address.toLowerCase();
    const name = displayName(c, c.address).toLowerCase();
    const org = (c.org_name || "").toLowerCase();
    const user = (c.username || "").toLowerCase();
    const blurb = (c.blurb || "").toLowerCase();
    if (words.length > 0 && !words.every((w) => name.includes(w) || addr.includes(w) || org.includes(w) || user.includes(w) || blurb.includes(w))) {
      continue;
    }
    const kind = kindOf(c);
    const seq = recency?.get(addr);
    if (seq !== undefined) {
      inTranscript.push({ c, seq, name, kind, addr });
    } else {
      const at = Math.max(chatAt.get(addr) ?? 0, mentionedAt.get(addr) ?? 0);
      rest.push({ c, at, name, kind, addr });
    }
  }

  inTranscript.sort(
    (a, b) =>
      b.seq - a.seq ||
      (a.name < b.name ? -1 : a.name > b.name ? 1 : 0) ||
      (a.kind < b.kind ? -1 : a.kind > b.kind ? 1 : 0) ||
      (a.addr < b.addr ? -1 : a.addr > b.addr ? 1 : 0),
  );
  rest.sort(
    (a, b) =>
      b.at - a.at ||
      (a.name < b.name ? -1 : a.name > b.name ? 1 : 0) ||
      (a.kind < b.kind ? -1 : a.kind > b.kind ? 1 : 0) ||
      (a.addr < b.addr ? -1 : a.addr > b.addr ? 1 : 0),
  );

  return [...inTranscript.map((x) => x.c), ...rest.map((x) => x.c)];
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

/** A contact's type in the user's words, as Orgtree's hover names it
 *  (Orgtree netmentions.tsx, USER 2026-10-29 "(org, person, independent chat)"). */
const TYPE_WORD = { org: "org", person: "person", chat: "independent chat" } as const;
const PRESENCE_WORD = { online: "Online", offline: "Offline", disconnected: "Status unknown" } as const;

/** How a mention of `address` shows (user 2026-10-29, docket
 *  hubchat-inline-mentions-of-other-hubchat-clients): "on" when one of my hubs
 *  that isn't disabled lists it in its directory (the composer's own test) or
 *  it is mine; "off" when none does; "unknown" before any directory arrived
 *  (like the composer, it blocks nothing then). `title` is the link's tooltip:
 *  for "on", Orgtree's lines for the same address (symmetry audit #2): presence
 *  and type, then the name, the hub's description and the address (when it has
 *  a name), then what a click does; for "off", the composer's sentence. */
export interface MentionReach {
  state: "on" | "off" | "unknown"; contact: Contact | undefined; me: boolean; name: string; title: string;
  /** The roster's name a reachable mention shows in place of its address, as
   *  Orgtree's contact chip does (symmetry audit #2); null keeps the address. */
  shown: string | null;
}
export function mentionReach(address: string, snap: Snap = getSnap()): MentionReach {
  const me = snap.state?.me;
  const mine = !!me && me.address === address;
  const contact = snap.byAddr.get(address);
  const name = mine ? me!.name || address : displayName(contact, address);
  const hubs = snap.state?.hubs ?? [];
  const listed = !!contact && peerHubs(contact, hubs).some((h) => h.state !== "disabled");
  const state = mine || listed ? "on" : snap.directory.length ? "off" : "unknown";
  const named = mine ? !!me!.name : !!(contact && (contact.org_name || contact.username));
  const status = mine ? "You" : listed ? PRESENCE_WORD[presence(contact, hubs).state] + " · " + TYPE_WORD[kindOf(contact)] : null;
  const title = state === "off" ? notOnYourHubs(name)
    : status ? [status, named ? name : null, mine ? null : contact?.blurb, named ? "@net:" + address : null, "Open chat"].filter(Boolean).join("\n")
    : "@net:" + address;
  return { state, contact, me: mine, name, title, shown: state === "on" && named ? name : null };
}

/** Resolve `@net:<addr>`, `@org:<slug>`, or `@<known-contact>` in message Markdown. */
export function resolveMentionTarget(
  raw: string,
  snap: Snap = getSnap(),
  contacts: Contact[] = mentionContacts(snap),
): { slug: string; label: string; title: string } | null {
  const netMatch = /^@net:([a-z0-9](?:[a-z0-9._-]*[a-z0-9])?)$/i.exec(raw);
  if (netMatch) {
    const slug = netMatch[1]!.toLowerCase();
    const c = snap.byAddr.get(slug) ?? contacts.find((item) => item.address.toLowerCase() === slug);
    const title = c ? `${displayName(c, slug)} · @net:${slug}` : `@net:${slug}`;
    return { slug, label: raw, title };
  }
  const orgMatch = /^@org:([a-z0-9](?:[a-z0-9._-]*[a-z0-9])?)$/i.exec(raw);
  if (orgMatch) {
    const target = orgMatch[1]!.toLowerCase();
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
