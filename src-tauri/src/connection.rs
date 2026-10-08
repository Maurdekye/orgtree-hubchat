//! SPIKE: the background hub connection. One identity, one hub, long-polling
//! forever; each incoming message is acked, receipted `delivered` and shown as
//! a notification. Phase 2 replaces this with the real sync engine and store.
//!
//! Spike config lives in the data dir: `hub.txt` (hub address, default
//! 127.0.0.1:7399 — reach the PC through `adb reverse tcp:7399 tcp:7399`),
//! `spike-identity.txt` (id and secret, created on first run). Every event is
//! appended to `spike.log` with a wall-clock timestamp so background
//! behaviour can be measured afterwards.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};
use std::time::{Duration, SystemTime};

use hubchat_core::hub::Profile;
use hubchat_core::{HubAddress, HubClient, Identity};

/// How the core reaches the platform (notifications, the ongoing status line).
pub trait Platform: Send + Sync + 'static {
    fn notify(&self, title: &str, body: &str);
    fn status(&self, text: &str);
}

static STARTED: OnceLock<()> = OnceLock::new();

/// What the spike commands need from the running core.
pub struct Core {
    pub client: HubClient,
    pub me: Identity,
    pub dir: PathBuf,
}

static CORE: OnceLock<Core> = OnceLock::new();

pub fn core() -> Option<&'static Core> {
    CORE.get()
}

/// Start the connection once per process; later calls do nothing.
pub fn start(data_dir: PathBuf, platform: Arc<dyn Platform>) {
    if STARTED.set(()).is_err() {
        return;
    }
    std::thread::Builder::new()
        .name("hubchat-core".into())
        .spawn(move || {
            let rt = tokio::runtime::Builder::new_multi_thread()
                .worker_threads(2)
                .enable_all()
                .build()
                .expect("runtime");
            rt.block_on(run(data_dir, platform));
        })
        .expect("core thread");
}

fn now() -> String {
    humantime::format_rfc3339_millis(SystemTime::now()).to_string()
}

pub fn log(dir: &Path, line: &str) {
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join("spike.log"))
    {
        let _ = writeln!(f, "{} {line}", now());
    }
}

fn load_identity(dir: &Path) -> Identity {
    let p = dir.join("spike-identity.txt");
    if let Ok(s) = std::fs::read_to_string(&p) {
        if let Some((id, secret)) = s.trim().split_once(' ') {
            if let Ok(me) = Identity::from_parts(id, secret) {
                return me;
            }
        }
    }
    let me = Identity::generate("spike").expect("identity");
    let _ = std::fs::write(&p, format!("{} {}", me.id(), me.secret()));
    me
}

async fn run(dir: PathBuf, platform: Arc<dyn Platform>) {
    let _ = std::fs::create_dir_all(&dir);
    let me = load_identity(&dir);
    let hub =
        std::fs::read_to_string(dir.join("hub.txt")).unwrap_or_else(|_| "127.0.0.1:7399".into());
    let addr = match HubAddress::parse(&hub) {
        Ok(a) => a,
        Err(e) => {
            log(&dir, &format!("bad hub address {hub:?}: {e}"));
            platform.status("Hub address is invalid");
            return;
        }
    };
    log(&dir, &format!("start address={} hub={addr}", me.address()));
    let client = HubClient::new(addr);
    let _ = CORE.set(Core {
        client: client.clone(),
        me: me.clone(),
        dir: dir.clone(),
    });
    let profile = Profile {
        kind: "chat".into(),
        org_name: "Hubchat".into(),
        username: "spike".into(),
        blurb: String::new(),
    };
    let mut backoff = 1u64;
    let mut registered = false;
    loop {
        if !registered {
            match client.register(&me, &profile).await {
                Ok(r) => {
                    registered = true;
                    backoff = 1;
                    log(&dir, &format!("registered hub={}", r.name));
                    platform.status(&format!("Connected to {} as {}", r.name, me.address()));
                }
                Err(e) => {
                    log(&dir, &format!("register failed: {e}"));
                    platform.status(&format!("Can't reach the hub, retrying in {backoff} s"));
                    tokio::time::sleep(Duration::from_secs(backoff)).await;
                    backoff = (backoff * 2).min(60);
                    continue;
                }
            }
        }
        let t0 = std::time::Instant::now();
        match client.poll(&me, 55).await {
            Ok(p) => {
                backoff = 1;
                log(
                    &dir,
                    &format!(
                        "poll ok after {} ms, {} msgs",
                        t0.elapsed().as_millis(),
                        p.messages.len()
                    ),
                );
                if p.messages.is_empty() {
                    continue;
                }
                let ids: Vec<String> = p.messages.iter().map(|m| m.id.clone()).collect();
                for m in &p.messages {
                    log(
                        &dir,
                        &format!(
                            "msg id={} from={} sent_at={:?} received_at={}",
                            m.id, m.from, m.sent_at, m.received_at
                        ),
                    );
                    platform.notify(&m.from, &m.body);
                }
                if let Err(e) = client.ack(&me, &ids).await {
                    log(&dir, &format!("ack failed: {e}"));
                }
                let at = now();
                let rec: Vec<_> = ids
                    .iter()
                    .map(|id| (id.clone(), "delivered", at.clone()))
                    .collect();
                if let Err(e) = client.receipts(&me, &rec).await {
                    log(&dir, &format!("receipts failed: {e}"));
                }
            }
            Err(e) => {
                log(
                    &dir,
                    &format!("poll failed after {} ms: {e}", t0.elapsed().as_millis()),
                );
                if e.status() == Some(401) {
                    registered = false; // hub forgot us: re-register
                }
                platform.status(&format!("Disconnected, retrying in {backoff} s"));
                tokio::time::sleep(Duration::from_secs(backoff)).await;
                backoff = (backoff * 2).min(60);
            }
        }
    }
}
