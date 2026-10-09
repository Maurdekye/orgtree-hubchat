// The composer: per-chat drafts, reply bar, attachments (up to 10, picked or
// dropped on the chat) and a size check against the hub's limit before
// anything is sent.
import { useEffect, useImperativeHandle, useLayoutEffect, useRef, useState, type Ref } from "react";
import { api, newId, type Contact, type HubStatus, type Message, type NewAttachment } from "../api";
import { Icon } from "../lib/icons";
import { bytes, isLong, MAX_FILES, num, utf8Len } from "../lib/format";
import { isImage, pastedImages, pastedName, thumbable, useFilePreview } from "../lib/images";
import { errText, pickFiles } from "../lib/native";
import { displayName, limitFor, preview } from "../lib/peers";
import { noteDraft, refreshChats } from "../lib/store";
import { usePlatform } from "./ui";

interface Props {
  peer: string;
  c: Contact | undefined;
  hubs: HubStatus[];
  replyTo: Message | null;
  onCancelReply: () => void;
  onSent: () => void;
  ref?: Ref<ComposerApi>;
  /** Why nothing can be sent here (no hub lists the address): shown instead of the box. */
  off?: string | null;
}

/** For the conversation: files dropped on the chat join the attachments. */
export interface ComposerApi {
  addFiles: (sources: string[]) => void;
}

/** A file waiting to be sent: its name and size. */
function AttChip({ a, onRemove }: { a: NewAttachment; onRemove: () => void }) {
  return (
    <span className="attchip">
      <Icon name={isImage(a.name) ? "image" : "file"} /><span className="ell">{a.name}</span> <span className="s">{bytes(a.bytes)}</span>
      <button className="icon-btn" title="Remove" aria-label={"Remove " + a.name} onClick={onRemove}><Icon name="close" /></button>
    </span>
  );
}

/** An image waiting to be sent, as a thumbnail (user 2026-10-09 07:06Z);
 *  the file chip if it can't be read. */
function AttThumb({ a, onRemove }: { a: NewAttachment; onRemove: () => void }) {
  const url = useFilePreview(a.source, a.name);
  if (url === "failed") return <AttChip a={a} onRemove={onRemove} />;
  return (
    <span className="attthumb" title={a.name + " · " + bytes(a.bytes)}>
      {url ? <img src={url} alt={a.name} draggable={false} /> : <span className="ph"><Icon name="image" /></span>}
      <button className="icon-btn" title="Remove" aria-label={"Remove " + a.name} onClick={onRemove}><Icon name="close" /></button>
    </span>
  );
}

