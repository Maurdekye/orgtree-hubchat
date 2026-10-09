// "Don't have a mail hub?" (user 2026-10-08 23:07Z, 23:09Z): how to get a hub
// Hubchat can connect to (Orgtree's built-in one, or the standalone mail hub
// v2.0) and how to reach it from outside the local network: Tailscale,
// recommended, or the hub's relay-only door. Brief here and worded after the
// hub's README (Connect Hubchat), which has the full steps.
// Every device, the hub's own computer too, connects through the relay-only
// door, never the main port opened to the network (user 2026-10-09 05:46Z):
// the main port's page shows every message on the hub. On the hub's computer
// that is localhost:7371, so a phone linked from it is handed the computer's
// names at 7371 (hub_aliases keeps the port), which the phone can reach.
//   HubHelp: the inline toggle (HubAdder, under the address field)
//   HubHelpLink: a link that opens HubHelpDialog (banners, strips, Directory,
//     Settings) through openHubHelp
//   HubHelpDialog: a modal on desktop, a bottom sheet on Android; Escape, the
//     scrim and Android's back button close it
import { useEffect, useRef, useState, type ReactNode, type Ref } from "react";
import { createRoot } from "react-dom/client";
import { Icon } from "../lib/icons";
import { openLink } from "../lib/native";
import { Modal, ModalHead, PlatformCtx, usePlatform, type Platform } from "./ui";
import "../styles/hubhelp.css";

const HUB_REPO = "https://github.com/Maurdekye/orgtree-mailhub";
export const HUB_DOCS = HUB_REPO + "#connect-hubchat";
const TAILSCALE = "https://tailscale.com/download";
const TITLE = "Running and reaching a mail hub";

/** A link that opens in the system browser. */
function Ext({ href, children }: { href: string; children: ReactNode }) {
  return <a href={href} target="_blank" rel="noreferrer" onClick={(e) => { e.preventDefault(); void openLink(href); }}>{children}<Icon name="open_in_new" /></a>;
}
const M = ({ children }: { children: ReactNode }) => <span className="mono">{children}</span>;

