// Transfer progress and paged history, reached through Settings on both layouts.
import { useEffect, useRef, useState } from "react";
import { api, type TransferHistoryView } from "../api";
import { Icon } from "../lib/icons";
import { bytes } from "../lib/format";
import { errText } from "../lib/native";
import { displayName } from "../lib/peers";
import { cancelTransfer, useActiveTransfers, useSnap, type Active } from "../lib/store";
import { toast } from "../lib/toast";
import { pressKeys, usePlatform } from "./ui";

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
  const finishing = ["publishing", "exporting"].includes(t.state) || (!t.upload && t.state === "running" && t.done === t.total);
  const label = finishing ? "Finishing..." : ["queued", "pending"].includes(t.state) ? "Waiting" : t.state === "paused" ? "Paused" : pct(t.done, t.total) + "%";
  return (
    <div className={"xrow " + (t.upload ? "up" : "down")} onClick={() => t.peer && onOpen(t.peer)} role="button">
      <span className="xic"><Icon name={t.upload ? "upload" : "download"} /></span>
      <div className="t">
        <b>{t.name || "File"}</b>
        <span>{t.upload ? "to " : "from "}{who} · {label} · {bytes(t.done)} of {bytes(t.total)}</span>
        <i className="tb-bar"><b style={{ width: pct(t.done, t.total) + "%" }} /></i>
      </div>
      {!finishing && (android
        ? <button className="link" onClick={(e) => { e.stopPropagation(); void cancel(); }}>Cancel</button>
        : <button className="icon-btn" title={t.upload ? "Cancel upload" : "Cancel download"} aria-label="Cancel" onClick={(e) => { e.stopPropagation(); void cancel(); }}><Icon name="close" /></button>)}
    </div>
  );
}

function HistoryRow({ t, refresh }: {t: TransferHistoryView; refresh: () => void}) {
  const [busy, setBusy] = useState(false);
  const pending = useRef(false);
  const live = useRef(true);
  useEffect(()=>{live.current=true;return ()=>{live.current=false;};},[]);
  const [error, setError] = useState("");
  const run = async (download: boolean) => {
    if (pending.current) return;
    pending.current = true; setBusy(true); setError("");
    try {
      if (download) await api.download(t.message_id, t.local_id, true);
      else await api.openAttachment(t.message_id, t.local_id, false);
      // Admission finishes quickly; the scheduler reports actual completion.
    } catch (e) { if(live.current){const text=errText(e); setError(text); toast(text);} }
    finally { pending.current=false; if(live.current){setBusy(false);refresh();} }
  };
  const missing="Local file is missing or no longer available.";
  const remote=t.state === "expired" ? "File no longer available on the hub." : "";
  return <div className={"xrow xhistory-row " + t.state} data-transfer={t.local_id}>
    <span className="xic"><Icon name={t.upload ? "upload" : "download"} /></span>
    <div className="t"><b>{t.name || "File"}</b><span>{t.upload ? "Sent" : "Received"} · {bytes(t.total)}</span>
      {!t.available && <span className="xreason">{missing}</span>}
      {(error || remote || t.error) && <span className="xreason" role="status">{error || remote || t.error}</span>}
    </div>
    <div className="xactions">
      <button className="link" disabled={busy || !t.available} title={!t.available ? missing : undefined} onClick={() => void run(false)}>View</button>
      {!t.upload && <button className="link" disabled={busy || !t.can_redownload} title={!t.can_redownload ? remote || "Download is not available." : "Save a fresh copy"} onClick={() => void run(true)}>Redownload</button>}
    </div>
  </div>;
}

