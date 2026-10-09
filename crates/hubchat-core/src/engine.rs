//! The sync engine: one connection task per hub (register, long poll, ack,
//! receipts, roster) and one sender task that drains the outgoing queue.
//! Everything lands in the `Store`; the shell is told what changed through
//! `Host::event` and re-reads what it shows.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

use serde::Serialize;
use tokio::sync::Notify;

use crate::hub::{CancelFlag, Outgoing, Profile};
use crate::store::{Message, NewOutgoing, Store};
use crate::{Error, HubAddress, HubClient, Identity, Result};

/// First line of a reply's wire body (see `Engine::wire_body`).
pub const QUOTE_PREFIX: &str = "> ";

/// Remove the one-line quote a Hubchat reply carries, when the message also
/// names what it answers (so the quote is shown from the referenced message).
pub fn strip_quote(body: &str) -> &str {
    match body
        .strip_prefix(QUOTE_PREFIX)
        .and_then(|r| r.split_once('\n'))
    {
        Some((_, rest)) => rest,
        None => body,
    }
}

/// Bodies longer than this go to v2 hubs as an uploaded part (the hub takes
/// up to 32 MiB of JSON per send).
pub const BODY_INLINE_MAX: u64 = 16 * 1024 * 1024;

/// Files at least this big go through v2's resumable uploads.
pub const RESUMABLE_MIN: u64 = 8 * 1024 * 1024;

/// Long bodies (v2, G6) are fetched whole up to this size.
pub const LONG_BODY_FETCH_MAX: u64 = 64 * 1024 * 1024;

/// Back-off between reconnects (design F7: 8, 16, 32 s, then 32 s).
const BACKOFF: [u64; 3] = [8, 16, 32];

/// How long scrolling back waits for a hub that is still connecting.
const CONNECTING_GRACE: Duration = Duration::from_secs(4);

/// Messages per history page when scrolling back.
const HISTORY_PAGE: u32 = 50;

/// What one `load_older` call did.
#[derive(Debug, Clone, Default, Serialize)]
pub struct OlderPage {
    /// Messages new to this device.
    pub added: usize,
    /// Hubs with still older messages of this chat.
    pub more: Vec<String>,
    /// Hubs this device started from now on that couldn't be asked
    /// (offline): their older messages are not loaded yet.
    pub unreachable: Vec<String>,
}

fn start_key(url: &str) -> String {
    format!("sync.start_ms.{url}")
}

fn chats_key(url: &str) -> String {
    format!("sync.chat_list_due.{url}")
}

fn clock_key(url: &str) -> String {
    format!("hub.clock_offset_ms.{url}")
}

/// A hub timestamp (RFC 3339, `Z`) in unix ms.
fn hub_ms(t: &str) -> Option<i64> {
    let st = humantime::parse_rfc3339_weak(t).ok()?;
    st.duration_since(SystemTime::UNIX_EPOCH)
        .ok()
        .map(|d| d.as_millis() as i64)
}

pub fn now() -> String {
    humantime::format_rfc3339_millis(SystemTime::now()).to_string()
}

/// What the engine reports to the shell. The shell re-reads the store.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Event {
    /// A chat's messages or unread count changed.
    Chat { peer: String },
    /// A new incoming message (the shell decides whether to notify).
    Incoming {
        peer: String,
        id: String,
        preview: String,
        /// Another of our devices was in use when it came (user 23:50Z):
        /// no notification here.
        quiet: bool,
    },
    /// A hub's connection state changed.
    Hub { url: String },
    /// The merged directory changed.
    Directory,
    /// Upload or download progress for one attachment.
    Transfer {
        local_id: String,
        message_id: String,
        upload: bool,
        done: u64,
        total: u64,
    },
}

/// The platform side: events, opening picked files, where downloads go.
pub trait Host: Send + Sync + 'static {
    fn event(&self, ev: Event);
    /// Open an attachment source: a path, or on Android a content:// URI.
    fn open_source(&self, source: &str) -> std::io::Result<std::fs::File>;
    /// Folder for downloaded attachments.
    fn download_dir(&self) -> PathBuf;
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum HubState {
    Connecting,
    Connected,
    /// Reachable but refused us, or not a hub. Needs the user.
    Refused,
    Disconnected,
}

#[derive(Debug, Clone, Serialize)]
pub struct HubStatus {
    pub url: String,
    pub name: String,
    pub state: HubState,
    pub error: Option<String>,
    /// Unix ms of the next reconnect attempt while disconnected.
    pub retry_at_ms: Option<u64>,
    pub max_attachment_bytes: u64,
    /// What the hub advertises in /healthz `features` (empty on older hubs).
    pub features: Vec<String>,
    /// The hub's software version as it reports it; None = unknown (v1 hubs).
    pub version: Option<String>,
    /// Unix ms when the last poll or sync answer was fully taken in.
    pub answered_ms: Option<u64>,
    /// Unix ms since which a poll or sync has been waiting for its answer
    /// (the hub parks one while it has no news).
    pub waiting_since_ms: Option<u64>,
    /// The hub's clock minus ours, in ms (hubs that report `now`): its
    /// received times are its own clock.
    pub clock_offset_ms: Option<i64>,
}

struct HubRuntime {
    client: HubClient,
    status: HubStatus,
    retry_now: Arc<Notify>,
    /// Start the session over at once (a check: Android's periodic mode).
    kick: Arc<Notify>,
    stop: CancelFlag,
}

/// A request the hub hasn't answered for this long is parked: the hub
/// answers at once when it has news, so there is none.
const PARKED_AFTER_MS: u64 = 5000;

pub struct Engine {
    store: Arc<Store>,
    me: Identity,
    profile: Mutex<Profile>,
    host: Arc<dyn Host>,
    hubs: Mutex<HashMap<String, HubRuntime>>,
    queue_changed: Notify,
    /// Set by shutdown(): the sender stops; hub tasks were stopped one by one.
    stopped: std::sync::atomic::AtomicBool,
    transfers: Mutex<HashMap<String, CancelFlag>>,
    read_receipts: std::sync::atomic::AtomicBool,
    /// This installation's (device_id, device_name) for v2 sync.
    device: Mutex<(String, String)>,
    /// This device is in use (focused, recently touched): our other devices
    /// keep quiet while it is (user 23:50Z).
    active: std::sync::atomic::AtomicBool,
}

fn unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

impl Engine {
    pub fn new(
        store: Arc<Store>,
        me: Identity,
        profile: Profile,
        host: Arc<dyn Host>,
    ) -> Arc<Self> {
        Arc::new(Self {
            store,
            me,
            profile: Mutex::new(profile),
            host,
            hubs: Mutex::new(HashMap::new()),
            queue_changed: Notify::new(),
            stopped: std::sync::atomic::AtomicBool::new(false),
            transfers: Mutex::new(HashMap::new()),
            read_receipts: std::sync::atomic::AtomicBool::new(true),
            active: std::sync::atomic::AtomicBool::new(false),
            device: Mutex::new((
                format!("hc-{}", &uuid::Uuid::new_v4().simple().to_string()[..12]),
                "Hubchat".into(),
            )),
        })
    }

    pub fn me(&self) -> &Identity {
        &self.me
    }

    pub fn store(&self) -> &Store {
        &self.store
    }

