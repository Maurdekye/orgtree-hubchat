// Image attachments (user 23:29Z; design att-img): the picture in the bubble,
// with the upload or download overlay while it moves, and the full-size
// viewer a click opens (desktop overlay, Android screen; Esc or back closes
// it). Without a preview (too big, gone from the hub, none fetched) the
// file card shows instead.
import { useEffect, useState, type ReactNode } from "react";
import { api, type Attachment, type Message } from "../api";
import { bytes } from "../lib/format";
import { Icon } from "../lib/icons";
import { closeImage, openImage, usePreview, useShownImage } from "../lib/images";
import { errText, openAttachment, openFile } from "../lib/native";
import { attView } from "../lib/peers";
import { useTransfer } from "../lib/store";
import { toast } from "../lib/toast";
import { usePlatform } from "./ui";
import "../styles/images.css";

export function AttImage({ a, m, fallback }: { a: Attachment; m: Message; fallback: ReactNode }) {
  const prog = useTransfer(a.local_id);
  const v = attView(a, m, prog && (a.state === "uploading" || a.state === "downloading") ? prog : undefined);
  const { url, failed } = usePreview(m, a);
  if (failed) return <>{fallback}</>;
  const moving = v.st === "busy" || v.st === "bad";
  return (
    <div className={"att-img" + (moving ? " xfer" : "") + (url ? "" : " wait")} data-att={a.local_id} role="button" title={a.name} aria-label={"Image " + a.name}
      onClick={(e) => { e.stopPropagation(); if (url) openImage(m, a); }}>
      {url ? <img src={url} alt={a.name} draggable={false} /> : <span className="att-ph"><Icon name="image" /></span>}
      {moving ? (
        <span className={"att-ovl " + v.st}>
          <span className="att-s">{v.sub}</span>
          {v.bar != null ? <span className="att-bar"><i style={{ width: v.bar + "%" }} /></span> : null}
        </span>
      ) : null}
      {v.cancel ? <button className="icon-btn att-x" title={m.outgoing ? "Cancel upload" : "Cancel download"} aria-label="Cancel" onClick={(e) => { e.stopPropagation(); api.cancelTransfer(a.local_id).catch(() => {}); }}><Icon name="close" /></button> : null}
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
  const [saved, setSaved] = useState<string | null>(a.state === "done" ? a.local_path : null);
  const [saving, setSaving] = useState(false);

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

  const download = () => {
    setSaving(true);
    api.download(m.id, a.local_id).then((p) => { setSaved(p); toast("Saved to Downloads"); }, (e) => toast("Download failed: " + errText(e))).finally(() => setSaving(false));
  };
  const open = (reveal: boolean) => openAttachment(m.id, a.local_id, reveal).catch((e) => toast(errText(e)));
  const fromHere = m.outgoing && a.source && !a.source.startsWith("content://") && platform === "desktop" ? a.source : null;
  const actions = m.outgoing
    ? (fromHere ? <button className="link" onClick={() => openFile(fromHere).catch((e) => toast(errText(e)))}>Open</button> : null)
    : saved
      ? <><button className="link" onClick={() => open(false)}>Open</button>{platform === "desktop" ? <button className="link" onClick={() => open(true)}>Show in folder</button> : null}</>
      : <button className="link" disabled={saving} onClick={download}>{saving ? "Downloading…" : "Download"}</button>;

  return (
    <div className="imgview" role="dialog" aria-label={a.name} onClick={close}>
      <button className="icon-btn iv-close" onClick={(e) => { e.stopPropagation(); close(); }} title={platform === "desktop" ? "Close (Esc)" : undefined} aria-label="Close">
        <Icon name={platform === "android" ? "back" : "close"} />
      </button>
      {url ? <img src={url} alt={a.name} onClick={(e) => e.stopPropagation()} /> : <span className="att-ph"><Icon name="image" /></span>}
      <div className="iv-cap" onClick={(e) => e.stopPropagation()}>
        <span className="ell">{a.name}</span><span className="dim"> · {bytes(a.bytes)}</span>{actions ? <span className="iv-acts">{actions}</span> : null}
      </div>
    </div>
  );
}
