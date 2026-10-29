// The composer: per-chat drafts, the messages it replies to (one, or several),
// options picked on question cards, attachments (up to 10, picked or dropped
// on the chat) and a size check against the hub's limit before anything is
// sent.
import { useEffect, useImperativeHandle, useLayoutEffect, useRef, useState, type Ref } from "react";
import { api, newId, type Contact, type HubStatus, type Message, type NewAttachment, type NewOutgoing } from "../api";
import { Icon } from "../lib/icons";
import { bytes, fileTitle, isLong, MAX_FILES, num, utf8Len } from "../lib/format";
import { isImage, pastedImages, pastedName, thumbable, useFilePreview } from "../lib/images";
import { errText, pickFiles } from "../lib/native";
import { displayName, kindOf, limitFor, msgTime, preview } from "../lib/peers";
import { activeMentionToken, completeMentionText, filterMentionContacts, mentionContacts } from "../lib/mentions";
import { foldAt } from "../lib/replies";
import { cardOf, type CardKnown } from "../lib/questions";
import { noteDraft, usePreparation, getPreparation, prepareSend, clearPreparation, getSnap, useSnap } from "../lib/store";
import { KindGlyph, PeerAvatar, usePlatform } from "./ui";
import { refreshKey, useRestoredRefreshState, useRefreshState } from "../lib/refresh";

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
  /** a send is in flight: the reply bar's × is disabled, so Escape waits too */
  busy: () => boolean;
}

/** A file waiting to be sent: its name and size. */
function AttChip({ a, onRemove, disabled }: { a: NewAttachment; onRemove: () => void; disabled?: boolean }) {
  return (
    <span className="attchip" title={fileTitle(a.name, a.bytes)}>
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
    <span className="attthumb" title={fileTitle(a.name, a.bytes)}>
      {url ? <img src={url} alt={a.name} draggable={false} /> : <span className="ph"><Icon name="image" /></span>}
      <button className="icon-btn" title="Remove" aria-label={"Remove " + a.name} disabled={disabled} onClick={onRemove}><Icon name="close" /></button>
    </span>
  );
}

