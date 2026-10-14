import { FileBackgroundBanner } from "./Banners";
// A conversation: header, hub banner, timeline (oldest to newest, day
// dividers, grouped bubbles) and the composer. On desktop, files dropped on
// it are attached.
import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { api, type Message, type SetupState } from "../api";
import { Icon } from "../lib/icons";
import { useFileDrop } from "../lib/drop";
import { bytes, dayLabel, dayStart, MAX_FILES } from "../lib/format";
import { copyText, errText } from "../lib/native";
import { displayName, hubByUrl, hubName, hubStatusText, kindInfo, kindOf, limitFor, msgTime, peerHubs, presence, preview, viaHub } from "../lib/peers";
import { scanSetup } from "../lib/setup";
import { refreshChats, useMessages, useSendRoute, useSnap, useTyping } from "../lib/store";
import { replyIds } from "../lib/replies";
import { cardOf, answerNotes, cardKnown, isOpen, isSettled, settlement } from "../lib/questions";
import { loadDraftAnswers, pick, removeReply, sentDraftAnswers, setReplies, unpick, useDraftAnswers } from "../lib/answers";
import { toast } from "../lib/toast";
import { Composer, type ComposerApi } from "./Composer";
import { HubHelpLink } from "./HubHelp";
import { HubChip, routeLabel, routeVia } from "./HubPicker";
import { MessageView } from "./MessageView";
import { Addr, KindChip, KindGlyph, NoteCard, PeerAvatar, PresText, pressKeys, useNow, usePlatform, useSeenTick } from "./ui";

const GROUP_GAP = 5 * 60000;
const NONE: Message[] = [];
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
  /** Close this DM while retaining its history. */
  onCloseDm?: () => void;
}

