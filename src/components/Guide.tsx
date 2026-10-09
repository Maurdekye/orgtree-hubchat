// The setup guide (docs/setup.md, user ruling 2026-10-09 via the linking
// design §4): one document in the repo, bundled here as Hubchat's help. It
// opens from Settings › Help, from "Don't have a mail hub?" and from the
// welcome screen's "No code? ... See the guide". The same markdown renderer
// as messages; its links go to the system browser.
import { useEffect, useRef, type MouseEvent } from "react";
import { createRoot } from "react-dom/client";
import guideSrc from "../../docs/setup.md?raw";
import { Icon } from "../lib/icons";
import { md } from "../lib/md";
import { openLink } from "../lib/native";
import { getSnap } from "../lib/store";
import { useBackCloses } from "./HubHelp";
import { Modal, ModalHead, PlatformCtx, usePlatform, type Platform } from "./ui";
import "../styles/setup.css";

const TITLE = "Setting up Hubchat";
// the document's own title is the dialog's; the body starts after it. Its
// screenshots are for GitHub: the app doesn't carry the pictures, and this
// renderer shows HTML as text, so each marked block is left out
const html = md(guideSrc.replace(/^# .*\n+/, "").replace(/<!-- screenshots -->[\s\S]*?<!-- \/screenshots -->\n*/g, ""));

/** The guide itself. */
export function GuideBody() {
  const click = (e: MouseEvent) => {
    const a = (e.target as HTMLElement).closest("[data-href]");
    if (a) { e.preventDefault(); void openLink(a.getAttribute("data-href") || ""); }
  };
  return <div className="guide" onClick={click} dangerouslySetInnerHTML={{ __html: html }} />;
}

let shut: (() => void) | null = null;

/** Open the guide over whatever is showing, in a React root of its own
 *  (it may open from onboarding, which has no layout to hold it). */
export function openGuide(platform: Platform = getSnap().state?.platform ?? "desktop") {
  if (shut) return;
  const box = document.createElement("div");
  box.className = "guide-root";
  document.body.appendChild(box);
  const root = createRoot(box);
  function close() {
    if (shut !== close) return;
    shut = null;
    setTimeout(() => { root.unmount(); box.remove(); }, 0);
  }
  shut = close;
  root.render(<PlatformCtx.Provider value={platform}><GuideDialog onClose={close} /></PlatformCtx.Provider>);
}

/** A modal on desktop; a full screen on Android, which its back button closes. */
function GuideDialog({ onClose }: { onClose: () => void }) {
  const android = usePlatform() === "android";
  const closeRef = useRef(onClose);
  closeRef.current = onClose;
  useEffect(() => {
    const k = (e: KeyboardEvent) => { if (e.key === "Escape") { e.preventDefault(); e.stopPropagation(); closeRef.current(); } };
    window.addEventListener("keydown", k, true);
    return () => window.removeEventListener("keydown", k, true);
  }, []);
  useBackCloses(android, onClose);
  if (android) {
    return (
      <div className="screen guide-screen" role="dialog" aria-modal="true" aria-label={TITLE}>
        <div className="appbar flat"><button className="icon-btn" onClick={onClose} aria-label="Back"><Icon name="back" /></button><div className="title">{TITLE}</div></div>
        <div className="scr-body"><GuideBody /></div>
      </div>
    );
  }
  return (
    <Modal onClose={onClose} className="guide-modal">
      <ModalHead title={TITLE} onClose={onClose} />
      <div className="modal-b"><GuideBody /></div>
    </Modal>
  );
}
