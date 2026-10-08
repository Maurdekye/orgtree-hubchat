// Add a hub: type an address, Check it (GET /healthz through the core), then
// add it. A hub may be added while unreachable; the core keeps retrying.
// Used by onboarding and by Settings › Hubs.
import { useState } from "react";
import { api, type Probe } from "../api";
import { Icon } from "../lib/icons";
import { bytes } from "../lib/format";
import { errText } from "../lib/native";
import { refreshState } from "../lib/store";
import { NoteCard, usePlatform } from "./ui";

type Phase = { k: "idle" } | { k: "checking"; input: string } | { k: "result"; p: Probe } | { k: "adding" } | { k: "error"; msg: string };

export function HubAdder({ onAdded, existing, autoFocus, onCancel }: { onAdded?: (url: string, name: string) => void; existing: string[]; autoFocus?: boolean; onCancel?: () => void }) {
  const platform = usePlatform();
  const [value, setValue] = useState("");
  const [ph, setPh] = useState<Phase>({ k: "idle" });

  const check = async () => {
    const input = value.trim(); if (!input) return;
    setPh({ k: "checking", input });
    try { setPh({ k: "result", p: await api.probeHub(input) }); } catch (e) { setPh({ k: "error", msg: errText(e) }); }
  };
  const add = async (name: string) => {
    const input = value.trim();
    setPh({ k: "adding" });
    try {
      const url = await api.addHub(input);
      await refreshState();
      setValue(""); setPh({ k: "idle" });
      onAdded?.(url, name);
    } catch (e) { setPh({ k: "error", msg: errText(e) }); }
  };

  let card = null;
  if (ph.k === "checking") card = <div className="probe-card busy"><span className="spin" /><div>Reaching <span className="mono">{ph.input}</span>…</div></div>;
  if (ph.k === "adding") card = <div className="probe-card busy"><span className="spin" /><div>Adding the hub…</div></div>;
  if (ph.k === "error") card = <div className="probe-card bad"><Icon name="error" /><div><b>Couldn't add that hub</b>{ph.msg}</div></div>;
  if (ph.k === "result") {
    const p = ph.p;
    const dupe = p.result !== "invalid" && existing.includes(p.url);
    if (dupe) card = <div className="probe-card bad"><Icon name="error" /><div><b>Already added</b>You already use <span className="mono">{p.url}</span>.</div></div>;
    else if (p.result === "connected") {
      card = (
        <div className="probe-card ok"><Icon name="check_circle" />
          <div><b>Connected to {p.name}</b><span className="mono">{p.url}</span> · files up to {bytes(p.max_attachment_bytes)} per message
            <div className="acts"><button className="btn primary" onClick={() => add(p.name)}><Icon name="add" />Add {p.name}</button></div>
          </div>
        </div>
      );
    } else if (p.result === "unreachable") {
      card = (
        <div className="probe-card bad"><Icon name="error" />
          <div><b>Can't reach that hub</b>{p.error}. Check the address, that the hub is running, and that this device is on its network.
            <div className="acts"><button className="btn" onClick={check}><Icon name="refresh" />Try again</button><button className="btn ghost" onClick={() => add(p.url.replace(/^https?:\/\//, ""))}>Add anyway and keep trying</button></div>
          </div>
        </div>
      );
    } else if (p.result === "not_a_hub") {
      card = <div className="probe-card bad"><Icon name="error" /><div><b>Not an Orgtree mail hub</b>Something answered at <span className="mono">{p.url}</span>, but it isn't a hub: {p.error}</div></div>;
    } else {
      card = <div className="probe-card bad"><Icon name="error" /><div><b>Check the address</b>{p.error}</div></div>;
    }
  }

  const busy = ph.k === "checking" || ph.k === "adding";
  const checkBtn = <button className={"btn" + (platform === "android" ? " block" : "")} onClick={check} disabled={!value.trim() || busy}><Icon name="link" />Check</button>;
  return (
    <>
      <div className="field">
        <label htmlFor="hub-in">Hub address</label>
        <div style={{ display: "flex", gap: 8 }}>
          <label className="input" style={{ flex: 1 }}>
            <Icon name="dns" />
            <input id="hub-in" value={value} autoFocus={autoFocus} placeholder="hub.office.lan or 10.0.0.5:7370" autoComplete="off" spellCheck={false} autoCapitalize="off" inputMode="url"
              onChange={(e) => { setValue(e.target.value); if (ph.k !== "checking" && ph.k !== "adding") setPh({ k: "idle" }); }}
              onKeyDown={(e) => { if (e.key === "Enter") { e.preventDefault(); void check(); } }} />
          </label>
          {platform === "desktop" ? checkBtn : null}
          {platform === "desktop" && onCancel ? <button className="btn ghost" onClick={onCancel}>Cancel</button> : null}
        </div>
        <div className="help">No <span className="mono">http://</span>? Hubchat adds it. No port? It uses 7370.</div>
      </div>
      {card}
      {platform === "android" ? <div className="pad" style={{ marginBottom: 8 }}>{checkBtn}</div> : null}
      <NoteCard icon="privacy"><b>The hub can read your messages.</b> The hub's operator can read messages that pass through it; everyone on a hub is listed in its directory. Only use hubs run by people you trust.</NoteCard>
    </>
  );
}
