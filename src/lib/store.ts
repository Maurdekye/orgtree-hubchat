// The UI's data layer: one small external store holding what the core reports
// (state, chat list, directory) plus in-memory transfer progress. It loads
// once and re-reads only what an `hc` event says changed; transfer progress
// is kept apart so a progress tick re-renders only the file card it concerns.
import { useEffect, useState, useSyncExternalStore } from "react";
import { api, type ChatSummary, type Contact, type HcEvent, type Message, type State } from "../api";
import { errText } from "./native";

export interface Snap {
  ready: boolean;
  /** Set when the core could not be read at all. */
  error: string | null;
  state: State | null;
  chats: ChatSummary[];
  directory: Contact[];
  /** address -> contact, rebuilt with the directory. */
  byAddr: Map<string, Contact>;
  /** Onboarding stays up until its last step, even after the identity exists. */
  onboarding: boolean;
  /** Drafts typed this session (chat-list "Draft:" previews). */
  drafts: Record<string, string>;
}

let snap: Snap = { ready: false, error: null, state: null, chats: [], directory: [], byAddr: new Map(), onboarding: false, drafts: {} };
const subs = new Set<() => void>();
function set(p: Partial<Snap>) { snap = { ...snap, ...p }; subs.forEach((f) => f()); }
const subscribe = (f: () => void) => { subs.add(f); return () => { subs.delete(f); }; };

export const getSnap = () => snap;
export function useSnap(): Snap { return useSyncExternalStore(subscribe, getSnap); }

// ------------------------------------------------------------------ loads
export async function refreshState(): Promise<void> {
  try { set({ state: await api.state(), error: null }); } catch (e) { set({ error: errText(e) }); }
}
export async function refreshChats(): Promise<void> {
  if (!snap.state?.me) return;
  try { set({ chats: await api.chats() }); } catch (e) { console.warn("hc_chats", e); }
}
export async function refreshDirectory(): Promise<void> {
  if (!snap.state?.me) return;
  try {
    const directory = await api.directory();
    set({ directory, byAddr: new Map(directory.map((c) => [c.address, c])) });
  } catch (e) { console.warn("hc_directory", e); }
}
export async function refreshAll(): Promise<void> {
  await refreshState();
  await Promise.all([refreshChats(), refreshDirectory()]);
}

/** Coalesce bursts of events into one re-read per kind. */
const pending = { state: false, chats: false, dir: false };
let timer: ReturnType<typeof setTimeout> | null = null;
function schedule(k: keyof typeof pending) {
  pending[k] = true;
  if (timer) return;
  timer = setTimeout(() => {
    timer = null;
    const p = { ...pending }; pending.state = pending.chats = pending.dir = false;
    if (p.state) void refreshState();
    if (p.chats) void refreshChats();
    if (p.dir) void refreshDirectory();
  }, 40);
}

// ------------------------------------------------------- per-chat changes
const chatSubs = new Map<string, Set<() => void>>();
export function onChatChange(peer: string, f: () => void): () => void {
  let s = chatSubs.get(peer); if (!s) chatSubs.set(peer, (s = new Set()));
  s.add(f);
  return () => { s!.delete(f); };
}
function chatChanged(peer: string) { chatSubs.get(peer)?.forEach((f) => f()); }

/** A chat's messages, re-read whenever the core says that chat changed. */
export function useMessages(peer: string | null): { msgs: Message[]; loaded: boolean; reload: () => void } {
  const [data, setData] = useState<{ peer: string | null; msgs: Message[]; loaded: boolean }>({ peer: null, msgs: [], loaded: false });
  const [tick, setTick] = useState(0);
  useEffect(() => {
    if (!peer) return;
    let live = true;
    api.chat(peer).then((msgs) => { if (live) setData({ peer, msgs, loaded: true }); }, (e) => console.warn("hc_chat", e));
    return () => { live = false; };
  }, [peer, tick]);
  useEffect(() => (peer ? onChatChange(peer, () => setTick((t) => t + 1)) : undefined), [peer]);
  const same = data.peer === peer;
  return { msgs: same ? data.msgs : [], loaded: same && data.loaded, reload: () => setTick((t) => t + 1) };
}

