// Contact info (the prototype's contactHTML on desktop, SCR.info on Android):
// who an address is, which of your hubs reach it, and the files in the chat.
// Desktop shows it in the right-hand panel (the conversation header, its info
// button, Ctrl+I); Android as a screen from the conversation's menu or a tap
// on its name. An address no hub lists still opens: its name is the address.
import { Icon, type IconName } from "../lib/icons";
import { bytes, shortWhen } from "../lib/format";
import { copyText } from "../lib/native";
import { displayName, hubCls, hubStatusText, kindInfo, kindOf, msgTime, peerHubs, splitAddr, viaHub } from "../lib/peers";
import { useMessages, useSnap } from "../lib/store";
import { Addr, KindChip, PeerAvatar, PresText, useNow, usePlatform } from "./ui";

const ONLINE_NOTE = "“Online” means its Orgtree is connected to the hub — not that an agent is typing.";
const fileIcon = (name: string): IconName => (/\.(png|jpe?g|gif|webp|bmp|svg|avif|heic)$/i.test(name) ? "image" : "file");
const bare = (url: string) => url.replace(/^https?:\/\//, "");

/** `onClose`: desktop closes the panel; Android goes back to the chat. */
export function ContactInfo({ peer, onClose }: { peer: string; onClose: () => void }) {
  const platform = usePlatform();
  const snap = useSnap();
  const hubs = snap.state?.hubs || [];
  const c = snap.byAddr.get(peer);
  const name = displayName(c, peer);
  const kind = kindOf(c);
  const k = kindInfo(kind);
  const ph = peerHubs(c, hubs);
  const via = viaHub(c, hubs);
  const now = useNow(ph.some((h) => h.state === "disconnected"));
  const { msgs, loaded } = useMessages(peer);
  const files = msgs.flatMap((m) => m.attachments.map((a) => ({ a, m })));
  const tag = splitAddr(peer)[1];
  const blurb = (c?.blurb || "").trim();
  const copy = () => void copyText("@net:" + peer, "Address copied");

  if (platform === "android") {
    return (
      <>
        <div className="appbar flat"><button className="icon-btn" onClick={onClose} aria-label="Back"><Icon name="back" /></button><div className="title">Contact info</div></div>
        <div className="scr-body">
          <div className="hero">
            <PeerAvatar address={peer} c={c} hubs={hubs} size={96} />
            <div className="name">{name}</div>
            {c ? <KindChip kind={kind} /> : null}
            <div><PresText c={c} hubs={hubs} /></div>
          </div>
          <div className="quick">
            <button onClick={onClose}><Icon name="forum" />Message</button>
            <button onClick={copy}><Icon name="copy" />Copy address</button>
          </div>
          <div className="sec-h">Address</div>
          <div className="li">
            <Icon name="at" />
            <div className="t">
              <div className="t1 mono" style={{ fontSize: 14 }}><span style={{ overflowWrap: "anywhere" }}><Addr a={peer} net /></span></div>
              {tag ? <div className="t2">The tag <span className="mono">{tag}</span> comes from their key. Two people can share an id; the tag tells them apart.</div> : null}
            </div>
          </div>
          {blurb ? <><div className="sec-h">About</div><div className="li"><Icon name="info" /><div className="t"><div className="t1" style={{ fontSize: 15, overflowWrap: "anywhere" }}>{blurb}</div></div></div></> : null}
          {c ? <><div className="sec-h">What this is</div><div className="li"><Icon name={k.icon} /><div className="t"><div className="t2" style={{ fontSize: 14, color: "var(--ink)" }}>{k.what}{kind === "org" ? " " + ONLINE_NOTE : ""}</div></div></div></> : null}
          <div className="sec-h">Reachable through</div>
          {ph.length ? ph.map((h) => (
            <div className="li" key={h.url}>
              <Icon name="dns" />
              <div className="t"><div className="t1">{h.name}</div><div className="t2 mono">{bare(h.url)}</div></div>
              <span className={"hubst " + hubCls(h)}><span className="dot" />{h.state === "connected" ? "connected" : hubCls(h)}</span>
            </div>
          )) : <div className="li"><div className="t"><div className="t2">None of your hubs</div></div></div>}
          <div className="sec-h">Files</div>
          {!loaded ? null : files.length ? files.map(({ a, m }) => (
            <div className="li" key={a.local_id}>
              <Icon name={fileIcon(a.name)} />
              <div className="t"><div className="t1" style={{ fontSize: 15, overflowWrap: "anywhere" }}>{a.name}</div><div className="t2">{bytes(a.bytes)} · {shortWhen(msgTime(m))}</div></div>
            </div>
          )) : <div className="li"><div className="t"><div className="t2">No files yet</div></div></div>}
        </div>
      </>
    );
  }

  return (
    <aside className="info" aria-label="Contact info">
      <div className="info-head"><span>Contact info</span><button className="icon-btn" onClick={onClose} title="Close" aria-label="Close"><Icon name="close" /></button></div>
      <div className="info-body scroll">
        <div className="info-hero">
          <PeerAvatar address={peer} c={c} hubs={hubs} size={96} />
          <div className="name">{name}</div>
          {c ? <KindChip kind={kind} /> : null}
          <div style={{ marginTop: 4 }}><PresText c={c} hubs={hubs} /></div>
        </div>
        <div className="info-sec">
          <h4>Address</h4>
          <div className="addrbox"><span title={"@net:" + peer}><Addr a={peer} net /></span><button className="icon-btn" onClick={copy} title="Copy" aria-label="Copy address"><Icon name="copy" /></button></div>
          {tag ? <div className="help" style={{ marginTop: 6 }}>The last part, <span className="mono">{tag}</span>, comes from their key. Two people can share an id; the tag tells them apart.</div> : null}
        </div>
        {blurb ? <div className="info-sec"><h4>About</h4><div style={{ fontSize: 13, overflowWrap: "anywhere" }}>{blurb}</div></div> : null}
        {c ? (
          <div className="info-sec">
            <h4>What this is</h4>
            <div style={{ fontSize: 13, color: "var(--ink)" }}>{k.what}{kind === "org" ? <> <span className="dim">{ONLINE_NOTE}</span></> : null}</div>
          </div>
        ) : null}
        <div className="info-sec">
          <h4>Reachable through</h4>
          {ph.length ? ph.map((h) => (
            <div className="hubline" key={h.url}>
              <Icon name="dns" />
              <span className="hl-t"><b>{h.name}</b> <span className="mono dim" style={{ fontSize: 11.5 }}>{bare(h.url)}</span></span>
              <span className={"st" + (h.state === "connected" ? " ok" : "")}>{h.state === "connected" ? (h.url === via?.url ? "in use" : "connected") : hubStatusText(h, now)}</span>
            </div>
          )) : <div className="dim" style={{ fontSize: 13 }}>None of your hubs</div>}
        </div>
        <div className="info-sec">
          <h4>Files</h4>
          {!loaded ? null : files.length ? files.map(({ a, m }) => (
            <div className="filerow" key={a.local_id} title={a.name}>
              <span className="att-ic"><Icon name={fileIcon(a.name)} /></span>
              <span className="t"><span className="ell">{a.name}</span><span>{bytes(a.bytes)} · {shortWhen(msgTime(m))}</span></span>
            </div>
          )) : <div className="dim" style={{ fontSize: 13 }}>No files yet</div>}
        </div>
      </div>
    </aside>
  );
}