/** Read only visible pages. Progress ticks do not query attachment history. */
function TransferHistory({ active }: {active: Active[]}) {
  const snap=useSnap();
  const [rows,setRows]=useState<TransferHistoryView[]>([]);
  const [next,setNext]=useState<string|null>(null);
  const [error,setError]=useState("");
  const [loading,setLoading]=useState(false);
  const epoch=useRef(0);
  const pending=useRef(false);
  const [revision,setRevision]=useState(0);
  const owner=snap.state?.me?.address;
  const [pageOwner,setPageOwner]=useState(owner);
  const activeKey=active.map(t=>t.local_id+":"+t.state).join("|");
  useEffect(()=>{
    const mine=++epoch.current;
    pending.current=true;setLoading(true);setRows([]);setNext(null);setError("");
    api.transferHistory().then(page=>{
      if(epoch.current!==mine)return;
      setRows(page.rows);setNext(page.next);setPageOwner(owner);
    },e=>{if(epoch.current===mine)setError(errText(e));}).finally(()=>{
      if(epoch.current===mine){pending.current=false;setLoading(false);}
    });
    return ()=>{epoch.current++;};
  },[owner,activeKey,revision]);
  const more=async()=>{
    if(!next||pending.current)return;
    const mine=epoch.current,after=next;
    pending.current=true;setLoading(true);setError("");
    try {
      const page=await api.transferHistory(after);
      if(epoch.current!==mine)return;
      if(page.next===after)throw Error("Transfer history page did not advance");
      setRows(old=>[...new Map([...old,...page.rows].map(r=>[r.local_id,r])).values()]);setNext(page.next);
    } catch(e){if(epoch.current===mine)setError(errText(e));}
    finally{if(epoch.current===mine){pending.current=false;setLoading(false);}}
  };
  const visible=pageOwner===owner?rows.filter(r=>!active.some(t=>t.local_id===r.local_id)):[];
  return <div className="xhistory">
    <div className="help xhistory-label">Transfer history</div>
    {visible.map(t=><HistoryRow key={t.local_id} t={t} refresh={()=>setRevision(r=>r+1)} />)}
    {!visible.length&&!loading&&!error&&<div className="help xhistory-label">No finished transfers yet.</div>}
    {error&&<div role="status" className="help xhistory-label">{error} <button className="link" onClick={()=>setRevision(r=>r+1)}>Retry</button></div>}
    {loading&&<div role="status" className="help xhistory-label">Loading…</div>}
    {next&&<button className="link xhistory-more" disabled={loading} onClick={()=>void more()}>Load more</button>}
  </div>;
}

/** Shared Settings section: active rows and bounded, on-demand history. */
export function TransfersPanel({ onOpen }: { onOpen?: (peer: string) => void }) {
  const list = useActiveTransfers();
  const platform = usePlatform();
  return <div className="transfers-panel xlist">
    <div className="help xhistory-label">In progress{list.length ? ` · ${overall(list)}%` : ""}</div>
    {list.length ? list.map(t => <XRow key={t.local_id} t={t} onOpen={onOpen ?? (() => {})} android={platform === "android"} />)
      : <div className="help xhistory-label">No transfers in progress.</div>}
    <TransferHistory active={list} />
  </div>;
}

/** Legacy compact control, kept for the standalone collapse/history proofs. */
export function TransfersChip({ onOpen }: { onOpen: (peer: string) => void }) {
  const platform = usePlatform();
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
  const p = overall(list);
  return (
    <>
      <button ref={btn} className={"tb-xfer" + (open ? " on" : "")} onClick={() => setOpen((o) => !o)} title={"Transfers: " + list.length + " in progress"} aria-label={list.length ? p + "% " + list.length + " transfers in progress" : "Transfers"} aria-expanded={open}>
        <Icon name="transfers" /><span>{list.length ? p+"%" : "Transfers"}</span>{!!list.length&&<span className="n">{list.length}</span>}
        {!!list.length&&<i className="tb-bar"><b style={{ width: p + "%" }} /></i>}
      </button>
      {open ? (
        <div className="pop xpop" ref={pop} style={{ left: pos.x, top: pos.y }}>
          <div className="pop-h">Transfers</div>
          {list.map((t) => <XRow key={t.local_id} t={t} onOpen={(peer) => { setOpen(false); onOpen(peer); }} />)}
          <TransferHistory active={list} />
          <div className="sep" />
          <div className="help" style={{ padding: "4px 10px 6px", fontSize: 12, maxWidth: 380 }}>{platform === "android" ? "If Android pauses a transfer, open Hubchat to resume it." : "Transfers keep going while Hubchat is open or in the tray."} Interrupted files resume when the hub supports it; older hubs restart from zero.</div>
        </div>
      ) : null}
    </>
  );
}

/** Android: progress and history above the chat list. */
export function TransfersStrip({ onOpen }: { onOpen: (peer: string) => void }) {
  const list = useActiveTransfers();
  const [open, setOpen] = useState(false);
  const p = overall(list);
  const ups = list.filter((t) => t.upload).length;
  const what = ups === list.length ? "Uploading" : ups === 0 ? "Downloading" : "Transferring";
  return (
    <div className="xstrip">
      <div className="strip" onClick={() => setOpen((o) => !o)} role="button" tabIndex={0} onKeyDown={pressKeys(() => setOpen((o) => !o))} aria-expanded={open}>
        <Icon name="transfers" />
        <span>{list.length ? <><b>{what} {list.length === 1 ? (list[0].name || "a file") : list.length + " files"}</b> · {p}%</> : <b>Transfers</b>}</span>
        <Icon name={open ? "expand_less" : "expand_more"} className="xchev" />
        {!!list.length&&<i className="tb-bar"><b style={{ width: p + "%" }} /></i>}
      </div>
      {open ? <div className="xlist">{list.map((t) => <XRow key={t.local_id} t={t} onOpen={onOpen} android />)}<TransferHistory active={list} /></div> : null}
    </div>
  );
}
