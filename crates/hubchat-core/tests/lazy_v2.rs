//! Lazy history (mail hub v2.0.1, `lazy_history`): a new device starts from
//! now and pages older mail in as the user scrolls back, one row per id
//! whichever hubs hold it. A hub without the feature still syncs from the
//! beginning.
//! Set HUBCHAT_LAZY_HUB and HUBCHAT_LAZY_HUB2 to two scratch hubs v2.0.1 with
//! separate databases (skipped when HUBCHAT_LAZY_HUB is unset; the second
//! defaults to the first), and HUBCHAT_V2_HUB to a hub v2.0.0 for the
//! fallback.

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use hubchat_core::engine::{Engine, Event, Host};
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
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

fn uid(p: &str) -> String {
    format!("{p}{}", &uuid::Uuid::new_v4().simple().to_string()[..8])
}

fn url(a: &str) -> String {
    HubAddress::parse(a).unwrap().to_string()
}

fn lazy_hubs() -> Option<(String, String)> {
    let a = std::env::var("HUBCHAT_LAZY_HUB").ok()?;
    let b = std::env::var("HUBCHAT_LAZY_HUB2").unwrap_or_else(|_| a.clone());
    Some((a, b))
}

/// A new device of `me` on `hubs`, once every hub has answered its first
/// sync to the end.
async fn new_device(me: &Identity, hubs: &[&str], dir: &std::path::Path) -> Arc<Engine> {
    let id = uid("dev-");
    let host = Arc::new(TestHost {
        downloads: dir.join(&id),
    });
    let store = Arc::new(Store::open_in_memory().unwrap());
    let d = Engine::new(store, me.clone(), profile(me.id()), host);
    d.set_device(&id, &id);
    d.start().unwrap();
    for h in hubs {
        d.add_hub(h).unwrap();
    }
    until("the new device's first sync", 120, || {
        let s = d.hub_statuses();
        s.len() == hubs.len() && s.iter().all(|s| s.answered_ms.is_some())
    })
    .await;
    d
}

/// Register both on `hub`; the client to send through.
async fn on_hub(hub: &str, a: &Identity, b: &Identity) -> HubClient {
    let c = HubClient::new(HubAddress::parse(hub).unwrap());
    c.register(a, &profile("alex")).await.unwrap();
    c.register(b, &profile("pat")).await.unwrap();
    c
}

/// `n` messages from `from` to `to` through `c`, ids `<prefix><i>`.
async fn send_many(c: &HubClient, from: &Identity, to: &Identity, prefix: &str, n: usize) -> Vec<String> {
    let mut ids = Vec::new();
    for chunk in (0..n).collect::<Vec<_>>().chunks(16) {
        let sends = chunk.iter().map(|i| {
            let mut out = Outgoing::new(&to.address(), &format!("old {i}"));
            out.id = format!("{prefix}{i:05}");
            ids.push(out.id.clone());
            async move { c.send(from, &out).await.unwrap() }
        });
        futures_util::future::join_all(sends.collect::<Vec<_>>()).await;
    }
    ids
}

/// What the chat shows (the engine's view, below the history floor left out).
fn shown(d: &Engine, peer: &str) -> Vec<String> {
    d.chat(peer, None, None, 100_000)
        .unwrap()
        .into_iter()
        .map(|m| m.id)
        .collect()
}

fn chat_ids(d: &Engine, peer: &str) -> Vec<String> {
    d.store()
        .chat(peer, None, None, 100_000)
        .unwrap()
        .into_iter()
        .map(|m| m.id)
        .collect()
}

/// Scroll back until no hub has more; the number of pages it took.
async fn page_to_start(d: &Engine, peer: &str) -> usize {
    let mut pages = 0;
    loop {
        let r = d.load_older(peer).await.unwrap();
        assert!(r.unreachable.is_empty(), "a hub couldn't be asked: {r:?}");
        pages += 1;
        if r.more.is_empty() {
            return pages;
        }
        assert!(pages < 100, "paging never reached the start");
    }
}

