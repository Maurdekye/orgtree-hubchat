// Small shared pieces: platform context, avatar, address, chips, dialogs.
import { createContext, useContext, useEffect, useId, useRef, useState, type CSSProperties, type KeyboardEvent as ReactKeyEvent, type ReactNode } from "react";
import QRCode from "qrcode";
import type { Contact, HubStatus, Message } from "../api";
import { Icon, type IconName } from "../lib/icons";
import { initials } from "../lib/format";
import { displayName, kindInfo, kindOf, presence, splitAddr, tickInfo, type Kind, type PresState } from "../lib/peers";
import { useToasts } from "../lib/toast";
import { appVersion } from "../lib/native";

export type Platform = "desktop" | "android";
export const PlatformCtx = createContext<Platform>("desktop");
export const usePlatform = () => useContext(PlatformCtx);

export function Avatar({ kind, name, size, pres, presTitle }: { kind: Kind | "me"; name: string; size: number; pres?: PresState; presTitle?: string }) {
  const cls = kind === "me" ? "k-me" : kindInfo(kind).cls;
  return (
    <span className={"av s" + size + " " + cls}>
      {kind === "chat" ? <Icon name="terminal" /> : initials(name)}
      {pres ? <i className={"pres p-" + pres} title={presTitle} /> : null}
    </span>
  );
}

/** Avatar for an address, with presence from the directory. */
export function PeerAvatar({ address, c, hubs, size, withPres = true }: { address: string; c: Contact | undefined; hubs: HubStatus[]; size: number; withPres?: boolean }) {
  const p = presence(c, hubs);
  return <Avatar kind={kindOf(c)} name={displayName(c, address)} size={size} pres={withPres ? p.state : undefined} presTitle={p.text} />;
}

/** maya.<span class="tg">e71f2b</span> */
export function Addr({ a, net }: { a: string; net?: boolean }) {
  const [head, tag] = splitAddr(a);
  return <>{net ? "@net:" : ""}{head}{tag ? <span className="tg">{tag}</span> : null}</>;
}

export function PresText({ c, hubs }: { c: Contact | undefined; hubs: HubStatus[] }) {
  const p = presence(c, hubs);
  useSeenTick(p.state === "offline");
  return <span className={"ptext p-" + p.state}>{p.state === "disconnected" ? <Icon name="cloud_off" /> : null}{p.text}</span>;
}

export function KindChip({ kind, short }: { kind: Kind; short?: boolean }) {
  const k = kindInfo(kind);
  return <span className={"chip " + k.cls}><Icon name={k.icon} />{short ? k.short : k.label}</span>;
}

/** The small org / session glyph after a name (people get none). */
export function KindGlyph({ kind }: { kind: Kind }) {
  if (kind === "person") return null;
  const k = kindInfo(kind);
  return <span className={"crow-kind " + k.cls} title={k.label}><Icon name={k.icon} /></span>;
}

export function Tick({ m }: { m: Message }) {
  const t = tickInfo(m);
  return <span className={"tick " + t.cls} title={t.label} role="img" aria-label={t.label}><Icon name={t.ic} /></span>;
}

export function Switch({ on, onChange, label }: { on: boolean; onChange: (v: boolean) => void; label: string }) {
  return <button className={"switch" + (on ? " on" : "")} role="switch" aria-checked={on} aria-label={label} onClick={() => onChange(!on)} />;
}

/** A QR code of `text` (an address), drawn by the qrcode package. */
export function QR({ text, size }: { text: string; size: number }) {
  const [src, setSrc] = useState("");
  useEffect(() => {
    let live = true;
    QRCode.toDataURL(text, { margin: 1, width: size * 2, color: { dark: "#17191d", light: "#ffffff" } }).then((u) => { if (live) setSrc(u); }, () => setSrc(""));
    return () => { live = false; };
  }, [text, size]);
  return <span className="qrbox">{src ? <img src={src} width={size} height={size} alt={"QR code for " + text} /> : <span style={{ display: "block", width: size, height: size }} />}</span>;
}

/** onKeyDown for a div that acts as a button: Enter and Space press it (not
 *  when the key came from a control inside it). */
export const pressKeys = (fn: () => void) => (e: ReactKeyEvent<HTMLElement>) => {
  if (e.target !== e.currentTarget || (e.key !== "Enter" && e.key !== " ")) return;
  e.preventDefault();
  fn();
};

const FOCUSABLE = 'a[href], button:not([disabled]), input:not([disabled]):not([type=hidden]), textarea:not([disabled]), select:not([disabled]), [tabindex]:not([tabindex="-1"])';

/** A dialog's keyboard manners: focus goes in when it opens (unless a field
 *  inside already took it), Tab and Shift+Tab stay inside it, and closing it
 *  puts the focus back where it was. */
export function useDialogFocus(): { ref: React.RefObject<HTMLDivElement | null>; onKeyDown: (e: ReactKeyEvent<HTMLElement>) => void } {
  const ref = useRef<HTMLDivElement>(null);
  const [from] = useState(() => document.activeElement as HTMLElement | null);
  useEffect(() => {
    const box = ref.current;
    if (box && !box.contains(document.activeElement)) box.focus({ preventScroll: true });
    return () => { if (from && from !== document.body && document.contains(from)) from.focus({ preventScroll: true }); };
  }, [from]);
  const onKeyDown = (e: ReactKeyEvent<HTMLElement>) => {
    if (e.key !== "Tab" || e.defaultPrevented) return;
    const box = ref.current;
    if (!box) return;
    const all = [...box.querySelectorAll<HTMLElement>(FOCUSABLE)].filter((x) => x.offsetParent !== null || x === document.activeElement);
    if (!all.length) { e.preventDefault(); box.focus(); return; }
    const first = all[0], last = all[all.length - 1], at = document.activeElement;
    if (e.shiftKey && (at === first || at === box)) { e.preventDefault(); last.focus(); }
    else if (!e.shiftKey && at === last) { e.preventDefault(); first.focus(); }
    else if (!box.contains(at)) { e.preventDefault(); first.focus(); }
  };
  return { ref, onKeyDown };
}

