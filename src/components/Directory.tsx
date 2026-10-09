// Directory: everyone on all your hubs in one merged list, online first, then
// seen in the last 7 days, then the rest. Filters: the hub's address kinds
// (people, orgs, chats) and, with several hubs, the hub.
import { useState } from "react";
import type { Contact } from "../api";
import { Icon } from "../lib/icons";
import { ms } from "../lib/format";
import { displayName, kindOf, presence, type Kind } from "../lib/peers";
import { useSnap } from "../lib/store";
import { HubHelpLink } from "./HubHelp";
import { Addr, Avatar, KindGlyph, Modal, ModalHead, PeerAvatar, usePlatform } from "./ui";

type KindF = "all" | Kind;

/** What a device waiting to be linked says about itself (src-tauri/src/link.rs). */
const LINK_WAITING = "Waiting to be linked to a Hubchat identity";
/** A throwaway link-a-device address (`link.<tag>` with that about line), never
 *  a person; a real "link" with any other about line is listed as usual. */
export const isLinkWaiting = (c: Contact) => /^link\.[^.]+$/.test(c.address) && c.blurb === LINK_WAITING;
/** The directory as people see it: without you and without waiting link addresses. */
export const listed = (dir: Contact[], me: string | undefined) => dir.filter((c) => c.address !== me && !isLinkWaiting(c));

