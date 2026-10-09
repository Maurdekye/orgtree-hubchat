// The UI's whole interface to the Rust core (src-tauri/src/commands.rs).
// The core owns hubs, identity, messages and transfers; the UI renders what
// these calls return and re-reads when an `hc` event says something changed.
//
// Outside Tauri (a plain browser during `npm run dev`) main.tsx installs the
// in-memory mock from src/lib/mock.ts over `api` before the first render, so
// the UI can be developed and screenshotted without the Rust core.
import { invoke } from "@tauri-apps/api/core";
import { listen, UnlistenFn } from "@tauri-apps/api/event";

export type HubState = "connecting" | "connected" | "refused" | "disconnected";

export interface HubStatus {
  url: string;
  name: string;
  state: HubState;
  error: string | null;
  /** Unix ms of the next reconnect while disconnected. */
  retry_at_ms: number | null;
  max_attachment_bytes: number;
  features: string[];
  /** The hub's software version, as it reports it (absent or null: unknown). */
  version?: string | null;
}

export interface Me {
  id: string;
  address: string;
  name: string;
  about: string;
}

/** Settings › Notifications. */
export interface NotifySettings {
  /** Notify for chats that aren't on screen. */
  enabled: boolean;
  /** Show the message text (off: "New message"). */
  preview: boolean;
  /** Play a sound (desktop; Android follows its own channel settings). */
  sound: boolean;
}

export interface State {
  me: Me | null;
  recovery_saved: boolean;
  read_receipts: boolean;
  notifications: NotifySettings;
  /** Android (design D6): stay connected (true) or check about every 15 minutes; null on desktop. */
  stay_connected: boolean | null;
  hubs: HubStatus[];
  platform: "desktop" | "android";
}

/** Outgoing: queued | sending | sent | fetched | delivered | read | failed. Incoming: received. */
export type MessageState =
  | "queued" | "sending" | "sent" | "fetched" | "delivered" | "read" | "failed" | "received";

/** pending | uploading | uploaded | failed | cancelled (outgoing);
 *  remote | downloading | done | failed | cancelled | expired (incoming). */
export interface Attachment {
  local_id: string;
  hub_id: string | null;
  name: string;
  bytes: number;
  source: string | null;
  local_path: string | null;
  state: string;
  error: string | null;
}

export interface Message {
  id: string;
  peer: string;
  outgoing: boolean;
  hub: string | null;
  body: string;
  kind: string | null;
  reply_to: string | null;
  sent_at: string | null;
  received_at: string | null;
  created_at: string;
  state: MessageState;
  fetched_at: string | null;
  delivered_at: string | null;
  read_at: string | null;
  error: string | null;
  seen: boolean;
  attachments: Attachment[];
}

export interface ChatSummary {
  peer: string;
  last: Message;
  unread: number;
}

/** A directory row: one address merged across every hub that lists it. */
export interface Contact {
  address: string;
  /** org | chat | person ("" if unknown) */
  kind: string;
  org_name: string;
  username: string;
  blurb: string;
  online: boolean;
  last_seen: string | null;
  hubs: string[];
}

export type Probe =
  | { result: "connected"; url: string; name: string; max_attachment_bytes: number; features: string[]; version?: string | null }
  | { result: "unreachable"; url: string; error: string }
  | { result: "not_a_hub"; url: string; error: string }
  | { result: "invalid"; error: string };

export interface Resolved {
  exact: Contact | null;
  matches: Contact[];
  is_me: boolean;
  valid: boolean;
  address: string;
}

export interface NewAttachment {
  name: string;
  bytes: number;
  /** A path, or on Android a content:// URI from the picker. */
  source: string;
}

export interface NewOutgoing {
  id: string;
  peer: string;
  body: string;
  kind?: string | null;
  reply_to?: string | null;
  attachments?: NewAttachment[];
}

/** A device using this identity, as a mail hub v2.0 lists it. */
export interface DeviceEntry {
  device_id: string;
  name: string | null;
  created_at: string | null;
  last_seen: string | null;
  online: boolean;
}

export interface Devices {
  this_device: string;
  devices: DeviceEntry[];
}

export type HcEvent =
  | { type: "chat"; peer: string }
  | { type: "incoming"; peer: string; id: string; preview: string }
  | { type: "hub"; url: string }
  | { type: "directory" }
  | { type: "transfer"; local_id: string; message_id: string; upload: boolean; done: number; total: number };

/** What hc_link_start (new device) and hc_link_offer (signed-in device)
 *  return: the one-time code, its QR text and the hub it goes through. */
export interface LinkStart {
  code: string; qr: string;
  /** The hub as this device knows it. */
  hub: string;
  /** What a link offer's QR names it: as other devices may reach it (localhost is no address for a phone). */
  hubs?: string[];
}

