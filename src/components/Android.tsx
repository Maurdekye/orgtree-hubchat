// Android layout: a stack of full-screen screens. The system back button and
// Escape go up one screen (history entries mirror the stack); with the
// conversation's menu open they close the menu instead.
import { useCallback, useEffect, useRef, useState } from "react";
import type { Message } from "../api";
import { Icon } from "../lib/icons";
import { displayName, kindOf } from "../lib/peers";
import { getSnap, useSendRoute, useSnap } from "../lib/store";
import { closeDm, openDm } from "../lib/dm";
import { useNavigationIntent } from "../lib/navigationIntent";
import { chatLinkTarget } from "../lib/chatlink";
import { installKeyboardImages } from "../lib/keyboardImages";
import { startTake, useActive, useForeground, useLastChat, useMessage, usePendingLink, useReadTracking } from "../lib/visibility";
import { FileBackgroundBanner, HubBanner, RecoveryBanner, UpdateBanner } from "./Banners";
import { startTailnetWatch } from "../lib/tailnet";
import { startUpdateChecks } from "../lib/updates";
import { isSetupLink, startSetup, useSetupChat } from "../lib/setup";
import { ChatRows, EmptyChats, useFilteredChats, type ChatFilter } from "./ChatList";
import { ContactInfo } from "./ContactInfo";
import { Conversation } from "./Conversation";
import { Directory } from "./Directory";
import { hasChoice, HubSheet, routeLabel } from "./HubPicker";
import { MessageInfoBody } from "./MessageInfo";
import { MessageView } from "./MessageView";
import { NewChatInput, NewChatResults, useResolve } from "./NewChat";
import { api } from "../api";
import { JoinFlow } from "./JoinLink";
import { LinkDevice, type LinkTab } from "./LinkDevice";
import { endJoin, routeLink, startJoin, useJoin, type JoinReq } from "../lib/join";
import { copyText, errText } from "../lib/native";
import { toast } from "../lib/toast";
import { SettingsList, SettingsSection, TABS, type SetTab } from "./Settings";
import { TransfersStrip } from "./Transfers";
import { ImageViewer } from "./AttImage";
import { AppVersion, Toasts } from "./ui";

type Scr =
  | { s: "chats" } | { s: "conv"; p: string } | { s: "msginfo"; id: string; p: string } | { s: "info"; p: string }
  | { s: "newchat" } | { s: "directory" } | { s: "settings" } | { s: "set"; tab: SetTab } | { s: "link"; tab?: LinkTab; input?: string };

// MainActivity asks the page first: its visible route can still have a parent
// when WebView history is missing or skips an entry opened before a user tap.
// Overlay entries own Back before the route beneath them.
let closeOpenMenu: (() => boolean) | null = null;
let cancelPendingNavigation: (() => void) | null = null;
let backOpenScreen: (() => boolean) | null = null;
(window as unknown as { __hcBack: () => boolean }).__hcBack = () => {
  cancelPendingNavigation?.();
  if (closeOpenMenu?.()) return true;
  const s = history.state as Record<string, unknown> | null;
  if (s && Object.keys(s).some((k) => k !== "hc")) { history.back(); return true; }
  if (backOpenScreen?.()) return true;
  if (s && typeof s.hc === "number" && s.hc > 1) { history.back(); return true; }
  return false;
};