export function Composer({ peer, c, hubs, replyTo, onCancelReply, onSent, ref, off }: Props) {
  const platform = usePlatform();
  const [text, setText] = useState("");
  const [atts, setAtts] = useState<NewAttachment[]>([]);
  const [err, setErr] = useState<string | null>(null);
  const [sending, setSending] = useState(false);
  const ta = useRef<HTMLTextAreaElement>(null);
  const saveT = useRef<ReturnType<typeof setTimeout> | null>(null);
  const textRef = useRef(text);
  textRef.current = text;

  // load this chat's draft; flush the previous chat's on switch
  useEffect(() => {
    let live = true;
    setText(""); setAtts([]); setErr(null);
    api.draft(peer).then((d) => { if (live && d) { setText(d); noteDraft(peer, d); } }, () => {});
    return () => {
      live = false;
      if (saveT.current) { clearTimeout(saveT.current); saveT.current = null; void api.setDraft(peer, textRef.current).catch(() => {}); }
    };
  }, [peer]);
  useEffect(() => { if (replyTo) ta.current?.focus(); }, [replyTo]);

  useLayoutEffect(() => {
    const t = ta.current; if (!t) return;
    t.style.height = "auto";
    t.style.height = Math.min(platform === "android" ? 132 : 200, t.scrollHeight) + "px";
  }, [text, platform]);

  const change = (v: string) => {
    setText(v); setErr(null);
    noteDraft(peer, v);
    if (saveT.current) clearTimeout(saveT.current);
    saveT.current = setTimeout(() => { saveT.current = null; void api.setDraft(peer, v).catch(() => {}); }, 400);
  };

  const lim = limitFor(c, hubs);
  const total = utf8Len(text) + atts.reduce((n, a) => n + a.bytes, 0);
  const over = lim ? total > lim.bytes : false;
  const overNote = lim ? "This message is " + bytes(total) + ". Hub " + lim.hub.name + " takes up to " + bytes(lim.bytes) + " per message, text and files together." : "";
  const can = (!!text.trim() || atts.length > 0) && !over && !sending;

  const attsRef = useRef(atts);
  attsRef.current = atts;
  const addFiles = async (sources: string[]) => {
    setErr(null);
    const next = [...attsRef.current];
    for (const source of sources) {
      if (next.length >= MAX_FILES) { setErr("Up to " + MAX_FILES + " files per message."); break; }
      if (next.some((a) => a.source === source)) continue;
      try { const info = await api.fileInfo(source); next.push({ name: info.name, bytes: info.bytes, source }); }
      catch (e) { setErr("Can't attach " + ((!source.startsWith("content://") && source.split(/[\\/]/).pop()) || "that file") + ": " + errText(e)); }
    }
    setAtts(next);
  };
  useImperativeHandle(ref, () => ({ addFiles: (sources) => void addFiles(sources) }));
  // an image pasted into the box joins the attachments (user 23:40Z); text
  // still pastes as text
  const paste = async (files: File[]) => {
    const when = new Date();
    const paths: string[] = [];
    for (const [i, f] of files.entries()) {
      try { paths.push(await api.savePasted(pastedName(f.type, when, i + 1), new Uint8Array(await f.arrayBuffer()))); }
      catch (e) { setErr("Can't attach the pasted image: " + errText(e)); }
    }
    if (paths.length) await addFiles(paths);
  };
  const attach = async () => {
    setErr(null);
    let picked: string[];
    try { picked = await pickFiles(); } catch (e) { setErr(errText(e)); return; }
    await addFiles(picked);
  };

  const send = async () => {
    if (!can) { if (over) setErr(overNote); return; }
    setSending(true);
    try {
      await api.send({ id: newId(), peer, body: text.trim(), reply_to: replyTo?.id ?? null, attachments: atts });
      if (saveT.current) { clearTimeout(saveT.current); saveT.current = null; }
      setText(""); setAtts([]); setErr(null);
      noteDraft(peer, "");
      void api.setDraft(peer, "").catch(() => {});
      onCancelReply();
      onSent();
      void refreshChats();
    } catch (e) { setErr(errText(e)); }
    finally { setSending(false); ta.current?.focus(); }
  };

  const who = replyTo ? (replyTo.outgoing ? "yourself" : displayName(c, peer)) : "";
  const top = (
    <>
      {replyTo ? (
        <div className="replybar">
          {platform === "desktop" ? <Icon name="reply" className="lead" /> : null}
          <div className="rb-t"><span className="rb-who">{platform === "android" ? <Icon name="reply" style={{ width: 16, height: 16 }} /> : null}Replying to {who}</span><span className="rb-txt">{preview(replyTo)}</span></div>
          <button className="icon-btn" onClick={onCancelReply} title="Cancel reply (Esc)" aria-label="Cancel reply"><Icon name="close" /></button>
        </div>
      ) : null}
      {atts.length ? (
        <div className="attrow">
          {atts.map((a) => {
            const remove = () => setAtts(atts.filter((x) => x !== a));
            return thumbable(a.name, a.bytes) ? <AttThumb key={a.source} a={a} onRemove={remove} /> : <AttChip key={a.source} a={a} onRemove={remove} />;
          })}
        </div>
      ) : null}
      {err ? <div className="comp-err"><Icon name="error" />{err}</div> : null}
    </>
  );
  const meta = over
    ? <span style={{ color: "var(--bad)" }}>{overNote}</span>
    : <>
        {platform === "desktop" ? <span><kbd>Enter</kbd> send · <kbd>Shift</kbd>+<kbd>Enter</kbd> new line</span> : null}
        {isLong(text) ? <span className="count">{num(text.length)} characters · shows collapsed</span> : null}
      </>;
  const row = (
    <div className="comp-row">
      <div className="comp-box">
        <button className="icon-btn" onClick={attach} title="Attach files" aria-label="Attach files"><Icon name="attach" /></button>
        <textarea ref={ta} rows={1} value={text} spellCheck placeholder={"Message " + displayName(c, peer)}
          onChange={(e) => change(e.target.value)}
          onPaste={(e) => { const imgs = pastedImages(e.clipboardData); if (imgs.length) { e.preventDefault(); void paste(imgs); } }}
          onKeyDown={(e) => {
            if (platform === "desktop" && e.key === "Enter" && !e.shiftKey && !e.nativeEvent.isComposing) { e.preventDefault(); void send(); }
            else if (e.key === "Escape" && replyTo) { e.stopPropagation(); e.preventDefault(); onCancelReply(); }
          }} />
      </div>
      {/* pressing Send leaves the focus (and Android's keyboard) in the message box */}
      <button className="sendbtn" disabled={!can} onMouseDown={(e) => e.preventDefault()} onClick={send}title={platform === "desktop" ? "Send (Enter)" : "Send"} aria-label="Send"><Icon name="send" /></button>
    </div>
  );
  if (off) {
    const note = <div className="comp-off" role="status"><Icon name="cloud_off" /><span>{off}</span></div>;
    return platform === "android" ? <div className="composer">{note}</div> : <div className="composer"><div className="composer-in">{note}</div></div>;
  }
  if (platform === "android") return <div className="composer">{top}{row}<div className="comp-meta">{meta}</div></div>;
  return <div className="composer"><div className="composer-in">{top}{row}<div className="comp-meta">{meta}</div></div></div>;
}
