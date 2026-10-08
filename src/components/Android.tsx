// Android layout: a stack of full-screen screens. The system back button and
// Escape go up one screen (history entries mirror the stack).
import { useCallback, useEffect, useRef, useState } from "react";
import type { Message } from "../api";
import { Icon } from "../lib/icons";
import { displayName, kindOf } from "../lib/peers";
import { useSnap } from "../lib/store";
import { useForeground, useMessage, usePendingLink, useReadTracking } from "../lib/visibility";
import { HubBanner, RecoveryBanner } from "./Banners";
import { ChatRows, EmptyChats, useFilteredChats, type ChatFilter } from "./ChatList";
import { Conversation } from "./Conversation";
import { Directory } from "./Directory";
import { MessageInfoBody } from "./MessageInfo";
import { MessageView } from "./MessageView";
import { NewChatInput, NewChatResults, useResolve } from "./NewChat";
import { api } from "../api";
import { LinkDevice, type LinkTab } from "./LinkDevice";
import { SettingsList, SettingsSection, TABS, type SetTab } from "./Settings";
import { TransfersStrip } from "./Transfers";
import { Toasts } from "./ui";

type Scr =
  | { s: "chats" } | { s: "conv"; p: string } | { s: "msginfo"; id: string; p: string }
  | { s: "newchat" } | { s: "directory" } | { s: "settings" } | { s: "set"; tab: SetTab } | { s: "link"; tab?: LinkTab; input?: string };

