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
use serde::Serialize;
use tauri::{AppHandle, Emitter};

use crate::keylog;
use crate::secrets::Loaded;

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
    /// Whether an app is installed (Android, Scan setup code's Tailscale
    /// check). None where it can't be told.
    fn app_installed(&self, _package: &str) -> Option<bool> {
        None
    }
    /// Open another app or a settings page for Scan setup code
    /// ("get_tailscale", "open_tailscale", "wifi_settings"), or (Settings ›
    /// Notifications) "connection_notification". False when nothing opened.
    fn open_app(&self, _what: &str) -> bool {
        false
    }
    /// Whether this device's traffic goes through a VPN now (Android: the
    /// hub notice says when Tailscale is off). None where it can't be told.
    fn vpn_active(&self) -> Option<bool> {
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
    fn push_state(&self) -> Option<PushState> { None }
    fn set_push(&self, _on: bool, _distributor: &str) -> Result<(), String> {
        Err("only Android has this setting".into())
    }
    /// Reconcile registrations after hubs or identity change.
    fn refresh_push(&self) {}
    /// Take back a chat's message notification: everything in it was read
    /// (here, or on another device; user 23:50Z). Where shown notifications
    /// can't be withdrawn, nothing.
    fn clear_notification(&self, _peer: &str) {}
    /// The device's own name (user 00:16Z: linking asks for none): the
    /// computer's name here, the phone's on Android.
    fn device_name(&self) -> String {
        computer_name()
    }
    /// The phone's maker as Android reports it ("samsung", "Google"), for
    /// help only some phones need; empty elsewhere.
    fn device_maker(&self) -> String {
        String::new()
    }
}

/// Windows: the computer's name as its owner wrote it (Settings › System ›
/// About; the host name keeps its case, COMPUTERNAME is upper case).
/// Linux: the host name. macOS: the computer name (System Settings › General
/// › About).
fn computer_name() -> String {
    #[cfg(windows)]
    {
        use winreg::enums::HKEY_LOCAL_MACHINE;
        let host: Option<String> = winreg::RegKey::predef(HKEY_LOCAL_MACHINE)
            .open_subkey(r"SYSTEM\CurrentControlSet\Services\Tcpip\Parameters")
            .and_then(|k| k.get_value("Hostname"))
            .ok();
        if let Some(h) = host.filter(|h| !h.trim().is_empty()) {
            return h.trim().to_owned();
        }
    }
    #[cfg(target_os = "linux")]
    if let Ok(h) = std::fs::read_to_string("/etc/hostname") {
        if !h.trim().is_empty() {
            return h.trim().to_owned();
        }
    }
    #[cfg(target_os = "macos")]
    if let Ok(out) = std::process::Command::new("/usr/sbin/scutil")
        .args(["--get", "ComputerName"])
        .output()
    {
        let h = String::from_utf8_lossy(&out.stdout);
        if out.status.success() && !h.trim().is_empty() {
            return h.trim().to_owned();
        }
    }
    #[cfg(target_os = "linux")]
    const FALLBACK: &str = "Linux PC";
    #[cfg(target_os = "macos")]
    const FALLBACK: &str = "Mac";
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    const FALLBACK: &str = "Windows PC";
    std::env::var("COMPUTERNAME")
        .ok()
        .filter(|n| !n.trim().is_empty())
        .unwrap_or_else(|| FALLBACK.into())
}

pub struct Core {
    pub dir: PathBuf,
    pub store: Arc<Store>,
    engine: Mutex<Option<Arc<Engine>>>,
    background_checks: Mutex<usize>,
    pub rt: tokio::runtime::Handle,
    host: Arc<ShellHost>,
    /// Set while this device had an identity but its key can't be found.
    key_lost: Mutex<Option<KeyLost>>,
}

/// This device had an identity, but its key is gone (user 2026-10-10: Windows
/// lost every saved sign-in after a crash, and Hubchat started over as if new,
/// without a word). The UI offers linking, recovery words or starting over.
#[derive(Clone, Serialize)]
pub struct KeyLost {
    /// Whose data this is, when known (Hubchat notes it from 1.0.3 on).
    address: Option<String>,
    /// The key store didn't answer: it may still have the key (try again).
    unreadable: bool,
}