/** What a typed code, a hubchat://link URL or an older hubchat-link: text holds.
 *  `role`: "give" (a signed-in device offers its identity: the scanner joins),
 *  "take" (a new device asks for one: a signed-in scanner approves), or null
 *  (a typed code; the older hubchat-link: text counts as "take"). */
export type LinkRole = "give" | "take";
export interface ParsedLink {
  code: string;
  /** The likeliest address of the link's hub. */
  hub: string | null;
  /** Every address the link names for its hub, the likeliest first. */
  hubs: string[];
  /** The hub's name, to check the right hub answered. */
  hub_name: string | null;
  role: LinkRole | null;
}

/** One hub an arriving identity brings, as this device reaches it (user 20:38Z). */
export interface HubRow {
  /** The hub as the other device knows it (may be localhost-style). */
  theirs: string;
  /** Best address found on this device: one that answered, else the first candidate. */
  address: string;
  /** Addresses tried, likeliest first. */
  candidates: string[];
  /** The hub name that answered at `address` (null: nothing answered). */
  name: string | null;
  /** Why `address` didn't answer (null when reachable). */
  error: string | null;
  reachable: boolean;
}

/** Progress of a link started with hc_link_start (Tauri event "hc-link"). */
export type LinkEvent =
  | { state: "waiting"; expires_in_s: number }
  | { state: "failed"; error: string }
  | { state: "expired" }
  /** A signed-in device joined a link for the identity it already has: nothing changes. */
  | { state: "same"; address: string }
  /** An identity arrived: held in memory, nothing saved, until hc_link_confirm
   *  (adopt it with the hubs the user kept) or hc_link_discard (drop it).
   *  `from`: this device's identity now (confirming switches from it), or
   *  null on a fresh device; `to`, `name`: what arrived; `hubs`: the hubs
   *  that come with it, one row each. */
  | { state: "review"; from: string | null; to: string; name: string; hubs: HubRow[] };

/** Signed-in device: the waiting device a code names. */
export interface LinkLookup {
  code: string;
  address: string;
  /** null: not in this device's directory yet (rosters refresh about once a minute). */
  device_name: string | null;
  hubs: string[];
  /** A scanned QR named a hub this device doesn't use. */
  unknown_hub: string | null;
}

