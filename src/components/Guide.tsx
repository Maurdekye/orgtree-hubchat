// The setup guide (docs/setup.md, user ruling 2026-10-09 via the linking
// design §4): one document in the repo, bundled here as Hubchat's help. It
// opens from Settings › Help, from "Don't have a mail hub?" and from the
// welcome screen's "No code? ... See the guide". The same markdown renderer
// as messages; its links go to the system browser.
import { useEffect, useId, useRef, useState, type MouseEvent, type KeyboardEvent as ReactKeyEvent } from "react";
import { createRoot } from "react-dom/client";
import guideSrc from "../../docs/setup.md?raw";
import { Icon } from "../lib/icons";
import { esc, guideMd } from "../lib/md";
import { splitGuide } from "../lib/guide-sections";
import { openLink } from "../lib/native";
import { getSnap } from "../lib/store";
import { useBackCloses } from "./HubHelp";
import { Modal, ModalHead, PlatformCtx, usePlatform, type Platform } from "./ui";
import "../styles/setup.css";

const TITLE = "Setting up Hubchat";
// the document's own title is the dialog's; the body starts after it. Its
// screenshots are for GitHub: the app doesn't carry the pictures, and this
// renderer shows HTML as text, so each marked block is left out
const guide = splitGuide(guideSrc);
const intro = guideMd(guide.intro);
const sections = guide.sections.map(section => {
  let heading = 0;
  const html = guideMd(section.markdown).replace(/<h([1-6])\b/g, (tag) =>
    `${tag} data-guide-anchor="${esc(section.anchors[heading++] ?? "")}"`);
  return { ...section, html };
});

/** The guide itself. */
export function GuideBody() {
  const [active, setActive] = useState(0);
  const id = useId();
  const tabs = useRef<(HTMLButtonElement | null)[]>([]);
  const body = useRef<HTMLDivElement>(null);
  const select = (index: number, focus = false) => {
    setActive(index);
    if (focus) tabs.current[index]?.focus();
    tabs.current[index]?.scrollIntoView({ block: "nearest", inline: "nearest" });
  };
  const click = (e: MouseEvent) => {
    const a = (e.target as HTMLElement).closest("[data-href]");
    if (!a) return;
    e.preventDefault();
    const href = a.getAttribute("data-href") || "";
    if (!href.startsWith("#")) { void openLink(href); return; }
    let anchor: string;
    try { anchor = decodeURIComponent(href.slice(1)); } catch { return; }
    const index = sections.findIndex(section => section.anchors.includes(anchor));
    if (index < 0) return;
    select(index, true);
    requestAnimationFrame(() => {
      const target = [...(body.current?.querySelectorAll<HTMLElement>("[data-guide-anchor]") ?? [])]
        .find(heading => heading.dataset.guideAnchor === anchor);
      target?.scrollIntoView({ block: "nearest" });
    });
  };
  const tabKeys = (e: ReactKeyEvent, index: number) => {
    if (e.altKey || e.ctrlKey || e.metaKey) return;
    const next = e.key === "ArrowRight" ? (index + 1) % sections.length
      : e.key === "ArrowLeft" ? (index + sections.length - 1) % sections.length
      : e.key === "Home" ? 0 : e.key === "End" ? sections.length - 1 : null;
    if (next == null) return;
    e.preventDefault();
    select(next, true);
  };
  return <div className="guide" onClick={click} onKeyDown={e => {
    if (e.key === "Enter" && (e.target as HTMLElement).closest("[data-href]")) {
      e.preventDefault(); (e.target as HTMLElement).closest<HTMLElement>("[data-href]")?.click();
    }
  }}>
    {intro && <div className="guide-intro" dangerouslySetInnerHTML={{ __html: intro }} />}
    {sections.length > 0 && <>
      <div className="guide-tabs" role="tablist" aria-label="Setup guide sections">
        {sections.map((section, index) => <button type="button" key={index}
          ref={el => { tabs.current[index] = el; }} role="tab" id={`${id}-tab-${index}`}
          aria-controls={`${id}-panel-${index}`} aria-selected={active === index}
          tabIndex={active === index ? 0 : -1} onClick={() => select(index)}
          onKeyDown={e => tabKeys(e, index)}>{section.label}</button>)}
      </div>
      {sections.map((section, index) => <div key={index} ref={active === index ? body : undefined}
        role="tabpanel" id={`${id}-panel-${index}`} aria-labelledby={`${id}-tab-${index}`}
        hidden={active !== index} tabIndex={0} className="guide-section"
        dangerouslySetInnerHTML={{ __html: section.html }} />)}
    </>}
  </div>;
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
