// First run. The fork: create a new identity (id → address → hubs → recovery
// words) or bring an existing one. Main path (user 19:12-19:13Z): the other
// device shows a link QR, this phone scans it (or types its code), joins
// through the hub and waits for the other device to approve; what arrives is
// reviewed (its hubs, as this device reaches them: user 20:38Z) and saved
// only on Confirm. Also: this device shows a code for the other one to
// approve, a key QR, a key file, or the recovery words. A bundle that brings
// hubs goes straight to the app; the words (or a bundle without hubs) go on
// to the add-hubs step.
import { useEffect, useRef, useState, type ReactNode } from "react";
import { copyWords, downloadWords } from "../lib/recovery";
import { api, type LinkEvent, type LinkStart, type Probe } from "../api";
import { Icon, Logo, type IconName } from "../lib/icons";
import { baseName, errText, pickKeyFile, scanQr } from "../lib/native";
import { getSnap, refreshAll, refreshState, setOnboarding, useSnap } from "../lib/store";
import { splitAddr } from "../lib/peers";
import { toast } from "../lib/toast";
import { HubAdder } from "./HubAdder";
import { HubReview, useHubReview } from "./HubReview";
import { chatLinkTarget } from "../lib/chatlink";
import { usePendingLink } from "../lib/visibility";
import { formatCode } from "./LinkDevice";
import { NoteCard, QR, useNow, usePlatform } from "./ui";
import footerCrop from "../assets/desktop-footer-qr.png";
import footerQr from "../assets/desktop-footer-qr.json";

// review: the hubs a linked identity brings (shown by LinkWait once it arrives)
type Step = "welcome" | "id" | "addr" | "hub" | "key" | "method" | "restore" | "linkhub" | "linkcode" | "scan" | "keyfile"
  | "typecode" | "joinname" | "joinhub" | "join" | "needgive" | "review";
// join: scan the other device's link QR; type: type its code (and the hub)
type Flow = "new" | "words" | "link" | "qr" | "file" | "join" | "type";
const FLOW: Record<Flow, Step[]> = {
  new: ["welcome", "id", "addr", "hub", "key"],
  words: ["welcome", "method", "restore", "hub"],
  link: ["welcome", "method", "linkhub", "linkcode", "review"],
  qr: ["welcome", "method", "scan", "hub"],
  file: ["welcome", "method", "keyfile", "hub"],
  join: ["welcome", "method", "scan", "joinname", "join", "review"],
  type: ["welcome", "method", "typecode", "joinname", "join", "review"],
};
const VIA: Record<Flow, string> = { new: "", words: "Your words", link: "Linking", qr: "The QR code", file: "Your key file", join: "Linking", type: "Linking" };

/** The other device's code, and the hub to join it through (verified: this device reached it). */
interface Join { code: string; hub: string | null; hubs?: string[]; hubName?: string | null; ok?: { url: string; name: string } }

interface Action { label: ReactNode; onClick: () => void; primary?: boolean; disabled?: boolean }

function Frame({ step, flow, back, actions, wide, children }: { step: Step; flow: Flow; back?: () => void; actions: Action[]; wide?: boolean; children: ReactNode }) {
  const platform = usePlatform();
  const cur = step === "joinhub" ? "joinname" : step === "needgive" ? "scan" : step;
  const dots = step === "welcome" ? null : <div className="dots">{FLOW[flow].map((s) => <i key={s} className={s === cur ? "on" : ""} />)}</div>;
  const btn = (a: Action, i: number, block: boolean) => (
    <button key={i} className={"btn" + (a.primary ? " primary" : " ghost") + (block ? " block" : "")} onClick={a.onClick} disabled={a.disabled}>{a.label}</button>
  );
  if (platform === "android") {
    return (
      <div className="screen"><div className="ob">
        {back ? <button className="icon-btn obback" onClick={back} aria-label="Back"><Icon name="back" /></button> : null}
        <div className="ob-body" style={back ? { paddingTop: 4 } : undefined}>{children}</div>
        {step !== "welcome" ? <div className="ob-foot">{dots}{actions.map((a, i) => btn(a, i, true))}</div> : null}
      </div></div>
    );
  }
  return (
    <div className="app"><div className="body"><div className="ob">
      <div className={"ob-card" + (wide ? " wide" : "")}>
        {children}
        {step !== "welcome" ? (
          <div className="ob-foot">
            {back ? <button className="btn ghost" onClick={back}><Icon name="back" />Back</button> : <span />}
            {dots}
            <div style={{ display: "flex", gap: 8 }}>{[...actions].reverse().map((a, i) => btn(a, i, false))}</div>
          </div>
        ) : null}
      </div>
    </div></div></div>
  );
}

const ObAddr = ({ a }: { a: string }) => { const [h, t] = splitAddr(a); return <span className="mono obaddr">@net:{h}<span className="tg acc">{t}</span></span>; };

function Feat({ ic, t, s }: { ic: IconName; t: string; s: string }) {
  return <div className="ob-feat"><span className="fi"><Icon name={ic} /></span><div><b>{t}</b><span>{s}</span></div></div>;
}
function Opt({ cls, ic, t, s, tag, onClick }: { cls: string; ic: IconName; t: string; s: string; tag?: ReactNode; onClick?: () => void }) {
  return (
    <button className={cls} onClick={onClick}>
      <span className="fi"><Icon name={ic} /></span>
      <span className="t"><b>{t}{tag}</b><span>{s}</span></span>
      <Icon name="chevron_right" className="chev" />
    </button>
  );
}
const Tag = ({ acc, children }: { acc?: boolean; children: ReactNode }) => <> <span className={"chip" + (acc ? " acc" : "")}>{children}</span></>;
const pad = (platform: string) => (platform === "android" ? { padding: "0 20px" } : undefined);

