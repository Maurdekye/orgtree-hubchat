// The composer: per-chat drafts, the messages it replies to (one, or several),
// options picked on question cards, attachments (up to 10, picked or dropped
// on the chat) and a size check against the hub's limit before anything is
// sent.
import { useEffect, useImperativeHandle, useLayoutEffect, useRef, useState, type Ref } from "react";
import { api, newId, type Contact, type HubStatus, type Message, type NewAttachment, type NewOutgoing } from "../api";
import { Icon } from "../lib/icons";
import { bytes, isLong, MAX_FILES, num, utf8Len } from "../lib/format";
import { isImage, pastedImages, pastedName, thumbable, useFilePreview } from "../lib/images";
import { errText, pickFiles } from "../lib/native";
import { displayName, limitFor, msgTime, preview } from "../lib/peers";
import { foldAt } from "../lib/replies";
import { cardOf, type CardKnown } from "../lib/questions";
import { noteDraft, usePreparation, getPreparation, prepareSend, clearPreparation } from "../lib/store";
import { usePlatform } from "./ui";

interface Props {
  peer: string;
  c: Contact | undefined;
  hubs: HubStatus[];
  /** What the message replies to, oldest first. */
  replies: Message[];
  /** Options picked on open question cards. */
  picked: { card: Message; labels: string[] }[];
  /** Where a question card stands: open (the store knows it, and nothing
   *  answered or skipped it), settled, or unknown (the store hasn't said). */
  cardState: (id: string) => CardKnown;
  /** Where each loaded message sits in the chat, oldest first. */
  pos: Map<string, number>;
  onRemoveReply: (id: string) => void;
  onUnpick: (cardId: string) => void;
  /** Esc or the reply bar's close: reply to nothing. */
  onCancelReply: () => void;
  /** The message went (it is queued): what it carried, so that only that is
   *  cleared from the replies and picks. */
  onSent: (m: NewOutgoing) => void;
  ref?: Ref<ComposerApi>;
  /** Why nothing can be sent here (no hub lists the address): shown instead of the box. */
  off?: string | null;
}

/** For the conversation: files dropped on the chat join the attachments;
 *  the keyboard (R to reply, Esc) puts the cursor back in the box. */
export interface ComposerApi {
  addFiles: (sources: string[]) => void;
  focus: () => void;
}

/** A file waiting to be sent: its name and size. */
function AttChip({ a, onRemove, disabled }: { a: NewAttachment; onRemove: () => void; disabled?: boolean }) {
  return (
    <span className="attchip">
      <Icon name={isImage(a.name) ? "image" : "file"} /><span className="ell">{a.name}</span> <span className="s">{bytes(a.bytes)}</span>
      <button className="icon-btn" title="Remove" aria-label={"Remove " + a.name} disabled={disabled} onClick={onRemove}><Icon name="close" /></button>
    </span>
  );
}

/** An image waiting to be sent, as a thumbnail (user 2026-10-09 07:06Z);
 *  the file chip if it can't be read. */
function AttThumb({ a, onRemove, disabled }: { a: NewAttachment; onRemove: () => void; disabled?: boolean }) {
  const url = useFilePreview(a.source, a.name);
  if (url === "failed") return <AttChip a={a} onRemove={onRemove} disabled={disabled} />;
  return (
    <span className="attthumb" title={a.name + " · " + bytes(a.bytes)}>
      {url ? <img src={url} alt={a.name} draggable={false} /> : <span className="ph"><Icon name="image" /></span>}
      <button className="icon-btn" title="Remove" aria-label={"Remove " + a.name} disabled={disabled} onClick={onRemove}><Icon name="close" /></button>
    </span>
  );
}

