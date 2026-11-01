//! Translation between people (Mail hub 2.4.0, feature `translate`).
//! Not built yet: no language, and every notification at once.

use super::{Engine, Event};
use crate::hub_v2::SyncedMessage;
use crate::store::Store;
use crate::{HubClient, Result};

pub fn chosen_language(_store: &Store) -> String {
    String::new()
}

impl Engine {
    pub fn language(&self) -> String {
        String::new()
    }

    pub fn set_language(&self, _language: &str) -> Result<()> {
        let _ = Event::Status;
        Ok(())
    }

    pub(super) fn hub_said_language(&self, _url: &str, _said: Option<&str>, _reset: bool) -> Result<()> {
        Ok(())
    }

    pub(super) async fn tell_language(&self, _client: &HubClient, _url: &str, _features: &[String]) -> Result<()> {
        Ok(())
    }

    pub(super) fn notice_incoming(&self, _url: &str, m: &SyncedMessage, quiet: bool) {
        // markdown syntax is for the chat; a notification gets plain words
        let preview = crate::text::plain_preview(&m.env.body, 140);
        self.host.event(self.incoming_event(m.env.from.clone(), m.env.id.clone(), preview, quiet));
    }

    pub(super) fn translation_came(&self) {}

    pub(super) fn release_held(&self, _now_ms: u64) -> Option<u64> {
        None
    }
}
