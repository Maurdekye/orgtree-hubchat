//! Hubchat Web's shell: what `src-tauri` is to the apps, for a browser.
//!
//! - `shell`: the commands the UI calls, answered from the core. It knows
//!   nothing of a browser, so its tests run natively.

pub mod shell;
