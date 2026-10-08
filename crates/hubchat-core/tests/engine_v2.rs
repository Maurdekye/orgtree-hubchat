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
