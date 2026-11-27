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

use crate::keylog::{self, Fact};
use crate::secrets::Loaded;

/// What only the platform can do.
pub trait Platform: Send + Sync + 'static {
    /// Schedule a fresh native launcher projection, including while the UI is hidden.
    fn shortcuts_changed(&self) {}
    /// Show a message notification (title = sender, body = preview).
    /// `sound`: play the notification sound (Settings › Notifications).
    fn notify(&self, title: &str, body: &str, peer: &str, sound: bool);
    /// Incoming notifications retain the identity of the producing engine.
    fn notify_incoming(&self, owner: &str, title: &str, body: &str, peer: &str, sound: bool) {
        let _ = owner;
        self.notify(title, body, peer, sound);
    }
    /// A local transfer failure uses its own replacement key, separate from
    /// incoming-message notifications which disappear when a chat is read.
    fn notify_transfer(&self, notice:&hubchat_core::store::TransferFailure, title:&str, body:&str, sound:bool) -> std::io::Result<()> {
        self.notify(title,body,&notice.peer,sound); Ok(())
    }

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
    /// Idempotent export of a durable transfer. The key survives process death;
    /// implementations retain a journal and never delete the private source.
    fn publish_transfer(&self, _key: &str, _path: &std::path::Path, _name: &str)
        -> std::io::Result<Option<String>> { Ok(None) }
    fn transfer_published(&self, _key: &str) -> std::io::Result<()> { Ok(()) }
    fn transfer_abandoned(&self, _key:&str, _source:&str)->std::io::Result<()> { Ok(()) }
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
    /// Cheap lifecycle read; no distributor enumeration.
    fn push_active(&self) -> bool { false }
    fn set_push(&self, _on: bool, _distributor: &str) -> Result<(), String> {
        Err("only Android has this setting".into())
    }
    /// None on desktop; Android reports actual foreground-service coverage.
    fn file_background_covered(&self) -> Option<bool> { None }
    fn app_visible(&self) -> Option<bool> { None }
    fn set_file_work(&self, _wanted:bool) {}
    fn file_background_hint(&self) -> Option<String> { None }
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
    const FALLBACK: &str = "Linux device";
    #[cfg(target_os = "macos")]
    const FALLBACK: &str = "Mac";
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    const FALLBACK: &str = "Windows PC";
    std::env::var("COMPUTERNAME")
        .ok()
        .filter(|n| !n.trim().is_empty())
        .unwrap_or_else(|| FALLBACK.into())
}

/// WHO MAY CHANGE OR READ THE SIGNED-IN IDENTITY (tray reads and actions, identity adoption, sign-out).
/// One thread at a time, and the thread inside may enter again: a tray section's window show runs
/// `tray_recent::sync` inline on the main thread (Tauri's `run_on_main_thread` does so there), and sync opens
/// its own section. With the engine's plain mutex as the guard that nested section waited on its own thread
/// forever (the tray chat-list freeze, measured 2026-11-26). Nested sections can't see a half-done identity
/// change: adoption and sign-out take the gate only around swapping the engine and call nothing inside it.
#[derive(Default)]
struct IdentityGate {
    held: Mutex<(Option<std::thread::ThreadId>, usize)>,
    free: std::sync::Condvar,
}

struct IdentityPass<'a>(&'a IdentityGate);

impl IdentityGate {
    fn enter(&self) -> IdentityPass<'_> {
        let me = std::thread::current().id();
        let mut held = self.held.lock().unwrap_or_else(|p| p.into_inner());
        loop {
            match held.0 {
                None => { *held = (Some(me), 1); break; }
                Some(owner) if owner == me => { held.1 += 1; break; }
                Some(_) => held = self.free.wait(held).unwrap_or_else(|p| p.into_inner()),
            }
        }
        IdentityPass(self)
    }
}

impl Drop for IdentityPass<'_> {
    fn drop(&mut self) {
        let mut held = self.0.held.lock().unwrap_or_else(|p| p.into_inner());
        held.1 -= 1;
        if held.1 == 0 {
            held.0 = None;
            self.0.free.notify_all();
        }
    }
}

pub struct Core {
    pub dir: PathBuf,
    pub store: Arc<Store>,
    /// Held only to read or swap the engine, never across other work: see IdentityGate.
    engine: Mutex<Option<Arc<Engine>>>,
    identity_gate: IdentityGate,
    background_checks: Mutex<usize>,
    file_admissions: std::sync::atomic::AtomicUsize,
    file_lifetime: Mutex<()>,
    pub rt: tokio::runtime::Handle,
    host: Arc<ShellHost>,
    /// Set while this device had an identity but its key can't be found.
    key_lost: Mutex<Option<KeyLost>>,
    identity_load: hubchat_core::notification_reply::IdentityLoader,
    #[cfg(windows)]
    shortcut_projection: Mutex<Option<(String, Vec<hubchat_core::store::ShortcutContact>)>>,
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
    /// Linux only: the key store in this session holds a different identity
    /// than this device's local chats. Always None off Linux.
    #[serde(skip_serializing_if = "Option::is_none")]
    found: Option<String>,
}

/// Whether adopting `new` must first remove this device's local data: never
/// when there is none; when it belongs to another known identity; and, when
/// its owner is unknown (kept before Hubchat noted it), only for a brand-new
/// identity, which can't be the old one.
fn must_wipe(owner: Option<&str>, has_data: bool, new: &str, brand_new: bool) -> bool {
    has_data && owner.map_or(brand_new, |o| o != new)
}

