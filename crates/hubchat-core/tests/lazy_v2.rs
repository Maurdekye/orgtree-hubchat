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
    assert!(
        chat_ids(&d, &pat.address()).is_empty(),
        "old mail came with the first sync"
    );

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
    assert_eq!(
        chat_ids(&d, &pat.address()),
        vec![live.id.clone()],
        "an old message showed up before it was paged in"
    );

    let pages = page_to_start(&d, &pat.address()).await;
    let have = chat_ids(&d, &pat.address());
    let distinct: BTreeSet<_> = have.iter().cloned().collect();
    assert_eq!(have.len(), distinct.len(), "duplicates");
    let mut want: BTreeSet<_> = old.iter().cloned().collect();
    want.insert(live.id.clone());
    assert_eq!(distinct, want, "gaps after paging back");
    assert_eq!(pages, 3, "135 messages in pages of 50");
    // the one read elsewhere came with its receipt
    assert!(d.store().message(&old[0]).unwrap().unwrap().seen);
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
    assert!(chat_ids(&d, &pat.address()).is_empty());
    page_to_start(&d, &pat.address()).await;
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
