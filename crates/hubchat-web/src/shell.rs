//! The shell of Hubchat Web, without the browser: one store, one identity,
//! one engine, and the UI's commands answered from them.
//!
//! The commands have the SAME names and argument shapes as the apps' Tauri
//! commands (`src-tauri/src/commands.rs`), because the same UI calls them
//! through the same `src/api.ts` table: a command renamed there is renamed
//! here. Nothing in this module touches JavaScript, so its tests run
//! natively; `wasm.rs` wraps it for the browser.
//!
//! What a browser cannot do yet answers with a sentence the UI can show
//! (`NOT_YET`), never with a pretend success.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use hubchat_core::engine::{Engine, Event, Host};
use hubchat_core::hub::{Health, Profile};
use hubchat_core::questions::{Answer, ReplyMeta};
use hubchat_core::store::{NewOutgoing, Store};
use hubchat_core::{recovery, rt, HubAddress, Identity};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

/// A command this shell does not do yet.
pub const NOT_YET: &str = "This isn't available in Hubchat Web yet.";

/// Spec R2.2: a page served over https may only call https.
pub const NEEDS_HTTPS: &str = "Hubchat Web can only reach a hub over HTTPS. Ask the hub's owner for its https:// address.";

/// Where events go: the event's name (`hc`, as the apps emit it) and its JSON.
pub type Sink = Arc<dyn Fn(&str, &str) + Send + Sync>;

pub(crate) type R<T> = Result<T, String>;

pub(crate) fn s<E: std::fmt::Display>(e: E) -> String {
    e.to_string()
}

/// This device had an identity, but its key is not in the browser's key
/// store (the lost-key screen).
#[derive(Clone, Serialize)]
pub struct KeyLost {
    address: Option<String>,
    unreadable: bool,
}

struct ShellHost {
    sink: Sink,
    /// Where this shell's files are (`files.rs`): `/` of Hubchat Web's own
    /// files in a browser, a folder of their own in a test.
    root: PathBuf,
}

impl Host for ShellHost {
    fn event(&self, ev: Event) {
        if let Ok(json) = serde_json::to_string(&ev) {
            (self.sink)("hc", &json);
        }
    }
    // In a browser the core opens a source through its own file layer and
    // never asks for a `std::fs::File` (there is none in a page).
    #[cfg(target_arch = "wasm32")]
    fn open_source(&self, _source: &str) -> std::io::Result<std::fs::File> {
        Err(std::io::Error::new(std::io::ErrorKind::Unsupported, NOT_YET))
    }
    #[cfg(not(target_arch = "wasm32"))]
    fn open_source(&self, source: &str) -> std::io::Result<std::fs::File> {
        std::fs::File::open(source)
    }
    fn download_dir(&self) -> PathBuf {
        self.root.join("downloads")
    }
    fn staging_dir(&self) -> PathBuf {
        self.root.join("outbox")
    }
}

/// An identity about to be taken on this device: the browser's key store
/// saves it first (`wasm.rs`), then `Shell::adopt` starts it, as the apps
/// save the key before they start the engine.
pub struct Adoption {
    pub me: Identity,
    pub brand_new: bool,
    pub(crate) meta: Vec<(&'static str, String)>,
    /// The hubs that come with it (a link, a key QR or a key file), added once it runs.
    pub(crate) hubs: Vec<String>,
}

pub struct Shell {
    store: Arc<Store>,
    engine: Mutex<Option<Arc<Engine>>>,
    host: Arc<ShellHost>,
    key_lost: Mutex<Option<KeyLost>>,
    /// What the browser calls this device ("Safari on iPhone").
    own_name: String,
    foreground: AtomicBool,
    open_chat: Mutex<Option<String>>,
    /// The link this browser is waiting on (`link.rs`): cancelled by a new one, Cancel or shutdown.
    pub(crate) link_stop: Mutex<Option<hubchat_core::hub::CancelFlag>>,
    /// What a link brought, until the user confirms or drops it. Memory only.
    pub(crate) pending: Mutex<Option<hubchat_core::link::Bundle>>,
    /// The hubs this page's server suggests (`hc_web_suggested_hubs`), which a link may be
    /// completed through (`link.rs`). Memory only: the page says them again at each start.
    pub(crate) suggested: Mutex<Vec<String>>,
}

fn must_wipe(owner: Option<&str>, has_data: bool, new: &str, brand_new: bool) -> bool {
    has_data && owner.map_or(brand_new, |o| o != new)
}

/// A hub address a page can call: https anywhere, http only on this very
/// machine (which a browser treats as secure).
pub fn page_can_reach(url: &str) -> bool {
    let Some(rest) = url.strip_prefix("http://") else { return url.starts_with("https://") };
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    let host = match authority.strip_prefix('[') {
        Some(v6) => v6.split(']').next().unwrap_or(""),
        None => authority.rsplit_once(':').map_or(authority, |(h, _)| h),
    };
    matches!(host, "localhost" | "127.0.0.1" | "::1")
}

pub(crate) fn arg<T: DeserializeOwned>(args: &Value, name: &str) -> R<T> {
    let v = args.get(name).cloned().unwrap_or(Value::Null);
    serde_json::from_value(v).map_err(|e| format!("invalid args `{name}`: {e}"))
}

pub(crate) fn out<T: Serialize>(v: T) -> R<Value> {
    serde_json::to_value(v).map_err(s)
}

#[derive(Serialize)]
#[serde(tag = "result", rename_all = "snake_case")]
pub(crate) enum Probe {
    Connected { url: String, name: String, max_attachment_bytes: u64, features: Vec<String>, version: Option<String>, orgs: Option<u64>, discovered: bool },
    Unreachable { url: String, error: String },
    NotAHub { url: String, error: String },
    Invalid { error: String },
}

/// What the composer sends (`hc_send`), as the apps' command takes it.
#[derive(Deserialize)]
struct SendRequest {
    #[serde(flatten)]
    msg: NewOutgoing,
    #[serde(default)]
    replies: Vec<String>,
    #[serde(default)]
    answers: Vec<Answer>,
}

impl Shell {
    /// Open the shell on its store. `identity`: what the browser's key store
    /// holds, if anything. Without one, a store that belonged to an identity
    /// shows the lost-key screen.
    pub fn open(store: Arc<Store>, sink: Sink, own_name: &str, identity: Option<Identity>) -> R<Arc<Shell>> {
        Self::open_at(store, sink, own_name, identity, PathBuf::from("/"))
    }