function Chats({ go }: { go: (s: Scr) => void }) {
  const [filter, setFilter] = useState<ChatFilter>("all");
  const [q, setQ] = useState("");
  const [searching, setSearching] = useState(false);
  const { rows, total } = useFilteredChats(filter, q, null);
  const f = (k: ChatFilter, label: string) => <button className={"fchip" + (filter === k ? " on" : "")} onClick={() => setFilter(k)}>{label}</button>;
  return (
    <>
      <div className="appbar" role="banner">
        {searching ? <>
          <button className="icon-btn" onClick={() => { setSearching(false); setQ(""); }} aria-label="Close search"><Icon name="back" /></button>
          <label className="to-field" style={{ flex: 1, margin: "0 8px 0 0", height: 44 }}><input autoFocus value={q} onChange={(e) => setQ(e.target.value)} placeholder="Search chats" autoComplete="off" /></label>
        </> : <>
          <div className="title" role="heading" aria-level={1}>Hubchat<AppVersion /></div>
          <button className="icon-btn" onClick={() => setSearching(true)} aria-label="Search"><Icon name="search" /></button>
          <button className="icon-btn" onClick={() => go({ s: "directory" })} aria-label="Directory"><Icon name="contacts" /></button>
          <button className="icon-btn" onClick={() => go({ s: "settings" })} aria-label="Settings"><Icon name="settings" /></button>
        </>}
      </div>
      <UpdateBanner />
      <HubBanner />
      <FileBackgroundBanner />
      <RecoveryBanner onShow={() => go({ s: "set", tab: "recovery" })} />
      <TransfersStrip onOpen={(p) => go({ s: "conv", p })} />
      <div className="fchips">{f("all", "All")}{f("agents", "Agents")}{f("people", "People")}</div>
      <div className="scr-body" role="main">
        {!total ? <EmptyChats onNew={() => go({ s: "newchat" })} />
          : rows.length ? <><ChatRows rows={rows} selected={null} onOpen={(p) => go({ s: "conv", p })} /><div style={{ height: 90 }} /></>
          : <div className="empty"><b>No chats match</b>To reach someone new, use New chat and type their address.</div>}
      </div>
      <button className="fab" onClick={() => go({ s: "newchat" })} aria-label="New chat"><Icon name="new_chat" />New chat</button>
    </>
  );
}

function NewChatScreen({ back, open, dir }: { back: () => void; open: (a: string) => void; dir: () => void }) {
  const [q, setQ] = useState("");
  const r = useResolve(q);
  return (
    <>
      <div className="appbar flat"><button className="icon-btn" onClick={back} aria-label="Back"><Icon name="back" /></button><div className="title">New chat</div></div>
      <NewChatInput q={q} setQ={setQ} onEnter={() => { if (r?.exact) open(r.exact.address); }} />
      <div className="scr-body"><NewChatResults q={q} r={r} onOpen={open} onDirectory={dir} /></div>
    </>
  );
}

function MsgInfoScreen({ id, peer, back }: { id: string; peer: string; back: () => void }) {
  const m = useMessage(id, peer);
  const snap = useSnap();
  const c = snap.byAddr.get(peer);
  const noop = useCallback(() => {}, []);
  return (
    <>
      <div className="appbar flat"><button className="icon-btn" onClick={back} aria-label="Back"><Icon name="back" /></button><div className="title">Message info</div></div>
      <div className="scr-body">
        {m ? <>
          <div style={{ padding: "14px 12px 4px" }}><MessageView m={m} first peerName={displayName(c, peer)} peerKind={kindOf(c)} quoted={undefined} hover={false} onReply={noop} onInfo={noop} onDelete={noop} onJump={noop} onOpenAddr={noop} /></div>
          <MessageInfoBody m={m} />
        </> : <div className="empty">This message is no longer on this device.</div>}
      </div>
    </>
  );
}

/** The conversation's ⋮ menu: a bottom sheet (the prototype's conv-menu). */
function ConvMenu({ peer, onContact, onClose, onCloseDm }: { peer: string; onContact: () => void; onClose: () => void; onCloseDm: () => void }) {
  const snap = useSnap();
  const route = useSendRoute(peer);
  const [hubSheet, setHubSheet] = useState(false);
  if (hubSheet && route) return <HubSheet peer={peer} route={route} who={displayName(snap.byAddr.get(peer), peer)} onClose={onClose} />;
  return (
    <>
      <div className="sheet-scrim" onClick={onClose} />
      <div className="sheet" role="menu" aria-label="Chat menu">
        <div className="grip" />
        <button className="mi" role="menuitem" onClick={onContact}><Icon name="info" />Contact info</button>
        {hasChoice(route) ? <button className="mi" role="menuitem" onClick={() => setHubSheet(true)}><Icon name="dns" />Send through…<span className="s">{routeLabel(route, snap.state?.hubs || []).replace(/^via /, "")}</span></button> : null}
        <button className="mi" role="menuitem" onClick={() => { onClose(); void copyText("@net:" + peer, "Address copied"); }}><Icon name="copy" />Copy address</button>
        <button className="mi" role="menuitem" onClick={onCloseDm}><Icon name="close" />Close DM</button>
      </div>
    </>
  );
}

