// Tell the core what is on screen: it decides notifications and read
// receipts from this. A chat counts as read only while it is visible and the
// window is focused (desktop) or the app is in the foreground (Android).
import { useEffect, useRef, useState } from "react";
import { api, type Message } from "../api";
import { getSnap, onChatChange, useSnap } from "./store";

export function useForeground(platform: "desktop" | "android"): boolean {
  const calc = () => document.visibilityState === "visible" && (platform === "android" || document.hasFocus());
  const [fg, setFg] = useState(calc);
  useEffect(() => {
    const f = () => setFg(calc());
    window.addEventListener("focus", f); window.addEventListener("blur", f);
    document.addEventListener("visibilitychange", f);
    f();
    return () => { window.removeEventListener("focus", f); window.removeEventListener("blur", f); document.removeEventListener("visibilitychange", f); };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [platform]);
  return fg;
}

/** How long without a touch, a key or the mouse before a device on screen
 *  stops counting as in use. */
const IDLE_MS = 150_000;

/** In use (user 23:50Z): on screen (focused, on desktop) and touched in the
 *  last few minutes. While it is, our other devices don't notify; the core
 *  tells the hubs that can carry it. */
export function useActive(foreground: boolean): void {
  const last = useRef(Date.now());
  const fg = useRef(foreground);
  fg.current = foreground;
  const [active, setActive] = useState(false);
  useEffect(() => {
    const touch = () => { last.current = Date.now(); setActive((a) => a || fg.current); };
    const evs = ["pointerdown", "keydown", "wheel", "touchstart", "mousemove"] as const;
    evs.forEach((e) => window.addEventListener(e, touch, { passive: true, capture: true }));
    const t = setInterval(() => setActive(fg.current && Date.now() - last.current < IDLE_MS), 15000);
    return () => { evs.forEach((e) => window.removeEventListener(e, touch, { capture: true })); clearInterval(t); };
  }, []);
  // coming to the front counts as use; leaving it ends it at once
  useEffect(() => { if (foreground) last.current = Date.now(); setActive(foreground); }, [foreground]);
  useEffect(() => { void api.setActive(active).catch(() => {}); }, [active]);
}

/** Report foreground + visible chat; mark it read on open and as messages arrive. */
export function useReadTracking(peer: string | null, foreground: boolean): void {
  const snap = useSnap();
  const unread = peer ? snap.chats.find((c) => c.peer === peer)?.unread ?? 0 : 0;
  const inflight = useRef(false);
  const visible = foreground && !!peer && !!snap.state?.me;

  useEffect(() => { void api.uiState(foreground, visible ? peer : null).catch(() => {}); }, [foreground, visible, peer]);

  const mark = (p: string) => {
    if (inflight.current) return;
    inflight.current = true;
    api.markRead(p).catch(() => {}).finally(() => { inflight.current = false; });
  };
  // on open (and when coming back to the window)
  useEffect(() => { if (visible && peer) mark(peer); }, [visible, peer]); // eslint-disable-line react-hooks/exhaustive-deps
  // when new messages arrive while it is on screen
  useEffect(() => { if (visible && peer && unread > 0) mark(peer); }, [visible, peer, unread]); // eslint-disable-line react-hooks/exhaustive-deps
}

/** One message, kept current (Message info). */
export function useMessage(id: string | null, peer: string | null): Message | null {
  const [m, setM] = useState<Message | null>(null);
  const [tick, setTick] = useState(0);
  useEffect(() => {
    if (!id) { setM(null); return; }
    let live = true;
    api.message(id).then((r) => { if (live) setM(r); }, () => {});
    return () => { live = false; };
  }, [id, tick]);
  useEffect(() => (peer ? onChatChange(peer, () => setTick((t) => t + 1)) : undefined), [peer]);
  return m && m.id === id ? m : null;
}

export const myAddress = () => getSnap().state?.me?.address || "";

/** Android: a hubchat:// link the system opened the app with (a phone camera
 *  scanning another device's link QR). Taken at start and whenever the app
 *  comes back to the foreground; `f` gets each link once. */
export function usePendingLink(f: (link: string) => void): void {
  const ref = useRef(f);
  ref.current = f;
  useEffect(() => {
    const take = () => {
      if (document.visibilityState !== "visible") return;
      api.takePendingLink().then((l) => { if (l) ref.current(l); }, () => {});
    };
    take();
    window.addEventListener("focus", take);
    document.addEventListener("visibilitychange", take);
    const un = api.onPendingLink(take);
    return () => { window.removeEventListener("focus", take); document.removeEventListener("visibilitychange", take); void un.then((u) => u()); };
  }, []);
}
