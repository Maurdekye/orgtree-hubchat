// "Last seen" per kind (Mail hub 2.6.0, feature `last_active`; docket
// hubchat-last-seen-time-for-orgs-people-and-indep, user 2026-11-24 11:44Z):
// an org, the last time an agent in charge of its org mailbox finished a
// turn, "now" while one is mid-turn; a person, the last time they had
// Hubchat in front; an agent session, the last time it sent a message to
// anyone. No imports at run time, so tools/last-seen.test.ts runs it as is.

/** The words for "now" (user 2026-11-24 ma46b9293f2d5 chose "active now";
 *  this is the one place). */
export const ACTIVE_NOW = "active now";

/** Hubs without `last_active` show no time at all, never the old last-call
 *  time, which isn't when a person last visited (user 2026-11-24
 *  ma46b9293f2d5; flip here to show it again while offline). */
export const OLD_HUBS_SHOW_LAST_CALL = false;

export interface SeenInput {
  /** a connected hub that lists them has the feature `last_active` */
  known: boolean;
  online: boolean;
  /** active now on a hub of theirs */
  active: boolean;
  /** their last active time in ms on the clock of the hub that said it, 0 for none */
  activeAt: number;
  /** that hub's clock minus this device's, ms (0 when unknown): the time is
   *  worded against the hub's clock, so a device whose clock is off doesn't
   *  shift it (review H4) */
  hubOffset: number;
  /** their last call to a hub in ms (the old `last_seen`), 0 for none */
  lastCall: number;
}

/** "active now", "last seen 5 min ago", or null (no time to show); `ticks`:
 *  it holds a time that moves on. `ago` words a past time. */
export function lastSeenWords(i: SeenInput, ago: (t: number) => string): { words: string; ticks: boolean } | null {
  if (i.known) {
    if (i.active && i.online) return { words: ACTIVE_NOW, ticks: false };
    return i.activeAt ? { words: "last seen " + ago(i.activeAt - i.hubOffset), ticks: true } : null;
  }
  const t = OLD_HUBS_SHOW_LAST_CALL && !i.online ? i.lastCall : 0;
  return t ? { words: "last seen " + ago(t), ticks: true } : null;
}

/** The note under a message waiting for someone offline: the time in
 *  brackets when there is one, nothing otherwise (never "offline (offline)",
 *  review H1). */
export function offlineNote(name: string, seen: { words: string } | null): string {
  return name + " is offline" + (seen ? " (" + seen.words + ")" : "");
}

/** A presence's words with the last seen after them: "Online · active now"
 *  (`text`), "online · active now" (`short`); offline, the last seen alone
 *  (short) or "Offline" with nothing to say. */
export function withSeen(word: string, offline: boolean, seen: { words: string } | null): { text: string; short: string } {
  if (!seen) return { text: word, short: word.toLowerCase() };
  return { text: word + " · " + seen.words, short: offline ? seen.words : word.toLowerCase() + " · " + seen.words };
}
