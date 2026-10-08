//! The app's single core per process: store, identity, engine, and the bridge
//! from engine events to the UI and to notifications. On Android the
//! ConnectionService creates it (it outlives the activity); on desktop the
//! Tauri setup does. Commands reach it through `get()`.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use hubchat_core::engine::{Engine, Event, Host};
use hubchat_core::hub::Profile;
use hubchat_core::store::Store;
use hubchat_core::Identity;
use tauri::{AppHandle, Emitter};

/// What only the platform can do.
pub trait Platform: Send + Sync + 'static {
    /// Show a message notification (title = sender, body = preview).
    /// `sound`: play the notification sound (Settings › Notifications).
    fn notify(&self, title: &str, body: &str, peer: &str, sound: bool);
    /// Update the ongoing "connected" line (Android) / tray tooltip (desktop).
    fn status(&self, text: &str);
    fn open_source(&self, source: &str) -> std::io::Result<std::fs::File>;
    /// The file name to show for a picked source (Android: the provider's
    /// display name; elsewhere the last path component).
    fn source_name(&self, source: &str) -> Option<String> {
        std::path::Path::new(source)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
    }
    fn download_dir(&self) -> PathBuf;
    /// After a download: move it where the user finds downloads (Android:
    /// the shared Downloads collection). Returns the new location, if moved.
    fn publish_download(&self, _path: &std::path::Path, _name: &str) -> Option<String> {
        None
    }
    /// A hubchat:// link the system opened the app with (taken once).
    fn take_pending_link(&self) -> Option<String> {
        None
    }
    /// A chat a tapped notification asked to open (taken once).
    fn take_pending_chat(&self) -> Option<String> {
        None
    }
    /// Android (design D6): stay connected with the ongoing notification
    /// (true, the default) or check about every 15 minutes (false). None
    /// where there is no such choice.
    fn stay_connected(&self) -> Option<bool> {
        None
    }
    fn set_stay_connected(&self, _on: bool) -> Result<(), String> {
        Err("only Android has this setting".into())
    }
}

pub struct Core {
    pub dir: PathBuf,
    pub store: Arc<Store>,
    engine: Mutex<Option<Arc<Engine>>>,
    pub rt: tokio::runtime::Handle,
    host: Arc<ShellHost>,
}

struct ShellHost {
    app: OnceLock<AppHandle>,
    platform: Arc<dyn Platform>,
    /// The UI is on screen and focused.
    foreground: AtomicBool,
    /// The chat the UI shows, if any.
    open_chat: Mutex<Option<String>>,
    store: Arc<Store>,
    /// Settings › Notifications (design): notify at all and show the
    /// message text (on until turned off), play a sound (desktop; off until
    /// turned on, as the design's prototype has it).
    notify_on: AtomicBool,
    notify_text: AtomicBool,
    notify_sound: AtomicBool,
}

/// What Settings › Notifications holds.
#[derive(serde::Serialize, Clone, Copy)]
pub struct NotifySettings {
    pub enabled: bool,
    pub preview: bool,
    pub sound: bool,
}

impl Host for ShellHost {
    fn event(&self, ev: Event) {
        if let Event::Incoming { peer, preview, .. } = &ev {
            let showing = self.foreground.load(Ordering::Relaxed)
                && self.open_chat.lock().unwrap().as_deref() == Some(peer.as_str());
            if !showing && self.notify_on.load(Ordering::Relaxed) {
                let title = self
                    .store
                    .display_name(peer)
                    .ok()
                    .flatten()
                    .unwrap_or_else(|| peer.clone());
                let body = if self.notify_text.load(Ordering::Relaxed) {
                    preview.clone()
                } else {
                    "New message".to_string()
                };
                self.platform
                    .notify(&title, &body, peer, self.notify_sound.load(Ordering::Relaxed));
            }
        }
        if let Some(app) = self.app.get() {
            let _ = app.emit("hc", &ev);
        }
    }
    fn open_source(&self, source: &str) -> std::io::Result<std::fs::File> {
        self.platform.open_source(source)
    }
    fn download_dir(&self) -> PathBuf {
        self.platform.download_dir()
    }
}

static CORE: OnceLock<Core> = OnceLock::new();

pub fn get() -> Result<&'static Core, String> {
    CORE.get()
        .ok_or_else(|| "Hubchat is still starting".to_string())
}

/// Create the core once per process (later calls return the existing one).
pub fn init(dir: PathBuf, platform: Arc<dyn Platform>) -> Result<&'static Core, String> {
    if let Some(c) = CORE.get() {
        return Ok(c);
    }
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let store = Arc::new(Store::open(&dir.join("hubchat.sqlite3")).map_err(|e| e.to_string())?);
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::Builder::new()
        .name("hubchat-core".into())
        .spawn(move || {
            let rt = tokio::runtime::Builder::new_multi_thread()
                .worker_threads(2)
                .enable_all()
                .build()
                .expect("runtime");
            let _ = tx.send(rt.handle().clone());
            rt.block_on(std::future::pending::<()>());
        })
        .map_err(|e| e.to_string())?;
    let rt = rx.recv().map_err(|e| e.to_string())?;
    let on = |k: &str| store.meta(k).ok().flatten().as_deref() != Some("off");
    let host = Arc::new(ShellHost {
        app: OnceLock::new(),
        platform,
        foreground: AtomicBool::new(false),
        open_chat: Mutex::new(None),
        store: store.clone(),
        notify_on: AtomicBool::new(on("settings.notify")),
        notify_text: AtomicBool::new(on("settings.notify_preview")),
        notify_sound: AtomicBool::new(store.meta("settings.notify_sound").ok().flatten().as_deref() == Some("on")),
    });
    let core = Core {
        dir,
        store,
        engine: Mutex::new(None),
        rt,
        host,
    };
    let core = match CORE.set(core) {
        Ok(()) => CORE.get().unwrap(),
        Err(_) => return Ok(CORE.get().unwrap()), // lost a race: use the winner
    };
    if let Some(me) = crate::secrets::load_identity(&core.dir) {
        core.start_engine(me)?;
    } else {
        core.host.platform.status("Set up Hubchat to connect");
    }
    Ok(core)
}

