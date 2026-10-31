// Image attachments (user 23:29Z; design att-img): the picture in the bubble,
// with the upload or download overlay while it moves, and the full-size
// viewer a click opens (desktop overlay, Android screen; Esc or back closes
// it). Without a preview (too big, gone from the hub, none fetched) the
// file card shows instead.
import { useEffect, useRef, useState, type PointerEvent as ReactPointerEvent, type ReactNode, type WheelEvent as ReactWheelEvent } from "react";
import { api, type Attachment, type Message } from "../api";
import { bytes } from "../lib/format";
import { Icon } from "../lib/icons";
import { closeImage, openImage, usePreview, useShownImage } from "../lib/images";
import { errText, openAttachment, openFile } from "../lib/native";
import { attView } from "../lib/peers";
import { onChatChange, useTransfer } from "../lib/store";
import { toast } from "../lib/toast";
import { HOME, panView, pinchView, resetPageZoom, toggleZoom, wheelView, type Pt, type View } from "./pinchmath";
import { pressKeys, usePlatform } from "./ui";
import "../styles/images.css";

export function AttImage({ a, m, fallback }: { a: Attachment; m: Message; fallback: ReactNode }) {
  const prog = useTransfer(a.local_id);
  const v = attView(a, m, prog && (a.state === "uploading" || a.state === "downloading") ? prog : undefined);
  const { url, failed } = usePreview(m, a);
  if (failed) return <>{fallback}</>;
  const moving = v.st === "busy" || v.st === "bad";
  return (
    <div className={"att-img" + (moving ? " xfer" : "") + (url ? "" : " wait")} data-att={a.local_id} role="button" tabIndex={0} title={a.name} aria-label={"Image " + a.name}
      onKeyDown={pressKeys(() => { if (url) openImage(m, a); })}
      onClick={(e) => { e.stopPropagation(); if (url) openImage(m, a); }}>
      {url ? <img src={url} alt={a.name} draggable={false} /> : <span className="att-ph"><Icon name="image" /></span>}
      {moving ? (
        <span className={"att-ovl " + v.st}>
          <span className="att-s">{v.sub}</span>
          {v.bar != null ? <span className="att-bar"><i style={{ width: v.bar + "%" }} /></span> : null}
        </span>
      ) : null}
      {v.cancel ? <button className="icon-btn att-x" title={m.outgoing && !a.download ? "Cancel upload" : "Cancel download"} aria-label="Cancel" onClick={(e) => { e.stopPropagation(); api.cancelTransfer(a.local_id).catch(() => {}); }}><Icon name="close" /></button> : null}
    </div>
  );
}

/** Android: the open viewer's history entry. */
let entry: string | null = null;

/** Mounted once per layout: the image a bubble opened, if any. */
export function ImageViewer() {
  const s = useShownImage();
  return s ? <Viewer key={s.m.id + "/" + s.a.local_id} m={s.m} a={s.a} /> : null;
}

