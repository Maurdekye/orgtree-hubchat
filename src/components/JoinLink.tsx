// A signed-in device joins another device's link (user 19:21-19:22Z): reach
// the link's hub (or ask for an address that works), join, wait for the other
// device to approve. What arrives is either this device's own identity
// ("same": nothing changes) or another one ("switch": leave this identity and
// adopt that one, or keep it). Desktop: a modal; Android: a full screen.
import { useEffect, useRef, useState, type ReactNode } from "react";
import { api, type LinkStart } from "../api";
import { Icon } from "../lib/icons";
import { endJoin, type JoinReq } from "../lib/join";
import { errText } from "../lib/native";
import { copyWords, downloadWords } from "../lib/recovery";
import { reloadAfterSwitch, useSnap } from "../lib/store";
import { toast } from "../lib/toast";
import { HubAdder } from "./HubAdder";
import { Acts, Lead } from "./LinkDevice";
import { Addr, Modal, ModalHead, NoteCard, useNow, usePlatform } from "./ui";

type Hub = { url: string; name: string };
type St =
  | { k: "probe"; hub: string }
  | { k: "hub"; hub: string | null; why: string | null }
  | { k: "starting"; hub: Hub }
  | { k: "waiting"; hub: Hub; s: LinkStart; until: number }
  | { k: "failed"; hub: Hub | null; msg: string }
  | { k: "expired"; hub: Hub }
  | { k: "same"; address: string }
  | { k: "switch"; from: string; to: string; busy?: boolean };

/** Desktop: a modal titled `heading` (else `title`); Android: a screen with
 *  `title` in its app bar and `heading` on top of the body. */
function Shell({ title, heading, onClose, children }: { title: ReactNode; heading?: ReactNode; onClose: () => void; children: ReactNode }) {
  const platform = usePlatform();
  if (platform === "android") {
    return (
      <div className="scr">
        <div className="appbar flat"><button className="icon-btn" onClick={onClose} aria-label="Back"><Icon name="back" /></button><div className="title">{title}</div></div>
        <div className="scr-body lc-pane join">{heading ? <h2 className="join-h">{heading}</h2> : null}{children}</div>
      </div>
    );
  }
  return (
    <Modal onClose={() => {}} className="link3 join">
      <ModalHead title={heading ?? title} onClose={onClose} />
      <div className="modal-b">{children}</div>
    </Modal>
  );
}

