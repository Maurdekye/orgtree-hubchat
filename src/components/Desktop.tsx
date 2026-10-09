// Windows layout: sidebar (chats) + conversation, overlays as modals, the
// info panel on the right (Contact info, or one message's info). The window
// frame is native.
import { useCallback, useEffect, useRef, useState } from "react";
import type { Message } from "../api";
import { Icon, Logo } from "../lib/icons";
import { copyText, errText } from "../lib/native";
import { routeLink, startJoin, useJoin } from "../lib/join";
import { toast } from "../lib/toast";
import { hubSummary } from "../lib/peers";
import { useSnap } from "../lib/store";
import { useActive, useForeground, useMessage, usePendingLink, useReadTracking } from "../lib/visibility";
import { isSetupLink, startSetup, useSetupChat } from "../lib/setup";
import { chatLinkTarget } from "../lib/chatlink";
import { startUpdateChecks } from "../lib/updates";
import { HubBanner, RecoveryBanner, UpdateBanner } from "./Banners";
import { ChatRows, EmptyChats, RailRows, useFilteredChats, type ChatFilter } from "./ChatList";
import { ContactInfo } from "./ContactInfo";
import { Conversation } from "./Conversation";
import { Directory } from "./Directory";
import { JoinFlow } from "./JoinLink";
import { LinkDeviceModal, type LinkTab } from "./LinkDevice";
import { MessageInfoBody } from "./MessageInfo";
import { MessageView } from "./MessageView";
import { NewChatInput, NewChatResults, useResolve } from "./NewChat";
import { SettingsModal, type SetTab } from "./Settings";
import { TransfersChip } from "./Transfers";
import { ImageViewer } from "./AttImage";
import { Addr, Avatar, Modal, ModalHead, Toasts } from "./ui";

type Overlay = null | { k: "newchat"; q: string } | { k: "directory" } | { k: "settings"; tab: SetTab } | { k: "link"; tab: LinkTab; input?: string };
/** The right-hand panel: the open chat's Contact info, or one message's info. */
type Info = null | { k: "contact" } | { k: "msg"; id: string; peer: string };

function NewChatModal({ initial, onOpen, onClose, onDirectory }: { initial: string; onOpen: (a: string) => void; onClose: () => void; onDirectory: () => void }) {
  const [q, setQ] = useState(initial);
  const r = useResolve(q);
  const enter = () => {
    if (r?.exact) onOpen(r.exact.address);
    else if (r && r.matches.length === 1) onOpen(r.matches[0].address);
  };
  return (
    <Modal onClose={onClose}>
      <ModalHead title="New chat" onClose={onClose} />
      <div className="modal-b">
        <NewChatInput q={q} setQ={setQ} onEnter={enter} />
        <div className="nc-res"><NewChatResults q={q} r={r} onOpen={onOpen} onDirectory={onDirectory} /></div>
      </div>
    </Modal>
  );
}

function InfoPanel({ id, peer, onClose }: { id: string; peer: string; onClose: () => void }) {
  const m = useMessage(id, peer);
  const noop = useCallback(() => {}, []);
  return (
    <aside className="info">
      <div className="info-head"><span>Message info</span><button className="icon-btn" onClick={onClose} title="Close" aria-label="Close"><Icon name="close" /></button></div>
      <div className="info-body scroll">
        {m ? <>
          <div className="mi-preview"><MessageView m={m} first peerName="" peerKind="person" quoted={undefined} hover={false} onReply={noop} onInfo={noop} onDelete={noop} onJump={noop} onOpenAddr={noop} /></div>
          <MessageInfoBody m={m} />
        </> : <div className="list-empty">This message is no longer on this device.</div>}
      </div>
    </aside>
  );
}

const RAIL_KEY = "hubchat.chatlist.rail";