function Chats({ go }: { go: (s: Scr) => void }) {
  const [filter, setFilter] = useState<ChatFilter>("all");
  const [q, setQ] = useState("");
  const [searching, setSearching] = useState(false);
  const { rows, total } = useFilteredChats(filter, q, null);
  const f = (k: ChatFilter, label: string) => <button className={"fchip" + (filter === k ? " on" : "")} onClick={() => setFilter(k)}>{label}</button>;
  return (
    <>
      <div className="appbar">
        {searching ? <>
          <button className="icon-btn" onClick={() => { setSearching(false); setQ(""); }} aria-label="Close search"><Icon name="back" /></button>
          <label className="to-field" style={{ flex: 1, margin: "0 8px 0 0", height: 44 }}><input autoFocus value={q} onChange={(e) => setQ(e.target.value)} placeholder="Search chats" autoComplete="off" /></label>
        </> : <>
          <div className="title">Hubchat</div>
          <button className="icon-btn" onClick={() => setSearching(true)} aria-label="Search"><Icon name="search" /></button>
          <button className="icon-btn" onClick={() => go({ s: "directory" })} aria-label="Directory"><Icon name="contacts" /></button>
          <button className="icon-btn" onClick={() => go({ s: "settings" })} aria-label="Settings"><Icon name="settings" /></button>
        </>}
      </div>
      <HubBanner />
      <RecoveryBanner onShow={() => go({ s: "set", tab: "recovery" })} />
      <TransfersStrip onOpen={(p) => go({ s: "conv", p })} />
      <div className="fchips">{f("all", "All")}{f("agents", "Agents")}{f("people", "People")}</div>
      <div className="scr-body">
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

export function Android() {
  const [stack, setStack] = useState<Scr[]>([{ s: "chats" }]);
  const [dir, setDir] = useState<"enter" | "back" | "">("");
  const depth = useRef(1);
  const top = stack[stack.length - 1];
  const fg = useForeground("android");
  useReadTracking(top.s === "conv" ? top.p : null, fg);

  const stackRef = useRef(stack);
  stackRef.current = stack;
  const go = useCallback((s: Scr) => {
    const st = stackRef.current;
    const t = st[st.length - 1];
    setDir("enter");
    // opening a chat from New chat / Directory replaces those screens
    if (s.s === "conv" && (t.s === "newchat" || t.s === "directory")) { setStack([...st.slice(0, -1), s]); return; }
    history.pushState({ hc: st.length + 1 }, "");
    depth.current = st.length + 1;
    setStack([...st, s]);
  }, []);
  const back = useCallback(() => { if (depth.current > 1) history.back(); }, []);

  useEffect(() => {
    history.replaceState({ hc: 1 }, "");
    const pop = (e: PopStateEvent) => {
      const d = (e.state && typeof e.state.hc === "number" ? e.state.hc : 1) as number;
      depth.current = d;
      setDir("back");
      setStack((st) => st.slice(0, Math.max(1, d)));
    };
    const key = (e: KeyboardEvent) => { if (e.key === "Escape" && !document.querySelector(".dialog")) back(); };
    window.addEventListener("popstate", pop);
    window.addEventListener("keydown", key);
    return () => { window.removeEventListener("popstate", pop); window.removeEventListener("keydown", key); };
  }, [back]);

  // a tapped message notification names a chat: open it (at start, and
  // whenever the app comes back to the foreground)
  useEffect(() => {
    const take = () => {
      if (document.visibilityState !== "visible") return;
      api.takePendingChat().then((p) => {
        if (!p) return;
        const t = stackRef.current[stackRef.current.length - 1];
        if (t.s === "conv" && t.p === p) return;
        go({ s: "conv", p });
      }, () => {});
    };
    take();
    window.addEventListener("focus", take);
    document.addEventListener("visibilitychange", take);
    return () => { window.removeEventListener("focus", take); document.removeEventListener("visibilitychange", take); };
  }, [go]);

  // a phone camera opened a hubchat:// link: a new device showed that code, so
  // look it up on Link a device › Approve a code
  usePendingLink((input) => go({ s: "link", tab: "approve", input }));

  const onInfo = useCallback((m: Message) => go({ s: "msginfo", id: m.id, p: m.peer }), [go]);
  const openChat = useCallback((a: string) => go({ s: "conv", p: a }), [go]);

  let screen;
  switch (top.s) {
    case "chats": screen = <Chats go={go} />; break;
    case "conv": return <Shell dir={dir} k={"conv:" + top.p}><Conversation key={top.p} peer={top.p} onBack={back} onInfo={onInfo} onOpenAddr={openChat} /></Shell>;
    case "msginfo": screen = <MsgInfoScreen id={top.id} peer={top.p} back={back} />; break;
    case "newchat": screen = <NewChatScreen back={back} open={openChat} dir={() => go({ s: "directory" })} />; break;
    case "directory": return <Shell dir={dir} k="directory"><Directory onOpen={openChat} onBack={back} /></Shell>;
    case "settings": return <Shell dir={dir} k="settings"><SettingsList onOpen={(tab) => go({ s: "set", tab })} onBack={back} /></Shell>;
    case "set": screen = (
      <>
        <div className="appbar flat"><button className="icon-btn" onClick={back} aria-label="Back"><Icon name="back" /></button><div className="title">{TABS.find((t) => t[0] === top.tab)![1]}</div></div>
        <div className="scr-body"><SettingsSection tab={top.tab} onLink={() => go({ s: "link" })} /></div>
      </>
    ); break;
    case "link": screen = (
      <>
        <div className="appbar flat"><button className="icon-btn" onClick={back} aria-label="Back"><Icon name="back" /></button><div className="title">Link a device</div></div>
        <div className="scr-body"><LinkDevice key={top.input || ""} initial={top.tab} input={top.input} onClose={back} onRecovery={() => go({ s: "set", tab: "recovery" })} /></div>
      </>
    ); break;
  }
  return <Shell dir={dir} k={top.s + stack.length}><div className="scr">{screen}</div></Shell>;
}

/** The screen root; the inner .scr is keyed so a push or pop animates it. */
function Shell({ dir, k, children }: { dir: string; k: string; children: React.ReactNode }) {
  return (
    <div className="screen">
      <div className="viewport"><div key={k} className={"scr-anim " + dir} style={{ position: "absolute", inset: 0 }}>{children}</div></div>
      <Toasts />
    </div>
  );
}