struct ShellHost {
    staging: PathBuf,
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

// Failure notices remain useful with the chat open. Unlike incoming mail,
// neither foreground state nor message-preview settings suppress them.
fn failure_notice(
    settings: NotifySettings,
    name: &str,
    notice: &hubchat_core::store::TransferFailure,
) -> Option<(String, &'static str, bool)> {
    if !settings.enabled { return None; }
    // Never include the raw error, paths, request URLs or message contents.
    let (suffix, body) = if notice.download {
        ("download failed", "A file couldn't be downloaded. Open this chat to retry.")
    } else {
        ("file not sent", "A file couldn't be sent. Open this chat to retry or remove the file.")
    };
    Some((format!("{name}: {suffix}"), body, settings.sound && notice.attempts == 0))
}

/// What notifications may do now: Settings › Notifications, unless our
/// status is Do not disturb (user 2026-10-31 08:52Z): then nothing shows and
/// nothing sounds, on any of our devices, for a message or a failed file.
/// Mail, unread counts and badges go on; the settings themselves stay.
fn notify_now(settings: NotifySettings, dnd: bool) -> NotifySettings {
    if dnd { NotifySettings { enabled: false, sound: false, ..settings } } else { settings }
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

impl ShellHost {
    /// Settings › Notifications as they apply now: off on Do not disturb
    /// (our status, as the core keeps it).
    fn notify_settings_now(&self) -> NotifySettings {
        let settings = NotifySettings {
            enabled: self.notify_on.load(Ordering::Relaxed),
            preview: self.notify_text.load(Ordering::Relaxed),
            sound: self.notify_sound.load(Ordering::Relaxed),
        };
        notify_now(settings, hubchat_core::engine::chosen_status(&self.store) == "dnd")
    }
}

impl Host for ShellHost {
    fn transfer_failed(&self, notice: &hubchat_core::store::TransferFailure) -> std::io::Result<()> {
        let settings = self.notify_settings_now();
        let name = self.store.display_name(&notice.peer).ok().flatten()
            .unwrap_or_else(|| notice.peer.clone());
        if let Some((title, body, sound)) = failure_notice(settings, &name, notice) {
            self.platform.notify_transfer(notice, &title, body, sound)?;
        }
        Ok(())
    }
    fn file_work_changed(&self) {
        if let Ok(c)=get() { c.rt.spawn(async move { c.refresh_file_lifetime(); }); }
    }
    fn event(&self, ev: Event) {
        if let Event::Incoming { peer, quiet, .. } = &ev {
            let showing = self.foreground.load(Ordering::Relaxed)
                && self.open_chat.lock().unwrap().as_deref() == Some(peer.as_str());
            // `quiet`: another of our devices was in use when it came; on Do
            // not disturb nothing shows or sounds
            let now = self.notify_settings_now();
            if !showing && !quiet && now.enabled {
                if let Some(n) = hubchat_core::notification_reply::incoming_notice(&ev, &self.store, now.preview) {
                    self.platform.notify_incoming(&n.owner, &n.title, &n.body, &n.peer, now.sound);
                }
            }
        }
        // read on any device: its notification goes (user 23:50Z)
        if let Event::Chat { peer } = &ev {
            if self.store.unread(peer).is_ok_and(|n| n == 0) {
                self.platform.clear_notification(peer);
            }
        }
        if matches!(&ev, Event::Incoming { .. } | Event::Chat { .. } | Event::Directory) {
            self.platform.shortcuts_changed();
        }
        // A contact picture whose bytes were unavailable can be fetched after
        // its hub connects, even when names/order/metadata have not changed.
          #[cfg(any(windows, target_os = "android"))]
          if matches!(&ev, Event::Hub { .. }) { self.platform.shortcuts_changed(); }
        if let Some(app) = self.app.get() {
            let _ = app.emit("hc", &ev);
            #[cfg(windows)]
            crate::tray_recent::event(app, &ev);
        }
    }
    fn open_source(&self, source: &str) -> std::io::Result<std::fs::File> {
        self.platform.open_source(source)
    }
    fn download_dir(&self) -> PathBuf {
        self.platform.download_dir()
    }
    fn publish_download(&self, key: &str, path: &std::path::Path, name: &str)
        -> std::io::Result<Option<String>> {
        self.platform.publish_transfer(key, path, name)
    }
    fn download_export_committed(&self, key: &str) -> std::io::Result<()> {
        self.platform.transfer_published(key)
    }
    fn download_export_abandoned(&self,key:&str,source:&str)->std::io::Result<()> {
        self.platform.transfer_abandoned(key,source)
    }
    fn staging_dir(&self) -> PathBuf { self.staging.clone() }
}

static CORE: OnceLock<Core> = OnceLock::new();

pub fn get() -> Result<&'static Core, String> {
    CORE.get()
        .ok_or_else(|| "Hubchat is still starting".to_string())
}

/// Create the core once per process (later calls return the existing one).
pub fn init(dir: PathBuf, platform: Arc<dyn Platform>) -> Result<&'static Core, String> {
    if let Some(c) = CORE.get() {
        c.finish_startup()?;
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
        staging: dir.join("outbox"),
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
        identity_gate: IdentityGate::default(),
        background_checks: Mutex::new(0),
        file_admissions: std::sync::atomic::AtomicUsize::new(0),
        file_lifetime: Mutex::new(()),
        rt,
        host,
        key_lost: Mutex::new(None),
        identity_load: Default::default(),
        #[cfg(windows)]
        shortcut_projection: Mutex::new(None),
    };
    let core = match CORE.set(core) {
        Ok(()) => CORE.get().unwrap(),
        Err(_) => CORE.get().unwrap(), // winner may still need its key loaded
    };
    core.finish_startup()?;
    Ok(core)
}

impl Core {
    fn finish_startup(&self) -> Result<(), String> {
        self.identity_load.attach(|| {
            if self.has_identity() { Ok(true) } else { self.load_key_inner() }
        })
    }

    pub fn platform(&self) -> &dyn Platform {
        &*self.host.platform
    }

    /// Called by the Tauri setup so events reach the UI. Events the core sent
    /// before this were dropped, and the page may already have read its state
    /// (a WebView created during setup pumps the UI thread, so the page can
    /// load before setup ends): tell it to read everything again.
    pub fn attach_ui(&self, app: AppHandle) {
        self.host.platform.shortcuts_changed();
        #[cfg(windows)]
        crate::tray_recent::changed(&app);
        if self.host.app.set(app).is_ok() {
            if let Some(app) = self.host.app.get() {
                let _ = app.emit("hc", serde_json::json!({"type": "attached"}));
            }
        }
    }