/// A new device shows none of the old mail at first, gets live mail at
/// once, and paging back reaches the chat's start with every message once.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_new_device_starts_from_now_and_pages_back_to_the_start() {
    let Some((hub, _)) = lazy_hubs() else {
        eprintln!("SKIPPED: set HUBCHAT_LAZY_HUB to a scratch mail hub v2.0.1");
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let alex = Identity::generate("alex").unwrap();
    let pat = Identity::generate("pat").unwrap();
    let c = on_hub(&hub, &alex, &pat).await;
    let mut old = send_many(&c, &pat, &alex, &uid("in-"), 120).await;
    old.extend(send_many(&c, &alex, &pat, &uid("out-"), 15).await);

    let d = new_device(&alex, &[&hub], dir.path()).await;
    let st = &d.hub_statuses()[0];
    assert!(st.clock_offset_ms.is_some(), "no clock offset from the hub's now");
    // the chat list comes from the hub's: the newest message and the count
    let chats = d.store().chats().unwrap();
    assert_eq!(chats.len(), 1);
    assert_eq!(chats[0].peer, pat.address());
    assert_eq!(chats[0].last.id, *old.last().unwrap(), "the newest message");
    assert_eq!(chats[0].unread, 120);
    assert_eq!(
        chat_ids(&d, &pat.address()),
        vec![old.last().unwrap().clone()],
        "old mail came with the first sync"
    );
    assert!(shown(&d, &pat.address()).is_empty(), "the chat shows nothing older yet");

    // a receipt for an old message (another device read it): no pop-in
    let t = hubchat_core::engine::now();
    c.receipts(&alex, &[(old[0].clone(), "read", t)]).await.unwrap();
    // live mail still arrives
    let mut live = Outgoing::new(&alex.address(), "new");
    live.id = uid("live-");
    c.send(&pat, &live).await.unwrap();
    until("the live message", 30, || {
        d.store().message(&live.id).unwrap().is_some()
    })
    .await;
    assert!(
        d.store().message(&old[0]).unwrap().is_none(),
        "an old message showed up before it was paged in"
    );
    assert_eq!(shown(&d, &pat.address()), vec![live.id.clone()]);

    let pages = page_to_start(&d, &pat.address()).await;
    let have = chat_ids(&d, &pat.address());
    let distinct: BTreeSet<_> = have.iter().cloned().collect();
    assert_eq!(have.len(), distinct.len(), "duplicates");
    let mut want: BTreeSet<_> = old.iter().cloned().collect();
    want.insert(live.id.clone());
    assert_eq!(distinct, want, "gaps after paging back");
    assert_eq!(pages, 3, "135 messages in pages of 50");
    assert_eq!(shown(&d, &pat.address()).len(), 136, "all of it shows");
    // the one read elsewhere came with its receipt
    assert!(d.store().message(&old[0]).unwrap().unwrap().seen);
    d.mark_read(&pat.address()).await.unwrap();
    assert_eq!(d.store().unread(&pat.address()).unwrap(), 0);
    // at the start: nothing more to ask
    let r = d.load_older(&pat.address()).await.unwrap();
    assert_eq!((r.added, r.more.len()), (0, 0));
    d.shutdown();
}

/// Two hubs: a message both hold is stored once and the paging of both
/// reaches the start; each hub's own messages all arrive.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn paging_two_hubs_stores_a_shared_message_once() {
    let Some((hub_a, hub_b)) = lazy_hubs() else {
        eprintln!("SKIPPED: set HUBCHAT_LAZY_HUB(2) to scratch mail hubs v2.0.1");
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let alex = Identity::generate("alex").unwrap();
    let pat = Identity::generate("pat").unwrap();
    let ca = on_hub(&hub_a, &alex, &pat).await;
    let cb = on_hub(&hub_b, &alex, &pat).await;
    let mut want: BTreeSet<String> = BTreeSet::new();
    want.extend(send_many(&ca, &pat, &alex, &uid("a-"), 60).await);
    let shared = uid("both-");
    want.extend(send_many(&ca, &pat, &alex, &shared, 10).await);
    want.extend(send_many(&cb, &pat, &alex, &shared, 10).await);
    want.extend(send_many(&cb, &pat, &alex, &uid("b-"), 40).await);

    let d = new_device(&alex, &[&hub_a, &hub_b], dir.path()).await;
    let peer = pat.address();
    // each hub's chat list counts its own (the 10 both hold, twice)
    let unread = d.store().unread(&peer).unwrap();
    assert_eq!(unread, if hub_a == hub_b { 110 } else { 120 });
    assert!(shown(&d, &peer).is_empty());
    // no pop-ins: each page only adds older messages than those on screen
    let mut on_screen: Vec<String> = Vec::new();
    let mut pages = 0;
    loop {
        let r = d.load_older(&peer).await.unwrap();
        let now = shown(&d, &peer);
        let kept = &now[now.len() - on_screen.len()..];
        assert_eq!(kept, on_screen.as_slice(), "page {pages}: a message appeared between messages on screen");
        on_screen = now;
        pages += 1;
        if r.more.is_empty() {
            break;
        }
        assert!(pages < 20);
    }
    assert_eq!(d.store().unread(&peer).unwrap(), 110, "every unread one loaded, each once");
    let have = chat_ids(&d, &pat.address());
    let distinct: BTreeSet<_> = have.iter().cloned().collect();
    assert_eq!(have.len(), distinct.len(), "duplicates");
    assert_eq!(distinct, want, "gaps");
    assert_eq!(want.len(), 110);
    if hub_a != hub_b {
        let hubs = d.store().message_hubs(&format!("{shared}00003")).unwrap();
        assert_eq!(hubs.len(), 2, "a shared message should know both hubs: {hubs:?}");
    }
    for h in [&hub_a, &hub_b] {
        let m = d.store().history_mark(&url(h), &pat.address()).unwrap().unwrap();
        assert!(m.done && m.oldest_ms.is_some(), "{h}: {m:?}");
    }
    d.shutdown();
}

