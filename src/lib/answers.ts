// A chat's unsent answers: the messages the next message replies to and the
// options picked on question cards. Send sends picks separately from the
// typed text/files draft (user/coordinator 2026-10-14). They survive a restart
// (user 2026-10-11 22:42Z), so the core
// keeps them beside the chat's draft.
import { useSyncExternalStore } from "react";
import { api, type DraftAnswers, type NewOutgoing } from "../api";

const EMPTY: DraftAnswers = { replies: [], picks: {} };
const cache = new Map<string, DraftAnswers>();
// Which identity's answers the cache holds: a read that started under an
// earlier one is dropped when it lands (forgetDraftAnswers moves this on).
let generation = 0;
const subs = new Set<() => void>();
const changed = () => subs.forEach((f) => f());

export function useDraftAnswers(peer: string): DraftAnswers {
  return useSyncExternalStore((f) => { subs.add(f); return () => { subs.delete(f); }; }, () => cache.get(peer) ?? EMPTY);
}

/** Read a chat's saved answers once; anything changed meanwhile wins, and
 *  a read another identity started is dropped. */
export async function loadDraftAnswers(peer: string): Promise<void> {
  if (cache.has(peer)) return;
  const mine = generation;
  const d = await api.draftAnswers(peer).catch(() => null);
  if (mine !== generation || cache.has(peer) || !d || (!d.replies.length && !Object.keys(d.picks).length)) return;
  cache.set(peer, { replies: d.replies, picks: d.picks }); changed();
}

function save(peer: string, d: DraftAnswers) {
  cache.set(peer, d); changed();
  void api.setDraftAnswers(peer, d).catch(() => {});
}

/** Replace the held targets (explicit reset/Cancel All). */
export function setReplies(peer: string, ids: string[]) {
  const d = cache.get(peer) ?? EMPTY;
  save(peer, { ...d, replies: ids });
}
/** Reply adds targets to the current draft; replying again never duplicates
 *  one or drops another. The composer orders them by the chat timeline. */
export function addReplies(peer: string, ids: string[]) {
  const d = cache.get(peer) ?? EMPTY;
  const replies = [...new Set([...d.replies, ...ids])];
  if (replies.length === d.replies.length) return;
  save(peer, { ...d, replies });
}
export function removeReply(peer: string, id: string) {
  const d = cache.get(peer) ?? EMPTY;
  save(peer, { ...d, replies: d.replies.filter((x) => x !== id) });
}

/** Tap an option: single-choice swaps to it (or drops it when tapped again),
 *  multiple-choice adds or removes it. */
export function pick(peer: string, card: string, label: string, multiple: boolean) {
  const d = cache.get(peer) ?? EMPTY;
  const cur = d.picks[card] ?? [];
  const next = cur.includes(label) ? cur.filter((l) => l !== label) : multiple ? [...cur, label] : [label];
  const picks = { ...d.picks };
  if (next.length) picks[card] = next; else delete picks[card];
  save(peer, { ...d, picks });
}
export function unpick(peer: string, cards: string[]) {
  const d = cache.get(peer) ?? EMPTY;
  if (!cards.some((c) => c in d.picks)) return;
  const picks = { ...d.picks };
  for (const c of cards) delete picks[c];
  save(peer, { ...d, picks });
}

/** The message went: the messages it replied to, the cards it answered among
 *  them, are done. Everything else stays: a reply or pick chosen while it was
 *  being prepared (review R3), and a pick it couldn't carry yet (its card not
 *  known to be open). */
export function sentDraftAnswers(peer: string, m: Pick<NewOutgoing, "reply_to" | "replies" | "answers">) {
  const d = cache.get(peer);
  if (!d) return;
  const went = new Set([...(m.replies ?? []), ...(m.reply_to ? [m.reply_to] : []), ...(m.answers ?? []).map((a) => a.question)]);
  const picks = { ...d.picks };
  for (const c of went) delete picks[c];
  save(peer, { replies: d.replies.filter((id) => !went.has(id)), picks });
}

/** Another identity now holds this device: forget the old one's, including
 *  reads still on their way. */
export function forgetDraftAnswers() {
  generation += 1;
  cache.clear(); changed();
}
