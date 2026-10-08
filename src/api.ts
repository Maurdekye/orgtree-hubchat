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
}

export interface Me {
  id: string;
  address: string;
  name: string;
  about: string;
}

export interface State {
  me: Me | null;
  recovery_saved: boolean;
  read_receipts: boolean;
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
  | { result: "connected"; url: string; name: string; max_attachment_bytes: number; features: string[] }
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

export type HcEvent =
  | { type: "chat"; peer: string }
  | { type: "incoming"; peer: string; id: string; preview: string }
  | { type: "hub"; url: string }
  | { type: "directory" }
  | { type: "transfer"; local_id: string; message_id: string; upload: boolean; done: number; total: number };

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

  probeHub: (input: string) => invoke<Probe>("hc_probe_hub", { input }),
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