export function Desktop() {
  const snap = useSnap();
  const me = snap.state!.me!;
  const hubs = snap.state!.hubs;
  const [chat, setChat] = useState<string | null>(null);
  const [filter, setFilter] = useState<ChatFilter>("all");
  const [q, setQ] = useState("");
  // the chat list as a narrow rail of avatars (user 2026-10-09 08:39Z), kept across restarts
  const [rail, setRailState] = useState(() => localStorage.getItem(RAIL_KEY) === "1");
  const setRail = useCallback((on: boolean) => {
    localStorage.setItem(RAIL_KEY, on ? "1" : "0");
    // the rail has no search box or filters, so it must not hide chats behind them
    if (on) { setQ(""); setFilter("all"); }
    setRailState(on);
  }, []);
  const [ov, setOv] = useState<Overlay>(null);
  const [info, setInfo] = useState<Info>(null);
  const fg = useForeground("desktop");
  useActive(fg);
  useReadTracking(ov ? null : chat, fg);
  useEffect(() => { startUpdateChecks("desktop"); }, []);

  // the panel stays open across chats: Contact info follows the open chat,
  // another chat's message info gives way to it (the prototype's openChat)
  const open = useCallback((peer: string) => { setChat(peer); setOv(null); setInfo((i) => (i?.k === "msg" && i.peer !== peer ? { k: "contact" } : i)); }, []);
  const onInfo = useCallback((m: Message) => setInfo({ k: "msg", id: m.id, peer: m.peer }), []);
  const toggleContact = useCallback(() => setInfo((i) => (i?.k === "contact" ? null : { k: "contact" })), []);
  // the right-click menus' "Contact info": that chat, with its panel
  const contactInfo = useCallback((peer: string) => { open(peer); setInfo({ k: "contact" }); }, [open]);
  const settings = (tab: SetTab) => setOv({ k: "settings", tab });
  // a hubchat:// link the system opened us with: a signed-in device's (role
  // give) is joined; a new device's (role take) is approved here
  // Scan setup code opens the org's chat when it's done
  useSetupChat((p) => open(p));
  usePendingLink((input) => {
    if (isSetupLink(input)) { startSetup(input); return; }
    // a profile QR's chat link opens that chat; your own opens your profile
    const to = chatLinkTarget(input);
    if (to !== null) {
      if (!to) toast("That chat link doesn't name an address.");
      else if (to === snap.state?.me?.address) settings("profile");
      else open(to);
      return;
    }
    routeLink(input, "approve").then((r) => (r.k === "join" ? startJoin(r.p.code, r.p.hub, r.p.hubs, r.p.hub_name) : setOv({ k: "link", tab: "approve", input: r.input })), (e) => toast(errText(e)));
  });
  // joining another identity's link: it replaces whatever overlay started it
  const join = useJoin();
  useEffect(() => { if (join) setOv(null); }, [join]);

  // Alt+Up / Alt+Down (user 2026-10-09 08:35Z): the chat above or below the
  // open one in the list as it shows now (sorted, filtered); the new chat's
  // message box takes the focus once it has mounted
  const rowsRef = useRef<{ peer: string }[]>([]);
  const railRef = useRef(rail);
  railRef.current = rail;
  const focusBox = useRef(false);
  useEffect(() => {
    if (!chat || !focusBox.current) return;
    focusBox.current = false;
    document.querySelector<HTMLTextAreaElement>(".conv .composer textarea")?.focus();
    document.querySelector(".clist .sel")?.scrollIntoView({ block: "nearest" });
  }, [chat]);

  useEffect(() => {
    const key = (e: KeyboardEvent) => {
      if (e.altKey && !e.ctrlKey && !e.shiftKey && !e.metaKey && (e.key === "ArrowUp" || e.key === "ArrowDown")) {
        // under a modal, the picture viewer or a join the list isn't in play
        if (ov || join || document.querySelector(".scrim, .imgview")) return;
        e.preventDefault();
        const rs = rowsRef.current, down = e.key === "ArrowDown";
        const i = rs.findIndex((r) => r.peer === chat);
        // no chat open, or the open one filtered out: Down starts at the top, Up has nowhere to go
        const to = i < 0 ? (down ? rs[0] : undefined) : rs[i + (down ? 1 : -1)];
        if (to) { focusBox.current = true; open(to.peer); }
      }
      else if (e.ctrlKey && !e.shiftKey && !e.altKey && e.key.toLowerCase() === "n") { e.preventDefault(); setOv({ k: "newchat", q: "" }); }
      else if (e.ctrlKey && e.key === ",") { e.preventDefault(); setOv({ k: "settings", tab: "profile" }); }
      else if (e.ctrlKey && e.key.toLowerCase() === "k") {
        e.preventDefault();
        // the search box lives in the full panel: bring it back first
        if (railRef.current) { setRail(false); setTimeout(() => document.getElementById("chat-q")?.focus(), 0); }
        else document.getElementById("chat-q")?.focus();
      }
      else if (e.ctrlKey && !e.shiftKey && !e.altKey && e.key.toLowerCase() === "i") {
        // Contact info, while a chat is open and nothing covers it
        if (chat && !ov && !document.querySelector(".scrim")) { e.preventDefault(); toggleContact(); }
      }
      else if (e.key === "Escape") {
        if (document.querySelector(".modal.confirm, .modal.link3")) return;
        if (ov) { e.preventDefault(); setOv(null); } else if (info) setInfo(null);
      }
    };
    window.addEventListener("keydown", key);
    return () => window.removeEventListener("keydown", key);
  }, [ov, join, info, chat, open, toggleContact, setRail]);

  const { rows, counts, total } = useFilteredChats(filter, q, chat);
  rowsRef.current = rows;
  const hs = hubSummary(hubs);
  const fchip = (k: ChatFilter, label: string) => (
    <button className={"fchip" + (filter === k ? " on" : "")} onClick={() => setFilter(k)}>{label}{counts[k] ? <span className="n">{counts[k]}</span> : null}</button>
  );

  return (
    <div className="app">
      <UpdateBanner />
      <HubBanner />
      <RecoveryBanner onShow={() => settings("recovery")} />
      <div className="body">
        {rail ? (
          <aside className="side rail">
            <button className="icon-btn" onClick={() => setRail(false)} title="Show the chat list" aria-label="Show the chat list"><Icon name="chevron_right" /></button>
            <span className="rail-me" onClick={() => settings("profile")} title="Profile"><Avatar kind="me" name={me.name || me.id} size={32} /></span>
            <TransfersChip onOpen={open} />
            <button className="icon-btn" onClick={() => setOv({ k: "newchat", q: "" })} title="New chat (Ctrl+N)" aria-label="New chat"><Icon name="new_chat" /></button>
            <button className="icon-btn" onClick={() => setOv({ k: "directory" })} title="Directory: everyone on your hubs" aria-label="Directory"><Icon name="contacts" /></button>
            <button className="icon-btn" onClick={() => settings("profile")} title="Settings (Ctrl+,)" aria-label="Settings"><Icon name="settings" /></button>
            <button className={"hubpill rail-hub " + hs.state} onClick={() => settings("hubs")} title={hs.text + " · Hub connections"} aria-label="Hub connections"><span className="dot" /></button>
            <div className="clist scroll rail-list">
              <RailRows rows={rows} selected={chat} onOpen={open} onInfo={contactInfo} />
            </div>
          </aside>
        ) : (
        <aside className="side">
          <div className="side-head">
            <span onClick={() => settings("profile")} title="Profile" style={{ cursor: "pointer", marginRight: 8 }}><Avatar kind="me" name={me.name || me.id} size={32} /></span>
            <div className="side-title">Chats</div>
            <TransfersChip onOpen={open} />
            <button className="icon-btn" onClick={() => setOv({ k: "newchat", q: "" })} title="New chat (Ctrl+N)" aria-label="New chat"><Icon name="new_chat" /></button>
            <button className="icon-btn" onClick={() => setOv({ k: "directory" })} title="Directory: everyone on your hubs" aria-label="Directory"><Icon name="contacts" /></button>
            <button className="icon-btn" onClick={() => settings("profile")} title="Settings (Ctrl+,)" aria-label="Settings"><Icon name="settings" /></button>
          </div>
          <div className="side-sub">
            <button className={"hubpill " + hs.state} onClick={() => settings("hubs")} title="Hub connections">
              <span className="dot" /><span>{hs.text}</span><Icon name="chevron_right" />
            </button>
            <button className="icon-btn" onClick={() => setRail(true)} title="Shrink the chat list to icons" aria-label="Shrink the chat list"><Icon name="chevron_left" /></button>
          </div>
          <label className="search input"><Icon name="search" /><input id="chat-q" value={q} onChange={(e) => setQ(e.target.value)} placeholder="Search chats" autoComplete="off" spellCheck={false} /></label>
          <div className="filters">{fchip("all", "All")}{fchip("agents", "Agents")}{fchip("people", "People")}</div>
          <div className="clist scroll">
            {!total && !chat ? <EmptyChats onNew={() => setOv({ k: "newchat", q: "" })} />
              : rows.length ? <ChatRows rows={rows} selected={chat} onOpen={open} onInfo={contactInfo} />
              : <div className="list-empty"><Icon name="search" /><b>No chats match</b>To reach someone new, use <button className="link" onClick={() => setOv({ k: "newchat", q })}>New chat</button> and type their address.</div>}
          </div>
          <div className="side-foot">
            <span>You</span>
            <span className="addr ell" title="Your address: share it so people and agents can reach you"><Addr a={me.address} net /></span>
            <button className="icon-btn" onClick={() => copyText("@net:" + me.address, "Address copied")} title="Copy your address" aria-label="Copy your address"><Icon name="copy" /></button>
            <button className="icon-btn side-qr" onClick={() => setOv({ k: "link", tab: "offer" })} title="Link a device: show QR code" aria-label="Link a device"><Icon name="qr" /></button>
          </div>
        </aside>
        )}
        {chat ? <Conversation key={chat} peer={chat} onInfo={onInfo} onOpenAddr={open} onContact={toggleContact} infoOn={!!info && (info.k === "contact" || info.peer === chat)} /> : (
          <section className="conv" role="main">
            <div className="conv-empty">
              <Logo size={64} />
              <h2>{total ? "Pick a chat" : "Welcome to Hubchat"}</h2>
              <p>{total ? "Or start a new one with an address." : "Start a chat by typing an address: an Orgtree org, an agent session (such as Claude Code or Codex), or a person."}</p>
              <button className="btn primary lg" onClick={() => setOv({ k: "newchat", q: "" })}><Icon name="new_chat" />New chat</button>
              <div className="help" style={{ marginTop: 18 }}>Your address: <span className="code-pill"><Addr a={me.address} net /></span> <button className="link" onClick={() => copyText("@net:" + me.address, "Address copied")}>Copy</button></div>
            </div>
          </section>
        )}
        {chat && info?.k === "contact" ? <ContactInfo peer={chat} onClose={() => setInfo(null)} /> : null}
        {chat && info?.k === "msg" && info.peer === chat ? <InfoPanel id={info.id} peer={info.peer} onClose={() => setInfo(null)} /> : null}
      </div>
      {ov?.k === "newchat" ? <NewChatModal initial={ov.q} onOpen={open} onClose={() => setOv(null)} onDirectory={() => setOv({ k: "directory" })} /> : null}
      {ov?.k === "directory" ? <Directory onOpen={open} onInfo={contactInfo} onClose={() => setOv(null)} /> : null}
      {ov?.k === "link" ? <LinkDeviceModal initial={ov.tab} input={ov.input} onClose={() => setOv(null)} onRecovery={() => settings("recovery")} /> : null}
      {ov?.k === "settings" ? <SettingsModal tab={ov.tab} setTab={(t) => setOv({ k: "settings", tab: t })} onClose={() => setOv(null)} /> : null}
      {join ? <JoinFlow key={join.n} req={join} /> : null}
      <ImageViewer />
      <Toasts />
    </div>
  );
}
