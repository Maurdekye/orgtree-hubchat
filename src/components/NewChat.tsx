// New chat: the To: field takes @net:slug, slug or a bare id and resolves it
// live against the directory. Starting a chat only opens it; nothing is sent
// until you write.
import { useEffect, useState } from "react";
import { api, type Contact, type Resolved } from "../api";
import { Icon } from "../lib/icons";
import { displayName, kindInfo, kindOf, presence, viaHub } from "../lib/peers";
import { useSnap } from "../lib/store";
import { isLinkWaiting, listed } from "./Directory";
import { scanSetup } from "../lib/setup";
import { Addr, KindGlyph, PeerAvatar, pressKeys, usePlatform } from "./ui";

export function useResolve(q: string): Resolved | null {
  const [r, setR] = useState<Resolved | null>(null);
  useEffect(() => {
    if (!q.trim()) { setR(null); return; }
    let live = true;
    const t = setTimeout(() => { api.resolve(q).then((x) => { if (live) setR(x); }, () => {}); }, 180);
    return () => { live = false; clearTimeout(t); };
  }, [q]);
  return q.trim() ? r : null;
}

export function NewChatInput({ q, setQ, onEnter }: { q: string; setQ: (v: string) => void; onEnter: () => void }) {
  const platform = usePlatform();
  const input = <input id="nc-q" autoFocus value={q} placeholder="address or id" autoComplete="off" autoCapitalize="off" spellCheck={false}
    onChange={(e) => setQ(e.target.value)} onKeyDown={(e) => { if (e.key === "Enter") onEnter(); }} />;
  if (platform === "android") return <label className="to-field"><span className="pre">@net:</span>{input}</label>;
  return (
    <>
      <label className="input" style={{ height: 44 }}><span className="pre">@net:</span>{input}</label>
      <div className="help" style={{ margin: "8px 2px 4px" }}>Type the address someone gave you, like <span className="mono">research.alex.a3f9c1</span>, or just their id.</div>
    </>
  );
}

