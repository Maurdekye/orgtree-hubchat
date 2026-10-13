//! The same hub under another address (user 2026-10-13 18:51Z: the hub was
//! added as `localhost`, removed, and added again as the PC's name): the
//! contacts and chats must survive and stay messageable through the new
//! address.
//! Set HUBCHAT_V2_HUB to a scratch mail hub v2 (skipped when unset). The test
//! reaches that one hub a second way, through a TCP forwarder: another
//! address, the same hub. Each scenario prints what it saw at every step
//! before it asserts, so a failure says where the contacts went.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use hubchat_core::engine::{Engine, Event, Host, HubState};
use hubchat_core::hub::Profile;
use hubchat_core::store::{NewOutgoing, Store};
use hubchat_core::{HubAddress, Identity};

struct TestHost;

impl Host for TestHost {
    fn event(&self, _ev: Event) {}
    fn open_source(&self, source: &str) -> std::io::Result<std::fs::File> {
        std::fs::File::open(source)
    }
    fn download_dir(&self) -> std::path::PathBuf {
        std::env::temp_dir()
    }
}

fn device(me: &Identity, name: &str) -> Arc<Engine> {
    let store = Arc::new(Store::open_in_memory().unwrap());
    let profile = Profile { kind: "person".into(), org_name: name.into(), username: name.into(), blurb: String::new() };
    let e = Engine::new(store, me.clone(), profile, Arc::new(TestHost));
    e.set_device(name, name);
    e.start().unwrap();
    e
}

fn url(a: &str) -> String {
    HubAddress::parse(a).unwrap().to_string()
}

fn uid(p: &str) -> String {
    format!("{p}{}", &uuid::Uuid::new_v4().simple().to_string()[..8])
}

fn msg(id: &str, to: &str, body: &str) -> NewOutgoing {
    NewOutgoing { id: id.into(), peer: to.into(), body: body.into(), kind: None, reply_to: None, attachments: vec![] }
}

/// Wait for `f`; false (not a panic) when it never held, so the scenario
/// can report every step.
async fn until(secs: u64, mut f: impl FnMut() -> bool) -> bool {
    let t0 = Instant::now();
    while !f() {
        if t0.elapsed() >= Duration::from_secs(secs) {
            return false;
        }
        tokio::time::sleep(Duration::from_millis(150)).await;
    }
    true
}

fn connected(e: &Engine, hub: &str) -> bool {
    let u = url(hub);
    e.hub_statuses().iter().any(|s| s.url == u && s.state == HubState::Connected && s.answered_ms.is_some())
}

/// The hubs `e`'s directory lists `who` on (None: not listed at all).
fn listed_on(e: &Engine, who: &str) -> Option<Vec<String>> {
    e.store().directory().unwrap().into_iter().find(|c| c.address == who).map(|c| c.hubs)
}

fn has_chat(e: &Engine, peer: &str) -> bool {
    e.store().chats().unwrap().iter().any(|c| c.peer == peer)
}

fn state(e: &Engine, id: &str) -> String {
    e.store().message(id).unwrap().map_or("missing".into(), |m| m.state)
}

/// A TCP forwarder to the hub: the same hub at another address.
struct Forwarder {
    port: u16,
    tasks: Arc<Mutex<Vec<tokio::task::JoinHandle<()>>>>,
}

impl Forwarder {
    async fn start(target: &str) -> Self {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let target = target.trim_start_matches("http://").to_owned();
        let tasks: Arc<Mutex<Vec<tokio::task::JoinHandle<()>>>> = Arc::default();
        let t = tasks.clone();
        let accept = tokio::spawn(async move {
            while let Ok((mut inbound, _)) = listener.accept().await {
                let target = target.clone();
                t.lock().unwrap().push(tokio::spawn(async move {
                    if let Ok(mut out) = tokio::net::TcpStream::connect(&target).await {
                        let _ = tokio::io::copy_bidirectional(&mut inbound, &mut out).await;
                    }
                }));
            }
        });
        tasks.lock().unwrap().push(accept);
        Forwarder { port, tasks }
    }

    fn addr(&self) -> String {
        format!("127.0.0.1:{}", self.port)
    }
}

impl Drop for Forwarder {
    fn drop(&mut self) {
        for t in self.tasks.lock().unwrap().drain(..) {
            t.abort();
        }
    }
}

#[derive(Clone, Copy, Debug)]
enum Order {
    /// Remove the old address, then add the new one (the user's words).
    RemoveThenAdd,
    /// Add the new address first, then remove the old one.
    AddThenRemove,
}

