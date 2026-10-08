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
    fn notify(&self, title: &str, body: &str, peer: &str);
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
}

impl Host for ShellHost {
    fn event(&self, ev: Event) {
        if let Event::Incoming { peer, preview, .. } = &ev {
            let showing = self.foreground.load(Ordering::Relaxed)
                && self.open_chat.lock().unwrap().as_deref() == Some(peer.as_str());
            if !showing {
                self.platform.notify(peer, preview, peer);
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
    let host = Arc::new(ShellHost {
        app: OnceLock::new(),
        platform,
        foreground: AtomicBool::new(false),
        open_chat: Mutex::new(None),
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
