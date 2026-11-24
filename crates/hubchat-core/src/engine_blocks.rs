//! Blocking (hub feature `blocking`, Mail hub 2.6.0; Orgtree docket
//! blocking-contacts-on-the-hub-hubchat-1-4-0-orgtr, user 2026-11-24 18:56Z:
//! "users or orgs may choose to block a specific contact"). The hubs keep
//! and enforce it: a blocked contact's sends to us are refused there, and
//! (two-way, user 19:06Z) so are ours to them.
//!
//! **Picked here**: a block or unblock is owed to every saved hub
//! (`blocks.owed.<url>`, `{address: on}`) and shows at once (the directory
//! applies what is owed). Each hub's sync tells it before its next request,
//! and a pick wakes a parked sync to do so now (`status_wake`). A hub
//! without the feature, or one that refuses the change, is owed nothing.
//!
//! **Heard**: each sync answer may carry `blocks` (whom we block there,
//! picked on any of our devices) and `blocked_by` (who blocks us there:
//! the user's "Show it in their DM", 19:06Z). Both are kept per hub
//! (`blocks.mine.<url>`, `blocks.by.<url>`) and merged in the directory.

use std::collections::BTreeMap;

use super::{Engine, Event};
use crate::store::{blocks_by_key, blocks_mine_key, blocks_owed_key};
use crate::{Error, HubClient, Result};

impl Engine {
    /// Whom we block, over every saved hub, with what is still owed.
    pub fn blocked(&self) -> Result<Vec<String>> {
        Ok(self.store.block_sets()?.0.into_iter().collect())
    }

    /// Block (`on`) or unblock an address: owed to every saved hub, shown at
    /// once, told now to each connected hub with the feature.
    pub fn set_blocked(&self, address: &str, on: bool) -> Result<()> {
        let address = address.trim().trim_start_matches('@');
        if address.is_empty() || address == self.me.address().as_str() {
            return Err(Error::Invalid("that address can't be blocked".into()));
        }
        for h in self.store.hubs()? {
            let key = blocks_owed_key(&h.url);
            let mut owed = owed_map(self.store.meta(&key)?);
            owed.insert(address.to_string(), on);
            self.store.set_meta(&key, &serde_json::to_string(&owed).unwrap_or_default())?;
        }
        self.host.event(Event::Directory);
        self.status_wake.notify_waiters();
        Ok(())
    }

    /// A hub's word on blocks (its sync answer): kept, and the directory
    /// told when either list moved.
    pub(super) fn hub_said_blocks(&self, url: &str, mine: Option<&[String]>, by: Option<&[String]>) -> Result<()> {
        let mut moved = false;
        for (key, list) in [(blocks_mine_key(url), mine), (blocks_by_key(url), by)] {
            let Some(list) = list else { continue };
            let text = serde_json::to_string(list).unwrap_or_default();
            if self.store.meta(&key)?.as_deref() != Some(text.as_str()) {
                self.store.set_meta(&key, &text)?;
                moved = true;
            }
        }
        if moved {
            self.host.event(Event::Directory);
        }
        Ok(())
    }

    /// Tell the hub the blocks it is owed (its sync calls this before each
    /// request). Taken: owed no longer (unless picked again meanwhile), and
    /// the hub's list is kept. Refused (a 4xx), or a hub without the
    /// feature: owed no longer. A server error waits for the next turn; a
    /// network failure ends the session, as any does.
    pub(super) async fn tell_blocks(&self, client: &HubClient, url: &str, features: &[String]) -> Result<()> {
        let key = blocks_owed_key(url);
        let owed = owed_map(self.store.meta(&key)?);
        if owed.is_empty() {
            return Ok(());
        }
        if !features.iter().any(|f| f == "blocking") {
            return self.store.delete_meta(&key);
        }
        for (address, on) in owed {
            let done = match client.set_block(&self.me, &address, on).await {
                Ok(list) => {
                    self.store.set_meta(&blocks_mine_key(url), &serde_json::to_string(&list).unwrap_or_default())?;
                    true
                }
                Err(e) => match e.status() {
                    Some(400..=499) => true,
                    Some(_) => false,
                    None => return Err(e),
                },
            };
            if done {
                let mut now = owed_map(self.store.meta(&key)?);
                if now.get(&address) == Some(&on) {
                    now.remove(&address);
                }
                if now.is_empty() {
                    self.store.delete_meta(&key)?;
                } else {
                    self.store.set_meta(&key, &serde_json::to_string(&now).unwrap_or_default())?;
                }
            }
        }
        self.host.event(Event::Directory);
        Ok(())
    }
}

fn owed_map(v: Option<String>) -> BTreeMap<String, bool> {
    v.and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
}
