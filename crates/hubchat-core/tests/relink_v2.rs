//! Relinking a device whose key is lost (user report 2026-10-14, Orgtree
//! docket phone-linking-says-existing-hubs-are-invalid-whe). The lost-key
//! screen keeps the device's saved hubs but runs no identity. The user linked
//! their PC again from the phone, and the review called the PC's own saved
//! hubs "invalid input: This saved hub is unavailable; restore the identity
//! and connect it before linking." although they were fine. Coordinator
//! ruling 2026-10-14: with no identity running, an enabled saved hub is
//! reached as setup reaches it; a hub disabled on the device still refuses.
//!
//! The link runs for real on a scratch hub v2: the phone's engine sends the
//! sealed identity to the address the PC's link code makes, and the PC takes
//! it as the shell does (src-tauri link.rs `hc_link_start` and `arrived`):
//! its hub client from `link::start_client` with no engine, polling and the
//! review inside the link's `Reception`, and each hub the identity brings
//! checked through `link::Discovery` with that reception as its origin. The
//! PC's saved hubs are local /healthz stand-ins that count the checks reaching
//! them. (`link::review_hubs` is left out: it takes a loopback hub for the
//! other device's own, and every address here is loopback; for the user's
//! hubs it gave each one a row of its own, as here.)
//!
//! Set HUBCHAT_V2_HUB to a scratch mail hub v2 (skipped when unset).

use std::sync::atomic::{AtomicUsize, Ordering::SeqCst};
use std::sync::Arc;
use std::time::{Duration, Instant};

use hubchat_core::engine::{Engine, Event, Host, HubState};
use hubchat_core::hub::{CancelFlag, Profile};
use hubchat_core::link::{self, Bundle, Discovery, Reception};
use hubchat_core::store::Store;
use hubchat_core::{HubAddress, Identity};

struct Quiet;

impl Host for Quiet {
    fn event(&self, _ev: Event) {}
    fn open_source(&self, source: &str) -> std::io::Result<std::fs::File> {
        std::fs::File::open(source)
    }
    fn download_dir(&self) -> std::path::PathBuf {
        std::env::temp_dir()
    }
}

fn scratch_hub() -> Option<String> {
    let addr = std::env::var("HUBCHAT_V2_HUB").ok()?;
    Some(HubAddress::parse(&addr).unwrap().to_string())
}

