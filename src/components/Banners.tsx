// Calm app-level banners: a hub that can't be reached (with its retry
// countdown, Retry now and Help on running and reaching a hub), the
// recovery-words reminder, and (desktop) an available update.
import { useState } from "react";
import { api } from "../api";
import { Icon } from "../lib/icons";
import { errText } from "../lib/native";
import { useSnap } from "../lib/store";
import { toast } from "../lib/toast";
import { useUpdate } from "../lib/updates";
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
  if (!p) return null;
  const retry = () => api.retryNow().catch(() => {});
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

/** Desktop: an update is ready to install. Dismissed until the next check. */
export function UpdateBanner() {
  const avail = useUpdate();
  const [busy, setBusy] = useState(false);
  const [hidden, setHidden] = useState<string | null>(null);
  if (!avail || hidden === avail.version) return null;
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