    /// Short tray reads/actions cannot cross identity adoption or sign-out.
    /// Never put an await/network call inside this closure.
    #[cfg(windows)]
    pub fn tray_with_identity<T>(&self, expected: Option<&str>, f: impl FnOnce(&str) -> Result<T, String>) -> Result<T, String> {
        let _pass = self.identity_gate.enter();
        let owner = self.engine().map_err(|_| "No identity".to_string())?.me().address();
        if expected.is_some_and(|e| e != owner) || self.store.meta("identity.address").map_err(|e|e.to_string())?.as_deref() != Some(owner.as_str()) {
            return Err("Identity changed".into());
        }
        f(&owner)
    }

    /// Resolve and publish one projection while identity adoption is excluded.
    /// Publishers do only local OS work and must never call back into the core.
    #[cfg(any(windows, target_os = "android"))]
    pub fn with_shortcut_data<T>(&self, open_only: bool, limit: Option<u32>, publish: impl FnOnce(&str, Vec<hubchat_core::store::ShortcutContact>) -> Result<T, String>) -> Result<T, String> {
        let _pass = self.identity_gate.enter();
        let owner = self.engine().map(|e|e.me().address()).unwrap_or_default();
        let rows = if owner.is_empty() {
            vec![]
        } else {
            if self.store.meta("identity.address").map_err(|e|e.to_string())?.as_deref() != Some(owner.as_str()) {
                return Err("Identity changed".into());
            }
            self.store.shortcut_contacts(open_only, limit).map_err(|e|e.to_string())?
        };
        publish(&owner, rows)
    }

    #[cfg(windows)]
    pub fn with_shortcuts(&self, open_only: bool, limit: Option<u32>, publish: impl FnOnce(&str, Vec<hubchat_core::store::ShortcutContact>) -> Result<(), String>) -> Result<(), String> {
        self.with_shortcut_data(open_only, limit, |owner, rows| {
            let mut last = self.shortcut_projection.lock().unwrap();
            if last.as_ref().is_some_and(|(o,r)|o == owner && r == &rows) { return Ok(()); }
            publish(owner, rows.clone())?;
            *last = Some((owner.to_owned(), rows));
            Ok(())
        })
    }