impl Core {
    pub fn platform(&self) -> &dyn Platform {
        &*self.host.platform
    }

    /// Called by the Tauri setup so events reach the UI.
    pub fn attach_ui(&self, app: AppHandle) {
        let _ = self.host.app.set(app);
    }

    pub fn notify_settings(&self) -> NotifySettings {
        NotifySettings {
            enabled: self.host.notify_on.load(Ordering::Relaxed),
            preview: self.host.notify_text.load(Ordering::Relaxed),
            sound: self.host.notify_sound.load(Ordering::Relaxed),
        }
    }

    pub fn set_notify_settings(&self, n: NotifySettings) -> Result<(), String> {
        let v = |on: bool| if on { "on" } else { "off" };
        for (k, on) in [
            ("settings.notify", n.enabled),
            ("settings.notify_preview", n.preview),
            ("settings.notify_sound", n.sound),
        ] {
            self.store.set_meta(k, v(on)).map_err(|e| e.to_string())?;
        }
        self.host.notify_on.store(n.enabled, Ordering::Relaxed);
        self.host.notify_text.store(n.preview, Ordering::Relaxed);
        self.host.notify_sound.store(n.sound, Ordering::Relaxed);
        Ok(())
    }

    pub fn set_ui_state(&self, foreground: bool, chat: Option<String>) {
        self.host.foreground.store(foreground, Ordering::Relaxed);
        *self.host.open_chat.lock().unwrap() = chat;
    }

    pub fn engine(&self) -> Result<Arc<Engine>, String> {
        self.engine
            .lock()
            .unwrap()
            .clone()
            .ok_or_else(|| "no identity yet".to_string())
    }

    pub fn has_identity(&self) -> bool {
        self.engine.lock().unwrap().is_some()
    }

    fn profile_from_store(&self, me: &Identity) -> Profile {
        let m = |k: &str| self.store.meta(k).ok().flatten().unwrap_or_default();
        Profile {
            kind: "person".into(),
            org_name: m("profile.name"),
            username: me.id().to_owned(),
            blurb: m("profile.about"),
        }
    }

    /// Save a new or restored identity and start talking to the hubs.
    /// Leave the current identity on this device (switching to another):
    /// off the device lists of v2 hubs, connections stopped, key and local
    /// data forgotten. The address itself stays registered for its other
    /// devices.
    pub async fn leave_identity(&self) -> Result<(), String> {
        let Some(e) = self.engine.lock().unwrap().take() else {
            return Ok(());
        };
        let e2 = e.clone();
        let _ = self
            .rt
            .spawn(async move { e2.sign_out_this_device().await })
            .await;
        e.shutdown();
        crate::secrets::forget_identity(&self.dir)?;
        self.store.wipe().map_err(|e| e.to_string())?;
        self.host.platform.status("Switching identity");
        Ok(())
    }

    pub fn adopt_identity(&self, me: Identity) -> Result<(), String> {
        if self.has_identity() {
            return Err("this device already has an identity".into());
        }
        crate::secrets::save_identity(&self.dir, &me)?;
        self.start_engine(me)
    }

    fn start_engine(&self, me: Identity) -> Result<(), String> {
        let profile = self.profile_from_store(&me);
        let engine = Engine::new(self.store.clone(), me, profile, self.host.clone());
        // v2 hubs list each installation as a device: a stable id, a readable name.
        let device_id = match self.store.meta("device.id").ok().flatten() {
            Some(id) => id,
            None => {
                let id = format!(
                    "hc-{}",
                    &hubchat_core::engine::now().replace(|c: char| !c.is_ascii_digit(), "")[..14]
                );
                let id = format!("{id}-{:04x}", rand_u16());
                self.store
                    .set_meta("device.id", &id)
                    .map_err(|e| e.to_string())?;
                id
            }
        };
        engine.set_device(&device_id, &device_name());
        let read = self.store.meta("settings.read_receipts").ok().flatten();
        engine.set_read_receipts(read.as_deref() != Some("off"));
        {
            // start() spawns its tasks onto the core runtime.
            let _guard = self.rt.enter();
            engine.start().map_err(|e| e.to_string())?;
        }
        self.host
            .platform
            .status(&format!("Connected as {}", engine.me().address()));
        *self.engine.lock().unwrap() = Some(engine);
        Ok(())
    }
}

fn rand_u16() -> u16 {
    use std::hash::{BuildHasher, Hasher};
    let mut h = std::collections::hash_map::RandomState::new().build_hasher();
    h.write_u128(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0),
    );
    h.finish() as u16
}

/// What other devices see this one called (Settings › Devices).
fn device_name() -> String {
    if cfg!(target_os = "android") {
        "Android phone".into()
    } else {
        std::env::var("COMPUTERNAME")
            .map(|n| format!("PC {n}"))
            .unwrap_or_else(|_| "Windows PC".into())
    }
}