/// A hub v2.0.0 has no lazy history: the device syncs from the beginning
/// there, and scrolling back asks it nothing.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn an_older_hub_still_syncs_from_the_beginning() {
    let Ok(hub) = std::env::var("HUBCHAT_V2_HUB") else {
        eprintln!("SKIPPED: set HUBCHAT_V2_HUB to a scratch mail hub v2.0.0");
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let alex = Identity::generate("alex").unwrap();
    let pat = Identity::generate("pat").unwrap();
    let c = on_hub(&hub, &alex, &pat).await;
    let health = c.healthz().await.unwrap();
    assert!(!health.supports("lazy_history"), "{hub} is not a hub v2.0.0");
    let old: BTreeSet<_> = send_many(&c, &pat, &alex, &uid("in-"), 20).await.into_iter().collect();

    let d = new_device(&alex, &[&hub], dir.path()).await;
    until("the full history", 30, || {
        chat_ids(&d, &pat.address()).len() == 20
    })
    .await;
    let have: BTreeSet<_> = chat_ids(&d, &pat.address()).into_iter().collect();
    assert_eq!(have, old);
    let r = d.load_older(&pat.address()).await.unwrap();
    assert_eq!((r.added, r.more.len(), r.unreachable.len()), (0, 0, 0));
    d.shutdown();
}

/// Measurement (run with --ignored): a new device's first sync on a large
/// history, from the beginning (hub v2.0.0) against from now (v2.0.1).
/// HUBCHAT_LAZY_N messages each (default 2000).
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore]
async fn measure_first_sync_full_against_from_now() {
    let (Some((lazy, _)), Ok(full)) = (lazy_hubs(), std::env::var("HUBCHAT_V2_HUB")) else {
        eprintln!("SKIPPED: set HUBCHAT_LAZY_HUB and HUBCHAT_V2_HUB");
        return;
    };
    let n: usize = std::env::var("HUBCHAT_LAZY_N")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(2000);
    let dir = tempfile::tempdir().unwrap();
    for hub in [&full, &lazy] {
        let alex = Identity::generate("alex").unwrap();
        let pat = Identity::generate("pat").unwrap();
        let c = on_hub(hub, &alex, &pat).await;
        send_many(&c, &pat, &alex, &uid("m-"), n).await;
        let t0 = Instant::now();
        let d = new_device(&alex, &[hub], dir.path()).await;
        let first = t0.elapsed();
        let held = chat_ids(&d, &pat.address()).len();
        let t1 = Instant::now();
        let r = d.load_older(&pat.address()).await.unwrap();
        let page = t1.elapsed();
        eprintln!(
            "MEASURE {hub}: {n} messages, first sync {first:?} holding {held}; one older page {page:?} (+{})",
            r.added
        );
        d.shutdown();
    }
}

/// A TCP forwarder to a hub that the test takes down and brings back on the
/// same port (as in multihub_v2.rs).
struct Forwarder {
    port: u16,
    target: String,
    tasks: Arc<std::sync::Mutex<Vec<tokio::task::JoinHandle<()>>>>,
}