    /// Start a task per stored hub plus the sender. Call once, inside a runtime.
    pub fn start(self: &Arc<Self>) -> Result<()> {
        for h in self.store.hubs()? {
            if let Ok(a) = HubAddress::parse(&h.url) {
                self.spawn_hub(a, h.name, h.max_attachment_bytes);
            }
        }
        let me = self.clone();
        tokio::spawn(async move { me.sender_loop().await });
        // in use: say so again before the hub's 90 s run out
        let me = self.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(60)).await;
                if me.stopped.load(std::sync::atomic::Ordering::Relaxed) {
                    return;
                }
                if me.active.load(std::sync::atomic::Ordering::Relaxed) {
                    me.report_active(true).await;
                }
            }
        });
        Ok(())
    }

    /// The UI says whether this device is in use (focused, touched in the
    /// last few minutes). A change is told to every hub that can carry it.
    pub fn set_active(self: &Arc<Self>, on: bool) {
        if self.active.swap(on, std::sync::atomic::Ordering::Relaxed) != on {
            let me = self.clone();
            tokio::spawn(async move { me.report_active(on).await });
        }
    }

    /// Tell each hub with the "active" feature (best effort: a hub that
    /// misses it lets the last word expire after 90 s).
    async fn report_active(&self, on: bool) {
        let (device_id, _) = self.device();
        let clients: Vec<HubClient> = self
            .hubs
            .lock()
            .unwrap()
            .values()
            .filter(|h| h.status.features.iter().any(|f| f == "active"))
            .map(|h| h.client.clone())
            .collect();
        for c in clients {
            let _ = c.set_active(&self.me, &device_id, on).await;
        }
    }

    /// Name this installation for v2 hubs (stable id, shown name). Call
    /// before start().
    pub fn set_device(&self, id: &str, name: &str) {
        *self.device.lock().unwrap() = (id.to_owned(), name.to_owned());
    }

    pub fn device(&self) -> (String, String) {
        self.device.lock().unwrap().clone()
    }

    pub fn set_read_receipts(&self, on: bool) {
        self.read_receipts
            .store(on, std::sync::atomic::Ordering::Relaxed);
    }

    pub fn profile(&self) -> Profile {
        self.profile.lock().unwrap().clone()
    }

    /// Change the display name and about line everywhere: through
    /// POST /api/profile on hubs that support it, by registering again on
    /// older hubs (which refresh the fields on re-registration).
    pub async fn set_profile(&self, name: &str, about: &str) -> Result<()> {
        let profile = {
            let mut p = self.profile.lock().unwrap();
            p.org_name = name.to_owned();
            p.blurb = about.to_owned();
            p.clone()
        };
        let hubs: Vec<(HubClient, Vec<String>)> = self
            .hubs
            .lock()
            .unwrap()
            .values()
            .filter(|h| h.status.state == HubState::Connected)
            .map(|h| (h.client.clone(), h.status.features.clone()))
            .collect();
        for (client, features) in hubs {
            let has = |f: &str| features.iter().any(|x| x == f);
            let r = if has("profile") {
                client.set_profile(&self.me, name, about).await
            } else {
                let mut p = profile.clone();
                if p.kind == "person" && !has("person") {
                    p.kind = "chat".into();
                }
                client.register(&self.me, &p).await.map(|_| ())
            };
            if let Err(e) = r {
                // Not fatal: the next (re)registration carries the new values.
                let _ = e;
            }
        }
        Ok(())
    }

    // ------------------------------------------------------------ hubs

    /// Add a hub (it is kept even while unreachable) and connect to it.
    pub fn add_hub(self: &Arc<Self>, input: &str) -> Result<HubAddress> {
        let addr = HubAddress::parse(input)?;
        self.store.add_hub(addr.as_str(), &now())?;
        if !self.hubs.lock().unwrap().contains_key(addr.as_str()) {
            self.spawn_hub(addr.clone(), String::new(), None);
        }
        Ok(addr)
    }

    /// Remove a hub; with `unregister` also take our address off it.
    pub async fn remove_hub(&self, url: &str, unregister: bool) -> Result<()> {
        let rt = self.hubs.lock().unwrap().remove(url);
        if let Some(rt) = rt {
            rt.stop.cancel();
            rt.retry_now.notify_one();
            // deletes it still owes go now or never (best effort)
            if rt.status.state == HubState::Connected {
                let _ = self
                    .send_queued_deletes(&rt.client, url, &rt.status.features, u64::MAX)
                    .await;
            }
            if unregister {
                let _ = rt.client.unregister(&self.me).await;
            }
        }
        self.store.remove_hub(url)?;
        self.host.event(Event::Hub { url: url.into() });
        self.host.event(Event::Directory);
        // a chat pinned to it is on Automatic now: its waiting messages can go
        self.queue_changed.notify_one();
        Ok(())
    }

    /// One check, for Android's periodic mode (design D6): start every hub's
    /// session over, then wait (up to `timeout`) until each has taken in an
    /// answer or has a request parked (no news), and nothing waits to be
    /// sent. True when everything got through in time.
    pub async fn check_now(&self, timeout: Duration) -> bool {
        let start = unix_ms();
        self.kick();
        let deadline = tokio::time::Instant::now() + timeout;
        loop {
            let now_ms = unix_ms();
            let hubs_done = self.hubs.lock().unwrap().values().all(|h| {
                let s = &h.status;
                s.state == HubState::Refused
                    || s.answered_ms.is_some_and(|t| t >= start)
                    || s
                        .waiting_since_ms
                        .is_some_and(|t| t >= start && now_ms.saturating_sub(t) >= PARKED_AFTER_MS)
            });
            let queue_empty = self.store.queued().map(|q| q.is_empty()).unwrap_or(true);
            if hubs_done && queue_empty {
                return true;
            }
            if tokio::time::Instant::now() >= deadline {
                return false;
            }
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
    }

    /// Start every hub's session over now (and look at the outgoing queue):
    /// after a freeze a parked request may sit on a connection long gone.
    pub fn kick(&self) {
        for rt in self.hubs.lock().unwrap().values() {
            rt.kick.notify_one();
        }
        self.queue_changed.notify_one();
    }

    pub fn retry_now(&self) {
        for rt in self.hubs.lock().unwrap().values() {
            rt.retry_now.notify_one();
        }
        self.queue_changed.notify_one();
    }

    /// Send a message that is not part of any chat (device linking): through
    /// a connected hub whose roster lists `to`, without storing it.
    pub async fn send_unlisted(&self, to: &str, body: &str) -> Result<String> {
        let reaching = self.store.hubs_reaching(to)?;
        let connected = self.connected_hubs();
        let (url, client, _) = pick_hub(&connected, &reaching, None)
            .filter(|_| !reaching.is_empty())
            .ok_or_else(|| {
                Error::Invalid("none of your connected hubs can reach that device".into())
            })?;
        let sent = client.send(&self.me, &Outgoing::new(to, body)).await?;
        // Hubs that keep history (v2) would keep our copy and sync it to our
        // other devices: delete it now (the recipient keeps its own copy
        // until it takes it).
        let keeps_history = self
            .hubs
            .lock()
            .unwrap()
            .get(&url)
            .is_some_and(|h| h.status.features.iter().any(|f| f == "delete"));
        if keeps_history {
            let _ = client.delete_message(&self.me, &sent.id).await;
        }
        Ok(url)
    }

    /// Stop every hub connection and the sender (switching identity). The
    /// engine is not usable afterwards.
    pub fn shutdown(&self) {
        self.stopped
            .store(true, std::sync::atomic::Ordering::Relaxed);
        for (_, rt) in self.hubs.lock().unwrap().drain() {
            rt.stop.cancel();
            rt.retry_now.notify_one();
        }
        for c in self.transfers.lock().unwrap().values() {
            c.cancel();
        }
        self.queue_changed.notify_one();
    }

    /// Take this device off our device list on every v2 hub (switching to
    /// another identity). Older hubs have no device list: nothing to do.
    /// Never unregisters the address: our other devices still use it.
    pub async fn sign_out_this_device(&self) {
        let (device_id, _) = self.device();
        let clients: Vec<HubClient> = self
            .hubs
            .lock()
            .unwrap()
            .values()
            .filter(|h| h.status.features.iter().any(|f| f == "devices"))
            .map(|h| h.client.clone())
            .collect();
        for c in clients {
            let _ = c.sign_out_device(&self.me, &device_id).await;
        }
    }

    pub fn hub_statuses(&self) -> Vec<HubStatus> {
        let hubs = self.hubs.lock().unwrap();
        let mut v: Vec<_> = hubs.values().map(|h| h.status.clone()).collect();
        v.sort_by(|a, b| a.url.cmp(&b.url));
        v
    }

    fn spawn_hub(self: &Arc<Self>, addr: HubAddress, name: String, max: Option<u64>) {
        let rt = HubRuntime {
            client: HubClient::new(addr.clone()),
            status: HubStatus {
                url: addr.to_string(),
                name,
                state: HubState::Connecting,
                error: None,
                retry_at_ms: None,
                max_attachment_bytes: max.unwrap_or(crate::hub::LEGACY_MAX_ATTACHMENT_BYTES),
                features: Vec::new(),
                version: None,
                answered_ms: None,
                waiting_since_ms: None,
                clock_offset_ms: self
                    .store
                    .meta(&clock_key(&addr.to_string()))
                    .ok()
                    .flatten()
                    .and_then(|v| v.parse().ok()),
            },
            retry_now: Arc::new(Notify::new()),
            kick: Arc::new(Notify::new()),
            stop: CancelFlag::default(),
        };
        let (client, retry, kick, stop) = (
            rt.client.clone(),
            rt.retry_now.clone(),
            rt.kick.clone(),
            rt.stop.clone(),
        );
        self.hubs.lock().unwrap().insert(addr.to_string(), rt);
        let me = self.clone();
        tokio::spawn(async move { me.hub_loop(client, retry, kick, stop).await });
    }

    fn set_status(&self, url: &str, f: impl FnOnce(&mut HubStatus)) {
        let changed = {
            let mut hubs = self.hubs.lock().unwrap();
            match hubs.get_mut(url) {
                Some(h) => {
                    let before = format!("{:?}", h.status);
                    f(&mut h.status);
                    before != format!("{:?}", h.status)
                }
                None => false,
            }
        };
        if changed {
            self.host.event(Event::Hub { url: url.into() });
        }
    }

    fn connected_hubs(&self) -> Vec<(String, HubClient, u64)> {
        self.hubs
            .lock()
            .unwrap()
            .values()
            .filter(|h| h.status.state == HubState::Connected)
            .map(|h| {
                (
                    h.status.url.clone(),
                    h.client.clone(),
                    h.status.max_attachment_bytes,
                )
            })
            .collect()
    }

    async fn hub_loop(
        self: Arc<Self>,
        client: HubClient,
        retry: Arc<Notify>,
        kick: Arc<Notify>,
        stop: CancelFlag,
    ) {
        let url = client.address().to_string();
        let mut failures = 0usize;
        while !stop.is_cancelled() {
            let outcome = tokio::select! {
                r = self.session(&client, &url, &stop) => Some(r),
                // a check: start over now (after a freeze the parked request
                // may be on a connection that is long gone)
                _ = kick.notified() => None,
            };
            let Some(outcome) = outcome else {
                failures = 0;
                self.mark(&url, |s| s.waiting_since_ms = None);
                continue;
            };
            match outcome {
                Ok(()) => return, // stopped
                Err(e) => {
                    if stop.is_cancelled() {
                        return;
                    }
                    let refused = matches!(e, Error::NotAHub(_)) || matches!(e.status(), Some(403));
                    let wait = BACKOFF[failures.min(BACKOFF.len() - 1)];
                    failures += 1;
                    self.set_status(&url, |s| {
                        s.state = if refused {
                            HubState::Refused
                        } else {
                            HubState::Disconnected
                        };
                        s.error = Some(e.to_string());
                        s.retry_at_ms = Some(unix_ms() + wait * 1000);
                    });
                    self.host.event(Event::Directory); // presence there is now unknown
                    tokio::select! {
                        _ = tokio::time::sleep(Duration::from_secs(wait)) => {}
                        _ = retry.notified() => { failures = 0; }
                        _ = kick.notified() => { failures = 0; }
                    }
                    self.set_status(&url, |s| {
                        s.state = HubState::Connecting;
                        s.retry_at_ms = None;
                    });
                }
            }
        }
    }

    /// One connected session: probe, register, then poll until an error.
    async fn session(&self, client: &HubClient, url: &str, stop: &CancelFlag) -> Result<()> {
        let asked = unix_ms();
        let health = client.healthz().await?;
        self.clock_reading(url, health.now, asked, unix_ms())?;
        let mut profile = self.profile.lock().unwrap().clone();
        // Older hubs store any kind but org/chat as "org": a person registers
        // as "chat" there (coordinator ruling 2026-10-08).
        if profile.kind == "person" && !health.supports("person") {
            profile.kind = "chat".into();
        }
        client.register(&self.me, &profile).await?;
        let max = health.max_attachment_bytes();
        self.store
            .hub_ok(url, &health.name, health.max_attachment_bytes, &now())?;
        self.set_status(url, |s| {
            s.name = health.name.clone();
            s.state = HubState::Connected;
            s.error = None;
            s.retry_at_ms = None;
            s.max_attachment_bytes = max;
            s.features = health.features.clone();
            s.version = health.version.clone();
        });
        self.queue_changed.notify_one();
        if health.supports("sync") {
            return self
                .sync_session(client, url, stop, &profile, &health.features)
                .await;
        }
        let mut registered_again = false;
        while !stop.is_cancelled() {
            self.send_queued_deletes(client, url, &health.features, unix_ms())
                .await?;
            self.mark(url, |s| s.waiting_since_ms = Some(unix_ms()));
            let p = match client.poll(&self.me, crate::hub::POLL_WAIT_SECS).await {
                Ok(p) => p,
                Err(e) if e.status() == Some(401) && !registered_again => {
                    // The hub forgot us (pruned roster): register again.
                    client.register(&self.me, &profile).await?;
                    registered_again = true;
                    continue;
                }
                Err(e) => return Err(e),
            };
            registered_again = false;
            if stop.is_cancelled() {
                break;
            }
            if p.version.is_some() {
                let v = p.version.clone();
                self.set_status(url, |s| s.version = v);
            }
            self.store.set_roster(url, &p.roster)?;
            self.host.event(Event::Directory);
            for r in &p.receipts {
                self.store.apply_receipt(r)?;
                if let Some(m) = self.store.message(&r.id)? {
                    self.host.event(Event::Chat { peer: m.peer });
                }
            }
            if p.messages.is_empty() {
                self.answered(url);
                continue;
            }
            let t = now();
            let mut ids = Vec::new();
            for m in &p.messages {
                // A sealed identity for a device being linked is no chat.
                if m.body.starts_with(crate::link::LINK_MESSAGE_PREFIX) {
                    ids.push(m.id.clone());
                    continue;
                }
                // Persist first, then ack: the hub keeps it until we have it.
                if self.store.insert_incoming(url, m, &t)? {
                    // markdown syntax is for the chat; a notification gets plain words
                    let preview = crate::text::plain_preview(&m.body, 140);
                    self.host.event(Event::Incoming {
                        peer: m.from.clone(),
                        id: m.id.clone(),
                        preview,
                        quiet: false,
                    });
                }
                self.host.event(Event::Chat {
                    peer: m.from.clone(),
                });
                ids.push(m.id.clone());
            }
            client.ack(&self.me, &ids).await?;
            let rec: Vec<_> = ids
                .iter()
                .map(|id| (id.clone(), "delivered", t.clone()))
                .collect();
            client.receipts(&self.me, &rec).await?;
            self.answered(url);
        }
        Ok(())
    }

    /// A poll or sync answer is fully taken in.
    fn answered(&self, url: &str) {
        self.mark(url, |s| {
            s.answered_ms = Some(unix_ms());
            s.waiting_since_ms = None;
        });
    }

    /// Change a hub's progress markers: no event (they move on every poll).
    fn mark(&self, url: &str, f: impl FnOnce(&mut HubStatus)) {
        if let Some(h) = self.hubs.lock().unwrap().get_mut(url) {
            f(&mut h.status);
        }
    }

    /// v2 hubs: every device gets everything through sync (G1), history
    /// included (a first sync starts from the beginning).
    async fn sync_session(
        &self,
        client: &HubClient,
        url: &str,
        stop: &CancelFlag,
        profile: &Profile,
        features: &[String],
    ) -> Result<()> {
        let key = format!("sync.cursor.{url}");
        let start_key = start_key(url);
        // lazy history: a device new to this hub starts from now and pages
        // older mail in as the user scrolls back
        let lazy = features.iter().any(|f| f == "lazy_history");
        let me = self.me.address();
        let (device_id, device_name) = self.device();
        let mut cursor = self.store.meta(&key)?;
        // History (from the beginning, until a page without `more`) is not
        // news: it raises no notifications.
        let mut catching_up = cursor.is_none();
        let mut registered_again = false;
        while !stop.is_cancelled() {
            self.send_queued_deletes(client, url, features, unix_ms())
                .await?;
            let first_page = cursor.is_none();
            self.mark(url, |s| s.waiting_since_ms = Some(unix_ms()));
            let asked = unix_ms();
            let r = match client
                .sync(
                    &self.me,
                    &device_id,
                    &device_name,
                    cursor.as_deref(),
                    lazy,
                    // While catching up, don't park: a live message arriving
                    // during the history fetch must still count as news.
                    if catching_up {
                        0
                    } else {
                        crate::hub::POLL_WAIT_SECS
                    },
                )
                .await
            {
                Ok(r) => r,
                // registering again helps only once: a device signed out on
                // this hub stays refused (no spinning against the hub)
                Err(e) if e.status() == Some(401) && !registered_again => {
                    client.register(&self.me, profile).await?;
                    registered_again = true;
                    continue;
                }
                Err(e) if e.status() == Some(422) && cursor.is_some() => {
                    // "cursor is not a sync cursor from this hub": start over.
                    self.store.forget_hub_messages(url)?;
                    cursor = None;
                    catching_up = true;
                    continue;
                }
                Err(e) => return Err(e),
            };
            registered_again = false;
            if stop.is_cancelled() {
                break;
            }
            if r.version.is_some() {
                let v = r.version.clone();
                self.set_status(url, |s| s.version = v);
            }
            // a parked answer says nothing about the round trip
            if unix_ms() - asked < 2000 {
                self.clock_reading(url, r.now, asked, unix_ms())?;
            }
            if r.reset {
                self.store.forget_hub_messages(url)?;
            }
            // where this device's copy of the hub's mail begins: older mail
            // comes page by page (load_older)
            if first_page && r.start.as_deref() == Some("now") {
                self.start_from(url, r.now)?;
            } else if first_page {
                self.store.delete_meta(&start_key)?;
                self.store.delete_meta(&chats_key(url))?;
            } else if r.reset && self.store.meta(&start_key)?.is_some() {
                // a device that started from now starts over from now
                self.start_from(url, r.now)?;
            }
            let start_ms: Option<i64> = self.store.meta(&start_key)?.and_then(|v| v.parse().ok());
            let mut dir_changed = false;
            if first_page && !r.roster.is_empty() {
                self.store.set_roster(url, &r.roster)?;
                dir_changed = true;
            } else if !r.roster.is_empty() {
                self.store.upsert_roster(url, &r.roster)?;
                dir_changed = true;
            }
            if !r.roster_removed.is_empty() {
                self.store.remove_roster(url, &r.roster_removed)?;
                dir_changed = true;
            }
            if let Some(online) = &r.online {
                self.store.set_online(url, online)?;
                dir_changed = true;
            }
            if dir_changed {
                self.host.event(Event::Directory);
            }
            let t = now();
            let mut to_deliver = Vec::new();
            let mut peers = std::collections::BTreeSet::new();
            for ch in &r.changes {
                match ch {
                    crate::hub_v2::Change::Message(m) => {
                        // A sealed identity on its way to a device being
                        // linked (ours, synced back): no chat.
                        if m.env.body.starts_with(crate::link::LINK_MESSAGE_PREFIX) {
                            continue;
                        }
                        // a change to mail from before this device's start
                        // (a receipt, say) for a message not paged in yet:
                        // it comes with its page, not on its own
                        if let Some(start) = start_ms {
                            if hub_ms(&m.env.received_at).is_some_and(|t| t <= start)
                                && self.store.message(&m.env.id)?.is_none()
                            {
                                continue;
                            }
                        }
                        let fresh = self.store.upsert_synced_at(url, &me, m, self.offset(url))?;
                        if m.body_bytes.is_some() {
                            if let Ok(body) = client
                                .message_body(&self.me, &m.env.id, LONG_BODY_FETCH_MAX)
                                .await
                            {
                                let body = if m.env.reply_to.is_some() {
                                    strip_quote(&body).to_owned()
                                } else {
                                    body
                                };
                                self.store.set_body(&m.env.id, &body)?;
                            }
                        }
                        let incoming = m.env.from != me;
                        if incoming && m.delivered_at.is_none() {
                            to_deliver.push((m.env.id.clone(), "delivered", t.clone()));
                        }
                        if fresh && !catching_up {
                            // markdown syntax is for the chat; a notification gets plain words
                            let preview = crate::text::plain_preview(&m.env.body, 140);
                            self.host.event(Event::Incoming {
                                peer: m.env.from.clone(),
                                id: m.env.id.clone(),
                                preview,
                                quiet: !r.active.is_empty(),
                            });
                        }
                        peers.insert(if incoming {
                            m.env.from.clone()
                        } else {
                            m.env.to.clone()
                        });
                    }
                    crate::hub_v2::Change::Deleted(id) => {
                        if let Some(m) = self.store.message(id)? {
                            self.store.delete_message(id)?;
                            peers.insert(m.peer);
                        }
                    }
                    crate::hub_v2::Change::Other => {}
                }
            }
            for p in peers {
                self.host.event(Event::Chat { peer: p });
            }
            if !to_deliver.is_empty() {
                client.receipts(&self.me, &to_deliver).await?;
            }
            // Persist the cursor only after everything before it is stored.
            self.store.set_meta(&key, &r.cursor)?;
            cursor = Some(r.cursor);
            if !r.more {
                catching_up = false;
                if self.store.meta(&chats_key(url))?.is_some() {
                    self.take_chat_list(client, url).await?;
                }
                self.answered(url);
            }
        }
        Ok(())
    }

    /// A hub's `now` against our clock around the request: the offset is
    /// the hub's clock minus ours at the round trip's midpoint.
    fn clock_reading(&self, url: &str, hub_now: Option<i64>, asked: u64, answered: u64) -> Result<()> {
        let Some(hub_now) = hub_now else {
            return Ok(());
        };
        let offset = hub_now - ((asked + answered) / 2) as i64;
        self.store.set_meta(&clock_key(url), &offset.to_string())?;
        self.mark(url, |s| s.clock_offset_ms = Some(offset));
        Ok(())
    }

    /// This device's copy of `url`'s mail begins at `hub_now` (hub clock):
    /// whatever was paged back from it before starts again from there.
    fn start_from(&self, url: &str, hub_now: Option<i64>) -> Result<()> {
        // an answer without `now` (none should come): our clock, corrected
        let t = hub_now.unwrap_or_else(|| {
            let offset = self
                .hubs
                .lock()
                .unwrap()
                .get(url)
                .and_then(|h| h.status.clock_offset_ms);
            unix_ms() as i64 + offset.unwrap_or(0)
        });
        self.store.clear_history_marks(url)?;
        // its chat list is still to be taken in (take_chat_list)
        self.store.set_meta(&chats_key(url), "1")?;
        self.store.set_meta(&start_key(url), &t.to_string())
    }

    /// A device that started from now has none of its chats yet: the hub's
    /// chat list brings each chat's newest message and unread count (the
    /// rest pages in as the user scrolls back). Once per start: what comes
    /// after it comes through sync.
    async fn take_chat_list(&self, client: &HubClient, url: &str) -> Result<()> {
        let list = client.conversations(&self.me).await?;
        let me = self.me.address();
        let offset = self.offset(url);
        for c in &list {
            let mut listed = None;
            if let Some(m) = &c.last {
                if !m.env.body.starts_with(crate::link::LINK_MESSAGE_PREFIX) {
                    self.store.upsert_synced_at(url, &me, m, offset)?;
                    listed = Some(m.env.id.as_str());
                }
            }
            // the newest one counts here already when it is unread
            let listed_unread = c
                .last
                .as_ref()
                .is_some_and(|m| listed.is_some() && m.env.from != me && m.read_at.is_none());
            self.store.set_old_unread(
                url,
                &c.with,
                c.unread - i64::from(listed_unread),
                listed,
            )?;
            self.host.event(Event::Chat { peer: c.with.clone() });
        }
        self.store.delete_meta(&chats_key(url))
    }

    /// The hub's clock minus ours (0 when unknown).
    fn offset(&self, url: &str) -> i64 {
        self.hubs
            .lock()
            .unwrap()
            .get(url)
            .and_then(|h| h.status.clock_offset_ms)
            .unwrap_or(0)
    }

    /// A page of a chat as the user sees it (`Store::chat`), without what
    /// lies below the history floor: no message shows up later between
    /// messages already on screen.
    pub fn chat(
        &self,
        peer: &str,
        before: Option<(&str, &str)>,
        from: Option<(&str, &str)>,
        limit: u32,
    ) -> Result<Vec<Message>> {
        let mut v = self.store.chat(peer, before, from, limit)?;
        if let Some(floor) = self.history_floor(peer)? {
            v.retain(|m| m.created_at >= floor);
        }
        Ok(v)
    }

    /// How far back the chat with `peer` may show (our clock, the store's
    /// time form): as far as every hub that still has older messages has
    /// loaded. None: no limit. A hub that is down doesn't hold the rest
    /// back (its messages fill in when it is back), and the floor never
    /// rises again: what was on screen stays.
    pub fn history_floor(&self, peer: &str) -> Result<Option<String>> {
        let key = format!("history.floor.{peer}");
        // "all": the chat once showed everything, so it always will (a hub
        // that was down doesn't bring a floor back when it reconnects)
        let shown = self.store.meta(&key)?;
        if shown.as_deref() == Some("all") {
            return Ok(None);
        }
        let shown: Option<i64> = shown.and_then(|v| v.parse().ok());
        let floor = match (self.floor_now(peer)?, shown) {
            (Some(n), Some(s)) => Some(n.min(s)),
            (n, _) => n,
        };
        match floor {
            Some(f) if Some(f) != shown => self.store.set_meta(&key, &f.to_string())?,
            Some(_) => {}
            // unset or a time before: either way it is everything from now on
            None => self.store.set_meta(&key, "all")?,
        }
        Ok(floor.and_then(|ms| {
            let st = SystemTime::UNIX_EPOCH.checked_add(Duration::from_millis(u64::try_from(ms).ok()?))?;
            Some(humantime::format_rfc3339_millis(st).to_string())
        }))
    }

    /// The floor as the hubs stand now, unix ms on our clock.
    fn floor_now(&self, peer: &str) -> Result<Option<i64>> {
        let hubs: Vec<(String, bool, i64)> = self
            .hubs
            .lock()
            .unwrap()
            .iter()
            .map(|(u, h)| {
                (
                    u.clone(),
                    matches!(h.status.state, HubState::Disconnected | HubState::Refused),
                    h.status.clock_offset_ms.unwrap_or(0),
                )
            })
            .collect();
        let mut floor: Option<i64> = None;
        for (url, down, offset) in hubs {
            if down {
                continue;
            }
            let Some(start) = self
                .store
                .meta(&start_key(&url))?
                .and_then(|v| v.parse::<i64>().ok())
            else {
                continue;
            };
            let mark = self.store.history_mark(&url, peer)?.unwrap_or_default();
            if mark.done {
                continue;
            }
            let t = mark.oldest_ms.unwrap_or(start) - offset;
            floor = Some(floor.map_or(t, |f| f.max(t)));
        }
        Ok(floor)
    }

    // ---------------------------------------------------------- history

    /// Scrolling back: one older page of the chat with `peer` from every
    /// hub this device started from now on, stored by id (one row per id,
    /// whichever hubs hold it). A hub that isn't connected is named in
    /// `unreachable` and pages in when it is back.
    pub async fn load_older(&self, peer: &str) -> Result<OlderPage> {
        let hubs: Vec<(String, HubClient)> = self
            .hubs
            .lock()
            .unwrap()
            .iter()
            .map(|(u, h)| (u.clone(), h.client.clone()))
            .collect();
        let me = self.me.address();
        let mut out = OlderPage::default();
        for (url, client) in hubs {
            let Some(start) = self
                .store
                .meta(&start_key(&url))?
                .and_then(|v| v.parse::<i64>().ok())
            else {
                continue; // synced from the beginning: it has everything
            };
            let mark = self.store.history_mark(&url, peer)?.unwrap_or_default();
            if mark.done {
                continue;
            }
            if !self.connected_soon(&url).await {
                out.unreachable.push(url);
                continue;
            }
            let before = match &mark.before {
                Some(c) => crate::hub_v2::Before::Cursor(c.clone()),
                // the start's own millisecond too: sync may not bring it
                None => crate::hub_v2::Before::Time(start + 1),
            };
            let page = match client.history(&self.me, peer, &before, HISTORY_PAGE).await {
                Ok(p) => p,
                Err(e) if e.status().is_some() => return Err(e),
                // the connection failed: as good as offline
                Err(_) => {
                    out.unreachable.push(url);
                    continue;
                }
            };
            let mut oldest = mark.oldest_ms;
            let mut old_unread = mark.old_unread;
            let offset = self.offset(&url);
            for m in &page.messages {
                if let Some(t) = hub_ms(&m.env.received_at) {
                    oldest = Some(oldest.map_or(t, |o| o.min(t)));
                }
                if m.env.body.starts_with(crate::link::LINK_MESSAGE_PREFIX) {
                    continue;
                }
                if self.store.message(&m.env.id)?.is_none() {
                    out.added += 1;
                }
                self.store.upsert_synced_at(&url, &me, m, offset)?;
                // an unread one this hub's chat list counted is now here
                if m.env.from != me
                    && m.read_at.is_none()
                    && mark.listed_id.as_deref() != Some(m.env.id.as_str())
                {
                    old_unread -= 1;
                }
                if m.body_bytes.is_some() {
                    if let Ok(body) = client
                        .message_body(&self.me, &m.env.id, LONG_BODY_FETCH_MAX)
                        .await
                    {
                        let body = if m.env.reply_to.is_some() {
                            strip_quote(&body).to_owned()
                        } else {
                            body
                        };
                        self.store.set_body(&m.env.id, &body)?;
                    }
                }
            }
            let done = page.before.is_none();
            self.store.set_history_mark(
                &url,
                peer,
                &crate::store::HistoryMark {
                    before: page.before,
                    oldest_ms: oldest,
                    done,
                    old_unread: old_unread.max(0),
                    listed_id: mark.listed_id.clone(),
                },
            )?;
            if !done {
                out.more.push(url);
            }
        }
        if out.added > 0 {
            self.host.event(Event::Chat { peer: peer.into() });
        }
        Ok(out)
    }

    /// The hub is connected, or becomes so within a few seconds (right
    /// after start it is still connecting: not "down" for that moment).
    async fn connected_soon(&self, url: &str) -> bool {
        let t0 = std::time::Instant::now();
        loop {
            let state = self.hubs.lock().unwrap().get(url).map(|h| h.status.state.clone());
            match state {
                Some(HubState::Connected) => return true,
                Some(HubState::Connecting) if t0.elapsed() < CONNECTING_GRACE => {
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
                _ => return false,
            }
        }
    }

    // ---------------------------------------------------------- sending

    pub fn send(&self, msg: NewOutgoing) -> Result<()> {
        if msg.peer == self.me.address() {
            return Err(Error::Invalid("that's you".into()));
        }
        self.store.queue_outgoing(&msg, &now())?;
        self.host.event(Event::Chat {
            peer: msg.peer.clone(),
        });
        self.queue_changed.notify_one();
        Ok(())
    }

    /// Where a chat's messages go (the hub picker): the pinned hub if any,
    /// the Automatic choice, and the hubs that list the peer.
    pub fn send_route(&self, peer: &str) -> Result<SendRoute> {
        let r = self.route(peer, &self.connected_hubs())?;
        let mut hubs: Vec<RouteHub> = r
            .reaching
            .into_iter()
            .map(|(url, online)| RouteHub { url, online })
            .collect();
        hubs.sort_by(|a, b| a.url.cmp(&b.url));
        Ok(SendRoute {
            pinned: r.pinned,
            automatic: r.automatic.map(|(u, _, _)| u),
            next: r.next.map(|(u, _, _)| u),
            hubs,
        })
    }

    /// Pin a chat to one of our hubs, or back to Automatic with None.
    pub fn set_send_hub(&self, peer: &str, hub: Option<&str>) -> Result<()> {
        self.store.set_send_hub(peer, hub)?;
        self.host.event(Event::Chat { peer: peer.into() });
        self.queue_changed.notify_one();
        Ok(())
    }

    /// A pinned hub is used only while it is connected and lists the peer;
    /// otherwise the chat's messages wait for it rather than going another
    /// way (user 2026-10-09 08:41Z: "say so plainly instead of silently
    /// switching").
    fn route(&self, peer: &str, connected: &[(String, HubClient, u64)]) -> Result<Route> {
        let reaching = self.store.hubs_reaching(peer)?;
        let last = self.store.last_send_hub(peer)?;
        let pinned = self.store.send_hub(peer)?;
        let automatic = pick_hub(connected, &reaching, last.as_deref());
        let next = match &pinned {
            Some(p) if reaching.iter().any(|(u, _)| u == p) => {
                connected.iter().find(|(u, _, _)| u == p).cloned()
            }
            Some(_) => None,
            None => automatic.clone(),
        };
        Ok(Route {
            pinned,
            automatic,
            next,
            reaching,
        })
    }

    /// Put a failed message back in the queue (Retry). Uploads restart from zero.
    pub fn retry(&self, id: &str) -> Result<()> {
        if let Some(m) = self.store.message(id)? {
            for a in &m.attachments {
                if a.state != "uploaded" {
                    self.store
                        .set_attachment(&a.local_id, "pending", None, None, None)?;
                }
            }
            self.store.set_state(id, "queued", None)?;
            self.host.event(Event::Chat { peer: m.peer });
            self.queue_changed.notify_one();
        }
        Ok(())
    }

    pub fn cancel_transfer(&self, local_id: &str) {
        if let Some(c) = self.transfers.lock().unwrap().get(local_id) {
            c.cancel();
        }
    }

    async fn sender_loop(self: Arc<Self>) {
        while !self.stopped.load(std::sync::atomic::Ordering::Relaxed) {
            let mut progressed = false;
            if let Ok(queue) = self.store.queued() {
                for m in queue {
                    match self.send_one(&m.id).await {
                        Ok(true) => progressed = true,
                        Ok(false) => {}
                        Err(_) => {}
                    }
                }
            }
            if !progressed {
                tokio::select! {
                    _ = self.queue_changed.notified() => {}
                    _ = tokio::time::sleep(Duration::from_secs(30)) => {}
                }
            }
        }
    }

    /// Try to send one queued message. Ok(true) = it left the queue.
    async fn send_one(&self, id: &str) -> Result<bool> {
        let Some(m) = self.store.message(id)? else {
            return Ok(false);
        };
        let connected = self.connected_hubs();
        if connected.is_empty() {
            return Ok(false);
        }
        let Some((url, client, max)) = self.route(&m.peer, &connected)?.next else {
            return Ok(false);
        }; // its hubs are down: wait
        let total: u64 = m.body.len() as u64 + m.attachments.iter().map(|a| a.bytes).sum::<u64>();
        if !m.attachments.is_empty() && total > max {
            self.fail(
                &m.peer,
                id,
                &format!("too large for hub {url}: limit {max} bytes"),
            )?;
            return Ok(true);
        }
        self.store.set_state(id, "sending", None)?;
        let mut hub_ids = Vec::new();
        for a in &m.attachments {
            // ids are per hub: after a route change the file goes up again
            if a.state == "uploaded" && a.hub.as_deref() == Some(url.as_str()) {
                if let Some(h) = &a.hub_id {
                    hub_ids.push(h.clone());
                    continue;
                }
            }
            let Some(source) = &a.source else { continue };
            let file = match self.host.open_source(source) {
                Ok(f) => tokio::fs::File::from_std(f),
                Err(e) => {
                    self.store.set_attachment(
                        &a.local_id,
                        "failed",
                        None,
                        None,
                        Some(&e.to_string()),
                    )?;
                    self.fail(&m.peer, id, &format!("can't read {}: {e}", a.name))?;
                    return Ok(true);
                }
            };
            let cancel = CancelFlag::default();
            self.transfers
                .lock()
                .unwrap()
                .insert(a.local_id.clone(), cancel.clone());
            self.store
                .set_attachment(&a.local_id, "uploading", None, None, None)?;
            let host = self.host.clone();
            let (lid, mid) = (a.local_id.clone(), m.id.clone());
            let progress: crate::hub::Progress = Arc::new(move |done, total| {
                host.event(Event::Transfer {
                    local_id: lid.clone(),
                    message_id: mid.clone(),
                    upload: true,
                    done,
                    total,
                })
            });
            let resumable = a.bytes >= RESUMABLE_MIN
                && self
                    .hubs
                    .lock()
                    .unwrap()
                    .get(&url)
                    .is_some_and(|h| h.status.features.iter().any(|f| f == "uploads"));
            let r = if resumable {
                self.upload_resumable(&client, &url, a, file, progress, cancel.clone())
                    .await
            } else {
                client
                    .upload(&self.me, file, &a.name, Some(progress), cancel.clone())
                    .await
            };
            self.transfers.lock().unwrap().remove(&a.local_id);
            match r {
                Ok(meta) => {
                    self.store
                        .set_upload(&a.local_id, "uploaded", &meta.id, &url)?;
                    hub_ids.push(meta.id);
                }
                Err(e)
                    if resumable
                        && !cancel.is_cancelled()
                        && matches!(e, Error::Unreachable(_)) =>
                {
                    // The connection dropped mid-file: the hub keeps what it
                    // got. Wait for the network and continue from there.
                    self.store.set_attachment(
                        &a.local_id,
                        "pending",
                        None,
                        None,
                        Some(&e.to_string()),
                    )?;
                    self.store.set_state(id, "queued", None)?;
                    self.host.event(Event::Chat {
                        peer: m.peer.clone(),
                    });
                    return Ok(false);
                }
                Err(e) => {
                    let what = if cancel.is_cancelled() {
                        "cancelled"
                    } else {
                        "failed"
                    };
                    self.store.set_attachment(
                        &a.local_id,
                        what,
                        None,
                        None,
                        Some(&e.to_string()),
                    )?;
                    self.fail(&m.peer, id, &format!("upload {what}: {}", a.name))?;
                    return Ok(true);
                }
            }
        }
        let reply_field = self
            .hubs
            .lock()
            .unwrap()
            .get(&url)
            .is_some_and(|h| h.status.features.iter().any(|f| f == "reply_to"));
        let mut body = self.wire_body(&m)?;
        let mut body_part = None;
        let long_ok = self
            .hubs
            .lock()
            .unwrap()
            .get(&url)
            .is_some_and(|h| h.status.features.iter().any(|f| f == "long_messages"));
        if long_ok && body.len() as u64 > BODY_INLINE_MAX {
            match client
                .upload_body(&self.me, std::mem::take(&mut body))
                .await
            {
                Ok(part) => body_part = Some(part),
                Err(e) if matches!(e.status(), Some(413) | Some(422)) => {
                    self.fail(&m.peer, id, &e.to_string())?;
                    return Ok(true);
                }
                Err(_) => {
                    self.store.set_state(id, "queued", None)?;
                    return Ok(false);
                }
            }
        }
        let out = Outgoing {
            id: m.id.clone(),
            to: m.peer.clone(),
            body,
            body_part,
            reply_to: if reply_field {
                m.reply_to.clone()
            } else {
                None
            },
            kind: m.kind.clone(),
            thread_id: None,
            sent_at: m.sent_at.clone(),
            attachments: hub_ids,
        };
        match client.send(&self.me, &out).await {
            Ok(r) => {
                if !self.store.mark_sent(id, &url, &r.received_at)? {
                    // deleted while it was on its way: the hub's copy goes too
                    self.store.queue_deletes(&url, &[m.id.clone()], &now())?;
                    self.send_deletes_now(&[url.clone()]).await;
                }
                self.host.event(Event::Chat { peer: m.peer });
                Ok(true)
            }
            Err(e) if matches!(e.status(), Some(422) | Some(413) | Some(403)) => {
                self.fail(&m.peer, id, &e.to_string())?;
                Ok(true)
            }
            Err(_) => {
                // Network trouble: back to the queue, the hub task reconnects.
                self.store.set_state(id, "queued", None)?;
                Ok(false)
            }
        }
    }

    /// The body as sent: a reply starts with a one-line quote of what it
    /// answers, so clients without reply_to (Orgtree, older hubs) still see
    /// the context. `strip_quote` removes it again on receipt.
    fn wire_body(&self, m: &crate::store::Message) -> Result<String> {
        let Some(rid) = &m.reply_to else {
            return Ok(m.body.clone());
        };
        let quoted = self.store.message(rid)?.map(|q| q.body).unwrap_or_default();
        let line: String = quoted
            .lines()
            .next()
            .unwrap_or("")
            .chars()
            .take(120)
            .collect();
        Ok(format!("{QUOTE_PREFIX}{line}\n{}", m.body))
    }

    /// v2 hubs: upload in a resumable session; a retry continues where the
    /// hub's copy ends (the session id lives in the attachment's hub_id).
    async fn upload_resumable(
        &self,
        client: &HubClient,
        url: &str,
        a: &crate::store::Attachment,
        file: tokio::fs::File,
        progress: crate::hub::Progress,
        cancel: CancelFlag,
    ) -> Result<crate::hub::AttachmentMeta> {
        // a session on this hub only (the route may have changed)
        let existing = a
            .hub_id
            .clone()
            .filter(|_| a.state != "uploaded" && a.hub.as_deref() == Some(url));
        let upload_id = match existing {
            Some(id) if client.upload_state(&self.me, &id).await.is_ok() => id,
            _ => {
                let st = client.open_upload(&self.me, &a.name, a.bytes).await?;
                self.store
                    .set_upload(&a.local_id, "uploading", &st.id, url)?;
                st.id
            }
        };
        let st = client
            .resume_upload(&self.me, &upload_id, file, Some(progress), cancel)
            .await?;
        if !st.complete {
            return Err(Error::Unreachable("upload stopped before the end".into()));
        }
        Ok(crate::hub_v2::upload_meta(&st, &a.name))
    }

    fn fail(&self, peer: &str, id: &str, why: &str) -> Result<()> {
        self.store.set_state(id, "failed", Some(why))?;
        self.host.event(Event::Chat { peer: peer.into() });
        Ok(())
    }

    // --------------------------------------------------------- deleting

    /// "Delete for me": our copy goes from this device and, on v2 hubs, from
    /// every hub (so from all our devices); the other side keeps theirs.
    /// Every hub gets it, not just those known to hold a copy: that list
    /// misses copies (a send whose answer was lost, data from before it was
    /// kept), and an id a hub doesn't have costs nothing. A hub that is down
    /// gets it when it is back (B2). A send still under way stops, or its
    /// copy is deleted when it lands.
    pub async fn delete_message(&self, id: &str) -> Result<()> {
        let m = self.store.message(id)?;
        let hubs: Vec<String> = self.hubs.lock().unwrap().keys().cloned().collect();
        let lids = self.store.forget_message(id, &hubs, &now())?;
        self.cancel_transfers(&lids);
        if let Some(m) = m {
            self.host.event(Event::Chat { peer: m.peer });
        }
        self.send_deletes_now(&hubs).await;
        Ok(())
    }

    /// "Delete chat": on every hub. A connected hub deletes the conversation
    /// at once; one that is down (or fails) gets each message's delete when
    /// it is back, never the conversation's, which would take messages that
    /// came after the user deleted it (B2).
    pub async fn delete_chat(&self, peer: &str) -> Result<()> {
        let hubs: Vec<(String, HubClient, bool, bool)> = self
            .hubs
            .lock()
            .unwrap()
            .iter()
            .map(|(u, h)| {
                let connected = h.status.state == HubState::Connected;
                let capable = h.status.features.iter().any(|f| f == "delete");
                (u.clone(), h.client.clone(), connected, capable)
            })
            .collect();
        let later: Vec<String> = hubs
            .iter()
            .filter(|(_, _, connected, _)| !connected)
            .map(|(u, ..)| u.clone())
            .collect();
        let t = now();
        let (ids, lids) = self.store.forget_chat(peer, &later, &t)?;
        self.cancel_transfers(&lids);
        self.host.event(Event::Chat { peer: peer.into() });
        for (url, client, connected, capable) in hubs {
            let owed = !connected
                || (capable && client.delete_conversation(&self.me, peer).await.is_err());
            if connected && owed {
                self.store.queue_deletes(&url, &ids, &t)?;
            }
            // messages it holds that this device never had (lazy history):
            // its history up to now goes too, once it is back
            if owed {
                let at = unix_ms() as i64 + self.offset(&url);
                self.store.queue_chat_delete(&url, peer, at)?;
            }
        }
        Ok(())
    }

    fn cancel_transfers(&self, local_ids: &[String]) {
        let transfers = self.transfers.lock().unwrap();
        for lid in local_ids {
            if let Some(c) = transfers.get(lid) {
                c.cancel();
            }
        }
    }

    /// Send the deletes these hubs owe, those that are connected (best
    /// effort: what fails stays queued for the hub's own session).
    async fn send_deletes_now(&self, urls: &[String]) {
        for url in urls {
            let rt = self.hubs.lock().unwrap().get(url).and_then(|h| {
                (h.status.state == HubState::Connected)
                    .then(|| (h.client.clone(), h.status.features.clone()))
            });
            if let Some((client, features)) = rt {
                let _ = self
                    .send_queued_deletes(&client, url, &features, unix_ms())
                    .await;
            }
        }
    }

    /// Send the deletes this hub owes that are due by `due_ms`. Runs before
    /// every poll or sync, so a hub never hands back what we deleted; a hub
    /// without "delete" (v1) keeps no history, so it owes nothing. Network
    /// trouble or a 401 ends the session (it backs off and registers again);
    /// a hub that is busy or failing keeps that one for later and goes on
    /// with the rest; a hub that refuses one drops it.
    async fn send_queued_deletes(
        &self,
        client: &HubClient,
        url: &str,
        features: &[String],
        due_ms: u64,
    ) -> Result<()> {
        let capable = features.iter().any(|f| f == "delete");
        let lazy = features.iter().any(|f| f == "lazy_history");
        for (peer, before) in self.store.chat_deletes(url)? {
            if capable && lazy {
                self.send_chat_delete(client, url, &peer, before).await?;
            } else {
                // no history by time: the per-message deletes are all it gets
                self.store.chat_delete_done(url, &peer)?;
            }
        }
        let mut tried = std::collections::HashSet::new();
        loop {
            let batch: Vec<String> = self
                .store
                .due_deletes(url, due_ms)?
                .into_iter()
                .filter(|id| !tried.contains(id))
                .collect();
            if batch.is_empty() {
                return Ok(());
            }
            for id in batch {
                let r = if capable {
                    client.delete_message(&self.me, &id).await.map(|_| ())
                } else {
                    Ok(())
                };
                match r.as_ref().map_err(drain_outcome) {
                    Ok(()) | Err(Drain::Drop) => self.store.delete_done(url, &id)?,
                    Err(Drain::Later) => self.store.delete_later(url, &id, unix_ms())?,
                    Err(Drain::Stop) => {
                        self.store.delete_later(url, &id, unix_ms())?;
                        return r;
                    }
                }
                tried.insert(id);
            }
        }
    }

    /// An owed "Delete chat" on a hub with lazy history: every message of
    /// the chat it received before the delete, page by page (a later
    /// message stays, as B2 requires). Where it has got to is kept, so a
    /// broken connection goes on from there.
    async fn send_chat_delete(
        &self,
        client: &HubClient,
        url: &str,
        peer: &str,
        mut before: String,
    ) -> Result<()> {
        loop {
            let at = match before.parse::<i64>() {
                Ok(ms) => crate::hub_v2::Before::Time(ms),
                Err(_) => crate::hub_v2::Before::Cursor(before.clone()),
            };
            let page = match client.history(&self.me, peer, &at, 200).await {
                Ok(p) => p,
                Err(e) => match drain_outcome(&e) {
                    Drain::Drop => return self.store.chat_delete_done(url, peer),
                    Drain::Later => return Ok(()),
                    Drain::Stop => return Err(e),
                },
            };
            for m in &page.messages {
                if let Err(e) = client.delete_message(&self.me, &m.env.id).await {
                    match drain_outcome(&e) {
                        Drain::Drop => {}
                        Drain::Later => return Ok(()),
                        Drain::Stop => return Err(e),
                    }
                }
            }
            match page.before {
                Some(next) => {
                    self.store.set_chat_delete(url, peer, &next)?;
                    before = next;
                }
                None => return self.store.chat_delete_done(url, peer),
            }
        }
    }

    /// The devices that sync as us, per v2 hub (deduplicated by device id).
    pub async fn devices(&self) -> Vec<crate::hub_v2::DeviceEntry> {
        let clients: Vec<HubClient> = self
            .hubs
            .lock()
            .unwrap()
            .values()
            .filter(|h| h.status.features.iter().any(|f| f == "devices"))
            .map(|h| h.client.clone())
            .collect();
        let mut out: Vec<crate::hub_v2::DeviceEntry> = Vec::new();
        for c in clients {
            if let Ok(list) = c.devices(&self.me).await {
                for d in list {
                    match out.iter_mut().find(|x| x.device_id == d.device_id) {
                        Some(x) => x.online |= d.online,
                        None => out.push(d),
                    }
                }
            }
        }
        out
    }

    // ---------------------------------------------------------- reading

    /// The chat is on screen: mark it seen and send read receipts (if on).
    pub async fn mark_read(&self, peer: &str) -> Result<()> {
        let ids = self.store.mark_seen(peer, &now())?;
        if ids.is_empty() {
            return Ok(());
        }
        self.host.event(Event::Chat { peer: peer.into() });
        if !self
            .read_receipts
            .load(std::sync::atomic::Ordering::Relaxed)
        {
            return Ok(());
        }
        let t = now();
        let clients: HashMap<String, HubClient> = self
            .hubs
            .lock()
            .unwrap()
            .iter()
            .map(|(u, h)| (u.clone(), h.client.clone()))
            .collect();
        let mut by_hub: HashMap<String, Vec<(String, &str, String)>> = HashMap::new();
        for (id, hub) in ids {
            if let Some(h) = hub {
                by_hub.entry(h).or_default().push((id, "read", t.clone()));
            }
        }
        for (hub, items) in by_hub {
            if let Some(c) = clients.get(&hub) {
                let _ = c.receipts(&self.me, &items).await;
            }
        }
        Ok(())
    }

    /// Download an incoming attachment into the host's download folder.
    pub async fn download(&self, message_id: &str, local_id: &str) -> Result<PathBuf> {
        let m = self
            .store
            .message(message_id)?
            .ok_or_else(|| Error::Invalid("no such message".into()))?;
        let a = m
            .attachments
            .iter()
            .find(|a| a.local_id == local_id)
            .ok_or_else(|| Error::Invalid("no such attachment".into()))?;
        let hub_id = a
            .hub_id
            .clone()
            .ok_or_else(|| Error::Invalid("not on a hub".into()))?;
        let hub = a
            .hub
            .clone()
            .or_else(|| m.hub.clone())
            .ok_or_else(|| Error::Invalid("unknown hub".into()))?;
        let client = self
            .hubs
            .lock()
            .unwrap()
            .get(&hub)
            .map(|h| h.client.clone())
            .ok_or_else(|| Error::Invalid(format!("hub {hub} was removed")))?;
        let dir = self.host.download_dir();
        tokio::fs::create_dir_all(&dir).await?;
        let dest = unique_path(&dir, &a.name);
        let cancel = CancelFlag::default();
        self.transfers
            .lock()
            .unwrap()
            .insert(local_id.into(), cancel.clone());
        self.store
            .set_attachment(local_id, "downloading", None, None, None)?;
        let host = self.host.clone();
        let (lid, mid) = (local_id.to_string(), message_id.to_string());
        let progress: crate::hub::Progress = Arc::new(move |done, total| {
            host.event(Event::Transfer {
                local_id: lid.clone(),
                message_id: mid.clone(),
                upload: false,
                done,
                total,
            })
        });
        let r = client
            .download_file(&self.me, &hub_id, &dest, Some(progress), cancel.clone())
            .await;
        self.transfers.lock().unwrap().remove(local_id);
        let out = match r {
            Ok(_) => {
                self.store.set_attachment(
                    local_id,
                    "done",
                    None,
                    Some(&dest.to_string_lossy()),
                    None,
                )?;
                Ok(dest)
            }
            Err(e) => {
                let state = match (&e, cancel.is_cancelled()) {
                    (_, true) => "cancelled",
                    (Error::Hub { status: 410, .. }, _) => "expired",
                    _ => "failed",
                };
                self.store
                    .set_attachment(local_id, state, None, None, Some(&e.to_string()))?;
                Err(e)
            }
        };
        self.host.event(Event::Chat { peer: m.peer });
        out
    }

    /// Fetch an incoming attachment to `dest` for showing it in the chat (an
    /// image preview), leaving its download state alone: Download still saves
    /// it where downloads go. A hub that no longer has it marks it expired.
    pub async fn fetch_preview(
        &self,
        message_id: &str,
        local_id: &str,
        dest: &std::path::Path,
    ) -> Result<()> {
        let m = self
            .store
            .message(message_id)?
            .ok_or_else(|| Error::Invalid("no such message".into()))?;
        let a = m
            .attachments
            .iter()
            .find(|a| a.local_id == local_id)
            .ok_or_else(|| Error::Invalid("no such attachment".into()))?;
        let (Some(hub_id), Some(hub)) = (a.hub_id.clone(), a.hub.clone().or_else(|| m.hub.clone())) else {
            return Err(Error::Invalid("not on a hub".into()));
        };
        let client = self
            .hubs
            .lock()
            .unwrap()
            .get(&hub)
            .map(|h| h.client.clone())
            .ok_or_else(|| Error::Invalid(format!("hub {hub} was removed")))?;
        if let Some(dir) = dest.parent() {
            tokio::fs::create_dir_all(dir).await?;
        }
        // a partial file never passes for a whole one (and two fetches of
        // the same file don't share one)
        static N: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let part = dest.with_extension(format!("part{n}"));
        match client
            .download_file(&self.me, &hub_id, &part, None, CancelFlag::default())
            .await
        {
            Ok(_) => {
                tokio::fs::rename(&part, dest).await?;
                Ok(())
            }
            Err(e) => {
                let _ = tokio::fs::remove_file(&part).await;
                if let Error::Hub { status: 410, .. } = &e {
                    self.store
                        .set_attachment(local_id, "expired", None, None, Some(&e.to_string()))?;
                    self.host.event(Event::Chat { peer: m.peer });
                }
                Err(e)
            }
        }
    }
}

/// What a failed delete of a queued item means for it.
#[derive(Debug, PartialEq)]
enum Drain {
    /// Keep it, try again later, go on with the rest (busy, failing).
    Later,
    /// Keep it and end the session (network trouble, or the hub forgot us).
    Stop,
    /// The hub refuses it for good: drop it.
    Drop,
}

fn drain_outcome(e: &Error) -> Drain {
    match e.status() {
        Some(401) => Drain::Stop,
        Some(408) | Some(429) => Drain::Later,
        Some(s) if s >= 500 => Drain::Later,
        Some(_) => Drain::Drop,
        None => match e {
            Error::Unreachable(_) | Error::NotAHub(_) | Error::Io(_) => Drain::Stop,
            _ => Drain::Later,
        },
    }
}

/// A chat's route as the hub picker shows it.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct SendRoute {
    /// The hub the user pinned this chat to; None = Automatic.
    pub pinned: Option<String>,
    /// The hub Automatic would use now (None: none can, it waits).
    pub automatic: Option<String>,
    /// The hub the next message goes through (None: it waits).
    pub next: Option<String>,
    /// Our hubs whose directory lists the peer, by address.
    pub hubs: Vec<RouteHub>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct RouteHub {
    pub url: String,
    /// The peer is online there.
    pub online: bool,
}

struct Route {
    pinned: Option<String>,
    automatic: Option<(String, HubClient, u64)>,
    next: Option<(String, HubClient, u64)>,
    reaching: Vec<(String, bool)>,
}

/// Which connected hub a message to a peer goes through (B1: a stable
/// choice, so a chat doesn't hop between hubs): among connected hubs whose
/// roster lists the peer, one where the peer is online; among those, the
/// hub the chat `last` went through, else the first by address. If no
/// roster lists the peer anywhere, the first connected hub by address
/// (which answers 422 for an unknown address).
fn pick_hub(
    connected: &[(String, HubClient, u64)],
    reaching: &[(String, bool)],
    last: Option<&str>,
) -> Option<(String, HubClient, u64)> {
    let get = |url: &str| connected.iter().find(|(u, _, _)| u == url);
    if reaching.is_empty() {
        return connected.iter().min_by(|a, b| a.0.cmp(&b.0)).cloned();
    }
    let usable: Vec<&(String, bool)> = reaching.iter().filter(|(u, _)| get(u).is_some()).collect();
    let online = usable.iter().any(|(_, on)| *on);
    let mut best: Vec<&str> = usable
        .iter()
        .filter(|(_, on)| *on == online)
        .map(|(u, _)| u.as_str())
        .collect();
    best.sort();
    let url = best
        .iter()
        .find(|u| Some(**u) == last)
        .or(best.first())?;
    get(*url).cloned()
}

/// `name`, or `name (2)`, `name (3)`... so a download never overwrites.
/// The name is reduced to its last path component first.
fn unique_path(dir: &std::path::Path, name: &str) -> PathBuf {
    let base = std::path::Path::new(name)
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .filter(|s| !s.is_empty() && s != "." && s != "..")
        .unwrap_or_else(|| "file".into());
    let first = dir.join(&base);
    if !first.exists() {
        return first;
    }
    let (stem, ext) = match base.rsplit_once('.') {
        Some((s, e)) if !s.is_empty() => (s.to_string(), format!(".{e}")),
        _ => (base.clone(), String::new()),
    };
    (2..)
        .map(|i| dir.join(format!("{stem} ({i}){ext}")))
        .find(|p| !p.exists())
        .expect("unbounded")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hubs(urls: &[&str]) -> Vec<(String, HubClient, u64)> {
        urls.iter()
            .map(|u| {
                let a = HubAddress::parse(u).unwrap();
                (a.to_string(), HubClient::new(a), 0)
            })
            .collect()
    }

    fn pick(connected: &[&str], reaching: &[(&str, bool)], last: Option<&str>) -> Option<String> {
        let connected = hubs(connected);
        let url = |u: &str| HubAddress::parse(u).unwrap().to_string();
        let reaching: Vec<(String, bool)> =
            reaching.iter().map(|(u, on)| (url(u), *on)).collect();
        let last = last.map(url);
        pick_hub(&connected, &reaching, last.as_deref()).map(|(u, _, _)| u)
    }

    /// Review finding 4: what a queued delete's failure means.
    #[test]
    fn a_failed_delete_is_kept_dropped_or_ends_the_session() {
        let hub = |status| Error::Hub { status, detail: String::new() };
        for s in [408, 429, 500, 502, 503] {
            assert_eq!(drain_outcome(&hub(s)), Drain::Later, "{s}");
        }
        for s in [400, 403, 404, 410, 422] {
            assert_eq!(drain_outcome(&hub(s)), Drain::Drop, "{s}");
        }
        assert_eq!(drain_outcome(&hub(401)), Drain::Stop);
        assert_eq!(drain_outcome(&Error::Unreachable("down".into())), Drain::Stop);
    }

    /// B1: the hub a message goes through doesn't depend on the order the
    /// engine happens to hold its hubs in (a HashMap's).
    #[test]
    fn sending_picks_a_stable_hub() {
        let (a, b, c) = ("127.0.0.1:7001", "127.0.0.1:7002", "127.0.0.1:7003");
        let url = |u: &str| Some(HubAddress::parse(u).unwrap().to_string());
        for order in [[a, b, c], [c, b, a], [b, a, c], [b, c, a]] {
            // online beats offline, whatever the order
            assert_eq!(pick(&order, &[(b, true), (a, false)], None), url(b));
            // among online hubs, the one this chat last went through
            assert_eq!(pick(&order, &[(a, true), (b, true)], Some(b)), url(b));
            // ...but not when it is offline there and another is online
            assert_eq!(pick(&order, &[(a, true), (b, false)], Some(b)), url(a));
            // all offline: still the one last used
            assert_eq!(pick(&order, &[(a, false), (c, false)], Some(c)), url(c));
            // nothing to go on: the first by address
            assert_eq!(pick(&order, &[(b, true), (c, true)], None), url(b));
            // listed nowhere: the first connected hub by address (it answers 422)
            assert_eq!(pick(&order, &[], None), url(a));
        }
        // a hub that isn't connected is never picked
        assert_eq!(pick(&[a], &[(b, true), (a, false)], Some(b)), url(a));
        assert_eq!(pick(&[c], &[(b, true), (a, false)], None), None);
    }

    #[test]
    fn unique_path_never_escapes_or_overwrites() {
        let d = tempfile::tempdir().unwrap();
        assert_eq!(
            unique_path(d.path(), "../../evil.txt"),
            d.path().join("evil.txt")
        );
        std::fs::write(d.path().join("a.txt"), b"x").unwrap();
        assert_eq!(unique_path(d.path(), "a.txt"), d.path().join("a (2).txt"));
        assert_eq!(unique_path(d.path(), ".."), d.path().join("file"));
    }
}

#[cfg(test)]
mod quote_tests {
    use super::strip_quote;

    #[test]
    fn strips_only_a_leading_quote_line() {
        assert_eq!(
            strip_quote("> earlier question\nmy answer\nmore"),
            "my answer\nmore"
        );
        assert_eq!(strip_quote("no quote here"), "no quote here");
        assert_eq!(strip_quote("> only a quote"), "> only a quote");
    }
}
