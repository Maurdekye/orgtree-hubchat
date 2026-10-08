// Settings: Profile, Hubs, Devices, Recovery words, Privacy, Appearance,
// General (desktop), About.
// Desktop: one modal with a nav column. Android: a list and a screen each.
import { useEffect, useState, type ReactNode } from "react";
import { api, type Devices, type HubStatus } from "../api";
import { Icon, Logo, type IconName } from "../lib/icons";
import { bytes } from "../lib/format";
import { appVersion, autostart, copyText, errText } from "../lib/native";
import { hubCls, hubStatusText, hubSummary, hubVersion } from "../lib/peers";
import { refreshDirectory, refreshState, useSnap } from "../lib/store";
import { setThemePref, useThemePref, type ThemePref } from "../lib/theme";
import { toast } from "../lib/toast";
import { checkForUpdate, useUpdate } from "../lib/updates";
import { LinkDeviceModal } from "./LinkDevice";
import { copyWords, downloadWords } from "../lib/recovery";
import { HubAdder } from "./HubAdder";
import { Addr, Avatar, Confirm, NoteCard, QR, Switch, useNow, usePlatform } from "./ui";

export type SetTab = "profile" | "hubs" | "devices" | "recovery" | "privacy" | "appearance" | "general" | "about";
/** Every section (Android titles its screens from this too; General is desktop only). */
export const TABS: [SetTab, string, IconName][] = [["profile", "Profile", "person"], ["hubs", "Hubs", "dns"], ["devices", "Devices", "computer"], ["recovery", "Recovery words", "key"], ["privacy", "Privacy", "privacy"], ["appearance", "Appearance", "palette"], ["general", "General", "tune"], ["about", "About", "info"]];

// ---------------------------------------------------------------- shapes
function Row({ icon, t1, t2, right, onClick }: { icon?: IconName; t1: ReactNode; t2?: ReactNode; right?: ReactNode; onClick?: () => void }) {
  const platform = usePlatform();
  return (
    <div className={platform === "android" ? "li" : "set-row"} onClick={onClick} role={onClick ? "button" : undefined}>
      {icon ? <Icon name={icon} /> : null}
      <div className="t"><div className="t1">{t1}</div>{t2 ? <div className="t2">{t2}</div> : null}</div>
      {right}
    </div>
  );
}
function Sec({ title, children, first }: { title?: ReactNode; children: ReactNode; first?: boolean }) {
  const platform = usePlatform();
  if (platform === "android") return <>{title ? <div className="sec-h">{title}</div> : null}{children}</>;
  return <div className="set-sec" style={first ? { marginTop: 6 } : undefined}>{title ? <h4>{title}</h4> : null}{children}</div>;
}
function Card({ children }: { children: ReactNode }) {
  return usePlatform() === "android" ? <>{children}</> : <div className="set-card">{children}</div>;
}

