//! Scratch-only first-send visibility controls; no engine task or hub is started.
use super::*;

struct FirstSendHost(Mutex<Vec<Event>>);
impl Host for FirstSendHost {
    fn event(&self, e: Event) {
        self.0.lock().unwrap().push(e);
    }
    fn open_source(&self, _: &str) -> std::io::Result<std::fs::File> {
        unreachable!()
    }
    fn download_dir(&self) -> PathBuf {
        PathBuf::new()
    }
}

// The candidate closure is the production boundary between remote/history
// reads and persistence. Barriers schedule a real admission in that gap.
async fn stale_schedule(notification: bool, had_numeric: bool) {
    let store = Arc::new(Store::open_in_memory().unwrap());
    let me = Identity::generate("first-send-race").unwrap();
    let owner = me.address();
    store.set_meta("identity.address", &owner).unwrap();
    let engine = Engine::new(
        store.clone(),
        me,
        Profile::default(),
        Arc::new(FirstSendHost(Mutex::new(vec![]))),
    );
    let peer = "pat.abcdef";
    let key = format!("history.floor.{peer}");
    let candidate = unix_ms() + 60_000;
    connect(&engine, candidate);
    if had_numeric {
        store
            .set_meta(&key, &(candidate + 60_000).to_string())
            .unwrap();
    }
    let ready = Arc::new(std::sync::Barrier::new(2));
    let resume = Arc::new(std::sync::Barrier::new(2));
    let reader = {
        let engine = engine.clone();
        let ready = ready.clone();
        let resume = resume.clone();
        std::thread::spawn(move || {
            engine
                .history_floor_with(peer, || {
                    ready.wait();
                    resume.wait();
                    Ok(Some(candidate as i64))
                })
                .unwrap()
        })
    };
    ready.wait();
    let id = uuid::Uuid::new_v4().to_string();
    if notification {
        assert!(engine
            .queue_notification_reply(&owner, &id, peer, "local accepted")
            .unwrap());
    } else {
        engine
            .send(NewOutgoing {
                id: id.clone(),
                peer: peer.into(),
                body: "local accepted".into(),
                kind: None,
                reply_to: None,
                attachments: vec![],
            })
            .await
            .unwrap();
    }
    let row = store.message(&id).unwrap().unwrap();
    let committed = store.meta(&key).unwrap();
    resume.wait();
    let returned = reader.join().unwrap();
    assert_eq!(store.meta(&key).unwrap(), committed,"notification={notification}, old numeric={had_numeric}: stale writer must retain winning admission");
    assert!(
        returned.is_none_or(|f| f <= row.created_at),
        "reader must return its merged winner, not its stale candidate"
    );
    assert_eq!(engine.chat(peer, None, None, 50).unwrap().len(), 1);
    assert_eq!(store.queued().unwrap().len(), 1);
    if notification {
        assert!(!engine
            .queue_notification_reply(&owner, &id, peer, "replay")
            .unwrap());
        assert_eq!(store.meta(&key).unwrap(), committed);
    }
}

#[tokio::test]
async fn stale_reader_ordinary_absent() {
    stale_schedule(false, false).await;
}
#[tokio::test]
async fn stale_reader_ordinary_numeric() {
    stale_schedule(false, true).await;
}
#[tokio::test]
async fn stale_reader_notification_absent() {
    stale_schedule(true, false).await;
}
#[tokio::test]
async fn stale_reader_notification_numeric() {
    stale_schedule(true, true).await;
}

#[test]
fn stale_numeric_reader_preserves_a_concurrent_all_sentinel_and_returns_no_filter() {
    let store = Arc::new(Store::open_in_memory().unwrap());
    let engine = Engine::new(
        store.clone(),
        Identity::generate("first-send-race").unwrap(),
        Profile::default(),
        Arc::new(FirstSendHost(Mutex::new(vec![]))),
    );
    let peer = "pat.abcdef";
    let key = format!("history.floor.{peer}");
    let candidate = unix_ms() as i64;
    store.set_meta(&key, &(candidate + 1).to_string()).unwrap();
    let returned = engine
        .history_floor_with(peer, || {
            store.set_meta(&key, "all").unwrap();
            Ok(Some(candidate))
        })
        .unwrap();
    assert_eq!(store.meta(&key).unwrap().as_deref(), Some("all"));
    assert!(returned.is_none());
}

