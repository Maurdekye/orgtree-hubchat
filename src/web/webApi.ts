// Hubchat Web's side of src/api.ts (spec R1.3, R1.6, R1.7): the same
// commands with the same names and arguments, carried to the core's worker
// (core.worker.ts) instead of a native shell.
//
// One tab owns the core at a time: the database is a file only one worker
// may hold, so a tab takes the Web Lock `hubchat-core` before it starts the
// worker, and gives both up when another tab asks on the BroadcastChannel
// `hubchat` (Use it here).
import type { InvokeArgs, InvokeOptions } from "@tauri-apps/api/core";
import { makeApi, type Api, type Probe } from "../api";
import { installWebNative } from "../lib/native";
import { deviceName, layout } from "./device";
import { pickFile, pickFilesToSend, pickedText } from "./files";
import { saveFile, saveText } from "./save";
import { scanQr } from "./scan";
import { serverName, suggestedHubs } from "./suggest";
import { disablePush, enablePush, pendingChat, showUnread, syncPush, takePendingChat, watchPush } from "./push";
import { newerBuild } from "./update";
import { sentence, type FromCore, type ToCore } from "./wire";

const LOCK = "hubchat-core";
const CHANNEL = "hubchat";

/** What the core answers for a command a browser cannot do yet (crates/hubchat-web/src/shell.rs). */
const NOT_YET = "This isn't available in Hubchat Web yet.";

/** A small text file the core made for the page to save: the recovery words (`hc_recovery_file`), the
 *  passphrase-locked key file (`hc_key_file_text`). */
interface RecoveryFile { name: string; text: string }

/** A key file is a few hundred bytes; the apps read at most this much of one. */
const KEY_FILE_LIMIT = 1 << 20;
/** What stands for "where to save the key file": the browser decides where. */
const KEY_FILE_DEST = "hubchat-key.hubchat-key";

/** Hold the lock until the returned function is called. Null: another tab has it (and `wait` is off). */
function hold(wait: boolean): Promise<(() => void) | null> {
  return new Promise((resolve, reject) => {
    navigator.locks.request(LOCK, wait ? {} : { ifAvailable: true }, (lock) => {
      if (!lock) { resolve(null); return undefined; }
      return new Promise<void>((release) => resolve(release));
    }).catch(reject);
  });
}

/** R2.4: a browser reports a hub that refuses this page and a hub that is not there in the same way.
 *  One more request tells them apart: one whose answer the page may not read, which comes back only
 *  when something is there. */
async function explain(p: Probe): Promise<Probe> {
  if (p.result !== "unreachable") return p;
  try {
    await fetch(p.url.replace(/\/+$/, "") + "/healthz", { mode: "no-cors", cache: "no-store", credentials: "omit", signal: AbortSignal.timeout(8000) });
    return { result: "not_allowed", url: p.url, origin: location.origin, error: "it doesn't allow this web app yet (its owner adds " + location.origin + " to the hub's web app addresses)" };
  } catch {
    return p;
  }
}

/** R3.7: ask the browser to keep Hubchat's storage; without that it may delete it when space runs short. */
function keepStorage(): void {
  try { void navigator.storage.persist().catch(() => false); } catch { /* a browser without it */ }
}

/** The core, owned by this tab. */
export interface Core {
  api: Api;
  /** Resolves when another tab took the core over: this tab's is stopped and its lock given up. */
  taken: Promise<void>;
}

/** Take the core for this tab and start it. Null: another tab or window has it. With `takeOver`, ask
 *  that tab to give it up and wait for it. Rejects with a sentence when the core does not start. */
