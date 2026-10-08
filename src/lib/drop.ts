// Files dragged onto the desktop window (design: drop files on the chat).
// Tauri takes the drop natively (dragDropEnabled, its default), so the page
// gets paths rather than File objects: one listener on the webview's drag and
// drop events, fanned out to hooks that test the pointer against an element.
// The browser mock uses the page's own drag events with made-up paths.
import { useEffect, useRef, useState, type RefObject } from "react";
import { isTauri } from "../api";

type DropEv = { type: "over"; x: number; y: number } | { type: "drop"; x: number; y: number; paths: string[] } | { type: "leave" };

const subs = new Set<(e: DropEv) => void>();
const emit = (e: DropEv) => subs.forEach((f) => f(e));
let started = false;

function start() {
  if (started) return;
  started = true;
  if (isTauri) {
    void import("@tauri-apps/api/webview").then(({ getCurrentWebview }) =>
      getCurrentWebview().onDragDropEvent(({ payload: p }) => {
        if (p.type === "leave") { emit({ type: "leave" }); return; }
        // physical pixels in the webview; the page works in CSS pixels
        const k = window.devicePixelRatio || 1;
        const x = p.position.x / k, y = p.position.y / k;
        emit(p.type === "drop" ? { type: "drop", x, y, paths: p.paths } : { type: "over", x, y });
      }));
    return;
  }
  document.addEventListener("dragover", (e) => { e.preventDefault(); emit({ type: "over", x: e.clientX, y: e.clientY }); });
  document.addEventListener("dragleave", (e) => { if (!e.relatedTarget) emit({ type: "leave" }); });
  document.addEventListener("drop", (e) => {
    e.preventDefault();
    const paths = [...(e.dataTransfer?.files || [])].map((f) => "C:\\Users\\alex\\Downloads\\" + f.name);
    emit({ type: "drop", x: e.clientX, y: e.clientY, paths });
  });
}

/** The point is on `el` and nothing covers it there (a modal's scrim). */
function hits(el: HTMLElement | null, x: number, y: number): boolean {
  const at = el ? document.elementFromPoint(x, y) : null;
  return !!at && el!.contains(at);
}

/** While `on`: true as long as files are dragged over `ref`'s element, and
 *  `onDrop` gets the paths of files dropped on it. */
export function useFileDrop(ref: RefObject<HTMLElement | null>, on: boolean, onDrop: (paths: string[]) => void): boolean {
  const [over, setOver] = useState(false);
  const cb = useRef(onDrop);
  cb.current = onDrop;
  useEffect(() => {
    if (!on) return;
    start();
    const f = (e: DropEv) => {
      if (e.type === "leave") { setOver(false); return; }
      const hit = hits(ref.current, e.x, e.y);
      if (e.type === "over") { setOver(hit); return; }
      setOver(false);
      if (hit && e.paths.length) cb.current(e.paths);
    };
    subs.add(f);
    return () => { subs.delete(f); setOver(false); };
  }, [ref, on]);
  return over;
}
