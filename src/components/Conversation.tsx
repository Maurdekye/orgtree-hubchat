// A conversation: header, hub banner, timeline (oldest to newest, day
// dividers, grouped bubbles) and the composer. On desktop, files dropped on
// it are attached.
import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { api, type Message } from "../api";
import { Icon } from "../lib/icons";
import { useFileDrop } from "../lib/drop";
import { bytes, dayLabel, dayStart, MAX_FILES } from "../lib/format";
import { copyText, errText } from "../lib/native";
import { displayName, hubStatusText, kindInfo, kindOf, limitFor, msgTime, peerHubs, presence, viaHub } from "../lib/peers";
import { refreshChats, useMessages, useSnap } from "../lib/store";
import { toast } from "../lib/toast";
import { Composer, type ComposerApi } from "./Composer";
import { HubHelpLink } from "./HubHelp";
import { MessageView } from "./MessageView";
import { Addr, KindChip, KindGlyph, NoteCard, PeerAvatar, PresText, useNow, usePlatform } from "./ui";

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
  const { msgs, loaded, reload } = useMessages(peer);
  const [replyTo, setReplyTo] = useState<Message | null>(null);
  const [sel, setSel] = useState<string | null>(null);
  const [extra, setExtra] = useState<Record<string, Message | null>>({});
  const tl = useRef<HTMLDivElement>(null);
  const pinned = useRef(true);
  const [far, setFar] = useState(false);
  const newMark = useRef<{ peer: string; id: string | null } | null>(null);

  useEffect(() => { setReplyTo(null); setSel(null); pinned.current = true; }, [peer]);

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
  const onScroll = () => {
    const el = tl.current; if (!el) return;
    const gap = el.scrollHeight - el.scrollTop - el.clientHeight;
    pinned.current = gap < 80; setFar(gap > 300);
  };
  const toBottom = () => { const el = tl.current; if (el) { el.scrollTop = el.scrollHeight; pinned.current = true; setFar(false); } };

  const jump = useCallback((id: string) => {
    const el = document.getElementById("m-" + id);
    if (!el) { toast("That message is further back than this chat shows"); return; }
    el.scrollIntoView({ block: "center", behavior: "smooth" });
    el.classList.remove("flash"); void el.offsetWidth; el.classList.add("flash");
    setTimeout(() => el.classList.remove("flash"), 1500);
  }, []);
  const del = useCallback((m: Message) => {
    api.deleteMessage(m.id).then(() => { reload(); void refreshChats(); setSel(null); }, (e) => toast(errText(e)));
  }, [reload]);
  const reply = useCallback((m: Message) => { setReplyTo(m); setSel(null); }, []);
  const info = useCallback((m: Message) => { setSel(null); onInfo(m); }, [onInfo]);

  // the banner: none of this peer's hubs is connected
  const ph = peerHubs(c, hubs);
  const down = ph.length && !ph.some((h) => h.state === "connected") ? ph[0] : null;
  const now = useNow(!!down);
  const via = viaHub(c, hubs) || ph[0];

  // ------------------------------------------------------------ timeline
  const rows: React.ReactNode[] = [];
  if (loaded && !msgs.length) {
    const k = kindInfo(kind);
    rows.push(
      <div className="chat-start" key="start">
        <PeerAvatar address={peer} c={c} hubs={hubs} size={72} withPres={false} />
        <b>{name}</b>
        <div className="mono" style={{ fontSize: 12.5, overflowWrap: "anywhere" }}>@net:{peer}</div>
        <NoteCard icon={c ? k.icon : "search"}>
          {c ? <><b>{k.label}.</b> {k.what}{kind !== "person" ? " Replies can take a minute: an agent has to pick your message up first." : ""}{via ? <> Messages go through hub <b>{via.name}</b>.</> : null}</>
            : <><b>Not on your mail hubs.</b> None of the hubs you're connected to lists this address, so a message can't reach it.</>}
        </NoteCard>
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
      <MessageView key={m.id} m={m} first={first} peerName={name} peerKind={kind} quoted={quoted(m.reply_to)} hover={platform === "desktop"} selected={sel === m.id}
        onReply={reply} onInfo={info} onDelete={del} onJump={jump} onOpenAddr={onOpenAddr} />,
    );
    prev = m;
  }
  const p = presence(c, hubs);
  if (p.state === "offline" && msgs.some((m) => m.outgoing && m.state === "sent")) {
    rows.push(<div className="sysnote" key="pend"><Icon name="schedule" />{name} is offline ({p.short}). Your message waits on hub {via?.name} and is delivered when they reconnect.</div>);
  }
  if (msgs.some((m) => m.outgoing && (m.state === "queued" || m.state === "sending")) && !viaHub(c, hubs)) {
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
      <div className="scr">
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
            <div className="who" onClick={onContact} role="button" aria-label={"Contact info: " + name}>
              <PeerAvatar address={peer} c={c} hubs={hubs} size={40} />
              <div className="t">
                <span className="n"><span className="ell">{name}</span> <KindGlyph kind={kind} /></span>
                <span className="s">{p.state === "disconnected" ? <Icon name="cloud_off" /> : null}{p.state === "online" ? "online" : p.short}{via && hubs.length > 1 ? " · via " + via.name : ""}</span>
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
    <section className="conv" ref={conv}>
      <div className="conv-head">
        <span className="conv-av" onClick={onContact}><PeerAvatar address={peer} c={c} hubs={hubs} size={40} /></span>
        <div className="who" onClick={onContact} title="Contact info">
          <div className="n1"><span className="name">{name}</span>{c ? <KindChip kind={kind} /> : null}</div>
          <div className="n2">
            <PresText c={c} hubs={hubs} />
            <span className="sep" />
            <span className="mono ell"><Addr a={peer} net /></span>
            {via ? <><span className="sep" /><span className="chip hubchip" title={"Messages go through hub " + via.name}>via {via.name}</span></> : null}
          </div>
        </div>
        <button className="icon-btn" onClick={copyAddr} title="Copy address" aria-label="Copy address"><Icon name="copy" /></button>
        <button className={"icon-btn" + (infoOn ? " on" : "")} onClick={onContact} title="Contact info (Ctrl+I)" aria-label="Contact info" aria-pressed={!!infoOn}><Icon name="info" /></button>
      </div>
      <div className="tl-wrap">
        <div className="timeline scroll" ref={tl} onScroll={onScroll}><div className="tl-inner">{rows}</div></div>
        {jumpBtn}
        {dragging ? <DropZone lim={limitFor(c, hubs)} /> : null}
      </div>
      {composer}
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