impl Forwarder {
    fn start(target: &str) -> Self {
        let port = std::net::TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        let f = Forwarder {
            port,
            target: target.trim_start_matches("http://").to_owned(),
            tasks: Arc::new(std::sync::Mutex::new(Vec::new())),
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

/// A hub that is down: scrolling back names it and doesn't wait for it
/// (the other hub's messages all show); once it is back its messages fill
/// in, and none of those already on screen go away.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_hub_that_is_down_is_named_and_fills_in_when_back() {
    let Some((hub_a, hub_b)) = lazy_hubs().filter(|(a, b)| a != b) else {
        eprintln!("SKIPPED: set HUBCHAT_LAZY_HUB and HUBCHAT_LAZY_HUB2 to two scratch mail hubs v2.0.1");
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let alex = Identity::generate("alex").unwrap();
    let pat = Identity::generate("pat").unwrap();
    let ca = on_hub(&hub_a, &alex, &pat).await;
    let cb = on_hub(&hub_b, &alex, &pat).await;
    let on_a = send_many(&ca, &pat, &alex, &uid("a-"), 30).await;
    let on_b = send_many(&cb, &pat, &alex, &uid("b-"), 20).await;
    let fwd = Forwarder::start(&hub_b);
    let d = new_device(&alex, &[&hub_a, &fwd.addr()], dir.path()).await;
    let peer = pat.address();
    fwd.down();
    until("hub B seen down", 60, || {
        d.hub_statuses().iter().any(|s| {
            s.url == url(&fwd.addr()) && s.state == hubchat_core::engine::HubState::Disconnected
        })
    })
    .await;
    let r = d.load_older(&peer).await.unwrap();
    assert_eq!(r.unreachable, vec![url(&fwd.addr())]);
    let r = d.load_older(&peer).await.unwrap();
    assert!(r.more.is_empty(), "{r:?}");
    let before_back = shown(&d, &peer);
    let a_set: BTreeSet<_> = on_a.iter().cloned().collect();
    // hub B's newest message came with its chat list
    let want: BTreeSet<_> = a_set.iter().cloned().chain([on_b.last().unwrap().clone()]).collect();
    assert_eq!(before_back.iter().cloned().collect::<BTreeSet<_>>(), want, "hub A's all show");
    fwd.up();
    d.retry_now();
    until("hub B back", 60, || {
        d.hub_statuses().iter().all(|s| s.state == hubchat_core::engine::HubState::Connected)
    })
    .await;
    // back but not paged yet: what was on screen still is
    let reconnected = shown(&d, &peer);
    assert!(
        before_back.iter().all(|id| reconnected.contains(id)),
        "messages went off screen when hub B reconnected: {} of {} left",
        reconnected.len(),
        before_back.len()
    );
    page_to_start(&d, &peer).await;
    let after = shown(&d, &peer);
    let all: BTreeSet<_> = on_a.iter().chain(on_b.iter()).cloned().collect();
    assert_eq!(after.iter().cloned().collect::<BTreeSet<_>>(), all, "hub B's filled in");
    assert!(before_back.iter().all(|id| after.contains(id)), "a message on screen went away");
    d.shutdown();
}

/// B2 follow-up: "Delete chat" while a hub with lazy history is down. When
/// it is back, the messages it received before the delete go too, those
/// this device never loaded included; one that came after stays.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn delete_chat_on_a_hub_that_was_down_takes_what_this_device_never_loaded() {
    let Some((hub, _)) = lazy_hubs() else {
        eprintln!("SKIPPED: set HUBCHAT_LAZY_HUB to a scratch mail hub v2.0.1");
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let alex = Identity::generate("alex").unwrap();
    let pat = Identity::generate("pat").unwrap();
    let c = on_hub(&hub, &alex, &pat).await;
    let old = send_many(&c, &pat, &alex, &uid("old-"), 60).await;
    let fwd = Forwarder::start(&hub);
    let d = new_device(&alex, &[&fwd.addr()], dir.path()).await;
    let peer = pat.address();
    // this device holds only the chat list's newest message
    assert_eq!(chat_ids(&d, &peer).len(), 1);
    fwd.down();
    until("the hub seen down", 60, || {
        d.hub_statuses()[0].state == hubchat_core::engine::HubState::Disconnected
    })
    .await;
    d.delete_chat(&peer).await.unwrap();
    assert!(chat_ids(&d, &peer).is_empty());
    // (the delete's time is our clock made the hub's: allow for the error)
    tokio::time::sleep(Duration::from_millis(500)).await;
    // mail that arrives after the delete stays
    let mut later = Outgoing::new(&alex.address(), "after the delete");
    later.id = uid("later-");
    c.send(&pat, &later).await.unwrap();
    fwd.up();
    d.retry_now();
    let hub_url = url(&fwd.addr());
    until("the owed chat delete sent", 60, || {
        d.store().chat_deletes(&hub_url).unwrap().is_empty()
            && d.hub_statuses()[0].state == hubchat_core::engine::HubState::Connected
    })
    .await;
    let mut left = Vec::new();
    let mut before = hubchat_core::hub_v2::Before::Time(4_102_444_800_000);
    loop {
        let p = c.history(&alex, &peer, &before, 200).await.unwrap();
        left.extend(p.messages.into_iter().map(|m| m.env.id));
        match p.before {
            Some(b) => before = hubchat_core::hub_v2::Before::Cursor(b),
            None => break,
        }
    }
    assert_eq!(left, vec![later.id.clone()], "{} of {} old ones left", left.len() - 1, old.len());
    d.shutdown();
}