export function Android() {
  const intent = useNavigationIntent();
  const [stack, setStack] = useState<Scr[]>([{ s: "chats" }]);
  const [dir, setDir] = useState<"enter" | "back" | "">("");
  const depth = useRef(1);
  const top = stack[stack.length - 1];
  const fg = useForeground("android");
  useActive(fg);
  useReadTracking(top.s === "conv" ? top.p : null, fg);
  useEffect(() => { startUpdateChecks("android"); }, []);
  useEffect(() => { startTailnetWatch(); }, []);
  // pictures from the keyboard go into the message box
  useEffect(() => { installKeyboardImages(); }, []);

  const stackRef = useRef(stack);
  stackRef.current = stack;
  const pendingFallback = useRef<{ stack: Scr[]; current: () => boolean } | null>(null);
  const commitStack = useCallback((next: Scr[]) => {
    stackRef.current = next;
    depth.current = next.length;
    setStack(next);
  }, []);
  // the open conversation's menu (its address); a screen change closes it
  const [menu, setMenu] = useState<string | null>(null);
  const menuRef = useRef<string | null>(null);
  const openMenu = useCallback((p: string) => { intent.invalidate(); menuRef.current = p; setMenu(p); }, [intent]);
  const closeMenu = useCallback(() => { menuRef.current = null; setMenu(null); }, []);
  useEffect(() => {
    closeOpenMenu = () => { if (!menuRef.current) return false; closeMenu(); return true; };
    cancelPendingNavigation = intent.invalidate;
    return () => { closeOpenMenu = null; cancelPendingNavigation = null; };
  }, [closeMenu, intent]);
  const navigate = useCallback((s: Scr) => {
    const st = stackRef.current;
    const t = st[st.length - 1];
    closeMenu();
    setDir("enter");
    // opening a chat from New chat / Directory replaces those screens
    if (s.s === "conv" && (t.s === "newchat" || t.s === "directory")) { commitStack([...st.slice(0, -1), s]); return; }
    history.pushState({ hc: st.length + 1 }, "");
    commitStack([...st, s]);
  }, [closeMenu, commitStack]);
  const go = useCallback((s: Scr) => {
    const current = intent.begin();
    if (s.s === "conv") void openDm(s.p).then(() => { if (current()) navigate(s); }, (e) => { if (current()) toast(errText(e)); });
    else navigate(s);
  }, [navigate, intent]);
  const close = useCallback((peer: string) => {
    const current = intent.begin();
    void closeDm(peer, () => {
      closeMenu();
      // Remove every conversation from the stack so Back cannot restore the closed DM.
      history.replaceState({ hc: 1 }, "");
      setDir("back");
      commitStack([{ s: "chats" }]);
    }, current);
  }, [closeMenu, commitStack, intent]);
  const parent = useCallback((st: Scr[]) => {
    if (stackRef.current !== st || (st.length === 1 && st[0].s === "chats")) return false;
    closeMenu();
    setDir("back");
    commitStack(st.length > 1 ? st.slice(0, -1) : [{ s: "chats" }]);
    return true;
  }, [closeMenu, commitStack]);
  // Corner arrows navigate immediately. History is only a best-effort mirror;
  // a failed traversal must not trap a secondary screen.
  const back = useCallback(() => {
    intent.invalidate();
    const st = stackRef.current;
    const canTraverse = history.state?.hc === st.length && st.length > 1 && history.length > 1;
    if (!parent(st)) return;
    const next = stackRef.current;
    const sync = () => {
      if (stackRef.current === next && history.state?.hc !== next.length) history.replaceState({ hc: next.length }, "");
    };
    if (canTraverse) {
      try { history.back(); requestAnimationFrame(sync); } catch { sync(); }
    } else history.replaceState({ hc: next.length }, "");
  }, [intent, parent]);
  const systemBack = useCallback(() => {
    const st = stackRef.current;
    if (st.length === 1 && st[0].s === "chats") return false;
    intent.invalidate();
    const attempt = { stack: st, current: intent.capture() };
    pendingFallback.current = attempt;
    const fallback = () => {
      // A real pop, a newer navigation or unmount invalidates this attempt.
      if (pendingFallback.current === attempt && attempt.current() && parent(st)) history.replaceState({ hc: stackRef.current.length }, "");
      if (pendingFallback.current === attempt) pendingFallback.current = null;
    };
    if (history.state?.hc !== st.length || history.length <= 1) fallback();
    else {
      try { history.back(); requestAnimationFrame(fallback); } catch { fallback(); }
    }
    return true;
  }, [intent, parent]);
  useEffect(() => {
    backOpenScreen = systemBack;
    return () => { backOpenScreen = null; };
  }, [systemBack]);

  useEffect(() => {
    history.replaceState({ hc: 1 }, "");
    const pop = (e: PopStateEvent) => {
      // back with the menu open: close it and keep the screen (put back the
      // history entry the system back button took)
      if (menuRef.current) { intent.invalidate(); pendingFallback.current = null; closeMenu(); history.pushState({ hc: depth.current }, ""); return; }
      const d = (e.state && typeof e.state.hc === "number" ? e.state.hc : 1) as number;
      const st = stackRef.current;
      const attempt = pendingFallback.current;
      // A duplicate/skipped history entry can emit popstate without leaving
      // the route. Preserve only the still-current Back attempt across that
      // event; a newer intent must never be revived by a later pop.
      const unchanged = d >= st.length;
      const keepFallback = unchanged && attempt?.stack === st && attempt.current();
      intent.invalidate();
      if (keepFallback) attempt.current = intent.capture();
      else pendingFallback.current = null;
      setDir("back");
      if (!unchanged) commitStack(st.slice(0, Math.max(1, d)));
    };
    const key = (e: KeyboardEvent) => {
      if (e.key !== "Escape" || document.querySelector(".dialog")) return;
      intent.invalidate();
      if (menuRef.current) closeMenu(); else back();
    };
    window.addEventListener("popstate", pop);
    window.addEventListener("keydown", key);
    return () => { window.removeEventListener("popstate", pop); window.removeEventListener("keydown", key); };
  }, [back, closeMenu, commitStack, intent]);

  // a tapped message notification names a chat: open it (at start, and
  // whenever the app comes back to the foreground)
  useEffect(() => {
    const take = () => {
      if (document.visibilityState !== "visible") return;
      const current = intent.capture();
      return api.takePendingChat().then((p) => {
        if (!p || !current()) return false;
        const t = stackRef.current[stackRef.current.length - 1];
        if (t.s !== "conv" || t.p !== p) go({ s: "conv", p });
        return true;
      }, () => false);
    };
    startTake(take());
    window.addEventListener("focus", take);
    document.addEventListener("visibilitychange", take);
    window.addEventListener("hc-pending", take); // a notification tapped with Hubchat in front
    return () => {
      window.removeEventListener("focus", take);
      document.removeEventListener("visibilitychange", take);
      window.removeEventListener("hc-pending", take);
    };
  }, [go, intent]);

  // a phone camera opened a hubchat:// link: a signed-in device's (role give)
  // is joined; a new device's (role take) is looked up on Link a device ›
  // Approve a code
  // Scan setup code opens the org's chat when it's done
  useSetupChat((p) => go({ s: "conv", p }));
  usePendingLink((input) => {
    if (isSetupLink(input)) { intent.invalidate(); startSetup(input); return; }
    // a profile QR's chat link opens that chat; your own opens your profile
    const to = chatLinkTarget(input);
    if (to !== null) {
      if (!to) toast("That chat link doesn't name an address.");
      else if (to === getSnap().state?.me?.address) go({ s: "set", tab: "profile" });
      else go({ s: "conv", p: to });
      return;
    }
    const current = intent.begin();
    routeLink(input, "approve").then((r) => {
      if (!current()) return;
      if (r.k === "join") startJoin(r.p.code, r.p.hub, r.p.hubs, r.p.hub_name);
      else go({ s: "link", tab: "approve", input: r.input });
    }, (e) => { if (current()) toast(errText(e)); });
  });
  const join = useJoin();
  useEffect(() => { if (join) intent.invalidate(); }, [join, intent]);
  // the chat the stack is in (also under its info screens); the list alone is none
  const inChat = [...stack].reverse().find((s): s is { s: "conv"; p: string } => s.s === "conv")?.p ?? null;
  // restored over the list only; the back button then leads to the list
  useLastChat(inChat, stack.length > 1 || !!join, (p) => go({ s: "conv", p }));

  const onInfo = useCallback((m: Message) => go({ s: "msginfo", id: m.id, p: m.peer }), [go]);
  const openChat = useCallback((a: string) => go({ s: "conv", p: a }), [go]);

  let screen;
  switch (top.s) {
    case "chats": screen = <Chats go={go} />; break;
    case "conv": {
      const p = top.p;
      const contact = () => go({ s: "info", p });
      return (
        <Shell dir={dir} k={"conv:" + p}>
          <Conversation key={p} peer={p} onBack={back} onInfo={onInfo} onOpenAddr={openChat} onContact={contact} onMenu={() => openMenu(p)} />
          {menu === p ? <ConvMenu peer={p} onContact={contact} onClose={closeMenu} onCloseDm={() => close(p)} /> : null}
        </Shell>
      );
    }
    case "msginfo": screen = <MsgInfoScreen id={top.id} peer={top.p} back={back} />; break;
    case "info": screen = <ContactInfo peer={top.p} onClose={back} />; break;
    case "newchat": screen = <NewChatScreen back={back} open={openChat} dir={() => go({ s: "directory" })} />; break;
    case "directory": return <Shell dir={dir} k="directory"><Directory onOpen={openChat} onBack={back} /></Shell>;
    case "settings": return <Shell dir={dir} k="settings"><SettingsList onOpen={(tab) => go({ s: "set", tab })} onBack={back} /></Shell>;
    case "set": screen = (
      <>
        <div className="appbar flat"><button className="icon-btn" onClick={back} aria-label="Back"><Icon name="back" /></button><div className="title">{TABS.find((t) => t[0] === top.tab)![1]}</div></div>
        <div className="scr-body"><SettingsSection tab={top.tab} onLink={(tab, input) => go({ s: "link", tab, input })} /></div>
      </>
    ); break;
    case "link": screen = (
      <>
        <div className="appbar flat"><button className="icon-btn" onClick={back} aria-label="Back"><Icon name="back" /></button><div className="title">Link a device</div></div>
        <div className="scr-body"><LinkDevice key={top.input || ""} initial={top.tab} input={top.input} onClose={back} onRecovery={() => go({ s: "set", tab: "recovery" })} /></div>
      </>
    ); break;
  }
  if (join) return <Shell dir="enter" k={"join" + join.n}><JoinScreen req={join} /></Shell>;
  return <Shell dir={dir} k={top.s + stack.length}><div className="scr">{screen}</div></Shell>;
}