    /// `open`, with this shell's files under `root`.
    pub fn open_at(store: Arc<Store>, sink: Sink, own_name: &str, identity: Option<Identity>, root: PathBuf) -> R<Arc<Shell>> {
        let shell = Arc::new(Shell {
            store,
            engine: Mutex::new(None),
            host: Arc::new(ShellHost { sink, root }),
            key_lost: Mutex::new(None),
            own_name: own_name.trim().to_owned(),
            foreground: AtomicBool::new(false),
            open_chat: Mutex::new(None),
            link_stop: Mutex::new(None),
            pending: Mutex::new(None),
            suggested: Mutex::new(Vec::new()),
        });
        match identity {
            Some(me) => shell.start_with(me)?,
            None => {
                *shell.key_lost.lock().unwrap() =
                    shell.had_identity().then(|| KeyLost { address: shell.store.meta("identity.address").ok().flatten(), unreadable: false });
            }
        }
        Ok(shell)
    }

    pub fn store(&self) -> &Arc<Store> {
        &self.store
    }

    /// Where this shell's files are (`files.rs`).
    pub fn root(&self) -> &std::path::Path {
        &self.host.root
    }

    pub fn engine(&self) -> R<Arc<Engine>> {
        self.engine.lock().unwrap().clone().ok_or_else(|| "no identity yet".to_string())
    }

    /// One event to the page, under the name the apps emit it.
    pub(crate) fn emit(&self, name: &str, json: &str) {
        (self.host.sink)(name, json);
    }

    pub(crate) fn meta(&self, k: &str) -> String {
        self.store.meta(k).ok().flatten().unwrap_or_default()
    }

    fn had_identity(&self) -> bool {
        let has = |k: &str| self.store.meta(k).ok().flatten().is_some_and(|v| !v.is_empty());
        has("device.id") || has("identity.address")
    }

    pub(crate) fn device_name(&self) -> String {
        self.store.meta("device.name").ok().flatten().filter(|n| !n.trim().is_empty()).unwrap_or_else(|| {
            if self.own_name.is_empty() { "Hubchat Web".into() } else { self.own_name.clone() }
        })
    }

    fn start_with(&self, me: Identity) -> R<()> {
        let a = me.address();
        if self.store.meta("identity.address").ok().flatten().as_deref() != Some(a.as_str()) {
            let _ = self.store.set_meta("identity.address", &a);
        }
        *self.key_lost.lock().unwrap() = None;
        self.start_engine(me)
    }

    fn start_engine(&self, me: Identity) -> R<()> {
        let m = |k: &str| self.meta(k);
        let profile = Profile { kind: "person".into(), org_name: m("profile.name"), username: me.id().to_owned(), blurb: m("profile.about") };
        let engine = Engine::new(self.store.clone(), me, profile, self.host.clone());
        // v2 hubs list each installation as a device: a stable id, a readable name
        let device_id = match self.store.meta("device.id").ok().flatten() {
            Some(id) => id,
            None => {
                let stamp: String = hubchat_core::engine::now().chars().filter(char::is_ascii_digit).take(14).collect();
                let id = format!("hc-{stamp}-{:04x}", rand::random::<u16>());
                self.store.set_meta("device.id", &id).map_err(s)?;
                id
            }
        };
        engine.set_device(&device_id, &self.device_name());
        engine.set_read_receipts(m("settings.read_receipts") != "off");
        engine.start().map_err(s)?;
        *self.engine.lock().unwrap() = Some(engine);
        Ok(())
    }

    /// Whether `cmd` takes an identity onto this device, and which: the
    /// caller saves the key, then calls `adopt`.
    pub fn plan_identity(&self, cmd: &str, args: &Value) -> R<Option<Adoption>> {
        let plan = match cmd {
            "hc_create_identity" => {
                let (id, name): (String, String) = (arg(args, "id")?, arg(args, "name")?);
                Adoption { me: Identity::generate(&id).map_err(s)?, brand_new: true, meta: vec![("profile.name", name.trim().to_owned())], hubs: Vec::new() }
            }
            "hc_restore_words" => {
                let words: String = arg(args, "words")?;
                Adoption { me: recovery::from_words(&words).map_err(s)?, brand_new: false, meta: vec![("recovery.saved", "yes".into())], hubs: Vec::new() }
            }
            // a link's review confirmed, a key QR, a key file
            _ => return self.plan_link(cmd, args),
        };
        if self.engine.lock().unwrap().is_some() {
            return Err("this device already has an identity".into());
        }
        Ok(Some(plan))
    }

    /// Take the identity (its key is saved): local data that belongs to
    /// another identity goes first (user 2026-10-10: a new identity must
    /// never inherit the old one's chats and hubs). Returns its address.
    pub fn adopt(&self, plan: Adoption) -> R<String> {
        if self.engine.lock().unwrap().is_some() {
            return Err("this device already has an identity".into());
        }
        let address = plan.me.address();
        let owner = self.store.meta("identity.address").ok().flatten();
        if must_wipe(owner.as_deref(), self.had_identity(), &address, plan.brand_new) {
            self.store.wipe().map_err(s)?;
        }
        for (k, v) in &plan.meta {
            self.store.set_meta(k, v).map_err(s)?;
        }
        self.start_with(plan.me)?;
        if let Ok(e) = self.engine() {
            for h in &plan.hubs {
                let _ = e.add_hub(h);
            }
        }
        Ok(address)
    }

