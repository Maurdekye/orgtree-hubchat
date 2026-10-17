//! Notification replies enter the ordinary outbox once, even after worker replay.
use crate::{engine::{Engine, Event, BODY_INLINE_MAX}, Error, Result};

impl Engine {
    /// Wait for this reply, not unrelated hubs or messages. Never changes read state.
    pub async fn check_notification_reply(&self, owner: &str, id: &str, peer: &str, timeout: std::time::Duration) -> Result<&'static str> {
        if owner != self.me().address() { return Err(Error::Invalid("Notification identity changed".into())); }
        self.kick();
        let deadline = tokio::time::Instant::now() + timeout;
        loop {
            if self.stopped.load(std::sync::atomic::Ordering::Relaxed) { return Err(Error::Cancelled); }
            let state = self.store.notification_reply_state(owner, id, peer)?;
            if state != "retry" || tokio::time::Instant::now() >= deadline { return Ok(state); }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
    }
    pub fn incoming_event(&self, peer: String, id: String, preview: String, quiet: bool) -> Event {
        Event::Incoming { owner: self.me().address(), peer, id, preview, quiet }
    }
    /// The account was captured when the notification was posted, never at send time.
    /// Admission is local; routing, delivery and retries remain the sender's job.
    pub fn queue_notification_reply(&self, owner: &str, id: &str, peer: &str, body: &str) -> Result<bool> {
        if owner != self.me().address() {
            return Err(Error::Invalid("The notification belongs to an identity that is no longer connected.".into()));
        }
        if self.stopped.load(std::sync::atomic::Ordering::Relaxed) { return Err(Error::Cancelled); }
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
    fn delivery_fixture() -> (Arc<Store>, Arc<Engine>, String) {
        let store = Arc::new(Store::open_in_memory().unwrap());
        let me = Identity::generate("reply-delivery-proof").unwrap();
        let owner = me.address();
        store.set_meta("identity.address", &owner).unwrap();
        let engine = Engine::new(store.clone(), me, Profile::default(), Arc::new(HostStub(Mutex::new(vec![]))));
        (store, engine, owner)
    }
    #[tokio::test]
    async fn sent_reply_does_not_wait_for_another_conversations_outbox() {
        let (store, engine, owner) = delivery_fixture();
        let id = "bb05866b-b96c-46c0-ae3f-236b3dd65fa2";
        engine.queue_notification_reply(&owner, id, "pat.abcdef", "sent reply").unwrap();
        store.set_state(id, "sent", None).unwrap();
        engine.queue_notification_reply(&owner, "d48cc0bb-a9d2-4329-bfbd-eefec6f50880", "other.abcdef", "still offline").unwrap();
        assert_eq!(store.queued().unwrap().len(), 1);
        assert_eq!(engine.check_notification_reply(&owner, id, "pat.abcdef", std::time::Duration::ZERO).await.unwrap(), "sent");
    }
    #[tokio::test]
    async fn failed_reply_is_not_success_even_when_the_global_queue_is_empty() {
        let (store, engine, owner) = delivery_fixture();
        let id = "bb05866b-b96c-46c0-ae3f-236b3dd65fa2";
        engine.queue_notification_reply(&owner, id, "pat.abcdef", "failed reply").unwrap();
        store.set_state(id, "failed", Some("synthetic failure")).unwrap();
        assert!(store.queued().unwrap().is_empty());
        assert_eq!(engine.check_notification_reply(&owner, id, "pat.abcdef", std::time::Duration::ZERO).await.unwrap(), "failed");
    }
    #[tokio::test]
    async fn reply_check_preserves_read_state_and_recognizes_all_sent_receipts() {
        let (store, engine, owner) = delivery_fixture();
        let id = "bb05866b-b96c-46c0-ae3f-236b3dd65fa2";
        engine.queue_notification_reply(&owner, id, "pat.abcdef", "reply").unwrap();
        for state in ["queued", "sending", "sent", "fetched", "delivered", "read"] {
            store.set_state(id, state, None).unwrap();
            let seen = store.message(id).unwrap().unwrap().seen;
            let expected = if state == "queued" || state == "sending" { "retry" } else { "sent" };
            assert_eq!(engine.check_notification_reply(&owner, id, "pat.abcdef", std::time::Duration::ZERO).await.unwrap(), expected);
            assert_eq!(store.message(id).unwrap().unwrap().seen, seen);
        }
    }
    #[tokio::test]
    async fn reply_check_rejects_owner_peer_and_stopped_engine_changes() {
        let (store, engine, owner) = delivery_fixture();
        let id = "bb05866b-b96c-46c0-ae3f-236b3dd65fa2";
        engine.queue_notification_reply(&owner, id, "pat.abcdef", "reply").unwrap();
        store.set_state(id, "sent", None).unwrap();
        assert!(engine.check_notification_reply("other.abcdef", id, "pat.abcdef", std::time::Duration::ZERO).await.is_err());
        assert!(engine.check_notification_reply(&owner, id, "other.abcdef", std::time::Duration::ZERO).await.is_err());
        store.set_meta("identity.address", "other.abcdef").unwrap();
        assert!(engine.check_notification_reply(&owner, id, "pat.abcdef", std::time::Duration::ZERO).await.is_err());
        store.set_meta("identity.address", &owner).unwrap();
        engine.stopped.store(true, std::sync::atomic::Ordering::Relaxed);
        assert!(engine.check_notification_reply(&owner, id, "pat.abcdef", std::time::Duration::ZERO).await.is_err());
    }
    #[tokio::test]
    async fn deleted_reply_receipt_completes_without_resurrecting_text() {
        let (store, engine, owner) = delivery_fixture();
        let id = "bb05866b-b96c-46c0-ae3f-236b3dd65fa2";
        assert_eq!(engine.check_notification_reply(&owner, id, "pat.abcdef", std::time::Duration::ZERO).await.unwrap(), "retry");
        engine.queue_notification_reply(&owner, id, "pat.abcdef", "reply").unwrap();
        store.delete_chat("pat.abcdef").unwrap();
        assert_eq!(engine.check_notification_reply(&owner, id, "pat.abcdef", std::time::Duration::ZERO).await.unwrap(), "gone");
        assert!(store.message(id).unwrap().is_none());
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
