// Image attachments shown in the chat (user 23:29Z; design att-img): which
// files get a preview, their bytes as object URLs (fetched once per
// attachment, shared by the bubble and the viewer), the open viewer, and
// images pasted into the composer (user 23:40Z).
import { useEffect, useState, useSyncExternalStore } from "react";
import { api, type Attachment, type Message } from "../api";
import { errText } from "./native";
import { toast } from "./toast";

/** Larger images keep the file card. In step with src-tauri/src/media.rs. */
export const PREVIEW_MAX = 20 * 1024 * 1024;
const MIME: Record<string, string> = { png: "image/png", jpg: "image/jpeg", jpeg: "image/jpeg", gif: "image/gif", webp: "image/webp" };
const ext = (name: string) => (name.split(".").pop() || "").toLowerCase();

export const isImage = (name: string) => name.includes(".") && ext(name) in MIME;
/** Shown as a picture in the bubble. */
export const previewable = (a: Attachment) => isImage(a.name) && a.bytes <= PREVIEW_MAX && a.state !== "expired";

const urls = new Map<string, Promise<string>>();
const KEEP = 80;

function previewUrl(m: Message, a: Attachment): Promise<string> {
  const k = m.id + "/" + a.local_id;
  let p = urls.get(k);
  if (p) { urls.delete(k); urls.set(k, p); return p; } // most recently used last
  p = api.attachmentPreview(m.id, a.local_id).then((buf) => URL.createObjectURL(new Blob([buf], { type: MIME[ext(a.name)] })));
  p.catch(() => { if (urls.get(k) === p) urls.delete(k); }); // tried again next time
  urls.set(k, p);
  for (const [old, q] of urls) {
    if (urls.size <= KEEP) break;
    urls.delete(old);
    q.then(URL.revokeObjectURL, () => {});
  }
  return p;
}

/** The preview's URL once loaded; `failed` when there is none (the caller
 *  shows the file card). Tried again when the attachment changes (a
 *  download finishing, a hub coming back). */
