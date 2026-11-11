// What the page and the core's worker say to each other (spec R1.6). Plain
// objects over postMessage; nothing here knows what a command means.

/** The page to the worker. */
export type ToCore =
  /** Start the core. `deviceName`: what this browser is called among the user's devices. */
  | { t: "boot"; deviceName: string }
  /** One command of src/api.ts: its name and its arguments, as the apps' invoke() sends them. */
  | { t: "call"; id: number; cmd: string; args: unknown }
  /** A command that takes or gives more than JSON (wasm.rs, `raw`): `blob` is a file the user chose;
   *  the answer may be bytes or the browser's own File. */
  | { t: "raw"; id: number; cmd: string; args: unknown; blob?: Blob }
  /** One camera frame to read a QR code from (scan.ts): its grey pixels, one byte each. */
  | { t: "scan"; id: number; width: number; height: number; grey: ArrayBuffer }
  /** Stop the engine and say `stopped`: another tab takes over, or the page goes away. */
  | { t: "stop" };

/** The worker to the page. */
export type FromCore =
  | { t: "ready" }
  /** The core did not start; `error` is a sentence for the user. */
  | { t: "failed"; error: string }
  | { t: "ret"; id: number; ok: true; value: unknown }
  | { t: "ret"; id: number; ok: false; error: string }
  /** An event of the core: `name` is the apps' event name (`hc`), `ev` its payload. */
  | { t: "event"; name: string; ev: unknown }
  /** What a `scan` frame held: the QR code's text, or null. */
  | { t: "scanned"; id: number; text: string | null }
  | { t: "stopped" };

/** An error as a sentence: invoke() rejects with a string, as Tauri's does. */
export function sentence(e: unknown): string {
  if (typeof e === "string") return e;
  if (e instanceof Error) return e.message;
  try { return String(e); } catch { return "unknown error"; }
}