// --------------------------------------------------------------- sections
function Profile() {
  const snap = useSnap();
  const platform = usePlatform();
  const me = snap.state!.me!;
  const [name, setName] = useState(me.name);
  const [about, setAbout] = useState(me.about);
  const [busy, setBusy] = useState(false);
  const dirty = name.trim() !== me.name || about.trim() !== me.about;
  const save = async () => {
    setBusy(true);
    try { await api.setProfile(name.trim(), about.trim()); await refreshState(); toast("Profile saved"); }
    catch (e) { toast("Couldn't save: " + errText(e)); }
    finally { setBusy(false); }
  };
  return (
    <>
      {platform === "android"
        ? <div className="hero"><Avatar kind="me" name={me.name || me.id} size={96} /></div>
        : <div style={{ display: "flex", gap: 20, alignItems: "center", margin: "8px 0 6px" }}>
            <Avatar kind="me" name={me.name || me.id} size={72} />
            <div><div style={{ font: "600 19px var(--font-display)", color: "var(--ink-strong)" }}>{me.name || me.id}</div><div className="mono dim" style={{ fontSize: 12.5 }}><Addr a={me.address} net /></div></div>
          </div>}
      <Sec>
        <div className="field" style={platform === "android" ? { marginTop: 8 } : undefined}>
          <label htmlFor="pf-name">Display name</label>
          <label className="input"><input id="pf-name" value={name} maxLength={48} onChange={(e) => setName(e.target.value)} /></label>
          <div className="help">Shown to people and agents beside your address. Up to 48 characters.</div>
        </div>
        <div className="field">
          <label htmlFor="pf-about">About</label>
          <label className="input"><input id="pf-about" value={about} maxLength={200} placeholder="One line about you" onChange={(e) => setAbout(e.target.value)} /></label>
          <div className="help">Everyone on your hubs can see this. {about.length}/200</div>
        </div>
        <div className={platform === "android" ? "pad" : ""} style={{ marginBottom: 16 }}>
          <button className={"btn primary" + (platform === "android" ? " block" : "")} disabled={!dirty || busy} onClick={save}>{busy ? "Saving…" : "Save"}</button>
        </div>
        <div className="field">
          <label>Id</label>
          <label className="input" style={{ opacity: 0.75 }}><Icon name="lock" /><input value={me.id} disabled /></label>
          <div className="help">Part of your address; it can't change.</div>
        </div>
      </Sec>
      <Sec title="Your address">
        <Card>
          <Row icon="at" t1={<span className="mono"><Addr a={me.address} net /></span>} t2="Share it so people and agents can reach you. The tag after the dot comes from your key."
            right={<button className={platform === "android" ? "icon-btn" : "btn"} onClick={() => copyText("@net:" + me.address, "Address copied")} aria-label="Copy address"><Icon name="copy" />{platform === "android" ? null : "Copy"}</button>} />
          <div className={platform === "android" ? "qrwrap big" : "set-row"} style={platform === "android" ? undefined : { gap: 18 }}>
            <QR text={"@net:" + me.address} size={platform === "android" ? 200 : 132} />
            <span className="help">Someone can scan this to get your address. It holds your address, nothing else.</span>
          </div>
        </Card>
      </Sec>
    </>
  );
}

function HubRow({ h, now, onRemove }: { h: HubStatus; now: number; onRemove: () => void }) {
  const platform = usePlatform();
  const bad = h.state === "disconnected" || h.state === "refused";
  return (
    <Row icon="dns" t1={h.name}
      t2={<>
        <span className="mono">{h.url}</span><br />
        <span className={"hubst " + hubCls(h)}><span className="dot" />{hubStatusText(h, now)}</span>
        <span className="dim" style={{ fontSize: 12 }}> · hub version {hubVersion(h)} · files up to {bytes(h.max_attachment_bytes)} per message</span>
      </>}
      right={<div style={{ display: "flex", gap: 6, alignItems: "center", flexWrap: "wrap", justifyContent: "flex-end" }}>
        {bad ? <button className="btn" onClick={() => api.retryNow().catch(() => {})}><Icon name="refresh" />{platform === "android" ? "Retry" : "Retry now"}</button> : null}
        <button className="icon-btn" title="Remove hub" aria-label={"Remove hub " + h.name} onClick={onRemove}><Icon name="delete" /></button>
      </div>} />
  );
}

