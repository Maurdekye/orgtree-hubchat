//! One identity on two mail hubs v2.0 (B2, B3): deletes reach every hub
//! that holds a copy, one that was down included, and a hub starting its
//! sync over doesn't take messages another hub still holds.
//! Set HUBCHAT_V2_HUB and HUBCHAT_V2_HUB2 to two scratch v2 hubs with
//! separate databases (skipped when HUBCHAT_V2_HUB is unset). With only the
//! first, both "hubs" are that one: the tests then still cover a hub that
//! was down getting the delete when it is back, not a second hub's copy.
//! The hub that goes down is reached through a forwarder the test stops.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use hubchat_core::engine::{Engine, Event, Host, HubState};
use hubchat_core::hub::{Outgoing, Profile};
use hubchat_core::store::Store;
use hubchat_core::{HubAddress, HubClient, Identity};

struct TestHost {
    downloads: PathBuf,
}

impl Host for TestHost {
    fn event(&self, _ev: Event) {}
    fn open_source(&self, source: &str) -> std::io::Result<std::fs::File> {
        std::fs::File::open(source)
    }
    fn download_dir(&self) -> PathBuf {
        self.downloads.clone()
    }
}

fn device(me: &Identity, device_id: &str, dir: &std::path::Path) -> Arc<Engine> {
    let host = Arc::new(TestHost {
        downloads: dir.join(device_id),
    });
    let store = Arc::new(Store::open_in_memory().unwrap());
    let e = Engine::new(store, me.clone(), profile(me.id()), host);
    e.set_device(device_id, device_id);
    e
}

