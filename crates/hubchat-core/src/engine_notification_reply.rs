//! Notification replies enter the ordinary outbox once, even after worker replay.
use crate::{engine::{Engine, Event, BODY_INLINE_MAX}, Error, Result};

impl Engine {
    /// The account was captured when the notification was posted, never at send time.
    /// Admission is local; routing, delivery and retries remain the sender's job.
    pub fn queue_notification_reply(&self, owner: &str, id: &str, peer: &str, body: &str) -> Result<bool> {
        if owner != self.me().address() || self.stopped.load(std::sync::atomic::Ordering::Relaxed) {
            return Err(Error::Invalid("The notification belongs to an identity that is no longer connected.".into()));
        }
        let added = self.store.queue_notification_reply(owner, id, peer, body)?;
        if added {
            if body.len() as u64 > BODY_INLINE_MAX { self.host.file_work_changed(); }
            self.host.event(Event::Chat { peer: peer.into() });
        }
        self.queue_changed.notify_one();
        Ok(added)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{engine::Host, hub::Profile, Identity, store::Store};
    use std::sync::{Arc, Mutex};
    struct HostStub(Mutex<Vec<Event>>);
    impl Host for HostStub {
        fn event(&self, event: Event) { self.0.lock().unwrap().push(event); }
        fn open_source(&self, _: &str) -> std::io::Result<std::fs::File> { unreachable!() }
        fn download_dir(&self) -> std::path::PathBuf { std::path::PathBuf::new() }
    }
    #[test]
    fn engine_signals_once_and_refuses_a_stopped_or_changed_identity() {
        let store = Arc::new(Store::open_in_memory().unwrap());
        let me = Identity::generate("reply-proof").unwrap();
        let owner = me.address();
        store.set_meta("identity.address", &owner).unwrap();
        let host = Arc::new(HostStub(Mutex::new(vec![])));
        let engine = Engine::new(store.clone(), me, Profile::default(), host.clone());
        let id = "bb05866b-b96c-46c0-ae3f-236b3dd65fa2";
        assert!(engine.queue_notification_reply(&owner,id,"pat.abcdef","hello").unwrap());
        assert!(!engine.queue_notification_reply(&owner,id,"pat.abcdef","hello").unwrap());
        assert_eq!(host.0.lock().unwrap().len(), 1);
        store.set_meta("identity.address", "another.ffffff").unwrap();
        assert!(engine.queue_notification_reply(&owner,id,"pat.abcdef","hello").is_err());
        engine.stopped.store(true, std::sync::atomic::Ordering::Relaxed);
        store.set_meta("identity.address", &owner).unwrap();
        assert!(engine.queue_notification_reply(&owner,id,"pat.abcdef","hello").is_err());
    }
}