async fn until(secs: u64, mut f: impl FnMut() -> bool) -> bool {
    let t0 = Instant::now();
    while !f() {
        if t0.elapsed() >= Duration::from_secs(secs) {
            return false;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    true
}

fn connected(e: &Engine, u: &str) -> bool {
    e.hub_statuses().iter().any(|s| s.url == u && s.state == HubState::Connected && s.answered_ms.is_some())
}

/// A stand-in for one of the PC's saved hubs: answers /healthz as `name` and
/// counts the checks that reach it.
struct StandIn {
    url: String,
    checks: Arc<AtomicUsize>,
    task: tokio::task::JoinHandle<()>,
}

impl Drop for StandIn {
    fn drop(&mut self) {
        self.task.abort();
    }
}

async fn stand_in(name: &str) -> StandIn {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let checks = Arc::new(AtomicUsize::new(0));
    let copy = checks.clone();
    let body = format!(r#"{{"ok":true,"name":"{name}","version":"2.0.4","orgs":3,"features":["sync"]}}"#);
    let task = tokio::spawn(async move {
        while let Ok((mut stream, _)) = listener.accept().await {
            let mut buffer = [0u8; 2048];
            if stream.read(&mut buffer).await.unwrap_or(0) == 0 {
                continue;
            }
            copy.fetch_add(1, SeqCst);
            let reply = format!("HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}", body.len());
            let _ = stream.write_all(reply.as_bytes()).await;
        }
    });
    StandIn { url, checks, task }
}

/// The PC on the lost-key screen: its local data kept (the identity's address
/// noted, its device id, its saved hubs with `enabled` per hub), no engine.
fn lost_key_pc(dir: &std::path::Path, alex: &Identity, saved: &[(&str, bool)]) -> Arc<Store> {
    let s = Store::open(&dir.join("hubchat.sqlite3")).unwrap();
    s.set_meta("identity.address", &alex.address()).unwrap();
    s.set_meta("device.id", "hc-20261014100000-0001").unwrap();
    for (url, enabled) in saved {
        let url = HubAddress::parse(url).unwrap().to_string();
        s.add_hub(&url, "2026-10-14T10:00:00Z").unwrap();
        s.set_hub_enabled(&url, *enabled).unwrap();
    }
    Arc::new(s)
}

/// Alex's phone, signed in and connected to `hub`.
async fn phone_on(alex: &Identity, hub: &str) -> Arc<Engine> {
    let profile = Profile { kind: "person".into(), org_name: "Alex".into(), username: alex.id().into(), blurb: String::new() };
    let e = Engine::new(Arc::new(Store::open_in_memory().unwrap()), alex.clone(), profile, Arc::new(Quiet));
    e.set_device("alex-phone", "Alex's phone");
    e.start().unwrap();
    e.add_hub(hub).unwrap();
    assert!(until(30, || connected(&e, hub)).await, "the phone connected to the scratch hub");
    e
}

/// One review row: a hub the identity brings, and the name that answered its
/// check or the error the PC shows under it.
type Row = (String, Result<String, String>);

/// The review (`arrived`): each hub checked through the link's reception,
/// resolved by the PC's saved policy with no engine, 5 s at most each.
async fn review(pc: &Arc<Store>, reception: &Reception, hubs: &[String]) -> Vec<Row> {
    let store = pc.clone();
    let checks = Discovery::new(move |address| link::start_client(&store, address, || None)).with_origin(reception.clone());
    let mut rows = Vec::new();
    for h in hubs {
        let answer = match tokio::time::timeout(Duration::from_secs(5), checks.health(HubAddress::parse(h).unwrap())).await {
            Ok(Ok(health)) => Ok(health.name),
            Ok(Err(e)) => Err(e.to_string()),
            Err(_) => Err("no answer within 5 s".to_string()),
        };
        rows.push((h.clone(), answer));
    }
    rows
}

/// Link the PC (no engine) through `via` and take the identity the phone
/// sends with `hubs`, as the shell does; returns the review.
async fn relink(pc: &Arc<Store>, phone: &Engine, via: &str, alex: &Identity, hubs: Vec<String>) -> Vec<Row> {
    let code = link::new_link_code();
    let temp = link::link_identity(&code).unwrap();
    // hc_link_start: the transport comes from the same policy, with no engine
    let client = link::start_client(pc, HubAddress::parse(via).unwrap(), || None)
        .unwrap_or_else(|e| panic!("the PC could not start linking through {via}: {e}"));
    let profile = Profile { kind: "chat".into(), org_name: "Alex's PC".into(), username: "link".into(), blurb: "Waiting to be linked to a Hubchat identity".into() };
    client.register(&temp, &profile).await.unwrap();
    // the phone (hc_link_lookup, hc_link_approve): the waiting PC is listed,
    // and the identity goes to it sealed with the code
    let waiting = temp.address();
    assert!(until(30, || !phone.store().hubs_reaching(&waiting).unwrap().is_empty()).await, "the phone sees the waiting PC");
    let bundle = Bundle { id: alex.id().into(), secret: alex.secret().into(), name: "Alex".into(), about: String::new(), hubs, recovery_saved: true };
    phone.send_unlisted(&waiting, &link::seal_for_link(&code, &bundle).unwrap()).await.unwrap();
    // the PC's listener and its review
    let reception = Reception::new(client.clone(), CancelFlag::default());
    let t0 = Instant::now();
    let rows = loop {
        assert!(t0.elapsed() < Duration::from_secs(60), "the identity never reached the PC");
        let p = reception.work(client.poll(&temp, 5)).await.unwrap();
        let ids: Vec<String> = p.messages.iter().map(|m| m.id.clone()).collect();
        let found = p.messages.iter().find_map(|m| link::open_from_link(&code, &m.body).ok().map(|b| (m.id.clone(), b)));
        if let Some((id, arrived)) = found {
            assert_eq!(arrived.identity().unwrap().address(), alex.address());
            let cleanup = async {
                let _ = client.ack(&temp, &ids).await;
                let _ = client.delete_message(&temp, &id).await;
                Ok::<(), hubchat_core::Error>(())
            };
            let checked = async { Ok::<_, hubchat_core::Error>(review(pc, &reception, &arrived.hubs).await) };
            let mut published = None;
            reception.complete(cleanup, checked, |rows| published = Some(rows)).await.unwrap();
            break published.expect("the review was published");
        }
        if !ids.is_empty() {
            let _ = client.ack(&temp, &ids).await;
        }
    };
    let _ = client.unregister(&temp).await;
    for (h, r) in &rows {
        match r {
            Ok(name) => eprintln!("  review  {h}: reached {name}"),
            Err(e) => eprintln!("  review  {h}: {e}"),
        }
    }
    rows
}

fn row(rows: &[Row], url: &str) -> Result<String, String> {
    rows.iter().find(|(h, _)| h == url).map(|(_, r)| r.clone()).unwrap_or_else(|| panic!("no review row for {url}"))
}

/// The user's case: the PC links through an address it hasn't saved (theirs
/// was Orgtree's phone door), and the identity brings the PC's saved hubs.
/// Each saved, enabled one is checked for real and answers; the one disabled
/// on the PC stays refused, unasked.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_lost_key_relink_reaches_the_saved_hubs_it_brings() {
    let Some(hub) = scratch_hub() else {
        eprintln!("SKIPPED: set HUBCHAT_V2_HUB to a scratch mail hub v2");
        return;
    };
    let (pendragon, star, off) = (stand_in("pendragon").await, stand_in("star-system").await, stand_in("switched-off").await);
    let dir = tempfile::tempdir().unwrap();
    let alex = Identity::generate("alex").unwrap();
    let saved = [(pendragon.url.as_str(), true), (star.url.as_str(), true), (off.url.as_str(), false)];
    let pc = lost_key_pc(dir.path(), &alex, &saved);
    let phone = phone_on(&alex, &hub).await;

    let brings = vec![hub.clone(), pendragon.url.clone(), star.url.clone(), off.url.clone()];
    let rows = relink(&pc, &phone, &hub, &alex, brings).await;
    assert!(row(&rows, &hub).is_ok(), "the link's own hub answers");
    assert_eq!(row(&rows, &pendragon.url), Ok("pendragon".to_string()), "a saved, enabled hub of the PC");
    assert_eq!(row(&rows, &star.url), Ok("star-system".to_string()), "a saved, enabled hub of the PC");
    assert_eq!((pendragon.checks.load(SeqCst), star.checks.load(SeqCst)), (1, 1), "each answered its check itself");
    let refused = row(&rows, &off.url);
    assert!(refused.as_ref().is_err_and(|e| e.contains("Enable this saved hub")), "a hub disabled on the PC: {refused:?}");
    assert_eq!(off.checks.load(SeqCst), 0, "a disabled hub is never asked");
    // the review saved nothing and changed no switch
    let mut kept: Vec<(String, bool)> = pc.hubs().unwrap().into_iter().map(|h| (h.url, h.enabled)).collect();
    let mut was: Vec<(String, bool)> = saved.iter().map(|(u, on)| (HubAddress::parse(u).unwrap().to_string(), *on)).collect();
    kept.sort();
    was.sort();
    assert_eq!(kept, was);
    eprintln!("  ok  the PC's saved hubs answered their checks; the disabled one stayed unasked");
    phone.shutdown();
}

/// On the lost-key screen the PC links through a hub it has saved, as typing
/// that hub's own address does.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_lost_key_pc_links_through_its_own_saved_hub() {
    let Some(hub) = scratch_hub() else {
        eprintln!("SKIPPED: set HUBCHAT_V2_HUB to a scratch mail hub v2");
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let alex = Identity::generate("alex").unwrap();
    let pc = lost_key_pc(dir.path(), &alex, &[(hub.as_str(), true)]);
    let phone = phone_on(&alex, &hub).await;

    let rows = relink(&pc, &phone, &hub, &alex, vec![hub.clone()]).await;
    assert!(row(&rows, &hub).is_ok(), "the saved hub the link went through answers its check");
    assert!(pc.hub_enabled(&hub).unwrap());
    eprintln!("  ok  linked through the PC's own saved hub");
    phone.shutdown();
}
