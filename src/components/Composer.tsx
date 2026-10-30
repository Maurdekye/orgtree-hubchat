// The composer: per-chat drafts, the messages it replies to (one, or several),
// options picked on question cards, attachments (up to 10, picked or dropped
// on the chat) and a size check against the hub's limit before anything is
// sent.
import { useEffect, useImperativeHandle, useLayoutEffect, useMemo, useRef, useState, type Ref } from "react";
import { api, newId, type Contact, type HubStatus, type Message, type NewAttachment, type NewOutgoing } from "../api";
import { Icon } from "../lib/icons";
import { bytes, fileTitle, isLong, MAX_FILES, num, utf8Len } from "../lib/format";
import { isImage, isPastedImage, MAX_PASTED_BYTES, pastedFiles, pastedName, thumbable, useFilePreview } from "../lib/images";
import { errText, pickFiles } from "../lib/native";
import { displayName, kindOf, limitFor, msgTime, preview } from "../lib/peers";
import {
  activeMentionToken,
  completeMentionText,
  extractMentionRecency,
  filterMentionContacts,
  mentionContacts,
  MENU_STEP,
  useAccountMentions,
} from "../lib/mentions";
import { foldAt } from "../lib/replies";
import { cardOf, type CardKnown } from "../lib/questions";
import { noteDraft, usePreparation, getPreparation, prepareSend, clearPreparation, getSnap, useSnap } from "../lib/store";
import { KindGlyph, PeerAvatar, usePlatform } from "./ui";
import { refreshKey, useRestoredRefreshState, useRefreshState } from "../lib/refresh";
import { sendsOnEnter, useEnterKey } from "../lib/enterKey";