    /// The lost-key screen's Start over: this device's local data goes (the
    /// caller also clears the browser's key store).
    pub fn start_over(&self) -> R<()> {
        if self.engine.lock().unwrap().is_some() {
            return Err("this device already has an identity".into());
        }
        self.store.wipe().map_err(s)?;
        *self.key_lost.lock().unwrap() = None;
        Ok(())
    }

    /// Settings › Devices › Sign out of this device: off the device lists of
    /// v2 hubs, the engine stopped, local data forgotten (the caller also
    /// clears the browser's key store, first). The welcome screen follows.
    pub async fn sign_out(&self) -> R<()> {
        let e = self.engine.lock().unwrap().take().ok_or_else(|| "no identity yet".to_string())?;
        if let Some(link) = self.link_stop.lock().unwrap().take() {
            link.cancel();
        }
        e.sign_out_this_device().await;
        e.shutdown();
        self.store.wipe().map_err(s)?;
        *self.key_lost.lock().unwrap() = None;
        Ok(())
    }

    fn state(&self) -> Value {
        let engine = self.engine().ok();
        let on = |k: &str| self.meta(k) != "off";
        json!({
            "me": engine.as_ref().map(|e| json!({
                "id": e.me().id(),
                "address": e.me().address(),
                "name": self.meta("profile.name"),
                "about": self.meta("profile.about"),
                "picture": self.store.picture_meta(&e.me().address()).ok().flatten(),
            })),
            "recovery_saved": self.meta("recovery.saved") == "yes",
            "read_receipts": on("settings.read_receipts"),
            "status": hubchat_core::engine::chosen_status(&self.store),
            "language": hubchat_core::engine::chosen_language(&self.store),
            // a browser shows notifications only through its push subscription: they are on
            // when it has one (off until the person turns them on)
            "notifications": { "enabled": engine.as_ref().is_some_and(|e| e.web_push().is_some()), "preview": on("settings.notify_preview"), "sound": false },
            "stay_connected": null,
            "push": null,
            "file_background_hint": null,
            "device_name": self.device_name(),
            "device_maker": "",
            "hubs": engine.map(|e| e.hub_statuses()).unwrap_or_default(),
            "key_lost": self.key_lost.lock().unwrap().clone(),
            "key_restored": false,
            // the layout; the page picks it by its own width (src/web/webApi.ts)
            "platform": "android",
            "shell": "web",
        })
    }

    fn discovery(&self) -> hubchat_core::link::Discovery {
        let store = self.store.clone();
        let engine = self.engine().ok();
        hubchat_core::link::Discovery::new(move |address| hubchat_core::link::start_client(&store, address, || engine.clone()))
    }

    async fn probe_one(&self, input: &str, limit: Duration) -> Probe {
        self.probe_with(input, None, limit, None).await
    }

    /// /healthz at `input` within `limit`. `name`: the hub that must answer. `origin`: the link
    /// this request belongs to (it ends with the link).
    pub(crate) async fn probe_with(&self, input: &str, name: Option<&str>, limit: Duration, origin: Option<hubchat_core::link::Reception>) -> Probe {
        let addr = match HubAddress::parse(input) {
            Ok(a) => a,
            Err(e) => return Probe::Invalid { error: s(e) },
        };
        let url = addr.to_string();
        if !page_can_reach(&url) {
            return Probe::Invalid { error: NEEDS_HTTPS.into() };
        }
        let requests = match origin {
            Some(origin) => self.discovery().with_origin(origin),
            None => self.discovery(),
        };
        let r: Result<hubchat_core::Result<Health>, _> = rt::time::timeout(limit, requests.health(addr)).await;
        match r {
            Err(_) => Probe::Unreachable { url, error: format!("no answer within {} s", limit.as_secs()) },
            Ok(Ok(h)) if name.is_some_and(|n| n != h.name) => Probe::NotAHub { url, error: format!("another hub answers there ({})", h.name) },
            Ok(Ok(h)) => Probe::Connected {
                url,
                max_attachment_bytes: h.max_attachment_bytes(),
                name: h.name,
                features: h.features,
                version: h.version,
                orgs: h.orgs,
                discovered: false,
            },
            Ok(Err(hubchat_core::Error::NotAHub(e))) => Probe::NotAHub { url, error: e },
            Ok(Err(e)) => Probe::Unreachable { url, error: s(e) },
        }
    }

    /// Check an address before adding it. A bare host is tried only where a
    /// page can reach it: over https, and on this machine's own ports.
    async fn probe_hub(&self, input: &str) -> Probe {
        let cands = match HubAddress::candidates(input) {
            Ok(c) => c,
            Err(e) => return Probe::Invalid { error: s(e) },
        };
        let urls: Vec<String> = cands.iter().map(|a| a.to_string()).filter(|u| page_can_reach(u)).collect();
        if urls.is_empty() {
            return Probe::Invalid { error: NEEDS_HTTPS.into() };
        }
        if cands.len() == 1 {
            return self.probe_one(&urls[0], Duration::from_secs(30)).await;
        }
        // a bare host: each reachable address in turn, the likeliest first
        let mut first_failure = None;
        for u in &urls {
            match self.probe_one(u, Duration::from_secs(10)).await {
                Probe::Connected { url, name, max_attachment_bytes, features, version, orgs, .. } => {
                    return Probe::Connected { url, name, max_attachment_bytes, features, version, orgs, discovered: true }
                }
                other => {
                    first_failure.get_or_insert(other);
                }
            }
        }
        first_failure.unwrap_or(Probe::Invalid { error: "no address answered".into() })
    }

