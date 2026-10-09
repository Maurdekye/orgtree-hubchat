//! Two engines (alex, maya) talking through a real scratch hub: queue, send
//! with an attachment, receive, download, read receipts, failure on an
//! unknown address, offline queue that drains when a hub appears.

mod common;

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use hubchat_core::engine::{Engine, Event, Host, HubState};
use hubchat_core::hub::Profile;
use hubchat_core::store::{NewAttachment, NewOutgoing, Store};
use hubchat_core::Identity;

struct TestHost {
    events: Mutex<Vec<Event>>,
    downloads: PathBuf,
}

impl Host for TestHost {
    fn event(&self, ev: Event) {
        self.events.lock().unwrap().push(ev);
    }
    fn open_source(&self, source: &str) -> std::io::Result<std::fs::File> {
        std::fs::File::open(source)
    }
    fn download_dir(&self) -> PathBuf {
        self.downloads.clone()
    }
}

fn engine(id: &str, dir: &std::path::Path) -> (Arc<Engine>, Arc<TestHost>) {
    let host = Arc::new(TestHost {
        events: Mutex::new(Vec::new()),
        downloads: dir.join(id),
    });
    let store = Arc::new(Store::open_in_memory().unwrap());
    let profile = Profile {
        kind: "chat".into(),
        org_name: String::new(),
        username: id.into(),
        blurb: String::new(),
    };
    let e = Engine::new(
        store,
        Identity::generate(id).unwrap(),
        profile,
        host.clone(),
    );
    (e, host)
}

