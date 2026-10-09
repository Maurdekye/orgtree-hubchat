// Uploads and downloads in progress. Desktop: a chip in the sidebar header
// (count + overall %) with a popover listing each one. Android: a compact
// strip at the top of the chat list that expands to the same list.
import { useEffect, useRef, useState } from "react";
import { Icon } from "../lib/icons";
import { bytes } from "../lib/format";
import { errText } from "../lib/native";
import { displayName } from "../lib/peers";
import { cancelTransfer, useActiveTransfers, useSnap, type Active } from "../lib/store";
import { toast } from "../lib/toast";
import { pressKeys } from "./ui";

const pct = (done: number, total: number) => Math.floor((100 * done) / Math.max(1, total));

function overall(list: Active[]) {
  const total = list.reduce((s, t) => s + t.total, 0);
  const done = list.reduce((s, t) => s + t.done, 0);
  return pct(done, total);
}

function XRow({ t, onOpen, android }: { t: Active; onOpen: (peer: string) => void; android?: boolean }) {
  const snap = useSnap();
  const who = t.peer ? displayName(snap.byAddr.get(t.peer), t.peer) : "…";
  const cancel = () => cancelTransfer(t.local_id).catch((e) => toast(errText(e)));
  return (
    <div className={"xrow " + (t.upload ? "up" : "down")} onClick={() => t.peer && onOpen(t.peer)} role="button">
      <span className="xic"><Icon name={t.upload ? "upload" : "download"} /></span>
      <div className="t">
        <b>{t.name || "File"}</b>
        <span>{t.upload ? "to " : "from "}{who} · {pct(t.done, t.total)}% · {bytes(t.done)} of {bytes(t.total)}</span>
        <i className="tb-bar"><b style={{ width: pct(t.done, t.total) + "%" }} /></i>
      </div>
      {android
        ? <button className="link" onClick={(e) => { e.stopPropagation(); void cancel(); }}>Cancel</button>
        : <button className="icon-btn" title={t.upload ? "Cancel upload" : "Cancel download"} aria-label="Cancel" onClick={(e) => { e.stopPropagation(); void cancel(); }}><Icon name="close" /></button>}
    </div>
  );
}

/** Desktop: the chip (shown only while something moves) and its popover. */
export function TransfersChip({ onOpen }: { onOpen: (peer: string) => void }) {
  const list = useActiveTransfers();
  const [open, setOpen] = useState(false);
  const btn = useRef<HTMLButtonElement>(null);
  const pop = useRef<HTMLDivElement>(null);
  const [pos, setPos] = useState({ x: 0, y: 0 });

  useEffect(() => {
    if (!open) return;
    const r = btn.current?.getBoundingClientRect();
    if (r) setPos({ x: Math.max(8, Math.min(r.left, window.innerWidth - 400)), y: r.bottom + 6 });
    const down = (e: MouseEvent) => {
      if (pop.current?.contains(e.target as Node) || btn.current?.contains(e.target as Node)) return;
      setOpen(false);
    };
    const key = (e: KeyboardEvent) => { if (e.key === "Escape") { e.stopPropagation(); setOpen(false); } };
    window.addEventListener("mousedown", down);
    window.addEventListener("keydown", key, true);
    return () => { window.removeEventListener("mousedown", down); window.removeEventListener("keydown", key, true); };
  }, [open]);
  useEffect(() => { if (!list.length) setOpen(false); }, [list.length]);

  if (!list.length) return null;
  const p = overall(list);
  return (
    <>
      <button ref={btn} className={"tb-xfer" + (open ? " on" : "")} onClick={() => setOpen((o) => !o)} title={"Transfers: " + list.length + " in progress"} aria-label={p + "% " + list.length + " transfers in progress"}>
        <Icon name="transfers" /><span>{p}%</span><span className="n">{list.length}</span>
        <i className="tb-bar"><b style={{ width: p + "%" }} /></i>
      </button>
      {open ? (
        <div className="pop xpop" ref={pop} style={{ left: pos.x, top: pos.y }}>
          <div className="pop-h">Transfers</div>
          {list.map((t) => <XRow key={t.local_id} t={t} onOpen={(peer) => { setOpen(false); onOpen(peer); }} />)}
          <div className="sep" />
          <div className="help" style={{ padding: "4px 10px 6px", fontSize: 12, maxWidth: 380 }}>Transfers keep going while Hubchat is open or in the tray. A retry starts the file again: the hub can't resume a transfer.</div>
        </div>
      ) : null}
    </>
  );
}

/** Android: a strip above the chat list while something moves. */
export function TransfersStrip({ onOpen }: { onOpen: (peer: string) => void }) {
  const list = useActiveTransfers();
  const [open, setOpen] = useState(false);
  if (!list.length) return null;
  const p = overall(list);
  const ups = list.filter((t) => t.upload).length;
  const what = ups === list.length ? "Uploading" : ups === 0 ? "Downloading" : "Transferring";
  return (
    <div className="xstrip">
      <div className="strip" onClick={() => setOpen((o) => !o)} role="button" tabIndex={0} onKeyDown={pressKeys(() => setOpen((o) => !o))} aria-expanded={open}>
        <Icon name="transfers" />
        <span><b>{what} {list.length === 1 ? (list[0].name || "a file") : list.length + " files"}</b> · {p}%</span>
        <Icon name={open ? "expand_less" : "expand_more"} className="xchev" />
        <i className="tb-bar"><b style={{ width: p + "%" }} /></i>
      </div>
      {open ? <div className="xlist">{list.map((t) => <XRow key={t.local_id} t={t} onOpen={onOpen} android />)}</div> : null}
    </div>
  );
}