/// Whether adopting `new` must first remove this device's local data: never
/// when there is none; when it belongs to another known identity; and, when
/// its owner is unknown (kept before Hubchat noted it), only for a brand-new
/// identity, which can't be the old one.
fn must_wipe(owner: Option<&str>, has_data: bool, new: &str, brand_new: bool) -> bool {
    has_data && owner.map_or(brand_new, |o| o != new)
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

/// Public Android settings only. Push capabilities never cross the webview bridge.
#[derive(serde::Serialize, serde::Deserialize)]
pub struct PushState {
    pub enabled: bool,
    pub active: bool,
    pub distributor: String,
    pub distributors: Vec<String>,
    pub status: String,
}

#[cfg(target_os = "android")]
pub struct BackgroundCheck(&'static Core);
#[cfg(target_os = "android")]
impl Drop for BackgroundCheck {
    fn drop(&mut self) {
        *self.0.background_checks.lock().unwrap() -= 1;
        self.0.reconcile_background();
    }
}

impl Host for ShellHost {
    fn event(&self, ev: Event) {
        if let Event::Incoming { peer, preview, quiet, .. } = &ev {
            let showing = self.foreground.load(Ordering::Relaxed)
                && self.open_chat.lock().unwrap().as_deref() == Some(peer.as_str());
            // `quiet`: another of our devices was in use when it came
            if !showing && !quiet && self.notify_on.load(Ordering::Relaxed) {
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
        // read on any device: its notification goes (user 23:50Z)
        if let Event::Chat { peer } = &ev {
            if self.store.unread(peer).is_ok_and(|n| n == 0) {
                self.platform.clear_notification(peer);
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
        background_checks: Mutex::new(0),
        rt,
        host,
        key_lost: Mutex::new(None),
    };
    let core = match CORE.set(core) {
        Ok(()) => CORE.get().unwrap(),
        Err(_) => return Ok(CORE.get().unwrap()), // lost a race: use the winner
    };
    core.load_key()?;
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

    /// What the UI shows. True when it just came to the foreground.
    pub fn set_ui_state(&self, foreground: bool, chat: Option<String>) -> bool {
        let was = self.host.foreground.swap(foreground, Ordering::Relaxed);
        *self.host.open_chat.lock().unwrap() = chat;
        self.reconcile_background();
        foreground && !was
    }

    /// Serialize lifecycle decisions with overlapping wake checks. The last
    /// check restores suspension, unless the UI became visible meanwhile.
    pub fn reconcile_background(&self) {
        let checks = self.background_checks.lock().unwrap();
        let push_active = self.platform().push_state().is_some_and(|s| s.active);
        if let Ok(e) = self.engine() {
            e.set_suspended(push_active && !self.host.foreground.load(Ordering::Relaxed) && *checks == 0);
        }
    }

    #[cfg(target_os = "android")]
    pub fn background_check(&'static self) -> BackgroundCheck {
        *self.background_checks.lock().unwrap() += 1;
        self.reconcile_background();
        BackgroundCheck(self)
    }

    /// Native Android lifecycle also drives this: JavaScript can be frozen
    /// before its visibility event runs when an activity goes into background.
    #[cfg(target_os = "android")]
    pub fn set_app_visible(&self, visible: bool) {
        self.host.foreground.store(visible, Ordering::Relaxed);
        if !visible {
            // JavaScript may freeze before reporting inactive. Tell the hub
            // natively so its active lease does not defer the next push.
            if let Ok(e) = self.engine() {
                let _guard = self.rt.enter();
                e.set_active(false);
            }
        }
        self.reconcile_background();
    }

    pub fn engine(&self) -> Result<Arc<Engine>, String> {
        self.engine
            .lock()
            .unwrap()
            .clone()
            .ok_or_else(|| "no identity yet".to_string())
    }

    /// What other devices see this one called: the name set in Settings ›
    /// Devices, else the device's own.
    pub fn device_name(&self) -> String {
        self.store
            .meta("device.name")
            .ok()
            .flatten()
            .filter(|n| !n.trim().is_empty())
            .unwrap_or_else(|| self.host.platform.device_name())
    }

    /// Settings › Devices › Rename: kept, and sent with the next sync (a
    /// v2 hub shows it on the device list). Empty goes back to the device's own.
    pub fn set_device_name(&self, name: &str) -> Result<String, String> {
        let name = name.trim();
        if name.chars().count() > 64 {
            return Err("a device name has at most 64 characters".into());
        }
        self.store
            .set_meta("device.name", name)
            .map_err(|e| e.to_string())?;
        let now = self.device_name();
        if let Ok(e) = self.engine() {
            let (id, _) = e.device();
            e.set_device(&id, &now);
        }
        Ok(now)
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

    /// Read the key and start (start-up, and Try again on the lost-key
    /// screen); without one, note whether this device had an identity.
    pub fn load_key(&self) -> Result<(), String> {
        if self.has_identity() {
            return Ok(());
        }
        let unreadable = match crate::secrets::load_identity(&self.dir) {
            Loaded::Found(me) => return self.start_with(me),
            Loaded::Restored(me) => {
                // the UI says so once (dismissed: key_restored_seen)
                self.store.set_meta("key.restored", "yes").map_err(|e| e.to_string())?;
                return self.start_with(me);
            }
            Loaded::Missing => false,
            Loaded::Unreadable => true,
        };
        *self.key_lost.lock().unwrap() = self.had_identity().then(|| KeyLost {
            address: self.store.meta("identity.address").ok().flatten(),
            unreadable,
        });
        self.host.platform.status("Set up Hubchat to connect");
        Ok(())
    }

    fn start_with(&self, me: Identity) -> Result<(), String> {
        self.note_owner(&me);
        *self.key_lost.lock().unwrap() = None;
        self.start_engine(me)
    }

    pub fn key_lost(&self) -> Option<KeyLost> {
        self.key_lost.lock().unwrap().clone()
    }

    /// The local data belongs to an identity: it ran here (device.id is made
    /// at its first start) or Hubchat noted whose it is.
    fn had_identity(&self) -> bool {
        let has = |k: &str| self.store.meta(k).ok().flatten().is_some_and(|v| !v.is_empty());
        has("device.id") || has("identity.address")
    }

    /// Note whose the local data is (lost-key screen; nothing inherits it).
    fn note_owner(&self, me: &Identity) {
        let a = me.address();
        if self.store.meta("identity.address").ok().flatten().as_deref() != Some(a.as_str()) {
            let _ = self.store.set_meta("identity.address", &a);
        }
    }

    /// The lost-key screen's Start over: this device's local data and any
    /// trace of the old key go; the address keeps working on other devices.
    pub fn start_over(&self) -> Result<(), String> {
        if self.has_identity() {
            return Err("this device already has an identity".into());
        }
        // The missing identity key prevents a signed hub unregister. Stop the
        // distributor locally; endpoint rejection or device revocation cleans
        // the hub registration when this device can no longer authenticate.
        if self.platform().push_state().is_some() {
            self.platform().set_push(false, "")?;
        }
        crate::secrets::forget_identity(&self.dir)?;
        self.store.wipe().map_err(|e| e.to_string())?;
        *self.key_lost.lock().unwrap() = None;
        keylog::note(&self.dir, "started over: the local data of the lost identity was removed");
        Ok(())
    }

    /// Save a new or restored identity and start talking to the hubs.
    /// Leave the current identity on this device (switching to another):
    /// off the device lists of v2 hubs, connections stopped, key and local
    /// data forgotten. The address itself stays registered for its other
    /// devices.
    pub async fn leave_identity(&self) -> Result<(), String> {
        if self.platform().push_state().is_some() {
            self.platform().set_push(false, "")?;
        }
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
        keylog::note(&self.dir, "left this identity: key and local data forgotten");
        self.host.platform.status("Switching identity");
        Ok(())
    }

    /// Take an identity on this device (new, recovery words, a link, a key
    /// QR or file) with the `meta` it brings (profile and the like). Local
    /// data that belongs to another identity goes first (user 2026-10-10:
    /// a new identity must never inherit the old one's chats and hubs).
    pub fn adopt_identity(&self, me: Identity, brand_new: bool, meta: &[(&str, &str)]) -> Result<(), String> {
        if self.has_identity() {
            return Err("this device already has an identity".into());
        }
        let owner = self.store.meta("identity.address").ok().flatten();
        if must_wipe(owner.as_deref(), self.had_identity(), &me.address(), brand_new) {
            if self.platform().push_state().is_some() {
                self.platform().set_push(false, "")?;
            }
            self.store.wipe().map_err(|e| e.to_string())?;
            keylog::note(&self.dir, "another identity's local data was removed before taking this one");
        }
        for (k, v) in meta {
            self.store.set_meta(k, v).map_err(|e| e.to_string())?;
        }
        crate::secrets::save_identity(&self.dir, &me)?;
        keylog::note(&self.dir, if brand_new { "key saved: a new identity" } else { "key saved: an identity brought to this device" });
        self.start_with(me)
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
        engine.set_device(&device_id, &self.device_name());
        let read = self.store.meta("settings.read_receipts").ok().flatten();
        engine.set_read_receipts(read.as_deref() != Some("off"));
        engine.set_suspended(self.platform().push_state().is_some_and(|s| s.active)
            && !self.host.foreground.load(Ordering::Relaxed));
        {
            // start() spawns its tasks onto the core runtime.
            let _guard = self.rt.enter();
            engine.start().map_err(|e| e.to_string())?;
        }
        self.host
            .platform
            .status(&format!("Connected as {}", engine.me().address()));
        *self.engine.lock().unwrap() = Some(engine);
        self.platform().refresh_push();
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



#[cfg(test)]
mod key_tests {
    use super::must_wipe;

    #[test]
    fn a_new_identity_never_inherits_another_identitys_local_data() {
        // nothing here yet: nothing to remove
        assert!(!must_wipe(None, false, "alex.111111", true));
        assert!(!must_wipe(None, false, "alex.111111", false));
        // the same identity coming back (relinked, recovery words): kept
        assert!(!must_wipe(Some("alex.111111"), true, "alex.111111", false));
        // another identity: removed, however it arrives
        assert!(must_wipe(Some("alex.111111"), true, "pat.222222", false));
        assert!(must_wipe(Some("alex.111111"), true, "pat.222222", true));
        // data whose owner wasn't noted (before 1.0.3): a brand-new identity
        // can't be its owner; one brought here may be, so it is kept
        assert!(must_wipe(None, true, "pat.222222", true));
        assert!(!must_wipe(None, true, "alex.111111", false));
    }
}
