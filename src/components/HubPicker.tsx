// The hub picker (user 2026-10-09 08:41Z): when someone is on two or more of
// my hubs, choose which one my messages to them go through. Automatic is the
// core's own stable choice (online first, then the hub the chat last used);
// a chosen hub is pinned for the chat on this device. Desktop: the header's
// "via" chip opens a popover; Android: "Send through…" in the ⋮ menu opens
// a bottom sheet with the same choices.
import { useEffect, useRef, useState } from "react";
import { api, type HubStatus, type SendRoute } from "../api";
import { Icon } from "../lib/icons";
import { errText } from "../lib/native";
import { hubByUrl, hubName } from "../lib/peers";
import { useSnap } from "../lib/store";
import { toast } from "../lib/toast";

/** The hub a chat's messages go through now, as the header names it: the
 *  next hub, else the pinned one (waiting), else Automatic's. */
export const routeVia = (r: SendRoute | null) => r && (r.next ?? r.pinned ?? r.automatic);

/** "via office" or "via office · pinned". */
export function routeLabel(r: SendRoute | null, hubs: HubStatus[]): string {
  const via = routeVia(r);
  return via ? "via " + hubName(hubs, via) + (r?.pinned ? " · pinned" : "") : "";
}

/** There is a choice to make: two or more of my hubs list them. */
export const hasChoice = (r: SendRoute | null) => !!r && r.hubs.length >= 2;

type HubAt = "online" | "offline" | "down";

/** Automatic and each hub, with their state there (shared by both layouts). */
function Choices({ peer, route, who, onDone }: { peer: string; route: SendRoute; who: string; onDone: () => void }) {
  const hubs = useSnap().state?.hubs || [];
  const pick = (hub: string | null) => {
    onDone();
    if (hub !== route.pinned) api.setSendHub(peer, hub).catch((e) => toast(errText(e)));
  };
  const row = (sel: boolean, label: string, sub: string, run: () => void, dot?: HubAt) => (
    <button key={label} type="button" className={"mi hubopt" + (sel ? " sel" : "")} role="menuitemradio" aria-checked={sel} onClick={run}>
      <span className="ck">{sel ? <Icon name="check" /> : null}</span>
      {dot ? <span className="hic"><span className={"hdot " + dot} /></span> : <Icon name="sync" />}
      <span className="t"><b>{label}</b><small>{sub}</small></span>
    </button>
  );
  return (
    <>
      {row(!route.pinned, "Automatic",
        route.automatic ? "Now: " + hubName(hubs, route.automatic) : "None of your hubs can reach " + who + " right now.",
        () => pick(null))}
      {route.hubs.map(({ url, online }) => {
        const at: HubAt = hubByUrl(hubs, url)?.state !== "connected" ? "down" : online ? "online" : "offline";
        return row(route.pinned === url, hubName(hubs, url),
          at === "down" ? "Can't reach this hub" : who + (online ? " is online here" : " is offline here"),
          () => pick(url), at);
      })}
    </>
  );
}

/** Desktop: the header chip; a button with the choices when there are any. */
export function HubChip({ peer, route, who }: { peer: string; route: SendRoute | null; who: string }) {
  const hubs = useSnap().state?.hubs || [];
  const [open, setOpen] = useState(false);
  const btn = useRef<HTMLButtonElement>(null);
  const pop = useRef<HTMLDivElement>(null);
  const [pos, setPos] = useState({ x: 0, y: 0 });
  const via = routeVia(route);

  useEffect(() => {
    if (!open) return;
    const r = btn.current?.getBoundingClientRect();
    if (r) setPos({ x: Math.max(8, Math.min(r.left, window.innerWidth - 340)), y: r.bottom + 6 });
    const down = (e: MouseEvent) => {
      if (pop.current?.contains(e.target as Node) || btn.current?.contains(e.target as Node)) return;
      setOpen(false);
    };
    const key = (e: KeyboardEvent) => { if (e.key === "Escape") { e.stopPropagation(); setOpen(false); } };
    window.addEventListener("mousedown", down);
    window.addEventListener("keydown", key, true);
    return () => { window.removeEventListener("mousedown", down); window.removeEventListener("keydown", key, true); };
  }, [open]);
  useEffect(() => { if (!hasChoice(route)) setOpen(false); }, [route]);

  if (!via) return null;
  if (!hasChoice(route)) return <span className="chip hubchip" title={"Messages go through hub " + hubName(hubs, via)}>{routeLabel(route, hubs)}</span>;
  return (
    <>
      <button ref={btn} type="button" className={"chip hubchip pick" + (open ? " on" : "") + (route!.pinned ? " pinned" : "")}
        title="Choose the hub your messages go through" aria-haspopup="menu" aria-expanded={open}
        onClick={(e) => { e.stopPropagation(); setOpen((o) => !o); }}>
        {routeLabel(route, hubs)}<Icon name="expand_more" />
      </button>
      {open ? (
        <div className="pop hubpop" ref={pop} role="menu" aria-label="Send through" style={{ left: pos.x, top: pos.y }} onClick={(e) => e.stopPropagation()}>
          <div className="pop-h">Send through</div>
          <Choices peer={peer} route={route!} who={who} onDone={() => setOpen(false)} />
        </div>
      ) : null}
    </>
  );
}

/** Android: the bottom sheet "Send through…" opens. */
export function HubSheet({ peer, route, who, onClose }: { peer: string; route: SendRoute; who: string; onClose: () => void }) {
  return (
    <>
      <div className="sheet-scrim" onClick={onClose} />
      <div className="sheet" role="menu" aria-label="Send through">
        <div className="grip" />
        <h4>Send through</h4>
        <Choices peer={peer} route={route} who={who} onDone={onClose} />
      </div>
    </>
  );
}