function Hubs() {
  const snap = useSnap();
  const platform = usePlatform();
  const hubs = snap.state!.hubs;
  const now = useNow(hubs.some((h) => h.state === "disconnected"));
  const [adding, setAdding] = useState(false);
  const [rm, setRm] = useState<HubStatus | null>(null);
  const [unreg, setUnreg] = useState(true);
  const [busy, setBusy] = useState(false);
  const remove = async () => {
    if (!rm) return;
    setBusy(true);
    try { await api.removeHub(rm.url, unreg); await refreshState(); await refreshDirectory(); toast("Removed hub " + rm.name); setRm(null); }
    catch (e) { toast("Couldn't remove it: " + errText(e)); }
    finally { setBusy(false); }
  };
  const only = rm ? snap.chats.map((c) => snap.byAddr.get(c.peer)).filter((c) => c && c.hubs.length === 1 && c.hubs[0] === rm.url) : [];
  return (
    <>
      <NoteCard icon="privacy" style={platform === "desktop" ? { marginTop: 6 } : undefined}><b>Hubs see messages in plain text.</b> The hub's operator can read messages that pass through it; everyone on a hub is listed in its directory. Only add hubs run by people you trust.</NoteCard>
      <Sec title="Your hubs">
        <Card>
          {hubs.map((h) => platform === "android" ? <div className="card" key={h.url}><HubRow h={h} now={now} onRemove={() => setRm(h)} /></div> : <HubRow key={h.url} h={h} now={now} onRemove={() => setRm(h)} />)}
          {!hubs.length ? <Row t1="No hubs" t2="Add a hub to send and receive messages." /> : null}
          {adding
            ? <div className={platform === "android" ? "" : "addhub"} style={platform === "android" ? { paddingTop: 12 } : undefined}><HubAdder existing={hubs.map((h) => h.url)} autoFocus onAdded={(_u, n) => { setAdding(false); toast("Added hub " + n); }} onCancel={() => setAdding(false)} /></div>
            : platform === "android"
              ? <div className="pad" style={{ marginTop: 8 }}><button className="btn block" onClick={() => setAdding(true)}><Icon name="add" />Add a hub</button></div>
              : <div className="set-row"><button className="btn" onClick={() => setAdding(true)}><Icon name="add" />Add a hub</button><span className="help">You can be on several hubs at once. Your address is the same on all of them.</span></div>}
        </Card>
      </Sec>
      <Sec title="How sending picks a hub">
        <div className={"help" + (platform === "android" ? " pad" : "")}>When someone is on more than one of your hubs, Hubchat sends through a connected hub that lists them. Their address is the same everywhere.</div>
      </Sec>
      {rm ? (
        <Confirm title={"Remove hub " + rm.name + "?"} okLabel="Remove hub" danger busy={busy} onOk={remove} onCancel={() => setRm(null)}>
          <p>You'll stop sending and receiving through <b>{rm.name}</b>.</p>
          {only.length ? <p>{only.map((c) => c!.org_name || c!.username || c!.address).join(", ")} {only.length > 1 ? "are" : "is"} only reachable through it.</p> : null}
          <label className="checkrow"><input type="checkbox" checked={unreg} onChange={(e) => setUnreg(e.target.checked)} /> Also remove my address from this hub (take me off its member list)</label>
        </Confirm>
      ) : null}
    </>
  );
}