    /// One command of the UI: the apps' name, the apps' arguments (as the
    /// page's `invoke` sends them, camelCase), the apps' answer.
    pub async fn call(self: &Arc<Self>, cmd: &str, args: &Value) -> R<Value> {
        let unit = |r: hubchat_core::Result<()>| r.map(|_| Value::Null).map_err(s);
        match cmd {
            // ------------------------------------------------------ state
            "hc_state" => Ok(self.state()),
            "hc_ui_state" => {
                self.foreground.store(arg(args, "foreground")?, Ordering::Relaxed);
                *self.open_chat.lock().unwrap() = arg(args, "chat")?;
                Ok(Value::Null)
            }
            // --------------------------------------------------- identity
            "hc_check_id" => {
                let id: String = arg(args, "id")?;
                let r = hubchat_core::identity::validate_id(&id);
                Ok(json!({ "ok": r.is_ok(), "error": r.err().map(s), "max_len": recovery::MAX_ID_LEN }))
            }
            "hc_create_identity" | "hc_restore_words" => match self.plan_identity(cmd, args)? {
                // a caller without a key store of its own (tests)
                Some(plan) => out(self.adopt(plan)?),
                None => Err(NOT_YET.into()),
            },
            "hc_start_over" => self.start_over().map(|_| Value::Null),
            // a caller without a key store of its own (tests); the page's goes first (wasm.rs)
            "hc_sign_out" => self.sign_out().await.map(|_| Value::Null),
            "hc_retry_key" | "hc_key_restored_seen" => Ok(Value::Null),
            "hc_recovery_words" => out(recovery::to_words(self.engine()?.me()).map_err(s)?),
            // The apps write the recovery file themselves (`hc_save_recovery`); a
            // page cannot, so it gets the same text and hands it to the browser
            // (src/web/webApi.ts), then says `hc_recovery_saved`.
            "hc_recovery_file" => {
                let e = self.engine()?;
                let text = recovery::to_file(e.me(), &hubchat_core::engine::now()).map_err(s)?;
                Ok(json!({ "name": format!("hubchat-recovery-{}.txt", e.me().id()), "text": text }))
            }
            "hc_recovery_saved" => unit(self.store.set_meta("recovery.saved", "yes")),
            "hc_set_profile" => {
                let (name, about): (String, String) = (arg(args, "name")?, arg(args, "about")?);
                self.store.set_meta("profile.name", name.trim()).map_err(s)?;
                self.store.set_meta("profile.about", about.trim()).map_err(s)?;
                unit(self.engine()?.set_profile(name.trim(), about.trim()).await)
            }
            "hc_picture" => {
                let peer: String = arg(args, "peer")?;
                out(self.engine()?.picture(&peer).await.map_err(s)?)
            }
            "hc_set_notifications" => {
                let v = |on: bool| if on { "on" } else { "off" };
                for (k, name) in [("settings.notify", "enabled"), ("settings.notify_preview", "preview"), ("settings.notify_sound", "sound")] {
                    self.store.set_meta(k, v(arg(args, name)?)).map_err(s)?;
                }
                // "Show message text" goes with the subscription: every hub is told
                if let Ok(e) = self.engine() {
                    if let Some(mut subscription) = e.web_push() {
                        let preview: bool = arg(args, "preview")?;
                        if subscription.preview != preview {
                            subscription.preview = preview;
                            e.set_web_push(Some(&subscription)).map_err(s)?;
                        }
                    }
                }
                Ok(Value::Null)
            }
            // The hubs this page's server suggests (its /config.json, read by the page). A link
            // whose hub has no address a page can call is tried through them (link.rs). Needs no
            // identity: a new browser is linked before it has one.
            "hc_web_suggested_hubs" => {
                let addresses: Vec<String> = arg(args, "addresses")?;
                *self.suggested.lock().unwrap() = addresses.into_iter().take(16).collect();
                Ok(Value::Null)
            }
            // ---------------------------------------- notifications (engine_webpush)
            // What this browser's notifications are: where they go (the page compares it with
            // the browser's own subscription), and what each hub has of it.
            "hc_web_push" => {
                let e = self.engine()?;
                let s = e.web_push();
                Ok(json!({
                    "endpoint": s.as_ref().map(|s| s.endpoint.clone()),
                    "preview": s.as_ref().is_none_or(|s| s.preview),
                    "hubs": e.web_push_hubs().map_err(|e| e.to_string())?,
                    "resubscribe": e.web_push_must_resubscribe(),
                }))
            }
            // The page turned notifications on (its subscription) or off (null).
            "hc_web_push_set" => {
                let subscription: Option<hubchat_core::engine::WebPush> = arg(args, "subscription")?;
                unit(self.engine()?.set_web_push(subscription.as_ref()))
            }
            // Off, and every hub told now (before starting over, when nothing is left to tell them later).
            "hc_web_push_off" => {
                let e = self.engine()?;
                e.set_web_push(None).map_err(s)?;
                let _ = e.unregister_push().await;
                Ok(Value::Null)
            }
            "hc_set_active" => {
                if let Ok(e) = self.engine() {
                    e.set_active(arg(args, "on")?);
                }
                Ok(Value::Null)
            }
            "hc_composing" => {
                if let Ok(e) = self.engine() {
                    e.composing(&arg::<String>(args, "peer")?, arg(args, "on")?);
                }
                Ok(Value::Null)
            }
            "hc_set_device_name" => {
                let name: String = arg(args, "name")?;
                let name = name.trim();
                if name.chars().count() > 64 {
                    return Err("a device name has at most 64 characters".into());
                }
                self.store.set_meta("device.name", name).map_err(s)?;
                let now = self.device_name();
                if let Ok(e) = self.engine() {
                    let (id, _) = e.device();
                    e.set_device(&id, &now);
                }
                out(now)
            }
            "hc_set_status" => unit(self.engine()?.set_status(&arg::<String>(args, "status")?)),
            "hc_set_blocked" => unit(self.engine()?.set_blocked(&arg::<String>(args, "address")?, arg::<bool>(args, "on")?)),
            "hc_blocked" => out(self.engine()?.blocked().map_err(s)?),
            "hc_own_lockdown" => out(self.engine()?.own_lockdown().map_err(s)?),
            "hc_set_language" => unit(self.engine()?.set_language(arg::<String>(args, "language")?.trim())),
            "hc_set_read_receipts" => {
                let on: bool = arg(args, "on")?;
                self.store.set_meta("settings.read_receipts", if on { "on" } else { "off" }).map_err(s)?;
                if let Ok(e) = self.engine() {
                    e.set_read_receipts(on);
                }
                Ok(Value::Null)
            }
            // ------------------------------------------------------- hubs
            "hc_probe_hub" => out(self.probe_hub(&arg::<String>(args, "input")?).await),
            "hc_add_hub" => {
                let input: String = arg(args, "input")?;
                let url = HubAddress::parse(&input).map_err(s)?.to_string();
                if !page_can_reach(&url) {
                    return Err(NEEDS_HTTPS.into());
                }
                out(self.engine()?.add_hub(&url).map(|a| a.to_string()).map_err(s)?)
            }
            "hc_set_hub_enabled" => unit(self.engine()?.set_hub_enabled(&arg::<String>(args, "url")?, arg(args, "enabled")?)),
            "hc_remove_hub" => unit(self.engine()?.remove_hub(&arg::<String>(args, "url")?, arg(args, "unregister")?).await),
            "hc_retry_now" => {
                self.engine()?.retry_now();
                Ok(Value::Null)
            }
            // -------------------------------------------- directory, chats
            "hc_directory" => out(self.store.directory().map_err(s)?),
            "hc_resolve" => {
                let input: String = arg(args, "input")?;
                let me = self.engine().map(|e| e.me().address()).unwrap_or_default();
                let raw = input.trim().trim_start_matches("@net:").to_lowercase();
                let valid = !raw.is_empty() && raw.chars().all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || matches!(ch, '.' | '_' | '-'));
                let dir = self.store.directory().map_err(s)?;
                let exact = dir.iter().find(|d| d.address == raw).cloned();
                let prefix = format!("{raw}.");
                let matches: Vec<_> = if exact.is_none() && valid { dir.into_iter().filter(|d| d.address.starts_with(&prefix)).collect() } else { Vec::new() };
                Ok(json!({ "exact": exact, "matches": matches, "is_me": raw == me, "valid": valid, "address": raw }))
            }
            "hc_chats" => out(self.store.chats().map_err(s)?),
            "hc_close_dm" => unit(self.engine()?.close_dm(&arg::<String>(args, "peer")?)),
            "hc_open_dm" => unit(self.engine()?.open_dm(&arg::<String>(args, "peer")?)),
            "hc_chat" => {
                let peer: String = arg(args, "peer")?;
                let pair = |a: &str, b: &str| -> R<Option<(String, String)>> {
                    Ok(match (arg::<Option<String>>(args, a)?, arg::<Option<String>>(args, b)?) {
                        (Some(a), Some(b)) => Some((a, b)),
                        _ => None,
                    })
                };
                let (before, from) = (pair("beforeAt", "beforeId")?, pair("fromAt", "fromId")?);
                let (before, from) = (before.as_ref().map(|(a, b)| (a.as_str(), b.as_str())), from.as_ref().map(|(a, b)| (a.as_str(), b.as_str())));
                let limit = arg::<Option<u32>>(args, "limit")?.unwrap_or(50).min(5000);
                // the engine leaves out what older pages still have to fill in
                out(match self.engine() {
                    Ok(e) => e.chat(&peer, before, from, limit),
                    Err(_) => self.store.chat(&peer, before, from, limit),
                }
                .map_err(s)?)
            }
            "hc_load_older" => out(self.engine()?.load_older(&arg::<String>(args, "peer")?).await.map_err(s)?),
            "hc_message" => out(self.store.message(&arg::<String>(args, "id")?).map_err(s)?),
            "hc_send" => {
                let SendRequest { msg, replies, answers } = arg(args, "msg")?;
                unit(self.engine()?.send_with(msg, ReplyMeta { replies, answers }).await)
            }
            "hc_draft_answers" => out(self.store.draft_answers(&arg::<String>(args, "peer")?).map_err(s)?),
            "hc_set_draft_answers" => unit(self.store.set_draft_answers(&arg::<String>(args, "peer")?, &arg(args, "answers")?)),
            "hc_card_answers" => out(self.store.card_answers(&arg::<String>(args, "peer")?, &arg::<Vec<String>>(args, "ids")?).map_err(s)?),
            "hc_retry" => unit(self.engine()?.retry(&arg::<String>(args, "id")?)),
            // ------------------------------------------------------ files (files.rs)
            "hc_file_info" => self.file_info(&arg::<String>(args, "source")?).await,
            "hc_download" => unit(
                self.engine()?
                    .queue_download_copy(&arg::<String>(args, "messageId")?, &arg::<String>(args, "localId")?, arg::<Option<bool>>(args, "freshCopy")?.unwrap_or(false))
                    .map(|_| ()),
            ),
            "hc_cancel_transfer" => unit(self.engine()?.cancel_transfer(&arg::<String>(args, "localId")?)),
            "hc_remove_attachment" => unit(self.engine()?.remove_attachment(&arg::<String>(args, "messageId")?, &arg::<String>(args, "localId")?).await),
            "hc_transfers" => out(self.store.transfer_page(arg::<Option<String>>(args, "after")?.as_deref().unwrap_or("")).map_err(s)?),
            "hc_transfer_history" => {
                out(self.engine()?.transfer_history(arg::<Option<String>>(args, "after")?.as_deref().unwrap_or("")).map_err(s)?)
            }
            "hc_mark_read" => unit(self.engine()?.mark_read(&arg::<String>(args, "peer")?).await),
            "hc_send_route" => out(self.engine()?.send_route(&arg::<String>(args, "peer")?).map_err(s)?),
            "hc_set_send_hub" => unit(self.engine()?.set_send_hub(&arg::<String>(args, "peer")?, arg::<Option<String>>(args, "hub")?.as_deref())),
            "hc_delete_message" => unit(self.engine()?.delete_message(&arg::<String>(args, "id")?).await),
            "hc_delete_chat" => unit(self.engine()?.delete_chat(&arg::<String>(args, "peer")?).await),
            "hc_devices" => {
                let e = self.engine()?;
                Ok(json!({ "this_device": e.device().0, "devices": e.devices().await }))
            }
            "hc_draft" => out(self.store.draft(&arg::<String>(args, "peer")?).map_err(s)?),
            "hc_set_draft" => unit(self.store.set_draft(&arg::<String>(args, "peer")?, &arg::<String>(args, "body")?)),
            "hc_setup_status" => out(self.engine()?.setup_state(&arg::<String>(args, "org")?)),
            // ------------------- what belongs to an installed app: nothing here
            "hc_take_pending_chat" | "hc_take_pending_link" | "hc_app_update_check" | "hc_vpn_active" | "hc_app_installed" | "hc_clipboard_text" => Ok(Value::Null),
            "hc_tray_updates" => Ok(Value::Null),
            "hc_exit_on_close" | "hc_set_exit_on_close" | "hc_set_autostart" | "hc_orgtree_links" | "hc_open_app" => Ok(json!(false)),
            "hc_local_orgtree_orgs" => Ok(json!([])),
            "hc_app_update_state" => Ok(json!("")),
            "hc_log_info" | "hc_set_verbose_logging" => Ok(json!({
                "active": false, "verbose": false, "default_verbose": false, "pinned": false, "dir": "", "fallback": false,
                "files": 0, "bytes": 0, "max_bytes": 0, "retention_days": 0,
            })),
            // linking, the key QR and the key file (link.rs)
            _ => match self.link_call(cmd, args).await {
                Some(answer) => answer,
                None => Err(NOT_YET.into()),
            },
        }
    }

