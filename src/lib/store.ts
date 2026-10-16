// The UI's data layer: one small external store holding what the core reports
// (state, chat list, directory) plus in-memory transfer progress. It loads
// once and re-reads only what an `hc` event says changed; transfer progress
// is kept apart so a progress tick re-renders only the file card it concerns.
import { useCallback, useEffect, useRef, useState, useSyncExternalStore } from "react";
import { forgetDraftAnswers } from "./answers";
import { api, type ChatSummary, type Contact, type HcEvent, type Message, type NewOutgoing, type OlderPage, type SendRoute, type State, type TransferView } from "../api";
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
  /** The chat list has loaded once for this identity (`chats` is real). */
  chatsReady: boolean;
}

let snap: Snap = { ready: false, error: null, state: null, chats: [], directory: [], byAddr: new Map(), onboarding: false, drafts: {}, chatsReady: false };
const subs = new Set<() => void>();
function set(p: Partial<Snap>) { snap = { ...snap, ...p }; subs.forEach((f) => f()); }
const subscribe = (f: () => void) => { subs.add(f); return () => { subs.delete(f); }; };

export const getSnap = () => snap;
/** Every change, outside React (lib/tailnet.ts watches the hubs). */
export const subscribeSnap = subscribe;
export function useSnap(): Snap { return useSyncExternalStore(subscribe, getSnap); }

// ------------------------------------------------------------------ loads
export async function refreshState(): Promise<void> {
  try { set({ state: await api.state(), error: null }); } catch (e) { set({ error: errText(e) }); }
}
export async function refreshChats(): Promise<void> {
  if (!snap.state?.me) return;
  try { set({ chats: await api.chats(), chatsReady: true }); } catch (e) { console.warn("hc_chats", e); }
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
  await Promise.all([refreshChats(), refreshDirectory(), refreshTransfers()]);
}

