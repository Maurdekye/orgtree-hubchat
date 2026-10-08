// Who someone is, how to reach them and what state they are in, computed from
// the directory and the hub statuses the core reports.
import type { Attachment, Contact, HubStatus, Message } from "../api";
import { ago, bytes, ms } from "./format";
import { isImage } from "./images";
import { plain } from "./md";
import type { IconName } from "./icons";

export type Kind = "org" | "chat" | "person";
export const kindOf = (c: Contact | undefined): Kind => (c?.kind === "org" || c?.kind === "chat" ? c.kind : "person");
export const isAgent = (c: Contact | undefined) => kindOf(c) !== "person";

export interface KindInfo { label: string; short: string; icon: IconName; cls: string; what: string }
export function kindInfo(kind: Kind): KindInfo {
  if (kind === "org") return { label: "Orgtree org", short: "Org", icon: "org", cls: "k-org", what: "An Orgtree organization. Its agents read and answer mail sent to this address." };
  if (kind === "chat") return { label: "Claude Code session", short: "Session", icon: "terminal", cls: "k-chat", what: "A single Claude Code session connected to the hub." };
  return { label: "Person", short: "Person", icon: "person", cls: "k-person", what: "A person using Hubchat." };
}

/** contact.org_name || contact.username || the address. */
export const displayName = (c: Contact | undefined, address: string) => (c && (c.org_name || c.username)) || address;

/** Address with its tag dimmed: maya.<tag>e71f2b</tag>. */
export function splitAddr(a: string): [string, string] {
  const i = a.lastIndexOf(".");
  return i > 0 ? [a.slice(0, i + 1), a.slice(i + 1)] : [a, ""];
}