/** The help itself. */
export function HubHelpBody() {
  const android = usePlatform() === "android";
  return (
    <div className="hubhelp">
      <p>Hubchat's messages travel through a mail hub. You can run your own, in Orgtree or on its own.</p>
      <h5>With Orgtree (it has one built in)</h5>
      <ol>
        <li>In Orgtree, open <b>App settings › Mail hub</b>.</li>
        <li>Under <b>Public access</b>, turn on <b>Also serve a relay-only door on port 7371</b>, then click <b>Save hosting settings</b>. Leave <b>Hosting › Listen on</b> at <b>This computer only</b>.</li>
        <li>{android
          ? <>In Hubchat, add the computer's address and port 7371, for example <M>home-pc:7371</M>.</>
          : <>In Hubchat, add the computer's address and port 7371, for example <M>home-pc:7371</M>; on that computer itself, <M>localhost:7371</M>.</>}</li>
      </ol>
      <div className="hubhelp-warn"><Icon name="privacy" /><div><b>Why not "This computer and the local network"?</b> That opens the hub's main port, whose page shows every message on the hub, so anyone on your Wi-Fi could read them all. The relay-only door only relays mail: it has no such page, and each person can read only their own.</div></div>
      <h5>On its own (mail hub v2.0)</h5>
      <p>Get it from <Ext href={HUB_REPO}>github.com/Maurdekye/orgtree-mailhub</Ext>. In its folder, copy <M>.env.example</M> to <M>.env</M> and set <M>HUB_DB_PASSWORD</M>. For the same reason, also set <M>HUB_PUBLIC=1</M> (the relay-only door) and <M>HUB_BIND=127.0.0.1</M> (the main port stays on that computer). Then run <M>docker compose up -d --build</M>, and in Hubchat add the computer's address and port 7378, for example <M>home-pc:7378</M>.</p>
      <h5>From outside your network: use Tailscale</h5>
      <p>Tailscale is the safer, simpler choice: nothing is opened to the internet, only devices in your tailnet can reach the hub, and Tailscale encrypts the traffic (the hub has no encryption of its own).</p>
      <ol>
        <li>Install <Ext href={TAILSCALE}>Tailscale</Ext> on the hub's computer and on each phone or PC that runs Hubchat.</li>
        <li>In Hubchat, add the hub by the computer's Tailscale name and the door's port, for example <M>home-pc:7371</M> (or its <M>100.x.y.z</M> address).</li>
      </ol>
      <p>If a device can't connect, check that the hub computer's firewall lets the port in (on Windows, allow the hub if Windows asks).</p>
      <h5>The open internet: only the relay-only door</h5>
      <p>Without Tailscale, open only the relay-only door to the internet, never the main port 7370. Anyone who reaches the door can register an address and send mail, but can read only their own.</p>
      <p>Put a tunnel or reverse proxy that gives you an <M>https://</M> address in front of it, and add that address in Hubchat. A plain port forward sends every message and secret unencrypted.</p>
      <p className="hubhelp-more"><Ext href={HUB_DOCS}>Full instructions in the hub's README</Ext></p>
    </div>
  );
}

/** "Don't have a mail hub?": a link that opens the help in place. Controlled
 *  when `open` is given (HubAdder's "Can't reach that hub" card opens it too). */
export function HubHelp({ open: shown, onToggle, label, ref }: { open?: boolean; onToggle?: (open: boolean) => void; label?: string; ref?: Ref<HTMLDivElement> }) {
  const [own, setOwn] = useState(false);
  const open = shown ?? own;
  const toggle = () => { setOwn(!open); onToggle?.(!open); };
  return (
    <div className="hubhelp-wrap" ref={ref}>
      <button type="button" className="link hubhelp-toggle" onClick={toggle} aria-expanded={open}>
        <Icon name="help" />{label ?? "Don't have a mail hub?"}<Icon name={open ? "expand_less" : "expand_more"} />
      </button>
      {open ? <HubHelpBody /> : null}
    </div>
  );
}

/** A link (or, with `className`, a button) that opens the help as a dialog. */
export function HubHelpLink({ label = "Help", className = "link", icon }: { label?: string; className?: string; icon?: boolean }) {
  const platform = usePlatform();
  return <button type="button" className={className + " hubhelp-open"} onClick={(e) => openHubHelp(platform, e.currentTarget)} aria-haspopup="dialog">{icon ? <Icon name="help" /> : null}{label}</button>;
}

/** Closes the open help, if any. */
let shut: (() => void) | null = null;

/** Opens the help in a React root of its own, so it outlives what opened it:
 *  a "Can't reach hub" banner or strip hides while its hub retries
 *  ("connecting"). Desktop: over the window; Android: over the screens (or
 *  the onboarding page) that `from` is on. */
export function openHubHelp(platform: Platform, from?: Element | null) {
  if (shut) return;
  const host = platform === "android" ? from?.closest(".viewport, .ob") ?? document.querySelector(".viewport, .ob") ?? document.body : document.body;
  const box = document.createElement("div");
  box.className = "hubhelp-root";
  host.appendChild(box);
  const root = createRoot(box);
  // the screens it sat on went away (another identity took over): go with them
  const gone = setInterval(() => { if (!box.isConnected) close(); }, 1000);
  function close() {
    if (shut !== close) return;
    shut = null;
    clearInterval(gone);
    setTimeout(() => { root.unmount(); box.remove(); }, 0);
  }
  shut = close;
  root.render(<PlatformCtx.Provider value={platform}><HubHelpDialog onClose={close} /></PlatformCtx.Provider>);
}

let seq = 0;

/** Android: the sheet holds a history entry of its own, so the system back
 *  button closes it and the screen below stays; closed any other way, it
 *  takes that entry back off without the screens seeing it. */
function useBackCloses(on: boolean, close: () => void) {
  const [id] = useState(() => ++seq);
  const closeRef = useRef(close);
  closeRef.current = close;
  const alive = useRef(false);
  useEffect(() => {
    if (!on) return;
    alive.current = true;
    if (history.state?.hubhelp !== id) history.pushState({ ...(history.state ?? {}), hubhelp: id }, "");
    const pop = (e: PopStateEvent) => {
      if (history.state?.hubhelp === id) return;
      e.stopImmediatePropagation();
      closeRef.current();
    };
    window.addEventListener("popstate", pop, true);
    return () => {
      alive.current = false;
      window.removeEventListener("popstate", pop, true);
      // not on StrictMode's re-mount: only when it really closed
      setTimeout(() => {
        if (alive.current || history.state?.hubhelp !== id) return;
        const eat = (e: PopStateEvent) => { e.stopImmediatePropagation(); window.removeEventListener("popstate", eat, true); };
        window.addEventListener("popstate", eat, true);
        setTimeout(() => window.removeEventListener("popstate", eat, true), 1000);
        history.back();
      }, 0);
    };
  }, [on, id]);
}

/** The help as a modal (desktop) or a bottom sheet (Android), rendered in
 *  place: openHubHelp puts it where it belongs, away from the styles of the
 *  banner, card or list that opened it. Escape closes it (and nothing under
 *  it); on Android the back button does too. */
export function HubHelpDialog({ onClose }: { onClose: () => void }) {
  const android = usePlatform() === "android";
  const closeRef = useRef(onClose);
  closeRef.current = onClose;
  const close = () => closeRef.current();
  useEffect(() => {
    const k = (e: KeyboardEvent) => { if (e.key === "Escape") { e.preventDefault(); e.stopPropagation(); closeRef.current(); } };
    window.addEventListener("keydown", k, true);
    return () => window.removeEventListener("keydown", k, true);
  }, []);
  useBackCloses(android, onClose);
  if (android) {
    return (
      <>
        <div className="sheet-scrim" onClick={close} />
        <div className="sheet hubhelp-sheet" role="dialog" aria-modal="true" aria-label={TITLE}>
          <div className="grip" />
          <div className="hubhelp-sh"><h4>{TITLE}</h4><button className="icon-btn" onClick={close} aria-label="Close"><Icon name="close" /></button></div>
          <HubHelpBody />
        </div>
      </>
    );
  }
  return (
    <Modal onClose={close} className="hubhelp-modal">
      <ModalHead title={TITLE} onClose={close} />
      <div className="modal-b"><HubHelpBody /></div>
    </Modal>
  );
}