export function Conversation({ peer, onBack, onInfo, onOpenAddr, onContact, infoOn, onMenu, onCloseDm }: Props) {
  const platform = usePlatform();
  const snap = useSnap();
  const hubs = snap.state?.hubs || [];
  const c = snap.byAddr.get(peer);
  const name = displayName(c, peer);
  // they are writing to us now (hub feature `typing`)
  const typingNow = useTyping(peer);
  const kind = kindOf(c);
  const { msgs, loaded, reload, more, loadingOlder, loadOlder, unreachable, retryOlder } = useMessages(peer);
  // what the next message answers: the messages it replies to and the
  // options picked on question cards, kept per chat across restarts
  const pending = useDraftAnswers(peer);
  useEffect(() => { void loadDraftAnswers(peer); }, [peer]);
  // selected messages, in the order they were picked (Android long-press, desktop Ctrl+click)
  const [sel, setSel] = useState<string[]>([]);
  // desktop keyboard: the message Shift+Tab has highlighted (R replies to it)
  const [hl, setHl] = useState<string | null>(null);
  const hlRef = useRef(hl);
  hlRef.current = hl;
  const [extra, setExtra] = useState<Record<string, Message | null>>({});
  const tl = useRef<HTMLDivElement>(null);
  const pinned = useRef(true);
  const [far, setFar] = useState(false);
  const newMark = useRef<{ peer: string; id: string | null } | null>(null);

  useEffect(() => { setSel([]); setHl(null); pinned.current = true; }, [peer]);

  // Scan setup code: where linking this address with the org stands (a
  // chat-level note, hubchat-opus 11:38Z), read again as messages come
  const [setup, setSetup] = useState<SetupState | null>(null);
  useEffect(() => {
    let live = true;
    api.setupStatus(peer).then((s) => { if (live) setSetup(s); }, () => {});
    return () => { live = false; };
  }, [peer, msgs.length]);

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
  const pos = useMemo(() => new Map(msgs.map((m, i) => [m.id, i])), [msgs]);
  useEffect(() => {
    const want = [...msgs.flatMap(replyIds), ...pending.replies, ...Object.keys(pending.picks)];
    const missing = [...new Set(want)].filter((id) => !byId.has(id) && !(id in extra));
    if (!missing.length) return;
    let live = true;
    Promise.all(missing.map((id) => api.message(id).then((r) => [id, r] as const, () => [id, null] as const))).then((rs) => {
      if (live) setExtra((x) => ({ ...x, ...Object.fromEntries(rs) }));
    });
    return () => { live = false; };
  }, [msgs, byId, extra, pending]);
  const quoted = (id: string | null) => (id ? byId.get(id) ?? extra[id] : undefined);
  // a multi-reply's quotes, made once per change so the bubbles stay memoized
  const quoteLists = useMemo(() => {
    const out = new Map<string, (Message | null | undefined)[]>();
    for (const m of msgs) { const ids = replyIds(m); if (ids.length > 1) out.set(m.id, ids.map((id) => byId.get(id) ?? extra[id])); }
    return out;
  }, [msgs, byId, extra]);
  // question cards: each one's state, from the page and from the store's
  // first answer to every card held here (a card or its answer may lie
  // beyond the page), and what each reply did to them
  const asked = useMemo(() => [...new Set([
    ...Object.keys(pending.picks), ...pending.replies, ...msgs.flatMap(replyIds),
    ...msgs.filter((m) => m.kind === "question").map((m) => m.id).reverse(),
  ])], [msgs, pending]);
  const [found, setFound] = useState<{ peer: string; msgs: Message[] }>({ peer: "", msgs: NONE });
  useEffect(() => {
    let live = true;
    (asked.length ? api.cardAnswers(peer, asked) : Promise.resolve(NONE)).then((r) => { if (live) setFound({ peer, msgs: r }); }, (e) => console.warn("hc_card_answers", e));
    return () => { live = false; };
  }, [peer, asked]);
  const held = found.peer === peer ? found.msgs : NONE;
  const settled = useMemo(() => settlement(msgs, held), [msgs, held]);
  const cards = settled.states;
  const notes = useMemo(() => answerNotes(msgs, cards, held), [msgs, cards, held]);
  const open = useCallback((id: string) => isOpen(settled, id), [settled]);
  const cardState = useCallback((id: string) => cardKnown(settled, id), [settled]);
  const replies = useMemo(() => pending.replies.map((id) => byId.get(id) ?? extra[id]).filter((m): m is Message => !!m), [pending.replies, byId, extra]);
  // picks wait only on cards the store shows open: one answered meanwhile
  // (say on another device) drops them, one not known yet waits
  const picked = useMemo(() => Object.entries(pending.picks).flatMap(([id, labels]) => {
    const m = byId.get(id) ?? extra[id], card = m ? cardOf(m) : null;
    if (!m || !card || m.outgoing || !open(id)) return [];
    const l = card.options.filter((o) => labels.includes(o));
    return l.length ? [{ card: m, labels: l }] : [];
  }), [pending.picks, byId, extra, open]);
  useEffect(() => {
    const done = Object.keys(pending.picks).filter((id) => isSettled(settled, id));
    if (done.length) unpick(peer, done);
  }, [settled, pending.picks, peer]);
  const onPick = useCallback((m: Message, label: string) => { const card = cardOf(m); if (card) pick(peer, m.id, label, card.multiple); }, [peer]);

  // keep pinned to the newest message while at the bottom
  useLayoutEffect(() => {
    const el = tl.current; if (!el || !loaded) return;
    const nm = newMark.current?.id && document.getElementById("m-" + newMark.current.id);
    if (nm && pinned.current === true && el.dataset.opened !== peer) { el.scrollTop = Math.max(0, nm.offsetTop - 120); pinned.current = false; }
    else if (pinned.current) el.scrollTop = el.scrollHeight;
    el.dataset.opened = peer;
  }, [msgs, loaded, peer, replies]);
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
  // a hub whose older messages couldn't be loaded is back: go on from it
  const backOnline = unreachable.some((u) => hubByUrl(hubs, u)?.state === "connected");
  useEffect(() => {
    if (!backOnline) return;
    retryOlder();
    // at the top of a full view no scroll would ask: load now
    const el = tl.current;
    if (el && el.scrollTop < 400) older();
  }, [backOnline]); // eslint-disable-line react-hooks/exhaustive-deps
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
    api.deleteMessage(m.id).then(() => { reload(); void refreshChats(); setSel([]); }, (e) => toast(errText(e)));
  }, [reload]);
  // Reply answers one message, as before; the selection's Reply answers all
  // of it, oldest first (user 2026-10-11 13:18Z)
  // (the box takes the focus only then: replies saved from before don't open the keyboard)
  const reply = useCallback((m: Message) => { setReplies(peer, [m.id]); setSel([]); comp.current?.focus(); }, [peer]);
  const toggleSel = useCallback((m: Message) => setSel((s) => (s.includes(m.id) ? s.filter((x) => x !== m.id) : [...s, m.id])), []);
  const replySel = () => { const ids = msgs.filter((m) => sel.includes(m.id)).map((m) => m.id); if (ids.length) setReplies(peer, ids); setSel([]); comp.current?.focus(); };
  const info = useCallback((m: Message) => { setSel([]); onInfo(m); }, [onInfo]);

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
    if (!cur && e.key === "Escape" && sel.length) { e.preventDefault(); e.stopPropagation(); setSel([]); return; }
    // Escape anywhere in the conversation cancels a prepared reply, as its ×
    // does (user 2026-10-13); the highlight and a selection go first, an open
    // modal keeps the key, and with no reply Escape keeps its old meaning
    if (!cur && e.key === "Escape" && replies.length && !comp.current?.busy() && !document.querySelector(".modal, .dialog")) {
      e.preventDefault(); e.stopPropagation(); setReplies(peer, []); return;
    }
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
  if (unreachable.length && !loadingOlder) {
    const names = unreachable.map((u) => hubByUrl(hubs, u)?.name || u);
    const list = names.length > 1 ? names.slice(0, -1).join(", ") + " and " + names[names.length - 1] : names[0];
    rows.push(<div className="tl-gap" key="gap" role="note">Older messages through {list} are not loaded yet. They load when {names.length > 1 ? "those hubs are" : "it is"} back online.</div>);
  }
  if (loaded && !msgs.length && !more && !unreachable.length) {
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
      <MessageView key={m.id} m={m} first={first} peerName={name} peerKind={kind} quoted={quoted(m.reply_to)} quotes={quoteLists.get(m.id)} card={cards.get(m.id)} picks={pending.picks[m.id]} notes={notes.get(m.id)} hover={platform === "desktop"} selected={sel.includes(m.id)} highlighted={hl === m.id}
        onReply={reply} onSelect={toggleSel} onPick={onPick} onInfo={info} onDelete={del} onJump={jump} onOpenAddr={onOpenAddr} />,
    );
    prev = m;
    if (setup?.reply_id === m.id) rows.push(setupNote(setup, platform));
  }
  if (setup && (!setup.outcome || !msgs.some((m) => m.id === setup.reply_id))) rows.push(setupNote(setup, platform));
  const p = presence(c, hubs);
  useSeenTick(p.state === "offline");
  if (p.state === "offline" && msgs.some((m) => m.outgoing && m.state === "sent")) {
    rows.push(<div className="sysnote" key="pend"><Icon name="schedule" />{name} is offline ({p.short}). Your message waits on hub {via?.name} and is delivered when they reconnect.</div>);
  }
  const waiting = msgs.some((m) => m.outgoing && (m.state === "queued" || m.state === "sending"));
  if (waiting && route?.pinned && !route.next) {
    // a pinned hub that can't reach them: say why, never switch on our own
    const pin = hubName(hubs, route.pinned);
    const why = hubByUrl(hubs, route.pinned)?.state === "disabled"
      ? <>Messages to {name} wait until you enable hub {pin} on this device.</>
      : hubByUrl(hubs, route.pinned)?.state !== "connected"
      ? <>Messages to {name} wait until hub {pin} is back.</>
      : <>Messages to {name} wait: hub {pin} doesn't list them.</>;
    rows.push(<div className="sysnote" key="pinwait"><Icon name="schedule" />{why} <button className="link" onClick={() => api.setSendHub(peer, null).catch((e) => toast(errText(e)))}>Use Automatic</button></div>);
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
        lp: setTimeout(() => { const g = gesture.current; if (g && !g.swiping) { g.done = true; setSel((s) => (s.includes(id) ? s : [...s, id])); } }, 450) };
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
      if (!sel.length || (e.target as HTMLElement).closest("button, a")) return;
      const msg = (e.target as HTMLElement).closest(".msg") as HTMLElement | null;
      const m = msg ? byId.get(msg.dataset.id!) : undefined;
      if (m) toggleSel(m);
    },
  } : {};
  function endGesture() {
    const g = gesture.current; if (!g) return;
    clearTimeout(g.lp);
    suppressClick.current = g.done || g.swiping;
    g.row.style.transition = "transform .18s"; g.row.style.transform = "";
    if (g.ic) g.ic.style.opacity = "0";
    setTimeout(() => { g.row.style.transition = ""; }, 200);
    if (g.swiping && g.dx > 56) { const m = byId.get(g.id); if (m) reply(m); }
    gesture.current = null;
  }

  // no hub you're connected to lists this address: nothing can reach it
  // (an empty directory, say before any hub answered, blocks nothing)
  const off = !c && snap.directory.length > 0 ? name + " isn't on any of the mail hubs you're connected to." : null;
  const comp = useRef<ComposerApi>(null);
  const conv = useRef<HTMLElement>(null);
  const dragging = useFileDrop(conv, platform === "desktop" && !off, (paths) => comp.current?.addFiles(paths));
  const composer = <Composer ref={comp} peer={peer} c={c} hubs={hubs} replies={replies} picked={picked} cardState={cardState} pos={pos} onRemoveReply={(id) => removeReply(peer, id)} onUnpick={(id) => unpick(peer, [id])} onCancelReply={() => setReplies(peer, [])}
    onSent={(m) => { sentDraftAnswers(peer, m); pinned.current = true; reload(); }} off={off} />;
  const jumpBtn = <button className={"jump" + (far ? "" : " hide")} onClick={toBottom} title="Jump to the newest message" aria-label="Jump to the newest message"><Icon name="arrow_down" /></button>;
  const copyAddr = () => copyText("@net:" + peer, "Address copied");

  if (platform === "android") {
    const selMsg = sel.length === 1 ? byId.get(sel[0]) : undefined;
    return (
      <div className="scr" role="main">
        {sel.length ? (
          <div className="appbar select">
            <button className="icon-btn" onClick={() => setSel([])} aria-label="Cancel"><Icon name="close" /></button>
            <div className="title">{sel.length} selected</div>
            <button className="icon-btn" onClick={replySel} aria-label="Reply"><Icon name="reply" /></button>
            {selMsg ? <>
              <button className="icon-btn" onClick={() => { void copyText(selMsg.body, "Copied"); setSel([]); }} aria-label="Copy"><Icon name="copy" /></button>
              <button className="icon-btn" onClick={() => info(selMsg)} aria-label="Message info"><Icon name="info" /></button>
            </> : null}
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
            <Icon name="warning" /><span><b>{down.state === "disabled" ? down.name + " is disabled on this device" : down.state === "connecting" ? "Connecting to " + down.name + "…" : "Can't reach hub " + down.name}</b>{down.state === "disconnected" ? " · " + hubStatusText(down, now).replace("Can't reach this hub · ", "") : down.state === "refused" ? " · " + (down.error || "refused") : ""}</span>
            {down.state === "disconnected" ? <HubHelpLink /> : null}{down.state === "disabled" ? <button className="link" onClick={() => api.setHubEnabled(down.url, true).catch((e) => toast(errText(e)))}>Enable</button> : down.state !== "connecting" ? <button className="link" onClick={() => api.retryNow()}>Retry</button> : null}
          </div>
        ) : null}
        <FileBackgroundBanner />
        <div className="tl-wrap">
          <div className="timeline" ref={tl} onScroll={onScroll} {...touch}><div>{rows}{typingNow ? <TypingRow name={name} /> : null}</div></div>
          {jumpBtn}
        </div>
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
        {onCloseDm ? <button className="icon-btn" onClick={onCloseDm} title="Close DM (keep history)" aria-label="Close DM"><Icon name="close" /></button> : null}
      </div>
      <div className="tl-wrap">
        <div className="timeline scroll" ref={tl} onScroll={onScroll} tabIndex={platform === "desktop" ? -1 : undefined} role="region" aria-label={"Messages with " + name}
          onContextMenu={(e) => { if (hlRef.current && e.target === e.currentTarget) { e.preventDefault(); menuOnHl(); } }}
          onMouseDown={() => { if (hlRef.current) leaveHl(); }}
          onBlur={(e) => { if (hlRef.current && document.hasFocus() && !e.currentTarget.contains(e.relatedTarget as Node | null)) leaveHl(); }}><div className="tl-inner">{rows}{typingNow ? <TypingRow name={name} /> : null}</div></div>
        {jumpBtn}
        {dragging ? <DropZone lim={limitFor(c, hubs)} /> : null}
      </div>
      {sel.length ? (
        <div className="selbar" role="toolbar" aria-label="Selected messages">
          <span className="sb-n">{sel.length} selected</span>
          <span className="sb-hint">Ctrl+click adds or removes a message</span>
          <button className="btn sm" onClick={replySel}><Icon name="reply" />Reply</button>
          <button className="icon-btn" onClick={() => setSel([])} title="Clear selection (Esc)" aria-label="Clear selection"><Icon name="close" /></button>
        </div>
      ) : null}
      {composer}
      <div className="sr" role="status" aria-live="polite">{said}</div>
    </section>
  );
}

/** Under the newest message while the peer is writing to us (hub feature
 *  `typing`): its name and three dots, announced politely. */
function TypingRow({ name }: { name: string }) {
  return (
    <div className="typing-row" role="status" aria-live="polite" data-typing="1">
      <span className="typing-dots" aria-hidden="true"><i /><i /><i /></span>
      <span>{name} is typing…</span>
    </div>
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

/** The setup note: waiting for the org's answer, Linked, or the code didn't work. */
function setupNote(s: SetupState, platform: string) {
  if (s.outcome === "linked") return <div className="sysnote linked" key="setup" data-setup="linked"><Icon name="link" />Linked — {s.orgname} knows this address is you.</div>;
  if (s.outcome === "expired") {
    return (
      <div className="sysnote setup-bad" key="setup" data-setup="expired"><Icon name="error_outline" />That setup code didn't work. It may have expired or already been used. Show a new code on your PC, then scan again.
        {platform === "android" ? <div><button className="btn sm" onClick={() => void scanSetup()}><Icon name="qr" />Scan again</button></div> : null}
      </div>
    );
  }
  return <div className="sysnote" key="setup" data-setup="pending"><Icon name="schedule" />Waiting for {s.orgname} to confirm…</div>;
}
