//! Saved policy changes do not remove custody or unregister the identity.
use super::*;

impl Engine {
    pub fn set_hub_enabled(self: &Arc<Self>, url: &str, enabled: bool) -> Result<()> {
        let address = HubAddress::parse(url)?;
        let url = address.to_string();
        // Serialize policy and runtime changes in the same order as spawn.
        let mut hubs = self.hubs.lock().unwrap();
        self.store.set_hub_enabled(&url, enabled)?;
        if !enabled {
            if let Some(rt) = hubs.remove(&url) {
                rt.stop.cancel();
                rt.retry_now.notify_one();
                rt.kick.notify_one();
            }
        }
        let start = enabled && !hubs.contains_key(&url);
        drop(hubs);
        if start {
            let saved = self.store.hubs()?.into_iter().find(|h| h.url == url).ok_or_else(|| Error::Invalid("no such hub".into()))?;
            self.spawn_hub(address, saved.name, saved.max_attachment_bytes);
            self.store.wake_transfer_retries(Some(&url))?;
        }
        self.queue_changed.notify_one();
        self.body_changed.notify_one();
        self.host.event(Event::Hub { url: url.into() });
        self.host.event(Event::Directory);
        Ok(())
    }

    /// Use this for auxiliary work on a saved hub; a clone retains its generation.
    pub fn hub_client(&self, url: &str) -> Option<HubClient> {
        self.hubs.lock().unwrap().get(url).map(|h| h.client.clone())
    }

    fn set_status_for(&self, url: &str, stop: &CancelFlag, f: impl FnOnce(&mut HubStatus)) {
        let changed = {
            let mut hubs = self.hubs.lock().unwrap();
            match hubs.get_mut(url).filter(|h| h.stop.same(stop) && !stop.is_cancelled()) {
                Some(h) => { let before = format!("{:?}", h.status); f(&mut h.status); before != format!("{:?}", h.status) }
                None => false,
            }
        };
        if changed { self.host.event(Event::Hub { url: url.into() }); }
    }

    fn mark_for(&self, url: &str, stop: &CancelFlag, f: impl FnOnce(&mut HubStatus)) {
        if let Some(h) = self.hubs.lock().unwrap().get_mut(url).filter(|h| h.stop.same(stop) && !stop.is_cancelled()) {
            f(&mut h.status);
        }
    }
}
