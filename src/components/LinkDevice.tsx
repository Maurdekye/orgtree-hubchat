// Link a device, from a device that already has the identity (Settings ›
// Devices, or the QR button in the desktop sidebar footer). Four tabs:
//  - Show a QR code (the main path, user 19:12-19:13Z): this device makes a
//    one-time code and shows it as a QR; the new device scans it (or types the
//    code) and waits on the hub; its name appears here, Approve or Deny.
//  - Approve a code: the other direction. Type (or, on Android, scan) the code
//    a new device shows, find it in the directory, check its name, Approve.
//  - Show my key as a QR code: behind a warning; hidden again with Done.
//  - Key file: a passphrase twice, then Save….
import { useEffect, useRef, useState, type ReactNode } from "react";
import { api, type LinkLookup, type LinkStart } from "../api";
import { Icon } from "../lib/icons";
import { errText, saveKeyFileTo, scanQr } from "../lib/native";
import { refreshDirectory, refreshState, useSnap } from "../lib/store";
import { toast } from "../lib/toast";
import { Modal, ModalHead, NoteCard, QR, useNow, usePlatform } from "./ui";

export type LinkTab = "offer" | "approve" | "qr" | "file";
const TAB_LABELS: [LinkTab, string, string][] = [
  // [tab, desktop label, Android label]
  ["offer", "Show a QR code", "QR code"],
  ["approve", "Approve a code", "Approve"],
  ["qr", "My key as a QR code", "My key"],
  ["file", "Key file", "Key file"],
];

/** The row of actions under a tab: right-aligned on desktop, full-width on Android. */
function Acts({ children }: { children: ReactNode }) {
  return usePlatform() === "android" ? <div className="pad" style={{ display: "flex", gap: 8, marginTop: 6 }}>{children}</div> : <div className="lc-acts">{children}</div>;
}
function Lead({ children }: { children: ReactNode }) {
  return usePlatform() === "android" ? <div className="pad help" style={{ fontSize: 14, marginBottom: 6 }}>{children}</div> : <div className="lc-lead">{children}</div>;
}

/** Group a typed code as XXXX-XXXX-XXXX-XXXX while typing (a pasted QR text stays as it is). */
export function formatCode(v: string): string {
  if (/^(hubchat-link:|hubchat:\/\/)/i.test(v.trim())) return v.trim();
  const raw = v.toUpperCase().replace(/[^A-Z0-9]/g, "").slice(0, 16);
  return raw.replace(/(.{4})(?=.)/g, "$1-");
}

type Ap =
  | { k: "idle" }
  | { k: "looking"; input: string; tries: number }
  | { k: "unknown_hub"; hub: string; input: string }
  | { k: "notfound" }
  | { k: "confirm"; r: LinkLookup }
  | { k: "sending"; r: LinkLookup }
  | { k: "done"; name: string; hub: string }
  | { k: "error"; msg: string };

const LOOKUP_EVERY = 3000;
const LOOKUP_FOR = 90000;
/** A code this device offers works for 10 minutes; look for its device that long. */
const OFFER_FOR = 600e3;

