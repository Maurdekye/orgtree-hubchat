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
use crate::store::{NewOutgoing, Store};
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
        Ok(())
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
            if unregister {
                let _ = rt.client.unregister(&self.me).await;
            }
        }
        self.store.remove_hub(url)?;
        self.host.event(Event::Hub { url: url.into() });
        self.host.event(Event::Directory);
        Ok(())
    }

    /// One check, for Android's periodic mode (design D6): start every hub's
    /// session over, then wait (up to `timeout`) until each has taken in an
    /// answer or has a request parked (no news), and nothing waits to be
    /// sent. True when everything got through in time.
    pub async fn check_now(&self, timeout: Duration) -> bool {
        let start = unix_ms();
        for rt in self.hubs.lock().unwrap().values() {
            rt.kick.notify_one();
        }
        self.queue_changed.notify_one();
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
        let (url, client, _) = self
            .connected_hubs()
            .into_iter()
            .find(|(u, _, _)| reaching.contains(u))
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
        let health = client.healthz().await?;
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
            return self.sync_session(client, url, stop, &profile).await;
        }
        let mut registered_again = false;
        while !stop.is_cancelled() {
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
                    let preview: String = m.body.chars().take(140).collect();
                    self.host.event(Event::Incoming {
                        peer: m.from.clone(),
                        id: m.id.clone(),
                        preview,
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
    ) -> Result<()> {
        let key = format!("sync.cursor.{url}");
        let me = self.me.address();
        let (device_id, device_name) = self.device();
        let mut cursor = self.store.meta(&key)?;
        // History (from the beginning, until a page without `more`) is not
        // news: it raises no notifications.
        let mut catching_up = cursor.is_none();
        let mut registered_again = false;
        while !stop.is_cancelled() {
            let first_page = cursor.is_none();
            self.mark(url, |s| s.waiting_since_ms = Some(unix_ms()));
            let r = match client
                .sync(
                    &self.me,
                    &device_id,
                    &device_name,
                    cursor.as_deref(),
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
            if r.reset {
                self.store.forget_hub_messages(url)?;
            }
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
                        let fresh = self.store.upsert_synced(url, &me, m)?;
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
                            let preview: String = m.env.body.chars().take(140).collect();
                            self.host.event(Event::Incoming {
                                peer: m.env.from.clone(),
                                id: m.env.id.clone(),
                                preview,
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
                self.answered(url);
            }
        }
        Ok(())
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
        // Route: a connected hub whose roster lists the peer; else, if no
        // roster lists it anywhere, the first connected hub (which answers
        // 422 for an unknown address).
        let reaching = self.store.hubs_reaching(&m.peer)?;
        let route = connected
            .iter()
            .find(|(u, _, _)| reaching.contains(u))
            .or_else(|| {
                if reaching.is_empty() {
                    connected.first()
                } else {
                    None
                }
            })
            .cloned();
        let Some((url, client, max)) = route else {
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
            if a.state == "uploaded" {
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
                self.upload_resumable(&client, a, file, progress, cancel.clone())
                    .await
            } else {
                client
                    .upload(&self.me, file, &a.name, Some(progress), cancel.clone())
                    .await
            };
            self.transfers.lock().unwrap().remove(&a.local_id);
            match r {
                Ok(meta) => {
                    self.store.set_attachment(
                        &a.local_id,
                        "uploaded",
                        Some(&meta.id),
                        None,
                        None,
                    )?;
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
                self.store.mark_sent(id, &url, &r.received_at)?;
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
        a: &crate::store::Attachment,
        file: tokio::fs::File,
        progress: crate::hub::Progress,
        cancel: CancelFlag,
    ) -> Result<crate::hub::AttachmentMeta> {
        let existing = a.hub_id.clone().filter(|_| a.state != "uploaded");
        let upload_id = match existing {
            Some(id) if client.upload_state(&self.me, &id).await.is_ok() => id,
            _ => {
                let st = client.open_upload(&self.me, &a.name, a.bytes).await?;
                self.store
                    .set_attachment(&a.local_id, "uploading", Some(&st.id), None, None)?;
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

    /// Connected hubs that can delete our copy on the hub (v2 "delete").
    fn delete_capable(&self) -> Vec<(String, HubClient)> {
        self.hubs
            .lock()
            .unwrap()
            .values()
            .filter(|h| {
                h.status.state == HubState::Connected
                    && h.status.features.iter().any(|f| f == "delete")
            })
            .map(|h| (h.status.url.clone(), h.client.clone()))
            .collect()
    }

    /// "Delete for me": our copy goes from this device and, on a v2 hub,
    /// from the hub (so from all our devices); the other side keeps theirs.
    pub async fn delete_message(&self, id: &str) -> Result<()> {
        let m = self.store.message(id)?;
        if let Some(m) = &m {
            if let Some(hub) = &m.hub {
                if let Some((_, c)) = self.delete_capable().into_iter().find(|(u, _)| u == hub) {
                    c.delete_message(&self.me, id).await?;
                }
            }
        }
        self.store.delete_message(id)?;
        if let Some(m) = m {
            self.host.event(Event::Chat { peer: m.peer });
        }
        Ok(())
    }

    pub async fn delete_chat(&self, peer: &str) -> Result<()> {
        for (_, c) in self.delete_capable() {
            c.delete_conversation(&self.me, peer).await?;
        }
        self.store.delete_chat(peer)?;
        self.host.event(Event::Chat { peer: peer.into() });
        Ok(())
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
        let ids = self.store.mark_seen(peer)?;
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
        let hub = m
            .hub
            .clone()
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
