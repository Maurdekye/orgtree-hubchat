import { useEffect, useRef, useState, type ReactNode } from "react";
import { api, type PictureCrop } from "../api";
import { refreshAll } from "../lib/store";
import { errText } from "../lib/native";

/** The shared desktop/Android Profile editor. Cropping uses oriented native
 * previews; full original bytes never enter an image element or canvas. */
export function PictureEditor({ owner, hasPicture, trigger }: {
  owner: string; hasPicture: boolean; trigger?: (choose: () => void, busy: boolean, remove: () => void) => ReactNode;
}) {
  const [selected, setSelected] = useState<{ data: string; preview: string; width: number; height: number } | null>(null);
  const [crop, setCrop] = useState<PictureCrop>({ x: .5, y: .5, size: 1 });
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [editing, setEditing] = useState(false);
  const fileInput = useRef<HTMLInputElement>(null);
  const intent = useRef(0);
  useEffect(() => { intent.current++; setSelected(null); setError(""); setBusy(false); setEditing(false); return () => { intent.current++; }; }, [owner]);
  const start = () => {
    if (busy) return;
    setEditing(true);
    fileInput.current?.click();
  };
  const cancel = () => { intent.current++; setSelected(null); setError(""); setEditing(false); };
  const choose = async (file: File | undefined) => {
    const request = ++intent.current;
    setError(""); setSelected(null);
    if (!file) return;
    if (file.size > 5 * 1024 * 1024) { setError("Choose an image up to 5 MiB."); return; }
    setBusy(true);
    try {
      const bytes = new Uint8Array(await file.arrayBuffer());
      let binary = "";
      for (let at = 0; at < bytes.length; at += 8192) binary += String.fromCharCode(...bytes.subarray(at, at + 8192));
      const data = btoa(binary);
      const preview = await api.picturePreview(data);
      if (request !== intent.current) return;
      setSelected({ data, preview: "data:image/png;base64," + preview.data, width: preview.width, height: preview.height });
      setCrop({ x: .5, y: .5, size: 1 });
    } catch (e) { if (request === intent.current) setError(errText(e)); }
    finally { if (request === intent.current) setBusy(false); }
  };
  const save = async (remove: boolean) => {
    const request = ++intent.current;
    setBusy(true); setError("");
    try {
      await api.setPicture(owner, remove ? null : selected!.data, crop);
      await refreshAll();
      if (request === intent.current) { setSelected(null); setEditing(false); }
    } catch (e) { if (request === intent.current) setError(errText(e)); }
    finally { if (request === intent.current) setBusy(false); }
  };
  const side = selected ? Math.min(selected.width, selected.height) * crop.size : 1;
  return <>
    {trigger?.(start, busy, () => { setEditing(true); void save(true); })}
    <input ref={fileInput} id="picture-file" type="file" hidden={!!trigger} aria-label="Choose profile picture" accept="image/png,image/jpeg,image/webp" disabled={busy} onChange={e => { void choose(e.currentTarget.files?.[0]); e.currentTarget.value = ""; }} />
    {(!trigger || editing) && <div className="field">
    {!trigger && <label htmlFor="picture-file">Profile picture</label>}
    <div className="help">Static PNG, JPEG or WebP · up to 5 MiB and 16 megapixels. Shared across your devices and hubs.</div>
    {selected && <>
      <div aria-label="Square crop preview" style={{ position: "relative", width: 192, height: 192, overflow: "hidden", border: "1px solid var(--line)", margin: "12px 0" }}>
        <img alt="Selected profile picture crop" src={selected.preview} style={{ position: "absolute", maxWidth: "none", width: selected.width / side * 192, height: selected.height / side * 192, left: -(selected.width - side) * crop.x / side * 192, top: -(selected.height - side) * crop.y / side * 192 }} />
      </div>
      <label>Crop size<input aria-label="Crop size" type="range" min="0.1" max="1" step="0.01" value={crop.size} disabled={busy} onChange={e => setCrop(c => ({ ...c, size: Number(e.target.value) }))} /></label>
      <label>Horizontal position<input aria-label="Horizontal crop position" type="range" min="0" max="1" step="0.01" value={crop.x} disabled={busy} onChange={e => setCrop(c => ({ ...c, x: Number(e.target.value) }))} /></label>
      <label>Vertical position<input aria-label="Vertical crop position" type="range" min="0" max="1" step="0.01" value={crop.y} disabled={busy} onChange={e => setCrop(c => ({ ...c, y: Number(e.target.value) }))} /></label>
      <button className="btn primary" disabled={busy} onClick={() => void save(false)}>Save picture</button>{" "}
    </>}
    {hasPicture && !trigger && <button className="btn" disabled={busy} onClick={() => void save(true)}>Remove picture</button>}
    {(selected || trigger) && <>{" "}<button className="btn" disabled={busy} onClick={cancel}>Cancel</button></>}
    {busy && <p role="status">Processing picture…</p>}
    {error && <p role="alert">{error}</p>}
  </div>}
  </>;
}
