//! Hubchat's platform-independent core.
//!
//! Everything that talks to a mail hub lives here, so the Windows and Android
//! shells share one implementation. The UI never builds hub requests itself.

pub mod diag;
pub mod engine;
pub mod door;
pub mod error;
pub mod hub;
pub mod hub_v2;
pub mod identity;
pub mod link;
pub mod media;
pub mod recovery;
pub mod rt;
pub mod setup;
pub mod store;
#[cfg(not(target_arch = "wasm32"))]
pub mod stream;
// Hubchat Web: the same names, long polling only
#[cfg(target_arch = "wasm32")]
#[path = "stream_web.rs"]
pub mod stream;
// Hubchat Web's files: compiled everywhere so its tests run natively; used by rt::fs in a browser
pub mod webfs;
pub mod questions;
pub mod text;

pub use error::{Error, Result};
pub use hub::{HubAddress, HubClient};
pub use identity::Identity;

mod staging;

pub mod download;

pub mod notification_reply;
pub mod pictures;
pub mod picture_image;
