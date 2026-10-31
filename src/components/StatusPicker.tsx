// Your status (Mail hub 2.3.0 feature `status`; user 2026-10-31 08:52Z: "users on hubchat should be able to manually
// set their status (do not disturb prevents notifications from appearing on their desktop / phone)"; Orgtree docket
// org-presence-busy-orange-when-no-org-inbox-holde). Your own avatar wears the dot others see and opens the picker:
// desktop, at the head of the chat list (and in its rail), as a popover; Android, in the Chats screen's app bar, as a
// bottom sheet. The rows are the hub picker's: Automatic, Online, Busy and Do not disturb, each with what it does and
// its dot, the one in use checked; then Profile, which the avatar opened before. Your hubs keep the choice, so it is
// the same on all your devices; others see it only while one of them is connected (offline wins, 09:03Z).
// Invisible (hub feature `invisible`; docket hubchat-status-picker-add-invisible, user 2026-10-31 14:13Z, people only):
// the offline dot; others see you offline while you stay connected. A connected hub without it can't hide you, and the
// row names it.
import { useEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { api, type HubStatus, type MyStatus } from "../api";
import { Icon } from "../lib/icons";
import { errText } from "../lib/native";
import { myPresence } from "../lib/peers";
import { refreshState, useSnap } from "../lib/store";
import { toast } from "../lib/toast";
import { ChoiceRow } from "./HubPicker";
import { Avatar } from "./ui";

/** Each choice: what it does, and the dot others see (Automatic follows the hubs' own rules: no one dot). */
const CHOICES: { v: MyStatus; label: string; sub: string; dot?: "online" | "busy" | "dnd" | "offline" }[] = [
  { v: "auto", label: "Automatic", sub: "Online while you're here, busy after an hour away" },
  { v: "online", label: "Online", sub: "Online even while you're away", dot: "online" },
  { v: "busy", label: "Busy", sub: "Shown busy to everyone", dot: "busy" },
  { v: "dnd", label: "Do not disturb", sub: "No notifications on any of your devices", dot: "dnd" },
  { v: "invisible", label: "Invisible", sub: "Shown offline to everyone", dot: "offline" },
];

/** Invisible's line, naming each connected hub that can't hide you (it lacks the feature). */
function invisibleSub(sub: string, hubs: HubStatus[]): string {
  const cant = hubs.filter((h) => h.state === "connected" && !h.features.includes("invisible")).map((h) => h.name);
  return cant.length ? sub + " · " + cant.join(", ") + " can't hide you yet" : sub;
}

/** Pick a status: the core keeps it and tells your hubs. */
export const pickStatus = (v: MyStatus) => api.setStatus(v).then(() => refreshState()).catch((e) => toast(errText(e)));

function Choices({ onDone }: { onDone: () => void }) {
  const snap = useSnap();
  const status = snap.state?.status ?? "auto";
  return <>{CHOICES.map((c) => (
    <ChoiceRow key={c.v} cls="statusopt" sel={status === c.v} label={c.label} sub={c.v === "invisible" ? invisibleSub(c.sub, snap.state?.hubs ?? []) : c.sub} dot={c.dot}
      run={() => { onDone(); if (c.v !== status) void pickStatus(c.v); }} />
  ))}</>;
}

/** Your own avatar with the dot others see; it opens the picker. `onProfile` opens your profile. */
export function StatusButton({ size, onProfile }: { size: number; onProfile: () => void }) {
  const snap = useSnap();
  const me = snap.state?.me;
  const android = snap.state?.platform === "android";
  const p = myPresence(snap.state?.status ?? "auto", snap.state?.hubs ?? []);
  const [open, setOpen] = useState(false);
  const btn = useRef<HTMLButtonElement>(null);
  const pop = useRef<HTMLDivElement>(null);
  const [pos, setPos] = useState({ x: 0, y: 0 });

  useEffect(() => {
    if (!open || android) return;
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
  }, [open, android]);

  if (!me) return null;
  const profile = () => { setOpen(false); onProfile(); };
  const items = (
    <>
      <Choices onDone={() => setOpen(false)} />
      <div className="sep" />
      <button type="button" className="mi" role="menuitem" onClick={profile}><Icon name="person" />Profile</button>
    </>
  );
  return (
    <>
      <button ref={btn} type="button" className="me-status" title={p.title} aria-label={p.title} aria-haspopup="menu" aria-expanded={open}
        onClick={(e) => { e.stopPropagation(); setOpen((o) => !o); }}>
        <Avatar kind="me" name={me.name || me.id} size={size} pres={p.state} />
      </button>
      {open && !android ? (
        <div className="pop hubpop statuspop" ref={pop} role="menu" aria-label="Your status" style={{ left: pos.x, top: pos.y }} onClick={(e) => e.stopPropagation()}>
          <div className="pop-h">Your status</div>
          {items}
        </div>
      ) : null}
      {/* the app bar is positioned: the sheet goes to the screen, where the chat menu's sheet is */}
      {open && android ? createPortal(
        <>
          <div className="sheet-scrim" onClick={() => setOpen(false)} />
          <div className="sheet" role="menu" aria-label="Your status">
            <div className="grip" />
            <h4>Your status</h4>
            {items}
          </div>
        </>, btn.current?.closest(".scr-anim") ?? document.body) : null}
    </>
  );
}
