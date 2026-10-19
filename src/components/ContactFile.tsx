import { useEffect, useRef, useState, type MouseEvent } from "react";
import { api, type Attachment, type Message } from "../api";
import { attachmentMenu, localAttachment } from "../lib/attachmentActions";
import { openMenu } from "../lib/ctxmenu";
import { shortWhen } from "../lib/format";
import { openImage, previewable } from "../lib/images";
import { Icon } from "../lib/icons";
import { errText, openAttachment } from "../lib/native";
import { attView, msgTime } from "../lib/peers";
import { getSnap, useTransfer } from "../lib/store";
import { toast } from "../lib/toast";
import { usePlatform } from "./ui";

/** Metadata row; previews/downloads only start from an explicit action. */
export function ContactFile({ a, m, owner, onShowMessage }: { a: Attachment; m: Message; owner: string; onShowMessage: (id: string) => void }) {
  const platform = usePlatform(), progress = useTransfer(a.local_id);
  const v = attView(a, m, progress);
  const [requesting, setRequesting] = useState(false), [error, setError] = useState("");
  const pending = useRef(false), openAfter = useRef(false), live = useRef(false);
  useEffect(() => { live.current = true; return () => { live.current = false; openAfter.current = false; }; }, []);
  const current = () => live.current && getSnap().state?.me?.address === owner;
  const fail = (e: unknown) => { if (current()) { const text = errText(e); setError(text); toast(text); } };
  const saved = localAttachment(m, a), image = previewable(a) && v.st !== "expired";
  const canDownload = v.download || v.retry === "download";
  // A click may fetch a remote file first. A save-only menu action never opens
  // it later, and leaving this panel/identity cancels that deferred open.
  useEffect(() => {
    if (!openAfter.current || !current()) return;
    if (v.st === "bad" || v.st === "expired") { openAfter.current = false; return; }
    if (saved) { openAfter.current = false; void openAttachment(m.id, a.local_id, false).catch(fail); }
  }, [saved, v.st, owner, m.id, a.local_id]);
  const activate = async () => {
    if (!current() || pending.current) return;
    setError("");
    if (image) { openImage(m, a); return; }
    if (!saved && !canDownload && v.retry !== "send") return;
    pending.current = true; setRequesting(true);
    try {
      if (saved) await openAttachment(m.id, a.local_id, false);
      else if (canDownload) { openAfter.current = true; await api.download(m.id, a.local_id); }
      else await api.retry(m.id);
    } catch (e) { openAfter.current = false; fail(e); }
    finally { pending.current = false; if (current()) setRequesting(false); }
  };
  const menu = (e: MouseEvent<HTMLElement>) => {
    e.preventDefault(); e.stopPropagation();
    if (!current()) return;
    const entries = attachmentMenu(m, a, platform).map(it => it === "sep" ? it : { ...it, run: () => { if (current()) { openAfter.current = false; it.run(); } } });
    entries.push("sep", { label: "Show in chat", icon: "forum", run: () => { if (current()) onShowMessage(m.id); } });
    const r = e.currentTarget.getBoundingClientRect();
    openMenu(e.type === "contextmenu" && e.clientX ? e.clientX : r.left, e.type === "contextmenu" && e.clientY ? e.clientY : r.bottom, entries);
  };
  const disabled = requesting || (!image && !saved && !canDownload && v.retry !== "send");
  return <div className={"contact-file " + (platform === "android" ? "li" : "filerow") + " " + v.st} data-contact-file={a.local_id} onContextMenu={menu}>
    <button className="contact-file-open" onClick={() => void activate()} disabled={disabled} title={a.name} aria-label={(image ? "View " : saved ? "Open " : canDownload ? "Download and open " : "File ") + a.name}>
      <span className="att-ic"><Icon name={image ? "image" : v.ic} /></span>
      <span className="t"><span className="contact-file-name">{a.name}</span><span className="contact-file-meta">{requesting ? "Working…" : v.sub} · {shortWhen(msgTime(m))}</span>
        {error && <span className="contact-file-error" role="status">{error}</span>}
        {v.bar != null && <span className="contact-file-progress"><i style={{width:v.bar+"%"}} /></span>}
      </span>
    </button>
    <button className="icon-btn contact-file-more" onClick={menu} aria-label={"Actions for " + a.name} title="File actions"><Icon name="more_vert" /></button>
  </div>;
}
