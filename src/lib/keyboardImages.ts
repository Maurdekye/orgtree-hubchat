// Pictures from the Android keyboard (Gboard's GIFs and stickers, a picture
// on its clipboard; user 2026-10-09): KeyboardImages.kt hands each one to the
// page, and it goes into the message box exactly as a pasted picture does
// (the composer's paste handler). Anywhere else a toast says where it can go.
import { toast } from "./toast";

declare global {
  interface Window { __hubchatKeyboardImage?: (b64: string, type: string) => void }
}

const TYPES = ["image/png", "image/jpeg", "image/gif", "image/webp"];

export function installKeyboardImages(): void {
  window.__hubchatKeyboardImage = (b64, type) => {
    const box = document.activeElement;
    if (!(box instanceof HTMLTextAreaElement) || !box.closest(".composer")) { toast("Pictures go in the message box."); return; }
    if (!TYPES.includes(type)) { toast("Hubchat can't attach that kind of picture."); return; }
    const bin = atob(b64);
    const bytes = new Uint8Array(bin.length);
    for (let i = 0; i < bin.length; i++) bytes[i] = bin.charCodeAt(i);
    const files = new DataTransfer();
    files.items.add(new File([bytes], "keyboard", { type }));
    box.dispatchEvent(new ClipboardEvent("paste", { clipboardData: files, bubbles: true, cancelable: true }));
  };
}
