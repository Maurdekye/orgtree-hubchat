//! Shell notification payloads and readiness-safe local admission.
use crate::{engine::{Engine, Event}, store::Store, Error};

pub struct IncomingNotice {
    pub owner: String,
    pub peer: String,
    pub title: String,
    pub body: String,
}

pub fn incoming_notice(event: &Event, store: &Store, preview: bool) -> Option<IncomingNotice> {
    let Event::Incoming { peer, preview: body, .. } = event else { return None };
    Some(IncomingNotice {
        owner: store.meta("identity.address").ok().flatten().unwrap_or_default(),
        peer: peer.clone(),
        title: store.display_name(peer).ok().flatten().unwrap_or_else(|| peer.clone()),
        body: if preview { body.clone() } else { "New message".into() },
    })
}

pub fn admit(engine: Option<&Engine>, owner: &str, id: &str, peer: &str, body: &str) -> &'static str {
    let Some(engine) = engine else { return "rejected" };
    match engine.queue_notification_reply(owner, id, peer, body) {
        Ok(_) => "queued",
        Err(Error::Invalid(_)) => "rejected",
        Err(_) => "retry",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{engine::Host, hub::Profile, Identity};
    use std::sync::{Arc, Barrier};
    struct SilentHost;
    impl Host for SilentHost {
        fn event(&self, _: Event) {}
        fn open_source(&self, _: &str) -> std::io::Result<std::fs::File> { unreachable!() }
        fn download_dir(&self) -> std::path::PathBuf { Default::default() }
    }
    const ID: &str = "bb05866b-b96c-46c0-ae3f-236b3dd65fa2";
    const PEER: &str = "pat.abcdef";
    fn engine(store: Arc<Store>, me: Identity) -> Engine {
        Engine::new(store, me, Profile::default(), Arc::new(SilentHost))
    }

    #[test]
    fn delayed_event_after_identity_switch_keeps_original_owner() {
        let store = Arc::new(Store::open_in_memory().unwrap());
        let a = Identity::generate("alice").unwrap();
        let owner_a = a.address();
        store.set_meta("identity.address", &owner_a).unwrap();
        let engine_a = engine(store.clone(), a);
        let ev = engine_a.incoming_event(PEER.into(), "incoming-id".into(), "hello".into(), false);
        let entered = Arc::new(Barrier::new(2));
        let resume = Arc::new(Barrier::new(2));
        let (e, r, s) = (entered.clone(), resume.clone(), store.clone());
        let callback = std::thread::spawn(move || {
            e.wait(); r.wait();
            incoming_notice(&ev, &s, true).unwrap()
        });
        entered.wait();
        store.wipe().unwrap();
        let b = Identity::generate("bob").unwrap();
        store.set_meta("identity.address", &b.address()).unwrap();
        let engine_b = engine(store.clone(), b);
        resume.wait();
        let notice = callback.join().unwrap();
        assert_eq!(notice.owner, owner_a);
        assert_eq!(admit(Some(&engine_b), &notice.owner, ID, &notice.peer, "reply"), "rejected");
        assert!(store.message(ID).unwrap().is_none());
    }

    #[test]
    fn published_core_without_engine_retains_reply_for_retry() {
        assert_eq!(admit(None, "alice.123456", ID, PEER, "sole saved text"), "retry");
    }

    #[test]
    fn transient_init_failure_then_retry_admits_exactly_once() {
        let store = Arc::new(Store::open_in_memory().unwrap());
        let me = Identity::generate("alice").unwrap();
        let owner = me.address();
        store.set_meta("identity.address", &owner).unwrap();
        assert_eq!(admit(None, &owner, ID, PEER, "sole saved text"), "retry");
        assert!(store.message(ID).unwrap().is_none());
        let engine = engine(store.clone(), me);
        assert_eq!(admit(Some(&engine), &owner, ID, PEER, "sole saved text"), "queued");
        assert_eq!(admit(Some(&engine), &owner, ID, PEER, "sole saved text"), "queued");
        assert_eq!(store.message(ID).unwrap().unwrap().body, "sole saved text");
        assert_eq!(store.queue_notification_reply(&owner, ID, PEER, "sole saved text").unwrap(), false);
    }
}