interface Props {
  peer: string;
  c: Contact | undefined;
  hubs: HubStatus[];
  /** Messages currently loaded in the open conversation, oldest first. */
  msgs?: readonly Message[];
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

function keepMentionItemInView(listEl: HTMLElement, activeEl: HTMLElement | null): void {
  if (!activeEl) return;
  if (listEl.clientHeight > 0 && activeEl.offsetHeight > 0) {
    const itemTop = activeEl.offsetTop;
    const itemBottom = itemTop + activeEl.offsetHeight;
    if (itemTop < listEl.scrollTop) {
      listEl.scrollTop = itemTop;
    } else if (itemBottom > listEl.scrollTop + listEl.clientHeight) {
      listEl.scrollTop = itemBottom - listEl.clientHeight;
    }
    return;
  }
  activeEl.scrollIntoView?.({ block: "nearest" });
}

export function Composer({ peer, c, hubs, msgs, replies, picked, cardState, pos, onRemoveReply, onUnpick, onCancelReply, onSent, ref, off }: Props) {
  const platform = usePlatform();
  const enter = useEnterKey();
  const snap = useSnap();
  const [text, setText] = useState("");
  const [caret, setCaret] = useState(0);
  const [selection, setSelection] = useState<{ key: string | null; index: number; pick: string | null }>({
    key: null,
    index: -1,
    pick: null,
  });
  const [shownState, setShownState] = useState<{ key: string | null; count: number }>({
    key: null,
    count: MENU_STEP,
  });
  const [below, setBelow] = useState(false);
  const [dismissedAt, setDismissedAt] = useState<string | null>(null);
  const [atts, setAtts] = useState<NewAttachment[]>([]);
  const [err, setErr] = useState<string | null>(null);
  const [allReplies, setAllReplies] = useState(false);
  const preparation = usePreparation(peer);
  const sending = preparation?.phase === "preparing";
  const ta = useRef<HTMLTextAreaElement>(null);
  const listRef = useRef<HTMLDivElement>(null);
  const moreRef = useRef<HTMLDivElement>(null);
  const mentionSessionRef = useRef(0);
  const wasMentionOpenRef = useRef(false);
  const prevSessionKeyRef = useRef<string | null>(null);
  const prependAnchorRef = useRef<{ scrollHeight: number; scrollTop: number } | null>(null);
  const scrollSnapRef = useRef<{ scrollHeight: number; scrollTop: number } | null>(null);
  const userScrolledRef = useRef(false);
  const lastScrollSourceRef = useRef<"init" | "user" | "keyboard">("init");
  const programmaticScrollTopRef = useRef<number | null>(null);
  const keyMoveSeqRef = useRef(0);
  const ackedKeyMoveSeqRef = useRef(0);
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
    setText(""); setCaret(0); setSelection({ key: null, index: -1, pick: null }); setShownState({ key: null, count: MENU_STEP }); setDismissedAt(null); setAtts([]); setErr(null);
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
    setCaret(0); setSelection({ key: null, index: -1, pick: null }); setShownState({ key: null, count: MENU_STEP }); setDismissedAt(null);
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
  const canShowMention = !sending && mentionTok !== null && dismissedAt !== mentionKey;
  const contacts = useMemo(
    () => mentionContacts(snap),
    [snap.directory, snap.chats, snap.byAddr, snap.state?.me?.address],
  );
  const recencySessionRef = useRef<{
    start: number;
    msgs: readonly Message[] | undefined;
    contacts: Contact[];
    recency: ReadonlyMap<string, number>;
  } | null>(null);
  if (!canShowMention || !mentionTok) {
    recencySessionRef.current = null;
  } else if (
    recencySessionRef.current === null ||
    recencySessionRef.current.start !== mentionTok.start ||
    recencySessionRef.current.msgs !== msgs ||
    recencySessionRef.current.contacts !== contacts
  ) {
    recencySessionRef.current = {
      start: mentionTok.start,
      msgs,
      contacts,
      recency: extractMentionRecency(msgs ?? [], contacts, snap),
    };
  }
  const activeRecency = recencySessionRef.current?.recency;
  const mentioned = useAccountMentions(canShowMention, snap.chats, contacts, peer, msgs, snap.state?.me?.address);
  const rankedMentions = useMemo(
    () =>
      canShowMention && mentionTok
        ? filterMentionContacts(contacts, hubs, snap.chats, mentionTok.query, activeRecency, mentioned)
        : [],
    [canShowMention, mentionTok, contacts, hubs, snap.chats, activeRecency, mentioned],
  );
  const hasMentions = rankedMentions.length > 0;
  const sessionTokenKey = mentionTok && mentionKey ? `${text.slice(0, mentionTok.start)}:${mentionKey}` : null;
  if (!hasMentions || !mentionKey || !sessionTokenKey) {
    wasMentionOpenRef.current = false;
    prevSessionKeyRef.current = null;
  } else if (!wasMentionOpenRef.current || prevSessionKeyRef.current !== sessionTokenKey) {
    wasMentionOpenRef.current = true;
    prevSessionKeyRef.current = sessionTokenKey;
    mentionSessionRef.current += 1;
    prependAnchorRef.current = null;
    scrollSnapRef.current = null;
    userScrolledRef.current = false;
    lastScrollSourceRef.current = "init";
    programmaticScrollTopRef.current = null;
  }
  const sessionKey = hasMentions && mentionKey ? `${mentionSessionRef.current}:${mentionKey}` : null;
  const shown = sessionKey && shownState.key === sessionKey ? shownState.count : MENU_STEP;
  // Drawn bottom-up (the best match last, next to the box), oldest first.
  const mentionItems = useMemo(() => rankedMentions.slice(0, shown).reverse(), [rankedMentions, shown]);
  const mentionOpen = mentionItems.length > 0;
  const moreMentions = rankedMentions.length > shown;
  const capturePrependAnchor = () => {
    if (prependAnchorRef.current) return;
    const el = listRef.current;
    if (el) {
      prependAnchorRef.current = { scrollHeight: el.scrollHeight, scrollTop: el.scrollTop };
    } else if (scrollSnapRef.current) {
      prependAnchorRef.current = { ...scrollSnapRef.current };
    }
  };
  const extendMentions = () => {
    if (!sessionKey || rankedMentions.length <= shown) return;
    capturePrependAnchor();
    setShownState({ key: sessionKey, count: shown + MENU_STEP });
  };
  const hasExplicitSelection = sessionKey !== null && selection.key === sessionKey;
  const activeMentionIdx = useMemo(() => {
    if (mentionItems.length === 0) return 0;
    if (!hasExplicitSelection) return mentionItems.length - 1;
    if (selection.pick) {
      const found = mentionItems.findIndex((item) => item.address === selection.pick);
      if (found >= 0) return found;
    }
    if (selection.index >= 0 && selection.index < mentionItems.length) {
      return selection.index;
    }
    return mentionItems.length - 1;
  }, [mentionItems, hasExplicitSelection, selection]);

  useLayoutEffect(() => {
    if (!mentionOpen) {
      setBelow(false);
      return;
    }
    const el = ta.current;
    if (!el) return;
    const update = () => {
      const r = el.getBoundingClientRect();
      const viewHeight = (el.ownerDocument?.defaultView ?? window).innerHeight;
      if (!viewHeight || (r.top === 0 && r.bottom === 0 && r.height === 0)) {
        setBelow(false);
        return;
      }
      const aboveRoom = r.top;
      const belowRoom = viewHeight - r.bottom;
      setBelow(aboveRoom < 224 && belowRoom > aboveRoom);
    };
    update();
    window.addEventListener("resize", update);
    window.addEventListener("scroll", update, true);
    return () => {
      window.removeEventListener("resize", update);
      window.removeEventListener("scroll", update, true);
    };
  }, [mentionOpen, text]);

  useLayoutEffect(() => {
    if (!mentionOpen || !listRef.current) return;
    const listEl = listRef.current;
    const anchor = prependAnchorRef.current;
    prependAnchorRef.current = null;
    const hasKeyMove = keyMoveSeqRef.current !== ackedKeyMoveSeqRef.current;
    if (hasKeyMove) {
      ackedKeyMoveSeqRef.current = keyMoveSeqRef.current;
    }
    if (anchor && listEl.scrollHeight > anchor.scrollHeight) {
      listEl.scrollTop = anchor.scrollTop + (listEl.scrollHeight - anchor.scrollHeight);
      programmaticScrollTopRef.current = listEl.scrollTop;
    } else if (!hasExplicitSelection && !userScrolledRef.current) {
      const maxScroll = listEl.clientHeight > 0 ? Math.max(0, listEl.scrollHeight - listEl.clientHeight) : listEl.scrollHeight;
      listEl.scrollTop = below ? 0 : maxScroll;
      programmaticScrollTopRef.current = listEl.scrollTop;
    }
    if (hasKeyMove) {
      const active = listEl.querySelector(".mention-item.active") as HTMLElement | null;
      const prevTop = listEl.scrollTop;
      keepMentionItemInView(listEl, active);
      if (listEl.scrollTop !== prevTop) {
        programmaticScrollTopRef.current = listEl.scrollTop;
      }
    }
    scrollSnapRef.current = { scrollHeight: listEl.scrollHeight, scrollTop: listEl.scrollTop };
  }, [mentionOpen, sessionKey, mentionItems, activeMentionIdx, hasExplicitSelection, below]);

  const extendMentionsRef = useRef(extendMentions);
  extendMentionsRef.current = extendMentions;
  useEffect(() => {
    const root = listRef.current;
    const marker = moreRef.current;
    const Observer = root?.ownerDocument?.defaultView?.IntersectionObserver;
    if (!mentionOpen || !moreMentions || !root || !marker || !Observer) return;
    const seen = new Observer(
      (entries) => {
        if (entries.some((e) => e.isIntersecting) && lastScrollSourceRef.current !== "keyboard") {
          extendMentionsRef.current();
        }
      },
      { root },
    );
    seen.observe(marker);
    return () => seen.disconnect();
  }, [mentionOpen, moreMentions, mentionItems.length]);

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
  // a file pasted into the box joins the attachments, an image since user 23:40Z and any file now (symmetry audit
  // ux10 S4, as Orgtree's composer); text still pastes as text. A file is read into memory to hand it to the app, so
  // one over MAX_PASTED_BYTES is refused with a pointer to the picker.
  const paste = async (files: File[]) => {
    const when = new Date();
    const paths: string[] = [];
    for (const [i, f] of files.entries()) {
      const image = isPastedImage(f);
      const name = image ? pastedName(f.type, when, i + 1) : f.name || "pasted-file";
      const label = image ? "pasted image" : "\u201c" + name + "\u201d";
      if (f.size > MAX_PASTED_BYTES) { setErr("Can't attach " + label + ": it is too large to paste (over " + bytes(MAX_PASTED_BYTES) + "). Attach it with the paperclip or drop it on the chat."); continue; }
      try { paths.push(await api.savePasted(name, new Uint8Array(await f.arrayBuffer()))); }
      catch (e) { setErr("Can't attach " + label + ": " + errText(e)); }
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
  const mentionList = mentionOpen ? (
    <div
      ref={listRef}
      className={"mention-list" + (below ? " below" : "")}
      role="listbox"
      aria-label="Mention a contact"
      onScroll={(e) => {
        const el = e.currentTarget;
        if (programmaticScrollTopRef.current !== null && Math.abs(el.scrollTop - programmaticScrollTopRef.current) <= 1) {
          programmaticScrollTopRef.current = null;
          scrollSnapRef.current = { scrollHeight: el.scrollHeight, scrollTop: el.scrollTop };
          return;
        }
        programmaticScrollTopRef.current = null;
        userScrolledRef.current = true;
        lastScrollSourceRef.current = "user";
        scrollSnapRef.current = { scrollHeight: el.scrollHeight, scrollTop: el.scrollTop };
        if (el.scrollTop <= 40) extendMentions();
      }}
    >
      {moreMentions ? <div ref={moreRef} className="mention-more" aria-hidden="true" /> : null}
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
  ) : null;
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
      {!below ? mentionList : null}
    </>
  );
  const meta = over
    ? <span style={{ color: "var(--bad)" }}>{overNote}</span>
    : <>
        {platform === "desktop" ? (enter === "newline"
          ? <span><kbd>Ctrl</kbd>+<kbd>Enter</kbd> send · <kbd>Enter</kbd> new line</span>
          : <span><kbd>Enter</kbd> send · <kbd>Shift</kbd>+<kbd>Enter</kbd> new line</span>) : null}
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
          onPaste={(e) => { const pasted = pastedFiles(e.clipboardData); if (pasted.length) { e.preventDefault(); void paste(pasted); } }}
          onKeyDown={(e) => {
            if (mentionOpen && !e.nativeEvent.isComposing) {
              if (e.key === "ArrowDown" || e.key === "ArrowUp") {
                e.preventDefault();
                e.stopPropagation();
                const step = (e.key === "ArrowDown") === !below ? 1 : -1;
                if (step === 1) {
                  if (activeMentionIdx >= mentionItems.length - 1) return;
                  const next = activeMentionIdx + 1;
                  lastScrollSourceRef.current = "keyboard";
                  keyMoveSeqRef.current += 1;
                  setSelection({ key: sessionKey, index: next, pick: mentionItems[next]?.address ?? null });
                  return;
                }
                if (activeMentionIdx > 0) {
                  const next = activeMentionIdx - 1;
                  lastScrollSourceRef.current = "keyboard";
                  keyMoveSeqRef.current += 1;
                  setSelection({ key: sessionKey, index: next, pick: mentionItems[next]?.address ?? null });
                  return;
                }
                if (sessionKey && rankedMentions.length > shown) {
                  const nextCount = shown + MENU_STEP;
                  const nextLen = Math.min(rankedMentions.length, nextCount);
                  const nextTarget = rankedMentions[shown];
                  const nextIdx = Math.max(0, nextLen - 1 - shown);
                  capturePrependAnchor();
                  lastScrollSourceRef.current = "keyboard";
                  keyMoveSeqRef.current += 1;
                  setShownState({ key: sessionKey, count: nextCount });
                  setSelection({ key: sessionKey, index: nextIdx, pick: nextTarget?.address ?? null });
                }
                return;
              }
              if ((e.key === "Tab" && !e.shiftKey) || (e.key === "Enter" && !e.shiftKey && !e.ctrlKey && !e.metaKey && !e.altKey)) {
                const chosen = mentionItems[activeMentionIdx];
                if (chosen) { e.preventDefault(); e.stopPropagation(); pickMention(chosen); return; }
              }
              if (e.key === "Escape") {
                e.preventDefault();
                e.stopPropagation();
                setSelection({ key: null, index: -1, pick: null });
                if (mentionKey) setDismissedAt(mentionKey);
                return;
              }
            }
            if (platform === "desktop" && sendsOnEnter(enter, { key: e.key, shiftKey: e.shiftKey, ctrlKey: e.ctrlKey, metaKey: e.metaKey, isComposing: e.nativeEvent.isComposing })) { e.preventDefault(); void send(); }
            // Escape cancels the reply as its × does; an IME composition or an
            // open modal keeps the key
            else if (e.key === "Escape" && replies.length && !sending && !e.nativeEvent.isComposing
              && !document.querySelector(".modal, .dialog")) { e.stopPropagation(); e.preventDefault(); onCancelReply(); }
          }} />
      </div>
      {/* pressing Send leaves the focus (and Android's keyboard) in the message box */}
      <button className="sendbtn" disabled={!can} onMouseDown={(e) => e.preventDefault()} onClick={send}title={platform === "desktop" ? (enter === "newline" ? "Send (Ctrl+Enter)" : "Send (Enter)") : "Send"} aria-label="Send"><Icon name="send" /></button>
    </div>
  );
  if (off && !sending) {
    const note = <div className="comp-off" role="status"><Icon name="cloud_off" /><span>{off}</span></div>;
    return platform === "android" ? <div className="composer">{note}</div> : <div className="composer"><div className="composer-in">{note}</div></div>;
  }
  if (platform === "android") return <div className="composer">{top}{row}{below ? mentionList : null}<div className="comp-meta">{meta}</div></div>;
  return <div className="composer"><div className="composer-in">{top}{row}{below ? mentionList : null}<div className="comp-meta">{meta}</div></div></div>;
}