export async function openCore(takeOver = false): Promise<Core | null> {
  const channel = new BroadcastChannel(CHANNEL);
  let release: (() => void) | null;
  if (takeOver) {
    // in the line first, then the request: the lock comes straight to this tab when the owner lets go
    const waiting = hold(true);
    channel.postMessage({ t: "takeover" });
    release = await waiting;
  } else {
    release = await hold(false);
  }
  if (!release) { channel.close(); return null; }
  const free = release;

  const worker = new Worker(new URL("./core.worker.ts", import.meta.url), { type: "module" });
  const send = (m: ToCore) => worker.postMessage(m);
  const pending = new Map<number, { resolve: (v: unknown) => void; reject: (e: string) => void }>();
  const frames = new Map<number, (text: string | null) => void>();
  const listeners = new Map<string, Set<(e: { payload: unknown }) => void>>();
  let lastId = 0;
  let stopped: (() => void) | null = null;
  let dead: string | null = null;

  const ready = new Promise<void>((resolve, reject) => {
    worker.onmessage = (e: MessageEvent<FromCore>) => {
      const m = e.data;
      if (m.t === "ready") resolve();
      else if (m.t === "failed") reject(m.error);
      else if (m.t === "ret") {
        const p = pending.get(m.id);
        pending.delete(m.id);
        if (p) { if (m.ok) p.resolve(m.value); else p.reject(m.error); }
      } else if (m.t === "scanned") {
        const f = frames.get(m.id);
        frames.delete(m.id);
        f?.(m.text);
      } else if (m.t === "event") listeners.get(m.name)?.forEach((f) => f({ payload: m.ev }));
      else if (m.t === "stopped") stopped?.();
    };
    worker.onerror = (e) => {
      // the worker's script or the core itself broke: nothing more will answer
      dead = "Hubchat's core stopped (" + (e.message || "worker error") + "). Reload the page.";
      reject(dead);
      for (const p of pending.values()) p.reject(dead);
      pending.clear();
      for (const f of frames.values()) f(null);
      frames.clear();
    };
  });
  send({ t: "boot", deviceName: deviceName() });
  try {
    await ready;
  } catch (e) {
    worker.terminate();
    free();
    channel.close();
    throw sentence(e);
  }

  /** One command, to the core. */
  const call = <T>(cmd: string, args?: InvokeArgs): Promise<T> => {
    if (dead) return Promise.reject(dead);
    // bytes are not JSON: what carries them goes through `raw`
    if (args instanceof ArrayBuffer || ArrayBuffer.isView(args) || Array.isArray(args)) return Promise.reject(NOT_YET);
    return new Promise<T>((resolve, reject) => {
      const id = ++lastId;
      pending.set(id, { resolve: resolve as (v: unknown) => void, reject });
      send({ t: "call", id, cmd, args: args ?? {} });
    });
  };

  /** A command that takes or gives more than JSON (a chosen file in, bytes or a File out). */
  const raw = <T>(cmd: string, args: Record<string, unknown>, blob?: Blob): Promise<T> => {
    if (dead) return Promise.reject(dead);
    return new Promise<T>((resolve, reject) => {
      const id = ++lastId;
      pending.set(id, { resolve: resolve as (v: unknown) => void, reject });
      send({ t: "raw", id, cmd, args, blob });
    });
  };
  /** Keep a chosen file with the core: what stands for it from then on (the apps have a path). */
  const keepFile = (file: Blob, name: string): Promise<string> => raw<string>("hc_web_put", { name }, file);
  /** A picture's bytes, as the apps' commands answer them. */
  const picture = async (cmd: string, args: Record<string, unknown>): Promise<ArrayBuffer> => {
    const bytes = await raw<Uint8Array>(cmd, args);
    return bytes.buffer.slice(bytes.byteOffset, bytes.byteOffset + bytes.byteLength) as ArrayBuffer;
  };
  /** The apps open a saved file with the system's app. A page hands this browser's copy to the
   *  browser: the share sheet on an iPhone, a download elsewhere. */
  const openAttachment = async (args: Record<string, unknown>): Promise<void> => {
    const got = await raw<{ name: string; file: File }>("hc_web_file", args);
    await saveFile(new File([got.file], got.name, { type: got.file.type }));
  };

  // The apps write the recovery words file themselves (`hc_save_recovery`). A page hands it to the
  // browser, which shares or downloads only inside a click: so the file is fetched when the words
  // are shown, and Download finds it ready. It is the same secret the screen is already showing.
  let recoveryFile: RecoveryFile | null = null;
  /** The name the file was saved under; "" when the user closed the share sheet. */
  const saveRecovery = async (): Promise<string> => {
    const file = recoveryFile ?? await call<RecoveryFile>("hc_recovery_file");
    if (!(await saveText(file.name, file.text))) return "";
    await call("hc_recovery_saved");
    return file.name;
  };

  /** One camera frame, to the core: the text of the QR code in it, or null. */
  const readFrame = (width: number, height: number, grey: Uint8Array): Promise<string | null> => {
    if (dead) return Promise.reject(dead);
    return new Promise((resolve) => {
      const id = ++lastId;
      frames.set(id, resolve);
      const m: ToCore = { t: "scan", id, width, height, grey: grey.buffer as ArrayBuffer };
      worker.postMessage(m, [m.grey]);
    });
  };

  // The apps read and write the key file by its path. A page has the file the user chose, and hands
  // the one the core made to the browser.
  const importKeyFile = async (args: Record<string, unknown>): Promise<string> => {
    const text = await pickedText(String(args.source ?? ""), KEY_FILE_LIMIT);
    const address = await call<string>("hc_key_file_import", { text, passphrase: args.passphrase });
    keepStorage();
    return address;
  };
  const exportKeyFile = async (args: Record<string, unknown>): Promise<void> => {
    const file = await call<RecoveryFile>("hc_key_file_text", { passphrase: args.passphrase });
    if (!(await saveText(file.name, file.text))) throw "The key file wasn't saved.";
  };

  // Notifications (spec R4.6). The apps show them themselves; a browser shows only what a hub
  // pushes to its subscription. Turning them on asks the browser's permission, so it must run in
  // the tap; "Show message text" goes to the hubs with the subscription.
  const setNotifications = async (n: { enabled: boolean; preview: boolean; sound: boolean }): Promise<void> => {
    const now = await call<{ endpoint: string | null }>("hc_web_push");
    if (n.enabled && !now.endpoint) await enablePush(call, n.preview);
    else if (!n.enabled && now.endpoint) await disablePush(call, false);
    await call("hc_set_notifications", n);
  };

  /** One command of src/api.ts: the core's, but for what only the page can do. */
  const invoke = <T>(cmd: string, args?: InvokeArgs, options?: InvokeOptions): Promise<T> => {
    if (cmd === "hc_save_recovery") return saveRecovery() as Promise<T>;
    if (cmd === "hc_take_pending_chat") return Promise.resolve(takePendingChat()) as Promise<T>;
    if (cmd === "hc_set_notifications") return setNotifications((args ?? {}) as { enabled: boolean; preview: boolean; sound: boolean }) as Promise<T>;
    if (cmd === "hc_start_over") {
      // the hubs forget where this browser's notifications went, while there is still a key to ask with
      return disablePush(call, true).catch(() => { /* a hub that can't be told forgets nothing; the subscription is gone */ }).then(() => call<T>(cmd, args));
    }
    if (cmd === "hc_file_preview" || cmd === "hc_attachment_preview") return picture(cmd, (args ?? {}) as Record<string, unknown>) as Promise<T>;
    if (cmd === "hc_open_attachment") return openAttachment((args ?? {}) as Record<string, unknown>) as Promise<T>;
    if (cmd === "hc_save_pasted") {
      // a pasted picture: its bytes, and its name in a header (src/api.ts)
      const name = new Headers(options?.headers as HeadersInit | undefined).get("x-name") ?? "pasted.png";
      return keepFile(new Blob([args as Uint8Array<ArrayBuffer>]), name) as Promise<T>;
    }
    if (cmd === "hc_key_file_import") return importKeyFile((args ?? {}) as Record<string, unknown>) as Promise<T>;
    if (cmd === "hc_key_file_export") return exportKeyFile((args ?? {}) as Record<string, unknown>) as Promise<T>;
    const answer = call<T>(cmd, args);
    if (cmd === "hc_probe_hub") return (answer as Promise<Probe>).then(explain) as Promise<T>;
    if (cmd === "hc_recovery_words") {
      void answer.then(() => call<RecoveryFile>("hc_recovery_file")).then((f) => { recoveryFile = f; }, () => { /* Download asks again */ });
    } else if (cmd === "hc_create_identity" || cmd === "hc_restore_words" || cmd === "hc_link_confirm" || cmd === "hc_restore_qr") {
      void answer.then(keepStorage, () => { /* no identity came of it */ });
    }
    return answer;
  };
  const listen = <T>(event: string, handler: (e: { payload: T }) => void): Promise<() => void> => {
    const f = handler as (e: { payload: unknown }) => void;
    let set = listeners.get(event);
    if (!set) listeners.set(event, set = new Set());
    set.add(f);
    return Promise.resolve(() => { set.delete(f); });
  };

  const base = makeApi({ invoke, listen, mediaUrl: () => null });
  installWebNative({
    scanQr: () => scanQr(readFrame),
    pickKeyFile: () => pickFile(".hubchat-key,text/plain"),
    pickFiles: async () => {
      const paths: string[] = [];
      for (const file of await pickFilesToSend()) paths.push(await keepFile(file, file.name));
      return paths;
    },
    saveKeyFileTo: () => Promise.resolve(KEY_FILE_DEST),
    suggestedHubs,
    serverName,
    newerBuild,
  });
  const platform = layout();
  const api: Api = { ...base, state: async () => ({ ...(await base.state()), platform }) };
  void base.state().then((st) => {
    if (!st.me) return;
    keepStorage();
    // the browser's subscription and what the hubs were handed are brought in line again
    void syncPush(call).catch(() => { /* next start */ });
  }, () => { /* the UI's own start says why */ });
  watchPush(call);
  // a hub that was removed without being told still holds the subscription: a new one replaces it
  // the unread count on the app's icon and in the tab's title, while the app is open
  let counting: number | null = null;
  const countUnread = () => {
    if (counting !== null) return;
    counting = window.setTimeout(() => {
      counting = null;
      void call<{ unread?: number }[]>("hc_chats").then((chats) => showUnread(chats.reduce((n, c) => n + (c.unread ?? 0), 0)), () => { /* no identity yet */ });
    }, 400);
  };
  void listen<{ type?: string }>("hc", (e) => {
    const type = e.payload?.type;
    if (type === "hub") void syncPush(call, true).catch(() => { /* next start */ });
    else if (type === "chat" || type === "incoming") countUnread();
  });
  countUnread();
  if (pendingChat()) window.dispatchEvent(new Event("hc-pending"));

  // R1.7: a phone freezes a page in the background and its timers with it. Back in front, or back
  // online, every hub's sync starts again at once instead of waiting out a backoff.
  const wake = () => { if (document.visibilityState === "visible") void call("hc_retry_now").catch(() => { /* no identity yet */ }); };
  document.addEventListener("visibilitychange", wake);
  window.addEventListener("pageshow", wake);
  window.addEventListener("online", wake);

  const taken = new Promise<void>((resolve) => {
    channel.onmessage = (e: MessageEvent<{ t?: string }>) => {
      if (e.data?.t !== "takeover" || dead) return;
      dead = "Hubchat is open in another tab or window.";
      for (const p of pending.values()) p.reject(dead);
      pending.clear();
      for (const f of frames.values()) f(null);
      frames.clear();
      document.removeEventListener("visibilitychange", wake);
      window.removeEventListener("pageshow", wake);
      window.removeEventListener("online", wake);
      const done = () => { worker.terminate(); channel.close(); free(); resolve(); };
      // the engine stops and the database closes; a worker that does not answer is ended anyway
      const limit = setTimeout(done, 2000);
      stopped = () => { clearTimeout(limit); done(); };
      send({ t: "stop" });
    };
  });
  return { api, taken };
}