/** The real commands. Names and argument shapes match commands.rs. */
export const tauriApi = {
  state: () => invoke<State>("hc_state"),
  uiState: (foreground: boolean, chat: string | null) => invoke<void>("hc_ui_state", { foreground, chat }),

  checkId: (id: string) => invoke<{ ok: boolean; error: string | null; max_len: number }>("hc_check_id", { id }),
  createIdentity: (id: string, name: string) => invoke<string>("hc_create_identity", { id, name }),
  restoreWords: (words: string) => invoke<string>("hc_restore_words", { words }),
  recoveryWords: () => invoke<string[]>("hc_recovery_words"),
  recoverySaved: () => invoke<void>("hc_recovery_saved"),
  setProfile: (name: string, about: string) => invoke<void>("hc_set_profile", { name, about }),
  setReadReceipts: (on: boolean) => invoke<void>("hc_set_read_receipts", { on }),
  setNotifications: (n: NotifySettings) => invoke<void>("hc_set_notifications", { enabled: n.enabled, preview: n.preview, sound: n.sound }),
  setStayConnected: (on: boolean) => invoke<void>("hc_set_stay_connected", { on }),

  probeHub: (input: string) => invoke<Probe>("hc_probe_hub", { input }),
  /** A link's hub under each address it names: the likeliest that answers as `name`. */
  probeLinkHubs: (hubs: string[], name: string | null) => invoke<Probe>("hc_probe_link_hubs", { hubs, name }),
  addHub: (input: string) => invoke<string>("hc_add_hub", { input }),
  removeHub: (url: string, unregister: boolean) => invoke<void>("hc_remove_hub", { url, unregister }),
  retryNow: () => invoke<void>("hc_retry_now"),

  directory: () => invoke<Contact[]>("hc_directory"),
  resolve: (input: string) => invoke<Resolved>("hc_resolve", { input }),
  chats: () => invoke<ChatSummary[]>("hc_chats"),
  chat: (peer: string, before?: string, limit?: number) => invoke<Message[]>("hc_chat", { peer, before, limit }),
  message: (id: string) => invoke<Message | null>("hc_message", { id }),
  send: (msg: NewOutgoing) => invoke<void>("hc_send", { msg }),
  retry: (id: string) => invoke<void>("hc_retry", { id }),
  cancelTransfer: (localId: string) => invoke<void>("hc_cancel_transfer", { localId }),
  download: (messageId: string, localId: string) => invoke<string>("hc_download", { messageId, localId }),
  markRead: (peer: string) => invoke<void>("hc_mark_read", { peer }),
  deleteMessage: (id: string) => invoke<void>("hc_delete_message", { id }),
  deleteChat: (peer: string) => invoke<void>("hc_delete_chat", { peer }),
  draft: (peer: string) => invoke<string | null>("hc_draft", { peer }),
  setDraft: (peer: string, body: string) => invoke<void>("hc_set_draft", { peer, body }),
  fileInfo: (source: string) => invoke<{ name: string; bytes: number }>("hc_file_info", { source }),
  /** An image attachment's bytes for its preview (fetched from the hub once
   *  if this device hasn't got it; never into Downloads). */
  attachmentPreview: (messageId: string, localId: string) => invoke<ArrayBuffer>("hc_attachment_preview", { messageId, localId }),
  /** A pasted image saved as a file the composer can attach; its path. */
  savePasted: (name: string, data: Uint8Array) => invoke<string>("hc_save_pasted", data, { headers: { "x-name": name } }),
  devices: () => invoke<Devices>("hc_devices"),
  saveRecovery: (dest: string | null) => invoke<string>("hc_save_recovery", { dest }),

  // linking a device (src-tauri/src/link.rs)
  /** Join a link (any device). With `code` it joins the code another device shows
   *  (hc_link_offer) instead of making its own; either way "hc-link" events follow,
   *  ending in "review" (or "same": a signed-in device that already is that identity).
   *  `aliases`: the other addresses the link named for `hub` (not kept as extra hubs). */
  linkStart: (hub: string, deviceName: string, code?: string | null, aliases?: string[] | null) => invoke<LinkStart>("hc_link_start", { hub, deviceName, code: code ?? null, aliases: aliases ?? null }),
  /** Signed-in device: a one-time code (and QR) for a new device to scan or type. */
  linkOffer: (hub?: string | null) => invoke<LinkStart>("hc_link_offer", { hub: hub ?? null }),
  parseLink: (input: string) => invoke<ParsedLink>("hc_parse_link", { input }),
  linkCancel: () => invoke<void>("hc_link_cancel"),
  /** After a "review" event: adopt the identity that arrived with exactly these
   *  hub addresses, in this order. On a signed-in device it first leaves `from`
   *  (signed out on v2 hubs; key, chats, hubs and settings forgotten): that is
   *  the switch. Returns the new address. */
  linkConfirm: (hubs: string[]) => invoke<string>("hc_link_confirm", { hubs }),
  /** After a "review" event: drop what arrived; nothing changes. */
  linkDiscard: () => invoke<void>("hc_link_discard"),
  linkLookup: (input: string) => invoke<LinkLookup>("hc_link_lookup", { input }),
  linkApprove: (code: string) => invoke<string>("hc_link_approve", { code }),
  keyQr: () => invoke<string>("hc_key_qr"),
  restoreQr: (text: string) => invoke<string>("hc_restore_qr", { text }),
  keyFileExport: (passphrase: string, dest: string) => invoke<void>("hc_key_file_export", { passphrase, dest }),
  keyFileImport: (source: string, passphrase: string) => invoke<string>("hc_key_file_import", { source, passphrase }),
  onLink: (f: (e: LinkEvent) => void): Promise<UnlistenFn> => listen<LinkEvent>("hc-link", (e) => f(e.payload)),

  /** Android: the chat a tapped notification named, taken once. */
  takePendingChat: () => invoke<string | null>("hc_take_pending_chat"),
  /** The hubchat:// link the system opened the app with, taken once. */
  takePendingLink: () => invoke<string | null>("hc_take_pending_link"),
  /** Desktop: a hubchat:// link arrived while Hubchat runs (take it). */
  onPendingLink: (f: () => void): Promise<UnlistenFn> => listen("hc-link-pending", () => f()),
  /** Open a downloaded attachment (or, desktop, show it in its folder). */
  openAttachment: (messageId: string, localId: string, reveal: boolean) => invoke<void>("hc_open_attachment", { messageId, localId, reveal }),

  onEvent: (f: (e: HcEvent) => void): Promise<UnlistenFn> => listen<HcEvent>("hc", (e) => f(e.payload)),
};

export type Api = typeof tauriApi;

/** What the UI calls: the real commands, or the mock when not in Tauri. */
export const api: Api = { ...tauriApi };

/** Replace the implementation (the browser mock). Call before rendering. */
export function installApi(impl: Api): void {
  Object.assign(api, impl);
}

/** True inside the Tauri webview (desktop or Android). */
export const isTauri: boolean = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

/** A fresh client-side message id (the hub dedupes retries by it). */
export function newId(): string {
  return crypto.randomUUID().replace(/-/g, "");
}
