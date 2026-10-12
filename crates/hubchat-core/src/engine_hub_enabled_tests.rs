//! Scratch stores and cancelled/suspended clients; never a real hub.
use super::*;

const A: &str = "http://127.0.0.1:7997";
const B: &str = "http://127.0.0.1:7998";
const PEER: &str = "pat.abcdef";
struct SilentHost;
impl Host for SilentHost {
    fn event(&self, _: Event) {}
    fn open_source(&self, _: &str) -> std::io::Result<std::fs::File> { unreachable!() }
    fn download_dir(&self) -> PathBuf { PathBuf::new() }
}
fn engine() -> Arc<Engine> {
    let s = Arc::new(Store::open_in_memory().unwrap());
    let me = Identity::generate("hub-toggle-proof").unwrap();
    s.set_meta("identity.address", &me.address()).unwrap();
    s.add_hub(A, "t0").unwrap();
    s.add_hub(B, "t1").unwrap();
    let e = Engine::new(s, me, Profile::default(), Arc::new(SilentHost));
    e.set_suspended(true);
    e
}
fn attach(e: &Engine, url: &str) -> (HubClient, CancelFlag) {
    let stop = CancelFlag::default();
    let client = HubClient::new(HubAddress::parse(url).unwrap()).bind_stop(stop.clone());
    e.hubs.lock().unwrap().insert(url.into(), HubRuntime {
        client: client.clone(), stop: stop.clone(), retry_now: Arc::new(Notify::new()), kick: Arc::new(Notify::new()),
        status: HubStatus { url: url.into(), name: "scratch".into(), state: HubState::Connected,
            error: None, retry_at_ms: None, max_attachment_bytes: 1_000_000, features: vec![],
            version: None, answered_ms: None, waiting_since_ms: None, clock_offset_ms: None },
    });
    (client, stop)
}

#[test]
fn saved_policy_defaults_enabled_preserves_data_and_does_not_reenable_on_add() {
    let e = engine();
    e.store.set_draft(PEER, "draft").unwrap();
    e.store.set_send_hub(PEER, Some(A)).unwrap();
    e.store.set_meta("history.floor.pat.abcdef", "all").unwrap();
    assert!(e.store.hubs().unwrap().iter().all(|h| h.enabled));
    e.set_hub_enabled(A, false).unwrap();
    e.store.add_hub(A, "again").unwrap();
    assert!(!e.store.hub_enabled(A).unwrap());
    assert!(e.store.hub_enabled(B).unwrap());
    assert_eq!(e.store.draft(PEER).unwrap().as_deref(), Some("draft"));
    assert_eq!(e.store.send_hub(PEER).unwrap().as_deref(), Some(A));
    assert_eq!(e.store.meta("history.floor.pat.abcdef").unwrap().as_deref(), Some("all"));
    assert_eq!(e.hub_statuses()[0].state, HubState::Disabled);
    assert!(e.set_hub_enabled("http://127.0.0.1:7999", false).is_err());
    e.store.remove_hub(A).unwrap();
    e.store.add_hub(A, "new").unwrap();
    assert!(e.store.hub_enabled(A).unwrap());
    e.store.wipe().unwrap();
    assert!(e.store.meta(&format!("hub.disabled.{A}")).unwrap().is_none());
}

#[tokio::test]
async fn disabled_saved_hub_is_not_spawned_and_reenable_uses_new_generation() {
    let e = engine();
    let (old, stop) = attach(&e, A);
    e.set_hub_enabled(A, false).unwrap();
    assert!(old.session_cancelled());
    e.spawn_hub(HubAddress::parse(A).unwrap(), "stale start".into(), None);
    assert!(e.hub_client(A).is_none());
    e.set_hub_enabled(A, true).unwrap();
    let fresh = e.hub_client(A).unwrap();
    assert!(!fresh.session_cancelled());
    e.set_status_for(A, &stop, |s| s.state = HubState::Refused);
    e.mark_for(A, &stop, |s| s.answered_ms = Some(99));
    let h = e.hub_statuses().into_iter().find(|h| h.url == A).unwrap();
    assert_eq!(h.state, HubState::Connecting);
    assert!(h.answered_ms.is_none());
    assert!(matches!(old.healthz().await, Err(Error::FilePaused)));
    e.shutdown();
    tokio::task::yield_now().await;
}

#[tokio::test]
async fn ordinary_and_notification_admission_keep_waiting_without_route() {
    let e = engine();
    attach(&e, A);
    e.set_hub_enabled(A, false).unwrap();
    let id = uuid::Uuid::new_v4().to_string();
    e.send(NewOutgoing { id: id.clone(), peer: PEER.into(), body: "queued".into(), kind: None, reply_to: None, attachments: vec![] }).await.unwrap();
    assert_eq!(e.store.message(&id).unwrap().unwrap().state, "queued");
    assert!(e.send_route(PEER).unwrap().next.is_none());
    let reply = uuid::Uuid::new_v4().to_string();
    assert!(e.queue_notification_reply(&e.me.address(), &reply, PEER, "reply").unwrap());
    assert!(!e.queue_notification_reply(&e.me.address(), &reply, PEER, "reply").unwrap());
    assert_eq!(e.store.queued().unwrap().len(), 2);
    assert_eq!(e.chat(PEER, None, None, 20).unwrap().len(), 2);
}