fn connect(engine: &Engine, start: u64) {
    let addr = HubAddress::parse("127.0.0.1:7999").unwrap();
    let url = addr.to_string();
    engine
        .store
        .set_meta(&start_key(&url), &start.to_string())
        .unwrap();
    engine.hubs.lock().unwrap().insert(
        url.clone(),
        HubRuntime {
            client: HubClient::new(addr),
            status: HubStatus {
                url,
                name: "scratch-only".into(),
                state: HubState::Connected,
                error: None,
                retry_at_ms: None,
                max_attachment_bytes: 1_000_000,
                features: vec!["lazy_history".into()],
                version: None,
                answered_ms: None,
                waiting_since_ms: None,
                clock_offset_ms: Some(0),
            },
            retry_now: Arc::new(Notify::new()),
            kick: Arc::new(Notify::new()),
            stop: CancelFlag::default(),
        },
    );
}

#[tokio::test]
async fn accepted_first_send_is_visible_below_lazy_history_floor_without_inbound() {
    let store = Arc::new(Store::open_in_memory().unwrap());
    let host = Arc::new(FirstSendHost(Mutex::new(vec![])));
    let engine = Engine::new(
        store.clone(),
        Identity::generate("first-send-proof").unwrap(),
        Profile::default(),
        host.clone(),
    );
    let peer = "pat.abcdef";
    // A remembered hub boundary ahead of local time (e.g. clock rollback).
    // This is controlled state, not a reproduction of the reported device.
    let future = unix_ms() + 60_000;
    connect(&engine, future);
    assert!(engine.chat(peer, None, None, 50).unwrap().is_empty());
    let id = uuid::Uuid::new_v4().to_string();
    engine
        .send(NewOutgoing {
            id: id.clone(),
            peer: peer.into(),
            body: "first outgoing".into(),
            kind: None,
            reply_to: None,
            attachments: vec![],
        })
        .await
        .unwrap();
    let persisted = store.chat(peer, None, None, 50).unwrap();
    assert_eq!(persisted.len(), 1, "admitted once in the durable outbox");
    assert_eq!(persisted[0].state, "queued", "no fabricated delivery");
    assert!(host
        .0
        .lock()
        .unwrap()
        .iter()
        .any(|e| matches!(e, Event::Chat { peer: p } if p == peer)));
    let visible = engine.chat(peer, None, None, 50).unwrap();
    if let Ok(path) = std::env::var("HUBCHAT_FIRST_SEND_FIXTURE") {
        std::fs::write(
            path,
            serde_json::to_vec_pretty(
                &serde_json::json!({"peer":peer,"persisted":persisted,"visible":visible}),
            )
            .unwrap(),
        )
        .unwrap();
    }
    assert_eq!(
        visible.iter().map(|m| m.id.as_str()).collect::<Vec<_>>(),
        [id.as_str()],
        "the command/event re-read must display the accepted row without inbound traffic"
    );
    for state in ["sending", "failed", "sent", "delivered"] {
        store
            .set_state(
                &id,
                state,
                (state == "failed").then_some("synthetic offline failure"),
            )
            .unwrap();
        let page = engine.chat(peer, None, None, 50).unwrap();
        assert_eq!(page.len(), 1);
        assert_eq!(page[0].state, state);
    }
    let second = uuid::Uuid::new_v4().to_string();
    engine
        .send(NewOutgoing {
            id: second.clone(),
            peer: peer.into(),
            body: "second outgoing".into(),
            kind: None,
            reply_to: Some(id.clone()),
            attachments: vec![],
        })
        .await
        .unwrap();
    let page = engine.chat(peer, None, None, 50).unwrap();
    assert_eq!(page.len(), 2);
    assert!(page.iter().any(|m| m.id == id) && page.iter().any(|m| m.id == second));
    assert_eq!(
        engine
            .chat(peer, None, Some((&page[0].created_at, &page[0].id)), 50)
            .unwrap(),
        page
    );
}

#[tokio::test]
async fn a_later_lazy_start_does_not_hide_a_send_already_accepted_offline() {
    let store = Arc::new(Store::open_in_memory().unwrap());
    let engine = Engine::new(
        store.clone(),
        Identity::generate("first-send-proof").unwrap(),
        Profile::default(),
        Arc::new(FirstSendHost(Mutex::new(vec![]))),
    );
    let peer = "pat.abcdef";
    engine
        .send(NewOutgoing {
            id: "offline-first".into(),
            peer: peer.into(),
            body: "accepted offline".into(),
            kind: None,
            reply_to: None,
            attachments: vec![],
        })
        .await
        .unwrap();
    connect(&engine, unix_ms() + 1);
    assert_eq!(engine.chat(peer, None, None, 50).unwrap().len(), 1);
    assert_eq!(store.queued().unwrap().len(), 1);
}