// -------------------------------------------------------------- transfers
export interface Progress { done: number; total: number; upload: boolean; message_id: string }
const transfers = new Map<string, Progress>();
const tsubs = new Set<() => void>();
export function useTransfer(localId: string): Progress | undefined {
  return useSyncExternalStore((f) => { tsubs.add(f); return () => { tsubs.delete(f); }; }, () => transfers.get(localId));
}

/** A transfer in progress, for the transfers chip / strip. */
export interface Active extends Progress {
  local_id: string;
  /** Unix ms of its last progress event. */
  at: number;
  /** From the message, once read: the file name and the chat. */
  name: string | null;
  peer: string | null;
}
const active = new Map<string, Active>();
let activeList: Active[] = [];
const asubs = new Set<() => void>();
function activeChanged() { activeList = [...active.values()]; asubs.forEach((f) => f()); }
/** Every upload and download in progress. */
export function useActiveTransfers(): Active[] {
  return useSyncExternalStore((f) => { asubs.add(f); return () => { asubs.delete(f); }; }, () => activeList);
}

/** Fill in (or, if the attachment is no longer moving, drop) one transfer from its message. */
async function lookupTransfer(localId: string, messageId: string) {
  let m: Message | null = null;
  try { m = await api.message(messageId); } catch { return; }
  const t = active.get(localId); if (!t) return;
  const a = m?.attachments.find((x) => x.local_id === localId);
  if (!m || !a || (a.state !== "uploading" && a.state !== "downloading")) { active.delete(localId); activeChanged(); return; }
  if (t.name !== a.name || t.peer !== m.peer) { active.set(localId, { ...t, name: a.name, peer: m.peer }); activeChanged(); }
}

function noteTransfer(e: Extract<HcEvent, { type: "transfer" }>) {
  const prev = active.get(e.local_id);
  if (e.total > 0 && e.done >= e.total) {
    if (prev) { active.delete(e.local_id); activeChanged(); }
    return;
  }
  active.set(e.local_id, { done: e.done, total: e.total, upload: e.upload, message_id: e.message_id, local_id: e.local_id, at: Date.now(), name: prev?.name ?? null, peer: prev?.peer ?? null });
  activeChanged();
  if (!prev) void lookupTransfer(e.local_id, e.message_id);
}

/** Cancel an upload or download (it leaves the list at once). */
export async function cancelTransfer(localId: string): Promise<void> {
  if (active.delete(localId)) activeChanged();
  await api.cancelTransfer(localId);
}

// a transfer that stopped (failed, cancelled elsewhere) leaves no final
// progress event: re-check it when its chat changes, and drop silent ones
function recheckTransfers(peer: string) {
  for (const t of active.values()) if (t.peer === peer) void lookupTransfer(t.local_id, t.message_id);
}
setInterval(() => {
  const old = Date.now() - 30000;
  let n = 0;
  for (const t of active.values()) if (t.at < old) { active.delete(t.local_id); n++; }
  if (n) activeChanged();
}, 5000);

// ------------------------------------------------------------------- misc
export function setOnboarding(on: boolean) { set({ onboarding: on }); }
export function noteDraft(peer: string, body: string) {
  if ((snap.drafts[peer] || "") === body) return;
  set({ drafts: { ...snap.drafts, [peer]: body } });
}

function onEvent(e: HcEvent) {
  switch (e.type) {
    case "chat": schedule("chats"); chatChanged(e.peer); recheckTransfers(e.peer); break;
    case "incoming": schedule("chats"); chatChanged(e.peer); break;
    case "hub": schedule("state"); break;
    case "directory": schedule("dir"); break;
    case "transfer":
      transfers.set(e.local_id, { done: e.done, total: e.total, upload: e.upload, message_id: e.message_id });
      tsubs.forEach((f) => f());
      noteTransfer(e);
      break;
  }
}

let started = false;
/** Load everything once and start listening. */
export async function startStore(): Promise<void> {
  if (started) return;
  started = true;
  void api.onEvent(onEvent);
  await refreshState();
  if (snap.state && !snap.state.me) set({ onboarding: true });
  await Promise.all([refreshChats(), refreshDirectory()]);
  set({ ready: true });
}
