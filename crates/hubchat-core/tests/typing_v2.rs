//! Typing (hub feature `typing`, 2026-10-13; Orgtree docket
//! show-agent-typing-in-hubchat-using-ordered-messa): heartbeats (user
//! 2026-10-13 22:34Z). A writer (Orgtree's engine for an agent, or another
//! Hubchat) sends one about every 500 ms; each one the hub relays tells the
//! window once (an event `{"type": "typing", "peer": ...}`), and the window
//! shows the row while the last one came within a second. When they stop,
//! nothing more comes. Events are read as the window gets them (JSON), so
//! the test runs, and fails, on a core without heartbeats too. Our own
//! user's heartbeats: composing_v2.rs.
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
    /// The typing events as the window got them.
    fn typing_shapes(&self) -> Vec<Value> {
        self.events.lock().unwrap().iter().filter(|(_, e)| e["type"] == "typing").map(|(_, e)| e.clone()).collect()
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

/// One heartbeat from `who` to `to`, as Orgtree's engine sends one for an agent.
async fn beat(hub: &str, who: &Identity, to: &str) {
    let auth = format!("{}:{}", who.address(), who.secret());
    let r = reqwest::Client::new().post(format!("{hub}/api/typing")).header("X-Org-Auth", auth).json(&json!({ "to": [to] })).send().await.unwrap();
    let code = r.status();
    assert!(code.is_success(), "heartbeat: {code} {}", r.text().await.unwrap_or_default());
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
async fn a_writers_heartbeats_reach_the_window_while_they_come() {
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
    // alex's window is in use: the hub relays heartbeats to it at once
    a.set_active(true);
    tokio::time::sleep(Duration::from_secs(2)).await;
    let (alex_at, bob_at) = (alex.address(), bob.address());

    // bob writes for 4 s (as Orgtree's engine does for an agent): each
    // heartbeat reaches the window, never a second apart, and when they
    // stop nothing more comes
    let t = Instant::now();
    for _ in 0..8 {
        beat(&hub, &bob, &alex_at).await;
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    let last_beat = Instant::now() - Duration::from_millis(500);
    tokio::time::sleep(Duration::from_secs(2)).await;
    let got = window.beats(&bob_at, t);
    assert!(got.len() >= 6, "the window heard {} of 8 heartbeats", got.len());
    let gap = longest_gap(t, &got);
    assert!(gap < Duration::from_secs(1), "a {gap:?} gap: the row would have dropped");
    let after = got.last().unwrap().saturating_duration_since(last_beat);
    assert!(after < Duration::from_millis(1000), "a heartbeat reached the window {after:?} after the last was sent");
    assert!(window.typing_shapes().iter().all(|e| e.as_object().is_some_and(|o| o.len() == 2)), "{:?}", window.typing_shapes());
    eprintln!("  ok  {} heartbeats shown, longest gap {gap:?}, the last {after:?} after it was sent, nothing later", got.len());

    // bob writes, then his message comes: no heartbeat after it, and only
    // the message notifies
    let notified = window.notified();
    for _ in 0..3 {
        beat(&hub, &bob, &alex_at).await;
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    let id = format!("m{}", &uuid::Uuid::new_v4().simple().to_string()[..8]);
    let sent = Instant::now();
    b.send(NewOutgoing { id: id.clone(), peer: alex_at.clone(), body: "hello".into(), kind: None, reply_to: None, attachments: vec![] }).await.unwrap();
    assert!(until(20, || a.store().message(&id).unwrap().is_some()).await, "alex got bob's message");
    let came = window.incoming(&bob_at, sent).expect("the message told the window");
    tokio::time::sleep(Duration::from_secs(2)).await;
    assert!(window.beats(&bob_at, came).is_empty(), "a heartbeat after bob's message came");
    assert_eq!(window.notified(), notified + 1, "the message notified as ever, the heartbeats never");
    eprintln!("  ok  the message ends it; heartbeats never notify");
}
