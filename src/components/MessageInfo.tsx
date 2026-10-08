// Message info: the receipt ladder with times (outgoing) or what arrived when.
import type { Message } from "../api";
import { Icon } from "../lib/icons";
import { dayLabel, ms, timeSec } from "../lib/format";
import { displayName, hubName, isAgent } from "../lib/peers";
import { useSnap } from "../lib/store";
import { usePlatform } from "./ui";

export function MessageInfoBody({ m }: { m: Message }) {
  const snap = useSnap();
  const platform = usePlatform();
  const hubs = snap.state?.hubs || [];
  const c = snap.byAddr.get(m.peer);
  const who = displayName(c, m.peer);
  const agent = isAgent(c);
  const hub = hubName(hubs, m.hub);
  const when = (s: string | null) => { const t = ms(s); return t ? timeSec(t) + " · " + dayLabel(t) : null; };
  const H = ({ t }: { t: string }) => (platform === "android" ? <div className="sec-h">{t}</div> : <h4>{t}</h4>);

  if (!m.outgoing) {
    const kv = (k: string, v: string) => <div className="kv" key={k}><span className="k">{k}</span><span className="v">{v}</span></div>;
    return (
      <div className={platform === "android" ? "pad" : "info-sec"}>
        <H t="Received" />
        {kv("Sent", (when(m.sent_at) || "—") + " (their clock)")}
        {kv("Hub has it", when(m.received_at) || "—")}
        {kv("Hub", hub || "—")}
        {kv("Kind", m.kind || "message")}
      </div>
    );
  }
  const steps = [
    { label: "Written on this device", at: m.created_at, note: "Your clock" },
    { label: "Sent: hub " + (hub || "") + " has it", at: m.received_at || (m.state === "sent" ? m.sent_at : null), note: "Hub clock: orders the conversation" },
    { label: "Fetched by " + who, at: m.fetched_at, note: agent ? "Its Orgtree pulled it from the hub" : "One of their devices pulled it" },
    { label: "Delivered", at: m.delivered_at, note: agent ? "In its org inbox" : "On their device" },
    { label: "Read", at: m.read_at, note: agent ? "An agent or a person opened it" : "They opened the chat" },
  ];
  return (
    <div className={platform === "android" ? "" : "info-sec"}>
      <H t="Delivery" />
      {m.state === "failed" ? <div className="failed-line" style={{ margin: platform === "android" ? "0 20px 12px" : "0 0 12px" }}><Icon name="error" /><span>Not sent{m.error ? ": " + m.error : ""}</span></div> : null}
      <ul className="ladder">
        {steps.map((s) => {
          const w = when(s.at);
          return (
            <li key={s.label} className={w ? "done" : ""}>
              <span className="lk">{w ? <Icon name="check" /> : null}</span>
              <div className="ll">{s.label}</div>
              {w ? <div className="lt">{w}</div> : <div className="lt dim">—</div>}
              <div className="ln">{s.note}</div>
            </li>
          );
        })}
      </ul>
      <div className={"help" + (platform === "android" ? " pad" : "")} style={{ fontSize: 12 }}>
        Ticks: <Icon name="schedule" style={{ display: "inline", width: 13, height: 13, verticalAlign: -2 }} /> waiting · <Icon name="check" style={{ display: "inline", width: 14, height: 14, verticalAlign: -3 }} /> sent · <Icon name="done_all" style={{ display: "inline", width: 14, height: 14, verticalAlign: -3 }} /> delivered · <span style={{ color: "var(--ok)" }}><Icon name="done_all" style={{ display: "inline", width: 14, height: 14, verticalAlign: -3 }} /> read</span>. Delivered covers both “fetched” and “delivered”; this list shows each step.
      </div>
    </div>
  );
}