    /// Windows adds picture state to the names/order projection; Android
    /// coalesces its complete avatar payload in android_shortcuts instead.
    #[cfg(windows)]
    pub fn invalidate_shortcuts(&self) { *self.shortcut_projection.lock().unwrap() = None; }

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
        let visible = self.platform().app_visible().unwrap_or_else(||self.host.foreground.load(Ordering::Relaxed));
        let coverage = self.platform().file_background_covered();
        let (suspended,files_paused)=background_policy(visible,coverage,self.platform().push_active(),*checks);
        if let Ok(e) = self.engine() {
            e.set_files_paused(files_paused);
            e.set_suspended(suspended);
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
        self.refresh_file_lifetime();
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

    pub fn file_admission(&'static self, needed:bool) -> FileAdmission {
        if needed {
            self.file_admissions.fetch_add(1,Ordering::SeqCst);
            self.refresh_file_lifetime();
        }
        FileAdmission(needed.then_some(self))
    }
    pub fn refresh_file_lifetime(&self) {
        if self.platform().file_background_covered().is_none() { return; }
        let _serial=self.file_lifetime.lock().unwrap();
        let wanted=self.file_admissions.load(Ordering::SeqCst)>0 || self.store.pending_file_work().unwrap_or(true);
        // Platform implementations queue this on their main thread; no callback
        // may re-enter Core before this call returns.
        self.platform().set_file_work(wanted);
    }
    #[cfg(target_os="android")]
    pub fn file_service_changed(&self) {
        self.reconcile_background();
        if let Some(app)=self.host.app.get() {
            let _=app.emit("hc",serde_json::json!({"type":"file_lifecycle"}));
        }
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
        self.identity_load.run(|| self.has_identity(), || self.load_key_inner())
    }

    fn load_key_inner(&self) -> Result<bool, String> {
        let unreadable = match crate::secrets::load_identity(&self.dir) {
            Loaded::Found(me) => {
                #[cfg(target_os = "linux")]
                if self.store_mismatch(&me)? {
                    return Ok(false);
                }
                return self.start_with(me).map(|_| true);
            }
            Loaded::Restored(me) => {
                #[cfg(target_os = "linux")]
                if self.store_mismatch(&me)? {
                    return Ok(false);
                }
                // the UI says so once (dismissed: key_restored_seen)
                self.store.set_meta("key.restored", "yes").map_err(|e| e.to_string())?;
                return self.start_with(me).map(|_| true);
            }
            Loaded::Missing => false,
            Loaded::Unreadable => true,
        };
        *self.key_lost.lock().unwrap() = self.had_identity().then(|| KeyLost {
            address: self.store.meta("identity.address").ok().flatten(),
            unreadable,
            found: None,
        });
        self.host.platform.status("Set up Hubchat to connect");
        // Keep the lost-key UI available while a later cold attach retries the key.
        Ok(false)
    }

    fn start_with(&self, me: Identity) -> Result<(), String> {
        self.note_owner(&me);
        *self.key_lost.lock().unwrap() = None;
        self.start_engine(me)
    }

    pub fn key_lost(&self) -> Option<KeyLost> {
        self.key_lost.lock().unwrap().clone()
    }

    #[cfg(target_os = "linux")]
    fn store_mismatch(&self, me: &Identity) -> Result<bool, String> {
        let owner = self
            .store
            .meta("identity.address")
            .map_err(|e| e.to_string())?
            .filter(|a| !a.is_empty());
        if let Some(a) = owner {
            if a != me.address() {
                *self.key_lost.lock().unwrap() = Some(KeyLost {
                    address: Some(a),
                    unreadable: false,
                    found: Some(me.address()),
                });
                self.host.platform.status("Set up Hubchat to connect");
                return Ok(true);
            }
        }
        Ok(false)
    }

    #[cfg(target_os = "linux")]
    fn mismatch_guard_error(&self, me: &Identity, brand_new: bool) -> Option<String> {
        let lost = self.key_lost.lock().unwrap().clone();
        let Some(KeyLost {
            address,
            found: Some(found),
            ..
        }) = lost
        else {
            return None;
        };
        let owner = address.as_deref();
        if brand_new || owner != Some(me.address().as_str()) {
            let who = owner.unwrap_or("the previous identity");
            return Some(format!(
                "This device's chats belong to {who}. Use {who}'s recovery words, link or key file, or choose Use {found} here."
            ));
        }
        None
    }

    #[cfg(target_os = "linux")]
    pub fn use_found_identity(&self, expected: &str) -> Result<(), String> {
        let mut stale = false;
        let res = self.identity_load.run(
            || false,
            || {
                if self.has_identity() {
                    return Err("this device already has an identity".into());
                }
                let saved_lost = self.key_lost.lock().unwrap().clone();
                if saved_lost.as_ref().and_then(|k| k.found.as_deref()) != Some(expected) {
                    stale = true;
                    return Ok(false);
                }
                match crate::secrets::load_identity(&self.dir) {
                    Loaded::Found(me) | Loaded::Restored(me) if me.address() == expected => {
                        // Bypass mismatch_guard_error ONLY for this verified `expected`
                        // identity, serialized inside `identity_load.run`.
                        *self.key_lost.lock().unwrap() = None;
                        if let Err(e) = self.adopt_identity_inner(me, false, &[]) {
                            if self.had_identity() {
                                *self.key_lost.lock().unwrap() = saved_lost;
                            }
                            return Err(e);
                        }
                        Ok(true)
                    }
                    _ => {
                        stale = true;
                        Ok(false)
                    }
                }
            },
        );
        if stale {
            let _ = self.load_key();
            return Err("The key store changed. Try again.".into());
        }
        res?;
        self.platform().refresh_push();
        Ok(())
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

    /// Push cleanup must never prevent leaving or discarding an identity.
    fn forget_push(&self) {
        #[cfg(target_os = "android")]
        if self.platform().set_push(false, "").is_err() {
            keylog::note(&self.dir, "push cleanup failed while discarding the identity; continuing", &[Fact::PushCleanupFailed]);
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
        self.forget_push();
        crate::secrets::forget_identity(&self.dir)?;
        self.store.wipe().map_err(|e| e.to_string())?;
        *self.key_lost.lock().unwrap() = None;
        keylog::note(&self.dir, "started over: the local data of the lost identity was removed", &[Fact::StartedOver]);
        Ok(())
    }

    /// Save a new or restored identity and start talking to the hubs.
    /// Leave the current identity on this device (switching to another):
    /// off the device lists of v2 hubs, connections stopped, key and local
    /// data forgotten. The address itself stays registered for its other
    /// devices.
    pub async fn leave_identity(&self) -> Result<(), String> {
        self.leave(false).await
    }

    /// Settings › Devices › Sign out of this device: as leaving for another
    /// identity (the hubs forget this device and its push first, while the
    /// key can still ask), then the welcome screen.
    pub async fn sign_out(&self) -> Result<(), String> {
        if !self.has_identity() {
            return Err("no identity yet".into());
        }
        self.leave(true).await
    }

    async fn leave(&self, signing_out: bool) -> Result<(), String> {
        self.forget_push();
        let taken = { let _pass = self.identity_gate.enter(); self.engine.lock().unwrap().take() };
        let Some(e) = taken else {
            return Ok(());
        };
        self.host.platform.shortcuts_changed();
        #[cfg(windows)]
        if let Some(app) = self.host.app.get() { crate::tray_recent::reset(app); }
        let e2 = e.clone();
        let _ = self
            .rt
            .spawn(async move { e2.sign_out_this_device().await })
            .await;
        e.shutdown();
        crate::secrets::forget_identity(&self.dir)?;
        self.store.wipe().map_err(|e| e.to_string())?;
        *self.key_lost.lock().unwrap() = None;
        if signing_out {
            keylog::note(&self.dir, "signed out of this device: key and local data forgotten", &[Fact::LeftIdentity]);
            self.host.platform.status("Signed out");
        } else {
            keylog::note(&self.dir, "left this identity: key and local data forgotten", &[Fact::LeftIdentity]);
            self.host.platform.status("Switching identity");
        }
        Ok(())
    }

    /// Take an identity on this device (new, recovery words, a link, a key
    /// QR or file) with the `meta` it brings (profile and the like). Local
    /// data that belongs to another identity goes first (user 2026-10-10:
    /// a new identity must never inherit the old one's chats and hubs).
    pub fn adopt_identity(&self, me: Identity, brand_new: bool, meta: &[(&str, &str)]) -> Result<(), String> {
        self.identity_load.run(|| false, || self.adopt_identity_inner(me, brand_new, meta).map(|_| true))?;
        self.platform().refresh_push();
        Ok(())
    }

    fn adopt_identity_inner(&self, me: Identity, brand_new: bool, meta: &[(&str, &str)]) -> Result<(), String> {
        if self.has_identity() {
            return Err("this device already has an identity".into());
        }
        #[cfg(target_os = "linux")]
        if let Some(err) = self.mismatch_guard_error(&me, brand_new) {
            return Err(err);
        }
        let owner = self.store.meta("identity.address").ok().flatten();
        if must_wipe(owner.as_deref(), self.had_identity(), &me.address(), brand_new) {
            self.forget_push();
            self.store.wipe().map_err(|e| e.to_string())?;
            keylog::note(&self.dir, "another identity's local data was removed before taking this one", &[Fact::OtherLocalDataRemoved]);
        }
        for (k, v) in meta {
            self.store.set_meta(k, v).map_err(|e| e.to_string())?;
        }
        crate::secrets::save_identity(&self.dir, &me)?;
        keylog::note(&self.dir, if brand_new { "key saved: a new identity" } else { "key saved: an identity brought to this device" }, &[if brand_new { Fact::KeySavedNew } else { Fact::KeySavedBrought }]);
        self.start_with(me)?;
        Ok(())
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
        let visible=self.platform().app_visible().unwrap_or_else(||self.host.foreground.load(Ordering::Relaxed));
        let (suspended,files_paused)=background_policy(visible,self.platform().file_background_covered(),self.platform().push_active(),0);
        engine.set_files_paused(files_paused);
        engine.set_suspended(suspended);
        {
            // start() spawns its tasks onto the core runtime.
            let _guard = self.rt.enter();
            engine.start().map_err(|e| e.to_string())?;
        }
        self.host
            .platform
            .status(&format!("Connected as {}", engine.me().address()));
        {
            let _pass = self.identity_gate.enter();
            *self.engine.lock().unwrap() = Some(engine);
        }
        self.host.platform.shortcuts_changed();
        #[cfg(windows)]
        if let Some(app) = self.host.app.get() { crate::tray_recent::changed(app); }
        self.refresh_file_lifetime();
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

#[cfg(all(test, target_os = "linux"))]
mod linux_r5_core_tests {
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
    use std::sync::{Arc, Mutex, OnceLock};

    use hubchat_core::store::{NewOutgoing, Store};
    use hubchat_core::Identity;

    use super::{Core, Platform, ShellHost};
    use crate::secrets::linux::{with_test_backend, MockKeyring, SecretServiceState};

    static SEQ: AtomicU64 = AtomicU64::new(0);

    struct StubPlatform(PathBuf);
    impl Platform for StubPlatform {
        fn notify(&self, _: &str, _: &str, _: &str, _: bool) {}
        fn status(&self, _: &str) {}
        fn open_source(&self, s: &str) -> std::io::Result<std::fs::File> {
            std::fs::File::open(s)
        }
        fn download_dir(&self) -> PathBuf {
            self.0.join("downloads")
        }
    }

    fn make_core(label: &str) -> (Core, tokio::runtime::Runtime, PathBuf) {
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "hubchat-r5-core-{label}-{}-{n}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let store = Arc::new(Store::open(&dir.join("hubchat.sqlite3")).unwrap());
        let rt = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .enable_all()
            .build()
            .unwrap();
        let host = Arc::new(ShellHost {
            staging: dir.join("outbox"),
            app: OnceLock::new(),
            platform: Arc::new(StubPlatform(dir.clone())),
            foreground: AtomicBool::new(false),
            open_chat: Mutex::new(None),
            store: store.clone(),
            notify_on: AtomicBool::new(false),
            notify_text: AtomicBool::new(false),
            notify_sound: AtomicBool::new(false),
        });
        let core = Core {
            dir: dir.clone(),
            store,
            engine: Mutex::new(None),
            identity_gate: Default::default(),
            background_checks: Mutex::new(0),
            file_admissions: std::sync::atomic::AtomicUsize::new(0),
            file_lifetime: Mutex::new(()),
            rt: rt.handle().clone(),
            host,
            key_lost: Mutex::new(None),
            identity_load: Default::default(),
        };
        (core, rt, dir)
    }

    fn seed_owner_data(store: &Store, owner: Option<&str>) {
        store.set_meta("device.id", "hc-test-dev").unwrap();
        if let Some(a) = owner {
            store.set_meta("identity.address", a).unwrap();
        }
        store.add_hub("http://127.0.0.1:9", "2026-11-22T00:00:00Z").unwrap();
        store.set_draft("peer.111111", "unsent draft").unwrap();
        store
            .queue_outgoing(
                &NewOutgoing {
                    id: "msg-1".into(),
                    peer: "peer.111111".into(),
                    body: "hello".into(),
                    kind: None,
                    reply_to: None,
                    attachments: vec![],
                },
                "2026-11-22T00:00:00Z",
            )
            .unwrap();
    }

    fn cleanup(core: Core, rt: tokio::runtime::Runtime, dir: PathBuf) {
        if let Ok(e) = core.engine() {
            e.shutdown();
        }
        drop(core);
        drop(rt);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn r5_owner_a_found_b_sets_key_lost_found_without_starting_engine() {
        let (core, rt, dir) = make_core("mismatch");
        let a = Identity::generate("alice").unwrap();
        let b = Identity::generate("bob").unwrap();
        seed_owner_data(&core.store, Some(&a.address()));
        let kr = MockKeyring::new();
        with_test_backend(SecretServiceState::Absent, kr, || {
            crate::secrets::save_identity(&dir, &b).unwrap();
            core.load_key().unwrap();
        });
        assert!(!core.has_identity(), "must not start engine on owner mismatch");
        let lost = core.key_lost().expect("key_lost must be set");
        assert_eq!(lost.address.as_deref(), Some(a.address().as_str()));
        assert!(!lost.unreadable);
        assert_eq!(lost.found.as_deref(), Some(b.address().as_str()));
        assert_eq!(core.store.chat("peer.111111", None, None, 10).unwrap().len(), 1);
        cleanup(core, rt, dir);
    }

    #[test]
    fn r5_use_found_identity_rejects_stale_expected_without_wiping_a() {
        let (core, rt, dir) = make_core("stale");
        let a = Identity::generate("alice").unwrap();
        let b = Identity::generate("bob").unwrap();
        let c = Identity::generate("carol").unwrap();
        seed_owner_data(&core.store, Some(&a.address()));
        let kr = MockKeyring::new();
        with_test_backend(SecretServiceState::Absent, kr, || {
            crate::secrets::save_identity(&dir, &b).unwrap();
            core.load_key().unwrap();
            assert_eq!(core.key_lost().unwrap().found.as_deref(), Some(b.address().as_str()));
            // Key store changes to C before user confirms B.
            crate::secrets::save_identity(&dir, &c).unwrap();
            let err = core.use_found_identity(&b.address()).unwrap_err();
            assert_eq!(err, "The key store changed. Try again.");
        });
        assert!(!core.has_identity(), "must not start on stale expected");
        assert_eq!(core.store.meta("identity.address").unwrap().as_deref(), Some(a.address().as_str()));
        assert_eq!(core.store.chat("peer.111111", None, None, 10).unwrap().len(), 1, "A's messages preserved");
        assert_eq!(core.store.draft("peer.111111").unwrap().as_deref(), Some("unsent draft"), "A's drafts preserved");
        assert_eq!(core.store.hubs().unwrap().len(), 1, "A's hubs preserved");
        assert_eq!(core.key_lost().unwrap().found.as_deref(), Some(c.address().as_str()), "key state reloaded to C");
        cleanup(core, rt, dir);
    }

    #[test]
    fn r5_use_found_identity_wipes_a_sets_owner_b_and_starts_engine() {
        let (core, rt, dir) = make_core("accept-found");
        let a = Identity::generate("alice").unwrap();
        let b = Identity::generate("bob").unwrap();
        seed_owner_data(&core.store, Some(&a.address()));
        let kr = MockKeyring::new();
        with_test_backend(SecretServiceState::Absent, kr, || {
            crate::secrets::save_identity(&dir, &b).unwrap();
            core.load_key().unwrap();
            core.use_found_identity(&b.address()).expect("use_found_identity should succeed");
        });
        assert!(core.has_identity(), "engine started");
        assert_eq!(core.engine().unwrap().me().address(), b.address());
        assert_eq!(core.store.meta("identity.address").unwrap().as_deref(), Some(b.address().as_str()));
        assert!(core.key_lost().is_none(), "key_lost cleared");
        assert!(core.store.chat("peer.111111", None, None, 10).unwrap().is_empty(), "A's chats wiped");
        assert!(core.store.draft("peer.111111").unwrap().is_none(), "A's drafts wiped");
        assert!(core.store.hubs().unwrap().is_empty(), "A's hubs wiped");
        cleanup(core, rt, dir);
    }

    #[test]
    fn r5_owner_none_or_matching_owner_starts_as_1_3_11() {
        // Legacy ownerless profile starts normally with found key.
        {
            let (core, rt, dir) = make_core("owner-none");
            let b = Identity::generate("bob").unwrap();
            seed_owner_data(&core.store, None);
            let kr = MockKeyring::new();
            with_test_backend(SecretServiceState::Absent, kr.clone(), || {
                crate::secrets::save_identity(&dir, &b).unwrap();
                core.load_key().unwrap();
            });
            assert_eq!(kr.calls(), 0, "file backend must be used when Secret Service is absent");
            assert!(dir.join("identity-store").join("identity.key").exists());
            assert!(core.has_identity());
            assert!(core.key_lost().is_none());
            assert_eq!(core.engine().unwrap().me().address(), b.address());
            assert_eq!(core.store.chat("peer.111111", None, None, 10).unwrap().len(), 1);
            cleanup(core, rt, dir);
        }

        // Matching owner starts normally and keeps chats.
        {
            let (core, rt, dir) = make_core("owner-match");
            let a = Identity::generate("alice").unwrap();
            seed_owner_data(&core.store, Some(&a.address()));
            let kr = MockKeyring::new();
            with_test_backend(SecretServiceState::Absent, kr.clone(), || {
                crate::secrets::save_identity(&dir, &a).unwrap();
                core.load_key().unwrap();
            });
            assert_eq!(kr.calls(), 0, "file backend must be used when Secret Service is absent");
            assert!(dir.join("identity-store").join("identity.key").exists());
            assert!(core.has_identity());
            assert!(core.key_lost().is_none());
            assert_eq!(core.engine().unwrap().me().address(), a.address());
            assert_eq!(core.store.chat("peer.111111", None, None, 10).unwrap().len(), 1);
            cleanup(core, rt, dir);
        }
    }

    #[test]
    fn r5_guard_refuses_create_in_mismatch_state_without_wiping() {
        let (core, rt, dir) = make_core("guard-create");
        let a = Identity::generate("alice").unwrap();
        let b = Identity::generate("bob").unwrap();
        let fresh = Identity::generate("charlie").unwrap();
        seed_owner_data(&core.store, Some(&a.address()));
        let expected_err = format!(
            "This device's chats belong to {}. Use {}'s recovery words, link or key file, or choose Use {} here.",
            a.address(),
            a.address(),
            b.address()
        );
        let kr = MockKeyring::new();
        with_test_backend(SecretServiceState::Absent, kr, || {
            crate::secrets::save_identity(&dir, &b).unwrap();
            core.load_key().unwrap();
            let err = core
                .adopt_identity(fresh, true, &[("profile.name", "New User")])
                .unwrap_err();
            assert_eq!(err, expected_err);
            let crate::secrets::Loaded::Found(still) = crate::secrets::load_identity(&dir) else {
                panic!("key store must still hold B");
            };
            assert_eq!(still.address(), b.address());
        });
        assert!(!core.has_identity(), "engine must not start");
        let lost = core.key_lost().expect("mismatch state must stay set");
        assert_eq!(lost.address.as_deref(), Some(a.address().as_str()));
        assert_eq!(lost.found.as_deref(), Some(b.address().as_str()));
        assert_eq!(core.store.meta("identity.address").unwrap().as_deref(), Some(a.address().as_str()));
        assert_eq!(core.store.meta("profile.name").unwrap(), None, "no meta write before guard");
        assert_eq!(core.store.chat("peer.111111", None, None, 10).unwrap().len(), 1, "A's chats preserved");
        assert_eq!(core.store.draft("peer.111111").unwrap().as_deref(), Some("unsent draft"), "A's drafts preserved");
        assert_eq!(core.store.hubs().unwrap().len(), 1, "A's hubs preserved");
        cleanup(core, rt, dir);
    }

    #[test]
    fn r5_guard_refuses_non_owner_c_restore_in_mismatch_state_without_wiping_or_overwriting_b() {
        let (core, rt, dir) = make_core("guard-c");
        let a = Identity::generate("alice").unwrap();
        let b = Identity::generate("bob").unwrap();
        let c = Identity::generate("carol").unwrap();
        seed_owner_data(&core.store, Some(&a.address()));
        let expected_err = format!(
            "This device's chats belong to {}. Use {}'s recovery words, link or key file, or choose Use {} here.",
            a.address(),
            a.address(),
            b.address()
        );
        let kr = MockKeyring::new();
        with_test_backend(SecretServiceState::Absent, kr, || {
            crate::secrets::save_identity(&dir, &b).unwrap();
            core.load_key().unwrap();
            // Recovery words / key file / QR / link for C must be refused.
            let err_c = core
                .adopt_identity(c, false, &[("recovery.saved", "yes")])
                .unwrap_err();
            assert_eq!(err_c, expected_err);
            // Direct adopt_identity for B must also be refused (use_found_identity is the only path to B).
            let err_b = core
                .adopt_identity(b.clone(), false, &[("recovery.saved", "yes")])
                .unwrap_err();
            assert_eq!(err_b, expected_err);
            let crate::secrets::Loaded::Found(still) = crate::secrets::load_identity(&dir) else {
                panic!("key store must still hold B");
            };
            assert_eq!(still.address(), b.address());
        });
        assert!(!core.has_identity(), "engine must not start");
        let lost = core.key_lost().expect("mismatch state must stay set");
        assert_eq!(lost.address.as_deref(), Some(a.address().as_str()));
        assert_eq!(lost.found.as_deref(), Some(b.address().as_str()));
        assert_eq!(core.store.meta("identity.address").unwrap().as_deref(), Some(a.address().as_str()));
        assert_eq!(core.store.meta("recovery.saved").unwrap(), None, "no meta write before guard");
        assert_eq!(core.store.chat("peer.111111", None, None, 10).unwrap().len(), 1, "A's chats preserved");
        assert_eq!(core.store.draft("peer.111111").unwrap().as_deref(), Some("unsent draft"), "A's drafts preserved");
        assert_eq!(core.store.hubs().unwrap().len(), 1, "A's hubs preserved");
        cleanup(core, rt, dir);
    }

    #[test]
    fn r5_guard_accepts_owner_a_restore_keeps_chats_and_replaces_b_with_a() {
        let (core, rt, dir) = make_core("guard-a");
        let a = Identity::generate("alice").unwrap();
        let b = Identity::generate("bob").unwrap();
        let c = Identity::generate("carol").unwrap();
        seed_owner_data(&core.store, Some(&a.address()));
        let kr = MockKeyring::new();
        with_test_backend(SecretServiceState::Absent, kr, || {
            crate::secrets::save_identity(&dir, &b).unwrap();
            core.load_key().unwrap();
            // A refused C attempt first must leave mismatch state healthy for A recovery.
            assert!(core.adopt_identity(c, false, &[]).is_err());
            core.adopt_identity(a.clone(), false, &[("recovery.saved", "yes")])
                .expect("restoring A in mismatch state must succeed");
            let crate::secrets::Loaded::Found(now) = crate::secrets::load_identity(&dir) else {
                panic!("key store must now hold A");
            };
            assert_eq!(now.address(), a.address());
        });
        assert!(core.has_identity(), "engine started as A");
        assert_eq!(core.engine().unwrap().me().address(), a.address());
        assert!(core.key_lost().is_none(), "key_lost cleared");
        assert_eq!(core.store.meta("identity.address").unwrap().as_deref(), Some(a.address().as_str()));
        assert_eq!(core.store.meta("recovery.saved").unwrap().as_deref(), Some("yes"));
        assert_eq!(core.store.chat("peer.111111", None, None, 10).unwrap().len(), 1, "A's chats kept");
        assert_eq!(core.store.draft("peer.111111").unwrap().as_deref(), Some("unsent draft"), "A's drafts kept");
        assert_eq!(core.store.hubs().unwrap().len(), 1, "A's hubs kept");
        cleanup(core, rt, dir);

        // Outside mismatch state (key missing, found == None), adopting C still works as in 1.3.11.
        let (core2, rt2, dir2) = make_core("no-mismatch-c");
        let a2 = Identity::generate("alice").unwrap();
        let c2 = Identity::generate("carol").unwrap();
        seed_owner_data(&core2.store, Some(&a2.address()));
        let kr2 = MockKeyring::new();
        with_test_backend(SecretServiceState::Absent, kr2, || {
            core2.load_key().unwrap();
            assert!(core2.key_lost().unwrap().found.is_none());
            core2.adopt_identity(c2.clone(), false, &[]).expect("non-mismatch adopt C must succeed");
        });
        assert!(core2.has_identity());
        assert_eq!(core2.engine().unwrap().me().address(), c2.address());
        assert!(core2.store.chat("peer.111111", None, None, 10).unwrap().is_empty(), "non-mismatch must_wipe still wipes A");
        cleanup(core2, rt2, dir2);
    }
}

#[cfg(test)]
mod transfer_notice_tests {
    use super::{failure_notice, NotifySettings};
    use hubchat_core::store::TransferFailure;

    #[test]
    fn failure_notices_respect_settings_direction_and_privacy() {
        let settings = NotifySettings { enabled: true, preview: false, sound: true };
        let mut n = TransferFailure {
            message_id: "m".into(), peer: "peer".into(), episode: 0, attempts: 0,
            error: "secret local path and signed request URL".into(), download: false,
        };
        let upload = failure_notice(settings, "Pat", &n).unwrap();
        assert_eq!(upload.0, "Pat: file not sent");
        assert!(upload.1.contains("retry or remove"));
        assert!(!upload.1.contains("secret"));
        assert!(upload.2);
        let key = n.key();
        n.attempts = 1;
        assert!(!failure_notice(settings, "Pat", &n).unwrap().2);
        assert_eq!(n.key(), key);
        n.download = true;
        let download = failure_notice(settings, "Pat", &n).unwrap();
        assert_eq!(download.0, "Pat: download failed");
        assert!(download.1.contains("retry"));
        assert!(!download.1.contains("remove"));
        assert!(failure_notice(NotifySettings { enabled: false, ..settings }, "Pat", &n).is_none());
        assert!(!failure_notice(NotifySettings { sound: false, ..settings }, "Pat", &n).unwrap().2);
    }
}

#[cfg(test)]
mod do_not_disturb_tests {
    use super::{failure_notice, notify_now, NotifySettings};
    use hubchat_core::store::TransferFailure;

    /// On Do not disturb (user 2026-10-31 08:52Z): no notification and no
    /// sound, of a message or a failed file; the settings themselves stay.
    #[test]
    fn do_not_disturb_shows_no_notification_and_plays_no_sound() {
        let settings = NotifySettings { enabled: true, preview: true, sound: true };
        let now = notify_now(settings, true);
        assert_eq!((now.enabled, now.sound, now.preview), (false, false, true), "nothing shown or heard");
        let off = notify_now(settings, false);
        assert_eq!((off.enabled, off.sound, off.preview), (true, true, true), "otherwise the settings as they are");
        let n = TransferFailure {
            message_id: "m".into(), peer: "peer".into(), episode: 0, attempts: 0, error: String::new(), download: false,
        };
        assert!(failure_notice(notify_now(settings, true), "Pat", &n).is_none(), "a failed file: no notice either");
        assert!(failure_notice(notify_now(settings, false), "Pat", &n).is_some());
    }
}

/// Covers local preparation before a durable queued row exists.
pub struct FileAdmission(Option<&'static Core>);
impl Drop for FileAdmission {
    fn drop(&mut self) {
        if let Some(c)=self.0 {
            c.file_admissions.fetch_sub(1,Ordering::SeqCst);
            c.refresh_file_lifetime();
        }
    }
}
fn background_policy(visible:bool, coverage:Option<bool>, push:bool, checks:usize)->(bool,bool) {
    match coverage {
        Some(covered)=>(push&&!visible&&!covered&&checks==0,!visible&&!covered),
        None=>(push&&!visible&&checks==0,false),
    }
}
#[cfg(test)]
mod file_lifecycle_tests {
    use super::background_policy;
    #[test]
    fn background_checks_never_authorize_files_and_stay_connected_remains_covered() {
        for push in [false,true] {
            assert_eq!(background_policy(false,Some(true),push,0),(false,false));
            assert_eq!(background_policy(false,Some(false),push,0),(push,true));
            assert_eq!(background_policy(false,Some(false),push,1),(false,true));
            assert_eq!(background_policy(true,Some(false),push,0),(false,false));
        }
        assert_eq!(background_policy(false,None,false,0),(false,false));
    }
}

/// THE TRAY'S CHAT LIST FROZE HUBCHAT (measured 2026-11-26 from a hang dump of 1.4.0-alpha.3, docket
/// hubchat-freezes-about-half-the-time-when-a-conta): choosing a contact ran `tray_recent::open_chat`'s
/// identity section, whose window show queued `tray_recent::sync`; Tauri runs `run_on_main_thread` inline on
/// the main thread, so sync's own section waited on the lock its own thread held, forever.
#[cfg(all(test, windows))]
mod tray_identity_tests {
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex, OnceLock};
    use std::time::Duration;

    use hubchat_core::engine::Engine;
    use hubchat_core::store::Store;
    use hubchat_core::Identity;

    use super::{Core, Platform, ShellHost};

    struct StubPlatform(PathBuf);
    impl Platform for StubPlatform {
        fn notify(&self, _: &str, _: &str, _: &str, _: bool) {}
        fn status(&self, _: &str) {}
        fn open_source(&self, s: &str) -> std::io::Result<std::fs::File> {
            std::fs::File::open(s)
        }
        fn download_dir(&self) -> PathBuf {
            self.0.join("downloads")
        }
    }

    /// A signed-in core whose engine is never started: no hub, no key store, no network. Leaked, so a test
    /// thread that hangs can be left behind while the test reports.
    fn signed_in(label: &str) -> &'static Core {
        let dir = std::env::temp_dir().join(format!("hubchat-tray-identity-{label}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let store = Arc::new(Store::open(&dir.join("hubchat.sqlite3")).unwrap());
        let rt = Box::leak(Box::new(tokio::runtime::Builder::new_multi_thread().worker_threads(1).enable_all().build().unwrap()));
        let host = Arc::new(ShellHost {
            staging: dir.join("outbox"),
            app: OnceLock::new(),
            platform: Arc::new(StubPlatform(dir.clone())),
            foreground: AtomicBool::new(false),
            open_chat: Mutex::new(None),
            store: store.clone(),
            notify_on: AtomicBool::new(false),
            notify_text: AtomicBool::new(false),
            notify_sound: AtomicBool::new(false),
        });
        let core: &'static Core = Box::leak(Box::new(Core {
            dir,
            store,
            engine: Mutex::new(None),
            identity_gate: Default::default(),
            background_checks: Mutex::new(0),
            file_admissions: std::sync::atomic::AtomicUsize::new(0),
            file_lifetime: Mutex::new(()),
            rt: rt.handle().clone(),
            host,
            key_lost: Mutex::new(None),
            identity_load: Default::default(),
            shortcut_projection: Mutex::new(None),
        }));
        let me = Identity::generate("alice").unwrap();
        core.store.set_meta("identity.address", &me.address()).unwrap();
        let profile = core.profile_from_store(&me);
        *core.engine.lock().unwrap() = Some(Engine::new(core.store.clone(), me, profile, core.host.clone()));
        core
    }

    /// `f` on its own thread; None when it hasn't finished in 5 s (it hung)
    fn within<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> Option<T> {
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || { let _ = tx.send(f()); });
        rx.recv_timeout(Duration::from_secs(5)).ok()
    }

    #[test]
    fn a_tray_section_entered_again_on_its_own_thread_finishes() {
        let core = signed_in("nested");
        let owner = core.engine().unwrap().me().address();
        let expected = owner.clone();
        // the chain the dump showed: open_chat's section -> show -> sync's section, plus sync's fallback read
        let done = within(move || core.tray_with_identity(Some(&owner), |outer| {
            let inner = core.tray_with_identity(None, |inner| Ok(inner.to_owned()))?;
            let engine = core.engine()?.me().address();
            Ok((outer.to_owned(), inner, engine))
        }));
        let (outer, inner, engine) = done.expect("a tray section inside another on the same thread hung").unwrap();
        assert_eq!((outer.as_str(), inner.as_str(), engine.as_str()), (expected.as_str(), expected.as_str(), expected.as_str()));
    }

    /// POSITIVE CONTROL: another thread still waits for a section to end (what keeps identity adoption and
    /// sign-out out of a tray read or action)
    #[test]
    fn another_thread_waits_for_a_tray_section_to_end() {
        let core = signed_in("exclusive");
        let (entered_tx, entered_rx) = std::sync::mpsc::channel();
        let first_done = Arc::new(AtomicBool::new(false));
        let flag = first_done.clone();
        let first = std::thread::spawn(move || core.tray_with_identity(None, |_| {
            entered_tx.send(()).unwrap();
            std::thread::sleep(Duration::from_millis(300));
            flag.store(true, Ordering::SeqCst);
            Ok(())
        }));
        entered_rx.recv_timeout(Duration::from_secs(5)).expect("the first section started");
        let saw = within(move || core.tray_with_identity(None, |_| Ok(first_done.load(Ordering::SeqCst))))
            .expect("the second section ran once the first ended").unwrap();
        assert!(saw, "the second thread's section ran while the first was still inside its own");
        first.join().unwrap().unwrap();
    }
}