export function Composer({ peer, c, hubs, replies, picked, cardState, pos, onRemoveReply, onUnpick, onCancelReply, onSent, ref, off }: Props) {
  const platform = usePlatform();
  const [text, setText] = useState("");
  const [atts, setAtts] = useState<NewAttachment[]>([]);
  const [err, setErr] = useState<string | null>(null);
  const [allReplies, setAllReplies] = useState(false);
  const preparation = usePreparation(peer);
  const sending = preparation?.phase === "preparing";
  const ta = useRef<HTMLTextAreaElement>(null);
  const saveT = useRef<ReturnType<typeof setTimeout> | null>(null);
  const textRef = useRef(text);
  textRef.current = text;

  // load this chat's draft; flush the previous chat's on switch
  useEffect(() => {
    let live = true;
    setText(""); setAtts([]); setErr(null);
    const saved = getPreparation(peer);
    if (saved && saved.phase !== "queued") { setText(saved.message.body); setAtts(saved.message.attachments ?? []); }
    else api.draft(peer).then((d) => { if (live && d && !getPreparation(peer)) { setText(d); noteDraft(peer, d); } }, () => {});
    return () => {
      live = false;
      if (saveT.current) { clearTimeout(saveT.current); saveT.current = null; void api.setDraft(peer, textRef.current).catch(() => {}); }
    };
  }, [peer]);
  // queued: the box empties, but the replies are left to onSent: Reply in the
  // chat still works while the files are prepared, and a reply chosen then
  // isn't part of this message (review R3)
  useEffect(() => {
    if (preparation?.phase !== "queued") return;
    setText(""); setAtts([]); setErr(null);
    clearPreparation(peer);
    onSent(preparation.message);
    ta.current?.focus();
  }, [preparation, peer, onSent]);

  useLayoutEffect(() => {
    const t = ta.current; if (!t) return;
    t.style.height = "auto";
    t.style.height = Math.min(platform === "android" ? 132 : 200, t.scrollHeight) + "px";
  }, [text, platform]);

  const change = (v: string) => {
    if (sending) return;
    clearPreparation(peer);
    setText(v); setErr(null);
    noteDraft(peer, v);
    if (saveT.current) clearTimeout(saveT.current);
    saveT.current = setTimeout(() => { saveT.current = null; void api.setDraft(peer, v).catch(() => {}); }, 400);
  };

  const lim = limitFor(c, hubs);
  const total = utf8Len(text) + atts.reduce((n, a) => n + a.bytes, 0);
  const over = lim ? total > lim.bytes : false;
  const overNote = lim ? "This message is " + bytes(total) + ". Hub " + lim.hub.name + " takes up to " + bytes(lim.bytes) + " per message, text and files together." : "";
  // a message may go with no text when it answers or skips a question card
  // (user 2026-10-11 22:42Z): a picked option answers even without text, a
  // plain reply to an open card with no text skips it. With no text, files or
  // not, every card it plainly replies to is skipped if open, so each must be
  // known first: while the store hasn't said for one, the message waits,
  // whatever else it answers, and keeps everything it holds (review R2)
  const plain = replies.filter((r) => !!cardOf(r) && !r.outgoing && !picked.some((p) => p.card.id === r.id));
  const skips = plain.filter((r) => cardState(r.id) === "open").length;
  const checking = text.trim() ? 0 : plain.filter((r) => cardState(r.id) === "unknown").length;
  const answersCard = picked.length > 0 || skips > 0;
  const can = (!!text.trim() || atts.length > 0 || answersCard) && !checking && !over && !sending;

  const attsRef = useRef(atts);
  attsRef.current = atts;
  const addFiles = async (sources: string[]) => {
    if (getPreparation(peer)?.phase === "preparing") return;
    clearPreparation(peer);
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
  useImperativeHandle(ref, () => ({ addFiles: (sources) => void addFiles(sources), focus: () => ta.current?.focus() }));
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
    if (saveT.current) { clearTimeout(saveT.current); saveT.current = null; }
    // every card answered is also replied to; the whole set goes oldest first
    const all = [...replies];
    for (const p of picked) if (!all.some((r) => r.id === p.card.id)) all.push(p.card);
    all.sort((a, b) => (pos.get(a.id) ?? -1) - (pos.get(b.id) ?? -1) || msgTime(a) - msgTime(b));
    const ids = all.map((r) => r.id), prev = preparation?.message;
    await prepareSend({
      id: newId(), peer, body: text.trim(), attachments: atts,
      reply_to: ids[0] ?? prev?.reply_to ?? null,
      replies: ids.length > 1 ? ids : ids.length ? undefined : prev?.replies,
      answers: picked.length ? picked.map((p) => ({ question: p.card.id, picked: p.labels })) : ids.length ? undefined : prev?.answers,
    });
  };

  const one = replies.length === 1 ? replies[0] : null;
  const who = one ? (one.outgoing ? "yourself" : displayName(c, peer)) : "";
  const fold = foldAt(replies.length), folded = !allReplies && fold < replies.length;
  const top = (
    <>
      {one ? (
        <div className="replybar">
          {platform === "desktop" ? <Icon name="reply" className="lead" /> : null}
          <div className="rb-t"><span className="rb-who">{platform === "android" ? <Icon name="reply" style={{ width: 16, height: 16 }} /> : null}Replying to {who}</span><span className="rb-txt" dir="auto">{preview(one)}</span></div>
          <button className="icon-btn" disabled={sending} onClick={onCancelReply} title="Cancel reply (Esc)" aria-label="Cancel reply"><Icon name="close" /></button>
        </div>
      ) : replies.length ? (
        <div className="replybar multi">
          {platform === "desktop" ? <Icon name="reply" className="lead" /> : null}
          <div className="rb-t">
            <span className="rb-who">{platform === "android" ? <Icon name="reply" style={{ width: 16, height: 16 }} /> : null}Replying to {replies.length} messages</span>
            {(folded ? replies.slice(0, fold) : replies).map((r) => (
              <span className="rb-item" key={r.id}>
                <span className="rb-txt" dir="auto"><b>{r.outgoing ? "You" : displayName(c, peer)}:</b> {preview(r)}</span>
                <button className="icon-btn" disabled={sending} onClick={() => onRemoveReply(r.id)} title="Don't reply to this one" aria-label={"Don't reply to: " + preview(r).slice(0, 60)}><Icon name="close" /></button>
              </span>
            ))}
            {folded ? <button className="link rb-more" onClick={() => setAllReplies(true)}>and {replies.length - fold} more</button> : null}
          </div>
          <button className="icon-btn" disabled={sending} onClick={onCancelReply} title="Cancel reply (Esc)" aria-label="Cancel reply"><Icon name="close" /></button>
        </div>
      ) : null}
      {picked.length ? (
        <div className="ansrow" aria-label="Answers to send">
          {picked.map((p) => (
            <span className="anschip" key={p.card.id} title={preview(p.card)}>
              <Icon name="check_circle" />
              <span className="ell"><span className="aq" dir="auto">{preview(p.card)}</span> <b dir="auto">{p.labels.join(", ")}</b></span>
              <button className="icon-btn" disabled={sending} onClick={() => onUnpick(p.card.id)} title="Remove this answer" aria-label={"Remove the answer to: " + preview(p.card).slice(0, 60)}><Icon name="close" /></button>
            </span>
          ))}
        </div>
      ) : null}
      {atts.length ? (
        <div className="attrow">
          {atts.map((a) => {
            const remove = () => { clearPreparation(peer); setAtts(atts.filter((x) => x !== a)); };
            return thumbable(a.name, a.bytes) ? <AttThumb key={a.source} a={a} onRemove={remove} disabled={sending} /> : <AttChip key={a.source} a={a} onRemove={remove} disabled={sending} />;
          })}
        </div>
      ) : null}
      {sending && preparation.total > 0 ? <div className="comp-preparing" role="status" aria-live="polite">
        <span className="spin" /><span><b>Preparing files...</b> {bytes(preparation.done)} / {bytes(preparation.total)}<br /><small>{preparation.name}. Your message will queue when its files are ready.</small></span>
      </div> : sending ? <div className="comp-preparing" role="status">Queuing message...</div> : null}
      {err || preparation?.error ? <div className="comp-err" role="alert"><Icon name="error" />{err || preparation?.error}</div> : null}
    </>
  );
  const meta = over
    ? <span style={{ color: "var(--bad)" }}>{overNote}</span>
    : <>
        {platform === "desktop" ? <span><kbd>Enter</kbd> send · <kbd>Shift</kbd>+<kbd>Enter</kbd> new line</span> : null}
        {checking ? <span className="skipnote">Checking whether {checking === 1 ? "the question you reply to is" : checking + " questions you reply to are"} still open</span>
          : skips && !text.trim() ? <span className="skipnote">Sending with no text skips {skips === 1 ? "the question" : skips + " questions"} you reply to</span> : null}
        {isLong(text) ? <span className="count">{num(text.length)} characters · shows collapsed</span> : null}
      </>;
  const row = (
    <div className="comp-row">
      <div className="comp-box">
        <button className="icon-btn" disabled={sending} onClick={attach} title="Attach files" aria-label="Attach files"><Icon name="attach" /></button>
        <textarea disabled={sending} ref={ta} rows={1} value={text} spellCheck dir="auto" aria-label={"Message " + displayName(c, peer)} placeholder={picked.length ? "Add a message (optional)" : "Message " + displayName(c, peer)}
          onChange={(e) => change(e.target.value)}
          onPaste={(e) => { const imgs = pastedImages(e.clipboardData); if (imgs.length) { e.preventDefault(); void paste(imgs); } }}
          onKeyDown={(e) => {
            if (platform === "desktop" && e.key === "Enter" && !e.shiftKey && !e.nativeEvent.isComposing) { e.preventDefault(); void send(); }
            else if (e.key === "Escape" && replies.length && !sending) { e.stopPropagation(); e.preventDefault(); onCancelReply(); }
          }} />
      </div>
      {/* pressing Send leaves the focus (and Android's keyboard) in the message box */}
      <button className="sendbtn" disabled={!can} onMouseDown={(e) => e.preventDefault()} onClick={send}title={platform === "desktop" ? "Send (Enter)" : "Send"} aria-label="Send"><Icon name="send" /></button>
    </div>
  );
  if (off && !sending) {
    const note = <div className="comp-off" role="status"><Icon name="cloud_off" /><span>{off}</span></div>;
    return platform === "android" ? <div className="composer">{note}</div> : <div className="composer"><div className="composer-in">{note}</div></div>;
  }
  if (platform === "android") return <div className="composer">{top}{row}<div className="comp-meta">{meta}</div></div>;
  return <div className="composer"><div className="composer-in">{top}{row}<div className="comp-meta">{meta}</div></div></div>;
}