export function Directory({ onOpen, onClose, onBack }: { onOpen: (address: string) => void; onClose?: () => void; onBack?: () => void }) {
  const snap = useSnap();
  const platform = usePlatform();
  const hubs = snap.state?.hubs || [];
  const me = snap.state?.me;
  const [q, setQ] = useState("");
  const [kf, setKf] = useState<KindF>("all");
  const [hub, setHub] = useState<string>("all");

  const all = listed(snap.directory, me?.address);
  const qq = q.trim().toLowerCase();
  const shown = all.filter((c) => {
    if (kf !== "all" && kindOf(c) !== kf) return false;
    if (hub !== "all" && !c.hubs.includes(hub)) return false;
    if (qq && !(displayName(c, c.address).toLowerCase().includes(qq) || c.address.includes(qq) || c.blurb.toLowerCase().includes(qq))) return false;
    return true;
  });
  const week = Date.now() - 7 * 864e5;
  const live = (c: Contact) => presence(c, hubs).state;
  const byName = (a: Contact, b: Contact) => displayName(a, a.address).localeCompare(displayName(b, b.address));
  const online = shown.filter((c) => live(c) === "online").sort(byName);
  const rest = shown.filter((c) => live(c) !== "online");
  const recent = rest.filter((c) => ms(c.last_seen) >= week).sort((a, b) => ms(b.last_seen) - ms(a.last_seen));
  const quiet = rest.filter((c) => ms(c.last_seen) < week).sort((a, b) => ms(b.last_seen) - ms(a.last_seen) || byName(a, b));
  const onlineCount = all.filter((c) => live(c) === "online").length;
  const hubN = (u: string) => hubs.find((h) => h.url === u);

  const row = (c: Contact) => {
    const p = presence(c, hubs);
    const via = c.hubs.map((u) => { const h = hubN(u); return h ? <span key={u} className={"chip hubchip" + (h.state === "connected" ? "" : " stale")} title={"Reachable through " + h.name + (h.state === "connected" ? "" : " (not connected)")}>{h.name}</span> : null; });
    const started = snap.chats.some((x) => x.peer === c.address);
    return (
      <div className="nc-row dir-row" key={c.address} onClick={() => onOpen(c.address)} role="button">
        <PeerAvatar address={c.address} c={c} hubs={hubs} size={platform === "android" ? 44 : 40} />
        <div className="t">
          <div className="t1">{displayName(c, c.address)} <KindGlyph kind={kindOf(c)} /></div>
          <div className="t2"><Addr a={c.address} /></div>
          {c.blurb ? <div className="t3">{c.blurb}</div> : null}
          {platform === "android" ? <div className="via">{via}</div> : null}
        </div>
        {platform === "desktop" ? <div className="via">{via}</div> : null}
        <div className="r">
          <span>{p.state === "online" ? "online" : p.short}</span>
          {platform === "desktop" ? <span className="btn">{started ? "Open chat" : "Message"}</span> : null}
        </div>
      </div>
    );
  };
  const sec = (title: string, l: Contact[]) => (l.length ? <><div className="nc-sec">{title} · {l.length}</div>{l.map(row)}</> : null);

  const fchip = (k: KindF, label: string) => <button key={k} className={"fchip" + (kf === k ? " on" : "")} onClick={() => setKf(k)}>{label}</button>;
  const hchip = (u: string, label: string) => <button key={u} className={"fchip hubf" + (hub === u ? " on" : "")} onClick={() => setHub(u)}>{label}</button>;
  const filters = (
    <>
      {fchip("all", "All")}{fchip("person", "People")}{fchip("org", "Orgs")}{fchip("chat", "Chats")}
      {hubs.length > 1 ? <><span className="fsep" />{platform === "desktop" ? <span className="flabel">Hubs</span> : null}{hchip("all", platform === "android" ? "All hubs" : "All")}{hubs.map((h) => hchip(h.url, h.name))}</> : null}
    </>
  );
  const stale = hubs.filter((h) => h.state !== "connected");
  const body = (
    <>
      {!hubs.length ? <div className={platform === "android" ? "empty" : "list-empty"}><b>No hubs</b>Add a hub to see who is on it.<div className="hubhelp-cta"><HubHelpLink label="Don't have a mail hub?" /></div></div> : null}
      {stale.map((h) => (
        <div key={h.url} className="note-card warn" style={{ margin: platform === "android" ? "6px 20px 10px" : "0 0 10px" }}>
          <Icon name="cloud_off" /><div><b>Can't reach {h.name}.</b> Its part of the list may be out of date; status is unknown for anyone only on {h.name}. <HubHelpLink /></div>
        </div>
      ))}
      {me && hubs.length ? (
        <div className="dir-me">
          <Avatar kind="me" name={me.name || me.id} size={platform === "android" ? 40 : 36} />
          <div className="t"><b>You</b><span className="mono"><Addr a={me.address} /></span></div>
          {platform === "desktop" ? <span className="hubchips">{hubs.map((h) => <span key={h.url} className="chip hubchip">{h.name}</span>)}</span> : null}
          <span className="dim" style={{ fontSize: 12.5 }}>Everyone on your hubs sees you here</span>
        </div>
      ) : null}
      {sec("Online now", online)}{sec("Seen in the last 7 days", recent)}{sec("Not seen in over 7 days", quiet)}
      {hubs.length && !shown.length ? (
        <div className={platform === "android" ? "empty" : "list-empty"}>{qq || kf !== "all" || hub !== "all" ? <><b>No one matches</b>Try another filter, or type a full address in New chat.</> : <><b>No one else is on your hubs yet</b>Share your address so people and agents can reach you.</>}</div>
      ) : null}
    </>
  );
  const sub = all.length + " on your " + (hubs.length > 1 ? hubs.length + " hubs" : "hub") + " · " + onlineCount + " online";
  const search = <input autoFocus={platform === "desktop"} value={q} onChange={(e) => setQ(e.target.value)} placeholder={platform === "android" ? "Search everyone" : "Search everyone by name, address or about"} autoComplete="off" autoCapitalize="off" spellCheck={false} />;

  if (platform === "android") {
    return (
      <div className="scr">
        <div className="appbar flat">
          <button className="icon-btn" onClick={onBack} aria-label="Back"><Icon name="back" /></button>
          <div className="title">Directory<span className="sub">{sub}</span></div>
        </div>
        <label className="to-field"><span className="pre"><Icon name="search" /></span>{search}</label>
        <div className="fchips dir-f">{filters}</div>
        <div className="scr-body">{body}</div>
      </div>
    );
  }
  return (
    <Modal onClose={onClose!} className="dir1">
      <ModalHead title={<>Directory <span className="dim" style={{ fontSize: 13, fontWeight: 400 }}>Everyone on your hubs · {sub}</span></>} onClose={onClose} />
      <div className="dir-tools">
        <label className="input"><Icon name="search" />{search}</label>
        <div className="filters">{filters}</div>
      </div>
      <div className="dir-list scroll">{body}</div>
    </Modal>
  );
}
