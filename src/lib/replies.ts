// Multi-reply (user 2026-10-11 13:18Z): one message may answer several. Its
// reply_to stays the first, so everything that knows a single reply keeps
// working; `replies` lists them all, in timeline order, only when there are
// two or more.
import type { Message } from "../api";

/** Every message this one answers, oldest first (empty: not a reply). */
export function replyIds(m: Pick<Message, "reply_to" | "replies">): string[] {
  if (m.replies && m.replies.length > 1) return m.replies;
  return m.reply_to ? [m.reply_to] : [];
}

/** How many quotes (or reply chips) show before the rest fold into "and N
 *  more": up to three show whole, a longer list shows its first two. */
export function foldAt(n: number): number {
  return n > 3 ? 2 : n;
}