fn profile(name: &str) -> Profile {
    Profile {
        kind: "person".into(),
        org_name: name.into(),
        username: name.into(),
        blurb: String::new(),
    }
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

fn uid(p: &str) -> String {
    format!("{p}{}", &uuid::Uuid::new_v4().simple().to_string()[..8])
}

fn url(a: &str) -> String {
    HubAddress::parse(a).unwrap().to_string()
}

/// HUBCHAT_V2_HUB and HUBCHAT_V2_HUB2 (defaults to the first).
fn two_hubs() -> Option<(String, String)> {
    let a = std::env::var("HUBCHAT_V2_HUB").ok()?;
    let b = std::env::var("HUBCHAT_V2_HUB2").unwrap_or_else(|_| a.clone());
    Some((a, b))
}

/// A TCP forwarder to a hub that the test takes down and brings back on the
/// same port: an engine connected through it sees that hub go offline.
struct Forwarder {
    port: u16,
    target: String,
    tasks: Arc<Mutex<Vec<tokio::task::JoinHandle<()>>>>,
}

impl Forwarder {
    async fn start(target: &str) -> Self {
        let port = std::net::TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        let f = Forwarder {
            port,
            target: target.trim_start_matches("http://").to_owned(),
            tasks: Arc::new(Mutex::new(Vec::new())),
        };
        f.up();
        f
    }

    fn addr(&self) -> String {
        format!("127.0.0.1:{}", self.port)
    }

    fn up(&self) {
        let sock = tokio::net::TcpSocket::new_v4().unwrap();
        sock.set_reuseaddr(true).unwrap();
        sock.bind(([127, 0, 0, 1], self.port).into()).unwrap();
        let listener = sock.listen(64).unwrap();
        let (target, tasks) = (self.target.clone(), self.tasks.clone());
        let accept = tokio::spawn(async move {
            while let Ok((mut inbound, _)) = listener.accept().await {
                let target = target.clone();
                let pipe = tokio::spawn(async move {
                    if let Ok(mut out) = tokio::net::TcpStream::connect(&target).await {
                        let _ = tokio::io::copy_bidirectional(&mut inbound, &mut out).await;
                    }
                });
                tasks.lock().unwrap().push(pipe);
            }
        });
        self.tasks.lock().unwrap().push(accept);
    }

    /// Close the port and cut every open connection.
    fn down(&self) {
        for t in self.tasks.lock().unwrap().drain(..) {
            t.abort();
        }
    }
}

impl Drop for Forwarder {
    fn drop(&mut self) {
        self.down();
    }
}

fn hub_state(e: &Engine, addr: &str) -> Option<HubState> {
    let u = url(addr);
    e.hub_statuses()
        .into_iter()
        .find(|s| s.url == u)
        .map(|s| s.state)
}

/// Every hub of `e` has answered a sync to the end.
fn caught_up(e: &Engine, n: usize) -> bool {
    let s = e.hub_statuses();
    s.len() == n && s.iter().all(|s| s.answered_ms.is_some())
}

/// A fresh device of `me` on `hubs`, once it holds all those hubs still have.
async fn fresh_device(me: &Identity, hubs: &[&str], dir: &std::path::Path) -> Arc<Engine> {
    let d = device(me, &uid("fresh-"), dir);
    d.start().unwrap();
    for h in hubs {
        d.add_hub(h).unwrap();
    }
    until("fresh device caught up", 30, || caught_up(&d, hubs.len())).await;
    d
}

/// Maya puts one message (one id) to alex on each hub, as a send that was
/// retried through the other hub would.
async fn on_both_hubs(maya: &Identity, alex: &Identity, hub_a: &str, hub_b: &str, id: &str) {
    let hubs = if hub_a == hub_b {
        vec![hub_a]
    } else {
        vec![hub_a, hub_b]
    };
    for h in hubs {
        let c = HubClient::new(HubAddress::parse(h).unwrap());
        c.register(alex, &profile("alex")).await.unwrap();
        c.register(maya, &profile("maya")).await.unwrap();
        let mut out = Outgoing::new(&alex.address(), "on two hubs");
        out.id = id.into();
        c.send(maya, &out).await.unwrap();
    }
}

/// Device A has `id` from hub A and then from hub B (through the forwarder,
/// so hub B's copy is the last it heard of); then hub B goes down.
async fn synced_then_b_down(
    alex: &Identity,
    hub_a: &str,
    fwd: &Forwarder,
    id: &str,
    dir: &std::path::Path,
) -> Arc<Engine> {
    let a = device(alex, "alex-pc", dir);
    a.start().unwrap();
    a.add_hub(hub_a).unwrap();
    until("A has it from hub A", 20, || {
        a.store().message(id).unwrap().is_some()
    })
    .await;
    a.add_hub(&fwd.addr()).unwrap();
    until("A synced hub B", 20, || caught_up(&a, 2)).await;
    fwd.down();
    until("A sees hub B down", 40, || {
        hub_state(&a, &fwd.addr()) == Some(HubState::Disconnected)
    })
    .await;
    a
}

/// Hub B is back: A reconnects and has nothing left to delete there.
async fn b_back(a: &Engine, fwd: &Forwarder) {
    fwd.up();
    a.retry_now();
    until("A back on hub B, its queued deletes sent", 30, || {
        hub_state(a, &fwd.addr()) == Some(HubState::Connected)
            && a.store().pending_deletes(&url(&fwd.addr())).unwrap().is_empty()
    })
    .await;
}

/// B2: "Delete for me" while hub B is down deletes on hub A at once and on
/// hub B once it is back: no fresh device gets the message back.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn delete_reaches_every_hub_even_one_that_was_down() {
    let Some((hub_a, hub_b)) = two_hubs() else {
        eprintln!("SKIPPED: set HUBCHAT_V2_HUB (and HUBCHAT_V2_HUB2) to scratch mail hubs v2.0");
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let alex = Identity::generate("alex").unwrap();
    let maya = Identity::generate("maya").unwrap();
    let id = uid("d1-");
    on_both_hubs(&maya, &alex, &hub_a, &hub_b, &id).await;
    let fwd = Forwarder::start(&hub_b).await;
    let a = synced_then_b_down(&alex, &hub_a, &fwd, &id, dir.path()).await;

    a.delete_message(&id).await.unwrap();
    assert!(a.store().message(&id).unwrap().is_none());
    let c = fresh_device(&alex, &[&hub_a], dir.path()).await;
    assert!(
        c.store().message(&id).unwrap().is_none(),
        "hub A kept the message deleted while hub B was down"
    );

    b_back(&a, &fwd).await;
    // hub B on its own: hub A's sync could tell a device the message was
    // deleted and so hide a copy hub B kept
    let d = fresh_device(&alex, &[&hub_b], dir.path()).await;
    assert!(
        d.store().message(&id).unwrap().is_none(),
        "hub B kept the deleted message"
    );
    for e in [&a, &c, &d] {
        e.shutdown();
    }
}

/// B2: "Delete chat" while hub B is down: the same, for the whole chat.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn delete_chat_reaches_every_hub_even_one_that_was_down() {
    let Some((hub_a, hub_b)) = two_hubs() else {
        eprintln!("SKIPPED: set HUBCHAT_V2_HUB (and HUBCHAT_V2_HUB2) to scratch mail hubs v2.0");
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let alex = Identity::generate("alex").unwrap();
    let maya = Identity::generate("maya").unwrap();
    let id = uid("c1-");
    on_both_hubs(&maya, &alex, &hub_a, &hub_b, &id).await;
    let fwd = Forwarder::start(&hub_b).await;
    let a = synced_then_b_down(&alex, &hub_a, &fwd, &id, dir.path()).await;

    a.delete_chat(&maya.address()).await.unwrap();
    let chat = |e: &Engine| e.store().chat(&maya.address(), None, None, 10).unwrap();
    assert!(chat(&a).is_empty());
    let c = fresh_device(&alex, &[&hub_a], dir.path()).await;
    assert!(chat(&c).is_empty(), "hub A kept the chat deleted while hub B was down");

    b_back(&a, &fwd).await;
    // hub B on its own: hub A's sync could tell a device the message was
    // deleted and so hide a copy hub B kept
    let d = fresh_device(&alex, &[&hub_b], dir.path()).await;
    assert!(chat(&d).is_empty(), "hub B kept the deleted chat");
    for e in [&a, &c, &d] {
        e.shutdown();
    }
}

/// B3: hub A no longer knows this device's sync position (a 422 for the
/// cursor) and starts over: a message hub B also holds never leaves the
/// device meanwhile, and both hubs are known to hold it again afterwards.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_hub_starting_over_keeps_what_the_other_hub_holds() {
    let Some((hub_a, hub_b)) = two_hubs() else {
        eprintln!("SKIPPED: set HUBCHAT_V2_HUB (and HUBCHAT_V2_HUB2) to scratch mail hubs v2.0");
        return;
    };
    if hub_a == hub_b {
        eprintln!("SKIPPED: needs HUBCHAT_V2_HUB2, a second hub");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let alex = Identity::generate("alex").unwrap();
    let maya = Identity::generate("maya").unwrap();
    let id = uid("r1-");
    on_both_hubs(&maya, &alex, &hub_a, &hub_b, &id).await;
    let a = device(&alex, "alex-pc", dir.path());
    a.start().unwrap();
    a.add_hub(&hub_a).unwrap();
    until("A has it from hub A", 20, || a.store().message(&id).unwrap().is_some()).await;
    a.add_hub(&hub_b).unwrap();
    until("A synced hub B", 20, || caught_up(&a, 2)).await;
    assert_eq!(a.store().message_hubs(&id).unwrap(), {
        let mut v = vec![url(&hub_a), url(&hub_b)];
        v.sort();
        v
    });

    // hub A's answer to a cursor it doesn't know: 422, start over
    a.store()
        .set_meta(&format!("sync.cursor.{}", url(&hub_a)), "not-a-cursor")
        .unwrap();
    let t0 = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64;
    a.kick();
    let started_over = || {
        a.hub_statuses()
            .iter()
            .find(|s| s.url == url(&hub_a))
            .is_some_and(|s| s.answered_ms.is_some_and(|t| t > t0))
            && a.store().meta(&format!("sync.cursor.{}", url(&hub_a))).unwrap().as_deref()
                != Some("not-a-cursor")
    };
    let deadline = Instant::now() + Duration::from_secs(20);
    while !started_over() {
        assert!(
            a.store().message(&id).unwrap().is_some(),
            "the message left the device while hub A started over"
        );
        assert!(Instant::now() < deadline, "timed out waiting for hub A to start over");
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    assert!(a.store().message(&id).unwrap().is_some());
    until("both hubs hold it again", 10, || a.store().message_hubs(&id).unwrap().len() == 2).await;
    a.shutdown();
}