export function usePreview(m: Message, a: Attachment): { url: string | null; failed: boolean } {
  const [r, setR] = useState<{ key: string; url: string | null; failed: boolean }>({ key: "", url: null, failed: false });
  const key = m.id + "/" + a.local_id + "/" + a.state + "/" + (a.local_path || "");
  useEffect(() => {
    let live = true;
    previewUrl(m, a).then((url) => { if (live) setR({ key, url, failed: false }); }, () => { if (live) setR({ key, url: null, failed: true }); });
    return () => { live = false; };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [key]);
  return r.key === key ? r : { url: r.url, failed: false };
}

/** Shown as a thumbnail in the composer (user 2026-10-09 07:06Z). */
export const thumbable = (name: string, size: number) => isImage(name) && size <= PREVIEW_MAX;

/** A composer image's thumbnail: its object URL, null while it loads,
 *  "failed" when there is none (the composer shows the file chip). The URL
 *  goes when the attachment does. */
export function useFilePreview(source: string, name: string): string | null | "failed" {
  const [r, setR] = useState<{ key: string; url: string | null | "failed" }>({ key: "", url: null });
  const key = source + "\n" + name;
  useEffect(() => {
    let live = true;
    let made: string | null = null;
    api.filePreview(source, name).then((buf) => {
      made = URL.createObjectURL(new Blob([buf], { type: MIME[ext(name)] }));
      if (live) setR({ key, url: made });
      else URL.revokeObjectURL(made);
    }, () => { if (live) setR({ key, url: "failed" }); });
    return () => { live = false; if (made) URL.revokeObjectURL(made); };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [key]);
  return r.key === key ? r.url : null;
}

// ------------------------------------------------------------ copying

/** Copy image (user 2026-11-25 mee317e7dbd9a): the picture itself on the clipboard, as PNG, the one picture type
 *  every clipboard takes (a JPEG, WebP or GIF is drawn into PNG first; a GIF's first frame). The write starts inside
 *  the menu click, with the picture still on its way, so the click is the user gesture the clipboard asks for. */
export async function copyImage(m: Message, a: Attachment): Promise<void> {
  const png = previewUrl(m, a).then(async (url) => {
    const blob = await (await fetch(url)).blob();
    if (blob.type === "image/png") return blob;
    const bmp = await createImageBitmap(blob);
    const c = document.createElement("canvas");
    c.width = bmp.width; c.height = bmp.height;
    c.getContext("2d")!.drawImage(bmp, 0, 0);
    bmp.close();
    return await new Promise<Blob>((ok, no) => c.toBlob((b) => (b ? ok(b) : no(new Error("the picture could not be converted"))), "image/png"));
  });
  try {
    await navigator.clipboard.write([new ClipboardItem({ "image/png": png })]);
    toast("Image copied");
  } catch (e) {
    png.catch(() => {});
    toast("Couldn't copy the image: " + errText(e));
  }
}

// ------------------------------------------------------------ the viewer

let shown: { m: Message; a: Attachment } | null = null;
const subs = new Set<() => void>();
const notify = () => subs.forEach((f) => f());

export function openImage(m: Message, a: Attachment): void { shown = { m, a }; notify(); }
export function closeImage(): void { if (shown) { shown = null; notify(); } }
export function useShownImage() {
  return useSyncExternalStore((f) => { subs.add(f); return () => { subs.delete(f); }; }, () => shown);
}

// ------------------------------------------------------------ pasting

const two = (n: number) => String(n).padStart(2, "0");

/** pasted-YYYYMMDD-HHMMSS.png, local time; -2, -3... for more at once. */
export function pastedName(type: string, when: Date, i: number): string {
  const e = ({ "image/jpeg": "jpg", "image/gif": "gif", "image/webp": "webp" } as Record<string, string>)[type] || "png";
  const d = when.getFullYear() + two(when.getMonth() + 1) + two(when.getDate()) + "-" + two(when.getHours()) + two(when.getMinutes()) + two(when.getSeconds());
  return "pasted-" + d + (i > 1 ? "-" + i : "") + "." + e;
}

/** Images on the clipboard, unless it also holds text: copying text out of
 *  some apps (Word, say) puts a picture of it there too, and text wins. */
export function pastedImages(dt: DataTransfer | null): File[] {
  if (!dt || dt.types.includes("text/plain")) return [];
  return [...dt.files].filter(isPastedImage);
}

/** A pasted picture (the clipboard names these `image.png`; Hubchat names them by the time, see pastedName). */
export const isPastedImage = (f: File) => f.type in { "image/png": 1, "image/jpeg": 1, "image/gif": 1, "image/webp": 1 };

/** The most a pasted file may weigh: the bytes go through the page into the app. Bigger files are attached with the
 *  picker or dropped on the chat, which hand over a path instead. */
export const MAX_PASTED_BYTES = 64 * 1024 * 1024;

/** Whether a paste event must be completed from the native clipboard: Hubchat on a Linux desktop (WebKitGTK puts no
 *  picture in the paste event, so Ctrl+V of a screenshot brought nothing while right-click Paste, which reads the
 *  clipboard natively, attached it) when the event carries neither text nor files. Windows, macOS, Android and
 *  Hubchat Web keep their webview's own paste. */
export function needsNativePaste(dt: DataTransfer | null, os: string, desktop: boolean, web: boolean): boolean {
  if (os !== "linux" || !desktop || web) return false;
  if (!dt) return true;
  return dt.files.length === 0 && !dt.getData("text/plain");
}

/** Files on the clipboard (an image, or files copied in a file manager), unless it also holds text, which wins as it
 *  does for images. Pictures only used to qualify (symmetry audit ux10 S4). */
export function pastedFiles(dt: DataTransfer | null): File[] {
  if (!dt || dt.types.includes("text/plain")) return [];
  return [...dt.files];
}