/** What the browser mock's camera reads by default: a PC's link QR. */
const MOCK_LINK_QR = "hubchat://link?code=M3PX-7QRT-K2ZD-9HAW&hub=" + encodeURIComponent("http://hub.office.lan:7370") + "&role=give";

/** A real crop of the desktop sidebar footer (tools/make-footer-crop.mjs) with
 *  its QR button ringed: where to click on the PC to show the link QR. */
function FooterHint() {
  const q = footerQr;
  return (
    <figure className="fhint">
      <div className="fhint-img">
        <img src={footerCrop} alt="The bottom of Hubchat's chat list on a PC: your address, the copy button and the QR code button" />
        <i className="fhint-ring" style={{ left: `calc(${q.x}% - 5px)`, top: `calc(${q.y}% - 5px)`, width: `calc(${q.w}% + 10px)`, height: `calc(${q.h}% + 10px)` }} />
      </div>
      <figcaption>On your PC, click this button in Hubchat to show the code.</figcaption>
    </figure>
  );
}

// ------------------------------------------------- link: this device waits
type Arrived = Extract<LinkEvent, { state: "review" }>;
type Wait = { k: "starting" } | { k: "waiting"; s: LinkStart; until: number } | { k: "failed"; msg: string } | { k: "expired" }
  | { k: "review"; ev: Arrived; busy?: boolean; err?: string | null };

/** An identity arrived: its hubs, as this device reaches them, to check
 *  before anything is saved. Confirm adopts it with the ticked hubs. */
