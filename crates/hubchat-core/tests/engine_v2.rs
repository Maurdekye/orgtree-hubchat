//! Mail hub v2.0 features end to end against a running scratch v2 hub:
//! two devices of one identity plus a partner. Set HUBCHAT_V2_HUB to the
//! hub's address (e.g. 127.0.0.1:7397); skipped when unset.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use hubchat_core::engine::{Engine, Event, Host, HubState};
use hubchat_core::hub::Profile;
use hubchat_core::store::{NewOutgoing, Store};
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

fn device(me: &Identity, device_id: &str, dir: &std::path::Path) -> (Arc<Engine>, Arc<TestHost>) {
    let host = Arc::new(TestHost {
        events: Mutex::new(Vec::new()),
        downloads: dir.join(device_id),
    });
    let store = Arc::new(Store::open_in_memory().unwrap());
    let profile = Profile {
        kind: "person".into(),
        org_name: me.id().into(),
        username: me.id().into(),
        blurb: String::new(),
    };
    let e = Engine::new(store, me.clone(), profile, host.clone());
    e.set_device(device_id, device_id);
    (e, host)
}

async fn until(what: &str, secs: u64, mut f: impl FnMut() -> bool) {
    let t0 = Instant::now();
    while !f() {
        assert!(
            t0.elapsed() < Duration::from_secs(secs),
            "timed out waiting for {what}"
        );
        tokio::time::sleep(Duration::from_millis(150)).await;
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

fn uid(p: &str) -> String {
    format!("{p}{}", &uuid::Uuid::new_v4().simple().to_string()[..8])
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn every_device_gets_everything() {
    let Ok(hub) = std::env::var("HUBCHAT_V2_HUB") else {
        eprintln!("SKIPPED: set HUBCHAT_V2_HUB to a scratch mail hub v2.0");
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let alex = Identity::generate("alex").unwrap();
    let maya = Identity::generate("maya").unwrap();
    let (a, ha) = device(&alex, "alex-pc", dir.path());
    let (b, _hb) = device(&alex, "alex-phone", dir.path());
    let (m, _hm) = device(&maya, "maya-pc", dir.path());
    for e in [&a, &b, &m] {
        e.start().unwrap();
        e.add_hub(&hub).unwrap();
    }
    for e in [&a, &b, &m] {
        until("connected", 15, || {
            e.hub_statuses()
                .iter()
                .any(|s| s.state == HubState::Connected)
        })
        .await;
        let s = e.hub_statuses();
        assert!(s[0].features.iter().any(|f| f == "sync"), "hub is v2");
        assert_eq!(s[0].version.as_deref(), Some("2.0.0"));
    }

    // A message to alex reaches BOTH of alex's devices.
    let m1 = uid("m1-");
    m.send(msg(&m1, &alex.address(), "hello alex")).unwrap();
    until("A gets m1", 15, || {
        a.store().message(&m1).unwrap().is_some()
    })
    .await;
    until("B gets m1", 15, || {
        b.store().message(&m1).unwrap().is_some()
    })
    .await;
    assert!(ha
        .events
        .lock()
        .unwrap()
        .iter()
        .any(|e| matches!(e, Event::Incoming { id, .. } if *id == m1)));

    // What A sends shows up on B, as ours; maya's read reaches both.
    let a1 = uid("a1-");
    a.send(msg(&a1, &maya.address(), "from the pc")).unwrap();
    until("B sees A's message", 15, || {
        b.store().message(&a1).unwrap().is_some_and(|x| x.outgoing)
    })
    .await;
    until("maya gets a1", 15, || {
        m.store().message(&a1).unwrap().is_some()
    })
    .await;
    m.mark_read(&alex.address()).await.unwrap();
    until("A sees read", 20, || {
        a.store().message(&a1).unwrap().unwrap().state == "read"
    })
    .await;
    until("B sees read", 20, || {
        b.store().message(&a1).unwrap().unwrap().state == "read"
    })
    .await;

    // Reading on A clears unread on B.
    assert_eq!(
        b.store()
            .chats()
            .unwrap()
            .iter()
            .find(|c| c.peer == maya.address())
            .unwrap()
            .unread,
        1
    );
    a.mark_read(&maya.address()).await.unwrap();
    until("B unread cleared", 20, || {
        b.store()
            .chats()
            .unwrap()
            .iter()
            .find(|c| c.peer == maya.address())
            .is_some_and(|c| c.unread == 0)
    })
    .await;

    // A long body (over the 64 KiB sync cut) arrives whole.
    let long: String = "0123456789abcdef\n".repeat(10_000); // 170 KB
    let m2 = uid("m2-");
    m.send(msg(&m2, &alex.address(), &long)).unwrap();
    until("A gets long m2 whole", 20, || {
        a.store()
            .message(&m2)
            .unwrap()
            .is_some_and(|x| x.body == long)
    })
    .await;

    // Deleting on A removes our copy from B too; maya keeps hers.
    a.delete_message(&m1).await.unwrap();
    until("B drops m1", 20, || {
        b.store().message(&m1).unwrap().is_none()
    })
    .await;
    assert!(m.store().message(&m1).unwrap().is_some());

    // A third device starting fresh gets the history, without notifications.
    let (c, hc) = device(&alex, "alex-tablet", dir.path());
    c.start().unwrap();
    c.add_hub(&hub).unwrap();
    until("C gets history", 20, || {
        c.store().message(&a1).unwrap().is_some() && c.store().message(&m2).unwrap().is_some()
    })
    .await;
    assert!(
        c.store().message(&m1).unwrap().is_none(),
        "deleted stays deleted"
    );
    assert!(
        !hc.events
            .lock()
            .unwrap()
            .iter()
            .any(|e| matches!(e, Event::Incoming { .. })),
        "history is not news"
    );
    assert_eq!(c.store().message(&a1).unwrap().unwrap().state, "read");

    // The device list knows all three.
    let devs = a.devices().await;
    for d in ["alex-pc", "alex-phone", "alex-tablet"] {
        assert!(devs.iter().any(|x| x.device_id == d), "{d} in {devs:?}");
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn resumable_upload_continues_after_a_break_and_big_files_go_through_the_engine() {
    use hubchat_core::hub::CancelFlag;
    use hubchat_core::store::NewAttachment;
    use hubchat_core::{HubAddress, HubClient};
    let Ok(hub) = std::env::var("HUBCHAT_V2_HUB") else {
        eprintln!("SKIPPED: set HUBCHAT_V2_HUB to a scratch mail hub v2.0");
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let alex = Identity::generate("alex").unwrap();
    let c = HubClient::new(HubAddress::parse(&hub).unwrap());
    c.register(
        &alex,
        &Profile {
            kind: "person".into(),
            ..Default::default()
        },
    )
    .await
    .unwrap();

    // 24 MB file; the first attempt is cut off part-way.
    let src = dir.path().join("big.bin");
    let payload: Vec<u8> = (0..24_000_000u32)
        .map(|i| (i.wrapping_mul(2654435761) >> 13) as u8)
        .collect();
    std::fs::write(&src, &payload).unwrap();
    let st = c
        .open_upload(&alex, "big.bin", payload.len() as u64)
        .await
        .unwrap();
    let cancel = CancelFlag::default();
    let c2 = cancel.clone();
    let progress: hubchat_core::hub::Progress = Arc::new(move |done, _| {
        if done > 10_000_000 {
            c2.cancel();
        }
    });
    let f = tokio::fs::File::open(&src).await.unwrap();
    assert!(
        c.resume_upload(&alex, &st.id, f, Some(progress), cancel)
            .await
            .is_err(),
        "first attempt breaks"
    );
    let mid = c.upload_state(&alex, &st.id).await.unwrap();
    assert!(
        mid.offset > 0 && mid.offset < payload.len() as u64,
        "hub kept part: {}",
        mid.offset
    );
    let f = tokio::fs::File::open(&src).await.unwrap();
    let done = c
        .resume_upload(&alex, &st.id, f, None, CancelFlag::default())
        .await
        .unwrap();
    assert!(done.complete && done.offset == payload.len() as u64);
    let got = dir.path().join("got.bin");
    c.download_file(&alex, &st.id, &got, None, CancelFlag::default())
        .await
        .unwrap();
    assert_eq!(
        std::fs::read(&got).unwrap(),
        payload,
        "resumed file is byte-identical"
    );

    // Through the engine: a 24 MB attachment uses the resumable path.
    let maya = Identity::generate("maya").unwrap();
    let (a, _) = device(&alex, "alex-pc", dir.path());
    let (m, _) = device(&maya, "maya-pc", dir.path());
    for e in [&a, &m] {
        e.start().unwrap();
        e.add_hub(&hub).unwrap();
        until("connected", 15, || {
            e.hub_statuses()
                .iter()
                .any(|s| s.state == HubState::Connected)
        })
        .await;
    }
    let id = uid("big-");
    let mut out = msg(&id, &maya.address(), "the big file");
    out.attachments.push(NewAttachment {
        name: "big.bin".into(),
        bytes: payload.len() as u64,
        source: src.to_string_lossy().into(),
    });
    a.send(out).unwrap();
    until("maya gets the big file", 60, || {
        m.store().message(&id).unwrap().is_some()
    })
    .await;
    let got = m.store().message(&id).unwrap().unwrap();
    let path = m.download(&id, &got.attachments[0].local_id).await.unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), payload);
}

/// A person registered as "chat" (on a v1 hub, imported into v2 as it was)
/// becomes "person" on its next connect: the engine registers with the kind
/// it wants every time (user 19:57Z; needs a hub with mailhub fd0409d).
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_chat_registration_becomes_a_person_on_connect() {
    let Ok(hub) = std::env::var("HUBCHAT_V2_HUB") else {
        eprintln!("SKIPPED: set HUBCHAT_V2_HUB to a scratch mail hub v2.0");
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let me = Identity::generate(&uid("kind")).unwrap();
    let client = hubchat_core::HubClient::new(hubchat_core::HubAddress::parse(&hub).unwrap());
    let old = Profile {
        kind: "chat".into(),
        org_name: me.id().into(),
        username: me.id().into(),
        blurb: String::new(),
    };
    client.register(&me, &old).await.unwrap();
    let kind = |r: Vec<hubchat_core::hub::RosterEntry>| {
        r.into_iter()
            .find(|e| e.slug == me.address())
            .map(|e| e.kind)
    };
    assert_eq!(kind(client.roster(&me).await.unwrap()).as_deref(), Some("chat"));

    let (e, _h) = device(&me, "pc", dir.path());
    e.start().unwrap();
    e.add_hub(&hub).unwrap();
    until("connected", 20, || {
        e.hub_statuses()
            .iter()
            .any(|s| s.state == HubState::Connected)
    })
    .await;
    assert_eq!(kind(client.roster(&me).await.unwrap()).as_deref(), Some("person"));
    e.shutdown();
    let _ = client.unregister(&me).await;
}

/// Android's periodic mode (design D6): a check starts the sessions over and
/// returns once every hub has answered or parked its request (no news) and
/// nothing waits to be sent; what arrived meanwhile is in.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_check_takes_in_what_arrived_and_returns() {
    let Ok(hub) = std::env::var("HUBCHAT_V2_HUB") else {
        eprintln!("SKIPPED: set HUBCHAT_V2_HUB to a scratch mail hub v2.0");
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let ann = Identity::generate(&uid("chka")).unwrap();
    let bob = Identity::generate(&uid("chkb")).unwrap();
    let (a, ha) = device(&ann, "phone", dir.path());
    let (b, _hb) = device(&bob, "pc", dir.path());
    for e in [&a, &b] {
        e.start().unwrap();
        e.add_hub(&hub).unwrap();
    }
    for e in [&a, &b] {
        until("connected", 20, || {
            e.hub_statuses()
                .iter()
                .any(|s| s.state == HubState::Connected)
        })
        .await;
    }
    // nothing new: done once the fresh sync is parked
    let t0 = Instant::now();
    assert!(a.check_now(Duration::from_secs(30)).await, "a quiet check finishes");
    let quiet = t0.elapsed();
    assert!(quiet < Duration::from_secs(15), "a quiet check took {quiet:?}");

    // something to send and something to take in
    let out = uid("out-");
    a.send(msg(&out, &bob.address(), "sent by the check")).unwrap();
    let inc = uid("in-");
    b.send(msg(&inc, &ann.address(), "arrived while away")).unwrap();
    until("B sent it", 15, || {
        b.store()
            .message(&inc)
            .unwrap()
            .is_some_and(|m| m.state != "queued" && m.state != "sending")
    })
    .await;
    assert!(a.check_now(Duration::from_secs(30)).await, "the check finishes");
    assert!(a.store().message(&inc).unwrap().is_some(), "the message is in");
    assert!(
        a.store().queued().unwrap().is_empty(),
        "nothing waits to be sent"
    );
    until("B gets A's", 15, || b.store().message(&out).unwrap().is_some()).await;
    assert!(ha
        .events
        .lock()
        .unwrap()
        .iter()
        .any(|e| matches!(e, Event::Incoming { id, .. } if *id == inc)));
    a.shutdown();
    b.shutdown();
}

/// While one of an identity's devices is in use, a message to it makes no
/// notification on its other devices (user 23:50Z; hub feature "active",
/// mailhub a2207f5). Needs a hub with that feature; skipped otherwise.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn another_device_in_use_keeps_this_one_quiet() {
    let Ok(hub) = std::env::var("HUBCHAT_V2_HUB") else {
        eprintln!("SKIPPED: set HUBCHAT_V2_HUB to a scratch mail hub v2.0");
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let ann = Identity::generate(&uid("acta")).unwrap();
    let bob = Identity::generate(&uid("actb")).unwrap();
    let (pc, _hpc) = device(&ann, "ann-pc", dir.path());
    let (phone, hphone) = device(&ann, "ann-phone", dir.path());
    let (b, _hb) = device(&bob, "bob-pc", dir.path());
    for e in [&pc, &phone, &b] {
        e.start().unwrap();
        e.add_hub(&hub).unwrap();
    }
    for e in [&pc, &phone, &b] {
        until("connected", 15, || {
            e.hub_statuses()
                .iter()
                .any(|s| s.state == HubState::Connected)
        })
        .await;
    }
    if !pc.hub_statuses()[0].features.iter().any(|f| f == "active") {
        eprintln!("SKIPPED: this hub has no 'active' feature");
        return;
    }
    let quiet_of = |id: &str| {
        hphone.events.lock().unwrap().iter().find_map(|e| match e {
            Event::Incoming { id: i, quiet, .. } if i == id => Some(*quiet),
            _ => None,
        })
    };

    // nobody using a device: the phone notifies
    let m0 = uid("m0-");
    b.send(msg(&m0, &ann.address(), "nobody looking")).unwrap();
    until("phone gets m0", 20, || quiet_of(&m0).is_some()).await;
    assert_eq!(quiet_of(&m0), Some(false), "no device in use");

    // the PC in use: the phone keeps quiet
    pc.set_active(true);
    tokio::time::sleep(Duration::from_millis(1500)).await;
    let m1 = uid("m1-");
    b.send(msg(&m1, &ann.address(), "while the pc is in use")).unwrap();
    until("phone gets m1", 20, || quiet_of(&m1).is_some()).await;
    assert_eq!(quiet_of(&m1), Some(true), "the pc is in use");

    // the PC put down: the phone notifies again
    pc.set_active(false);
    tokio::time::sleep(Duration::from_millis(1500)).await;
    let m2 = uid("m2-");
    b.send(msg(&m2, &ann.address(), "the pc was put down")).unwrap();
    until("phone gets m2", 20, || quiet_of(&m2).is_some()).await;
    assert_eq!(quiet_of(&m2), Some(false), "the pc was put down");
    for e in [&pc, &phone, &b] {
        e.shutdown();
    }
}
