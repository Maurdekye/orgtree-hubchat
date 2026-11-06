import type { Message } from "../api";

/** Rows re-read on a routine chat change (receipts and state included), so a
 *  send costs the same however much history is loaded (docket
 *  mail-hub-queueing-outgoing-messages-takes-longer). */
export const WINDOW = 200;

/** `a` comes before `b` in a chat (by time, then id, as the core pages). */
export const earlier = (a: Message, b: Message) => a.created_at < b.created_at || (a.created_at === b.created_at && a.id < b.id);

/** `fresh`: the newest `limit` rows just read. Fewer than `limit` is the
 *  whole chat. Otherwise rows older than the window stay as they are (same
 *  objects, same order) and the window is replaced: new and changed rows in,
 *  deleted ones out. */
export function mergeWindow(prev: Message[], fresh: Message[], limit: number = WINDOW): Message[] {
  if (fresh.length < limit) return fresh;
  const first = fresh[0];
  return [...prev.filter((m) => earlier(m, first)), ...fresh];
}

/** What a re-read of an open chat is: the newest WINDOW rows merged, or the
 *  whole loaded range. `windowed`: asked for by a chat change or a send;
 *  `forceFull`: asked for by a delete (`reload()`), which wins. */
export function readPlan(ask: { windowed: boolean; forceFull: boolean }, loaded: number): "window" | "full" {
  return ask.windowed && !ask.forceFull && loaded > WINDOW ? "window" : "full";
}

/** The open chat takes a full read when the window or app gains focus, once
 *  per gain: not while it stays focused, not on a repeated event. */
export function focusGained(was: boolean, now: boolean): boolean {
  return !was && now;
}