export function Composer({ peer, c, hubs, replies, picked, cardState, pos, onRemoveReply, onUnpick, onCancelReply, onSent, ref, off }: Props) {
  const platform = usePlatform();
  const snap = useSnap();
  const [text, setText] = useState("");
  const [caret, setCaret] = useState(0);
  const [mentionIdx, setMentionIdx] = useState(0);
  const [dismissedAt, setDismissedAt] = useState<string | null>(null);
  const [atts, setAtts] = useState<NewAttachment[]>([]);
  const [err, setErr] = useState<string | null>(null);
  const [allReplies, setAllReplies] = useState(false);
  const preparation = usePreparation(peer);
  const sending = preparation?.phase === "preparing";
  const ta = useRef<HTMLTextAreaElement>(null);
  const saveT = useRef<ReturnType<typeof setTimeout> | null>(null);
  const textRef = useRef(text);
  textRef.current = text;
  const refreshId = refreshKey("composer", getSnap().state?.me?.address ?? "", peer);
  const retainedRefresh = useRestoredRefreshState<{text:string;atts:NewAttachment[]}>(refreshId);
  useRefreshState(refreshId, () => {
    if (sending) throw Error("Wait for the message to finish preparing before refreshing.");
    return {text:textRef.current,atts:attsRef.current};
  });
  // typing heartbeats while our user writes (hub feature `typing`, user
  // 2026-10-13 22:33Z): an edit with text tells the core, at most every
  // 250 ms; a cleared box, a sent message, another chat or the app leaving
  // the front stops it. The core sends only the address, never the draft.
  const typingOn = useRef(false);
  const typingAt = useRef(0);
  const composing = (on: boolean, to = peer) => {
    if (!on) {
      if (typingOn.current) { typingOn.current = false; void api.composing(to, false).catch(() => {}); }
      return;
    }
    const t = performance.now();
    if (typingOn.current && t - typingAt.current < 250) return;
    typingOn.current = true; typingAt.current = t;
    void api.composing(to, true).catch(() => {});
  };
  const composingRef = useRef(composing);
  composingRef.current = composing;
  useEffect(() => {
    const away = () => { if (document.visibilityState !== "visible" || (platform === "desktop" && !document.hasFocus())) composingRef.current(false); };
    const gone = () => composingRef.current(false);
    window.addEventListener("blur", away); document.addEventListener("visibilitychange", away); window.addEventListener("pagehide", gone);
    return () => { window.removeEventListener("blur", away); document.removeEventListener("visibilitychange", away); window.removeEventListener("pagehide", gone); };
  }, [platform]);

  // load this chat's draft; flush the previous chat's on switch
  useEffect(() => {
    let live = true;
    setText(""); setCaret(0); setMentionIdx(0); setDismissedAt(null); setAtts([]); setErr(null);
    const saved = getPreparation(peer);
    const refreshed = retainedRefresh;
    if (refreshed) {
      setText(refreshed.text); setAtts(refreshed.atts); noteDraft(peer, refreshed.text);
      void api.setDraft(peer, refreshed.text).catch(() => {});
    } else if (saved && saved.phase !== "queued") {
      const draft = saved.retainedDraft ?? saved.message;
      setText(draft.body); setAtts(draft.attachments ?? []);
    }
    else api.draft(peer).then((d) => { if (live && d && !getPreparation(peer)) { setText(d); noteDraft(peer, d); } }, () => {});
    return () => {
      live = false;
      // leaving the chat ends its typing
      composingRef.current(false, peer);
      if (saveT.current) { clearTimeout(saveT.current); saveT.current = null; void api.setDraft(peer, textRef.current).catch(() => {}); }
    };
  }, [peer]);
  // Queued messages empty the box. Legacy retained preparation state remains
  // readable. Replies are left to onSent: Reply in the
  // chat still works while the files are prepared, and a reply chosen then
  // isn't part of this message (review R3)
  useEffect(() => {
    if (preparation?.phase !== "queued") return;
    setText(preparation.retainedDraft?.body ?? "");
    setCaret(0); setMentionIdx(0); setDismissedAt(null);
    setAtts(preparation.retainedDraft?.attachments ?? []); setErr(null);
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
    composing(!!v.trim());
    clearPreparation(peer);
    setText(v); setErr(null);
    noteDraft(peer, v);
    if (saveT.current) clearTimeout(saveT.current);
    saveT.current = setTimeout(() => { saveT.current = null; void api.setDraft(peer, v).catch(() => {}); }, 400);
  };

  const syncCaret = () => {
    const el = ta.current;
    if (el) setCaret(el.selectionStart ?? textRef.current.length);
  };
  const mentionTok = sending ? null : activeMentionToken(text, caret);
  const mentionKey = mentionTok ? `${mentionTok.start}:${mentionTok.query}` : null;
  useEffect(() => { setMentionIdx(0); }, [mentionKey]);
  const mentionItems = mentionTok && dismissedAt !== mentionKey
    ? filterMentionContacts(mentionContacts(snap), hubs, snap.chats, mentionTok.query)
    : [];
  const mentionOpen = mentionItems.length > 0;
  const activeMentionIdx = mentionOpen ? Math.min(mentionIdx, mentionItems.length - 1) : 0;

  const pickMention = (contact: Contact) => {
    if (!mentionTok || sending) return;
    const res = completeMentionText(text, mentionTok, contact);
    change(res.text);
    setCaret(res.caret);
    setDismissedAt(`${mentionTok.start}:${mentionTok.query}`);
    queueMicrotask(() => {
      const el = ta.current;
      if (el) {
        try {
          el.focus();
          el.setSelectionRange(res.caret, res.caret);
        } catch {}
      }
    });
  };

  const lim = limitFor(c, hubs);
  const total = utf8Len(text) + atts.reduce((n, a) => n + a.bytes, 0);
  const over = lim ? total > lim.bytes : false;
  const overNote = lim ? "This message is " + bytes(total) + ". Hub " + lim.hub.name + " takes up to " + bytes(lim.bytes) + " per message, text and files together." : "";
  // a message may go with no text when it answers or skips a question card
  // (user 2026-10-11 22:42Z): a picked option answers even without text, a
  // plain reply to an open card with no text skips it (files or not):
  // each must be known first, or that message waits and retains its draft
  // (review R2). Picks and text are exclusive per card, not per send.
  const plain = replies.filter((r) => !!cardOf(r) && !r.outgoing);
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
  useImperativeHandle(ref, () => ({ addFiles: (sources) => void addFiles(sources), focus: () => ta.current?.focus(), busy: () => sending }));
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
    composing(false);
    if (saveT.current) { clearTimeout(saveT.current); saveT.current = null; }
    // every card answered is also replied to; the whole set goes oldest first
    const all = [...replies];
    for (const p of picked) if (!all.some((r) => r.id === p.card.id)) all.push(p.card);
    all.sort((a, b) => (pos.get(a.id) ?? -1) - (pos.get(b.id) ?? -1) || msgTime(a) - msgTime(b));
    const ids = all.map((r) => r.id), prev = preparation?.retainedDraft ? undefined : preparation?.message;
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
      {mentionOpen ? (
        <div className="mention-list" role="listbox" aria-label="Mention a contact">
          {mentionItems.map((item, idx) => {
            const itemName = displayName(item, item.address);
            const active = idx === activeMentionIdx;
            return (
              <button
                key={item.address}
                type="button"
                role="option"
                aria-selected={active}
                data-mention-address={item.address}
                className={"mention-item" + (active ? " active" : "")}
                onMouseDown={(e) => e.preventDefault()}
                onClick={() => pickMention(item)}
              >
                <PeerAvatar c={item} address={item.address} hubs={hubs} size={24} />
                <span className="mention-name ell">{itemName}</span>
                <KindGlyph kind={kindOf(item)} />
                <span className="mention-addr mono ell">{"@net:" + item.address}</span>
              </button>
            );
          })}
        </div>
      ) : null}
    </>
  );
  const meta = over
    ? <span style={{ color: "var(--bad)" }}>{overNote}</span>
    : <>
        {platform === "desktop" ? <span><kbd>Enter</kbd> send · <kbd>Shift</kbd>+<kbd>Enter</kbd> new line</span> : null}
        {picked.length ? <span className="answer-note">Picked cards use their options; other reply targets use your message.</span> : null}
        {checking ? <span className="skipnote">Checking whether {checking === 1 ? "the question you reply to is" : checking + " questions you reply to are"} still open</span>
          : skips && !text.trim() ? <span className="skipnote">Sending with no text skips {skips === 1 ? "the question" : skips + " questions"} you reply to</span> : null}
        {isLong(text) ? <span className="count">{num(text.length)} characters · shows collapsed</span> : null}
      </>;
  const row = (
    <div className="comp-row">
      <div className="comp-box">
        <button className="icon-btn" disabled={sending} onClick={attach} title="Attach files" aria-label="Attach files"><Icon name="attach" /></button>
        <textarea disabled={sending} ref={ta} rows={1} value={text} spellCheck dir="auto" aria-label={"Message " + displayName(c, peer)} placeholder={"Message " + displayName(c, peer)}
          onChange={(e) => { setCaret(e.target.selectionStart ?? e.target.value.length); setDismissedAt(null); change(e.target.value); }}
          onSelect={syncCaret}
          onClick={syncCaret}
          onKeyUp={syncCaret}
          onPaste={(e) => { const imgs = pastedImages(e.clipboardData); if (imgs.length) { e.preventDefault(); void paste(imgs); } }}
          onKeyDown={(e) => {
            if (mentionOpen && !e.nativeEvent.isComposing) {
              if (e.key === "ArrowDown") { e.preventDefault(); e.stopPropagation(); setMentionIdx((i) => (i + 1) % mentionItems.length); return; }
              if (e.key === "ArrowUp") { e.preventDefault(); e.stopPropagation(); setMentionIdx((i) => (i - 1 + mentionItems.length) % mentionItems.length); return; }
              if ((e.key === "Tab" && !e.shiftKey) || (e.key === "Enter" && !e.shiftKey && !e.ctrlKey && !e.metaKey && !e.altKey)) {
                const chosen = mentionItems[activeMentionIdx];
                if (chosen) { e.preventDefault(); e.stopPropagation(); pickMention(chosen); return; }
              }
              if (e.key === "Escape") { e.preventDefault(); e.stopPropagation(); if (mentionKey) setDismissedAt(mentionKey); return; }
            }
            if (platform === "desktop" && e.key === "Enter" && !e.shiftKey && !e.nativeEvent.isComposing) { e.preventDefault(); void send(); }
            // Escape cancels the reply as its × does; an IME composition or an
            // open modal keeps the key
            else if (e.key === "Escape" && replies.length && !sending && !e.nativeEvent.isComposing
              && !document.querySelector(".modal, .dialog")) { e.stopPropagation(); e.preventDefault(); onCancelReply(); }
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