/// Alex talks with Bob through the hub at `old`, then moves to `new` (the
/// same hub) in `order`, `unregister` as the removal dialog's box was.
/// Returns what went wrong, in order; empty when the contacts survived.
async fn scenario(old: &str, new: &str, order: Order, unregister: bool) -> Vec<String> {
    let tag = format!("[{order:?}, unregister={unregister}]");
    let mut bad = Vec::new();
    let alex = Identity::generate("alex").unwrap();
    let bob = Identity::generate("bob").unwrap();
    let a = device(&alex, "alex-pc");
    let b = device(&bob, "bob-pc");
    a.add_hub(old).unwrap();
    b.add_hub(old).unwrap();
    assert!(until(30, || connected(&a, old) && connected(&b, old)).await, "{tag} both connected at the old address");
    assert!(until(30, || listed_on(&a, &bob.address()).is_some() && listed_on(&b, &alex.address()).is_some()).await, "{tag} each lists the other");
    let first = uid("hi-");
    a.send(msg(&first, &bob.address(), "hi bob")).await.unwrap();
    assert!(until(30, || b.store().message(&first).unwrap().is_some()).await, "{tag} bob got alex's first message");
    let answer = uid("re-");
    b.send(msg(&answer, &alex.address(), "hi alex")).await.unwrap();
    assert!(until(30, || a.store().message(&answer).unwrap().is_some()).await, "{tag} alex got bob's answer");
    eprintln!("{tag} before: bob listed on {:?}, chat with bob: {}", listed_on(&a, &bob.address()), has_chat(&a, &bob.address()));

    match order {
        Order::RemoveThenAdd => {
            a.remove_hub(&url(old), unregister).await.unwrap();
            eprintln!("{tag} old address removed: bob listed on {:?}, chat with bob: {}", listed_on(&a, &bob.address()), has_chat(&a, &bob.address()));
            a.add_hub(new).unwrap();
            if !until(30, || connected(&a, new)).await {
                bad.push(format!("{tag} the new address never connected: {:?}", a.hub_statuses()));
            }
        }
        Order::AddThenRemove => {
            a.add_hub(new).unwrap();
            if !until(30, || connected(&a, new)).await {
                bad.push(format!("{tag} the new address never connected: {:?}", a.hub_statuses()));
            }
            eprintln!("{tag} both addresses: bob listed on {:?}", listed_on(&a, &bob.address()));
            a.remove_hub(&url(old), unregister).await.unwrap();
            // the hub may have just signed this identity out (unregister):
            // give the new address's session time to notice and come back
            tokio::time::sleep(Duration::from_secs(3)).await;
            if !until(30, || connected(&a, new)).await {
                bad.push(format!("{tag} the new address dropped after the old one was removed: {:?}", a.hub_statuses()));
            }
        }
    }

    let back = until(30, || listed_on(&a, &bob.address()).is_some_and(|h| h.contains(&url(new)))).await;
    eprintln!("{tag} after: bob listed on {:?}, chat with bob: {}, hubs {:?}", listed_on(&a, &bob.address()), has_chat(&a, &bob.address()), a.hub_statuses().iter().map(|s| (s.url.clone(), s.state.clone())).collect::<Vec<_>>());
    if !back {
        bad.push(format!("{tag} bob is not in alex's directory through the new address (the composer says he isn't on any hub)"));
    }
    if !has_chat(&a, &bob.address()) {
        bad.push(format!("{tag} alex's chat with bob is gone"));
    }
    if !until(10, || listed_on(&b, &alex.address()).is_some()).await {
        bad.push(format!("{tag} bob no longer lists alex"));
    }
    let again = uid("again-");
    a.send(msg(&again, &bob.address(), "still there?")).await.unwrap();
    if !until(30, || b.store().message(&again).unwrap().is_some()).await {
        bad.push(format!("{tag} alex's new message never reached bob (alex's copy: {})", state(&a, &again)));
    }
    let reply = uid("yes-");
    b.send(msg(&reply, &alex.address(), "yes")).await.unwrap();
    if !until(30, || a.store().message(&reply).unwrap().is_some()).await {
        bad.push(format!("{tag} bob's new message never reached alex (bob's copy: {})", state(&b, &reply)));
    }
    a.shutdown();
    b.shutdown();
    for line in &bad {
        eprintln!("FINDING {line}");
    }
    bad
}

async fn run(order: Order, unregister: bool) {
    let Ok(hub) = std::env::var("HUBCHAT_V2_HUB") else {
        eprintln!("SKIPPED: set HUBCHAT_V2_HUB to a scratch mail hub v2");
        return;
    };
    let fwd = Forwarder::start(&hub).await;
    let bad = scenario(&hub, &fwd.addr(), order, unregister).await;
    assert!(bad.is_empty(), "{bad:#?}");
}

/// What the user did: remove the hub (the dialog's "also remove my address"
/// box is ticked by default), then add the same hub by another name.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn same_hub_readded_at_another_address_keeps_contacts() {
    run(Order::RemoveThenAdd, true).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn same_hub_readded_without_unregistering_keeps_contacts() {
    run(Order::RemoveThenAdd, false).await;
}

/// The other order: the new name is added while the old one is still there,
/// so taking "my address" off the old entry takes it off the same hub.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn same_hub_added_again_then_old_address_removed_keeps_contacts() {
    run(Order::AddThenRemove, true).await;
}