#[tokio::test]
async fn historical_sync_stays_paged_while_local_and_live_incoming_rows_show_once() {
    let store = Arc::new(Store::open_in_memory().unwrap());
    let me = Identity::generate("first-send-proof").unwrap();
    let owner = me.address();
    let engine = Engine::new(
        store.clone(),
        me,
        Profile::default(),
        Arc::new(FirstSendHost(Mutex::new(vec![]))),
    );
    let peer = "pat.abcdef";
    connect(&engine, unix_ms() + 60_000);
    let old = "2020-01-01T00:00:00.000Z";
    for (id, from, to) in [
        ("historic-out", owner.as_str(), peer),
        ("historic-in", peer, owner.as_str()),
    ] {
        let m: crate::hub_v2::SyncedMessage = serde_json::from_value(serde_json::json!({"id":id,"from":from,"to":to,"body":"history","sent_at":old,"received_at":old,"attachments":[]})).unwrap();
        store
            .upsert_synced("http://127.0.0.1:7999", &owner, &m)
            .unwrap();
    }
    assert!(engine.chat(peer, None, None, 50).unwrap().is_empty());
    engine
        .send(NewOutgoing {
            id: "local-now".into(),
            peer: peer.into(),
            body: "local".into(),
            kind: None,
            reply_to: None,
            attachments: vec![],
        })
        .await
        .unwrap();
    assert_eq!(engine.chat(peer, None, None, 50).unwrap().len(), 1);
    let at = now();
    let env: crate::hub::Envelope = serde_json::from_value(serde_json::json!({"id":"live-reply","from":peer,"to":owner,"body":"reply","reply_to":"local-now","sent_at":at,"received_at":at,"attachments":[]})).unwrap();
    assert!(store
        .insert_incoming("http://127.0.0.1:7999", &env, &at)
        .unwrap());
    assert!(!store
        .insert_incoming("http://127.0.0.1:7999", &env, &at)
        .unwrap());
    assert_eq!(engine.chat(peer, None, None, 50).unwrap().len(), 2);
    assert_eq!(store.chat(peer, None, None, 50).unwrap().len(), 4);
}

#[test]
fn local_boundary_is_atomic_monotone_and_does_not_change_historical_or_closed_dms() {
    let s = Store::open_in_memory().unwrap();
    let peer = "pat.abcdef";
    let key = format!("history.floor.{peer}");
    let msg = NewOutgoing {
        id: "local".into(),
        peer: peer.into(),
        body: "accepted".into(),
        kind: None,
        reply_to: None,
        attachments: vec![],
    };
    s.queue_outgoing(&msg, "2026-10-12T09:30:00.000Z").unwrap();
    let original = s.meta(&key).unwrap();
    assert!(
        original.is_some(),
        "admission records visibility before any later lazy sync"
    );
    assert!(s.queue_outgoing(&msg, "2026-10-12T09:00:00.000Z").is_err());
    assert_eq!(
        s.meta(&key).unwrap(),
        original,
        "duplicate admission rolls back the boundary too"
    );
    let second = NewOutgoing {
        id: "later".into(),
        ..msg.clone()
    };
    s.queue_outgoing(&second, "2026-10-12T09:40:00.000Z")
        .unwrap();
    assert_eq!(
        s.meta(&key).unwrap(),
        original,
        "later sends never raise the boundary"
    );
    s.set_meta(&key, "all").unwrap();
    let third = NewOutgoing {
        id: "third".into(),
        ..msg.clone()
    };
    s.queue_outgoing(&third, "2026-10-12T09:50:00.000Z")
        .unwrap();
    assert_eq!(s.meta(&key).unwrap().as_deref(), Some("all"));
    s.close_dm(peer).unwrap();
    let fourth = NewOutgoing {
        id: "fourth".into(),
        ..msg
    };
    s.queue_outgoing(&fourth, "2026-10-12T10:00:00.000Z")
        .unwrap();
    assert!(
        s.chats().unwrap().is_empty(),
        "local admission must not reopen a closed DM"
    );
    assert_eq!(s.chat(peer, None, None, 50).unwrap().len(), 4);
}