function Recovery() {
  const snap = useSnap();
  const platform = usePlatform();
  const saved = !!snap.state?.recovery_saved;
  const [ask, setAsk] = useState(false);
  const [words, setWords] = useState<string[] | null>(null);
  const show = async () => {
    setAsk(false);
    try { setWords(await api.recoveryWords()); } catch (e) { toast(errText(e)); }
  };
  const markSaved = async () => { try { await api.recoverySaved(); await refreshState(); toast("Marked as saved"); } catch (e) { toast(errText(e)); } };
  const placeholder = Array.from({ length: 24 }, () => "••••••");
  return (
    <>
      <Sec first>
        <div className={"help" + (platform === "android" ? " pad" : "")} style={{ fontSize: 13.5, marginBottom: 12 }}>
          Your key is your identity. These 24 words hold it and your id: on a new device they bring back your address, even if you lose every device. <b>Anyone who has them can read and send as you.</b>
        </div>
        <ol className={"words" + (words ? "" : " blur")}>{(words || placeholder).map((w, i) => <li key={i}>{w}</li>)}</ol>
        <div className={"keyacts" + (platform === "android" ? " pad" : "")} style={{ marginTop: 12 }}>
          {words
            ? <><button className="btn" onClick={() => void copyWords(words)}><Icon name="copy" />Copy words</button>
                <button className="btn" onClick={() => void downloadWords(platform, snap.state?.me?.id || "words")}><Icon name="download" />Download</button></>
            : <button className="btn" onClick={() => setAsk(true)}><Icon name="visibility" />Show words</button>}
          {words ? <button className="btn ghost" onClick={() => setWords(null)}>Hide</button> : null}
          {!saved && words ? <button className="btn primary" onClick={markSaved}>I've saved them</button> : null}
          <span className="saved" style={saved ? undefined : { color: "var(--warn)" }}>{saved ? <><Icon name="check" />Saved</> : "Not saved yet"}</span>
        </div>
      </Sec>
      {ask ? (
        <Confirm title="Show your recovery words?" okLabel="Show them" onOk={show} onCancel={() => setAsk(false)}>
          <p>Anyone who sees these words can become you: read your messages and send as you. Make sure no one is looking at your screen.</p>
        </Confirm>
      ) : null}
    </>
  );
}

function Devices({ onLink, goTab }: { onLink?: () => void; goTab?: (t: SetTab) => void }) {
  const snap = useSnap();
  const platform = usePlatform();
  const me = snap.state!.me!;
  const [linking, setLinking] = useState(false);
  const link = onLink || (() => setLinking(true));
  const self = platform === "android" ? "This phone" : "This PC";
  const pad = platform === "android" ? " pad" : "";
  // Mail hub v2.0 lists every device that syncs as us; older hubs list none.
  const [list, setList] = useState<Devices | null>(null);
  useEffect(() => { api.devices().then(setList, () => setList(null)); }, []);
  const others = (list?.devices || []).filter((d) => d.device_id !== list?.this_device);
  return (
    <>
      <Sec title="Your devices" first>
        <Card>
          <Row icon={platform === "android" ? "phone" : "computer"} t1={<>{self} <span className="chip">this device</span></>} t2={<>Holds your key · <span className="mono"><Addr a={me.address} net /></span></>} />
          {platform === "android"
            ? <div className="pad" style={{ marginTop: 6 }}><button className="btn block" onClick={link}><Icon name="link" />Link a device</button></div>
            : <div className="set-row"><button className="btn" onClick={link}><Icon name="link" />Link a device</button><span className="help">Bring your identity to a new phone or PC: approve its code, show your key as a QR code, or save a key file.</span></div>}
        </Card>
        {others.length ? (
          <Card>
            {others.map((d) => (
              <Row key={d.device_id} icon={/phone|android/i.test(d.name || "") ? "phone" : "computer"} t1={d.name || d.device_id}
                t2={d.online ? "Online now" : d.last_seen ? "Last seen " + new Date(d.last_seen).toLocaleString() : "Not seen yet"} />
            ))}
          </Card>
        ) : null}
      </Sec>
      <Sec>
        <div className={"help" + pad}>All your devices are equal: each holds your one key and is <span className="mono"><Addr a={me.address} net /></span>. {list && list.devices.length ? "Your other devices are listed by hubs that run mail hub v2.0." : "Older hubs can't tell devices with the same key apart, so only this one is listed."}</div>
        <div className={"help" + pad} style={{ marginTop: 8 }}>Signing out one device comes with mail hub v2.0.</div>
      </Sec>
      {linking ? <LinkDeviceModal onClose={() => setLinking(false)} onRecovery={goTab ? () => { setLinking(false); goTab("recovery"); } : undefined} /> : null}
    </>
  );
}

function General() {
  const [on, setOn] = useState<boolean | null>(null);
  useEffect(() => { autostart.get().then(setOn, () => setOn(false)); }, []);
  const set = async (v: boolean) => {
    setOn(v);
    try { await autostart.set(v); } catch (e) { toast(errText(e)); setOn(!v); }
  };
  return (
    <Sec first>
      <Card>
        <Row icon="play" t1="Start with Windows" t2="Hubchat starts hidden in the tray when you sign in to Windows, so messages arrive before you open it." right={on == null ? null : <Switch on={on} onChange={set} label="Start with Windows" />} />
        <Row icon="minimize" t1="Closing the window keeps Hubchat running in the tray." t2="To quit, right-click the Hubchat icon in the tray and choose Quit." />
      </Card>
    </Sec>
  );
}

function Privacy() {
  const snap = useSnap();
  const on = !!snap.state?.read_receipts;
  const set = async (v: boolean) => { try { await api.setReadReceipts(v); await refreshState(); } catch (e) { toast(errText(e)); } };
  return (
    <Sec first>
      <Card>
        <Row icon="done_all" t1="Send read receipts" t2="Off: people and agents see your messages as delivered, never read. You still see theirs." right={<Switch on={on} onChange={set} label="Send read receipts" />} />
        <Row icon="contacts" t1="Directory" t2="Everyone on your hubs finds you in their directory: your name, address and about line. Being on a hub means being listed." />
        <Row icon="person" t1="Online status" t2="The hub shows you as online while Hubchat is connected, to everyone on that hub. This can't be hidden." />
        <Row icon="privacy" t1="Who can message you" t2="Anyone on your hubs who has your address." />
      </Card>
    </Sec>
  );
}

function Appearance() {
  const platform = usePlatform();
  const pref = useThemePref();
  const opts: [ThemePref, string, IconName, string][] = [["dark", "Dark", "dark_mode", "The default, like Orgtree"], ["light", "Light", "light_mode", ""], ["system", platform === "android" ? "Follow system" : "Match system", "contrast", "Follows your system's dark theme setting"]];
  if (platform === "android") {
    return (
      <Sec title="Theme">
        {opts.map(([v, l, , d]) => <div key={v} className="li" onClick={() => setThemePref(v)} role="radio" aria-checked={pref === v}><span className={"radio" + (pref === v ? " on" : "")} /><div className="t"><div className="t1">{l}</div>{d ? <div className="t2">{d}</div> : null}</div></div>)}
      </Sec>
    );
  }
  const mini = (t: string) => <span className={"mini " + t}><i className="ms" /><i className="mm"><i className="b1" /><i className="b2" /><i className="b3" /></i></span>;
  return (
    <Sec title="Theme" first>
      <div className="radio-cards">
        {opts.map(([v, l, ic]) => (
          <button key={v} className={"rcard" + (pref === v ? " on" : "")} onClick={() => setThemePref(v)} aria-pressed={pref === v}>
            <div className="pv">{v === "system" ? <>{mini("mt-dark")}{mini("mt-light")}</> : mini(v === "dark" ? "mt-dark" : "mt-light")}</div>
            <div className="t1"><Icon name={ic} />{l}</div>
          </button>
        ))}
      </div>
      <div className="help" style={{ marginTop: 10 }}>Dark is the default, like Orgtree. The accent is Orgtree's terracotta.</div>
    </Sec>
  );
}

function UpdateCheck() {
  const avail = useUpdate();
  const [st, setSt] = useState<{ k: "idle" } | { k: "checking" } | { k: "none" } | { k: "error"; msg: string } | { k: "installing" }>({ k: "idle" });
  const check = async () => {
    setSt({ k: "checking" });
    try { setSt((await checkForUpdate()) ? { k: "idle" } : { k: "none" }); } catch (e) { setSt({ k: "error", msg: errText(e) }); }
  };
  const install = async () => {
    if (!avail) return;
    setSt({ k: "installing" });
    try { await avail.install(); } catch (e) { setSt({ k: "error", msg: errText(e) }); }
  };
  const text = st.k === "checking" ? "Checking…" : st.k === "installing" ? "Downloading the update…" : avail ? "Hubchat " + avail.version + " is available."
    : st.k === "none" ? "You have the latest version." : st.k === "error" ? "Couldn't check for updates: " + st.msg : "Hubchat checks for updates when it starts and every 6 hours.";
  return (
    <div className="about-upd">
      <span className={"help" + (st.k === "error" ? " bad" : "")}>{text}</span>
      {avail
        ? <button className="btn primary" onClick={install} disabled={st.k === "installing"}><Icon name="restart" />Restart to update</button>
        : <button className="btn" onClick={check} disabled={st.k === "checking"}><Icon name="refresh" />Check for updates</button>}
    </div>
  );
}

function useVersion(): string {
  const [v, setV] = useState("");
  useEffect(() => { appVersion().then(setV, () => setV("")); }, []);
  return v;
}

function About() {
  const platform = usePlatform();
  const version = useVersion();
  const ver = version ? "Version " + version : "\u00a0";
  const text = "Hubchat is a chat client for the Orgtree mail hub. It talks to Orgtree orgs, Claude Code sessions and people by address. There is no account and no cloud: your identity lives on your devices and your messages travel through hubs you choose.";
  if (platform === "android") return <><div className="hero"><Logo size={72} /><div className="name">Hubchat</div><div className="dim">{ver}</div></div><div className="pad help" style={{ fontSize: 14, lineHeight: 1.55 }}>{text}</div></>;
  return (
    <>
      <div style={{ display: "flex", gap: 16, alignItems: "center", margin: "10px 0 18px" }}><Logo size={56} /><div><div style={{ font: "600 20px var(--font-display)", color: "var(--ink-strong)" }}>Hubchat</div><div className="dim">{ver}</div></div></div>
      <div className="help" style={{ fontSize: 13.5, lineHeight: 1.55 }}>{text}</div>
      <UpdateCheck />
    </>
  );
}

export function SettingsSection({ tab, onLink, goTab }: { tab: SetTab; onLink?: () => void; goTab?: (t: SetTab) => void }) {
  switch (tab) {
    case "profile": return <Profile />;
    case "hubs": return <Hubs />;
    case "devices": return <Devices onLink={onLink} goTab={goTab} />;
    case "general": return <General />;
    case "recovery": return <Recovery />;
    case "privacy": return <Privacy />;
    case "appearance": return <Appearance />;
    default: return <About />;
  }
}

/** Desktop: the settings modal. */
export function SettingsModal({ tab, setTab, onClose }: { tab: SetTab; setTab: (t: SetTab) => void; onClose: () => void }) {
  const snap = useSnap();
  const hubBad = snap.state?.hubs.some((h) => h.state === "disconnected" || h.state === "refused");
  return (
    <div className="scrim" onMouseDown={(e) => { if (e.target === e.currentTarget) onClose(); }}>
      <div className="modal settings" role="dialog" aria-modal="true">
        <nav className="set-nav">
          <h3>Settings</h3>
          {TABS.map(([k, label, ic]) => (
            <button key={k} className={tab === k ? "on" : ""} onClick={() => setTab(k)}>
              <Icon name={ic} />{label}
              {k === "recovery" && !snap.state?.recovery_saved ? <span className="warnmark" title="Recovery words not saved" /> : null}
              {k === "hubs" && hubBad ? <span className="warnmark" title="A hub is unreachable" /> : null}
            </button>
          ))}
        </nav>
        <div className="set-main">
          <div className="modal-h"><h3>{TABS.find((t) => t[0] === tab)![1]}</h3><button className="icon-btn" onClick={onClose} title="Close (Esc)" aria-label="Close"><Icon name="close" /></button></div>
          <div className="set-body scroll" key={tab}><SettingsSection tab={tab} goTab={setTab} /></div>
        </div>
      </div>
    </div>
  );
}

/** Android: the settings list. */
export function SettingsList({ onOpen, onBack }: { onOpen: (t: SetTab) => void; onBack: () => void }) {
  const snap = useSnap();
  const me = snap.state!.me!;
  const pref = useThemePref();
  const version = useVersion();
  const hs = hubSummary(snap.state!.hubs);
  const chev = <Icon name="chevron_right" className="chev" />;
  const warn = <span className="warnmark" />;
  return (
    <div className="scr">
      <div className="appbar flat"><button className="icon-btn" onClick={onBack} aria-label="Back"><Icon name="back" /></button><div className="title">Settings</div></div>
      <div className="scr-body">
        <div className="profile-card" onClick={() => onOpen("profile")} role="button">
          <Avatar kind="me" name={me.name || me.id} size={56} />
          <div className="t"><div className="n">{me.name || me.id}</div><div className="a"><Addr a={me.address} net /></div></div>
          <button className="icon-btn" onClick={(e) => { e.stopPropagation(); void copyText("@net:" + me.address, "Address copied"); }} aria-label="Copy address"><Icon name="copy" /></button>
        </div>
        <Row icon="dns" t1="Hubs" t2={hs.text} right={<>{snap.state!.hubs.some((h) => h.state !== "connected") ? warn : null}{chev}</>} onClick={() => onOpen("hubs")} />
        <Row icon="phone" t1="Devices" t2="Link a device · key file" right={chev} onClick={() => onOpen("devices")} />
        <Row icon="key" t1="Recovery words" t2={snap.state!.recovery_saved ? "Saved" : <span style={{ color: "var(--warn)" }}>Not saved yet</span>} right={<>{snap.state!.recovery_saved ? null : warn}{chev}</>} onClick={() => onOpen("recovery")} />
        <Row icon="privacy" t1="Privacy" t2="Read receipts · who can reach you" right={chev} onClick={() => onOpen("privacy")} />
        <Row icon="palette" t1="Appearance" t2={pref === "system" ? "Follow system" : pref === "light" ? "Light" : "Dark"} right={chev} onClick={() => onOpen("appearance")} />
        <Row icon="info" t1="About" t2={version ? "Version " + version : undefined} right={chev} onClick={() => onOpen("about")} />
      </div>
    </div>
  );
}
