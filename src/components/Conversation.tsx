// A conversation: header, hub banner, timeline (oldest to newest, day
// dividers, grouped bubbles) and the composer. On desktop, files dropped on
// it are attached.
import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { api, type Message } from "../api";
import { Icon } from "../lib/icons";
import { useFileDrop } from "../lib/drop";
import { bytes, dayLabel, dayStart, MAX_FILES } from "../lib/format";
import { copyText, errText } from "../lib/native";
import { displayName, hubByUrl, hubName, hubStatusText, kindInfo, kindOf, limitFor, msgTime, peerHubs, presence, preview, viaHub } from "../lib/peers";
import { refreshChats, useMessages, useSendRoute, useSnap } from "../lib/store";
import { toast } from "../lib/toast";
import { Composer, type ComposerApi } from "./Composer";
import { HubHelpLink } from "./HubHelp";
import { HubChip, routeLabel, routeVia } from "./HubPicker";
import { MessageView } from "./MessageView";
import { Addr, KindChip, KindGlyph, NoteCard, PeerAvatar, PresText, pressKeys, useNow, usePlatform } from "./ui";

const GROUP_GAP = 5 * 60000;
const CHIP_KINDS = new Set(["question", "request", "decision", "status"]);

interface Props {
  peer: string;
  onBack?: () => void;
  onInfo: (m: Message) => void;
  onOpenAddr: (address: string) => void;
  /** Contact info: desktop toggles the panel, Android opens the screen. */
  onContact?: () => void;
  /** Desktop: the info panel is open (its header button shows it). */
  infoOn?: boolean;
  /** Android: the app bar's menu (Contact info, Copy address). */
  onMenu?: () => void;
}

