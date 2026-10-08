// First run. The fork: create a new identity (id → address → hubs → recovery
// words) or bring an existing one: link through a hub (this device shows a
// code, the other approves it), scan a QR code (Android), a key file, or the
// recovery words. A bundle that brings hubs goes straight to the app; the
// words (or a bundle without hubs) go on to the add-hubs step.
import { useEffect, useRef, useState, type ReactNode } from "react";
import { api, type LinkStart } from "../api";
import { Icon, Logo, type IconName } from "../lib/icons";
import { baseName, errText, copyText, pickKeyFile, scanQr } from "../lib/native";
import { getSnap, refreshAll, refreshState, setOnboarding, useSnap } from "../lib/store";
import { splitAddr } from "../lib/peers";
import { toast } from "../lib/toast";
import { HubAdder } from "./HubAdder";
import { NoteCard, QR, useNow, usePlatform } from "./ui";

type Step = "welcome" | "id" | "addr" | "hub" | "key" | "method" | "restore" | "linkhub" | "linkname" | "linkcode" | "scan" | "keyfile";
type Flow = "new" | "words" | "link" | "qr" | "file";
const FLOW: Record<Flow, Step[]> = {
  new: ["welcome", "id", "addr", "hub", "key"],
  words: ["welcome", "method", "restore", "hub"],
  link: ["welcome", "method", "linkhub", "linkname", "linkcode"],
  qr: ["welcome", "method", "scan", "hub"],
  file: ["welcome", "method", "keyfile", "hub"],
};
const VIA: Record<Flow, string> = { new: "", words: "Your words", link: "Linking", qr: "The QR code", file: "Your key file" };

interface Action { label: ReactNode; onClick: () => void; primary?: boolean; disabled?: boolean }

