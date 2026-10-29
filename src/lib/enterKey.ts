// The desktop composer's Enter key, as Orgtree's "Enter key" display setting: Enter sends (Shift+Enter makes a new
// line), or Enter makes a new line and Ctrl/Command+Enter sends (symmetry audit ux10 S3). Kept in localStorage like the
// theme; a phone's keyboard has its own Send button and no such setting.
import { useSyncExternalStore } from "react";

export type EnterKey = "send" | "newline";
const KEY = "hubchat.enter-key";
const subs = new Set<() => void>();

export function enterKey(): EnterKey {
  return localStorage.getItem(KEY) === "newline" ? "newline" : "send";
}

export function setEnterKey(v: EnterKey): void {
  localStorage.setItem(KEY, v);
  subs.forEach((f) => f());
}

export function useEnterKey(): EnterKey {
  return useSyncExternalStore((f) => { subs.add(f); return () => { subs.delete(f); }; }, enterKey);
}

/** Does this key press send the message? Enter sends, or with the new-line choice Ctrl/Command+Enter does; an IME
 *  composition's Enter and Shift+Enter never send. */
export function sendsOnEnter(mode: EnterKey, e: { key: string; shiftKey: boolean; ctrlKey: boolean; metaKey: boolean; isComposing: boolean }): boolean {
  if (e.key !== "Enter" || e.isComposing) return false;
  return mode === "newline" ? e.ctrlKey || e.metaKey : !e.shiftKey;
}