export function NewChatResults({ q, r, onOpen, onDirectory }: { q: string; r: Resolved | null; onOpen: (address: string) => void; onDirectory: () => void }) {
  const snap = useSnap();
  const platform = usePlatform();
  const hubs = snap.state?.hubs || [];
  const up = hubs.filter((h) => h.state === "connected");
  const down = hubs.filter((h) => h.state !== "connected");
  const names = (l: typeof hubs) => l.map((h, i) => <span key={h.url}>{i ? " and " : ""}<b style={{ display: "inline" }}>{h.name}</b></span>);
  const started = (a: string) => snap.chats.some((c) => c.peer === a);
  // the Directory's own list and counts (no waiting link addresses, not you)
  const everyone = listed(snap.directory, snap.state?.me?.address);
  const onlineN = everyone.filter((c) => presence(c, hubs).state === "online").length;

  const row = (c: Contact) => {
    const p = presence(c, hubs);
    return (
      <div className="nc-row" key={c.address} onClick={() => onOpen(c.address)} role="button" tabIndex={0} onKeyDown={pressKeys(() => onOpen(c.address))}>
        <PeerAvatar address={c.address} c={c} hubs={hubs} size={platform === "android" ? 44 : 36} />
        <div className="t"><div className="t1">{displayName(c, c.address)} <KindGlyph kind={kindOf(c)} /></div><div className="t2"><Addr a={c.address} /></div></div>
        <div className="r"><span>{p.state === "online" ? "online" : p.short}</span>
          {platform === "desktop" && hubs.length > 1 ? <span className="chip hubchip">{c.hubs.map((u) => hubs.find((h) => h.url === u)?.name).filter(Boolean).join(" · ")}</span> : null}
        </div>
      </div>
    );
  };

  if (!q.trim()) {
    const recent = snap.chats.slice(0, 4);
    return (
      <>
        {recent.length ? <div className="nc-sec">Recent</div> : null}
        {recent.map((c) => { const ct = snap.byAddr.get(c.peer); return ct ? row(ct) : null; })}
        <div className="nc-sec">Directory</div>
        <div className={platform === "android" ? "" : "dir-chips"}>
          {platform === "android"
            ? <>
              <div className="li" onClick={onDirectory} role="button" tabIndex={0} onKeyDown={pressKeys(onDirectory)}><Icon name="contacts" /><div className="t"><div className="t1">Browse the directory</div><div className="t2">{everyone.length} on your hubs · {onlineN} online</div></div><Icon name="chevron_right" className="chev" /></div>
              <div className="li" onClick={() => void scanSetup()} role="button" tabIndex={0} onKeyDown={pressKeys(() => void scanSetup())}><Icon name="qr" /><div className="t"><div className="t1">Scan setup code</div><div className="t2">From Orgtree</div></div><Icon name="chevron_right" className="chev" /></div>
            </>
            : <button className="btn" onClick={onDirectory}><Icon name="contacts" />Browse everyone on your hubs <span className="dim" style={{ fontWeight: 400 }}>{everyone.length} people and agents · {onlineN} online</span></button>}
        </div>
        <div className="nc-msg"><Icon name="at" /><div><b>Or type an address</b>Someone may give you theirs, or click an <span className="mono">@net:</span> link in any message.</div></div>
      </>
    );
  }
  if (!r) return <div className="nc-msg"><span className="spin" /><div>Looking it up…</div></div>;
  if (!r.valid) return <div className="nc-msg"><Icon name="error_outline" /><div><b>That is not an address</b>Addresses use a–z, 0–9, dots, “_” and “-”, like <span className="mono">maya.e71f2b</span>.</div></div>;
  if (r.is_me) return <div className="nc-msg"><Icon name="person" /><div><b>That's you</b>Share this address so others can reach you.</div></div>;
  if (r.exact) {
    const c = r.exact; const p = presence(c, hubs); const k = kindInfo(kindOf(c)); const h = viaHub(c, hubs);
    return (
      <>
        <div className="nc-card">
          <PeerAvatar address={c.address} c={c} hubs={hubs} size={platform === "android" ? 52 : 48} />
          <div className="t">
            <div className="t1">{displayName(c, c.address)} <span className={"chip " + k.cls}><Icon name={k.icon} />{platform === "android" ? k.short : k.label}</span></div>
            <div className="t2"><Addr a={c.address} net /></div>
            <div className="t3"><span className={"ptext p-" + p.state}>{p.text}</span>{h ? <span>· {c.hubs.length > 1 ? "on hubs " + c.hubs.map((u) => hubs.find((x) => x.url === u)?.name || u).join(", ") : "on hub " + h.name}</span> : null}</div>
          </div>
          {platform === "desktop" ? <button className="btn primary" onClick={() => onOpen(c.address)}>{started(c.address) ? "Open chat" : "Start chat"}</button> : null}
        </div>
        {platform === "android" ? <div style={{ padding: "4px 16px 8px" }}><button className="btn primary block" onClick={() => onOpen(c.address)}>{started(c.address) ? "Open chat" : "Start chat"}</button></div> : null}
      </>
    );
  }
  const matches = r.matches.filter((c) => !isLinkWaiting(c));
  if (matches.length) {
    return (
      <>
        <div className="nc-sec">On your hubs</div>
        {matches.map(row)}
        {matches.length > 1 ? <div className={"help" + (platform === "android" ? " pad" : "")} style={{ margin: platform === "android" ? undefined : "8px 4px" }}>Several share the id <b>{r.address}</b>: the tag after the dot tells them apart.</div> : null}
      </>
    );
  }
  // nothing by address: offer directory entries whose name or address starts with it
  const qq = r.address;
  const near = qq.length >= 2 ? everyone.filter((c) => c.address.startsWith(qq) || displayName(c, c.address).toLowerCase().split(/\s+/).some((w) => w.startsWith(qq))).slice(0, 6) : [];
  if (near.length) {
    return (
      <>
        <div className="nc-sec">On your hubs</div>
        {near.map(row)}
        <div className={"help" + (platform === "android" ? " pad" : "")} style={{ margin: platform === "android" ? "10px 0" : "10px 4px" }}>
          Not who you mean? <button className="link" onClick={() => onOpen(r.address)}>Start a chat with {r.address} anyway</button>
        </div>
      </>
    );
  }
  return (
    <div className={"nc-msg" + (down.length ? " warn" : "")}>
      <Icon name={down.length ? "warning" : "search"} />
      <div>
        <b>No one with that address on {up.length ? names(up) : "your hubs"}</b>
        {up.length ? <>Searched the directories of {names(up)}. </> : <>None of your hubs is connected, so nothing could be checked. </>}
        {down.length ? <>{names(down)} can't be reached right now. </> : null}
        You can start the chat anyway: if no hub knows <span className="mono">{r.address}</span>, your message fails visibly and you can retry or delete it.
        <div><button className="btn" onClick={() => onOpen(r.address)}>Start chat anyway</button></div>
      </div>
    </div>
  );
}
