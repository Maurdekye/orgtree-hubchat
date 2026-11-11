// The core's worker (spec R1.3, R1.6): Hubchat's Rust core as WebAssembly
// (crates/hubchat-web), one per origin. It owns the database (a file in the
// origin's private file system, which only a dedicated worker may open this
// way), the hub connections and the key store; the page only sends commands
// and gets answers and events (wire.ts).
import init, { boot, call, stop } from "@hubchat-core";
import wasmUrl from "@hubchat-core/wasm?url";
import { clearIdentity, loadIdentity, saveIdentity } from "./keys";
import { sentence, type FromCore, type ToCore } from "./wire";

// lib "DOM" types `self` as a window; a dedicated worker's own is this much
const scope = self as unknown as {
  postMessage(m: FromCore): void;
  onmessage: ((e: MessageEvent<ToCore>) => void) | null;
};
const post = (m: FromCore) => scope.postMessage(m);

let started: Promise<void> | null = null;

async function start(deviceName: string): Promise<void> {
  await init({ module_or_path: wasmUrl });
  await boot({
    deviceName,
    emit: (name, json) => post({ t: "event", name, ev: JSON.parse(json) }),
    loadIdentity,
    saveIdentity,
    clearIdentity,
  });
}

scope.onmessage = (e) => {
  const m = e.data;
  if (m.t === "boot") {
    started ??= start(m.deviceName);
    started.then(() => post({ t: "ready" }), (err) => post({ t: "failed", error: sentence(err) }));
  } else if (m.t === "call") {
    const { id, cmd, args } = m;
    (started ?? Promise.reject("the core is not started"))
      .then(() => call(cmd, JSON.stringify(args ?? null)))
      .then((json) => post({ t: "ret", id, ok: true, value: JSON.parse(json) }), (err) => post({ t: "ret", id, ok: false, error: sentence(err) }));
  } else if (m.t === "stop") {
    try { stop(); } catch { /* the page ends this worker anyway */ }
    post({ t: "stopped" });
  }
};
