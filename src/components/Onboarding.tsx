// First run. The fork: create a new identity (id → address → hubs → recovery
// words) or bring an existing one (recovery words now; linking through a hub,
// QR and key file come in the next build).
import { useEffect, useState, type ReactNode } from "react";
import { api } from "../api";
import { Icon, Logo, type IconName } from "../lib/icons";
import { errText, copyText } from "../lib/native";
import { refreshAll, refreshState, setOnboarding, useSnap } from "../lib/store";
import { splitAddr } from "../lib/peers";
import { HubAdder } from "./HubAdder";
import { NoteCard, usePlatform } from "./ui";

type Step = "welcome" | "id" | "addr" | "hub" | "key" | "method" | "restore";
const FLOW: Record<"new" | "have", Step[]> = { new: ["welcome", "id", "addr", "hub", "key"], have: ["welcome", "method", "restore", "hub"] };

interface Action { label: ReactNode; onClick: () => void; primary?: boolean; disabled?: boolean }

function Frame({ step, path, back, actions, wide, children }: { step: Step; path: "new" | "have"; back?: () => void; actions: Action[]; wide?: boolean; children: ReactNode }) {
  const platform = usePlatform();
  const dots = step === "welcome" ? null : <div className="dots">{FLOW[path].map((s) => <i key={s} className={s === step ? "on" : ""} />)}</div>;
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
function Opt({ cls, ic, t, s, tag, onClick, disabled }: { cls: string; ic: IconName; t: string; s: string; tag?: ReactNode; onClick?: () => void; disabled?: boolean }) {
  return (
    <button className={cls} onClick={onClick} disabled={disabled} aria-disabled={disabled} style={disabled ? { opacity: 0.6, cursor: "default" } : undefined}>
      <span className="fi"><Icon name={ic} /></span>
      <span className="t"><b>{t}{tag}</b><span>{s}</span></span>
      {disabled ? null : <Icon name="chevron_right" className="chev" />}
    </button>
  );
}
const Soon = () => <span className="chip">Coming in the next build</span>;

export function Onboarding() {
  const snap = useSnap();
  const platform = usePlatform();
  const device = platform === "android" ? "phone" : "PC";
  const [step, setStep] = useState<Step>("welcome");
  const [path, setPath] = useState<"new" | "have">("new");
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

  const go = (s: Step) => { setErr(null); setStep(s); };

  if (step === "welcome") {
    return (
      <Frame step={step} path={path} actions={[]}>
        <Logo size={64} />
        <h1>Hubchat</h1>
        <p className="lead" style={{ marginBottom: 0 }}>Chat with your agents, and people, over your own Orgtree mail hub.</p>
        <div className="ob-feats">
          <Feat ic="key" t="Your key is your identity" s={"Made on your " + device + "; your address comes from it. No account, and nobody issues it."} />
          <Feat ic="dns" t="Your hub, your network" s="Messages travel through a hub you choose. Nothing goes to a cloud." />
          <Feat ic="bot" t="Agents are first-class" s="Talk to Orgtree orgs and Claude Code sessions the way you talk to people." />
        </div>
        <div className="fork" style={platform === "android" ? { padding: "0 20px" } : undefined}>
          <Opt cls="fork-opt primary" ic="add" t="Create a new identity" s="Choose an id. Hubchat makes your key and your address." onClick={() => { setPath("new"); go("id"); }} />
          <Opt cls="fork-opt" ic="sync" t="I already use Hubchat" s="Bring your identity from another device or your recovery words." onClick={() => { setPath("have"); go("method"); }} />
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
      <Frame step={step} path={path} back={() => go("welcome")} actions={[{ label: busy ? "Creating…" : "Continue", primary: true, disabled: !idCheck.ok || busy, onClick: create }]}>
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
        <div className="help" style={{ marginTop: 8, padding: platform === "android" ? "0 20px" : undefined }}>The 6-character tag comes from the key Hubchat makes next. It keeps your address unique: two people can both be “{idCheck.ok ? id : "alex"}”.</div>
        {err ? <div className="help bad" style={{ marginTop: 8, padding: platform === "android" ? "0 20px" : undefined }}>{err}</div> : null}
      </Frame>
    );
  }

  if (step === "addr") {
    const [h, t] = splitAddr(address);
    return (
      <Frame step={step} path={path} actions={[{ label: "Continue", primary: true, onClick: () => go("hub") }]}>
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
    const next = () => (path === "new" ? go("key") : void finish());
    const existing = snap.state?.hubs.map((h) => h.url) || [];
    return (
      <Frame step={step} path={path}
        actions={[{ label: "Continue", primary: true, disabled: !added.length, onClick: next }, ...(added.length ? [] : [{ label: "Skip for now", onClick: next }])]}>
        <h2>{path === "have" ? "Add your hubs" : "Add your hub"}</h2>
        <p className="lead">
          {path === "have"
            ? <>Your words brought back <ObAddr a={address || snap.state?.me?.address || ""} />. Hubs don't travel in the words: add the ones you use, and Hubchat signs in to each with your key.</>
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
              <div style={{ margin: "-4px 0 14px", padding: platform === "android" ? "0 20px" : undefined }}><button className="link" onClick={() => setMore(true)}>+ Add another hub</button></div>
              <NoteCard icon="privacy"><b>The hub can read your messages.</b> The hub's operator can read messages that pass through it; everyone on a hub is listed in its directory.</NoteCard>
            </>}
      </Frame>
    );
  }

  if (step === "key") {
    const saved = async () => { try { await api.recoverySaved(); } catch (e) { setErr(errText(e)); return; } await finish(); };
    return (
      <Frame step={step} path={path} wide actions={[{ label: "I've saved them", primary: true, disabled: !words.length, onClick: saved }, { label: "Later", onClick: () => void finish() }]}>
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
      <Frame step={step} path={path} back={() => go("welcome")} wide actions={[]}>
        <h2>Bring your identity to this {device}</h2>
        <p className="lead">Your identity is a key, and your address comes with it: there is no id to choose. How should this {device} get it?</p>
        <div className="methods">
          <Opt cls="method" ic="key" t="Recovery words" s="Type the 24 words you saved when you made your identity." tag={<span className="chip acc">Works now</span>} onClick={() => go("restore")} />
          <Opt cls="method" ic="link" t="Link through a hub" s="This device shows a code and your other device approves it." tag={<Soon />} disabled />
          <Opt cls="method" ic="qr" t="Scan a QR code" s="Your other device shows your key as a QR code. No network needed." tag={<Soon />} disabled />
          <Opt cls="method" ic="file" t="Key file" s="Open a key file saved from Hubchat, with its passphrase." tag={<Soon />} disabled />
        </div>
      </Frame>
    );
  }

  // restore
  const n = restore.toLowerCase().split(/[^a-z]+/).filter(Boolean).length;
  const doRestore = async () => {
    setBusy(true); setErr(null);
    try { const a = await api.restoreWords(restore); setAddress(a); await refreshState(); go("hub"); }
    catch (e) { setErr(errText(e)); }
    finally { setBusy(false); }
  };
  return (
    <Frame step={step} path={path} back={() => go("method")} wide actions={[{ label: busy ? "Checking…" : "Continue", primary: true, disabled: !n || busy, onClick: doRestore }]}>
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