    /// Stop the engine (the page is going away).
    pub fn shutdown(&self) {
        if let Some(link) = self.link_stop.lock().unwrap().take() {
            link.cancel();
        }
        if let Some(e) = self.engine.lock().unwrap().take() {
            e.shutdown();
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    fn shell(store: Arc<Store>, identity: Option<Identity>) -> (Arc<Shell>, Arc<Mutex<Vec<(String, String)>>>) {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let sink_seen = seen.clone();
        let sink: Sink = Arc::new(move |name, json| sink_seen.lock().unwrap().push((name.to_owned(), json.to_owned())));
        (Shell::open(store, sink, "Test browser", identity).unwrap(), seen)
    }

    pub(crate) fn fresh() -> (Arc<Shell>, Arc<Mutex<Vec<(String, String)>>>) {
        shell(Arc::new(Store::open_in_memory().unwrap()), None)
    }

    /// A shell whose files are in `root` (files.rs's tests).
    pub(crate) fn fresh_in(root: PathBuf) -> (Arc<Shell>, Arc<Mutex<Vec<(String, String)>>>) {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let sink_seen = seen.clone();
        let sink: Sink = Arc::new(move |name, json| sink_seen.lock().unwrap().push((name.to_owned(), json.to_owned())));
        (Shell::open_at(Arc::new(Store::open_in_memory().unwrap()), sink, "Test browser", None, root).unwrap(), seen)
    }

    pub(crate) async fn take(shell: &Arc<Shell>, cmd: &str, args: Value) -> String {
        let plan = shell.plan_identity(cmd, &args).unwrap().expect("an identity command");
        shell.adopt(plan).unwrap()
    }

    #[tokio::test]
    async fn a_new_device_makes_an_identity_and_keeps_it() {
        let (sh, _) = fresh();
        let st = sh.call("hc_state", &json!({})).await.unwrap();
        assert_eq!(st["me"], Value::Null);
        assert_eq!(st["key_lost"], Value::Null);
        assert_eq!(st["shell"], "web");
        assert_eq!(st["platform"], "android");
        assert_eq!(st["hubs"], json!([]));
        // the id is checked as the apps check it
        assert_eq!(sh.call("hc_check_id", &json!({ "id": "Maya" })).await.unwrap()["ok"], false);
        let ok = sh.call("hc_check_id", &json!({ "id": "maya" })).await.unwrap();
        assert_eq!((ok["ok"].clone(), ok["max_len"].clone()), (json!(true), json!(24)));
        // nothing that needs an identity works before there is one
        assert_eq!(sh.call("hc_recovery_words", &json!({})).await.unwrap_err(), "no identity yet");

        let address = take(&sh, "hc_create_identity", json!({ "id": "maya", "name": "  Maya Lin " })).await;
        assert!(address.starts_with("maya."), "{address}");
        let st = sh.call("hc_state", &json!({})).await.unwrap();
        assert_eq!(st["me"]["address"], json!(address));
        assert_eq!(st["me"]["id"], "maya");
        assert_eq!(st["me"]["name"], "Maya Lin");
        assert_eq!(st["recovery_saved"], false);
        assert_eq!(st["device_name"], "Test browser");
        assert_eq!(sh.store().meta("identity.address").unwrap().as_deref(), Some(address.as_str()));
        assert!(sh.store().meta("device.id").unwrap().is_some_and(|d| d.starts_with("hc-")), "a device id is made at the first start");
        // one identity per device
        assert!(sh.plan_identity("hc_create_identity", &json!({ "id": "other", "name": "" })).is_err());

        let words: Vec<String> = serde_json::from_value(sh.call("hc_recovery_words", &json!({})).await.unwrap()).unwrap();
        assert_eq!(words.len(), 24);
        // the recovery file is the apps' text; the browser saves it, so reading it marks nothing saved
        let file = sh.call("hc_recovery_file", &json!({})).await.unwrap();
        assert_eq!(file["name"], "hubchat-recovery-maya.txt");
        let text = file["text"].as_str().unwrap();
        assert!(text.starts_with("Hubchat recovery words\r\n") && text.contains(&format!("Address: @net:{address}\r\n")), "{text}");
        assert!(text.contains(&format!(" 1. {}\r\n", words[0])) && text.contains(&format!("24. {}\r\n", words[23])), "{text}");
        assert_eq!(sh.call("hc_state", &json!({})).await.unwrap()["recovery_saved"], false);
        sh.call("hc_recovery_saved", &json!({})).await.unwrap();
        assert_eq!(sh.call("hc_state", &json!({})).await.unwrap()["recovery_saved"], true);

        // the same words on another device are the same address
        let (other, _) = fresh();
        let again = take(&other, "hc_restore_words", json!({ "words": words.join(" ") })).await;
        assert_eq!(again, address);
        assert_eq!(other.call("hc_state", &json!({})).await.unwrap()["recovery_saved"], true);
        sh.shutdown();
        other.shutdown();
    }

    #[tokio::test]
    async fn a_store_whose_key_is_gone_shows_the_lost_key_screen() {
        let store = Arc::new(Store::open_in_memory().unwrap());
        let (sh, _) = shell(store.clone(), None);
        let address = take(&sh, "hc_create_identity", json!({ "id": "pat", "name": "Pat" })).await;
        let me = sh.engine().unwrap().me().clone();
        sh.shutdown();
        // the browser's key store was cleared, its database was not
        let (lost, _) = shell(store.clone(), None);
        let st = lost.call("hc_state", &json!({})).await.unwrap();
        assert_eq!(st["me"], Value::Null);
        assert_eq!(st["key_lost"], json!({ "address": address, "unreadable": false }));
        // with the key, it is the same identity again, nothing lost
        let (back, _) = shell(store.clone(), Some(me));
        assert_eq!(back.call("hc_state", &json!({})).await.unwrap()["me"]["address"], json!(address));
        back.shutdown();
        // Start over forgets the old data, and a new identity inherits nothing
        let (lost, _) = shell(store, None);
        lost.call("hc_set_draft", &json!({ "peer": "ann.111111", "body": "left over" })).await.unwrap();
        lost.call("hc_start_over", &json!({})).await.unwrap();
        assert_eq!(lost.call("hc_state", &json!({})).await.unwrap()["key_lost"], Value::Null);
        assert_eq!(lost.call("hc_draft", &json!({ "peer": "ann.111111" })).await.unwrap(), Value::Null);
    }

    #[tokio::test]
    async fn signing_out_leaves_this_device_empty_and_on_the_welcome_screen() {
        let store = Arc::new(Store::open_in_memory().unwrap());
        let (sh, _) = shell(store.clone(), None);
        take(&sh, "hc_create_identity", json!({ "id": "pat", "name": "Pat" })).await;
        sh.call("hc_set_draft", &json!({ "peer": "ann.111111", "body": "half a thought" })).await.unwrap();
        sh.call("hc_set_notifications", &json!({ "enabled": true, "preview": false, "sound": false })).await.unwrap();
        sh.call("hc_sign_out", &json!({})).await.unwrap();
        // the welcome screen, not the lost-key screen: nothing of the identity is left to find
        let st = sh.call("hc_state", &json!({})).await.unwrap();
        assert_eq!((st["me"].clone(), st["key_lost"].clone()), (Value::Null, Value::Null), "{st}");
        assert_eq!(sh.call("hc_recovery_words", &json!({})).await.unwrap_err(), "no identity yet");
        assert_eq!(store.meta("identity.address").unwrap(), None);
        assert_eq!(store.meta("device.id").unwrap(), None);
        assert_eq!(store.draft("ann.111111").unwrap(), None);
        // a page opened on this store later starts at the welcome screen too
        let (again, _) = shell(store, None);
        assert_eq!(again.call("hc_state", &json!({})).await.unwrap()["key_lost"], Value::Null);
        // signing out twice has nothing to sign out of
        assert_eq!(sh.call("hc_sign_out", &json!({})).await.unwrap_err(), "no identity yet");
    }

    #[tokio::test]
    async fn commands_take_the_apps_names_and_arguments() {
        let (sh, _) = fresh();
        take(&sh, "hc_create_identity", json!({ "id": "maya", "name": "Maya" })).await;
        // camelCase, as the page's invoke() sends them
        let page = sh.call("hc_chat", &json!({ "peer": "ann.111111", "beforeAt": null, "beforeId": null, "fromAt": null, "fromId": null, "limit": null })).await.unwrap();
        assert_eq!(page, json!([]));
        assert_eq!(sh.call("hc_chats", &json!({})).await.unwrap(), json!([]));
        assert_eq!(sh.call("hc_directory", &json!({})).await.unwrap(), json!([]));
        assert_eq!(sh.call("hc_transfers", &json!({ "after": null })).await.unwrap()["rows"], json!([]));
        sh.call("hc_set_draft", &json!({ "peer": "ann.111111", "body": "half a thought" })).await.unwrap();
        assert_eq!(sh.call("hc_draft", &json!({ "peer": "ann.111111" })).await.unwrap(), "half a thought");
        let r = sh.call("hc_resolve", &json!({ "input": "@net:Ann.111111" })).await.unwrap();
        assert_eq!((r["address"].clone(), r["valid"].clone(), r["is_me"].clone()), (json!("ann.111111"), json!(true), json!(false)));
        sh.call("hc_set_notifications", &json!({ "enabled": true, "preview": false, "sound": false })).await.unwrap();
        let st = sh.call("hc_state", &json!({})).await.unwrap();
        // on the web notifications are on when the browser has a push subscription
        assert_eq!(st["notifications"], json!({ "enabled": false, "preview": false, "sound": false }));
        let subscription = json!({ "endpoint": "https://push.example/x", "p256dh": "p", "auth": "a", "vapid": "v", "navigate": "https://chat.example/", "preview": false });
        sh.call("hc_web_push_set", &json!({ "subscription": subscription })).await.unwrap();
        assert_eq!(sh.call("hc_state", &json!({})).await.unwrap()["notifications"]["enabled"], true);
        assert_eq!(
            sh.call("hc_web_push", &json!({})).await.unwrap(),
            json!({ "endpoint": "https://push.example/x", "preview": false, "hubs": [], "resubscribe": false })
        );
        // "Show message text" changes what the hubs are handed
        sh.call("hc_set_notifications", &json!({ "enabled": true, "preview": true, "sound": false })).await.unwrap();
        assert_eq!(sh.call("hc_web_push", &json!({})).await.unwrap()["preview"], true);
        sh.call("hc_web_push_off", &json!({})).await.unwrap();
        assert_eq!(sh.call("hc_state", &json!({})).await.unwrap()["notifications"]["enabled"], false);
        assert_eq!(sh.call("hc_web_push", &json!({})).await.unwrap()["endpoint"], Value::Null);
        assert!(sh.call("hc_web_push_set", &json!({ "subscription": { "endpoint": 7 } })).await.is_err());
        assert_eq!(sh.call("hc_set_device_name", &json!({ "name": " Kitchen tablet " })).await.unwrap(), "Kitchen tablet");
        assert_eq!(sh.call("hc_set_device_name", &json!({ "name": "" })).await.unwrap(), "Test browser");
        sh.call("hc_ui_state", &json!({ "foreground": true, "chat": null })).await.unwrap();
        // a missing argument is said, not guessed
        assert!(sh.call("hc_set_draft", &json!({ "peer": "ann.111111" })).await.unwrap_err().contains("body"));
        // what a browser can't do yet says so
        assert_eq!(sh.call("hc_key_file_export", &json!({ "passphrase": "x", "dest": "y" })).await.unwrap_err(), NOT_YET);
        assert_eq!(sh.call("hc_no_such_command", &json!({})).await.unwrap_err(), NOT_YET);
        sh.shutdown();
    }

    #[tokio::test]
    async fn a_message_is_kept_and_queued_until_a_hub_takes_it() {
        let (sh, seen) = fresh();
        take(&sh, "hc_create_identity", json!({ "id": "maya", "name": "Maya" })).await;
        sh.call("hc_send", &json!({ "msg": { "id": "m1", "peer": "ann.111111", "body": "hello Ann", "kind": null, "reply_to": null } })).await.unwrap();
        let page = sh.call("hc_chat", &json!({ "peer": "ann.111111", "limit": 50 })).await.unwrap();
        assert_eq!(page.as_array().unwrap().len(), 1, "{page}");
        assert_eq!(page[0]["body"], "hello Ann");
        assert_eq!(page[0]["outgoing"], true);
        assert!(matches!(page[0]["state"].as_str(), Some("queued" | "sending")), "{}", page[0]["state"]);
        let chats = sh.call("hc_chats", &json!({})).await.unwrap();
        assert_eq!(chats[0]["peer"], "ann.111111");
        // the UI was told through the same `hc` events the apps emit
        assert!(seen.lock().unwrap().iter().any(|(name, json)| name == "hc" && json.contains(r#""type":"chat""#) && json.contains("ann.111111")), "{:?}", seen.lock().unwrap());
        sh.shutdown();
    }

    #[tokio::test]
    async fn only_addresses_a_page_can_reach_are_hubs() {
        for (url, ok) in [
            ("https://pc.example.ts.net", true),
            ("https://pc.example.ts.net:8443/hub", true),
            ("http://localhost:7371", true),
            ("http://127.0.0.1:7378", true),
            ("http://[::1]:7371", true),
            ("http://10.0.4.2:7371", false),
            ("http://pc.example.ts.net:7371", false),
            ("http://localhost.evil.example:7371", false),
            ("http://127.0.0.1.evil.example", false),
        ] {
            assert_eq!(page_can_reach(url), ok, "{url}");
        }
        let (sh, _) = fresh();
        take(&sh, "hc_create_identity", json!({ "id": "maya", "name": "Maya" })).await;
        // refused before any request is made
        let p = sh.call("hc_probe_hub", &json!({ "input": "http://10.0.4.2:7371" })).await.unwrap();
        assert_eq!(p, json!({ "result": "invalid", "error": NEEDS_HTTPS }));
        assert_eq!(sh.call("hc_add_hub", &json!({ "input": "http://10.0.4.2:7371" })).await.unwrap_err(), NEEDS_HTTPS);
        assert_eq!(sh.call("hc_add_hub", &json!({ "input": "pc.example.ts.net:7371" })).await.unwrap_err(), NEEDS_HTTPS);
        assert_eq!(sh.call("hc_state", &json!({})).await.unwrap()["hubs"], json!([]));
        // an https hub is taken (it connects when it can)
        assert_eq!(sh.call("hc_add_hub", &json!({ "input": "https://hub.invalid" })).await.unwrap(), "https://hub.invalid");
        let hubs = sh.call("hc_state", &json!({})).await.unwrap()["hubs"].clone();
        assert_eq!(hubs[0]["url"], "https://hub.invalid");
        sh.shutdown();
    }
}
