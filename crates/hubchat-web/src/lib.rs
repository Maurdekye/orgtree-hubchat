//! Hubchat Web's shell: what `src-tauri` is to the apps, for a browser.
//!
//! The same core (`hubchat-core`) runs here as WebAssembly, in one dedicated
//! worker of the page: its database is a file in the origin's private file
//! system, its requests are the browser's `fetch`, and the identity's key
//! stays in this browser. The server that serves the page never sees a key,
//! a message or a hub address.
//!
//! - `shell`: the commands the UI calls, answered from the core. It knows
//!   nothing of a browser, so its tests run natively.
//! - `files`: the files of chats (chosen, sent, shown, saved), with the
//!   core's own file layer; its tests run natively too.
//! - `wasm` (browser builds only): starts the shell in the worker and passes
//!   commands, events and the key between it and the worker's script
//!   (`src/web/core.worker.ts`).

pub mod files;
pub mod link;
pub mod scan;
pub mod shell;

#[cfg(target_arch = "wasm32")]
mod wasm;
