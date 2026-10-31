//! Our status (hub feature `status`, Mail hub 2.3.0): RED. The shape the
//! tests use, doing nothing yet: nothing kept, owed, told or heard.
#![allow(dead_code)]

use super::Engine;
use crate::store::Store;
use crate::{HubClient, Result};

/// The four choices, as the hubs have them.
pub const STATUSES: [&str; 4] = ["auto", "online", "busy", "dnd"];

pub fn chosen_status(_store: &Store) -> String {
    "auto".into()
}

impl Engine {
    pub fn status(&self) -> String {
        chosen_status(&self.store)
    }

    pub fn set_status(&self, _status: &str) -> Result<()> {
        Ok(())
    }

    pub(super) fn hub_said_status(&self, _url: &str, _said: &str, _reset: bool) -> Result<()> {
        Ok(())
    }

    pub(super) async fn tell_status(&self, _client: &HubClient, _url: &str, _features: &[String]) -> Result<()> {
        Ok(())
    }
}
