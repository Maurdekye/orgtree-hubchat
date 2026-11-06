// A message the user just sent shows at once, before the core has stored it (coordinator rulings 2026-11-06 m0b072f823ac6, after
// the user's "Before the bubble appears" m8873192c7b34). Pure state: the store injects the core calls, tests inject fakes.
//
// Life of a local row: `sending` (queued styling) -> the core stores the message and a read returns it, and the row goes
// (`settle`), the stored row replacing it in place, same id. A send that fails leaves the row `failed` with Retry (same id, one
// send) and Discard (removes the bubble; the caller puts the text back into an EMPTY composer).
import type { Message, NewOutgoing } from "../api";

export interface Local {
  msg: NewOutgoing;
  phase: "sending" | "failed";
  error: string | null;
  /** when it was sent from the box (the bubble's time) */
  at: string;
  /** the core has the message (send resolved); the row waits for a read to return it */
  sent: boolean;
}

export interface Deps {
  /** crash recovery: the draft is saved before the send, as before */
  saveDraft(peer: string, body: string): Promise<void>;
  send(m: NewOutgoing): Promise<void>;
  /** after the core has it: clear the draft, refresh the chat list */
  sentOk(m: NewOutgoing): Promise<void> | void;
  now(): string;
  changed(): void;
}

const NONE: readonly Local[] = Object.freeze([]);

export class Outgoing {
  private byPeer = new Map<string, Local[]>();
  private runs = new Map<string, number>();
  private d: Deps;
  constructor(d: Deps) { this.d = d; }

  /** The peer's local rows, oldest first; the SAME array until it changes. */
  rows(peer: string): readonly Local[] {
    return this.byPeer.get(peer) ?? NONE;
  }
  /** Is `body` the text of a message this chat still holds locally? Its saved recovery draft (written before the send) is then not
   *  to be restored into the box on a chat return: the message is already sent or held, and the next send would append to it. */
  holdsBody(peer: string, body: string): boolean {
    return this.rows(peer).some((r) => r.msg.body === body);
  }
  isLocal(id: string): boolean {
    for (const rows of this.byPeer.values()) if (rows.some((r) => r.msg.id === id)) return true;
    return false;
  }
  private set(peer: string, rows: Local[]) {
    if (rows.length) this.byPeer.set(peer, rows); else this.byPeer.delete(peer);
    this.d.changed();
  }
  private patch(id: string, fn: (l: Local) => Local) {
    for (const [peer, rows] of this.byPeer) {
      if (rows.some((r) => r.msg.id === id)) { this.set(peer, rows.map((r) => (r.msg.id === id ? fn(r) : r))); return; }
    }
  }

  /** Enter: the row exists, and the UI has been told, before this returns. The core calls follow without being waited for. */
  submit(m: NewOutgoing): void {
    if (this.isLocal(m.id)) return;
    this.set(m.peer, [...this.rows(m.peer), { msg: m, phase: "sending", error: null, at: this.d.now(), sent: false }]);
    void this.run(m);
  }

  private async run(m: NewOutgoing): Promise<void> {
    const token = (this.runs.get(m.id) ?? 0) + 1;
    this.runs.set(m.id, token);
    const live = () => this.runs.get(m.id) === token && this.isLocal(m.id);
    try {
      await this.d.saveDraft(m.peer, m.body);
      if (!live()) return;
      await this.d.send(m);
      if (!live()) return;
      this.patch(m.id, (l) => ({ ...l, sent: true }));
      await this.d.sentOk(m);
    } catch (e) {
      if (!live()) return;
      this.patch(m.id, (l) => ({ ...l, phase: "failed", error: e instanceof Error ? e.message : String(e) }));
    }
  }

  /** Retry: the same message, the same id, one send. */
  retry(id: string): void {
    let found: Local | undefined;
    for (const rows of this.byPeer.values()) found ??= rows.find((r) => r.msg.id === id);
    if (!found || found.phase !== "failed") return;
    this.patch(id, (l) => ({ ...l, phase: "sending", error: null }));
    void this.run(found.msg);
  }

  /** Discard: the bubble goes; the message is returned so the caller can restore it into an empty composer. */
  discard(id: string): NewOutgoing | null {
    for (const [peer, rows] of this.byPeer) {
      const row = rows.find((r) => r.msg.id === id);
      if (!row) continue;
      this.runs.delete(id);
      this.set(peer, rows.filter((r) => r.msg.id !== id));
      return row.msg;
    }
    return null;
  }

  /** A read returned these stored ids: their local rows go (the stored row replaces the local one in place). */
  settle(peer: string, stored: Iterable<string>): void {
    const ids = new Set(stored);
    const rows = this.rows(peer);
    const keep = rows.filter((r) => !(r.sent && ids.has(r.msg.id)));
    if (keep.length !== rows.length) this.set(peer, keep);
  }

  /** The identity changed: nothing local belongs to the new one. */
  clear(): void {
    this.runs.clear();
    if (this.byPeer.size) { this.byPeer.clear(); this.d.changed(); }
  }
}

/** A local row as the Message the bubble draws (state queued, or failed with its error). */
export function toMessage(l: Local): Message {
  const m = l.msg;
  return {
    id: m.id, peer: m.peer, outgoing: true, hub: null, body: m.body, kind: m.kind ?? null,
    reply_to: m.reply_to ?? null, ...(m.replies ? { replies: m.replies } : {}), ...(m.answers ? { answers: m.answers } : {}),
    sent_at: null, received_at: null, created_at: l.at, state: l.phase === "failed" ? "failed" : "queued",
    fetched_at: null, delivered_at: null, read_at: null, error: l.error, seen: true,
    attachments: (m.attachments ?? []).map((a, i) => ({
      local_id: `local:${m.id}:${i}`, hub_id: null, name: a.name, bytes: a.bytes, source: a.source, local_path: null, state: "pending", error: null,
    })),
  };
}

/** The stored messages with the local rows after them; the SAME array when there are none, and a local row whose id is stored is not drawn twice. */
export function withLocal(stored: Message[], locals: readonly Local[]): Message[] {
  if (!locals.length) return stored;
  const have = new Set(stored.map((m) => m.id));
  const extra = locals.filter((l) => !have.has(l.msg.id)).map(toMessage);
  return extra.length ? [...stored, ...extra] : stored;
}
