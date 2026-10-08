// Dark (default) / Light / Match system, kept in localStorage and applied to
// <html> as the prototype does (class theme-*, plus a data-theme attribute).
import { useSyncExternalStore } from "react";

export type ThemePref = "dark" | "light" | "system";
const KEY = "hubchat.theme";
const subs = new Set<() => void>();

export function themePref(): ThemePref {
  const v = localStorage.getItem(KEY);
  return v === "light" || v === "system" ? v : "dark";
}

const mq = typeof window !== "undefined" && window.matchMedia ? window.matchMedia("(prefers-color-scheme: light)") : null;

export function applyTheme(): void {
  const p = themePref();
  const eff = p === "system" ? (mq && mq.matches ? "light" : "dark") : p;
  const html = document.documentElement;
  html.classList.remove("theme-dark", "theme-light");
  html.classList.add("theme-" + eff);
  html.dataset.theme = eff;
}

export function setThemePref(p: ThemePref): void {
  localStorage.setItem(KEY, p);
  applyTheme();
  subs.forEach((f) => f());
}

mq?.addEventListener("change", () => { if (themePref() === "system") applyTheme(); });

export function useThemePref(): ThemePref {
  return useSyncExternalStore((f) => { subs.add(f); return () => { subs.delete(f); }; }, themePref);
}