/** The id its ModalHead title carries, which names the dialog. */
const ModalTitle = createContext<string | undefined>(undefined);

/** Desktop modal with scrim (Esc and scrim click close it). */
export function Modal({ onClose, className, children }: { onClose: () => void; className?: string; children: ReactNode }) {
  const id = useId();
  const dlg = useDialogFocus();
  return (
    <div className="scrim" onMouseDown={(e) => { if (e.target === e.currentTarget) onClose(); }}>
      <ModalTitle.Provider value={id}>
        <div className={"modal" + (className ? " " + className : "")} role="dialog" aria-modal="true" aria-labelledby={id} tabIndex={-1} ref={dlg.ref} onKeyDown={dlg.onKeyDown}>{children}</div>
      </ModalTitle.Provider>
    </div>
  );
}

export function ModalHead({ title, onClose, children }: { title: ReactNode; onClose?: () => void; children?: ReactNode }) {
  const id = useContext(ModalTitle);
  return (
    <div className="modal-h">
      <h3 id={id} aria-level={2}>{title}</h3>
      {children}
      {onClose ? <button className="icon-btn" onClick={onClose} title="Close (Esc)" aria-label="Close"><Icon name="close" /></button> : null}
    </div>
  );
}

/** A confirm dialog: a centred modal on desktop, a Material dialog on Android. */
export function Confirm({ title, children, okLabel, danger, onOk, onCancel, busy }: { title: string; children: ReactNode; okLabel: string; danger?: boolean; onOk: () => void; onCancel: () => void; busy?: boolean }) {
  const platform = usePlatform();
  useEffect(() => {
    const k = (e: KeyboardEvent) => { if (e.key === "Escape") { e.preventDefault(); onCancel(); } };
    window.addEventListener("keydown", k);
    return () => window.removeEventListener("keydown", k);
  }, [onCancel]);
  if (platform === "android") {
    return (
      <>
        <div className="sheet-scrim" onClick={onCancel} />
        <div className="dialog" role="dialog" aria-modal="true" aria-labelledby="confirm-title">
          <h4 id="confirm-title">{title}</h4>
          {children}
          <div className="acts">
            <button className="btn ghost" onClick={onCancel}>Cancel</button>
            <button className={"btn ghost" + (danger ? " danger" : "")} onClick={onOk} disabled={busy}>{okLabel}</button>
          </div>
        </div>
      </>
    );
  }
  return (
    <Modal onClose={onCancel} className="confirm">
      <ModalHead title={title} />
      <div className="modal-b">{children}</div>
      <div className="modal-f">
        <button className="btn ghost" onClick={onCancel}>Cancel</button>
        <button className={"btn " + (danger ? "danger solid" : "primary")} onClick={onOk} disabled={busy}>{okLabel}</button>
      </div>
    </Modal>
  );
}

export function Toasts() {
  const list = useToasts();
  const platform = usePlatform();
  // the container stays in the page so a screen reader announces what lands in it
  if (platform === "android") return <div role="status" aria-live="polite">{list.length ? <div className="snack">{list[list.length - 1].text}</div> : null}</div>;
  return <div className="toasts" role="status" aria-live="polite">{list.map((t) => <div className="toast" key={t.id}>{t.text}</div>)}</div>;
}

export function NoteCard({ icon, warn, children, style }: { icon: IconName; warn?: boolean; children: ReactNode; style?: CSSProperties }) {
  return <div className={"note-card" + (warn ? " warn" : "")} style={style}><Icon name={icon} /><div>{children}</div></div>;
}

let version: string | null = null;

/** The app's version (tauri.conf.json), quietly beside the app's name
 *  (user 2026-10-09 07:42Z: visible on the main screen; 19:00Z: in the
 *  desktop title bar). `drag` keeps the title bar draggable over it: Tauri
 *  only drags from an element that carries the attribute itself. */
export function AppVersion({ drag }: { drag?: boolean } = {}) {
  const [v, setV] = useState(version);
  useEffect(() => {
    if (!version) appVersion().then((x) => { version = x; setV(x); }, () => {});
  }, []);
  return v ? <span className="ver" title={"Hubchat " + v} data-tauri-drag-region={drag || undefined}>v{v}</span> : null;
}

/** Re-render every `ms` while `on` (countdowns, "last seen"). */
export function useNow(on: boolean, ms = 1000): number {
  const [n, setN] = useState(() => Date.now());
  useEffect(() => {
    if (!on) return;
    const t = setInterval(() => setN(Date.now()), ms);
    return () => clearInterval(t);
  }, [on, ms]);
  return n;
}

/** "last seen 3 min ago" is worked out when it is drawn: while one is on
 *  screen, draw it again every 30 s so it keeps counting. */
export const useSeenTick = (on: boolean): number => useNow(on, 30_000);
