// The core as wasm-bindgen builds it from crates/hubchat-web (src/wasm.rs).
// The two files are build output (tools/web-build.mjs), found through the
// `@hubchat-core` alias of vite.web.config.ts; these are their types, so the
// project type-checks without a build of the core.
declare module "@hubchat-core" {
  /** The worker's side of the core (crates/hubchat-web/src/wasm.rs, `boot`). */
  export interface CoreHost {
    /** What this browser is called among the user's devices. */
    deviceName: string;
    /** An event for the page: the apps' event name and its payload as JSON. */
    emit(name: string, json: string): void;
    loadIdentity(): Promise<{ id: string; secret: string } | null>;
    saveIdentity(id: string, secret: string): Promise<void>;
    clearIdentity(): Promise<void>;
  }
  /** Load the WebAssembly module (wasm-bindgen's own start). */
  export default function init(options: { module_or_path: string | URL }): Promise<unknown>;
  /** Start the core in this worker: the storage, the database, the identity this browser keeps. */
  export function boot(host: CoreHost): Promise<void>;
  /** One command: its name and its arguments as JSON; the answer as JSON. Rejects with a sentence. */
  export function call(cmd: string, args: string): Promise<string>;
  /** The text of a QR code in a frame of `width` x `height` grey pixels. Undefined: none in it. */
  export function scan(width: number, height: number, grey: Uint8Array): string | undefined;
  /** Stop the engine. */
  export function stop(): void;
}