function Frame({ step, flow, back, actions, wide, children }: { step: Step; flow: Flow; back?: () => void; actions: Action[]; wide?: boolean; children: ReactNode }) {
  const platform = usePlatform();
  const dots = step === "welcome" ? null : <div className="dots">{FLOW[flow].map((s) => <i key={s} className={s === step ? "on" : ""} />)}</div>;
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

// ------------------------------------------------- link: this device waits
type Wait = { k: "starting" } | { k: "waiting"; s: LinkStart; until: number } | { k: "failed"; msg: string } | { k: "expired" };

/** Show the code and its QR, listen for approval, count down 10 minutes. */
function LinkWait({ hub, hubName, name, onDone, onCancel, step, flow }: { hub: string; hubName: string; name: string; onDone: (address: string) => void; onCancel: () => void; step: Step; flow: Flow }) {
  const platform = usePlatform();
  const [st, setSt] = useState<Wait>({ k: "starting" });
  const [attempt, setAttempt] = useState(0);
  const now = useNow(st.k === "waiting");
  const startedFor = useRef(-1);
  const alive = useRef(false);
  const finished = useRef(false);
  const onDoneRef = useRef(onDone);
  onDoneRef.current = onDone;

  // stop listening when this screen goes away (not on StrictMode's re-mount)
  useEffect(() => {
    alive.current = true;
    return () => { alive.current = false; setTimeout(() => { if (!alive.current && !finished.current) void api.linkCancel().catch(() => {}); }, 0); };
  }, []);
  useEffect(() => {
    let un: (() => void) | null = null; let gone = false;
    void api.onLink((e) => {
      if (e.state === "waiting") setSt((s) => (s.k === "waiting" ? { ...s, until: Date.now() + e.expires_in_s * 1000 } : s));
      else if (e.state === "done") { finished.current = true; onDoneRef.current(e.address); }
      else if (e.state === "failed") setSt({ k: "failed", msg: e.error });
      else setSt({ k: "expired" });
    }).then((u) => { if (gone) u(); else un = u; });
    return () => { gone = true; un?.(); };
  }, []);
  useEffect(() => {
    if (startedFor.current === attempt) return;
    startedFor.current = attempt;
    setSt({ k: "starting" });
    api.linkStart(hub, name).then(
      (s) => setSt({ k: "waiting", s, until: Date.now() + 600e3 }),
      (e) => setSt({ k: "failed", msg: errText(e) }),
    );
  }, [attempt, hub, name]);

  const cancel = () => { finished.current = true; void api.linkCancel().catch(() => {}); onCancel(); };
  const again = () => setAttempt((a) => a + 1);
  const device = platform === "android" ? "phone" : "PC";
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
  const [idCheck, setIdCheck] = useState<{ ok: boolean; error: string | null; max_len: number }>({ ok: false, error: null, max_len: 24 });
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState<string | null>(null);
  const [address, setAddress] = useState("");
  const [added, setAdded] = useState<{ url: string; name: string }[]>([]);
  const [more, setMore] = useState(false);
  const [words, setWords] = useState<string[]>([]);
  const [restore, setRestore] = useState("");
  const [linkHub, setLinkHub] = useState<{ url: string; name: string } | null>(null);
  const [devName, setDevName] = useState(platform === "android" ? "Android phone" : "Windows PC");
  const [keyFile, setKeyFile] = useState<string | null>(null);
  const [pass, setPass] = useState("");

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

  if (step === "welcome") {
    return (
      <Frame step={step} flow={flow} actions={[]}>
        <Logo size={64} />
        <h1>Hubchat</h1>
        <p className="lead" style={{ marginBottom: 0 }}>Chat with your agents, and people, over your own Orgtree mail hub.</p>
        <div className="ob-feats">
          <Feat ic="key" t="Your key is your identity" s={"Made on your " + device + "; your address comes from it. No account, and nobody issues it."} />
          <Feat ic="dns" t="Your hub, your network" s="Messages travel through a hub you choose. Nothing goes to a cloud." />
          <Feat ic="bot" t="Agents are first-class" s="Talk to Orgtree orgs and Claude Code sessions the way you talk to people." />
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
        <h2>Choose your id</h2>
        <p className="lead">People and agents reach you by your address. Pick a short id: it becomes part of your address and can't change later.</p>
        <div className="field">
          <label htmlFor="ob-id">Your id</label>
          <label className={"input" + (showErr ? " bad" : "")}>
            <input id="ob-id" value={id} autoFocus placeholder="e.g. alex" maxLength={idCheck.max_len} autoComplete="off" autoCapitalize="off" spellCheck={false}
              onChange={(e) => setId(e.target.value.toLowerCase().replace(/\s+/g, "-"))}
              onKeyDown={(e) => { if (e.key === "Enter" && idCheck.ok && !busy) void create(); }} />
          </label>
          <div className={"help" + (showErr ? " bad" : "")}>{showErr || "Lowercase letters, numbers and “-”, up to " + idCheck.max_len + " characters."}</div>
        </div>
        <div className="field">
          <label htmlFor="ob-name">Display name <span className="dim" style={{ fontWeight: 400 }}>(optional)</span></label>
          <label className="input"><input id="ob-name" value={name} placeholder="e.g. Alex" maxLength={48} autoComplete="off" onChange={(e) => setName(e.target.value)} /></label>
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
          <button className="btn" disabled={!words.length} onClick={() => copyText(words.join(" "), "Recovery words copied")}><Icon name="copy" />Copy words</button>
        </div>
        <NoteCard icon="warning" warn><b>Keep them private.</b> Anyone with these words can read your messages and send as you. Write them down, or keep them in a password manager.</NoteCard>
        {err ? <div className="help bad">{err}</div> : null}
      </Frame>
    );
  }

  if (step === "method") {
    return (
      <Frame step={step} flow={flow} back={() => go("welcome")} wide actions={[]}>
        <h2>Bring your identity to this {device}</h2>
        <p className="lead">Your identity is a key, and your address comes with it: there is no id to choose. How should this {device} get it?</p>
        <div className="methods">
          <Opt cls="method" ic="link" t="Link through a hub" s={"This " + device + " shows a code and your other device approves it. Your key, hubs and profile arrive sealed with that code."} tag={<Tag acc>Recommended</Tag>} onClick={() => pick("link", "linkhub")} />
          <Opt cls="method" ic="qr" t="Scan a QR code" s="Your other device shows your key as a QR code. No network needed." tag={platform === "desktop" ? <Tag>Needs a camera</Tag> : undefined} onClick={() => pick("qr", "scan")} />
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
        <HubAdder existing={[]} autoFocus onPick={(url, n) => { setLinkHub({ url, name: n }); go("linkname"); }} />
      </Frame>
    );
  }

  if (step === "linkname") {
    const ok = !!devName.trim();
    return (
      <Frame step={step} flow={flow} back={() => go("linkhub")} actions={[{ label: "Show the code", primary: true, disabled: !ok, onClick: () => go("linkcode") }]}>
        <h2>Name this {device}</h2>
        <p className="lead">Your other device shows this name when it asks you to approve, so you can tell it's this {device}.</p>
        <div className="field">
          <label htmlFor="ob-dev">Device name</label>
          <label className="input"><Icon name={platform === "android" ? "phone" : "computer"} />
            <input id="ob-dev" value={devName} autoFocus maxLength={48} autoComplete="off" onChange={(e) => setDevName(e.target.value)} onKeyDown={(e) => { if (e.key === "Enter" && ok) go("linkcode"); }} />
          </label>
          <div className="help">Linking through <b>{linkHub?.name}</b> <span className="mono">{linkHub?.url}</span></div>
        </div>
      </Frame>
    );
  }

  if (step === "linkcode" && linkHub) {
    return <LinkWait step={step} flow={flow} hub={linkHub.url} hubName={linkHub.name} name={devName.trim()} onDone={(a) => void arrived(a)} onCancel={() => go("method")} />;
  }

  if (step === "scan") {
    if (platform === "desktop") {
      return (
        <Frame step={step} flow={flow} back={() => go("method")} wide actions={[]}>
          <h2>This PC can't scan a code</h2>
          <p className="lead">Scanning your key needs a camera, and PCs rarely have one that can read a phone's screen. Use one of these instead:</p>
          <div className="methods">
            <Opt cls="method" ic="link" t="Link through a hub" s="This PC shows a code and your phone approves it." tag={<Tag acc>Recommended</Tag>} onClick={() => pick("link", "linkhub")} />
            <Opt cls="method" ic="file" t="Key file" s="Save a key file on your other device, then open it here." onClick={() => pick("file", "keyfile")} />
            <Opt cls="method" ic="key" t="Recovery words" s="Type the 24 words you saved." onClick={() => pick("words", "restore")} />
          </div>
        </Frame>
      );
    }
    const scan = async () => {
      setBusy(true); setErr(null);
      try {
        const t = await scanQr("hubchat-key:MOCK");
        if (!t) { setBusy(false); return; }
        if (/^hubchat-link:/i.test(t.trim())) {
          setErr("That is a link code from a device that is waiting to be linked. It works the other way round: this phone should show a code. Go back and choose “Link through a hub”, or on your other device open Settings › Devices › Link a device › Show my key as a QR code.");
          setBusy(false); return;
        }
        const a = await api.restoreQr(t);
        await arrived(a);
      } catch (e) { setErr(errText(e)); setBusy(false); }
    };
    return (
      <Frame step={step} flow={flow} back={() => go("method")} actions={[{ label: busy ? "Scanning…" : <><Icon name="camera" />Open the camera</>, primary: true, disabled: busy, onClick: scan }]}>
        <h2>Scan your key</h2>
        <p className="lead">On your other device open <b>Settings › Devices › Link a device › Show my key as a QR code</b>, then scan it with this phone.</p>
        <div className="cam-hint"><div className="vf" /></div>
        {err ? <div className="probe-card bad"><Icon name="error" /><div><b>Couldn't use that code</b>{err}</div></div>
          : <div className="help" style={pad(platform)}>The QR code carries your key, your id, your profile and your hub list. Nothing goes over the network.</div>}
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
