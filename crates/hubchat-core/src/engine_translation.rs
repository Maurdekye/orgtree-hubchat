//! Translation between people (Mail hub 2.4.0, feature `translate`; Orgtree
//! docket mail-hub-hubchat-automatic-translation-between-u; the user's
//! answers 2026-11-01 12:15Z and "Build it" 12:16Z; design
//! hubchat-translation-design-1-3-0).
//!
//! **The language we read in** (`language.mine`: a BCP 47 tag, "" for none)
//! is the person's, not this device's, and is kept like our status
//! (engine_status): a pick here is owed to every saved hub
//! (`language.owed.<url>`); each hub's sync tells it before its next request
//! (`POST /api/profile {language}`), only to a hub with the feature, and a
//! pick wakes a parked sync to do so at once (`status_wake`). Each hub's last
//! word (`language` in its sync answers) is kept (`language.said.<url>`): a
//! word that changed is a pick on another of our devices, and this device
//! takes it. Nobody else is told it: the hubs keep it private.
//!
//! **A translation** comes in our own sync only, on the message
//! (`hub_v2::Translation`), and again when its outcome is known; the store
//! keeps the last one told, as shown beside the message.
//!
//! **Notifications wait for it** (user: "Wait up to 4 s"): new mail whose
//! translation is on its way is held until it comes, or until 4 s after the
//! hub received the message, then notified, with the translation when there
//! is one. Mail that already waited at the hub (a push wake) is not held
//! again. The window shows the original at once either way.

use std::time::Duration;

use super::{hub_ms, unix_ms, Engine, Event};
use crate::hub_v2::{is_language_tag, SyncedMessage};
use crate::store::{language_owed_key, language_said_key, Store};
use crate::{Error, HubClient, Result};

const MINE: &str = "language.mine";

/// How long a notification waits for its translation, from when the hub
/// received the message (user: "Wait up to 4 s").
pub const NOTICE_WAIT_MS: i64 = 4000;

/// The language we read in as this device knows it: "" (none) until one is
/// picked here or heard from a hub.
pub fn chosen_language(store: &Store) -> String {
    store.meta(MINE).ok().flatten().filter(|l| l.is_empty() || is_language_tag(l)).unwrap_or_default()
}

/// New mail whose notification waits for its translation.
pub(super) struct Held {
    peer: String,
    id: String,
    quiet: bool,
    until_ms: u64,
}

/// What a notification says for a message: its translation when there is
/// one, else its text; plain words (markdown syntax is for the chat).
fn notice_text(body: &str, translation: Option<&crate::hub_v2::Translation>) -> String {
    let text = translation.filter(|t| t.status == "translated").and_then(|t| t.body.as_deref()).unwrap_or(body);
    crate::text::plain_preview(text, 140)
}

impl Engine {
    pub fn language(&self) -> String {
        chosen_language(&self.store)
    }

    /// The person picks the language they read in here ("" for none): kept,
    /// owed to every saved hub, and told at once to each connected one with
    /// the feature; the others are told when they connect.
    pub fn set_language(&self, language: &str) -> Result<()> {
        if !language.is_empty() && !is_language_tag(language) {
            return Err(Error::Invalid(format!("\"{language}\" is not a language tag")));
        }
        self.store.set_meta(MINE, language)?;
        for h in self.store.hubs()? {
            self.store.set_meta(&language_owed_key(&h.url), language)?;
        }
        self.host.event(Event::Status);
        self.status_wake.notify_waiters();
        Ok(())
    }

    /// A language heard from a hub becomes ours: kept, shown, and owed
    /// instead to each hub still owed the one it replaces.
    fn adopt_language(&self, language: &str) -> Result<()> {
        self.store.set_meta(MINE, language)?;
        for h in self.store.hubs()? {
            let owed = language_owed_key(&h.url);
            if self.store.meta(&owed)?.is_some() {
                self.store.set_meta(&owed, language)?;
            }
        }
        self.host.event(Event::Status);
        Ok(())
    }

