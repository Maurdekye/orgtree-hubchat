//! Notifications for Hubchat Web (hub feature `webpush`, Mail hub 2.5.1):
//! a browser's push subscription, registered with every hub.
//!
//! A phone freezes a page in the background, so Hubchat Web hears of new
//! mail only through the browser's own push. The browser makes one
//! subscription (and the key that signs for it); each hub is handed it, and
//! sends its notifications there itself.
//!
//! **Kept here** as `webpush.subscription`: what the page handed over, or
//! nothing when notifications are off.
//!
//! **Told**: each hub's sync compares what it last heard (`webpush.said.
//! <url>`) with that, before each request, and registers or removes what
//! differs. So a hub added later, a hub that was unreachable, and one that
//! was updated meanwhile all get it without anyone asking again; a change
//! wakes parked syncs (`status_wake`) to do it at once.
//!
//! The apps do not use this: Android registers a UnifiedPush capability
//! through `Engine::register_push`.

use super::{Engine, Event};
use crate::store::webpush_said_key;
use crate::{HubClient, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

const SUBSCRIPTION: &str = "webpush.subscription";
const RESUBSCRIBE: &str = "webpush.resubscribe";

/// A browser's push subscription as a hub takes it. No `Debug`: the
/// endpoint, `auth` and `vapid` are capabilities.
#[derive(Clone, Serialize, Deserialize, PartialEq)]
pub struct WebPush {
    pub endpoint: String,
    pub p256dh: String,
    pub auth: String,
    /// The private half of the key the browser subscribed under: each hub
    /// signs its pushes with it.
    pub vapid: String,
    /// The web app's own address, which a notification opens.
    pub navigate: String,
    /// Whether a notification may carry the sender and the text.
    pub preview: bool,
}

impl WebPush {
    /// What tells one registration from another, without its secrets.
    fn mark(&self) -> String {
        let json = serde_json::to_string(self).unwrap_or_default();
        hex::encode(&Sha256::digest(json.as_bytes())[..8])
    }
}

/// What a hub has of this browser's notifications, for Settings.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct WebPushHub {
    pub url: String,
    pub name: String,
    /// `registered`, `needs_update` (a hub without the feature), `refused`
    /// (it said no: its endpoint policy, usually) or `waiting`.
    pub state: &'static str,
}

impl Engine {
    /// The subscription notifications go to; None while they are off.
    pub fn web_push(&self) -> Option<WebPush> {
        self.store.meta(SUBSCRIPTION).ok().flatten().and_then(|json| serde_json::from_str(&json).ok())
    }

    /// The page turned notifications on (its subscription), changed what
    /// they may show, or turned them off (None). Every hub is told at its
    /// next turn; parked syncs wake for it.
    pub fn set_web_push(&self, subscription: Option<&WebPush>) -> Result<()> {
        match subscription {
            Some(s) => self.store.set_meta(SUBSCRIPTION, &serde_json::to_string(s).map_err(|e| crate::Error::Invalid(e.to_string()))?)?,
            None => self.store.delete_meta(SUBSCRIPTION)?,
        }
        self.store.delete_meta(RESUBSCRIBE)?;
        self.status_wake.notify_waiters();
        Ok(())
    }

    /// Each saved hub and what it has of this browser's notifications.
    pub fn web_push_hubs(&self) -> Result<Vec<WebPushHub>> {
        let mark = self.web_push().map(|s| s.mark());
        Ok(self
            .store
            .hubs()?
            .into_iter()
            .map(|h| {
                let said = self.store.meta(&webpush_said_key(&h.url)).ok().flatten();
                let state = match (&mark, said.as_deref()) {
                    (Some(m), Some(s)) if s.strip_prefix("registered:") == Some(m.as_str()) => "registered",
                    (Some(m), Some(s)) if s.strip_prefix("refused:") == Some(m.as_str()) => "refused",
                    (Some(_), Some("needs_update")) => "needs_update",
                    _ => "waiting",
                };
                WebPushHub { url: h.url, name: h.name, state }
            })
            .collect())
    }

    /// A hub that was removed may still hold the subscription (it could not
    /// be told to forget it): the page must make a new one, so the old one
    /// is dead, and register that with the hubs that remain.
    pub fn web_push_must_resubscribe(&self) -> bool {
        self.store.meta(RESUBSCRIBE).ok().flatten().is_some()
    }

    /// Before a hub is removed: take the subscription off it. Not confirmed
    /// (unreachable, or it failed): `web_push_must_resubscribe`.
    pub(super) async fn forget_web_push(&self, client: &HubClient, url: &str) {
        let held = self.store.meta(&webpush_said_key(url)).ok().flatten().is_some_and(|s| s.starts_with("registered:"));
        if !held {
            return;
        }
        if client.unregister_push(&self.me, &self.device().0).await.is_err() {
            let _ = self.store.set_meta(RESUBSCRIBE, "yes");
            self.host.event(Event::Hub { url: url.to_string() });
        }
    }

    /// Bring the hub's copy in line with ours (its sync calls this before
    /// each request). Nothing to do costs two reads. A network failure ends
    /// the session, as any does; a server error waits for the next turn.
    pub(super) async fn tell_web_push(&self, client: &HubClient, url: &str, features: &[String]) -> Result<()> {
        let said_key = webpush_said_key(url);
        let said = self.store.meta(&said_key)?;
        let Some(subscription) = self.web_push() else {
            // notifications are off: a hub that holds a registration drops it
            if said.as_deref().is_some_and(|s| s.starts_with("registered:")) {
                match client.unregister_push(&self.me, &self.device().0).await {
                    Ok(()) => {}
                    Err(e) if matches!(e.status(), Some(400..=499)) => {}
                    Err(e) if e.status().is_some() => return Ok(()),
                    Err(e) => return Err(e),
                }
            }
            if said.is_some() {
                self.store.delete_meta(&said_key)?;
                self.host.event(Event::Hub { url: url.to_string() });
            }
            return Ok(());
        };
        let mark = subscription.mark();
        let settled = |word: &str| said.as_deref() == Some(format!("{word}:{mark}").as_str());
        if settled("registered") || settled("refused") {
            return Ok(());
        }
        let now = if !features.iter().any(|f| f == "webpush") {
            "needs_update".to_string()
        } else {
            match client.register_web_push(&self.me, &self.device().0, &subscription).await {
                Ok(()) => format!("registered:{mark}"),
                // the hub knows this device from its first sync on: asked again after it
                Err(e) if e.status() == Some(409) => return Ok(()),
                Err(e) if matches!(e.status(), Some(400..=499)) => format!("refused:{mark}"),
                Err(e) if e.status().is_some() => return Ok(()),
                Err(e) => return Err(e),
            }
        };
        if said.as_deref() != Some(now.as_str()) {
            self.store.set_meta(&said_key, &now)?;
            self.host.event(Event::Hub { url: url.to_string() });
        }
        Ok(())
    }
}
