//! Hubchat's platform-independent core.
//!
//! Everything that talks to a mail hub lives here, so the Windows and Android
//! shells share one implementation. The UI never builds hub requests itself.

pub mod engine;
pub mod error;
pub mod hub;
pub mod hub_v2;
pub mod identity;
pub mod link;
pub mod recovery;
pub mod store;
pub mod text;

pub use error::{Error, Result};
pub use hub::{HubAddress, HubClient};
pub use identity::Identity;
