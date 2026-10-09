// The desktop window's own title bar (user 18:23Z: frameless, like Orgtree).
// The bar is a native drag region: dragging moves the window with Windows
// snap, double-click maximises/restores. Close hides to the tray (the Rust
// side turns CloseRequested into hide), exactly like the old frame's X.
import { useEffect, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { Logo } from "../lib/icons";

const inTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

const MIN = "M6 19h12v2H6z";
const MAX = "M4 4h16v16H4V4zm2 4v10h12V8H6z";
const RESTORE = "M8 4h12v12h-2V6H8V4zM4 8h12v12H4V8zm2 4v6h8v-6H6z";
const CLOSE = "M19 6.41L17.59 5 12 10.59 6.41 5 5 6.41 10.59 12 5 17.59 6.41 19 12 13.41 17.59 19 19 17.59 13.41 12z";

const Ic = ({ d }: { d: string }) => (
  <svg viewBox="0 0 24 24" width="15" height="15" aria-hidden="true"><path d={d} fill="currentColor" /></svg>
);

export function TitleBar() {
  const [maxed, setMaxed] = useState(false);
  useEffect(() => {
    if (!inTauri) return;
    const w = getCurrentWindow();
    let alive = true;
    const read = () => void w.isMaximized().then((m) => { if (alive) setMaxed(m); }, () => {});
    read();
    const un = w.onResized(read);
    return () => { alive = false; void un.then((f) => f()); };
  }, []);
  const w = inTauri ? getCurrentWindow() : null;
  return (
    <div className="titlebar" role="banner" data-tauri-drag-region>
      <div className="tb-brand" data-tauri-drag-region>
        <Logo size={16} />
        <span role="heading" aria-level={1} data-tauri-drag-region>Hubchat</span>
      </div>
      <div className="tb-fill" data-tauri-drag-region />
      <div className="window-controls" role="group" aria-label="Window controls">
        <button type="button" className="window-control" aria-label="Minimize window" title="Minimize"
          onClick={() => void w?.minimize()}><Ic d={MIN} /></button>
        <button type="button" className="window-control" aria-label={maxed ? "Restore window" : "Maximize window"} title={maxed ? "Restore" : "Maximize"}
          onClick={() => void w?.toggleMaximize()}><Ic d={maxed ? RESTORE : MAX} /></button>
        <button type="button" className="window-control close" aria-label="Close window" title="Close (Hubchat keeps running in the tray)"
          onClick={() => void w?.close()}><Ic d={CLOSE} /></button>
      </div>
    </div>
  );
}
