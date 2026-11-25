// Lockdown (Mail hub 2.6.0; Orgtree docket org-mailbox-lockdown-allow-list-mode-that-denies, user 2026-11-25
// 13:26Z: "a little lock icon in the corner of their status while enabled to anyone with access, and to anyone
// without access, their whole status symbol is replaced with a lock, and they cannot be messaged"). The hubs say,
// per contact, `open` (their list counts us) or `closed`; closed on any hub wins (the core merges it). The hub
// refuses a send anyway; these only keep the app honest about it. Pure, so tools/lockdown.test.ts runs it in Node.

type Lockable = { locked?: "open" | "closed" | string | null } | undefined;

export const LOCKED_TEXT = "In lockdown: you can't message them";

/** Closed: the lock in place of their whole status. Null: show their presence as usual. */
export function lockedPresence(c: Lockable): { state: "locked"; text: string; short: string; ticks: false } | null {
  return c?.locked === "closed" ? { state: "locked", text: LOCKED_TEXT, short: "in lockdown", ticks: false } : null;
}

/** Open: their status keeps its dot, with a small lock in the corner. */
export const lockBadge = (c: Lockable) => c?.locked === "open";

/** Closed contacts can't be messaged (no composer, no answer buttons). */
export const canMessage = (c: Lockable) => c?.locked !== "closed";

/** In their DM, in place of the composer: the hub's own refusal words. */
export const lockBarText = (name: string) => `@${name} is in lockdown and only takes messages from addresses on its list`;

/** Settings: our own address is in lockdown on these hubs (set by another client: 1.4.0 has no switch). */
export function ownLockdownText(hubs: string[]): string | null {
  if (!hubs.length) return null;
  const names = hubs.length === 1 ? hubs[0] : hubs.slice(0, -1).join(", ") + " and " + hubs[hubs.length - 1];
  return `Your address is in lockdown on ${names}: only addresses on its list can message you.`;
}

/** A question card's answer buttons: none at all on a closed contact's open question (review I8: its answer would
 *  be refused), while an answered card still shows what was picked. */
export const answerOptionsShown = ({ settled, closed }: { settled: boolean; closed: boolean }) => settled || !closed;
