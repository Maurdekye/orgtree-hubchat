// Calm app-level banners: a hub that can't be reached (with its retry
// countdown, Retry now and Help on running and reaching a hub; on Android,
// when Tailscale is off and the hub needs it, that instead), the
// recovery-words reminder, and an available update (desktop installs it on
// restart; Android walks through its own installer).
import { useState } from "react";
import { api } from "../api";
import { Icon } from "../lib/icons";
import { errText } from "../lib/native";
import { useSnap } from "../lib/store";
import { openTailscale, useTailscaleOff } from "../lib/tailnet";
import { toast } from "../lib/toast";
import { ALLOW_TEXT, allowInstalls, useInstallStep, useUpdate, type Available } from "../lib/updates";
import { HubHelpLink } from "./HubHelp";
import { useNow, usePlatform } from "./ui";

export function useHubProblem() {
  const snap = useSnap();
  const hubs = snap.state?.hubs || [];
  const bad = hubs.find((h) => h.state === "refused") || hubs.find((h) => h.state === "disconnected");
  const now = useNow(!!bad);
  if (!bad) return null;
  const secs = bad.retry_at_ms ? Math.max(0, Math.ceil((bad.retry_at_ms - now) / 1000)) : null;
  const more = hubs.filter((h) => h !== bad && (h.state === "disconnected" || h.state === "refused")).length;
  return { hub: bad, refused: bad.state === "refused", secs, more };
}

export function HubBanner() {
  const p = useHubProblem();
  const platform = usePlatform();
  const tsOff = useTailscaleOff();
  if (!p) return null;
  const retry = () => api.retryNow().catch(() => {});
  // the likely cause, and what fixes it (user 2026-10-10 06:44Z: Tailscale
  // turned itself off). No Retry: with Tailscale back on, this turns into the
  // usual notice, Retry and all; it goes once the hub is back.
  if (platform === "android" && tsOff && !p.refused) {
    return (
      <div className="strip warn" role="region" aria-label="Hub notice" data-why="tailscale"><Icon name="warning" />
        <span><b>Tailscale seems to be off.</b> Your hub is only reachable through it.</span>
        <button className="link nowrap" onClick={() => void openTailscale()}>Open Tailscale</button>
      </div>
    );
  }
  const text = p.refused
    ? <><b>Hub {p.hub.name} refused the connection</b>{p.hub.error ? ": " + p.hub.error : ""}</>
    : <><b>Can't reach hub {p.hub.name}</b>{p.secs != null ? " · retrying in " + p.secs + " s" : ""}{p.more ? " · " + p.more + " more hub" + (p.more > 1 ? "s" : "") + " down" : ""}</>;
  if (platform === "android") {
    return <div className={"strip warn" + (p.refused ? " bad" : "")} role="region" aria-label="Hub notice"><Icon name={p.refused ? "error" : "warning"} /><span>{text}</span>{p.refused ? null : <HubHelpLink />}<button className="link" onClick={retry}>Retry</button></div>;
  }
  return (
    <div className={"banner warn" + (p.refused ? " bad" : "")} role="region" aria-label="Hub notice">
      <Icon name={p.refused ? "error" : "warning"} /><span>{text}</span>
      {p.refused ? null : <HubHelpLink className="btn ghost" icon />}
      <button className="btn" onClick={retry}><Icon name="refresh" />Retry now</button>
    </div>
  );
}

const KEY = "hubchat.recoveryBannerDismissed";
export function RecoveryBanner({ onShow }: { onShow: () => void }) {
  const snap = useSnap();
  const platform = usePlatform();
  const [gone, setGone] = useState(() => sessionStorage.getItem(KEY) === "1");
  if (gone || !snap.state?.me || snap.state.recovery_saved) return null;
  const dismiss = () => { sessionStorage.setItem(KEY, "1"); setGone(true); };
  if (platform === "android") {
    return (
      <div className="strip" role="region" aria-label="Recovery notice"><Icon name="key" /><span><b>Save your recovery words.</b> They are the only way back to your address.</span>
        <button className="link" onClick={onShow}>Show</button>
        <button className="icon-btn" style={{ width: 32, height: 32 }} onClick={dismiss} aria-label="Dismiss"><Icon name="close" /></button>
      </div>
    );
  }
  return (
    <div className="banner" role="region" aria-label="Recovery notice">
      <Icon name="key" />
      <span><b>Save your recovery words.</b> If you lose this device, they are the only way to get your address back.</span>
      <button className="btn" onClick={onShow}>Show them</button>
      <button className="icon-btn" style={{ width: 28, height: 28 }} onClick={dismiss} title="Dismiss until next start" aria-label="Dismiss"><Icon name="close" /></button>
    </div>
  );
}

/** An update is ready to install. Dismissed until the next check. */
export function UpdateBanner() {
  const avail = useUpdate();
  const platform = usePlatform();
  const [busy, setBusy] = useState(false);
  const [hidden, setHidden] = useState<string | null>(null);
  if (!avail || hidden === avail.version) return null;
  if (platform === "android") return <AndroidUpdate avail={avail} onHide={() => setHidden(avail.version)} />;
  const go = async () => {
    setBusy(true);
    try { await avail.install(); } catch (e) { toast("Couldn't update: " + errText(e)); setBusy(false); }
  };
  return (
    <div className="banner upd" role="region" aria-label="Update">
      <Icon name="restart" />
      <span><b>Hubchat {avail.version} is available.</b>{busy ? " Downloading…" : " It installs when Hubchat restarts."}</span>
      <button className="btn primary" onClick={go} disabled={busy}>{busy ? "Updating…" : "Restart to update"}</button>
      <button className="icon-btn" style={{ width: 28, height: 28 }} onClick={() => setHidden(avail.version)} title="Not now" aria-label="Dismiss"><Icon name="close" /></button>
    </div>
  );
}

/** Android (user 2026-10-09 08:29Z): Update downloads the release's APK and
 *  hands it to Android's installer. The first time, Android must allow
 *  Hubchat to install apps; back from that setting, the update carries on
 *  (lib/updates.ts, for Settings › About too). */
function AndroidUpdate({ avail, onHide }: { avail: Available; onHide: () => void }) {
  const step = useInstallStep();
  const go = () => void avail.install();
  let text: React.ReactNode;
  let action: React.ReactNode = null;
  switch (step.k) {
    case "permission":
      text = <><b>{ALLOW_TEXT[0]}</b> {ALLOW_TEXT[1]}</>;
      action = <button className="btn primary" onClick={allowInstalls}>Allow</button>;
      break;
    case "downloading":
      text = <><b>Downloading Hubchat {avail.version}…</b> {step.pct}%</>;
      break;
    case "installing":
    case "confirm":
      text = <><b>Installing Hubchat {avail.version}.</b> Android may ask you to confirm.</>;
      break;
    case "failed":
      text = <><b>The update didn't install.</b> {step.msg}</>;
      action = <button className="btn" onClick={go}>Try again</button>;
      break;
    default:
      text = <b>Hubchat {avail.version} is ready.</b>;
      action = <button className="btn primary" onClick={go}>Update</button>;
  }
  const busy = step.k === "downloading" || step.k === "installing" || step.k === "confirm";
  return (
    <div className="banner upd" role="status">
      <Icon name="restart" />
      <span>{text}</span>
      {action}
      {busy ? null : <button className="icon-btn" style={{ width: 28, height: 28 }} onClick={onHide} title="Not now" aria-label="Dismiss"><Icon name="close" /></button>}
    </div>
  );
}
