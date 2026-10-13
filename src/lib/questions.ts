// Question cards (docket hubchat-question-messages-carry-preset-answers-y):
// a message of kind "question" is a card. With preset options it is
// single-choice or multiple-choice; without, it is free-response. Picking
// options attaches them to the composer; they go out with the next message.
// A card settles once, for good (user 2026-10-11 22:44Z): the first message
// that answers it (a picked option, a reply with text) or skips it (a reply
// with no text). Later replies are ordinary chat and change nothing.
import type { Message } from "../api";
import { replyIds } from "./replies";

/** The most options a card has. The core checks every rule on what
 *  arrives (crates/hubchat-core/src/questions.rs) and keeps a card's options
 *  only if they pass; once it has, their plain-text list is gone from the
 *  body, so this side checks the shape only and is never stricter. */
export const MAX_OPTIONS = 10;

export interface Card {
  /** Empty: a free-response question. */
  options: string[];
  multiple: boolean;
}

export type CardState =
  | { st: "open" }
  | { st: "answered"; by: Message; picked: string[] | null }
  | { st: "skipped"; by: Message };

const okLabel = (l: unknown): l is string => typeof l === "string" && l.length > 0;

/** The card a message is, or null. */
export function cardOf(m: Message): Card | null {
  if (m.kind !== "question") return null;
  const q = m.question;
  if (!q || !Array.isArray(q.options) || q.options.length < 2 || q.options.length > MAX_OPTIONS || !q.options.every(okLabel) || new Set(q.options).size !== q.options.length) return { options: [], multiple: false };
  return { options: q.options, multiple: q.multiple === true };
}

/** The labels a message picked on a card, if they are a valid answer to it. */
export function pickedFor(m: Message, cardId: string, card: Card): string[] | null {
  const a = m.answers?.find((x) => x.question === cardId);
  if (!a || !card.options.length || !Array.isArray(a.picked) || !a.picked.length) return null;
  const labels = [...new Set(a.picked)];
  if (!labels.every((l) => card.options.includes(l))) return null;
  if (!card.multiple && labels.length !== 1) return null;
  return card.options.filter((o) => labels.includes(o));
}

/** What a reply did to one card it answers: the options it picked, or a
 *  skip (no text); `counted` unless an earlier answer had settled the card. */
export interface AnswerNote {
  id: string;
  picked: string[] | null;
  skipped: boolean;
  counted: boolean;
}

/** For each message that replies to cards: what it did to each of them.
 *  `held`: cards known beyond `msgs` (api.cardAnswers), so a quoted card
 *  further back gets its note too. */
export function answerNotes(msgs: Message[], states: Map<string, CardState>, held: Message[] = []): Map<string, AnswerNote[]> {
  const byId = new Map([...held, ...msgs].map((m) => [m.id, m]));
  const out = new Map<string, AnswerNote[]>();
  for (const m of msgs) {
    const notes: AnswerNote[] = [];
    for (const id of replyIds(m)) {
      const c = byId.get(id), card = c ? cardOf(c) : null;
      if (!c || !card || c.outgoing === m.outgoing) continue;
      const st = states.get(id), picked = pickedFor(m, id, card);
      notes.push({ id, picked, skipped: !picked && !m.body.trim(), counted: !!st && st.st !== "open" && st.by.id === m.id });
    }
    if (notes.length) out.set(m.id, notes);
  }
  return out;
}

const cmp = (x: string, y: string) => (x < y ? -1 : x > y ? 1 : 0);
/** Hub order: the hub's receipt time, then the id, the same on every device.
 *  What hasn't reached a hub yet comes after, in the order it was written
 *  (the order this device's outbox sends it in). The core's card_answers
 *  (store_questions.rs) finds a card's first answer in the same order. */
function order(a: Message, b: Message): number {
  if (a.received_at && b.received_at) return cmp(a.received_at, b.received_at) || cmp(a.id, b.id);
  if (a.received_at || b.received_at) return a.received_at ? -1 : 1;
  return cmp(a.created_at, b.created_at) || cmp(a.id, b.id);
}

/** Every card's state in a chat, decided the same way on every device: the
 *  first answer in hub order (other devices' answers included) settles it.
 *  Only the other side answers a card: its sender's own replies don't. */
export function cardStates(msgs: Message[]): Map<string, CardState> {
  const cards = new Map<string, { m: Message; card: Card }>();
  for (const m of msgs) { const card = cardOf(m); if (card) cards.set(m.id, { m, card }); }
  const out = new Map<string, CardState>();
  for (const id of cards.keys()) out.set(id, { st: "open" });
  if (!cards.size) return out;
  const answers = msgs.filter((m) => replyIds(m).some((id) => cards.has(id))).sort(order);
  for (const a of answers) {
    for (const id of replyIds(a)) {
      const c = cards.get(id);
      if (!c || c.m.outgoing === a.outgoing || out.get(id)?.st !== "open") continue;
      const picked = pickedFor(a, id, c.card);
      out.set(id, picked ? { st: "answered", by: a, picked } : a.body.trim() ? { st: "answered", by: a, picked: null } : { st: "skipped", by: a });
    }
  }
  return out;
}

/** A chat's cards as the view knows them: the loaded page together with
 *  what the store found for the cards the view holds (api.cardAnswers: each
 *  card and the first answer to it, however far back). A card or its answer
 *  may lie beyond the page (review R1), so the page alone proves nothing. */
export interface Settlement {
  states: Map<string, CardState>;
  /** Cards the store has answered for: their state is known. */
  known: Set<string>;
}

export function settlement(page: Message[], found: Message[]): Settlement {
  const all = new Map(page.map((m) => [m.id, m]));
  for (const m of found) all.set(m.id, m);
  return { states: cardStates([...all.values()]), known: new Set(found.filter((m) => cardOf(m)).map((m) => m.id)) };
}

/** Open for an answer: the store has answered for the card and nothing
 *  settled it. Until the store answers, a card's state is unknown, and
 *  unknown is not open: no pick on it goes, no empty reply skips it. */
export const isOpen = (s: Settlement, id: string) => s.known.has(id) && s.states.get(id)?.st === "open";

/** Some answer or skip settled the card: whichever the store finds first,
 *  it stays settled. */
export function isSettled(s: Settlement, id: string): boolean {
  const st = s.states.get(id)?.st;
  return !!st && st !== "open";
}

/** Where a card stands for the composer: open, settled, or unknown (the
 *  store hasn't answered for it yet, or couldn't). */
export type CardKnown = "open" | "settled" | "unknown";
export const cardKnown = (s: Settlement, id: string): CardKnown => (isOpen(s, id) ? "open" : isSettled(s, id) ? "settled" : "unknown");
