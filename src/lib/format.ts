// Sizes, times and small text helpers (ported from the prototype's core.js).

const DAYS = ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"];
const MONTHS = ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"];

/** RFC 3339 (or null) to Unix ms; 0 when missing or unparsable. */
export function ms(s: string | null | undefined): number {
  if (!s) return 0;
  const t = Date.parse(s);
  return Number.isFinite(t) ? t : 0;
}

export const pad = (n: number) => String(n).padStart(2, "0");
export const time = (t: number) => { const d = new Date(t); return pad(d.getHours()) + ":" + pad(d.getMinutes()); };
export const timeSec = (t: number) => time(t) + ":" + pad(new Date(t).getSeconds());
export const dayStart = (t: number) => { const d = new Date(t); d.setHours(0, 0, 0, 0); return d.getTime(); };
export const dayDiff = (t: number) => Math.round((dayStart(Date.now()) - dayStart(t)) / 864e5);

export function dayLabel(t: number): string {
  const n = dayDiff(t); const d = new Date(t);
  if (n === 0) return "Today";
  if (n === 1) return "Yesterday";
  if (n > 1 && n < 7) return DAYS[d.getDay()];
  return DAYS[d.getDay()] + ", " + d.getDate() + " " + MONTHS[d.getMonth()] + (d.getFullYear() !== new Date().getFullYear() ? " " + d.getFullYear() : "");
}

/** Chat-list time: 18:09 · Yesterday · Mon · 3 Oct. */
export function shortWhen(t: number): string {
  if (!t) return "";
  const n = dayDiff(t); const d = new Date(t);
  if (n === 0) return time(t);
  if (n === 1) return "Yesterday";
  if (n > 1 && n < 7) return DAYS[d.getDay()].slice(0, 3);
  return d.getDate() + " " + MONTHS[d.getMonth()].slice(0, 3);
}

/** "just now", "12 min ago", "3 h ago", "yesterday at 18:09", "5 days ago". */
export function ago(t: number): string {
  const s = Math.max(0, (Date.now() - t) / 1000);
  if (s < 50) return "just now";
  if (s < 3600) return Math.round(s / 60) + " min ago";
  if (s < 86400 && dayDiff(t) === 0) return Math.round(s / 3600) + " h ago";
  const n = dayDiff(t);
  if (n <= 1) return "yesterday at " + time(t);
  return n + " days ago";
}

/** 24 KB, 1.4 MB, 1 GB. */
export function bytes(n: number): string {
  if (n < 1024) return n + " B";
  if (n < 1048576) return Math.round(n / 1024) + " KB";
  if (n < 1073741824) return (n / 1048576).toFixed(n < 10485760 ? 1 : 0) + " MB";
  return (n / 1073741824).toFixed(1).replace(/\.0$/, "") + " GB";
}

export const num = (n: number) => n.toLocaleString("en-US");

export function initials(name: string): string {
  const w = String(name || "?").replace(/[^\p{L}\p{N} ]/gu, " ").trim().split(/\s+/);
  return ((w[0] || "?")[0] + (w.length > 1 ? w[w.length - 1][0] : "")).toUpperCase();
}

export const utf8Len = (s: string) => new TextEncoder().encode(s).length;

/** Long messages show collapsed with Show more. */
export const LONG = { chars: 1200, lines: 16 };
export const isLong = (body: string) => body.length > LONG.chars || body.split("\n").length > LONG.lines;
export const MAX_FILES = 10;