#[tokio::test]
async fn copied_control_clients_are_barred_after_disable_and_do_not_dispatch() {
    let e = engine();
    let (client, _) = attach(&e, A);
    e.set_hub_enabled(A, false).unwrap();
    assert!(matches!(client.roster(&e.me).await, Err(Error::FilePaused)));
    assert!(matches!(client.poll(&e.me, 0).await, Err(Error::FilePaused)));
    assert!(matches!(client.register(&e.me, &Profile::default()).await, Err(Error::FilePaused)));
    assert!(matches!(client.unregister(&e.me).await, Err(Error::FilePaused)));
    assert!(matches!(client.register_push(&e.me, "device", "invalid", "key", "auth").await, Err(Error::FilePaused)));
    assert!(matches!(client.message_body(&e.me, "id", 1).await, Err(Error::FilePaused)));
}

#[tokio::test]
async fn inflight_generation_guard_drops_work_and_returns_resumable_pause() {
    let e = engine();
    let (client, _) = attach(&e, A);
    let begun = Arc::new(Notify::new());
    let copy = begun.clone();
    let task = tokio::spawn(async move { client.session_work(async {
        copy.notify_one();
        std::future::pending::<Result<()>>().await
    }).await });
    begun.notified().await;
    e.set_hub_enabled(A, false).unwrap();
    assert!(matches!(tokio::time::timeout(Duration::from_secs(1), task).await.unwrap().unwrap(), Err(Error::FilePaused)));
}

#[test]
fn local_toggle_survives_reopen_without_changing_other_device() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("scratch.sqlite");
    {
        let s = Store::open(&path).unwrap();
        s.add_hub(A, "t0").unwrap();
        s.add_hub(B, "t1").unwrap();
        s.hub_ok(A, "Office", Some(12345), "ok").unwrap();
        s.set_hub_enabled(A, false).unwrap();
    }
    let reopened = Store::open(&path).unwrap();
    let h = reopened.hubs().unwrap();
    assert!(!h[0].enabled);
    assert_eq!(h[0].name, "Office");
    assert_eq!(h[0].max_attachment_bytes, Some(12345));
    assert_eq!(h[0].last_ok_at.as_deref(), Some("ok"));
    assert!(h[1].enabled);
    let other_device = Store::open_in_memory().unwrap();
    other_device.add_hub(A, "same identity, other installation").unwrap();
    assert!(other_device.hub_enabled(A).unwrap());
}

#[test]
fn disabled_pin_waits_but_automatic_uses_enabled_alternative() {
    let e = engine();
    for url in [A, B] {
        let entry = serde_json::from_value::<crate::hub::RosterEntry>(serde_json::json!({ "slug": PEER, "online": true })).unwrap();
        e.store.set_roster(url, &[entry]).unwrap();
        attach(&e, url);
    }
    e.set_send_hub(PEER, Some(A)).unwrap();
    e.set_hub_enabled(A, false).unwrap();
    let r = e.send_route(PEER).unwrap();
    assert_eq!(r.pinned.as_deref(), Some(A));
    assert!(r.next.is_none());
    assert_eq!(r.automatic.as_deref(), Some(B));
    assert_eq!(r.hubs.len(), 2, "roster knowledge is retained");
    e.set_send_hub(PEER, None).unwrap();
    assert_eq!(e.send_route(PEER).unwrap().next.as_deref(), Some(B));
}

#[tokio::test]
async fn previously_selected_send_route_cannot_turn_waiting_into_failure() {
    let e = engine();
    let (client, _) = attach(&e, A);
    let id = uuid::Uuid::new_v4().to_string();
    e.send(NewOutgoing { id: id.clone(), peer: PEER.into(), body: "still waiting".into(), kind: None, reply_to: None, attachments: vec![] }).await.unwrap();
    e.set_hub_enabled(A, false).unwrap();
    assert!(!e.send_one_on(&id, (A.into(), client, 1_000_000)).await.unwrap());
    let row = e.store.message(&id).unwrap().unwrap();
    assert_eq!(row.state, "queued");
    assert!(row.error.is_none());
}

#[tokio::test]
async fn retired_generation_cannot_change_replacement_clock_or_policy() {
    let e = engine();
    let (_, stop) = attach(&e, A);
    e.set_hub_enabled(A, false).unwrap();
    e.set_hub_enabled(A, true).unwrap();
    e.store.set_meta(&clock_key(A), "7").unwrap();
    assert!(matches!(e.clock_reading(A, &stop, Some(100), 0, 0), Err(Error::FilePaused)));
    assert_eq!(e.store.meta(&clock_key(A)).unwrap().as_deref(), Some("7"));
    e.store.set_meta("identity.address", "other-owner").unwrap();
    assert!(e.set_hub_enabled(A, false).is_err());
    assert!(e.store.hub_enabled(A).unwrap());
    e.shutdown();
    assert!(matches!(e.set_hub_enabled(A, false), Err(Error::Cancelled)));
    tokio::task::yield_now().await;
}
