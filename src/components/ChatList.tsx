// The chat list rows (desktop sidebar and Android's first screen).
import type { MouseEvent as ReactMouseEvent } from "react";
import type { ChatSummary } from "../api";
import { Icon, Logo } from "../lib/icons";
import { shortWhen } from "../lib/format";
import { displayName, isAgent, kindOf, msgTime, preview } from "../lib/peers";
import { useSnap } from "../lib/store";
import { openMenu } from "../lib/ctxmenu";
import { copyText } from "../lib/native";
import { Addr, KindGlyph, PeerAvatar, Tick, usePlatform } from "./ui";

export type ChatFilter = "all" | "agents" | "people";

export function useFilteredChats(filter: ChatFilter, q: string, open: string | null) {
  const snap = useSnap();
  let chats: (ChatSummary | { peer: string; last: null; unread: 0 })[] = snap.chats;
  // a chat opened from New chat shows in the list before its first message
  if (open && !chats.some((c) => c.peer === open)) chats = [{ peer: open, last: null, unread: 0 }, ...chats];
  const qq = q.trim().toLowerCase();
  const counts = { all: chats.length, agents: 0, people: 0 };
  const rows = chats.filter((c) => {
    const ct = snap.byAddr.get(c.peer);
    if (isAgent(ct)) counts.agents++; else counts.people++;
    if (filter === "agents" && !isAgent(ct)) return false;
    if (filter === "people" && isAgent(ct)) return false;
    if (qq && !displayName(ct, c.peer).toLowerCase().includes(qq) && !c.peer.includes(qq)) return false;
    return true;
  });
  return { rows, counts, total: snap.chats.length };
}

/** Desktop right-click on a chat (user 2026-10-09 05:54Z): what the chat's
 *  header and Android's chat menu offer. */
export function chatMenu(e: ReactMouseEvent, peer: string, onOpen: (peer: string) => void, onInfo?: (peer: string) => void, open = "Open") {
  e.preventDefault();
  openMenu(e.clientX, e.clientY, [
    { label: open, icon: "forum", run: () => onOpen(peer) },
    ...(onInfo ? [{ label: "Contact info", icon: "info" as const, run: () => onInfo(peer) }] : []),
    { label: "Copy address", icon: "copy", run: () => void copyText("@net:" + peer, "Address copied") },
  ]);
}

export function ChatRows({ rows, selected, onOpen, onInfo }: { rows: ReturnType<typeof useFilteredChats>["rows"]; selected: string | null; onOpen: (peer: string) => void; onInfo?: (peer: string) => void }) {
  const snap = useSnap();
  const platform = usePlatform();
  const hubs = snap.state?.hubs || [];
  return (
    <>
      {rows.map((c) => {
        const ct = snap.byAddr.get(c.peer);
        const m = c.last;
        const draft = c.peer !== selected ? snap.drafts[c.peer] : "";
        let prev;
        if (draft) prev = <><span className="q" style={{ color: "var(--accent-text)" }}>Draft:</span> <span className="t">{draft}</span></>;
        else if (!m) prev = <span className="t">No messages yet</span>;
        else if (m.outgoing) prev = <><Tick m={m} /><span className="t">You: {preview(m)}</span></>;
        else prev = <>{m.kind === "question" ? <span className="q">Question ·</span> : null}{m.kind === "question" ? " " : null}<span className="t">{preview(m)}</span></>;
        return (
          <div key={c.peer} className={"crow" + (selected === c.peer ? " sel" : "") + (c.unread ? " unread" : "")} onClick={() => onOpen(c.peer)} role="button" tabIndex={0}
            onContextMenu={platform === "desktop" ? (e) => chatMenu(e, c.peer, onOpen, onInfo) : undefined}
            onKeyDown={(e) => { if (e.key === "Enter") onOpen(c.peer); }}>
            <PeerAvatar address={c.peer} c={ct} hubs={hubs} size={platform === "android" ? 52 : 44} />
            <div className="crow-main">
              <div className="crow-l1"><span className="crow-name">{displayName(ct, c.peer)}</span><KindGlyph kind={kindOf(ct)} /><span className="crow-time">{m ? shortWhen(msgTime(m)) : ""}</span></div>
              <div className="crow-l2"><span className="crow-prev">{prev}</span>{c.unread ? <span className="badge">{c.unread}</span> : null}</div>
            </div>
          </div>
        );
      })}
    </>
  );
}

/** The collapsed chat list (desktop, user 2026-10-09 08:39Z): one avatar per
 *  chat with its presence dot and unread badge; the name shows on hover. */
export function RailRows({ rows, selected, onOpen, onInfo }: { rows: ReturnType<typeof useFilteredChats>["rows"]; selected: string | null; onOpen: (peer: string) => void; onInfo?: (peer: string) => void }) {
  const snap = useSnap();
  const hubs = snap.state?.hubs || [];
  return (
    <>
      {rows.map((c) => {
        const ct = snap.byAddr.get(c.peer);
        const name = displayName(ct, c.peer);
        return (
          <button key={c.peer} className={"rrow" + (selected === c.peer ? " sel" : "") + (c.unread ? " unread" : "")} onClick={() => onOpen(c.peer)}
            title={name} aria-label={name + (c.unread ? ", " + c.unread + " unread" : "")}
            onContextMenu={(e) => chatMenu(e, c.peer, onOpen, onInfo)}>
            <PeerAvatar address={c.peer} c={ct} hubs={hubs} size={40} />
            {c.unread ? <span className="badge">{c.unread}</span> : null}
          </button>
        );
      })}
    </>
  );
}

export function EmptyChats({ onNew }: { onNew: () => void }) {
  const snap = useSnap();
  const platform = usePlatform();
  const me = snap.state?.me;
  if (platform === "android") {
    return (
      <div className="empty"><Logo size={72} /><b>No chats yet</b>Start one with an address: an Orgtree org, an agent session (such as Claude Code or Codex) or a person.
        {me ? <div className="addr">Your address<br /><span className="code-pill"><Addr a={me.address} net /></span></div> : null}
      </div>
    );
  }
  return (
    <div className="list-empty"><Icon name="forum" /><b>No chats yet</b>Start one with an address: an Orgtree org, an agent session (such as Claude Code or Codex) or a person.
      <div style={{ marginTop: 14 }}><button className="btn primary" onClick={onNew}><Icon name="new_chat" />New chat</button></div>
    </div>
  );
}