export function Conversation({ peer, onBack, onInfo, onOpenAddr, onContact, infoOn, onMenu }: Props) {
  const platform = usePlatform();
  const snap = useSnap();
  const hubs = snap.state?.hubs || [];
  const c = snap.byAddr.get(peer);
  const name = displayName(c, peer);
  const kind = kindOf(c);
  const { msgs, loaded, reload, more, loadingOlder, loadOlder } = useMessages(peer);
  const [replyTo, setReplyTo] = useState<Message | null>(null);
  const [sel, setSel] = useState<string | null>(null);
  // desktop keyboard: the message Shift+Tab has highlighted (R replies to it)
  const [hl, setHl] = useState<string | null>(null);
  const hlRef = useRef(hl);
  hlRef.current = hl;
  const [extra, setExtra] = useState<Record<string, Message | null>>({});
  const tl = useRef<HTMLDivElement>(null);
  const pinned = useRef(true);
  const [far, setFar] = useState(false);
  const newMark = useRef<{ peer: string; id: string | null } | null>(null);

  useEffect(() => { setReplyTo(null); setSel(null); setHl(null); pinned.current = true; }, [peer]);

  // "N new messages": where the unread messages began when the chat opened
  // (taken from the chat list's count, which a quick mark-read can't race)
  const [unreadAtOpen] = useState(() => snap.chats.find((x) => x.peer === peer)?.unread ?? 0);
  if (loaded && newMark.current?.peer !== peer) {
    const inc = msgs.filter((m) => !m.outgoing);
    const firstUnseen = msgs.find((m) => !m.outgoing && !m.seen)?.id ?? null;
    newMark.current = { peer, id: unreadAtOpen > 0 && inc.length >= unreadAtOpen ? inc[inc.length - unreadAtOpen].id : firstUnseen };
  }

  // reply quotes: from this chat, else fetched once by id
  const byId = useMemo(() => new Map(msgs.map((m) => [m.id, m])), [msgs]);
  useEffect(() => {
    const missing = msgs.filter((m) => m.reply_to && !byId.has(m.reply_to) && !(m.reply_to in extra)).map((m) => m.reply_to!);
    if (!missing.length) return;
    let live = true;
    Promise.all(missing.map((id) => api.message(id).then((r) => [id, r] as const, () => [id, null] as const))).then((rs) => {
      if (live) setExtra((x) => ({ ...x, ...Object.fromEntries(rs) }));
    });
    return () => { live = false; };
  }, [msgs, byId, extra]);
  const quoted = (id: string | null) => (id ? byId.get(id) ?? extra[id] : undefined);

  // keep pinned to the newest message while at the bottom
  useLayoutEffect(() => {
    const el = tl.current; if (!el || !loaded) return;
    const nm = newMark.current?.id && document.getElementById("m-" + newMark.current.id);
    if (nm && pinned.current === true && el.dataset.opened !== peer) { el.scrollTop = Math.max(0, nm.offsetTop - 120); pinned.current = false; }
    else if (pinned.current) el.scrollTop = el.scrollHeight;
    el.dataset.opened = peer;
  }, [msgs, loaded, peer, replyTo]);
  useEffect(() => {
    const el = tl.current; if (!el || !window.ResizeObserver) return;
    const ro = new ResizeObserver(() => { if (pinned.current) el.scrollTop = el.scrollHeight; });
    ro.observe(el); if (el.firstElementChild) ro.observe(el.firstElementChild);
    return () => ro.disconnect();
  }, [peer]);
  // older history a page at a time, as the top comes near (user 23:46Z),
  // keeping what is on screen where it was
  const anchor = useRef<{ h: number; t: number } | null>(null);
  const olderBusy = useRef(false);
  const older = useCallback(() => {
    const el = tl.current; if (!el || olderBusy.current) return;
    olderBusy.current = true;
    anchor.current = { h: el.scrollHeight, t: el.scrollTop };
    void loadOlder().then((p) => { if (!p.length) anchor.current = null; }).finally(() => { olderBusy.current = false; });
  }, [loadOlder]);
  useLayoutEffect(() => {
    const el = tl.current, a = anchor.current;
    if (!el || !a) return;
    anchor.current = null;
    el.scrollTop = el.scrollHeight - a.h + a.t;
  }, [msgs]);
  // a short page that doesn't fill the view: the next one, so it can scroll
  useEffect(() => {
    const el = tl.current;
    if (el && loaded && more && !loadingOlder && el.scrollHeight <= el.clientHeight + 40) older();
  }, [loaded, more, loadingOlder, msgs, older]);
  const onScroll = () => {
    const el = tl.current; if (!el) return;
    const gap = el.scrollHeight - el.scrollTop - el.clientHeight;
    pinned.current = gap < 80; setFar(gap > 300);
    if (el.scrollTop < 400 && more && !loadingOlder) older();
  };
  const toBottom = () => { const el = tl.current; if (el) { el.scrollTop = el.scrollHeight; pinned.current = true; setFar(false); } };

  const flash = (el: HTMLElement, smooth = true) => {
    el.scrollIntoView({ block: "center", behavior: smooth ? "smooth" : "auto" });
    el.classList.remove("flash"); void el.offsetWidth; el.classList.add("flash");
    setTimeout(() => el.classList.remove("flash"), 1500);
  };
  // a reply's original further back: older pages until it shows
  const jumpTo = useRef<string | null>(null);
  const jump = useCallback(async (id: string) => {
    const el = document.getElementById("m-" + id);
    if (el) { flash(el); return; }
    jumpTo.current = id;
    let before: Message | undefined;
    for (let i = 0; i < 400; i++) {
      const p = await loadOlder(before);
      if (!p.length || p.some((m) => m.id === id)) { if (!p.some((m) => m.id === id)) { jumpTo.current = null; toast("That message isn't in this chat on this device"); } break; }
      before = p[0];
    }
  }, [loadOlder]);
  useLayoutEffect(() => {
    const id = jumpTo.current; if (!id) return;
    const el = document.getElementById("m-" + id);
    if (el) { jumpTo.current = null; pinned.current = false; flash(el, false); }
  }, [msgs]);
  const del = useCallback((m: Message) => {
    api.deleteMessage(m.id).then(() => { reload(); void refreshChats(); setSel(null); }, (e) => toast(errText(e)));
  }, [reload]);
  const reply = useCallback((m: Message) => { setReplyTo(m); setSel(null); }, []);
  const info = useCallback((m: Message) => { setSel(null); onInfo(m); }, [onInfo]);

  // ------------------------------------- desktop: Shift+Tab walks the bubbles
  // (user 2026-10-09 08:38Z). From the message box the first Shift+Tab
  // highlights the newest message and the focus moves to the timeline, so R
  // and the other keys never type into the box; Shift+Tab goes older, Tab
  // newer, Tab past the newest and Esc go back to the box, R replies. Only
  // bubbles count (msgs): no day lines, "new messages" line or system notes.
  const growFrom = useRef<string | null>(null);
  const show = useCallback((id: string) => {
    setHl(id);
    document.getElementById("m-" + id)?.scrollIntoView({ block: "nearest" });
  }, []);
  const leaveHl = () => { growFrom.current = null; setHl(null); };
  const backToBox = () => { leaveHl(); comp.current?.focus(); };
  // the right-click menu of the highlighted message, from the keyboard (Menu key, Shift+F10)
  const menuOnHl = () => {
    const bub = hlRef.current ? document.querySelector<HTMLElement>("#m-" + hlRef.current + " .bubble") : null;
    const view = tl.current?.getBoundingClientRect();
    if (!bub || !view) return;
    const r = bub.getBoundingClientRect();
    bub.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, cancelable: true, clientX: Math.round(r.left + 24), clientY: Math.round(Math.min(Math.max(r.top, view.top) + 24, view.bottom - 8)) }));
  };
  const keys = (e: React.KeyboardEvent) => {
    if (platform !== "desktop") return;
    if (e.nativeEvent.isComposing || e.ctrlKey || e.altKey || e.metaKey) return;
    const t = e.target as HTMLElement, cur = hlRef.current;
    if (!cur) {
      // the default order would go to the attach button: never from here
      if (e.key === "Tab" && e.shiftKey && t.matches(".composer textarea")) {
        e.preventDefault();
        const last = msgs[msgs.length - 1];
        if (last) { show(last.id); tl.current?.focus({ preventScroll: true }); }
      }
      return;
    }
    if (!tl.current?.contains(t)) return;
    const i = msgs.findIndex((m) => m.id === cur);
    if (i < 0) { leaveHl(); return; }
    if (e.key === "Tab") {
      e.preventDefault(); growFrom.current = null;
      if (e.shiftKey) {
        if (i > 0) show(msgs[i - 1].id);
        // at the oldest loaded one: fetch the next page now (scrolling to it
        // may not move anything, so the scroll handler wouldn't), and step
        // onto its newest message once it is in
        else if (more) { growFrom.current = msgs[0].id; older(); }
      } else if (i < msgs.length - 1) show(msgs[i + 1].id);
      else backToBox();
    } else if (e.key === "ContextMenu" || (e.key === "F10" && e.shiftKey)) {
      e.preventDefault(); menuOnHl();
    } else if (e.key === "Escape") {
      // only the highlight goes, not the info panel behind it
      e.preventDefault(); e.stopPropagation(); backToBox();
    } else if (e.key.toLowerCase() === "r" && e.key.length === 1) {
      e.preventDefault();
      reply(msgs[i]); backToBox();
    }
  };
  // screen readers: what the highlight is on, and the newest incoming message
  // of the open chat (not for history, not in the background, one every 4 s)
  const [said, setSaid] = useState("");
  const lastIn = useRef<string | null | undefined>(undefined);
  const saidAt = useRef(0);
  useEffect(() => { lastIn.current = undefined; }, [peer]);
  useEffect(() => {
    if (!loaded) return;
    const inc = [...msgs].reverse().find((m) => !m.outgoing);
    if (lastIn.current === undefined) { lastIn.current = inc?.id ?? null; return; }
    if (!inc || inc.id === lastIn.current) return;
    lastIn.current = inc.id;
    if (document.visibilityState !== "visible" || Date.now() - saidAt.current < 4000) return;
    saidAt.current = Date.now();
    setSaid(displayName(c, peer) + ": " + preview(inc).slice(0, 200));
  }, [msgs, loaded]);
  useEffect(() => {
    const m = hl ? msgs.find((x) => x.id === hl) : null;
    if (m) setSaid((m.outgoing ? "You" : displayName(c, peer)) + ": " + preview(m).slice(0, 200));
  }, [hl]);
  useEffect(() => {
    const from = growFrom.current;
    if (!from || !hlRef.current || msgs[0]?.id === from) return;
    growFrom.current = null;
    const j = msgs.findIndex((m) => m.id === from);
    if (j > 0) show(msgs[j - 1].id);
  }, [msgs, show]);

  // the banner: none of this peer's hubs is connected
  const ph = peerHubs(c, hubs);
  const down = ph.length && !ph.some((h) => h.state === "connected") ? ph[0] : null;
  const now = useNow(!!down);
  // the hub the core really sends through (the hub picker), not a guess
  const route = useSendRoute(peer);
  const routed = routeVia(route);
  const via = (routed && hubByUrl(hubs, routed)) || viaHub(c, hubs) || ph[0];
  const viaLabel = routed ? routeLabel(route, hubs) : via ? "via " + via.name : "";

  // ------------------------------------------------------------ timeline
  const rows: React.ReactNode[] = [];
  if (loadingOlder) rows.push(<div className="tl-older" key="older" aria-label="Loading earlier messages"><span className="spin" /></div>);
  if (loaded && !msgs.length) {
    const k = kindInfo(kind);
    rows.push(
      <div className="chat-start" key="start">
        <PeerAvatar address={peer} c={c} hubs={hubs} size={72} withPres={false} />
        <b>{name}</b>
        <div className="mono" style={{ fontSize: 12.5, overflowWrap: "anywhere" }}>@net:{peer}</div>
        {/* one plain line (user 2026-10-09 07:26Z) */}
        <NoteCard icon={c ? k.icon : "search"}>{c ? k.intro : "Not on any mail hub you're connected to."}</NoteCard>
      </div>,
    );
  }
  let day = 0; let prev: Message | null = null;
  for (const m of msgs) {
    const t = msgTime(m); const d = dayStart(t);
    if (d !== day) { rows.push(<div className="day" key={"d" + d}><span>{dayLabel(t)}</span></div>); day = d; prev = null; }
    if (newMark.current?.id === m.id) {
      const n = msgs.filter((x) => !x.outgoing && msgTime(x) >= t).length;
      rows.push(<div className="newmark" key="new">{n} new message{n > 1 ? "s" : ""}</div>);
    }
    const first = !prev || prev.outgoing !== m.outgoing || t - msgTime(prev) > GROUP_GAP || (!m.outgoing && !!m.kind && CHIP_KINDS.has(m.kind));
    rows.push(
      <MessageView key={m.id} m={m} first={first} peerName={name} peerKind={kind} quoted={quoted(m.reply_to)} hover={platform === "desktop"} selected={sel === m.id} highlighted={hl === m.id}
        onReply={reply} onInfo={info} onDelete={del} onJump={jump} onOpenAddr={onOpenAddr} />,
    );
    prev = m;
  }
  const p = presence(c, hubs);
  if (p.state === "offline" && msgs.some((m) => m.outgoing && m.state === "sent")) {
    rows.push(<div className="sysnote" key="pend"><Icon name="schedule" />{name} is offline ({p.short}). Your message waits on hub {via?.name} and is delivered when they reconnect.</div>);
  }
  const waiting = msgs.some((m) => m.outgoing && (m.state === "queued" || m.state === "sending"));
  if (route?.pinned && !route.next) {
    // a pinned hub that can't reach them: say so, never switch on our own
    rows.push(<div className="sysnote" key="pinwait"><Icon name="schedule" />Messages to {name} wait for hub {hubName(hubs, route.pinned)}, which can't reach them right now. <button className="link" onClick={() => api.setSendHub(peer, null).catch((e) => toast(errText(e)))}>Use Automatic</button></div>);
  } else if (waiting && !viaHub(c, hubs)) {
    rows.push(<div className="sysnote" key="wait"><Icon name="schedule" />Waiting for a connection. Queued messages go out on their own.</div>);
  }

  // ------------------------------------------- android: long-press / swipe
  const gesture = useRef<{ id: string; row: HTMLElement; ic: HTMLElement | null; x: number; y: number; dx: number; swiping: boolean; lp: ReturnType<typeof setTimeout>; done: boolean } | null>(null);
  const suppressClick = useRef(false);
  const touch = platform === "android" ? {
    onPointerDown: (e: React.PointerEvent) => {
      const t = e.target as HTMLElement;
      const msg = t.closest(".msg") as HTMLElement | null;
      if (!msg || t.closest("button, a, .att-file, .quote")) return;
      const id = msg.dataset.id!;
      gesture.current = { id, row: msg.querySelector(".row") as HTMLElement, ic: msg.querySelector(".swipe-ic"), x: e.clientX, y: e.clientY, dx: 0, swiping: false, done: false,
        lp: setTimeout(() => { const g = gesture.current; if (g && !g.swiping) { g.done = true; setSel(id); } }, 450) };
    },
    onPointerMove: (e: React.PointerEvent) => {
      const g = gesture.current; if (!g) return;
      const dx = e.clientX - g.x; const dy = e.clientY - g.y;
      if (!g.swiping && Math.abs(dy) > 8) { clearTimeout(g.lp); gesture.current = null; return; }
      if (!g.swiping && dx > 10) { g.swiping = true; clearTimeout(g.lp); }
      if (g.swiping) { g.dx = Math.max(0, Math.min(84, dx)); g.row.style.transform = "translateX(" + g.dx + "px)"; if (g.ic) g.ic.style.opacity = String(Math.min(1, g.dx / 60)); }
    },
    onPointerUp: () => endGesture(),
    onPointerCancel: () => endGesture(),
    onClick: (e: React.MouseEvent) => {
      // the click that ends a long-press (or a swipe) is not a tap
      if (suppressClick.current) { suppressClick.current = false; return; }
      if (!sel || (e.target as HTMLElement).closest("button, a")) return;
      const msg = (e.target as HTMLElement).closest(".msg") as HTMLElement | null;
      if (msg) setSel(sel === msg.dataset.id ? null : msg.dataset.id!);
    },
  } : {};
  function endGesture() {
    const g = gesture.current; if (!g) return;
    clearTimeout(g.lp);
    suppressClick.current = g.done || g.swiping;
    g.row.style.transition = "transform .18s"; g.row.style.transform = "";
    if (g.ic) g.ic.style.opacity = "0";
    setTimeout(() => { g.row.style.transition = ""; }, 200);
    if (g.swiping && g.dx > 56) { const m = byId.get(g.id); if (m) setReplyTo(m); }
    gesture.current = null;
  }

  // no hub you're connected to lists this address: nothing can reach it
  // (an empty directory, say before any hub answered, blocks nothing)
  const off = !c && snap.directory.length > 0 ? name + " isn't on any of the mail hubs you're connected to." : null;
  const comp = useRef<ComposerApi>(null);
  const conv = useRef<HTMLElement>(null);
  const dragging = useFileDrop(conv, platform === "desktop" && !off, (paths) => comp.current?.addFiles(paths));
  const composer = <Composer ref={comp} peer={peer} c={c} hubs={hubs} replyTo={replyTo} onCancelReply={() => setReplyTo(null)} onSent={() => { pinned.current = true; reload(); }} off={off} />;
  const jumpBtn = <button className={"jump" + (far ? "" : " hide")} onClick={toBottom} title="Jump to the newest message" aria-label="Jump to the newest message"><Icon name="arrow_down" /></button>;
  const copyAddr = () => copyText("@net:" + peer, "Address copied");

  if (platform === "android") {
    const selMsg = sel ? byId.get(sel) : undefined;
    return (
      <div className="scr" role="main">
        {selMsg ? (
          <div className="appbar select">
            <button className="icon-btn" onClick={() => setSel(null)} aria-label="Cancel"><Icon name="close" /></button>
            <div className="title">1 selected</div>
            <button className="icon-btn" onClick={() => reply(selMsg)} aria-label="Reply"><Icon name="reply" /></button>
            <button className="icon-btn" onClick={() => { void copyText(selMsg.body, "Copied"); setSel(null); }} aria-label="Copy"><Icon name="copy" /></button>
            <button className="icon-btn" onClick={() => info(selMsg)} aria-label="Message info"><Icon name="info" /></button>
          </div>
        ) : (
          <div className="appbar">
            <button className="icon-btn" onClick={onBack} aria-label="Back"><Icon name="back" /></button>
            <div className="who" onClick={onContact} role="button" tabIndex={0} title="Contact info" onKeyDown={onContact ? pressKeys(onContact) : undefined}>
              <PeerAvatar address={peer} c={c} hubs={hubs} size={40} />
              <div className="t">
                <span className="n"><span className="ell">{name}</span> <KindGlyph kind={kind} /></span>
                <span className="s">{p.state === "disconnected" ? <Icon name="cloud_off" /> : null}{p.state === "online" ? "online" : p.short}{viaLabel && hubs.length > 1 ? " · " + viaLabel : ""}</span>
              </div>
            </div>
            <button className="icon-btn" onClick={onMenu} aria-label="More"><Icon name="more_vert" /></button>
          </div>
        )}
        {down ? (
          <div className={"strip warn" + (down.state === "refused" ? " bad" : "")}>
            <Icon name="warning" /><span><b>{down.state === "connecting" ? "Connecting to " + down.name + "…" : "Can't reach hub " + down.name}</b>{down.state === "disconnected" ? " · " + hubStatusText(down, now).replace("Can't reach this hub · ", "") : down.state === "refused" ? " · " + (down.error || "refused") : ""}</span>
            {down.state === "disconnected" ? <HubHelpLink /> : null}{down.state !== "connecting" ? <button className="link" onClick={() => api.retryNow()}>Retry</button> : null}
          </div>
        ) : null}
        <div className="timeline" ref={tl} onScroll={onScroll} {...touch}><div>{rows}</div></div>
        {jumpBtn}
        {composer}
      </div>
    );
  }

  return (
    <section className="conv" role="main" ref={conv} onKeyDown={keys}>
      <div className="conv-head">
        <span className="conv-av" onClick={onContact}><PeerAvatar address={peer} c={c} hubs={hubs} size={40} /></span>
        <div className="who" onClick={onContact} title="Contact info">
          <div className="n1"><span className="name">{name}</span>{c ? <KindChip kind={kind} /> : null}</div>
          <div className="n2">
            <PresText c={c} hubs={hubs} />
            <span className="sep" />
            <span className="mono ell"><Addr a={peer} net /></span>
            {routed ? <><span className="sep" /><HubChip peer={peer} route={route} who={name} /></>
              : via ? <><span className="sep" /><span className="chip hubchip" title={"Messages go through hub " + via.name}>via {via.name}</span></> : null}
          </div>
        </div>
        <button className="icon-btn" onClick={copyAddr} title="Copy address" aria-label="Copy address"><Icon name="copy" /></button>
        <button className={"icon-btn" + (infoOn ? " on" : "")} onClick={onContact} title="Contact info (Ctrl+I)" aria-label="Contact info" aria-pressed={!!infoOn}><Icon name="info" /></button>
      </div>
      <div className="tl-wrap">
        <div className="timeline scroll" ref={tl} onScroll={onScroll} tabIndex={platform === "desktop" ? -1 : undefined} role="region" aria-label={"Messages with " + name}
          onContextMenu={(e) => { if (hlRef.current && e.target === e.currentTarget) { e.preventDefault(); menuOnHl(); } }}
          onMouseDown={() => { if (hlRef.current) leaveHl(); }}
          onBlur={(e) => { if (hlRef.current && document.hasFocus() && !e.currentTarget.contains(e.relatedTarget as Node | null)) leaveHl(); }}><div className="tl-inner">{rows}</div></div>
        {jumpBtn}
        {dragging ? <DropZone lim={limitFor(c, hubs)} /> : null}
      </div>
      {composer}
      <div className="sr" role="status" aria-live="polite">{said}</div>
    </section>
  );
}

/** Shown over the timeline while files are dragged over the chat. */
function DropZone({ lim }: { lim: ReturnType<typeof limitFor> }) {
  return (
    <div className="dropzone">
      <Icon name="upload" />Drop files to attach
      <span>Up to {MAX_FILES} files{lim ? <> and {bytes(lim.bytes)} per message on hub {lim.hub.name}, text included</> : " per message"}</span>
    </div>
  );
}
