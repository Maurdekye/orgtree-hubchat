//! Typing (hub feature `typing`, 2026-10-13; Orgtree docket
//! show-agent-typing-in-hubchat-using-ordered-messa): our own user writing
//! sends heartbeats too (user 2026-10-13 22:33Z and 22:34Z). From the first
//! edit the composer reports, one goes every 500 ms through the hub the next
//! message would take, until the last edit is 3 s old, or the composer stops
//! (box cleared, chat left, app in the background), or the message is sent,
//! or the device goes out of use. Only the address goes: the hub refuses a
//! heartbeat with anything else in it. Bob writes; alex's window shows it.
//!
//! Set HUBCHAT_V2_HUB to a scratch mail hub v2 WITH the `typing` feature
//! (skipped when unset).

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use hubchat_core::engine::{Engine, Event, Host, HubState};
use hubchat_core::hub::Profile;
use hubchat_core::store::{NewOutgoing, Store};
use hubchat_core::{HubAddress, Identity};
use serde_json::{json, Value};

/// The events as the window gets them, with when they came.
#[derive(Default)]
struct Window {
    events: Mutex<Vec<(Instant, Value)>>,
}

impl Host for Window {
    fn event(&self, ev: Event) {
        self.events.lock().unwrap().push((Instant::now(), serde_json::to_value(&ev).unwrap()));
    }
    fn open_source(&self, source: &str) -> std::io::Result<std::fs::File> {
        std::fs::File::open(source)
    }
    fn download_dir(&self) -> std::path::PathBuf {
        std::env::temp_dir()
    }
}

impl Window {
    /// When each typing event from `peer` came, from `since` on.
    fn beats(&self, peer: &str, since: Instant) -> Vec<Instant> {
        self.events.lock().unwrap().iter().filter(|(t, e)| *t >= since && e["type"] == "typing" && e["peer"] == peer).map(|(t, _)| *t).collect()
    }
    fn incoming(&self, peer: &str, since: Instant) -> Option<Instant> {
        self.events.lock().unwrap().iter().find(|(t, e)| *t >= since && e["type"] == "incoming" && e["peer"] == peer).map(|(t, _)| *t)
    }
    fn notified(&self) -> usize {
        self.events.lock().unwrap().iter().filter(|(_, e)| e["type"] == "incoming").count()
    }
}

fn engine_with(me: &Identity, name: &str, host: Arc<dyn Host>) -> Arc<Engine> {
    let profile = Profile { kind: "person".into(), org_name: name.into(), username: name.into(), blurb: String::new() };
    let e = Engine::new(Arc::new(Store::open_in_memory().unwrap()), me.clone(), profile, host);
    e.set_device(name, name);
    e.start().unwrap();
    e
}

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

