//! Scratch-only first-send visibility controls; no engine task or hub is started.
use super::*;

struct FirstSendHost(Mutex<Vec<Event>>);
impl Host for FirstSendHost {
    fn event(&self, e: Event) { self.0.lock().unwrap().push(e); }
    fn open_source(&self, _: &str) -> std::io::Result<std::fs::File> { unreachable!() }
    fn download_dir(&self) -> PathBuf { PathBuf::new() }
}

#[tokio::test]
async fn accepted_first_send_is_visible_below_lazy_history_floor_without_inbound() {
    let store = Arc::new(Store::open_in_memory().unwrap());
    let host = Arc::new(FirstSendHost(Mutex::new(vec![])));
    let engine = Engine::new(store.clone(), Identity::generate("first-send-proof").unwrap(), Profile::default(), host.clone());
    let peer = "pat.abcdef";
    // A remembered hub boundary ahead of local time (e.g. clock rollback).
    // This is controlled state, not a reproduction of the reported device.
    let future = unix_ms() + 60_000;
    let addr = HubAddress::parse("127.0.0.1:7999").unwrap();
    let url = addr.to_string();
    store.set_meta(&start_key(&url), &future.to_string()).unwrap();
    engine.hubs.lock().unwrap().insert(url.clone(), HubRuntime {
        client: HubClient::new(addr),
        status: HubStatus { url, name: "scratch-only".into(), state: HubState::Connected,
            error: None, retry_at_ms: None, max_attachment_bytes: 1_000_000,
            features: vec!["lazy_history".into()], version: None,
            answered_ms: None, waiting_since_ms: None, clock_offset_ms: Some(0) },
        retry_now: Arc::new(Notify::new()), kick: Arc::new(Notify::new()), stop: CancelFlag::default(),
    });
    assert!(engine.chat(peer, None, None, 50).unwrap().is_empty());
    let id = uuid::Uuid::new_v4().to_string();
    engine.send(NewOutgoing { id: id.clone(), peer: peer.into(), body: "first outgoing".into(), kind: None, reply_to: None, attachments: vec![] }).await.unwrap();
    let persisted = store.chat(peer, None, None, 50).unwrap();
    assert_eq!(persisted.len(), 1, "admitted once in the durable outbox");
    assert_eq!(persisted[0].state, "queued", "no fabricated delivery");
    assert!(host.0.lock().unwrap().iter().any(|e| matches!(e, Event::Chat { peer: p } if p == peer)));
    assert_eq!(engine.chat(peer, None, None, 50).unwrap().iter().map(|m| m.id.as_str()).collect::<Vec<_>>(), [id.as_str()], "the command/event re-read must display the accepted row without inbound traffic");
}
