// Pure state helpers for the Linux store-mismatch flow: identifying mismatch
// state, recognizing native mismatch refusals, and holding setup invitations
// while a mismatch choice is unresolved.
import type { State } from "../api";

export interface SetupReq { input: string; n: number }

export const FINISH_IDENTITY_NOTICE = "Finish choosing an identity first";

/** True when no identity is active and the session's key store holds a
 *  different identity than this device's recorded local chats (Linux only). */
export function isMismatchState(state: State | null | undefined): boolean {
  return Boolean(state && !state.me && state.key_lost?.found);
}

/** True when an adopt/restore command was refused by the native mismatch guard. */
export function isMismatchRefusal(err: string, found: string | null | undefined): boolean {
  return Boolean(found) && err.startsWith("This device's chats belong to ");
}

/** Coordinates the active setup overlay with an invitation held while a
 *  store-mismatch choice is pending. */
export class SetupGate {
  private cur: SetupReq | null = null;
  private held: string | null = null;
  private n = 0;

  active(): SetupReq | null { return this.cur; }
  pending(): string | null { return this.held; }

  /** Open `input` immediately unless `mismatch` is active; during mismatch,
   *  keep `input` pending and return false so the caller notifies the user. */
  request(input: string, mismatch: boolean): boolean {
    if (mismatch) {
      this.held = input;
      return false;
    }
    this.held = null;
    this.cur = { input, n: ++this.n };
    return true;
  }

  end(): void {
    this.cur = null;
  }

  /** Reconcile active/held state whenever mismatch or sign-in readiness changes.
   *  Returns true if the active overlay changed. */
  sync(mismatch: boolean, canOpen: boolean): boolean {
    if (mismatch) {
      if (this.cur) {
        this.held = this.cur.input;
        this.cur = null;
        return true;
      }
      return false;
    }
    if (canOpen && !this.cur && this.held) {
      const input = this.held;
      this.held = null;
      this.cur = { input, n: ++this.n };
      return true;
    }
    return false;
  }
}