function ReviewStep({ ev, flow, busy, err, onConfirm, onCancel }: { ev: Arrived; flow: Flow; busy?: boolean; err?: string | null; onConfirm: (urls: string[]) => void; onCancel: () => void }) {
  const platform = usePlatform();
  const device = platform === "android" ? "phone" : "PC";
  const rev = useHubReview(ev.hubs);
  const back = () => { if (!busy) onCancel(); };
  return (
    <Frame step="review" flow={flow} back={back} wide
      actions={[{ label: busy ? "Saving…" : "Confirm", primary: true, disabled: busy || rev.blocked, onClick: () => onConfirm(rev.urls) }, { label: "Cancel", disabled: busy, onClick: back }]}>
      <h2>Bring these hubs to this {device}?</h2>
      <p className="lead"><ObAddr a={ev.to} />{ev.name ? <> ({ev.name})</> : null} arrived. {ev.hubs.length
        ? <>These hubs come with it. Check the address this {device} should use for each; untick any you don't want.</>
        : <>No hubs came with it. Add the one this {device} should use.</>}</p>
      <HubReview rev={rev} disabled={busy} />
      {err ? <div className="probe-card bad"><Icon name="error" /><div><b>Couldn't save it</b>{err}</div></div> : null}
    </Frame>
  );
}

/** Show the code and its QR, listen for approval, count down 10 minutes,
 *  then review what arrived. With `code` (the other device's code, scanned
 *  or typed) this device joins that code instead and shows no code of its
 *  own; `other` then offers a new scan (or a new typed code) when it fails
 *  or runs out. */
function LinkWait({ hub, hubName, name, code, aliases, other, onDone, onCancel, step, flow }: { hub: string; hubName: string; name: string; code?: string; aliases?: string[]; other?: Action; onDone: (address: string) => void; onCancel: () => void; step: Step; flow: Flow }) {
  const platform = usePlatform();
  const [st, setSt] = useState<Wait>({ k: "starting" });
  const [attempt, setAttempt] = useState(0);
  const now = useNow(st.k === "waiting");
  const startedFor = useRef(-1);
  const alive = useRef(false);
  /** live: listening; review: an identity waits for Confirm; over: confirmed or cancelled. */
  const phase = useRef<"live" | "review" | "over">("live");
  const onDoneRef = useRef(onDone);
  onDoneRef.current = onDone;

  // leaving this screen stops listening, or drops what arrived unconfirmed
  // (not on StrictMode's re-mount)
  useEffect(() => {
    alive.current = true;
    return () => {
      alive.current = false;
      setTimeout(() => {
        if (alive.current) return;
        if (phase.current === "live") void api.linkCancel().catch(() => {});
        else if (phase.current === "review") void api.linkDiscard().catch(() => {});
      }, 0);
    };
  }, []);
  useEffect(() => {
    let un: (() => void) | null = null; let gone = false;
    void api.onLink((e) => {
      if (e.state === "waiting") setSt((s) => (s.k === "waiting" ? { ...s, until: Date.now() + e.expires_in_s * 1000 } : s));
      else if (e.state === "review") { phase.current = "review"; setSt({ k: "review", ev: e }); }
      else if (e.state === "failed") setSt({ k: "failed", msg: e.error });
      else if (e.state === "expired") setSt({ k: "expired" });
    }).then((u) => { if (gone) u(); else un = u; });
    return () => { gone = true; un?.(); };
  }, []);
  useEffect(() => {
    if (startedFor.current === attempt) return;
    startedFor.current = attempt;
    setSt({ k: "starting" });
    api.linkStart(hub, name, code ?? null, aliases ?? null).then(
      (s) => setSt({ k: "waiting", s, until: Date.now() + 600e3 }),
      (e) => setSt({ k: "failed", msg: errText(e) }),
    );
  }, [attempt, hub, name, code]);

  const cancel = () => {
    const was = phase.current;
    phase.current = "over";
    if (was === "review") void api.linkDiscard().catch(() => {});
    else void api.linkCancel().catch(() => {});
    onCancel();
  };
  const again = () => setAttempt((a) => a + 1);
  const confirm = async (urls: string[]) => {
    setSt((s) => (s.k === "review" ? { ...s, busy: true, err: null } : s));
    let address: string;
    try { address = await api.linkConfirm(urls); }
    catch (e) { setSt((s) => (s.k === "review" ? { ...s, busy: false, err: errText(e) } : s)); return; }
    phase.current = "over";
    onDoneRef.current(address);
  };
  const device = platform === "android" ? "phone" : "PC";
  const otherDev = platform === "android" ? "your PC" : "your other device";
  if (st.k === "review") return <ReviewStep ev={st.ev} flow={flow} busy={st.busy} err={st.err} onConfirm={(urls) => void confirm(urls)} onCancel={cancel} />;
  if (code) {
    let jb: ReactNode;
    if (st.k === "waiting") {
      const left = Math.max(0, Math.ceil((st.until - now) / 1000));
      const mmss = Math.floor(left / 60) + ":" + String(left % 60).padStart(2, "0");
      jb = (
        <>
          <div className="approve"><Icon name={platform === "android" ? "phone" : "computer"} /><div className="t"><b>“{name}”</b><span>Asking to be linked · code {st.s.code}</span></div></div>
          <div className="waiting"><span className="spin" /><span>Waiting for you to approve it on {otherDev}…</span><span className="cd" title="The code works for 10 minutes">{mmss}</span></div>
          <div className="help" style={{ marginTop: 10, ...pad(platform) }}>{otherDev === "your PC" ? "Your PC" : "Your other device"} shows <b>Link “{name}”?</b> under <b>Link a device › Show a QR code</b>. Your key, hub list and profile come back sealed with the code through {hubName}: the hub carries them but can't read them.</div>
        </>
      );
    } else if (st.k === "starting") {
      jb = <div className="probe-card busy"><span className="spin" /><div>Joining through {hubName}…</div></div>;
    } else if (st.k === "expired") {
      jb = <div className="probe-card bad"><Icon name="hourglass" /><div><b>The code expired</b>It wasn't approved within 10 minutes. Show a new code on {otherDev} ({otherDev === "your PC" ? "the QR button, or " : ""}Link a device › Show a QR code) and try again.</div></div>;
    } else {
      jb = <div className="probe-card bad"><Icon name="error" /><div><b>Linking didn't work</b>{st.msg}</div></div>;
    }
    const acts: Action[] = st.k === "waiting" || st.k === "starting"
      ? [{ label: "Cancel", onClick: cancel }]
      : [...(other ? [{ ...other, primary: true }] : []), ...(st.k === "failed" ? [{ label: "Try again", primary: !other, onClick: again }] : []), { label: "Cancel", onClick: cancel }];
    return (
      <Frame step={step} flow={flow} back={cancel} wide actions={acts}>
        <h2>Approve this {device} on {otherDev}</h2>
        <p className="lead">A request from <b>“{name}”</b> is waiting there. Tap <b>Approve</b> on {otherDev} to bring your identity to this {device}.</p>
        {jb}
      </Frame>
    );
  }
  let body: ReactNode;
  if (st.k === "waiting") {
    const left = Math.max(0, Math.ceil((st.until - now) / 1000));
    const mmss = Math.floor(left / 60) + ":" + String(left % 60).padStart(2, "0");
    body = (
      <>
        {platform === "android"
          ? <div className="obpair"><QR text={st.s.qr} size={190} /><div className="paircode long">{st.s.code}</div></div>
          : <div className="pair"><QR text={st.s.qr} size={168} /><div style={{ minWidth: 0 }}><div className="help">This {device}'s code</div><div className="paircode long">{st.s.code}</div>
              <ol><li>A phone scans the QR code.</li><li>A PC types the code.</li><li>Check that it names <b>{name}</b>, then approve.</li></ol></div></div>}
        <div className="waiting"><span className="spin" /><span>Waiting for your other device to approve…</span><span className="cd" title="The code works for 10 minutes">{mmss}</span></div>
        <div className="help" style={{ marginTop: 10, ...pad(platform) }}>The code works once, for 10 minutes, through {hubName}. Your key, hub list and profile come back sealed with it: the hub carries them but can't read them.</div>
      </>
    );
  } else if (st.k === "starting") {
    body = <div className="probe-card busy"><span className="spin" /><div>Getting a code from {hubName}…</div></div>;
  } else if (st.k === "expired") {
    body = <div className="probe-card bad"><Icon name="hourglass" /><div><b>The code expired</b>Nobody approved it within 10 minutes. Get a new code and approve it from your other device.</div></div>;
  } else {
    body = <div className="probe-card bad"><Icon name="error" /><div><b>Linking didn't work</b>{st.msg}</div></div>;
  }
  const actions: Action[] = st.k === "waiting" || st.k === "starting"
    ? [{ label: "Cancel", onClick: cancel }]
    : [{ label: "Get a new code", primary: true, onClick: again }, { label: "Cancel", onClick: cancel }];
  return (
    <Frame step={step} flow={flow} back={cancel} wide actions={actions}>
      <h2>Approve this {device} from your other device</h2>
      <p className="lead">On a device that already uses Hubchat: <b>Settings › Devices › Link a device</b>, then scan this code or type it.</p>
      {body}
    </Frame>
  );
}

export function Onboarding() {
  const snap = useSnap();
  const platform = usePlatform();
  const device = platform === "android" ? "phone" : "PC";
  const [step, setStep] = useState<Step>("welcome");
  const [flow, setFlow] = useState<Flow>("new");
  const [id, setId] = useState("");
  const [name, setName] = useState("");
  // The id follows the display name until the user edits it (user 18:24Z).
  const [idTouched, setIdTouched] = useState(false);
  const [idCheck, setIdCheck] = useState<{ ok: boolean; error: string | null; max_len: number }>({ ok: false, error: null, max_len: 24 });
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState<string | null>(null);
  const [address, setAddress] = useState("");
  const [added, setAdded] = useState<{ url: string; name: string }[]>([]);
  const [more, setMore] = useState(false);
  const [words, setWords] = useState<string[]>([]);
  const [restore, setRestore] = useState("");
  const [linkHub, setLinkHub] = useState<{ url: string; name: string } | null>(null);
  // what the other device shows when it asks to approve: this device's own name
  const devName = snap.state?.device_name || (platform === "android" ? "Android phone" : "Windows PC");
  const [keyFile, setKeyFile] = useState<string | null>(null);
  const [pass, setPass] = useState("");
  const [join, setJoin] = useState<Join | null>(null);
  const [joinProbe, setJoinProbe] = useState<Probe | null>(null);
  const [code, setCode] = useState("");
  const otherDev = platform === "android" ? "your PC" : "your other device";

  const finish = async () => { setOnboarding(false); await refreshAll(); };
  useEffect(() => {
    let live = true;
    api.checkId(id).then((r) => { if (live) setIdCheck(r); }, () => {});
    return () => { live = false; };
  }, [id]);
  useEffect(() => {
    if (step !== "key" || words.length) return;
    api.recoveryWords().then(setWords, (e) => setErr(errText(e)));
  }, [step, words.length]);

  const go = (s: Step) => { setErr(null); setBusy(false); setStep(s); };
  const pick = (f: Flow, s: Step) => { setFlow(f); go(s); };
  /** An identity arrived (link, QR, key file): into the app if it brought hubs, else add some. */
  const arrived = async (a: string) => {
    setAddress(a);
    await refreshAll();
    if (getSnap().state?.hubs.length) { toast("Welcome back"); await finish(); }
    else go("hub");
  };

  /** Join the other device's code through `hub` (checked next, on the name step). */
  const joinWith = (c: string, hub: string | null, f: Flow, hubs: string[] = [], hubName: string | null = null) => {
    setJoin({ code: c, hub, hubs, hubName }); setJoinProbe(null); setFlow(f); go("joinname");
  };
  // a phone camera opened a hubchat:// link: a signed-in device's QR (role
  // give) is joined straight away; a waiting device's (role take) can't be
  // served from here, this device has no identity to give
  usePendingLink((input) => {
    if (getSnap().state?.me) return;
    if (chatLinkTarget(input) !== null) { toast("Set up Hubchat first, then open the chat link again."); return; }
    api.parseLink(input).then((p) => { if (p.role === "take") { setFlow("join"); go("needgive"); } else joinWith(p.code, p.hub, "join", p.hubs, p.hub_name); }, (e) => toast(errText(e)));
  });
  // the name step checks that this device reaches the code's hub; if not, ask for an address that works
  useEffect(() => {
    if (step !== "joinname" || !join) return;
    if (join.ok) { go("join"); return; }
    if (!join.hub) { go("joinhub"); return; }
    let live = true;
    // the link may name its hub several ways (a PC's localhost hub: its Tailscale name and addresses)
    (join.hubs && join.hubs.length > 0 ? api.probeLinkHubs(join.hubs, join.hubName ?? null) : api.probeHub(join.hub)).then((p) => {
      if (!live) return;
      if (p.result === "connected") setJoin((j) => (j ? { ...j, ok: { url: p.url, name: p.name } } : j));
      else { setJoinProbe(p); go("joinhub"); }
    }, (e) => { if (live) { setJoinProbe({ result: "invalid", error: errText(e) }); go("joinhub"); } });
    return () => { live = false; };
  }, [step, join]);

  /** Scan the other device's link QR (a key QR restores straight away, as before). */
  const scanLink = async () => {
    setBusy(true); setErr(null);
    try {
      const t = (await scanQr(MOCK_LINK_QR))?.trim();
      if (!t) { setBusy(false); return; }
      if (/^hubchat-key/i.test(t)) { setFlow("qr"); const a = await api.restoreQr(t); await arrived(a); return; }
      if (chatLinkTarget(t) !== null) {
        setErr("That's someone's chat QR code, not a link code. On your PC, click the QR button at the bottom of Hubchat's chat list, then scan the code it shows.");
        setBusy(false); return;
      }
      if (!/^(hubchat:\/\/|hubchat-link:)/i.test(t)) {
        setErr("That QR code isn't from Hubchat. On your PC, click the QR button at the bottom of Hubchat's chat list, then scan the code it shows.");
        setBusy(false); return;
      }
      const p = await api.parseLink(t);
      if (p.role === "take") { go("needgive"); return; }
      joinWith(p.code, p.hub, "join", p.hubs, p.hub_name);
    } catch (e) { setErr(errText(e)); setBusy(false); }
  };
  const startScan = () => { pick("join", "scan"); void scanLink(); };

  if (step === "welcome") {
    return (
      <Frame step={step} flow={flow} actions={[]}>
        <Logo size={64} />
        <h1>Hubchat</h1>
        <p className="lead" style={{ marginBottom: 0 }}>Chat with your agents, and people, over your own Orgtree mail hub.</p>
        <div className="ob-feats">
          <Feat ic="key" t="Your key is your identity" s={"Made on your " + device + "; your address comes from it. No account, and nobody issues it."} />
          <Feat ic="dns" t="Your hub, your network" s="Messages travel through a hub you choose. Nothing goes to a cloud." />
          <Feat ic="bot" t="Agents are first-class" s="Talk to Orgtree orgs and AI agent sessions, such as Claude Code or Codex, the way you talk to people." />
        </div>
        <div className="fork" style={pad(platform)}>
          <Opt cls="fork-opt primary" ic="add" t="Create a new identity" s="Choose an id. Hubchat makes your key and your address." onClick={() => pick("new", "id")} />
          <Opt cls="fork-opt" ic="sync" t="I already use Hubchat" s="Bring your identity from another device, a key file or your recovery words." onClick={() => pick("words", "method")} />
        </div>
      </Frame>
    );
  }

  if (step === "id") {
    const showErr = id && !idCheck.ok ? idCheck.error : null;
    const create = async () => {
      setBusy(true); setErr(null);
      try { const a = await api.createIdentity(id, name.trim()); setAddress(a); await refreshState(); go("addr"); }
      catch (e) { setErr(errText(e)); }
      finally { setBusy(false); }
    };
    return (
      <Frame step={step} flow={flow} back={() => go("welcome")} actions={[{ label: busy ? "Creating…" : "Continue", primary: true, disabled: !idCheck.ok || busy, onClick: create }]}>
        <h2>Who are you?</h2>
        <p className="lead">People and agents reach you by your address. Your id becomes part of it and can't change later; your display name can.</p>
        <div className="field">
          <label htmlFor="ob-name">Display name</label>
          <label className="input"><input id="ob-name" value={name} autoFocus placeholder="e.g. Alex Rivera" maxLength={48} autoComplete="off"
            onChange={(e) => { setName(e.target.value); if (!idTouched) setId(idFromName(e.target.value, idCheck.max_len)); }}
            onKeyDown={(e) => { if (e.key === "Enter" && idCheck.ok && !busy) void create(); }} /></label>
          <div className="help">How you appear to others. You can change it later.</div>
        </div>
        <div className="field">
          <label htmlFor="ob-id">Your id</label>
          <label className={"input" + (showErr ? " bad" : "")}>
            <input id="ob-id" value={id} placeholder="e.g. alex-rivera" maxLength={idCheck.max_len} autoComplete="off" autoCapitalize="off" spellCheck={false}
              onChange={(e) => { setIdTouched(true); setId(e.target.value.toLowerCase().replace(/\s+/g, "-")); }}
              onKeyDown={(e) => { if (e.key === "Enter" && idCheck.ok && !busy) void create(); }} />
          </label>
          <div className={"help" + (showErr ? " bad" : "")}>{showErr || (idTouched ? "" : "Made from your display name until you edit it. ") + "Lowercase letters, numbers, “.”, “_” and “-”, up to " + idCheck.max_len + " characters."}</div>
        </div>
        <div className="addrprev">
          {platform === "desktop" ? <Icon name="at" /> : null}
          <span>Your address will be</span>
          <span className="mono">@net:{idCheck.ok ? id : <span className="faint">your-id</span>}.<span className="tg pend">······</span></span>
        </div>
        <div className="help" style={{ marginTop: 8, ...pad(platform) }}>The 6-character tag comes from the key Hubchat makes next. It keeps your address unique: two people can both be “{idCheck.ok ? id : "alex"}”.</div>
        {err ? <div className="help bad" style={{ marginTop: 8, ...pad(platform) }}>{err}</div> : null}
      </Frame>
    );
  }

  if (step === "addr") {
    const [h, t] = splitAddr(address);
    return (
      <Frame step={step} flow={flow} actions={[{ label: "Continue", primary: true, onClick: () => go("hub") }]}>
        <h2>This is you</h2>
        <p className="lead">Hubchat made your key on this {device}. Your address is your id plus a tag taken from the key.</p>
        <div className="addr-hero">
          <div className="ah">@net:{h}<span className="tg acc">{t}</span></div>
          <div className="ah-parts"><span><b>{h.replace(/\.$/, "")}</b>your id</span><span><b className="acc">{t}</b>from your key</span></div>
        </div>
        <NoteCard icon="key"><b>Your key is your identity.</b> Whoever holds it is you, on every hub. Next: add a hub, then save your recovery words.</NoteCard>
      </Frame>
    );
  }

  if (step === "hub") {
    const next = () => (flow === "new" ? go("key") : void finish());
    const existing = snap.state?.hubs.map((h) => h.url) || [];
    return (
      <Frame step={step} flow={flow}
        actions={[{ label: "Continue", primary: true, disabled: !added.length, onClick: next }, ...(added.length ? [] : [{ label: "Skip for now", onClick: next }])]}>
        <h2>{flow !== "new" ? "Add your hubs" : "Add your hub"}</h2>
        <p className="lead">
          {flow === "words"
            ? <>Your words brought back <ObAddr a={address || snap.state?.me?.address || ""} />. Hubs don't travel in the words: add the ones you use, and Hubchat signs in to each with your key.</>
            : flow === "link" || flow === "join" || flow === "type"
              // the review may have left every hub out
              ? <>{VIA[flow]} brought back <ObAddr a={address || snap.state?.me?.address || ""} /> without a hub. Add the ones you use, and Hubchat signs in to each with your key.</>
              : flow !== "new"
                ? <>{VIA[flow]} brought back <ObAddr a={address || snap.state?.me?.address || ""} />, but no hubs came with it. Add the ones you use, and Hubchat signs in to each with your key.</>
                : "Hubchat talks through an Orgtree mail hub on your network. Ask whoever runs it for the address."}
        </p>
        {added.length ? (
          <div className="hublist-ob">
            {added.map((a) => {
              const st = snap.state?.hubs.find((h) => h.url === a.url);
              return <div className="hl" key={a.url}><Icon name="check_circle" /><span><b>{st?.name || a.name}</b> <span className="mono">{a.url}</span></span>
                <span className="dim" style={{ marginLeft: "auto", fontSize: 12 }}>{st?.state === "connected" ? "connected" : st?.state === "connecting" ? "connecting…" : "will keep trying"}</span></div>;
            })}
          </div>
        ) : null}
        {!added.length || more
          ? <HubAdder existing={existing} autoFocus onAdded={(url, n) => { setAdded((l) => [...l, { url, name: n }]); setMore(false); }} onCancel={added.length ? () => setMore(false) : undefined} />
          : <>
              <div style={{ margin: "-4px 0 14px", ...pad(platform) }}><button className="link" onClick={() => setMore(true)}>+ Add another hub</button></div>
              <NoteCard icon="privacy"><b>The hub can read your messages.</b> The hub's operator can read messages that pass through it; everyone on a hub is listed in its directory.</NoteCard>
            </>}
      </Frame>
    );
  }

  if (step === "key") {
    const saved = async () => { try { await api.recoverySaved(); } catch (e) { setErr(errText(e)); return; } await finish(); };
    return (
      <Frame step={step} flow={flow} wide actions={[{ label: "I've saved them", primary: true, disabled: !words.length, onClick: saved }, { label: "Later", onClick: () => void finish() }]}>
        <h2>Save your recovery words</h2>
        <p className="lead">These 24 words hold your key and your id. If you lose every device, they bring back {address ? <ObAddr a={address} /> : "your address"} on a new one.</p>
        {words.length ? <ol className="words">{words.map((w, i) => <li key={i}>{w}</li>)}</ol> : <div className="probe-card busy"><span className="spin" /><div>Reading your words…</div></div>}
        <div className={"keyacts" + (platform === "android" ? " pad" : "")} style={{ marginTop: 10 }}>
          <button className="btn" disabled={!words.length} onClick={() => void copyWords(words)}><Icon name="copy" />Copy words</button>
          <button className="btn" disabled={!words.length} onClick={() => void downloadWords(platform, snap.state?.me?.id || "words")}><Icon name="download" />Download</button>
        </div>
        <NoteCard icon="warning" warn><b>Keep them private.</b> Anyone with these words can read your messages and send as you. Write them down, or keep them in a password manager.</NoteCard>
        {err ? <div className="help bad">{err}</div> : null}
      </Frame>
    );
  }

  if (step === "method") {
    if (platform === "android") {
      return (
        <Frame step={step} flow={flow} back={() => go("welcome")} wide actions={[]}>
          <h2>Bring your identity to this phone</h2>
          <p className="lead">Your identity is a key, and your address comes with it: there is no id to choose. The easiest way: your PC shows a QR code, this phone scans it.</p>
          <div className="methods">
            <Opt cls="method primary" ic="qr" t="Scan the QR code from your other device" s="This phone joins through your hub; you approve it on the PC. Your key, hubs and profile arrive sealed." onClick={startScan} />
          </div>
          <FooterHint />
          <div className="ob-alt"><button className="link" onClick={() => { setCode(""); pick("type", "typecode"); }}>Type the code instead</button></div>
          <div className="ob-sub">Other ways</div>
          <div className="methods">
            <Opt cls="method" ic="link" t="Show a code on this phone instead" s="This phone shows a code and your other device approves it." onClick={() => pick("link", "linkhub")} />
            <Opt cls="method" ic="file" t="Key file" s="Open a key file saved from Hubchat, with its passphrase." onClick={() => pick("file", "keyfile")} />
            <Opt cls="method" ic="key" t="Recovery words" s="Type the 24 words you saved when you made your identity." onClick={() => pick("words", "restore")} />
          </div>
        </Frame>
      );
    }
    return (
      <Frame step={step} flow={flow} back={() => go("welcome")} wide actions={[]}>
        <h2>Bring your identity to this {device}</h2>
        <p className="lead">Your identity is a key, and your address comes with it: there is no id to choose. How should this {device} get it?</p>
        <div className="methods">
          <Opt cls="method" ic="link" t="Link through a hub" s={"This " + device + " shows a code and your other device approves it. Your key, hubs and profile arrive sealed with that code."} tag={<Tag acc>Recommended</Tag>} onClick={() => pick("link", "linkhub")} />
          <Opt cls="method" ic="dialpad" t="Type a code from your other device" s="Your other device shows a code under Link a device › Show a QR code. Type it here, and approve this PC there." onClick={() => { setCode(""); pick("type", "typecode"); }} />
          <Opt cls="method" ic="qr" t="Scan a QR code" s="Your other device shows your key as a QR code. No network needed." tag={<Tag>Needs a camera</Tag>} onClick={() => pick("qr", "scan")} />
          <Opt cls="method" ic="file" t="Key file" s="Open a key file saved from Hubchat, with its passphrase." onClick={() => pick("file", "keyfile")} />
          <Opt cls="method" ic="key" t="Recovery words" s="Type the 24 words you saved when you made your identity." onClick={() => pick("words", "restore")} />
        </div>
      </Frame>
    );
  }

  if (step === "linkhub") {
    return (
      <Frame step={step} flow={flow} back={() => go("method")} wide actions={[]}>
        <h2>Which hub does your other device use?</h2>
        <p className="lead">Linking travels through a hub, so this {device} needs to reach one that your other device uses. Nothing is added yet: the hubs you use come with your identity.</p>
        <HubAdder existing={[]} autoFocus onPick={(url, n) => { setLinkHub({ url, name: n }); go("linkcode"); }} />
      </Frame>
    );
  }

  if (step === "linkcode" && linkHub) {
    return <LinkWait step={step} flow={flow} hub={linkHub.url} hubName={linkHub.name} name={devName.trim()} onDone={(a) => void arrived(a)} onCancel={() => go("method")} />;
  }

  if (step === "typecode") {
    const isUrl = /^hubchat:\/\//i.test(code.trim());
    const ok = isUrl || code.replace(/[^A-Za-z0-9]/g, "").length === 16;
    const ready = ok && (!!linkHub || isUrl) && !busy;
    const typed = async () => {
      if (!ready) return;
      setBusy(true); setErr(null);
      try {
        const p = await api.parseLink(code);
        if (p.role === "take") { go("needgive"); return; }
        const hub = p.hub ?? linkHub?.url ?? null;
        setJoin({ code: p.code, hub, hubs: p.hubs, hubName: p.hub_name, ok: linkHub && hub === linkHub.url ? linkHub : undefined }); setJoinProbe(null);
        go("joinname");
      } catch (e) { setErr(errText(e)); setBusy(false); }
    };
    return (
      <Frame step={step} flow={flow} back={() => go("method")} wide
        actions={[{ label: busy ? "Checking…" : "Continue", primary: true, disabled: !ready, onClick: () => void typed() }]}>
        <h2>Type the code from your other device</h2>
        <p className="lead">On {otherDev}: {platform === "android" ? "click the QR button at the bottom of Hubchat's chat list" : "Settings › Devices › Link a device › Show a QR code"}. Type the code it shows under the QR code, and the hub it names.</p>
        <div className="field">
          <label htmlFor="ob-code">Code</label>
          <label className={"input lc-code" + (err ? " bad" : "")} style={{ height: 50 }}>
            <input id="ob-code" value={code} placeholder="XXXX-XXXX-XXXX-XXXX" autoFocus autoComplete="off" autoCapitalize="characters" spellCheck={false}
              onChange={(e) => { setCode(formatCode(e.target.value)); setErr(null); }} onKeyDown={(e) => { if (e.key === "Enter") void typed(); }} />
          </label>
          {err ? <div className="help bad">{err}</div> : <div className="help">16 letters and digits. Capitals and dashes don't matter.</div>}
        </div>
        {isUrl ? null : linkHub ? (
          <div className="field">
            <label>Hub</label>
            <div className="hublist-ob" style={{ margin: 0 }}><div className="hl"><Icon name="check_circle" /><span><b>{linkHub.name}</b> <span className="mono">{linkHub.url}</span></span>
              <button className="link" style={{ marginLeft: "auto" }} onClick={() => setLinkHub(null)}>Change</button></div></div>
          </div>
        ) : <HubAdder existing={[]} onPick={(url, n) => setLinkHub({ url, name: n })} />}
      </Frame>
    );
  }

  if (step === "joinname" && join) {
    // no name to type (user 00:16Z): this step only reaches the code's hub, then goes on
    return (
      <Frame step={step} flow={flow} back={() => go(flow === "type" ? "typecode" : "method")} actions={[]}>
        <h2>Linking this {device}</h2>
        <div className="probe-card busy"><span className="spin" /><div>Reaching the hub at <span className="mono">{join.hub}</span>…</div></div>
      </Frame>
    );
  }

  if (step === "joinhub" && join) {
    const why = joinProbe && joinProbe.result !== "connected" ? joinProbe.error : null;
    return (
      <Frame step={step} flow={flow} back={() => go(flow === "type" ? "typecode" : "method")} wide actions={[]}>
        <h2>Which address reaches your hub?</h2>
        {join.hub
          ? <p className="lead">The code goes through <span className="mono">{join.hub}</span>, but this {device} can't reach that address{why ? <> ({why})</> : null}.</p>
          : <p className="lead">Enter the hub {otherDev} uses.</p>}
        {join.hub ? <NoteCard icon="dns">{platform === "android" ? "Your PC" : "Your other device"} reached the hub at this address; enter the address this {device} can use (for example its Tailscale name).</NoteCard> : null}
        <HubAdder key={join.hub ?? ""} existing={[]} initial={join.hub ?? ""} autoFocus onPick={(url, n) => { setJoin({ ...join, hub: url, ok: { url, name: n } }); go("joinname"); }} />
      </Frame>
    );
  }

  if (step === "join" && join?.ok) {
    const other: Action | undefined = flow === "type"
      ? { label: "Type a new code", onClick: () => { setCode(""); go("typecode"); } }
      : platform === "android" ? { label: <><Icon name="camera" />Scan again</>, onClick: () => { go("scan"); void scanLink(); } } : undefined;
    return <LinkWait step={step} flow={flow} hub={join.ok.url} hubName={join.ok.name} name={devName.trim()} code={join.code} aliases={join.hubs} other={other} onDone={(a) => void arrived(a)} onCancel={() => go("method")} />;
  }

  if (step === "scan") {
    if (platform === "desktop") {
      return (
        <Frame step={step} flow={flow} back={() => go("method")} wide actions={[]}>
          <h2>This PC can't scan a code</h2>
          <p className="lead">Scanning your key needs a camera, and PCs rarely have one that can read a phone's screen. Use one of these instead:</p>
          <div className="methods">
            <Opt cls="method" ic="link" t="Link through a hub" s="This PC shows a code and your phone approves it." tag={<Tag acc>Recommended</Tag>} onClick={() => pick("link", "linkhub")} />
            <Opt cls="method" ic="dialpad" t="Type a code from your other device" s="Your phone shows a code under Link a device › Show a QR code. Type it here." onClick={() => { setCode(""); pick("type", "typecode"); }} />
            <Opt cls="method" ic="file" t="Key file" s="Save a key file on your other device, then open it here." onClick={() => pick("file", "keyfile")} />
            <Opt cls="method" ic="key" t="Recovery words" s="Type the 24 words you saved." onClick={() => pick("words", "restore")} />
          </div>
        </Frame>
      );
    }
    return (
      <Frame step={step} flow={flow} back={() => go("method")} actions={[{ label: busy ? "Scanning…" : <><Icon name="camera" />Open the camera</>, primary: true, disabled: busy, onClick: () => void scanLink() }]}>
        <h2>Scan the QR code from your other device</h2>
        <p className="lead">On your PC, open Hubchat and click the QR button at the bottom of the chat list (or <b>Settings › Devices › Link a device</b>). Then scan the code it shows.</p>
        <FooterHint />
        {err ? <div className="probe-card bad"><Icon name="error" /><div><b>Couldn't use that code</b>{err}</div></div>
          : <div className="help" style={{ marginTop: 12, ...pad(platform) }}>A QR code of your key (<b>Link a device › My key as a QR code</b>) works here too, with no network.</div>}
        <div className="ob-alt"><button className="link" onClick={() => { setCode(""); pick("type", "typecode"); }}>Type the code instead</button></div>
      </Frame>
    );
  }

  if (step === "needgive") {
    const acts: Action[] = platform === "android"
      ? [{ label: busy ? "Scanning…" : <><Icon name="camera" />Scan the right QR code</>, primary: true, disabled: busy, onClick: () => { setFlow("join"); go("scan"); void scanLink(); } }]
      : [{ label: "Type its code instead", primary: true, onClick: () => { setCode(""); pick("type", "typecode"); } }];
    return (
      <Frame step={step} flow={flow} back={() => go("method")} wide actions={acts}>
        <h2>That device is waiting for an identity too</h2>
        <p className="lead">The code you {platform === "android" ? "scanned" : "entered"} comes from a device that is waiting to be given an identity, and this {device} has none to give yet.</p>
        <NoteCard icon="qr"><b>Scan the QR your signed-in device shows (Settings › Devices › Link a device).</b> {platform === "android" ? "On a PC, the QR button at the bottom of Hubchat's chat list shows it too." : "This PC can also type the code shown under that QR code."}</NoteCard>
      </Frame>
    );
  }

  if (step === "keyfile") {
    const choose = async () => {
      setErr(null);
      try { const f = await pickKeyFile(); if (f) { setKeyFile(f); setPass(""); } } catch (e) { setErr(errText(e)); }
    };
    const unlock = async () => {
      if (!keyFile || !pass) return;
      setBusy(true); setErr(null);
      try { const a = await api.keyFileImport(keyFile, pass); await arrived(a); }
      catch (e) { setErr(errText(e)); setBusy(false); }
    };
    return (
      <Frame step={step} flow={flow} back={() => go("method")} wide actions={keyFile ? [{ label: busy ? "Unlocking…" : <><Icon name="lock" />Unlock</>, primary: true, disabled: !pass || busy, onClick: unlock }] : []}>
        <h2>Open your key file</h2>
        <p className="lead">A key file saved from Hubchat holds your key, your id, your profile and your hubs, locked with the passphrase you chose.</p>
        {!keyFile
          ? <div className="kdrop"><Icon name="file" /><b>Choose your key file</b><span>Usually <span className="mono">hubchat-key.hubchat-key</span>, saved from <b>Settings › Devices › Link a device › Key file</b>.</span><button className="btn" onClick={choose}>Choose file…</button></div>
          : <>
              <div className="kfile"><Icon name="key" /><div className="t"><b>{baseName(keyFile)}</b><span>Locked with a passphrase</span></div><button className="link" onClick={choose}>Change</button></div>
              <div className="field">
                <label htmlFor="ob-pass">Passphrase</label>
                <label className={"input" + (err ? " bad" : "")}><Icon name="lock" />
                  <input id="ob-pass" type="password" value={pass} autoFocus placeholder="The passphrase you chose when saving it" autoComplete="off"
                    onChange={(e) => { setPass(e.target.value); setErr(null); }} onKeyDown={(e) => { if (e.key === "Enter") void unlock(); }} />
                </label>
              </div>
            </>}
        {err ? <div className="probe-card bad"><Icon name="error" /><div><b>Couldn't open it</b>{err}</div></div> : null}
      </Frame>
    );
  }

  // restore (recovery words)
  const n = restore.toLowerCase().split(/[^a-z]+/).filter(Boolean).length;
  const doRestore = async () => {
    setBusy(true); setErr(null);
    try { const a = await api.restoreWords(restore); setAddress(a); await refreshState(); go("hub"); }
    catch (e) { setErr(errText(e)); }
    finally { setBusy(false); }
  };
  return (
    <Frame step={step} flow={flow} back={() => go("method")} wide actions={[{ label: busy ? "Checking…" : "Continue", primary: true, disabled: !n || busy, onClick: doRestore }]}>
      <h2>Recovery words</h2>
      <p className="lead">Your words carry your key and your id, so your address comes back exactly as it was.</p>
      <div className="field">
        <label htmlFor="ob-words">Your 24 recovery words, in order</label>
        <textarea id="ob-words" className="wordsin" rows={4} value={restore} autoFocus placeholder="amber lantern …" spellCheck={false} autoComplete="off" autoCapitalize="off"
          onChange={(e) => { setRestore(e.target.value); setErr(null); }} />
      </div>
      <div className={"wstat" + (err ? " bad" : "")}>
        <span>{err ? <><Icon name="error" />{err}</> : n + " of 24 words"}</span>
      </div>
    </Frame>
  );
}

/** An id made from a display name: lowercase, accents dropped, spaces and
 *  other separators as "-", only a-z 0-9 . _ -, starting with a letter or
 *  digit, at most `max` characters. */
export function idFromName(name: string, max: number): string {
  let s = name.normalize("NFKD").replace(/[\u0300-\u036f]/g, "").toLowerCase();
  s = s.replace(/[\s/\,;:+&]+/g, "-").replace(/[^a-z0-9._-]/g, "").replace(/[-._]*-[-._]*/g, "-");
  s = s.replace(/^[^a-z0-9]+/, "").slice(0, max).replace(/[._-]+$/, "");
  return s;
}