/** Coalesce bursts of events into one re-read per kind. */
const pending = { state: false, chats: false, dir: false, transfers: false };
let timer: ReturnType<typeof setTimeout> | null = null;
function schedule(k: keyof typeof pending) {
  pending[k] = true;
  if (timer) return;
  timer = setTimeout(() => {
    timer = null;
    const p = { ...pending }; pending.state = pending.chats = pending.dir = pending.transfers = false;
    if (p.state) void refreshState();
    if (p.chats) void refreshChats();
    if (p.dir) void refreshDirectory();
    if (p.transfers) void refreshTransfers();
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

/** Where a chat's messages go (the hub picker), re-read when the chat, the
 *  hubs or the directory change. */
export function useSendRoute(peer: string): SendRoute | null {
  const snap = useSnap();
  const [r, setR] = useState<{ peer: string; route: SendRoute } | null>(null);
  const [tick, setTick] = useState(0);
  useEffect(() => onChatChange(peer, () => setTick((t) => t + 1)), [peer]);
  useEffect(() => {
    let live = true;
    api.sendRoute(peer).then((route) => { if (live) setR({ peer, route }); }, (e) => console.warn("hc_send_route", e));
    return () => { live = false; };
  }, [peer, tick, snap.state, snap.directory]);
  return r?.peer === peer ? r.route : null;
}

/** History comes a page at a time (lazy history, user 23:46Z). */
export const PAGE = 50;
/** `a` comes before `b` in a chat (by time, then id, as the core pages). */
const earlier = (a: Message, b: Message) => a.created_at < b.created_at || (a.created_at === b.created_at && a.id < b.id);

interface Paged { peer: string | null; msgs: Message[]; loaded: boolean; more: boolean; unreachable: string[] }
// Before a chat's messages load, every render must hand out the SAME empty
// lists: a new [] each time changes every memo and effect keyed on them, and
// an effect that then sets state re-renders forever. After a tap or Back that
// loop runs as microtasks, so the load that would end it never arrives and the
// page freezes (Android Back from Contact info, hubchat-opus 2026-10-16).
const NO_MESSAGES: Message[] = [];
const NO_HUBS: string[] = [];

/** A chat's messages: the newest `page` first, older pages on request
 *  (`loadOlder`), and on every change the core reports a re-read of the
 *  range already shown plus anything new. */
export function useMessages(peer: string | null, page = PAGE) {
  const [data, setData] = useState<Paged>({ peer: null, msgs: [], loaded: false, more: false, unreachable: [] });
  const [tick, setTick] = useState(0);
  const [loadingOlder, setLoadingOlder] = useState(false);
  const cur = useRef(data);
  cur.current = data;
  const busy = useRef(false);
  useEffect(() => {
    if (!peer) return;
    let live = true;
    const d = cur.current;
    const oldest = d.peer === peer && d.msgs.length ? d.msgs[0] : null;
    (oldest ? api.chat(peer, { from: oldest, limit: 5000 }) : api.chat(peer, { limit: page })).then((msgs) => {
      if (!live) return;
      setData((prev) => {
        // fewer than a page: the hubs may still hold older ones (lazy history)
        if (!oldest || prev.peer !== peer) return { peer, msgs, loaded: true, more: true, unreachable: [] };
        // an older page that arrived meanwhile stays
        return { ...prev, msgs: [...prev.msgs.filter((m) => earlier(m, oldest)), ...msgs], loaded: true };
      });
    }, (e) => console.warn("hc_chat", e));
    return () => { live = false; };
  }, [peer, tick, page]);
  useEffect(() => (peer ? onChatChange(peer, () => setTick((t) => t + 1)) : undefined), [peer]);
  /** The next older page (before `from`, else before the oldest shown);
   *  resolves to its messages (none: the start). Past what this device
   *  holds, it asks the hubs for their next older page (lazy history). */
  const loadOlder = useCallback(async (from?: Message): Promise<Message[]> => {
    const d = cur.current;
    // a hub that couldn't be reached may be back: worth asking again
    if (!peer || d.peer !== peer || (!from && !d.more && !d.unreachable.length) || busy.current) return [];
    busy.current = true;
    setLoadingOlder(true);
    try {
      const before = from ?? d.msgs[0] ?? null;
      let older = await api.chat(peer, { before, limit: page });
      let hubs: OlderPage | null = null;
      if (older.length < page) {
        hubs = await api.loadOlder(peer);
        if (hubs.added || hubs.more.length) older = await api.chat(peer, { before, limit: page });
      }
      setData((prev) => (prev.peer !== peer ? prev : {
        ...prev,
        msgs: [...older.filter((m) => !prev.msgs.some((x) => x.id === m.id)), ...prev.msgs],
        more: older.length >= page || !!hubs?.more.length,
        unreachable: hubs ? hubs.unreachable : prev.unreachable,
      }));
      return older;
    } catch (e) {
      console.warn("hc_chat", e);
      return [];
    } finally {
      busy.current = false;
      setLoadingOlder(false);
    }
  }, [peer, page]);
  const same = data.peer === peer;
  return {
    msgs: same ? data.msgs : NO_MESSAGES,
    loaded: same && data.loaded,
    reload: () => setTick((t) => t + 1),
    /** Older messages exist beyond what is shown. */
    more: same && data.more,
    /** Hubs whose older messages aren't loaded because they can't be reached. */
    unreachable: same ? data.unreachable : NO_HUBS,
    /** A hub that couldn't be reached is back: go on loading from it. */
    retryOlder: () => setData((prev) => (prev.peer === peer && prev.unreachable.length ? { ...prev, more: true } : prev)),
    loadingOlder,
    loadOlder,
  };
}

// -------------------------------------------------------------- transfers
export interface Progress { done: number; total: number; upload: boolean; message_id: string }
const transfers = new Map<string, Progress>();
const tsubs = new Set<() => void>();
export function useTransfer(localId: string): Progress | undefined {
  return useSyncExternalStore((f) => { tsubs.add(f); return () => { tsubs.delete(f); }; }, () => transfers.get(localId));
}

/** Durable unfinished transfers, enriched by checkpoint/wire events. */
export interface Active extends Progress {
  created_at: string;
  local_id: string;
  name: string | null;
  peer: string | null;
  state: string;
  error: string | null;
}
const active = new Map<string, Active>();
let activeList: Active[] = [];
const asubs = new Set<() => void>();
function activeChanged() { activeList = [...active.values()].sort((a, b) => a.created_at.localeCompare(b.created_at) || a.message_id.localeCompare(b.message_id) || a.local_id.localeCompare(b.local_id)); asubs.forEach((f) => f()); }
export function useActiveTransfers(): Active[] {
  return useSyncExternalStore((f) => { asubs.add(f); return () => { asubs.delete(f); }; }, () => activeList);
}
let loadingTransfers = false;
let transfersDirty = false;
/** Reconstruct after startup or state changes; network ticks never refetch chats. */
export async function refreshTransfers(): Promise<void> {
  if (loadingTransfers) { transfersDirty = true; return; }
  loadingTransfers = true;
  try {
    do {
      transfersDirty = false;
      const snapshot = new Map<string, TransferView>();
      let after: string | null = null;
      do {
        const page = await api.transfers(after);
        for (const row of page.rows) snapshot.set(row.local_id, row);
        if (page.next && page.next === after) throw Error("Transfer page did not advance");
        after = page.next;
      } while (after);
      active.clear();
      for (const row of snapshot.values()) {
        const live = transfers.get(row.local_id);
        const moving = row.state === "running" || row.state === "uploading";
        const done = moving && live?.upload === row.upload ? live.done : row.done;
        if (!moving) transfers.delete(row.local_id);
        active.set(row.local_id, {...row, done});
      }
      for (const id of transfers.keys()) if (!snapshot.has(id)) transfers.delete(id);
      activeChanged(); tsubs.forEach(f => f());
    } while (transfersDirty);
  } catch (e) { console.warn("hc_transfers", e); }
  finally { loadingTransfers = false; }
}
function noteTransfer(e: Extract<HcEvent, { type: "transfer" }>) {
  const prev = active.get(e.local_id);
  if (prev) {
    active.set(e.local_id, {...prev, done:e.done, total:e.total});
    activeChanged();
  } else schedule("transfers");
}
/** State, not a timeout or a 100% byte count, decides when a row disappears. */
export async function cancelTransfer(localId: string): Promise<void> {
  await api.cancelTransfer(localId);
  await refreshTransfers();
}

// Local preparation survives navigation between chats. No message is admitted
// until the core has safely copied all files; progress is not a send receipt.
export interface Preparation {
  message: NewOutgoing;
  /** Answer-only sends leave this separate text/files draft in the composer. */
  retainedDraft?: Pick<NewOutgoing, "body" | "attachments">;
  phase: "preparing" | "queued" | "failed";
  done: number;
  total: number;
  name: string;
  error: string | null;
}
const preparations = new Map<string, Preparation>();
// Which identity the preparations belong to: reloadAfterSwitch moves this on.
let generation = 0;
const psubs = new Set<() => void>();
const preparationChanged = () => psubs.forEach((f) => f());
export const getPreparation = (peer: string) => preparations.get(peer) ?? null;
export const hasPreparingMessages = () => [...preparations.values()].some(p => p.phase === "preparing");
export function usePreparation(peer: string) {
  return useSyncExternalStore((f) => { psubs.add(f); return () => { psubs.delete(f); }; }, () => preparations.get(peer) ?? null);
}
export function clearPreparation(peer: string) {
  if (preparations.get(peer)?.phase === "preparing") return;
  preparations.delete(peer); preparationChanged();
}
export async function prepareSend(message: NewOutgoing, retainedDraft?: Preparation["retainedDraft"]): Promise<void> {
  if (preparations.get(message.peer)?.phase === "preparing") return;
  const p: Preparation = { message, retainedDraft, phase: "preparing", done: 0, total: (message.attachments ?? []).reduce((n, a) => n + a.bytes, 0), name: message.attachments?.[0]?.name ?? "", error: null };
  preparations.set(message.peer, p); preparationChanged();
  // A command completed after an identity switch must not touch the new UI:
  // after every wait, go on only while the identity that sent this is still
  // here and the chat's slot still holds this message. A switch empties the
  // slots, and the next identity may start its own send there (review R4).
  const gen = generation;
  const mine = () => gen === generation && preparations.get(message.peer)?.message.id === message.id;
  try {
    await api.setDraft(message.peer, retainedDraft?.body ?? message.body);
    if (!mine()) return;
    await api.send(message);
    if (!mine()) return;
    if (!retainedDraft) {
      noteDraft(message.peer, "");
      await api.setDraft(message.peer, "").catch(() => {});
      if (!mine()) return;
    }
    preparations.set(message.peer, { ...p, phase: "queued" });
    void refreshChats();
  } catch (e) {
    if (!mine()) return;
    preparations.set(message.peer, { ...p, phase: "failed", error: errText(e) });
  }
  preparationChanged();
}

// ------------------------------------------------------------------- misc
export function setOnboarding(on: boolean) { set({ onboarding: on }); }

/** This device now holds another identity (a link switched it): drop what
 *  belonged to the old one and load the new one, like a fresh sign-in. */
export async function reloadAfterSwitch(): Promise<void> {
  generation += 1;
  preparations.clear(); preparationChanged();
  forgetDraftAnswers();
  transfers.clear(); tsubs.forEach((f) => f());
  active.clear(); activeChanged();
  typing.clear(); typingChanged();
  set({ chats: [], directory: [], byAddr: new Map(), drafts: {}, onboarding: false, chatsReady: false });
  await refreshAll();
}
export function noteDraft(peer: string, body: string) {
  if ((snap.drafts[peer] || "") === body) return;
  set({ drafts: { ...snap.drafts, [peer]: body } });
}

/** Who is writing to us now (hub feature `typing`): typing heartbeats,
 *  about two a second while they write (user 2026-10-13 22:34Z). Each
 *  `typing` event is one; a peer shows as typing while its last one came
 *  within `TYPING_WINDOW_MS` by this device's own clock, so the row ends by
 *  itself a second after they stop (a writer that crashed included), and
 *  at once when their message comes. Shown in the chat and the chat list;
 *  never a notification. */
const typing = new Map<string, number>(); // peer -> when its last heartbeat came
const typingSubs = new Set<() => void>();
let typingSnap: ReadonlySet<string> = new Set();
let typingTimer: ReturnType<typeof setTimeout> | undefined;
export const TYPING_WINDOW_MS = 1000;
function typingChanged() {
  const t = performance.now();
  for (const [p, at] of typing) if (t - at >= TYPING_WINDOW_MS) typing.delete(p);
  if (typing.size !== typingSnap.size || [...typing.keys()].some((p) => !typingSnap.has(p))) {
    typingSnap = new Set(typing.keys());
    typingSubs.forEach((f) => f());
  }
  if (typingTimer) clearTimeout(typingTimer);
  typingTimer = undefined;
  if (!typing.size) return;
  const next = Math.min(...typing.values()) + TYPING_WINDOW_MS;
  typingTimer = setTimeout(typingChanged, Math.max(0, next - performance.now()) + 5);
}
function noteTyping(peer: string) {
  typing.set(peer, performance.now());
  typingChanged();
}
/** Their message came: the row goes at once. */
function typingEnded(peer: string) {
  if (typing.delete(peer)) typingChanged();
}
// a hidden window's timers may be held back: look again when it shows, so
// a row whose second is up never lingers there
document.addEventListener("visibilitychange", () => { if (typing.size) typingChanged(); });
const subscribeTyping = (f: () => void) => { typingSubs.add(f); return () => { typingSubs.delete(f); }; };
/** Whether `peer` is writing to us now. */
export function useTyping(peer: string | null | undefined): boolean {
  return useSyncExternalStore(subscribeTyping, () => !!peer && typingSnap.has(peer));
}
/** Everyone writing to us now. */
export function useTypingPeers(): ReadonlySet<string> {
  return useSyncExternalStore(subscribeTyping, () => typingSnap);
}

function onEvent(e: HcEvent) {
  switch (e.type) {
    case "preparing": {
      const p = preparations.get(e.peer);
      if (p?.phase === "preparing" && p.message.id === e.message_id) {
        preparations.set(e.peer, { ...p, done: e.done, total: e.total, name: e.name });
        preparationChanged();
      }
      break;
    }
    case "chat": schedule("chats"); chatChanged(e.peer); schedule("transfers"); break;
    case "incoming": typingEnded(e.peer); schedule("chats"); chatChanged(e.peer); break;
    case "file_lifecycle": schedule("state"); schedule("transfers"); break;
    case "hub": schedule("state"); schedule("transfers"); break;
    case "directory": schedule("dir"); schedule("state"); break;
    case "attached": schedule("state"); schedule("chats"); schedule("dir"); schedule("transfers"); break;
    case "typing": noteTyping(e.peer); break;
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
  // Listen before the first read: a change reported after that read would
  // be lost for good, since a steady state (a hub that stays connected)
  // reports nothing again.
  try { await api.onEvent(onEvent); } catch (e) { console.warn("hc events", e); }
  await refreshState();
  if (snap.state && !snap.state.me) set({ onboarding: true });
  await Promise.all([refreshChats(), refreshDirectory(), refreshTransfers()]);
  set({ ready: true });
}