/// The longest gap between `times`, and between `from` and the first.
fn longest_gap(from: Instant, times: &[Instant]) -> Duration {
    let mut last = from;
    let mut gap = Duration::ZERO;
    for t in times {
        gap = gap.max(t.saturating_duration_since(last));
        last = *t;
    }
    gap
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn our_user_writing_sends_heartbeats_until_a_pause_a_stop_or_a_send() {
    let Ok(addr) = std::env::var("HUBCHAT_V2_HUB") else {
        eprintln!("SKIPPED: set HUBCHAT_V2_HUB to a scratch mail hub v2 with typing");
        return;
    };
    let hub = HubAddress::parse(&addr).unwrap().to_string();
    let health: Value = reqwest::get(format!("{hub}/healthz")).await.unwrap().json().await.unwrap();
    assert!(health["features"].as_array().is_some_and(|f| f.contains(&json!("typing"))), "the scratch hub must have typing: {health}");

    let alex = Identity::generate("alex").unwrap();
    let bob = Identity::generate("bob").unwrap();
    let window = Arc::new(Window::default());
    let a = engine_with(&alex, "alex-pc", window.clone());
    let b = engine_with(&bob, "bob-pc", Arc::new(Quiet));
    a.add_hub(&hub).unwrap();
    b.add_hub(&hub).unwrap();
    assert!(until(30, || connected(&a, &hub) && connected(&b, &hub)).await, "both connected");
    a.set_active(true);
    let (alex_at, bob_at) = (alex.address(), bob.address());
    // bob's messages (and heartbeats) to alex have a way
    assert!(until(30, || b.send_route(&alex_at).is_ok_and(|r| r.next.is_some())).await, "bob can reach alex");
    tokio::time::sleep(Duration::from_secs(2)).await;
    let recent = || !window.beats(&bob_at, Instant::now() - Duration::from_millis(300)).is_empty();

    // bob types for 2 s: a heartbeat from the first edit on, every 500 ms,
    // and 3 s after his last edit they stop
    let t = Instant::now();
    for _ in 0..10 {
        b.composing(&alex_at, true);
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    let last_edit = Instant::now() - Duration::from_millis(200);
    assert!(until(10, || window.beats(&bob_at, t).len() >= 3).await, "alex's window shows bob writing");
    let first = window.beats(&bob_at, t)[0].saturating_duration_since(t);
    assert!(first < Duration::from_secs(1), "the first edit's heartbeat came after {first:?}");
    tokio::time::sleep(Duration::from_secs(5)).await;
    let got = window.beats(&bob_at, t);
    let gap = longest_gap(t + first, &got);
    assert!(gap < Duration::from_secs(1), "a {gap:?} gap while bob wrote");
    let ended = got.last().unwrap().saturating_duration_since(last_edit);
    assert!(ended < Duration::from_millis(4000), "heartbeats went on {ended:?} after bob's last edit");
    assert!(ended >= Duration::from_millis(2000), "heartbeats stopped {ended:?} after his last edit, before the pause was 3 s");
    eprintln!("  ok  bob writing: {} heartbeats, the first after {first:?}, the last {ended:?} after his last edit", got.len());

    // a stop (the box cleared, the chat left, the app in the background)
    // ends them at once
    b.composing(&alex_at, true);
    assert!(until(5, recent).await, "bob writing again");
    b.composing(&alex_at, false);
    let stopped = Instant::now();
    tokio::time::sleep(Duration::from_secs(2)).await;
    let late = window.beats(&bob_at, stopped + Duration::from_millis(700));
    assert!(late.is_empty(), "{} heartbeats came after bob stopped", late.len());
    eprintln!("  ok  a stop ends bob's heartbeats");

    // sending ends them too, and only the message notifies
    b.composing(&alex_at, true);
    assert!(until(5, recent).await, "bob writing a message");
    let id = format!("m{}", &uuid::Uuid::new_v4().simple().to_string()[..8]);
    let notified = window.notified();
    let sent = Instant::now();
    b.send(NewOutgoing { id: id.clone(), peer: alex_at.clone(), body: "hello".into(), kind: None, reply_to: None, attachments: vec![] }).await.unwrap();
    assert!(until(20, || a.store().message(&id).unwrap().is_some()).await, "alex got bob's message");
    let came = window.incoming(&bob_at, sent).expect("the message told the window");
    tokio::time::sleep(Duration::from_secs(2)).await;
    assert!(window.beats(&bob_at, came).is_empty(), "a heartbeat after bob's message came");
    assert_eq!(window.notified(), notified + 1, "the message notified, the heartbeats never");
    eprintln!("  ok  sending ends bob's heartbeats");

    // and so does his device going out of use (the app in the background,
    // on Android even while its JavaScript sleeps)
    b.composing(&alex_at, true);
    assert!(until(5, recent).await, "bob writing again");
    b.set_active(false);
    let rested = Instant::now();
    tokio::time::sleep(Duration::from_secs(2)).await;
    assert!(window.beats(&bob_at, rested + Duration::from_millis(700)).is_empty(), "heartbeats after bob's device went out of use");
    eprintln!("  ok  the device out of use ends them");
}