async fn until(what: &str, secs: u64, mut f: impl FnMut() -> bool) {
    let t0 = Instant::now();
    while !f() {
        assert!(
            t0.elapsed() < Duration::from_secs(secs),
            "timed out waiting for {what}"
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

fn msg(id: &str, to: &str, body: &str) -> NewOutgoing {
    NewOutgoing {
        id: id.into(),
        peer: to.into(),
        body: body.into(),
        kind: None,
        reply_to: None,
        attachments: vec![],
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn two_engines_chat_through_a_hub() {
    let Some(hub) = common::start_hub().await else {
        eprintln!("SKIPPED: no mailhub source");
        return;
    };
    let url = format!("127.0.0.1:{}", hub.port);
    let dir = tempfile::tempdir().unwrap();
    let (alex, _ah) = engine("alex", dir.path());
    let (maya, mh) = engine("maya", dir.path());

    // Alex queues a message before having any hub: it waits.
    alex.start().unwrap();
    alex.send(msg("m1", &maya.me().address(), "queued while offline"))
        .unwrap();
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(alex.store().message("m1").unwrap().unwrap().state, "queued");

    maya.start().unwrap();
    maya.add_hub(&url).unwrap();
    until("maya connected", 10, || {
        maya.hub_statuses()
            .iter()
            .any(|s| s.state == HubState::Connected)
    })
    .await;
    alex.add_hub(&url).unwrap();

    // The queued message drains once alex is connected and maya receives it.
    until("maya receives m1", 15, || {
        maya.store().message("m1").unwrap().is_some()
    })
    .await;
    assert!(mh
        .events
        .lock()
        .unwrap()
        .iter()
        .any(|e| matches!(e, Event::Incoming { id, .. } if id == "m1")));
    until("m1 delivered", 15, || {
        alex.store().message("m1").unwrap().unwrap().state == "delivered"
    })
    .await;

    // Read receipt.
    maya.mark_read(&alex.me().address()).await.unwrap();
    until("m1 read", 15, || {
        alex.store().message("m1").unwrap().unwrap().state == "read"
    })
    .await;

    // Attachment: upload, receive, download, identical.
    let src = dir.path().join("report.bin");
    let payload: Vec<u8> = (0..2_000_000u32).map(|i| (i % 253) as u8).collect();
    std::fs::write(&src, &payload).unwrap();
    let mut m2 = msg("m2", &maya.me().address(), "with a file");
    m2.attachments.push(NewAttachment {
        name: "report.bin".into(),
        bytes: payload.len() as u64,
        source: src.to_string_lossy().into(),
    });
    alex.send(m2).unwrap();
    until("maya receives m2", 15, || {
        maya.store().message("m2").unwrap().is_some()
    })
    .await;
    let got = maya.store().message("m2").unwrap().unwrap();
    assert_eq!(got.attachments.len(), 1);
    let path = maya
        .download("m2", &got.attachments[0].local_id)
        .await
        .unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), payload);
    assert_eq!(
        maya.store().message("m2").unwrap().unwrap().attachments[0].state,
        "done"
    );

    // Unknown address -> failed with the hub's reason; Retry re-queues.
    alex.send(msg("m3", "nobody.000000", "hello?")).unwrap();
    until("m3 failed", 15, || {
        alex.store().message("m3").unwrap().unwrap().state == "failed"
    })
    .await;
    let m3 = alex.store().message("m3").unwrap().unwrap();
    assert!(m3.error.unwrap().contains("422"));

    // Directory: both see each other, merged.
    until("directory", 70, || {
        alex.store()
            .directory()
            .unwrap()
            .iter()
            .any(|c| c.address == maya.me().address())
    })
    .await;

    // Sending to yourself is refused up front.
    assert!(alex.send(msg("m4", &alex.me().address(), "me")).is_err());

    // Removing the hub drops its roster.
    alex.remove_hub(&format!("http://{url}"), false)
        .await
        .unwrap();
    assert!(alex.store().directory().unwrap().is_empty());
    drop(hub);
}

/// B1: a chat whose peer two hubs reach keeps going through the hub it last
/// went through, instead of whichever hub the engine happens to list first.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_chat_keeps_its_hub_when_two_hubs_reach_the_peer() {
    let (Some(hub_a), Some(hub_b)) = (common::start_hub().await, common::start_hub().await) else {
        eprintln!("SKIPPED: no mailhub source");
        return;
    };
    let (url_a, url_b) = (
        format!("127.0.0.1:{}", hub_a.port),
        format!("127.0.0.1:{}", hub_b.port),
    );
    let dir = tempfile::tempdir().unwrap();
    let (alex, _ah) = engine("alex", dir.path());
    let (maya, _mh) = engine("maya", dir.path());
    let connected = |e: &Engine, n: usize| {
        e.hub_statuses()
            .iter()
            .filter(|s| s.state == HubState::Connected)
            .count()
            == n
    };
    maya.start().unwrap();
    maya.add_hub(&url_a).unwrap();
    maya.add_hub(&url_b).unwrap();
    until("maya on both hubs", 15, || connected(&maya, 2)).await;

    // The chat starts on hub B, alex's only hub so far.
    alex.start().unwrap();
    let b = alex.add_hub(&url_b).unwrap().to_string();
    until("alex on B", 15, || connected(&alex, 1)).await;
    alex.send(msg("k1", &maya.me().address(), "first")).unwrap();
    until("k1 sent", 15, || {
        alex.store().message("k1").unwrap().unwrap().hub.is_some()
    })
    .await;
    assert_eq!(alex.store().message("k1").unwrap().unwrap().hub, Some(b.clone()));

    // Hub A joins, and its roster (which the hub hands out with a poll
    // answer) lists maya too: wake alex's poll there with a message.
    let a = alex.add_hub(&url_a).unwrap();
    until("alex on both hubs", 15, || connected(&alex, 2)).await;
    hubchat_core::HubClient::new(a.clone())
        .send(
            maya.me(),
            &hubchat_core::hub::Outgoing::new(&alex.me().address(), "wake"),
        )
        .await
        .unwrap();
    until("both hubs reach maya", 15, || {
        alex.store().hubs_reaching(&maya.me().address()).unwrap().len() == 2
    })
    .await;

    for i in 2..=6 {
        let id = format!("k{i}");
        alex.send(msg(&id, &maya.me().address(), "more")).unwrap();
        until("sent", 15, || {
            alex.store().message(&id).unwrap().unwrap().hub.is_some()
        })
        .await;
        assert_eq!(
            alex.store().message(&id).unwrap().unwrap().hub,
            Some(b.clone()),
            "{id} left the chat's hub"
        );
    }
    alex.shutdown();
    maya.shutdown();
    drop((hub_a, hub_b));
}

/// The hub picker: a chat pinned to a hub goes through it, waits (never
/// switching on its own) while that hub can't reach the peer, and goes on
/// through Automatic's choice once unpinned.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_pinned_hub_carries_the_chat_and_waits_when_it_cannot() {
    let (Some(hub_a), Some(hub_b)) = (common::start_hub().await, common::start_hub().await) else {
        eprintln!("SKIPPED: no mailhub source");
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let (alex, _ah) = engine("alex", dir.path());
    let (maya, _mh) = engine("maya", dir.path());
    let connected = |e: &Engine, n: usize| {
        e.hub_statuses()
            .iter()
            .filter(|s| s.state == HubState::Connected)
            .count()
            == n
    };
    let mut urls = Vec::new();
    for e in [&maya, &alex] {
        e.start().unwrap();
        urls.clear();
        for h in [&hub_a, &hub_b] {
            urls.push(e.add_hub(&format!("127.0.0.1:{}", h.port)).unwrap());
        }
    }
    until("all connected", 15, || connected(&maya, 2) && connected(&alex, 2)).await;
    // the hubs hand out their roster with a poll answer: wake alex's polls
    let peer = maya.me().address();
    for a in &urls {
        hubchat_core::HubClient::new(a.clone())
            .send(maya.me(), &hubchat_core::hub::Outgoing::new(&alex.me().address(), "wake"))
            .await
            .unwrap();
    }
    until("both hubs reach maya", 15, || {
        alex.store().hubs_reaching(&peer).unwrap().len() == 2
    })
    .await;
    let (a, b) = (urls[0].to_string(), urls[1].to_string());
    let (first, other) = if a < b { (a, b) } else { (b, a) };
    let sent_via = |id: &str| alex.store().message(id).unwrap().unwrap().hub;

    // Automatic: both online, no history: the first by address.
    let r = alex.send_route(&peer).unwrap();
    assert_eq!((r.pinned.clone(), r.automatic.clone()), (None, Some(first.clone())));
    assert_eq!(r.next, r.automatic);
    assert_eq!(r.hubs.len(), 2);
    alex.send(msg("p1", &peer, "auto")).unwrap();
    until("p1 sent", 15, || sent_via("p1").is_some()).await;
    assert_eq!(sent_via("p1"), Some(first.clone()));

    // Pinned to the other hub: it goes there, though Automatic says otherwise.
    alex.set_send_hub(&peer, Some(&other)).unwrap();
    let r = alex.send_route(&peer).unwrap();
    assert_eq!((r.pinned.clone(), r.next.clone()), (Some(other.clone()), Some(other.clone())));
    assert_eq!(r.automatic, Some(first.clone()));
    alex.send(msg("p2", &peer, "pinned")).unwrap();
    until("p2 sent", 15, || sent_via("p2").is_some()).await;
    assert_eq!(sent_via("p2"), Some(other.clone()));

    // The pinned hub goes down: the message waits instead of switching.
    let (mut down, up) = if other == urls[0].to_string() { (hub_a, hub_b) } else { (hub_b, hub_a) };
    down.stop();
    until("pinned hub down", 30, || connected(&alex, 1)).await;
    let r = alex.send_route(&peer).unwrap();
    assert_eq!((r.pinned.clone(), r.next.clone()), (Some(other.clone()), None));
    assert_eq!(r.automatic, Some(first.clone()));
    alex.send(msg("p3", &peer, "waits")).unwrap();
    tokio::time::sleep(Duration::from_secs(2)).await;
    let p3 = alex.store().message("p3").unwrap().unwrap();
    assert_eq!((p3.state.as_str(), p3.hub), ("queued", None));

    // The pinned hub comes back: the waiting message goes through it.
    down.restart().await;
    alex.retry_now();
    maya.retry_now();
    until("p3 sent through the pinned hub", 30, || sent_via("p3").is_some()).await;
    assert_eq!(sent_via("p3"), Some(other.clone()));

    // Back to Automatic: the next goes through Automatic's choice.
    alex.set_send_hub(&peer, None).unwrap();
    let r = alex.send_route(&peer).unwrap();
    assert_eq!((r.pinned.clone(), r.next.clone()), (None, r.automatic.clone()));
    let auto = r.automatic.clone().expect("a hub can reach maya");
    alex.send(msg("p4", &peer, "auto again")).unwrap();
    until("p4 sent", 15, || sent_via("p4").is_some()).await;
    assert_eq!(sent_via("p4"), Some(auto));
    alex.shutdown();
    maya.shutdown();
    drop(up);
    drop(down);
}
