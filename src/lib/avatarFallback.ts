// Shared by the client Avatar and Windows shortcut rasterization. Colors and
// geometry stay in .av CSS; initials stay in the existing formatter.
import { initials } from "./format";
import { kindInfo, type Kind } from "./peers";

export function avatarFallback(kind: Kind | "me", name: string) {
  return { cls: kind === "me" ? "k-me" : kindInfo(kind).cls, text: initials(name) };
}

export function avatarPng(kind: Kind, name: string, backgroundOnly = false): string {
  const fallback = avatarFallback(kind, name);
  const probe = document.createElement("span");
  probe.className = "av s44 " + fallback.cls;
  probe.style.cssText = "position:fixed;left:-1000px;top:0";
  probe.textContent = fallback.text;
  document.body.append(probe);
  try {
    const style = getComputedStyle(probe);
    const canvas = document.createElement("canvas");
    canvas.width = canvas.height = 128;
    const ctx = canvas.getContext("2d");
    if (!ctx) throw Error("Avatar canvas unavailable");
    // Match the 44px contact-list avatar, rendered at a shell-friendly size.
    ctx.scale(128 / 44, 128 / 44);
    const radius = style.borderTopLeftRadius.endsWith("%")
      ? parseFloat(style.borderTopLeftRadius) * 44 / 100 : parseFloat(style.borderTopLeftRadius);
    ctx.beginPath(); ctx.roundRect(0, 0, 44, 44, radius);
    // Profile transparency sees the actual avatar tint over the contact-list
    // surface, never the unrelated shell/launcher panel behind the icon.
    ctx.fillStyle = getComputedStyle(document.documentElement).getPropertyValue("--side"); ctx.fill();
    ctx.fillStyle = style.backgroundColor; ctx.fill();
    ctx.fillStyle = style.color;
    ctx.font = `${style.fontWeight} ${style.fontSize} ${style.fontFamily}`;
    ctx.textAlign = "center"; ctx.textBaseline = "middle";
    if (!backgroundOnly) ctx.fillText(fallback.text, 22, 22, 40);
    return canvas.toDataURL("image/png");
  } finally { probe.remove(); }
}
