import type { Message } from "../api";

/** Rows re-read on a routine chat change (receipts and state included). */
export const WINDOW = 200;

/** Skeleton: the real merge lands with the fix. */
export function mergeWindow(prev: Message[], _fresh: Message[], _limit: number = WINDOW): Message[] {
  return prev;
}