export function JoinFlow({ req }: { req: JoinReq }) {
  const platform = usePlatform();
  const snap = useSnap();
  const self = platform === "android" ? "phone" : "PC";
  const [st, setSt] = useState<St>(() => (req.hub ? { k: "probe", hub: req.hub } : { k: "hub", hub: null, why: null }));
  const [attempt, setAttempt] = useState(0);
  const [ack, setAck] = useState(false);
  const now = useNow(st.k === "waiting");
  const stRef = useRef(st);
  stRef.current = st;
  const done = useRef(false);
  /** The hub this device joins through, once reached. */
  const hubRef = useRef<Hub | null>(null);
  const blk = platform === "android" ? " block" : "";

  // leaving this flow undoes what is in flight (not on StrictMode's re-mount)
  const alive = useRef(false);
  useEffect(() => {
    alive.current = true;
    return () => {
      alive.current = false;
      setTimeout(() => {
        if (alive.current || done.current) return;
        const k = stRef.current.k;
        if (k === "switch") void api.linkSwitchCancel().catch(() => {});
        else if (k === "starting" || k === "waiting") void api.linkCancel().catch(() => {});
      }, 0);
    };
  }, []);

  useEffect(() => {
    let un: (() => void) | null = null; let gone = false;
    void api.onLink((e) => {
      if (e.state === "waiting") setSt((s) => (s.k === "waiting" ? { ...s, until: Date.now() + e.expires_in_s * 1000 } : s));
      else if (e.state === "same") setSt({ k: "same", address: e.address });
      else if (e.state === "switch") { setAck(false); setSt({ k: "switch", from: e.from, to: e.to }); }
      else if (e.state === "done") { done.current = true; toast("This device is now @net:" + e.address); endJoin(); void reloadAfterSwitch(); }
      else if (e.state === "failed") setSt({ k: "failed", hub: hubRef.current, msg: e.error });
      else setSt((s) => (s.k === "waiting" ? { k: "expired", hub: s.hub } : s));
    }).then((u) => { if (gone) u(); else un = u; });
    return () => { gone = true; un?.(); };
  }, []);

  // reach the link's hub; if this device can't, ask for an address that works
  useEffect(() => {
    if (st.k !== "probe") return;
    let live = true;
    api.probeHub(st.hub).then(
      (p) => { if (!live) return; if (p.result === "connected") setSt({ k: "starting", hub: { url: p.url, name: p.name } }); else setSt({ k: "hub", hub: st.hub, why: p.error }); },
      (e) => { if (live) setSt({ k: "hub", hub: st.hub, why: errText(e) }); },
    );
    return () => { live = false; };
  }, [st]);

  // join: this device's name is what the other device shows when it asks to approve
  const startedFor = useRef("");
  useEffect(() => {
    if (st.k !== "starting") return;
    const key = attempt + "|" + st.hub.url;
    if (startedFor.current === key) return;
    startedFor.current = key;
    const hub = st.hub;
    hubRef.current = hub;
    void (async () => {
      let name = platform === "android" ? "Android phone" : "Windows PC";
      try { const d = await api.devices(); name = d.devices.find((x) => x.device_id === d.this_device)?.name || name; } catch { /* the default name */ }
      try {
        const s = await api.linkStart(hub.url, name, req.code);
        setSt((c) => (c.k === "starting" ? { k: "waiting", hub, s, until: Date.now() + 600e3 } : c));
      } catch (e) { setSt({ k: "failed", hub, msg: errText(e) }); }
    })();
  }, [st, attempt, platform, req.code]);

  const close = () => endJoin();
  const again = (hub: Hub) => { setAttempt((a) => a + 1); setSt({ k: "starting", hub }); };
  const keep = async () => { done.current = true; try { await api.linkSwitchCancel(); } catch { /* nothing was adopted */ } endJoin(); };
  const doSwitch = async (to: string) => {
    if (st.k !== "switch") return;
    setSt({ ...st, busy: true });
    try { await api.linkSwitch(); }
    catch (e) { toast("Couldn't switch: " + errText(e)); setSt({ ...st, busy: false }); return; }
    done.current = true;
    endJoin();
    await reloadAfterSwitch();
    toast("This device is now @net:" + to);
  };

  // Escape: Keep (switch), Cancel (otherwise); Android's back does the same
  const escRef = useRef<() => void>(close);
  escRef.current = st.k === "switch" ? () => { if (!st.busy) void keep(); } : close;
  useEffect(() => {
    if (platform === "android") return;
    const k = (e: KeyboardEvent) => { if (e.key === "Escape") { e.preventDefault(); e.stopPropagation(); escRef.current(); } };
    window.addEventListener("keydown", k, true);
    return () => window.removeEventListener("keydown", k, true);
  }, [platform]);

  if (st.k === "same") {
    return (
      <Shell title="Already this identity" onClose={close}>
        <div className="probe-card ok"><Icon name="check_circle" /><div><b>This {self} is already <span className="mono"><Addr a={st.address} net /></span></b>The other device offered the identity this {self} already has. Nothing changed.</div></div>
        <Acts><button className={"btn primary" + blk} onClick={close}>OK</button></Acts>
      </Shell>
    );
  }

  if (st.k === "switch") {
    const from = <span className="mono"><Addr a={st.from} net /></span>;
    const to = <span className="mono"><Addr a={st.to} net /></span>;
    const id = st.from.split(".")[0];
    const keepBtn = <button className={"btn ghost" + blk} onClick={() => void keep()} disabled={st.busy}><span>Keep <Addr a={st.from} net /></span></button>;
    const switchBtn = <button className={"btn primary" + blk} onClick={() => void doSwitch(st.to)} disabled={!ack || st.busy}><Icon name="sync" /><span>Switch to <Addr a={st.to} net /></span></button>;
    return (
      <Shell title="Switch identity" heading={<>Switch this {self} to <Addr a={st.to} net />?</>} onClose={() => { if (!st.busy) void keep(); }}>
        <Lead>This device is {from} now. Switching signs it out of {from} and removes {from}'s chats and settings from this device; {from} keeps working on your other devices.</Lead>
        <div className="switch-warn">
          <NoteCard icon="warning" warn>
            <b>If this is the last device with {from}, you can only get it back with its recovery words.</b>
            <div className="keyacts">
              <button className="btn" onClick={() => { api.recoveryWords().then((w) => copyWords(w), (e) => toast(errText(e))); }}><Icon name="copy" />Copy words</button>
              <button className="btn" onClick={() => void downloadWords(platform, snap.state?.me?.id || id)}><Icon name="download" />Download</button>
            </div>
          </NoteCard>
        </div>
        <label className="checkrow" style={platform === "android" ? { padding: "6px 20px" } : undefined}>
          <input type="checkbox" checked={ack} onChange={(e) => setAck(e.target.checked)} disabled={st.busy} />
          <span>I have {from}'s recovery words, or I don't need {from} on this device any more</span>
        </label>
        {st.busy ? <div className="probe-card busy"><span className="spin" /><div>Signing this {self} out of {from} and switching to {to}…</div></div> : null}
        {platform === "android"
          ? <div className="pad join-stack">{switchBtn}{keepBtn}</div>
          : <Acts>{keepBtn}{switchBtn}</Acts>}
      </Shell>
    );
  }

  const title = "Use another identity";
  if (st.k === "hub") {
    const mine = snap.state?.hubs || [];
    return (
      <Shell title={title} onClose={close}>
        {st.hub
          ? <Lead>The code goes through <span className="mono">{st.hub}</span>, but this {self} can't reach that address{st.why ? <> ({st.why})</> : null}. Enter the address this {self} can use (for example its Tailscale name).</Lead>
          : <Lead>Which hub does the other device use? Code <span className="mono">{req.code}</span> waits there.</Lead>}
        {!st.hub && mine.length ? (
          <div className={"keyacts join-hubs" + (platform === "android" ? " pad" : "")}>
            <span className="help">One of this {self}'s hubs:</span>
            {mine.map((h) => <button key={h.url} className="btn" title={h.url} onClick={() => setSt({ k: "probe", hub: h.url })}><Icon name="dns" />{h.name}</button>)}
          </div>
        ) : null}
        <HubAdder key={st.hub ?? ""} existing={[]} initial={st.hub ?? ""} autoFocus={platform === "desktop"} onPick={(url, name) => setSt({ k: "starting", hub: { url, name } })} />
        {platform === "desktop" ? <Acts><button className="btn ghost" onClick={close}>Cancel</button></Acts> : null}
      </Shell>
    );
  }

  let body: ReactNode;
  let acts: ReactNode;
  if (st.k === "waiting") {
    const left = Math.max(0, Math.ceil((st.until - now) / 1000));
    const mmss = Math.floor(left / 60) + ":" + String(left % 60).padStart(2, "0");
    body = (
      <>
        <div className="approve"><Icon name={platform === "android" ? "phone" : "computer"} /><div className="t"><b>This {self} is asking to be linked</b><span>Code {st.s.code} · through {st.hub.name}</span></div></div>
        <div className="waiting"><span className="spin" /><span>Waiting for you to approve it on the other device…</span><span className="cd" title="The code works for 10 minutes">{mmss}</span></div>
      </>
    );
    acts = <button className={"btn ghost" + blk} onClick={close}>Cancel</button>;
  } else if (st.k === "probe" || st.k === "starting") {
    body = <div className="probe-card busy"><span className="spin" /><div>{st.k === "probe" ? <>Reaching the hub at <span className="mono">{st.hub}</span>…</> : <>Joining through {st.hub.name}…</>}</div></div>;
    acts = <button className={"btn ghost" + blk} onClick={close}>Cancel</button>;
  } else {
    const hub = st.hub;
    body = st.k === "expired"
      ? <div className="probe-card bad"><Icon name="hourglass" /><div><b>The code expired</b>It wasn't approved within 10 minutes. Show a new code on the other device and scan it again.</div></div>
      : <div className="probe-card bad"><Icon name="error" /><div><b>Linking didn't work</b>{st.msg}</div></div>;
    acts = <>
      <button className={"btn ghost" + blk} onClick={close}>Close</button>
      {st.k === "failed" && hub ? <button className={"btn primary" + blk} onClick={() => again(hub)}><Icon name="refresh" />Try again</button> : null}
    </>;
  }
  return (
    <Shell title={title} onClose={close}>
      <Lead>On the other device, approve this {self}. Its identity then comes here sealed with the code; this {self} asks before it replaces <span className="mono"><Addr a={snap.state?.me?.address || ""} net /></span>.</Lead>
      {body}
      <Acts>{acts}</Acts>
    </Shell>
  );
}
