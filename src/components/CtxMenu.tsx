// The right-click menu (desktop; lib/ctxmenu.ts): at the pointer, kept
// inside the window, in the design's popover style (.pop). It never takes
// the focus, so a text field keeps its caret and selection: the arrow keys,
// Enter and Escape work through a window listener while it is open. A click
// elsewhere, the mouse wheel, resizing or leaving the window closes it; a
// scroll the app makes itself (older history loading, a new message) doesn't.
import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { closeMenu, useOpenMenu, type MenuItem } from "../lib/ctxmenu";
import { Icon } from "../lib/icons";

export function CtxMenu() {
  const m = useOpenMenu();
  const ref = useRef<HTMLDivElement>(null);
  const [pos, setPos] = useState<{ x: number; y: number } | null>(null);
  const [active, setActive] = useState(-1);
  const activeRef = useRef(active);
  activeRef.current = active;

  useLayoutEffect(() => {
    setActive(-1);
    if (!m || !ref.current) { setPos(null); return; }
    const r = ref.current.getBoundingClientRect();
    const x = m.x + r.width > innerWidth - 8 ? Math.max(8, innerWidth - r.width - 8) : m.x;
    // flipped above the pointer near the bottom, but never under the title bar
    const y = m.y + r.height > innerHeight - 8 ? Math.max(40, m.y - r.height) : m.y;
    setPos({ x, y });
  }, [m]);

  useEffect(() => {
    if (!m) return;
    const enabled = m.items.map((e, i) => (e !== "sep" && !e.disabled ? i : -1)).filter((i) => i >= 0);
    const run = (it: MenuItem) => { closeMenu(); it.run(); };
    const down = (e: MouseEvent) => { if (!ref.current?.contains(e.target as Node)) closeMenu(); };
    const key = (e: KeyboardEvent) => {
      const at = enabled.indexOf(activeRef.current);
      if (e.key === "Escape") { e.preventDefault(); e.stopPropagation(); closeMenu(); }
      else if (e.key === "ArrowDown" || e.key === "ArrowUp") {
        e.preventDefault(); e.stopPropagation();
        if (!enabled.length) return;
        const n = at < 0 ? (e.key === "ArrowDown" ? 0 : enabled.length - 1) : (at + (e.key === "ArrowDown" ? 1 : enabled.length - 1)) % enabled.length;
        setActive(enabled[n]);
      } else if (e.key === "Home" || e.key === "End") {
        e.preventDefault(); e.stopPropagation();
        if (enabled.length) setActive(enabled[e.key === "Home" ? 0 : enabled.length - 1]);
      } else if (e.key === "Enter" || e.key === " ") {
        const it = m.items[activeRef.current];
        if (it && it !== "sep" && !it.disabled) { e.preventDefault(); e.stopPropagation(); run(it); }
      } else if (e.key !== "Shift" && e.key !== "Control" && e.key !== "Alt") closeMenu();
    };
    const away = () => closeMenu();
    window.addEventListener("mousedown", down, true);
    window.addEventListener("keydown", key, true);
    window.addEventListener("blur", away);
    window.addEventListener("resize", away);
    window.addEventListener("wheel", away, { capture: true, passive: true });
    return () => {
      window.removeEventListener("mousedown", down, true);
      window.removeEventListener("keydown", key, true);
      window.removeEventListener("blur", away);
      window.removeEventListener("resize", away);
      window.removeEventListener("wheel", away, true);
    };
  }, [m]);

  if (!m) return null;
  return createPortal(
    <div className="pop ctx" role="menu" ref={ref} onContextMenu={(e) => e.preventDefault()}
      style={{ left: pos?.x ?? m.x, top: pos?.y ?? m.y, visibility: pos ? "visible" : "hidden" }}>
      {m.items.map((e, i) => (e === "sep"
        ? <div className="sep" role="separator" key={i} />
        : <button key={i} type="button" role="menuitem" tabIndex={-1} className={"mi" + (e.bad ? " bad" : "") + (i === active ? " on" : "")}
            aria-disabled={e.disabled || undefined} disabled={e.disabled}
            onMouseDown={(ev) => ev.preventDefault()} onMouseEnter={() => setActive(e.disabled ? -1 : i)}
            onClick={() => { if (!e.disabled) { closeMenu(); e.run(); } }}>
            <Icon name={e.icon} />{e.label}
          </button>))}
    </div>,
    document.body,
  );
}