    /// A hub's word on our language (`language` in its sync answer: None for
    /// none); `reset` when the answer started this device over there.
    pub(super) fn hub_said_language(&self, url: &str, said: Option<&str>, reset: bool) -> Result<()> {
        let said = said.unwrap_or("");
        if !said.is_empty() && !is_language_tag(said) {
            return Ok(());
        }
        let said_key = language_said_key(url);
        if let Some(owed) = self.store.meta(&language_owed_key(url))? {
            // our pick is on its way: only its own word ends the wait
            if owed == said {
                self.store.delete_meta(&language_owed_key(url))?;
                self.store.set_meta(&said_key, said)?;
            }
            return Ok(());
        }
        let before = if reset { None } else { self.store.meta(&said_key)? };
        self.store.set_meta(&said_key, said)?;
        let mine = self.language();
        match before {
            Some(b) if b == said => {}
            // picked on another of our devices
            Some(_) => {
                if said != mine {
                    self.adopt_language(said)?;
                }
            }
            // a first word: a choice made elsewhere is taken
            None if !said.is_empty() => {
                if said != mine {
                    self.adopt_language(said)?;
                }
            }
            // none from a hub that never heard ours: it is told ours
            None => {
                if !mine.is_empty() {
                    self.store.set_meta(&language_owed_key(url), &mine)?;
                }
            }
        }
        Ok(())
    }

    /// Tell the hub the language it is owed, if any (its sync calls this
    /// before each request). Taken: owed no longer (unless a newer pick came
    /// meanwhile), and its word is ours. Refused (a 4xx), or a hub without
    /// the feature: owed no longer. A server error waits for the next turn; a
    /// network failure ends the session, as any does.
    pub(super) async fn tell_language(&self, client: &HubClient, url: &str, features: &[String]) -> Result<()> {
        let owed_key = language_owed_key(url);
        let Some(owed) = self.store.meta(&owed_key)? else {
            return Ok(());
        };
        if !features.iter().any(|f| f == "translate") {
            return self.store.delete_meta(&owed_key);
        }
        match client.set_language(&self.me, &owed).await {
            Ok(()) => {
                if self.store.meta(&owed_key)?.as_deref() == Some(owed.as_str()) {
                    self.store.delete_meta(&owed_key)?;
                }
                self.store.set_meta(&language_said_key(url), &owed)
            }
            Err(e) => match e.status() {
                Some(400..=499) => self.store.delete_meta(&owed_key),
                Some(_) => Ok(()),
                None => Err(e),
            },
        }
    }

    /// New mail from someone else (already stored): notified now, or held
    /// while its translation is on its way, until 4 s after the hub received
    /// it.
    pub(super) fn notice_incoming(&self, url: &str, m: &SyncedMessage, quiet: bool) {
        if m.translation.as_ref().is_some_and(|t| t.status == "pending") {
            let now = unix_ms() as i64;
            let came = hub_ms(&m.env.received_at).map(|t| t - self.offset(url)).unwrap_or(now);
            let until = (came.min(now) + NOTICE_WAIT_MS).max(0);
            if until > now {
                self.held.lock().unwrap().push(Held {
                    peer: m.env.from.clone(),
                    id: m.env.id.clone(),
                    quiet,
                    until_ms: until as u64,
                });
                self.translation_wake.notify_one();
                return;
            }
        }
        let preview = notice_text(&m.env.body, m.translation.as_ref());
        self.host.event(self.incoming_event(m.env.from.clone(), m.env.id.clone(), preview, quiet));
    }

    /// A translation's outcome came for mail already here: a held
    /// notification may go.
    pub(super) fn translation_came(&self) {
        self.translation_wake.notify_one();
    }

    /// Notify each held message whose translation came or whose wait is
    /// over (one deleted meanwhile is not); when the next wait ends.
    pub(super) fn release_held(&self, now_ms: u64) -> Option<u64> {
        let held = std::mem::take(&mut *self.held.lock().unwrap());
        let mut keep = Vec::new();
        for h in held {
            let msg = match self.store.message(&h.id) {
                Ok(Some(msg)) => msg,
                Ok(None) => continue,
                Err(_) if h.until_ms > now_ms => {
                    keep.push(h);
                    continue;
                }
                Err(_) => continue,
            };
            let waiting = msg.translation.as_ref().is_some_and(|t| t.status == "pending");
            if waiting && h.until_ms > now_ms {
                keep.push(h);
                continue;
            }
            let preview = notice_text(&msg.body, msg.translation.as_ref());
            self.host.event(self.incoming_event(h.peer, h.id, preview, h.quiet));
        }
        let next = keep.iter().map(|h| h.until_ms).min();
        self.held.lock().unwrap().extend(keep);
        next
    }

    /// Runs for the engine's life: releases held notifications as their
    /// translations come or their waits end.
    pub(super) async fn held_loop(&self) {
        loop {
            if self.stopped.load(std::sync::atomic::Ordering::Relaxed) {
                return;
            }
            let wait = match self.release_held(unix_ms()) {
                Some(at) => at.saturating_sub(unix_ms()).clamp(1, 30_000),
                None => 30_000,
            };
            tokio::select! {
                _ = tokio::time::sleep(Duration::from_millis(wait)) => {}
                _ = self.translation_wake.notified() => {}
            }
        }
    }
}
