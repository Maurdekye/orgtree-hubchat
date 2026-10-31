//! Our status (hub feature `status`, Mail hub 2.3.0; Orgtree docket
//! org-presence-busy-orange-when-no-org-inbox-holde, user 2026-10-31 08:52Z:
//! "users on hubchat should be able to manually set their status (do not
//! disturb prevents notifications from appearing on their desktop /
//! phone)"). The person picks Automatic, Online, Busy or Do not disturb. It
//! is theirs, not this device's: each hub keeps it (`POST /api/status`) and
//! says it back in every sync answer, so all their devices know it, and it
//! shows to others only while one of them is connected (09:03Z, "Offline
//! for both"; the hubs' rule).
//!
//! **Kept here** as `status.mine`: what this device shows, and acts on: on
//! Do not disturb the shell shows no notification and plays no sound; mail,
//! unread counts and badges go on as ever.
//!
//! **Told**: a pick here is owed to every saved hub (`status.owed.<url>`).
//! Each hub's sync tells it before its next request, and a pick wakes a
//! parked sync to do so at once (`status_wake`), so every answer after that
//! carries it. An answer from before is not taken as a change: while a hub
//! is owed our pick, only its own word ends the wait. A hub without the
//! feature, or one that refuses it, is owed nothing.
//!
//! **Heard**: each hub's last word is kept (`status.said.<url>`). A word
//! that changed is a pick on another of our devices: this device takes it,
//! and a hub still owed the pick it replaces is owed this one instead. A
//! first word (a new device, a new hub, a reset) is taken when it is a
//! choice; Automatic from a hub that never heard ours is told ours.

use super::{Engine, Event};
use crate::store::{status_owed_key, status_said_key, Store};
use crate::{Error, HubClient, Result};

/// The four choices, as the hubs have them.
pub const STATUSES: [&str; 4] = ["auto", "online", "busy", "dnd"];

const MINE: &str = "status.mine";

/// Our status as this device knows it: Automatic until one is picked here or
/// heard from a hub. The shell reads it for each notification.
pub fn chosen_status(store: &Store) -> String {
    store.meta(MINE).ok().flatten().filter(|s| STATUSES.contains(&s.as_str())).unwrap_or_else(|| "auto".into())
}

impl Engine {
    pub fn status(&self) -> String {
        chosen_status(&self.store)
    }

    /// The person picks their status here: kept, owed to every saved hub,
    /// and told at once to each connected one with the feature (its parked
    /// sync wakes for it); the others are told when they connect.
    pub fn set_status(&self, status: &str) -> Result<()> {
        if !STATUSES.contains(&status) {
            return Err(Error::Invalid(format!("\"{status}\" is not a status: auto, online, busy or dnd")));
        }
        self.store.set_meta(MINE, status)?;
        for h in self.store.hubs()? {
            self.store.set_meta(&status_owed_key(&h.url), status)?;
        }
        self.host.event(Event::Status);
        self.status_wake.notify_waiters();
        Ok(())
    }

    /// A status heard from a hub becomes ours: kept, shown, and owed instead
    /// to each hub still owed the one it replaces.
    fn adopt_status(&self, status: &str) -> Result<()> {
        self.store.set_meta(MINE, status)?;
        for h in self.store.hubs()? {
            let owed = status_owed_key(&h.url);
            if self.store.meta(&owed)?.is_some() {
                self.store.set_meta(&owed, status)?;
            }
        }
        self.host.event(Event::Status);
        Ok(())
    }

    /// A hub's word on our status (`status` in its sync answer); `reset`
    /// when the answer started this device over there (a re-created hub
    /// database): a first word.
    pub(super) fn hub_said_status(&self, url: &str, said: &str, reset: bool) -> Result<()> {
        if !STATUSES.contains(&said) {
            return Ok(());
        }
        let said_key = status_said_key(url);
        if let Some(owed) = self.store.meta(&status_owed_key(url))? {
            // our pick is on its way: only its own word ends the wait
            if owed == said {
                self.store.delete_meta(&status_owed_key(url))?;
                self.store.set_meta(&said_key, said)?;
            }
            return Ok(());
        }
        let before = if reset { None } else { self.store.meta(&said_key)? };
        self.store.set_meta(&said_key, said)?;
        let mine = self.status();
        match before {
            Some(b) if b == said => {}
            // picked on another of our devices
            Some(_) => {
                if said != mine {
                    self.adopt_status(said)?;
                }
            }
            // a first word: a choice made elsewhere is taken
            None if said != "auto" => {
                if said != mine {
                    self.adopt_status(said)?;
                }
            }
            // Automatic from a hub that never heard ours: it is told ours
            None => {
                if mine != "auto" {
                    self.store.set_meta(&status_owed_key(url), &mine)?;
                }
            }
        }
        Ok(())
    }

    /// Tell the hub the status it is owed, if any (its sync calls this
    /// before each request). Taken: owed no longer (unless a newer pick came
    /// meanwhile), and its word is ours. Refused (a 4xx), or a hub without
    /// the feature: owed no longer. A server error waits for the next turn; a
    /// network failure ends the session, as any does.
    pub(super) async fn tell_status(&self, client: &HubClient, url: &str, features: &[String]) -> Result<()> {
        let owed_key = status_owed_key(url);
        let Some(owed) = self.store.meta(&owed_key)? else {
            return Ok(());
        };
        if !features.iter().any(|f| f == "status") {
            return self.store.delete_meta(&owed_key);
        }
        match client.set_status(&self.me, &owed).await {
            Ok(()) => {
                if self.store.meta(&owed_key)?.as_deref() == Some(owed.as_str()) {
                    self.store.delete_meta(&owed_key)?;
                }
                self.store.set_meta(&status_said_key(url), &owed)
            }
            Err(e) => match e.status() {
                Some(400..=499) => self.store.delete_meta(&owed_key),
                Some(_) => Ok(()),
                None => Err(e),
            },
        }
    }
}
