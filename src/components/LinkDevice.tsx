// Link a device, from a device that already has the identity (Settings ›
// Devices). Three tabs, as in the prototype's link modal:
//  - Approve a new device: type (or, on Android, scan) its code, find it in
//    the directory, check its name, Approve or Deny.
//  - Show my key as a QR code: behind a warning; hidden again with Done.
//  - Key file: a passphrase twice, then Save….
import { useEffect, useRef, useState, type ReactNode } from "react";
import { api, type LinkLookup } from "../api";
import { Icon } from "../lib/icons";
import { errText, saveKeyFileTo, scanQr } from "../lib/native";
import { refreshDirectory, refreshState, useSnap } from "../lib/store";
import { toast } from "../lib/toast";
import { Modal, ModalHead, NoteCard, QR, usePlatform } from "./ui";

export type LinkTab = "approve" | "qr" | "file";
const TAB_LABELS: [LinkTab, string][] = [["approve", "Approve a new device"], ["qr", "Show my key as a QR code"], ["file", "Key file"]];

/** The row of actions under a tab: right-aligned on desktop, full-width on Android. */
function Acts({ children }: { children: ReactNode }) {
  return usePlatform() === "android" ? <div className="pad" style={{ display: "flex", gap: 8, marginTop: 6 }}>{children}</div> : <div className="lc-acts">{children}</div>;
}
function Lead({ children }: { children: ReactNode }) {
  return usePlatform() === "android" ? <div className="pad help" style={{ fontSize: 14, marginBottom: 6 }}>{children}</div> : <div className="lc-lead">{children}</div>;
}

/** Group a typed code as XXXX-XXXX-XXXX-XXXX while typing (a pasted QR text stays as it is). */
function formatCode(v: string): string {
  if (/^hubchat-link:/i.test(v.trim())) return v.trim();
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

function ApproveTab({ onClose }: { onClose: () => void }) {
  const platform = usePlatform();
  const snap = useSnap();
  const [code, setCode] = useState("");
  /** A scanned QR's full text (it also names the hub); the box shows just its code. */
  const [qrText, setQrText] = useState<string | null>(null);
  const [st, setSt] = useState<Ap>({ k: "idle" });
  const run = useRef(0);
  const [scanning, setScanning] = useState(false);

  useEffect(() => () => { run.current++; }, []);

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
      if (!/^hubchat-link:/i.test(t.trim())) { setSt({ k: "error", msg: "That QR code isn't a Hubchat link code. On the new device choose “I already use Hubchat › Link through a hub” and scan the code it shows." }); return; }
      const m = /^hubchat-link:([^@]*)/i.exec(t.trim());
      setCode(formatCode(m ? m[1] : t)); setQrText(t.trim());
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

  const hubName = (url: string) => snap.state?.hubs.find((h) => h.url === url)?.name || url.replace(/^https?:\/\//, "");
  const devIcon = (name: string) => (/phone|android|pixel|galaxy/i.test(name) ? "phone" : "computer");

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

/** The three tabs (Android: a screen body; desktop: inside the modal). */
export function LinkDevice({ onClose, onRecovery, initial = "approve" }: { onClose: () => void; onRecovery?: () => void; initial?: LinkTab }) {
  const [tab, setTab] = useState<LinkTab>(initial);
  const platform = usePlatform();
  return (
    <div className={platform === "android" ? "lc-pane" : ""}>
      <div className="tabs" role="tablist">
        {TAB_LABELS.map(([k, l]) => <button key={k} role="tab" aria-selected={tab === k} className={tab === k ? "on" : ""} onClick={() => setTab(k)}>{platform === "android" ? l.replace("Show my key as a QR code", "My key as QR").replace("Approve a new device", "Approve") : l}</button>)}
      </div>
      {tab === "approve" ? <ApproveTab key="a" onClose={onClose} /> : tab === "qr" ? <KeyQrTab key="q" onClose={onClose} /> : <KeyFileTab key="f" onRecovery={onRecovery} />}
    </div>
  );
}

/** Desktop: Link a device as a modal over Settings. */
export function LinkDeviceModal({ onClose, onRecovery }: { onClose: () => void; onRecovery?: () => void }) {
  useEffect(() => {
    const k = (e: KeyboardEvent) => { if (e.key === "Escape") { e.preventDefault(); e.stopPropagation(); onClose(); } };
    window.addEventListener("keydown", k, true);
    return () => window.removeEventListener("keydown", k, true);
  }, [onClose]);
  return (
    <Modal onClose={onClose} className="link3">
      <ModalHead title="Link a device" onClose={onClose} />
      <div className="modal-b"><LinkDevice onClose={onClose} onRecovery={onRecovery} /></div>
    </Modal>
  );
}
