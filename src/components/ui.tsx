// Small shared pieces: platform context, avatar, address, chips, dialogs.
import { createContext, useContext, useEffect, useState, type CSSProperties, type ReactNode } from "react";
import QRCode from "qrcode";
import type { Contact, HubStatus, Message } from "../api";
import { Icon, type IconName } from "../lib/icons";
import { initials } from "../lib/format";
import { displayName, kindInfo, kindOf, presence, splitAddr, tickInfo, type Kind, type PresState } from "../lib/peers";
import { useToasts } from "../lib/toast";

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
  return <span className={"tick " + t.cls} title={t.label}><Icon name={t.ic} /></span>;
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

/** Desktop modal with scrim (Esc and scrim click close it). */
export function Modal({ onClose, className, children }: { onClose: () => void; className?: string; children: ReactNode }) {
  return (
    <div className="scrim" onMouseDown={(e) => { if (e.target === e.currentTarget) onClose(); }}>
      <div className={"modal" + (className ? " " + className : "")} role="dialog" aria-modal="true">{children}</div>
    </div>
  );
}

export function ModalHead({ title, onClose, children }: { title: ReactNode; onClose?: () => void; children?: ReactNode }) {
  return (
    <div className="modal-h">
      <h3>{title}</h3>
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
        <div className="dialog" role="dialog" aria-modal="true">
          <h4>{title}</h4>
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
  if (platform === "android") return list.length ? <div className="snack">{list[list.length - 1].text}</div> : null;
  return <div className="toasts">{list.map((t) => <div className="toast" key={t.id}>{t.text}</div>)}</div>;
}

export function NoteCard({ icon, warn, children, style }: { icon: IconName; warn?: boolean; children: ReactNode; style?: CSSProperties }) {
  return <div className={"note-card" + (warn ? " warn" : "")} style={style}><Icon name={icon} /><div>{children}</div></div>;
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
