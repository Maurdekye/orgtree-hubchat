import { useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { isTauri } from "../api";
import { iconHTML, TASK_ICONS } from "./icons";
import appIcon from "../../design/hubchat-icon.svg?raw";

export function taskIconPng(view: keyof typeof TASK_ICONS): string {
  const probe = document.createElement("button");
  // Preserve the current primary glyph on a permanently dark app-icon tile.
  probe.className = "icon-btn theme-dark";
  probe.style.cssText = "position:fixed;left:-1000px;top:0";
  probe.innerHTML = iconHTML(TASK_ICONS[view]);
  document.body.append(probe);
  try {
    const canvas = document.createElement("canvas");
    canvas.width = canvas.height = 128;
    const ctx = canvas.getContext("2d");
    if (!ctx) throw Error("Icon canvas unavailable");
    ctx.scale(128 / 24, 128 / 24);
    // Consume the app icon's own background colour and rounding, not a new tile.
    const tile = new DOMParser().parseFromString(appIcon, "image/svg+xml").querySelector("rect")!;
    const sourceWidth = Number(tile.getAttribute("width"));
    ctx.fillStyle = tile.getAttribute("fill")!;
    ctx.beginPath(); ctx.roundRect(0, 0, 24, 24, Number(tile.getAttribute("rx")) * 24 / sourceWidth); ctx.fill();
    ctx.translate(4, 4); ctx.scale(2 / 3, 2 / 3);
    ctx.fillStyle = getComputedStyle(probe).color;
    // Consume the exact path used by Icon/iconHTML; never redraw its geometry.
    ctx.fill(new Path2D(probe.querySelector("path")!.getAttribute("d")!));
    return canvas.toDataURL("image/png");
  } finally { probe.remove(); }
}

export function useShortcutTaskIcons(desktop: boolean) {
  useEffect(() => {
    if (!desktop || !isTauri || !navigator.userAgent.includes("Windows")) return;
    let live = true;
    const sync = async () => {
      for (const view of Object.keys(TASK_ICONS) as (keyof typeof TASK_ICONS)[]) {
        if (!live) return;
        try { await invoke("hc_shortcut_task_icon", { view, png: taskIconPng(view) }); }
        catch { /* Native availability can change during shutdown. */ }
      }
    };
    void sync();
    const observer = new MutationObserver(() => { void sync(); });
    observer.observe(document.documentElement, { attributes: true, attributeFilter: ["class", "data-theme"] });
    return () => { live = false; observer.disconnect(); };
  }, [desktop]);
}
