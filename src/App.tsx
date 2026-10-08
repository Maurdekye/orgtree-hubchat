// SPIKE page: shows the core's connection and runs the file-transfer round
// trip. Phase 2 replaces this with the real Hubchat UI.
import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open, save } from "@tauri-apps/plugin-dialog";
import "./App.css";

type Info = { address: string; hub: string; max_attachment_bytes: number | null; error: string | null };
type Xfer = { phase: string; done: number; total: number };
type RoundTrip = {
  name: string; bytes: number; upload_ms: number; download_ms: number;
  upload_mb_s: number; download_mb_s: number; identical: boolean;
  peak_rss_kb: number | null; saved_to: string;
};

const mb = (n: number) => (n / 1048576).toFixed(1) + " MB";

export default function App() {
  const [info, setInfo] = useState<Info | null>(null);
  const [xfer, setXfer] = useState<Xfer | null>(null);
  const [result, setResult] = useState<RoundTrip | null>(null);
  const [log, setLog] = useState<string[]>([]);
  const say = (s: string) => setLog((l) => [new Date().toLocaleTimeString() + " " + s, ...l].slice(0, 30));

  const refresh = () => invoke<Info>("spike_info").then(setInfo).catch((e) => say(String(e)));
  useEffect(() => {
    refresh();
    const un = listen<Xfer>("xfer", (e) => setXfer(e.payload));
    return () => { un.then((f) => f()); };
  }, []);

  async function pickAndSend() {
    const picked = await open({ multiple: false, directory: false });
    if (!picked) return;
    const path = String(picked);
    const name = decodeURIComponent(path.split(/[\\/]/).pop() || "file");
    say("picked " + path);
    setResult(null);
    try {
      const r = await invoke<RoundTrip>("spike_roundtrip", { path, name });
      setResult(r);
      say(`round trip ok: ${mb(r.bytes)} identical=${r.identical}`);
    } catch (e) {
      say("failed: " + e);
    }
  }

  async function saveCopy() {
    if (!result) return;
    const dest = await save({ defaultPath: result.name });
    if (!dest) return;
    try {
      const n = await invoke<number>("spike_save", { src: result.saved_to, dest: String(dest) });
      say(`saved ${mb(n)} to ${dest}`);
    } catch (e) {
      say("save failed: " + e);
    }
  }

  return (
    <main className="container" style={{ textAlign: "left", padding: 16 }}>
      <h2>Hubchat spike</h2>
      <p id="addr">Address: <b>{info?.address || "…"}</b></p>
      <p id="hub">Hub: {info?.hub} {info?.error ? "— " + info.error : "— reachable"}</p>
      <p>Max attachment: {info?.max_attachment_bytes ? mb(info.max_attachment_bytes) : "?"}</p>
      <div className="row" style={{ gap: 8, justifyContent: "flex-start" }}>
        <button id="refresh" onClick={refresh}>Refresh</button>
        <button id="pick" onClick={pickAndSend}>Pick file and round-trip</button>
        <button id="save" onClick={saveCopy} disabled={!result}>Save copy…</button>
      </div>
      {xfer && (
        <p id="progress">
          {xfer.phase}: {mb(xfer.done)} / {mb(xfer.total)} ({xfer.total ? Math.floor((100 * xfer.done) / xfer.total) : 0}%)
          <progress value={xfer.done} max={xfer.total || 1} style={{ width: "100%" }} />
        </p>
      )}
      {result && (
        <pre id="result" style={{ whiteSpace: "pre-wrap" }}>{JSON.stringify(result, null, 1)}</pre>
      )}
      <pre style={{ whiteSpace: "pre-wrap", fontSize: 12 }}>{log.join("\n")}</pre>
    </main>
  );
}
