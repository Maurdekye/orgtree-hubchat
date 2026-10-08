// Short confirmations ("Address copied"). Desktop shows .toast, Android .snack.
import { useSyncExternalStore } from "react";

export interface Toast { id: number; text: string }
let list: Toast[] = [];
let seq = 0;
const subs = new Set<() => void>();
const emit = () => subs.forEach((f) => f());

export function toast(text: string): void {
  const t = { id: ++seq, text };
  list = [...list.slice(-2), t];
  emit();
  setTimeout(() => { list = list.filter((x) => x !== t); emit(); }, 2600);
}

export function useToasts(): Toast[] {
  return useSyncExternalStore((f) => { subs.add(f); return () => { subs.delete(f); }; }, () => list);
}
