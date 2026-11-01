// lib/translation.ts — translation between people (Mail hub 2.4.0, feature `translate`; Orgtree docket
// mail-hub-hubchat-automatic-translation-between-u; design hubchat-translation-design-1-3-0). The person picks the
// language they read in (Settings › Profile); a hub that translates sends each message from someone writing in
// another language with its translation for them, which the bubble shows with the original a click away.
import { useSyncExternalStore } from "react";
import type { HubStatus, Message } from "../api";

/** The languages a person can read in, each named in its own language: the same list as Orgtree's Hosting tab. */
export const LANGUAGES: readonly (readonly [tag: string, name: string])[] = [
  ["ar", "العربية"], ["bn", "বাংলা"], ["cs", "Čeština"], ["da", "Dansk"], ["de", "Deutsch"],
  ["el", "Ελληνικά"], ["en", "English"], ["es", "Español"], ["fa", "فارسی"], ["fi", "Suomi"],
  ["fr", "Français"], ["he", "עברית"], ["hi", "हिन्दी"], ["hu", "Magyar"], ["id", "Bahasa Indonesia"],
  ["it", "Italiano"], ["ja", "日本語"], ["ko", "한국어"], ["nl", "Nederlands"], ["no", "Norsk"],
  ["pl", "Polski"], ["pt", "Português"], ["pt-BR", "Português (Brasil)"], ["ro", "Română"], ["ru", "Русский"],
  ["sv", "Svenska"], ["th", "ไทย"], ["tr", "Türkçe"], ["uk", "Українська"], ["vi", "Tiếng Việt"],
  ["zh-Hans", "中文（简体）"], ["zh-Hant", "中文（繁體）"],
];

/** A tag's own name, or the tag itself when it is not in the list. */
export const languageName = (tag: string): string => LANGUAGES.find(([t]) => t === tag)?.[1] ?? tag;

/** A tag's English name ("Translated from Hebrew"): Hubchat's words are English. */
export function languageInEnglish(tag: string): string {
  try { return new Intl.DisplayNames(["en"], { type: "language" }).of(tag) ?? tag; } catch { return tag; }
}

/** A message's translation for us as text, when the hub translated it (never on our own messages). */
export function translatedText(m: Message): string | null {
  const t = m.outgoing ? null : m.translation;
  return t?.status === "translated" && t.body ? t.body : null;
}

/** Why a message to us was not translated, as its bubble says it; null when nothing is to be said (none was needed,
 *  or it is on its way). */
export function untranslatedReason(m: Message): string | null {
  switch (m.outgoing ? null : m.translation?.status) {
    case "too_long": return "Not translated: too long";
    case "limit": return "Not translated: the hub's monthly translation limit is reached";
    case "failed": return "Couldn't translate";
    default: return null;
  }
}

// which translated messages show their original now (this window only, like Show more)
const originals = new Set<string>();
const listeners = new Set<() => void>();
let version = 0;
const subscribe = (f: () => void) => { listeners.add(f); return () => { listeners.delete(f); }; };
export function toggleOriginal(id: string) {
  if (!originals.delete(id)) originals.add(id);
  version++;
  listeners.forEach((f) => f());
}
export const showingOriginal = (id: string) => originals.has(id);
export function useShowingOriginal(id: string): boolean {
  useSyncExternalStore(subscribe, () => version);
  return originals.has(id);
}

/** The text a message shows now (what Copy copies): its translation for us, unless its original is showing. */
export function shownText(m: Message): string {
  const t = translatedText(m);
  return t !== null && !originals.has(m.id) ? t : m.body;
}

/** The Profile row's caption and whether the row can be used, from the connected hubs. */
export function translationCaption(hubs: HubStatus[]): { text: string; disabled: boolean } {
  const connected = hubs.filter((h) => h.state === "connected");
  if (!connected.length) return { text: "Your hubs are told when they connect.", disabled: false };
  const can = connected.filter((h) => h.features.includes("translate"));
  const cant = connected.filter((h) => !h.features.includes("translate")).map((h) => h.name);
  const cantText = cant.length ? cant.join(", ") + " can't translate yet" : "";
  if (!can.length) return { text: cantText + ".", disabled: true };
  const text = can.some((h) => h.translation && h.translation !== "off")
    ? "Translated by your hub using Claude. Messages to you leave the hub to be translated."
    : can.every((h) => h.translation === "off")
      ? "Your hub doesn't translate."
      : "Translated by your hub when its owner turns translation on. Messages to you then leave the hub to be translated.";
  return { text: cantText ? text + " · " + cantText : text, disabled: false };
}