export const hubByUrl = (hubs: HubStatus[], url: string | null | undefined) => hubs.find((h) => h.url === url);
export const hubName = (hubs: HubStatus[], url: string | null | undefined) => hubByUrl(hubs, url)?.name || (url ? url.replace(/^https?:\/\//, "") : "");

/** The hubs of mine that list this contact. */
export const peerHubs = (c: Contact | undefined, hubs: HubStatus[]) => (c ? hubs.filter((h) => c.hubs.includes(h.url)) : []);
/** A connected hub that reaches them (the core sends through one of these). */
export const viaHub = (c: Contact | undefined, hubs: HubStatus[]) => peerHubs(c, hubs).find((h) => h.state === "connected");

export type PresState = "online" | "offline" | "disconnected";
export interface Presence { state: PresState; text: string; short: string }

/** The three definitions, per hub: online = I'm connected to a hub that holds
 *  them and they are connected; offline = I'm connected, they are not;
 *  disconnected = none of their hubs is connected, so status is unknown. */
export function presence(c: Contact | undefined, hubs: HubStatus[]): Presence {
  if (!c) return { state: "disconnected", text: "Status unknown — not in your hubs' directories", short: "status unknown" };
  if (!viaHub(c, hubs)) {
    const names = peerHubs(c, hubs).map((h) => h.name);
    return { state: "disconnected", text: "Status unknown — no connection to " + (names.join(" or ") || "their hub"), short: "status unknown" };
  }
  if (c.online) return { state: "online", text: "Online", short: "online" };
  const seen = ms(c.last_seen);
  return seen
    ? { state: "offline", text: "Offline · last seen " + ago(seen), short: "last seen " + ago(seen) }
    : { state: "offline", text: "Offline", short: "offline" };
}

/** The per-message limit for a peer: the connected hub that reaches them,
 *  else the smallest limit among connected hubs (else among all hubs). */
export function limitFor(c: Contact | undefined, hubs: HubStatus[]): { hub: HubStatus; bytes: number } | null {
  const v = viaHub(c, hubs);
  if (v) return { hub: v, bytes: v.max_attachment_bytes };
  const pool = hubs.filter((h) => h.state === "connected");
  const list = pool.length ? pool : hubs;
  if (!list.length) return null;
  const h = list.reduce((a, b) => (b.max_attachment_bytes < a.max_attachment_bytes ? b : a));
  return { hub: h, bytes: h.max_attachment_bytes };
}

/** When a message happened, for ordering by day and the bubble's time. */
export const msgTime = (m: Message) => (m.outgoing ? ms(m.created_at) : ms(m.received_at) || ms(m.sent_at) || ms(m.created_at));

/** One-line preview, attachment names included. */
export function preview(m: Message | undefined | null): string {
  if (!m) return "";
  const t = plain(m.body);
  // an image goes by "Photo" (design)
  const names = m.attachments.map((a) => (isImage(a.name) ? "Photo" : a.name)).join(", ");
  if (!t) return names;
  return names ? names + " · " + t : t;
}

export type TickCls = "t-queued" | "t-sent" | "t-delivered" | "t-read" | "t-failed" | "";
export function tickInfo(m: Message): { ic: IconName; cls: TickCls; label: string } {
  switch (m.state) {
    case "queued":
    case "sending": return { ic: "schedule", cls: "t-queued", label: "Waiting" };
    case "sent": return { ic: "check", cls: "t-sent", label: "Sent — the hub has it" };
    case "fetched":
    case "delivered": return { ic: "done_all", cls: "t-delivered", label: "Delivered" };
    case "read": return { ic: "done_all", cls: "t-read", label: "Read" };
    case "failed": return { ic: "error", cls: "t-failed", label: "Not sent" + (m.error ? " — " + m.error : "") };
    default: return { ic: "check", cls: "", label: "" };
  }
}

export interface HubSummary { state: "ok" | "partial" | "busy" | "down" | "none"; text: string }
export function hubSummary(hubs: HubStatus[]): HubSummary {
  const n = hubs.length; const c = hubs.filter((h) => h.state === "connected").length;
  if (!n) return { state: "none", text: "No hubs" };
  if (c === n) return { state: "ok", text: n === 1 ? "Connected to " + hubs[0].name : n + " hubs connected" };
  if (c === 0 && hubs.every((h) => h.state === "connecting")) return { state: "busy", text: "Connecting…" };
  if (c === 0) return { state: "down", text: "No hub reachable" };
  return { state: "partial", text: c + " of " + n + " hubs connected" };
}

/** "Connected", "Connecting…", "Can't reach this hub · retrying in 8 s", the refusal. */
export function hubStatusText(h: HubStatus, now: number): string {
  if (h.state === "connected") return "Connected";
  if (h.state === "connecting") return "Connecting…";
  if (h.state === "refused") return "Refused" + (h.error ? ": " + h.error : "");
  const s = h.retry_at_ms ? Math.max(0, Math.ceil((h.retry_at_ms - now) / 1000)) : 0;
  return "Can't reach this hub" + (h.retry_at_ms ? " · retrying in " + s + " s" : "");
}
/** CSS class for .hubst (the prototype's names). */
/** A hub's reported version, or "unknown". */
export const hubVersion = (h: { version?: string | null }): string => (h.version && h.version.trim() ? h.version.trim() : "unknown");

export const hubCls = (h: HubStatus) => (h.state === "disconnected" ? "unreachable" : h.state);

/** Attachment card state, following the prototype's attState. */
export interface AttView { st: "busy" | "bad" | "got" | "remote" | "local" | "expired"; ic: IconName; sub: string; bar?: number; cancel?: boolean; retry?: "send" | "download"; download?: boolean; open?: boolean }
export function attView(a: Attachment, m: Message, prog?: { done: number; total: number }): AttView {
  const pct = prog && prog.total ? Math.floor((100 * prog.done) / prog.total) : 0;
  const size = bytes(a.bytes);
  if (a.state === "expired") return { st: "expired", ic: "cloud_off", sub: "No longer on the hub · " + size };
  if (m.outgoing) {
    switch (a.state) {
      case "uploading": return { st: "busy", ic: "upload", bar: pct, cancel: true, sub: prog ? "Uploading · " + bytes(prog.done) + " of " + bytes(prog.total || a.bytes) : "Uploading · " + size };
      case "pending": return { st: "busy", ic: "upload", bar: 0, cancel: true, sub: "Waiting to upload · " + size };
      case "failed": return { st: "bad", ic: "error", retry: "send", sub: "Upload failed" + (a.error ? " (" + a.error + ")" : "") + " · " + size };
      case "cancelled": return { st: "bad", ic: "close", retry: "send", sub: "Upload cancelled · " + size };
      default: return { st: "local", ic: "file", sub: size };
    }
  }
  switch (a.state) {
    case "downloading": return { st: "busy", ic: "download", bar: pct, cancel: true, sub: prog ? "Downloading · " + bytes(prog.done) + " of " + bytes(prog.total || a.bytes) : "Downloading · " + size };
    case "failed": return { st: "bad", ic: "error", retry: "download", sub: "Download failed" + (a.error ? " (" + a.error + ")" : "") + " · " + size };
    case "cancelled": return { st: "bad", ic: "close", retry: "download", sub: "Download cancelled · " + size };
    case "done": return { st: "got", ic: "file", open: true, sub: size + " · Saved · Open" };
    default: return { st: "remote", ic: "file", download: true, sub: size + " · Download" };
  }
}
