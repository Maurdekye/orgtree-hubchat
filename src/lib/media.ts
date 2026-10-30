// Inline video and audio (Hubchat 1.2.7; user 2026-10-30 14:24Z "an inline video player for hubchat would be nice",
// 14:33Z "Yes, add audio"), as Orgtree's media card plays them (renderer canvas/media.tsx after its 4.5.3 fix): which
// attachments get a player, its notes, and Escape leaving a fullscreen player. The player (components/AttMedia.tsx)
// reads its file through the hcmedia scheme (api.mediaUrl, src-tauri/src/media.rs), a range at a time from this
// device's copy or the hub: nothing is downloaded or saved.
import type { Attachment } from "../api";

/** Larger files keep only their file row, with the note. In step with hubchat-core media.rs and Orgtree's card. */
export const MEDIA_MAX_BYTES = 256 * 1024 * 1024;
const formats: Record<string, [kind: "video" | "audio", mime: string]> = {
  mp4: ["video", "video/mp4"], webm: ["video", "video/webm"],
  mp3: ["audio", "audio/mpeg"], wav: ["audio", "audio/wav"], ogg: ["audio", "audio/ogg"],
  m4a: ["audio", "audio/mp4"], aac: ["audio", "audio/aac"], flac: ["audio", "audio/flac"],
};
export const mediaFormat = (name: string) => (name.includes(".") ? formats[(name.split(".").pop() || "").toLowerCase()] : undefined);
/** Shown with a player (past the ceiling, with its note instead) above its file row. */
export const playable = (a: Attachment) => !!mediaFormat(a.name) && a.state !== "expired";

// Orgtree's words, as its card shows them
export const NOT_SUPPORTED = "This format is not supported here. Download the file to open it in another app.";
export const UNAVAILABLE = "Preview unavailable. Download the file to open it in another app.";
export const TOO_BIG = "Playback is limited to 256 MiB. Download this file to open it.";

// Escape leaves a fullscreen player and does nothing else: the chat, a prepared reply and an open panel stay.
// Registered when this module loads, before any component's listener, so it runs first. A WebView that leaves
// fullscreen on Escape by itself may still pass the key on: one that comes just after is held back too.
let leftAt = -Infinity, wasMedia = false;
if (typeof window !== "undefined") {
  document.addEventListener("fullscreenchange", () => {
    const media = document.fullscreenElement instanceof HTMLMediaElement;
    if (wasMedia && !media) leftAt = performance.now();
    wasMedia = media;
  });
  window.addEventListener("keydown", (e) => {
    if (e.key !== "Escape") return;
    const media = document.fullscreenElement instanceof HTMLMediaElement;
    if (!media && performance.now() - leftAt > 300) return;
    e.preventDefault(); e.stopImmediatePropagation();
    if (media) void document.exitFullscreen().catch(() => {});
  }, true);
}