function Viewer({ m, a }: { m: Message; a: Attachment }) {
  const platform = usePlatform();
  const { url } = usePreview(m, a);
  const [current, setCurrent] = useState(a);
  const [requesting, setRequesting] = useState(false);
  useEffect(() => {
    let live = true;
    const refresh = () => { void api.message(m.id).then((message) => {
      const next = message?.attachments.find((x) => x.local_id === a.local_id);
      if (live && next) setCurrent(next);
    }, () => {}); };
    refresh();
    const off = onChatChange(m.peer, refresh);
    return () => { live = false; off(); };
  }, [m.id, m.peer, a.local_id]);
  const progress = useTransfer(current.local_id);
  const view = attView(current, m, progress);
  const saved = view.open && current.local_path;
  const saving = requesting || view.st === "busy";

  useEffect(() => {
    if (platform === "android") {
      // its own history entry, so the system back button closes it: pushed
      // once per opening (effects may run twice), keeping the screen's depth
      const id = m.id + "/" + a.local_id;
      if (entry !== id) { entry = id; history.pushState({ ...(history.state || {}), hcImage: id }, ""); }
      const pop = () => { entry = null; closeImage(); };
      window.addEventListener("popstate", pop);
      return () => window.removeEventListener("popstate", pop);
    }
    const key = (e: KeyboardEvent) => { if (e.key === "Escape") { e.preventDefault(); e.stopPropagation(); closeImage(); } };
    window.addEventListener("keydown", key, true);
    return () => window.removeEventListener("keydown", key, true);
  }, [platform, m.id, a.local_id]);
  const close = () => { if (platform === "android") history.back(); else closeImage(); };

  // pinch, pan and double tap move the picture only: the overlay takes the touches (touch-action: none),
  // so the WebView never scales the page, and a page scale left by anything else is undone on open and close
  const img = useRef<HTMLImageElement>(null);
  const [zoom, setZoom] = useState<View>(HOME);
  const viewRef = useRef<View>(HOME);
  const fingers = useRef(new Map<number, Pt>());
  const gesture = useRef<{ view: View; a: Pt; b: Pt } | { view: View; at: Pt } | null>(null);
  const moved = useRef(false);
  const put = (v: View) => { viewRef.current = v; setZoom(v); };
  useEffect(() => {
    resetPageZoom(document);
    return () => { resetPageZoom(document); };
  }, []);
  const size = () => ({ w: img.current?.offsetWidth ?? 0, h: img.current?.offsetHeight ?? 0 });
  // the picture's unscaled centre on screen: its box centre less the current pan
  const centre = (): Pt => {
    const r = img.current?.getBoundingClientRect();
    return r ? { x: r.left + r.width / 2 - viewRef.current.x, y: r.top + r.height / 2 - viewRef.current.y } : { x: 0, y: 0 };
  };
  const from = (p: Pt): Pt => { const c = centre(); return { x: p.x - c.x, y: p.y - c.y }; };
  const begin = () => {
    const pts = [...fingers.current.values()];
    gesture.current = pts.length >= 2 ? { view: viewRef.current, a: pts[0]!, b: pts[1]! }
      : pts.length === 1 ? { view: viewRef.current, at: pts[0]! } : null;
  };
  const down = (e: ReactPointerEvent) => {
    if ((e.target as HTMLElement).closest(".iv-close, .iv-cap")) return;
    fingers.current.set(e.pointerId, { x: e.clientX, y: e.clientY });
    // no setPointerCapture: capture would retarget the click that ends a tap to the overlay (the picture's
    // own double click, and the tap that must not close, never reach their target); touches already stay with the picture
    moved.current = false;
    begin();
  };
  const move = (e: ReactPointerEvent) => {
    if (!fingers.current.has(e.pointerId) || !gesture.current) return;
    fingers.current.set(e.pointerId, { x: e.clientX, y: e.clientY });
    const pts = [...fingers.current.values()];
    const g = gesture.current;
    if ("a" in g && pts.length >= 2) {
      put(pinchView(g.view, from(g.a), from(g.b), from(pts[0]!), from(pts[1]!), size()));
      moved.current = true;
    } else if ("at" in g && pts.length === 1) {
      const dx = pts[0]!.x - g.at.x, dy = pts[0]!.y - g.at.y;
      if (Math.abs(dx) + Math.abs(dy) > 4) moved.current = true;
      if (g.view.scale > 1) put(panView(g.view, dx, dy, size()));
    }
  };
  const up = (e: ReactPointerEvent) => {
    fingers.current.delete(e.pointerId);
    begin();
  };
  const wheel = (e: ReactWheelEvent) => {
    if (!e.ctrlKey) return;
    put(wheelView(viewRef.current, e.deltaY, from({ x: e.clientX, y: e.clientY }), size()));
  };
  const dbl = (e: { clientX: number; clientY: number }) => {
    put(toggleZoom(viewRef.current, from({ x: e.clientX, y: e.clientY }), size()));
  };

  const download = () => {
    setRequesting(true);
    api.download(m.id, a.local_id).catch((e) => toast("Download failed: " + errText(e))).finally(() => setRequesting(false));
  };
  const open = (reveal: boolean) => openAttachment(m.id, a.local_id, reveal).catch((e) => toast(errText(e)));
  const fromHere = m.outgoing && a.source && !a.source.startsWith("content://") && platform === "desktop" ? a.source : null;
  const actions = m.outgoing && a.source && !current.download
    ? (fromHere ? <button className="link" onClick={() => openFile(fromHere).catch((e) => toast(errText(e)))}>Open</button> : null)
    : saved
      ? <><button className="link" onClick={() => open(false)}>Open</button>{platform === "desktop" ? <button className="link" onClick={() => open(true)}>Show in folder</button> : null}</>
      : saving
        ? <span className="iv-progress" role="status">{view.sub}</span>
        : <button className="link" onClick={download}>{view.retry ? "Retry download" : "Download"}</button>;

  return (
    <div className="imgview" role="dialog" aria-label={a.name} onClick={() => { if (!moved.current) close(); moved.current = false; }}
      onPointerDown={down} onPointerMove={move} onPointerUp={up} onPointerCancel={up} onWheel={wheel}>
      <button className="icon-btn iv-close" onClick={(e) => { e.stopPropagation(); close(); }} title={platform === "desktop" ? "Close (Esc)" : undefined} aria-label="Close">
        <Icon name={platform === "android" ? "back" : "close"} />
      </button>
      {url ? <img ref={img} src={url} alt={a.name} draggable={false} style={{ transform: `translate(${zoom.x}px, ${zoom.y}px) scale(${zoom.scale})` }} onClick={(e) => e.stopPropagation()} onDoubleClick={dbl} /> : <span className="att-ph"><Icon name="image" /></span>}
      <div className="iv-cap" onClick={(e) => e.stopPropagation()}>
        <span className="ell">{a.name}</span><span className="dim"> · {bytes(a.bytes)}</span>{actions ? <span className="iv-acts">{actions}</span> : null}
      </div>
    </div>
  );
}