const hubLabel = (hubs: { url: string; name: string }[] | undefined, url: string) => hubs?.find((h) => h.url === url)?.name || url.replace(/^https?:\/\//, "");
const devIcon = (name: string) => (/phone|android|pixel|galaxy/i.test(name) ? "phone" : "computer");

type Of =
  | { k: "starting" }
  | { k: "waiting"; o: LinkStart; until: number }
  | { k: "confirm"; o: LinkStart; until: number; r: LinkLookup }
  | { k: "sending"; o: LinkStart; until: number; r: LinkLookup }
  | { k: "done"; name: string; hub: string }
  | { k: "denied" }
  | { k: "expired" }
  | { k: "error"; msg: string };

/** This device shows a one-time code as a QR; the new device scans it and
 *  waits on the hub; look the code up every 3 s until its name shows. */
function OfferTab({ onClose }: { onClose: () => void }) {
  const platform = usePlatform();
  const snap = useSnap();
  const [st, setSt] = useState<Of>({ k: "starting" });
  const [attempt, setAttempt] = useState(0);
  const run = useRef(0);
  const now = useNow(st.k === "waiting");

  useEffect(() => {
    const my = ++run.current;
    setSt({ k: "starting" });
    void (async () => {
      let o: LinkStart;
      try { o = await api.linkOffer(); }
      catch (e) { if (run.current === my) setSt({ k: "error", msg: errText(e) }); return; }
      if (run.current !== my) return;
      const until = Date.now() + OFFER_FOR;
      setSt({ k: "waiting", o, until });
      while (run.current === my) {
        await new Promise((res) => setTimeout(res, LOOKUP_EVERY));
        if (run.current !== my) return;
        if (Date.now() > until) { setSt({ k: "expired" }); return; }
        void refreshDirectory();
        let r: LinkLookup;
        // a hub may be down for a moment: keep looking until the code runs out
        try { r = await api.linkLookup(o.code); } catch { continue; }
        if (run.current !== my) return;
        if (r.device_name != null) { setSt({ k: "confirm", o, until, r }); return; }
      }
    })();
    return () => { run.current++; };
  }, [attempt]);

  const again = () => setAttempt((a) => a + 1);
  const approve = async (o: LinkStart, until: number, r: LinkLookup) => {
    setSt({ k: "sending", o, until, r });
    try { const hub = await api.linkApprove(r.code); setSt({ k: "done", name: r.device_name || "The new device", hub }); }
    catch (e) { setSt({ k: "error", msg: errText(e) }); }
  };
  const deny = () => { run.current++; setSt({ k: "denied" }); };
  const blk = platform === "android" ? " block" : "";
  const hubs = snap.state?.hubs;
  const self = platform === "android" ? "phone" : "PC";

  if (st.k === "done") {
    return (
      <>
        <div className="probe-card ok"><Icon name="check_circle" /><div><b>“{st.name}” now uses your identity</b>It has your key, your hubs and your profile, sent sealed through {hubLabel(hubs, st.hub)}. It fetches your chats from your hubs.</div></div>
        <Acts><button className={"btn primary" + blk} onClick={onClose}>Done</button></Acts>
      </>
    );
  }
  if (st.k === "confirm" || st.k === "sending") {
    const r = st.r; const name = r.device_name || "New device";
    return (
      <>
        <Lead>A device scanned this {self}'s code and is waiting. Check the name: it should be the device you are holding.</Lead>
        <div className="approve"><Icon name={devIcon(name)} /><div className="t"><b>Link “{name}”?</b><span>Waiting on {hubLabel(hubs, st.o.hub)} · code {st.o.code}</span></div></div>
        <NoteCard icon="warning" warn><b>Approve only a device you are holding.</b> It gets your key and becomes you, like this {self}.</NoteCard>
        {st.k === "sending" ? <div className="probe-card busy"><span className="spin" /><div>Sending your key, hubs and profile to “{name}”…</div></div> : null}
        <Acts>
          <button className={"btn ghost" + blk} onClick={deny} disabled={st.k === "sending"}>Deny</button>
          <button className={"btn primary" + blk} onClick={() => approve(st.o, st.until, r)} disabled={st.k === "sending"}><Icon name="check" />Approve</button>
        </Acts>
      </>
    );
  }
  if (st.k !== "waiting") {
    const card = st.k === "starting" ? <div className="probe-card busy"><span className="spin" /><div>Making a one-time code…</div></div>
      : st.k === "denied" ? <div className="probe-card bad"><Icon name="close" /><div><b>Not linked</b>Nothing was sent to that device. Show a new code to link another one.</div></div>
      : st.k === "expired" ? <div className="probe-card bad"><Icon name="hourglass" /><div><b>The code expired</b>No device asked to be linked within 10 minutes. Show a new code and scan it again.</div></div>
      : <div className="probe-card bad"><Icon name="error" /><div><b>That didn't work</b>{st.msg}</div></div>;
    return (
      <>
        {card}
        {st.k !== "starting" ? (
          <Acts>
            {platform === "desktop" ? <button className="btn ghost" onClick={onClose}>Close</button> : null}
            <button className={"btn primary" + blk} onClick={again}><Icon name="refresh" />New code</button>
          </Acts>
        ) : null}
      </>
    );
  }
  const left = Math.max(0, Math.ceil((st.until - now) / 1000));
  const mmss = Math.floor(left / 60) + ":" + String(left % 60).padStart(2, "0");
  return (
    <>
      <div className="offer">
        <QR text={st.o.qr} size={platform === "android" ? 230 : 210} />
        <div className="paircode long">{st.o.code}</div>
        <div className="offer-hub">Through <b>{hubLabel(hubs, st.o.hub)}</b> <span className="mono">{st.o.hub}</span></div>
      </div>
      <div className="offer-how">
        {platform === "android"
          ? <>On the new device: open Hubchat › <b>I already use Hubchat</b>, then scan this QR code or type the code.</>
          : <>On your phone: open Hubchat › <b>I already use Hubchat</b> › <b>Scan the QR code</b>. Or point the phone's camera at it.</>}
      </div>
      <div className="waiting"><span className="spin" /><span>Waiting for your new device…</span><span className="cd" title="The code works for 10 minutes">{mmss}</span></div>
      <Acts>
        {platform === "desktop" ? <button className="btn ghost" onClick={onClose}>Close</button> : null}
        <button className={"btn" + blk} onClick={again}><Icon name="refresh" />New code</button>
      </Acts>
    </>
  );
}

function ApproveTab({ onClose, initialInput }: { onClose: () => void; initialInput?: string }) {
  const platform = usePlatform();
  const snap = useSnap();
  const [code, setCode] = useState("");
  /** A scanned QR's full text (it also names the hub); the box shows just its code. */
  const [qrText, setQrText] = useState<string | null>(null);
  const [st, setSt] = useState<Ap>({ k: "idle" });
  const run = useRef(0);
  const [scanning, setScanning] = useState(false);

  useEffect(() => () => { run.current++; }, []);
  // opened with a link (an Android deep link): fill in its code and look it up
  useEffect(() => {
    if (!initialInput) return;
    let gone = false;
    const t = initialInput.trim();
    api.parseLink(t).then(
      (p) => { if (!gone) { setCode(p.code); setQrText(t); void lookup(t); } },
      (e) => { if (!gone) setSt({ k: "error", msg: errText(e) }); },
    );
    return () => { gone = true; };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [initialInput]);

  /** Look the code up every 3 s for up to 90 s, until the device shows in the directory. */
  const lookup = async (input: string) => {
    const my = ++run.current;
    const t0 = Date.now();
    for (let tries = 1; ; tries++) {
      setSt({ k: "looking", input, tries });
      let r: LinkLookup;
      try { r = await api.linkLookup(input); }
      catch (e) { if (run.current === my) setSt({ k: "error", msg: errText(e) }); return; }
      if (run.current !== my) return;
      if (r.unknown_hub) { setSt({ k: "unknown_hub", hub: r.unknown_hub, input }); return; }
      if (r.device_name != null) { setSt({ k: "confirm", r }); return; }
      if (Date.now() - t0 + LOOKUP_EVERY > LOOKUP_FOR) { setSt({ k: "notfound" }); return; }
      await new Promise((res) => setTimeout(res, LOOKUP_EVERY));
      if (run.current !== my) return;
      void refreshDirectory();
    }
  };
  const cont = () => { const v = qrText || code.trim(); if (v) void lookup(v); };
  const reset = () => { run.current++; setSt({ k: "idle" }); };
  const scan = async () => {
    setScanning(true);
    try {
      const t = await scanQr("hubchat-link:K7QD-4MXP-9TRA-2HZE@http://hub.office.lan:7370");
      if (!t) return;
      if (!/^(hubchat-link:|hubchat:\/\/)/i.test(t.trim())) { setSt({ k: "error", msg: "That QR code isn't a Hubchat link code. On the new device choose “I already use Hubchat”, then “Show a code on this phone instead” (a PC: “Link through a hub”), and scan the code it shows." }); return; }
      const p = await api.parseLink(t);
      setCode(p.code); setQrText(t.trim());
      void lookup(t.trim());
    } catch (e) { setSt({ k: "error", msg: errText(e) }); }
    finally { setScanning(false); }
  };
  const addHub = async (hub: string, input: string) => {
    try { const url = await api.addHub(hub); await refreshState(); toast("Added hub " + url.replace(/^https?:\/\//, "")); void lookup(input); }
    catch (e) { setSt({ k: "error", msg: errText(e) }); }
  };
  const approve = async (r: LinkLookup) => {
    setSt({ k: "sending", r });
    try { const hub = await api.linkApprove(r.code); setSt({ k: "done", name: r.device_name || "the new device", hub }); }
    catch (e) { setSt({ k: "error", msg: errText(e) }); }
  };
  const deny = () => { run.current++; setCode(""); setQrText(null); setSt({ k: "idle" }); toast("Denied. Nothing was sent."); };

  const hubName = (url: string) => hubLabel(snap.state?.hubs, url);

  if (st.k === "done") {
    return (
      <>
        <div className="probe-card ok"><Icon name="check_circle" /><div><b>Linked “{st.name}”</b>It now has your key, your hubs and your profile, sent sealed through {hubName(st.hub)}. It fetches your chats from your hubs.</div></div>
        <Acts><button className={"btn primary" + (platform === "android" ? " block" : "")} onClick={onClose}>Done</button></Acts>
      </>
    );
  }
  if (st.k === "confirm" || st.k === "sending") {
    const r = st.r; const name = r.device_name || "New device";
    return (
      <>
        <Lead>Check the name below: it should be the device you are holding.</Lead>
        <div className="approve"><Icon name={devIcon(name)} /><div className="t"><b>Link “{name}”?</b><span>{r.hubs.length ? "Waiting on " + r.hubs.map(hubName).join(", ") + " · " : ""}code {r.code}</span></div></div>
        <NoteCard icon="warning" warn><b>Approve only a device you are holding.</b> It gets your key and becomes you, like this {platform === "android" ? "phone" : "PC"}.</NoteCard>
        {st.k === "sending" ? <div className="probe-card busy"><span className="spin" /><div>Sending your key, hubs and profile to “{name}”…</div></div> : null}
        <Acts>
          <button className={"btn ghost" + (platform === "android" ? " block" : "")} onClick={deny} disabled={st.k === "sending"}>Deny</button>
          <button className={"btn primary" + (platform === "android" ? " block" : "")} onClick={() => approve(r)} disabled={st.k === "sending"}><Icon name="check" />Approve</button>
        </Acts>
      </>
    );
  }

  const busy = st.k === "looking";
  return (
    <>
      <Lead>Through a hub: your key, hub list and profile go to the new device sealed with its one-time code. The hub carries them but can't read them.</Lead>
      {platform === "desktop"
        ? <ol className="lc-steps"><li>On the new device choose <b>I already use Hubchat › Link through a hub</b>.</li><li>It shows a code and a QR code. Type the code here.</li><li>Check the device it names, then approve.</li></ol>
        : null}
      {platform === "android" ? <div className="pad" style={{ marginBottom: 6 }}><button className="btn primary block" onClick={scan} disabled={scanning || busy}><Icon name="camera" />{scanning ? "Opening the camera…" : "Scan code"}</button></div> : null}
      <div className="field">
        <label htmlFor="lc-in">{platform === "android" ? "Or type the code from the new device" : "Code from the new device"}</label>
        <label className="input lc-code" style={{ height: 50 }}>
          <input id="lc-in" value={code} placeholder="XXXX-XXXX-XXXX-XXXX" autoComplete="off" autoCapitalize="characters" spellCheck={false} disabled={busy} autoFocus={platform === "desktop"}
            onChange={(e) => { setCode(formatCode(e.target.value)); setQrText(null); if (st.k !== "idle") setSt({ k: "idle" }); }}
            onKeyDown={(e) => { if (e.key === "Enter") cont(); }} />
        </label>
      </div>
      {st.k === "looking" ? <div className="probe-card busy"><span className="spin" /><div>Looking for the new device…{st.tries > 1 ? <span className="dim"> Its hub lists it within about a minute.</span> : null}</div></div> : null}
      {st.k === "unknown_hub" ? (
        <div className="probe-card bad"><Icon name="dns" />
          <div><b>The new device is waiting on a hub you don't use</b>It registered on <span className="mono">{st.hub}</span>, which isn't one of this device's hubs, so this device can't reach it.
            <div className="acts"><button className="btn" onClick={() => addHub(st.hub, st.input)}><Icon name="add" />Add that hub</button></div>
          </div>
        </div>
      ) : null}
      {st.k === "notfound" ? <div className="probe-card bad"><Icon name="error" /><div><b>Couldn't find the new device</b>Check the code, that the new device still shows it, and that both devices use the same hub. Then try again.</div></div> : null}
      {st.k === "error" ? <div className="probe-card bad"><Icon name="error" /><div><b>That didn't work</b>{st.msg}</div></div> : null}
      <Acts>
        {busy ? <button className={"btn ghost" + (platform === "android" ? " block" : "")} onClick={reset}>Stop looking</button>
          : platform === "desktop" ? <button className="btn ghost" onClick={onClose}>Cancel</button> : null}
        <button className={"btn" + (platform === "android" ? " block" : " primary")} onClick={cont} disabled={!code.trim() || busy}>{st.k === "notfound" || st.k === "error" ? "Try again" : "Continue"}</button>
      </Acts>
    </>
  );
}

function KeyQrTab({ onClose }: { onClose: () => void }) {
  const platform = usePlatform();
  const snap = useSnap();
  const [qr, setQr] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const show = async () => {
    setBusy(true);
    try { setQr(await api.keyQr()); } catch (e) { toast(errText(e)); } finally { setBusy(false); }
  };
  const n = snap.state?.hubs.length || 0;
  if (!qr) {
    return (
      <>
        <Lead>No network needed: a new phone reads your key straight off this screen.</Lead>
        <NoteCard icon="warning" warn><b>This QR code is your key.</b> Anyone who sees it can become you: read your messages and send as you. Show it only to your own phone, out of sight of other people and cameras.</NoteCard>
        <Acts>
          {platform === "desktop" ? <button className="btn ghost" onClick={onClose}>Cancel</button> : null}
          <button className={"btn primary" + (platform === "android" ? " block" : "")} onClick={show} disabled={busy}><Icon name="qr" />Show the QR code</button>
        </Acts>
      </>
    );
  }
  const steps = <ol><li>On the new phone choose <b>I already use Hubchat › Scan a QR code</b>.</li><li>Point it at this code.</li><li>It brings your key, your id, your profile and your {n} hub{n === 1 ? "" : "s"}.</li></ol>;
  return (
    <>
      {platform === "android"
        ? <div className="qrwrap big"><QR text={qr} size={260} /><span className="help">On the new phone: <span style={{ fontWeight: 600, color: "var(--ink)" }}>I already use Hubchat › Scan a QR code</span>. Nothing goes over the network.</span></div>
        : <div className="pair"><QR text={qr} size={210} />{steps}</div>}
      <div className={"help" + (platform === "android" ? " pad" : "")} style={{ marginTop: 14 }}>This {platform === "android" ? "phone" : "PC"} can't tell when the other device has read it: hide the code once it says Welcome back.</div>
      <Acts><button className={"btn primary" + (platform === "android" ? " block" : "")} onClick={() => setQr(null)}>Done</button></Acts>
    </>
  );
}

export function KeyFileTab({ onRecovery }: { onRecovery?: () => void }) {
  const platform = usePlatform();
  const [p1, setP1] = useState("");
  const [p2, setP2] = useState("");
  const [busy, setBusy] = useState(false);
  const [saved, setSaved] = useState<string | null>(null);
  const ok = p1.length >= 8 && p1 === p2;
  const save = async () => {
    setBusy(true);
    try {
      const dest = await saveKeyFileTo();
      if (!dest) return;
      await api.keyFileExport(p1, dest);
      setSaved(dest); setP1(""); setP2("");
      toast("Key file saved");
    } catch (e) { toast(errText(e)); }
    finally { setBusy(false); }
  };
  const mismatch = !!p2 && p1 !== p2;
  const short = !!p1 && p1.length < 8;
  return (
    <>
      <Lead>For a device that can't scan, or when no other device is at hand. The key file holds your key, your id, your profile and your hub list, locked with a passphrase. Keep it somewhere other than this {platform === "android" ? "phone" : "PC"}.</Lead>
      <div className="kfpanel" style={platform === "desktop" ? { margin: "0 0 12px" } : undefined}>
        <label className="input"><Icon name="lock" /><input type="password" value={p1} placeholder="Passphrase (8 or more characters)" autoComplete="new-password" onChange={(e) => { setP1(e.target.value); setSaved(null); }} /></label>
        <label className="input"><Icon name="lock" /><input type="password" value={p2} placeholder="Repeat it" autoComplete="new-password" onChange={(e) => setP2(e.target.value)} onKeyDown={(e) => { if (e.key === "Enter" && ok && !busy) void save(); }} /></label>
        {short ? <div className="help bad">At least 8 characters.</div> : mismatch ? <div className="help bad">The two passphrases don't match.</div> : <div className="help">You need this passphrase to open the file. Hubchat can't recover it.</div>}
      </div>
      {saved ? <div className="probe-card ok"><Icon name="check_circle" /><div><b>Saved</b><span className="mono" style={{ overflowWrap: "anywhere" }}>{saved}</span></div></div> : null}
      <NoteCard icon="key"><b>Recovery words do the same without a file.</b> They hold your key and your id (not your profile or hubs).{onRecovery ? <> <button className="link" onClick={onRecovery}>Show recovery words</button></> : " See Settings › Recovery words."}</NoteCard>
      <Acts><button className={"btn primary" + (platform === "android" ? " block" : "")} disabled={!ok || busy} onClick={save}><Icon name="save" />{busy ? "Saving…" : "Save…"}</button></Acts>
    </>
  );
}

/** The four tabs (Android: a screen body; desktop: inside the modal).
 *  `input`: a link to fill in and look up straight away on the Approve tab. */
export function LinkDevice({ onClose, onRecovery, initial = "offer", input }: { onClose: () => void; onRecovery?: () => void; initial?: LinkTab; input?: string }) {
  const [tab, setTab] = useState<LinkTab>(initial);
  const platform = usePlatform();
  return (
    <div className={platform === "android" ? "lc-pane" : ""}>
      <div className="tabs" role="tablist">
        {TAB_LABELS.map(([k, l, al]) => <button key={k} role="tab" aria-selected={tab === k} className={tab === k ? "on" : ""} onClick={() => setTab(k)}>{platform === "android" ? al : l}</button>)}
      </div>
      {tab === "offer" ? <OfferTab key="o" onClose={onClose} />
        : tab === "approve" ? <ApproveTab key="a" onClose={onClose} initialInput={input} />
        : tab === "qr" ? <KeyQrTab key="q" onClose={onClose} /> : <KeyFileTab key="f" onRecovery={onRecovery} />}
    </div>
  );
}

/** Desktop: Link a device as a modal (over Settings, or from the sidebar's QR button). */
export function LinkDeviceModal({ onClose, onRecovery, initial, input }: { onClose: () => void; onRecovery?: () => void; initial?: LinkTab; input?: string }) {
  useEffect(() => {
    const k = (e: KeyboardEvent) => { if (e.key === "Escape") { e.preventDefault(); e.stopPropagation(); onClose(); } };
    window.addEventListener("keydown", k, true);
    return () => window.removeEventListener("keydown", k, true);
  }, [onClose]);
  return (
    <Modal onClose={onClose} className="link3">
      <ModalHead title="Link a device" onClose={onClose} />
      <div className="modal-b"><LinkDevice onClose={onClose} onRecovery={onRecovery} initial={initial} input={input} /></div>
    </Modal>
  );
}