/** Joining another identity's link, over the whole stack. It holds one
 *  history entry of its own, so the system back button leaves it. */
function JoinScreen({ req }: { req: JoinReq }) {
  const alive = useRef(false);
  useEffect(() => {
    alive.current = true;
    if (history.state?.join !== req.n) {
      const d = (history.state && typeof history.state.hc === "number" ? history.state.hc : 1) as number;
      history.pushState({ hc: d, join: req.n }, "");
    }
    const pop = () => { if (history.state?.join !== req.n) endJoin(); };
    window.addEventListener("popstate", pop);
    return () => {
      alive.current = false;
      window.removeEventListener("popstate", pop);
      // closed by a button: drop the entry (not on StrictMode's re-mount)
      setTimeout(() => { if (!alive.current && history.state?.join === req.n) history.back(); }, 0);
    };
  }, [req.n]);
  return <JoinFlow req={req} />;
}

/** The screen root; the inner .scr is keyed so a push or pop animates it. */
function Shell({ dir, k, children }: { dir: string; k: string; children: React.ReactNode }) {
  return (
    <div className="screen">
      <div className="viewport"><div key={k} className={"scr-anim " + dir} style={{ position: "absolute", inset: 0 }}>{children}</div></div>
      <ImageViewer />
      <Toasts />
    </div>
  );
}
