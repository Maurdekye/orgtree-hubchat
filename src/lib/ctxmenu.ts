// Hubchat's own right-click menu on desktop (user 2026-10-09 05:54Z). The
// WebView's menu never shows: a message, a chat row, a Directory row, a
// link, a picture or a file opens its own menu (MessageView, ChatList,
// Directory); a text field gets Cut, Copy, Paste and Select all; selected
// text elsewhere gets Copy; anything else gets no menu at all.
import { useSyncExternalStore } from "react";
import { api } from "../api";
import type { IconName } from "./icons";
import { copyText } from "./native";
import { toast } from "./toast";

export interface MenuItem {
  label: string;
  icon: IconName;
  run: () => void;
  disabled?: boolean;
  /** Shown in red (destructive). */
  bad?: boolean;
}
export type MenuEntry = MenuItem | "sep";
export interface OpenMenu { x: number; y: number; items: MenuEntry[] }

let open: OpenMenu | null = null;
const subs = new Set<() => void>();
const emit = () => subs.forEach((f) => f());

/** Opens the menu at the pointer; separators at the ends or doubled are dropped. */
export function openMenu(x: number, y: number, items: MenuEntry[]): void {
  const clean: MenuEntry[] = [];
  for (const e of items) {
    if (e === "sep" && (!clean.length || clean[clean.length - 1] === "sep")) continue;
    clean.push(e);
  }
  while (clean[clean.length - 1] === "sep") clean.pop();
  open = clean.length ? { x, y, items: clean } : null;
  emit();
}

export function closeMenu(): void {
  if (open) { open = null; emit(); }
}

export function useOpenMenu(): OpenMenu | null {
  return useSyncExternalStore((f) => { subs.add(f); return () => { subs.delete(f); }; }, () => open);
}

/** The text selected inside `el`, if any. */
export function selectionIn(el: Element): string {
  const s = window.getSelection();
  if (!s || s.isCollapsed || !s.rangeCount) return "";
  const r = s.getRangeAt(0);
  return el.contains(r.commonAncestorContainer) ? s.toString() : "";
}

type Field = HTMLInputElement | HTMLTextAreaElement;
const TEXT_TYPES = new Set(["text", "search", "url", "email", "password", "tel", "number"]);
const isField = (el: Element | null): el is Field =>
  el instanceof HTMLTextAreaElement || (el instanceof HTMLInputElement && TEXT_TYPES.has(el.type));

/** Paste where the caret was: text if the clipboard has any (as Ctrl+V
 *  does); otherwise a picture goes where Ctrl+V would put it (the composer
 *  attaches it). Read natively: the WebView would ask the user first. */
async function pasteInto(f: Field, start: number, end: number): Promise<void> {
  const text = await api.clipboardText().catch(() => null);
  f.focus();
  try { f.setSelectionRange(start, end); } catch { /* number fields have no selection */ }
  if (text) { document.execCommand("insertText", false, text); return; }
  const png = await api.clipboardImage().catch(() => null);
  if (!png) { toast("Nothing to paste"); return; }
  const dt = new DataTransfer();
  dt.items.add(new File([png], "image.png", { type: "image/png" }));
  f.dispatchEvent(new ClipboardEvent("paste", { clipboardData: dt, bubbles: true, cancelable: true }));
}

/** Cut, Copy, Paste and Select all for a text field. */
export function fieldMenu(f: Field): MenuEntry[] {
  let start = 0, end = 0;
  try { start = f.selectionStart ?? 0; end = f.selectionEnd ?? 0; } catch { /* number fields */ }
  const has = end > start;
  const locked = f.readOnly || f.disabled;
  const secret = f.type === "password";
  // the menu takes no focus, but put the caret back in case something did
  const back = () => { f.focus(); try { f.setSelectionRange(start, end); } catch { /* number fields */ } };
  return [
    { label: "Cut", icon: "cut", disabled: !has || locked || secret, run: () => { back(); document.execCommand("cut"); } },
    { label: "Copy", icon: "copy", disabled: !has || secret, run: () => { back(); document.execCommand("copy"); } },
    { label: "Paste", icon: "paste", disabled: locked, run: () => void pasteInto(f, start, end) },
    "sep",
    { label: "Select all", icon: "select_all", disabled: !f.value, run: () => { f.focus(); f.select(); } },
  ];
}

/** The page-wide contextmenu listener (desktop): runs after a component
 *  that opened its own menu has called preventDefault. */
export function onContextMenu(e: MouseEvent): void {
  if (e.defaultPrevented) return;
  e.preventDefault();
  const t = e.target instanceof Element ? e.target : null;
  const f = t?.closest("textarea, input") ?? null;
  if (isField(f)) { openMenu(e.clientX, e.clientY, fieldMenu(f)); return; }
  const sel = window.getSelection()?.toString() ?? "";
  if (sel.trim()) { openMenu(e.clientX, e.clientY, [{ label: "Copy", icon: "copy", run: () => void copyText(sel, "Copied") }]); return; }
  closeMenu();
}
