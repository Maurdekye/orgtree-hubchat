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
use crate::secrets::{
    FallbackCleanup, FallbackFileStamp, ForgetResult, KeyStorage, Loaded, SaveStageError,
};

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
    file_admissions: std::sync::atomic::AtomicUsize,
    file_lifetime: Mutex<()>,
    pub rt: tokio::runtime::Handle,
    host: Arc<ShellHost>,
    /// Set while this device had an identity but its key can't be found.
    key_lost: Mutex<Option<KeyLost>>,
    /// Where this device's active identity key is stored (None when signed out / lost).
    key_storage: Mutex<Option<KeyStorage>>,
    /// True when a Linux permissions-only fallback key file could not be removed
    /// after a keyring save/load/forget and still remains on disk (H4).
    key_file_residual: Mutex<bool>,
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
}

const META_FORGOTTEN: &str = "key.forgotten";
const META_ADOPTING: &str = "key.adopting";
const META_FILE_CLEANUP_PENDING: &str = "key.file_cleanup_pending";
const META_INSTALLATION_WIPED: &str = "key.installation_wiped";

/// One identity explicitly forgotten (Start over or sign-out) while the OS
/// keyring could not yet be reached, so a returning keyring cannot sign it
/// back in (H3).
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
struct ForgetTombstone {
    address: String,
    at: String,
}

/// Staged identity adoption written before the credential save so an
/// interruption or database error between save and commit recovers cleanly on
/// next launch without stranding the key or leaking old chats (M6).
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
struct PendingAdoption {
    address: String,
    wipe: bool,
    #[serde(default)]
    brand_new: bool,
    #[serde(default)]
    meta: Vec<(String, String)>,
}

/// Durable record of a Linux permissions-only fallback file awaiting cleanup
/// (H4). Distinguishes explicit Sign out / Start over (`signed_out = true`,
/// eligible for retry on signed-out startup when no owner or adoption exists)
/// from active keyring-session cleanup failures (`signed_out = false`, retried
/// only after a verified keyring identity is accepted or adopted).
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
struct PendingFileCleanup {
    target: String,
    signed_out: bool,
    #[serde(default)]
    file_stamp: Option<FallbackFileStamp>,
    #[serde(default)]
    recorded_at_sec: Option<i64>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct StoreOwnership {
    owner: Option<String>,
    had_identity: bool,
    /// Durable marker set on Start over / Sign out and preserved across all
    /// subsequent adoptions and wipes, so a previously wiped installation never
    /// falls back to the legacy ownerless auto-accept rule (H3).
    installation_wiped: bool,
    tombstones: Vec<ForgetTombstone>,
    adopting: Option<PendingAdoption>,
    cleanup_pending: Option<PendingFileCleanup>,
}

fn unix_now_sec() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn parse_pending_cleanup(raw: Option<&str>) -> Result<Option<PendingFileCleanup>, String> {
    let Some(s) = raw.filter(|s| !s.trim().is_empty()) else {
        return Ok(None);
    };
    if s.trim_start().starts_with('{') {
        serde_json::from_str(s)
            .map(Some)
            .map_err(|e| format!("invalid {META_FILE_CLEANUP_PENDING} metadata: {e}"))
    } else {
        // Legacy plain-string marker from earlier dev builds: treat as non-signed-out
        // (fail-safe: never deletes without a verified active keyring identity).
        Ok(Some(PendingFileCleanup {
            target: s.to_owned(),
            signed_out: false,
            file_stamp: None,
            recorded_at_sec: None,
        }))
    }
}

fn save_pending_cleanup(store: &Store, pending: &PendingFileCleanup) -> Result<(), String> {
    let json = serde_json::to_string(pending).map_err(|e| e.to_string())?;
    store
        .set_meta(META_FILE_CLEANUP_PENDING, &json)
        .map_err(|e| e.to_string())
}

fn parse_tombstones(raw: Option<&str>) -> Result<Vec<ForgetTombstone>, String> {
    let Some(s) = raw.filter(|s| !s.trim().is_empty()) else {
        return Ok(Vec::new());
    };
    serde_json::from_str(s).map_err(|e| format!("invalid {META_FORGOTTEN} metadata: {e}"))
}

fn format_tombstones(list: &[ForgetTombstone]) -> Option<String> {
    if list.is_empty() {
        None
    } else {
        serde_json::to_string(list).ok()
    }
}

fn add_tombstone(list: &[ForgetTombstone], address: &str, at: String) -> Vec<ForgetTombstone> {
    let mut out: Vec<ForgetTombstone> = list
        .iter()
        .filter(|t| t.address != address)
        .cloned()
        .collect();
    out.push(ForgetTombstone {
        address: address.to_owned(),
        at,
    });
    out
}

fn remove_tombstones_matching(list: &[ForgetTombstone], remove_addrs: &[&str]) -> Vec<ForgetTombstone> {
    list.iter()
        .filter(|t| !remove_addrs.contains(&t.address.as_str()))
        .cloned()
        .collect()
}

fn parse_pending_adoption(raw: Option<&str>) -> Result<Option<PendingAdoption>, String> {
    let Some(s) = raw.filter(|s| !s.trim().is_empty()) else {
        return Ok(None);
    };
    serde_json::from_str(s)
        .map(Some)
        .map_err(|e| format!("invalid {META_ADOPTING} metadata: {e}"))
}

trait MetaReader {
    fn read_meta(&self, key: &str) -> Result<Option<String>, String>;
}

impl MetaReader for Store {
    fn read_meta(&self, key: &str) -> Result<Option<String>, String> {
        self.meta(key).map_err(|e| e.to_string())
    }
}

/// Read store ownership and key-lifecycle metadata, propagating every SQLite
/// read error instead of treating a broken database as missing metadata (M6).
fn read_store_ownership(store: &impl MetaReader) -> Result<StoreOwnership, String> {
    let owner = store.read_meta("identity.address")?.filter(|v| !v.is_empty());
    let device_id = store.read_meta("device.id")?.filter(|v| !v.is_empty());
    let installation_wiped = store
        .read_meta(META_INSTALLATION_WIPED)?
        .is_some_and(|v| !v.is_empty());
    let raw_forgotten = store.read_meta(META_FORGOTTEN)?;
    let raw_adopting = store.read_meta(META_ADOPTING)?;
    let raw_cleanup = store.read_meta(META_FILE_CLEANUP_PENDING)?;
    Ok(StoreOwnership {
        had_identity: owner.is_some() || device_id.is_some(),
        owner,
        installation_wiped,
        tombstones: parse_tombstones(raw_forgotten.as_deref())?,
        adopting: parse_pending_adoption(raw_adopting.as_deref())?,
        cleanup_pending: parse_pending_cleanup(raw_cleanup.as_deref())?,
    })
}

/// Whether adopting `new` must first remove this device's local data: never
/// when there is none; when it belongs to another known identity; and, when
/// its owner is unknown (kept before Hubchat noted it), only for a brand-new
/// identity, which can't be the old one.
fn must_wipe(owner: Option<&str>, has_data: bool, new: &str, brand_new: bool) -> bool {
    has_data && owner.map_or(brand_new, |o| o != new)
}

/// Whether an identity loaded from `storage` may start against the local store.
#[derive(Clone, Debug, PartialEq, Eq)]
enum LoadedDecision {
    Accept,
    /// Complete an interrupted adoption whose credential was already saved (M6).
    RecoverAdoption(PendingAdoption),
    /// Returning keyring identity matches a forget tombstone: refuse and clear keyring (H3).
    ForgottenKeyring,
    /// Keyring or fallback file disagrees with local store owner: show LostKey without deleting either key (H2).
    RejectMismatch { owner: Option<String> },
    /// Permissions-only fallback file without local identity data, or an
    /// unadopted keyring entry on a previously wiped installation: refuse without
    /// auto-starting and leave user on setup/choose-identity screen.
    IgnoreOrphan,
}

fn has_matching_tombstone(
    tombstones: &[ForgetTombstone],
    identity_addr: &str,
    had_identity: bool,
) -> bool {
    tombstones.iter().any(|t| t.address == identity_addr)
        || (!had_identity && tombstones.iter().any(|t| t.address == "*"))
}

fn decide_loaded_identity(
    storage: KeyStorage,
    identity_addr: &str,
    ownership: &StoreOwnership,
) -> LoadedDecision {
    if let Some(pending) = &ownership.adopting {
        if pending.address == identity_addr {
            return LoadedDecision::RecoverAdoption(pending.clone());
        }
    }
    match storage {
        KeyStorage::Keyring => {
            if has_matching_tombstone(&ownership.tombstones, identity_addr, ownership.had_identity) {
                return LoadedDecision::ForgottenKeyring;
            }
            if let Some(owner) = &ownership.owner {
                if owner != identity_addr {
                    return LoadedDecision::RejectMismatch {
                        owner: Some(owner.clone()),
                    };
                }
                return LoadedDecision::Accept;
            }
            // No recorded owner: a previously wiped installation (Start over /
            // Sign out) NEVER falls back to the untouched-legacy auto-accept
            // rule for an un-readopted wallet identity (H3).
            if ownership.installation_wiped {
                return if ownership.had_identity {
                    LoadedDecision::RejectMismatch { owner: None }
                } else {
                    LoadedDecision::IgnoreOrphan
                };
            }
            // Untouched legacy ownerless + tombstoneless profile -> Accept.
            LoadedDecision::Accept
        }
        KeyStorage::FilePermissionsOnly => {
            if has_matching_tombstone(&ownership.tombstones, identity_addr, ownership.had_identity) {
                return LoadedDecision::IgnoreOrphan;
            }
            match ownership.owner.as_deref() {
                Some(owner) if owner == identity_addr => LoadedDecision::Accept,
                Some(owner) => LoadedDecision::RejectMismatch {
                    owner: Some(owner.to_owned()),
                },
                None if ownership.had_identity => LoadedDecision::RejectMismatch { owner: None },
                None => LoadedDecision::IgnoreOrphan,
            }
        }
    }
}

trait LifecycleSecrets {
    fn load(&self) -> Loaded;
    fn confirm_loaded_keyring(&self, me: &Identity, restored: bool);
    fn save_staged(&self, me: &Identity) -> Result<KeyStorage, SaveStageError>;
    fn forget(&self) -> Result<ForgetResult, String>;
    fn forget_forgotten_keyring(&self, forgotten_addr: &str) -> Result<bool, String>;
    fn fallback_file_stamp(&self) -> Option<FallbackFileStamp>;
    fn cleanup_fallback(
        &self,
        allowed_addrs: &[&str],
        allow_any: bool,
        expected_stamp: Option<FallbackFileStamp>,
        not_newer_than_sec: Option<i64>,
    ) -> FallbackCleanup;
}

struct RealSecrets<'a>(&'a std::path::Path);

impl LifecycleSecrets for RealSecrets<'_> {
    fn load(&self) -> Loaded {
        crate::secrets::load_identity(self.0)
    }
    fn confirm_loaded_keyring(&self, me: &Identity, restored: bool) {
        crate::secrets::confirm_loaded_keyring(self.0, me, restored)
    }
    fn save_staged(&self, me: &Identity) -> Result<KeyStorage, SaveStageError> {
        crate::secrets::save_identity_staged(self.0, me)
    }
    fn forget(&self) -> Result<ForgetResult, String> {
        crate::secrets::forget_identity(self.0)
    }
    fn forget_forgotten_keyring(&self, forgotten_addr: &str) -> Result<bool, String> {
        crate::secrets::forget_forgotten_keyring(self.0, forgotten_addr)
    }
    fn fallback_file_stamp(&self) -> Option<FallbackFileStamp> {
        crate::secrets::fallback_file_stamp(self.0)
    }
    fn cleanup_fallback(
        &self,
        allowed_addrs: &[&str],
        allow_any: bool,
        expected_stamp: Option<FallbackFileStamp>,
        not_newer_than_sec: Option<i64>,
    ) -> FallbackCleanup {
        crate::secrets::cleanup_fallback_file(
            self.0,
            allowed_addrs,
            allow_any,
            expected_stamp,
            not_newer_than_sec,
        )
    }
}

struct LoadPlan {
    start: Option<(Identity, KeyStorage)>,
    key_lost: Option<KeyLost>,
    residual: bool,
}

struct ForgetCommit {
    res: ForgetResult,
}

/// Retry fallback cleanup on a signed-out/missing startup path ONLY when:
/// 1. A durable sign-out cleanup record (`signed_out == true`) exists,
/// 2. The profile does NOT currently own an identity or local identity data, AND
/// 3. No identity adoption is currently staged (protects a pending adoption's file).
/// Passes the exact target and file stamp so a credential newer than the
/// cleanup target is never touched (H4).
fn retry_signed_out_cleanup(
    store: &Store,
    secrets: &impl LifecycleSecrets,
    ownership: &StoreOwnership,
) -> bool {
    let Some(pending) = &ownership.cleanup_pending else {
        return false;
    };
    if !pending.signed_out
        || ownership.had_identity
        || ownership.owner.is_some()
        || ownership.adopting.is_some()
    {
        return true;
    }
    let allow_any = pending.target == "*";
    let allowed: Vec<&str> = if allow_any {
        vec![]
    } else {
        vec![pending.target.as_str()]
    };
    match secrets.cleanup_fallback(&allowed, allow_any, pending.file_stamp, pending.recorded_at_sec) {
        FallbackCleanup::NoneExisted | FallbackCleanup::Removed => {
            let _ = store.delete_meta(META_FILE_CLEANUP_PENDING);
            false
        }
        FallbackCleanup::Residual => true,
    }
}

fn run_fallback_cleanup(
    store: &Store,
    secrets: &impl LifecycleSecrets,
    active_addr: &str,
    replaced_owner: Option<&str>,
    pending: Option<&PendingFileCleanup>,
) -> bool {
    let allow_any = pending.is_some_and(|p| p.target == "*");
    let (expected_stamp, not_newer_than_sec) = if allow_any {
        pending.map_or((None, None), |p| (p.file_stamp, p.recorded_at_sec))
    } else {
        (None, None)
    };
    let mut allowed: Vec<&str> = vec![active_addr];
    if let Some(old) = replaced_owner {
        if !old.is_empty() && !allowed.contains(&old) {
            allowed.push(old);
        }
    }
    if let Some(p) = pending {
        if p.target != "*" && !p.target.is_empty() && !allowed.contains(&p.target.as_str()) {
            allowed.push(p.target.as_str());
        }
    }
    match secrets.cleanup_fallback(&allowed, allow_any, expected_stamp, not_newer_than_sec) {
        FallbackCleanup::NoneExisted | FallbackCleanup::Removed => {
            if pending.is_some() {
                let _ = store.delete_meta(META_FILE_CLEANUP_PENDING);
            }
            false
        }
        FallbackCleanup::Residual => {
            let next_target = if allow_any {
                "*"
            } else if let Some(old) = replaced_owner.filter(|s| !s.is_empty()) {
                old
            } else if let Some(p) = pending.filter(|p| !p.target.is_empty()) {
                p.target.as_str()
            } else {
                active_addr
            };
            // Active keyring session: `signed_out` is `false` so a later empty
            // wallet while this profile is still owned never deletes the VR copy (H4).
            let _ = save_pending_cleanup(
                store,
                &PendingFileCleanup {
                    target: next_target.to_owned(),
                    signed_out: false,
                    file_stamp: None,
                    recorded_at_sec: None,
                },
            );
            true
        }
    }
}

fn lifecycle_load(
    dir: &std::path::Path,
    store: &Store,
    secrets: &impl LifecycleSecrets,
    on_forget_push: impl FnOnce(),
) -> Result<LoadPlan, String> {
    let ownership = read_store_ownership(store)?;
    let loaded = secrets.load();
    let (me, storage, restored) = match loaded {
        Loaded::Found { identity, storage } => (identity, storage, false),
        Loaded::Restored { identity, storage } => (identity, storage, true),
        Loaded::Missing => {
            let residual = retry_signed_out_cleanup(store, secrets, &ownership);
            return Ok(LoadPlan {
                start: None,
                key_lost: ownership.had_identity.then(|| KeyLost {
                    address: ownership.owner,
                    unreadable: false,
                }),
                residual,
            });
        }
        Loaded::Unreadable => {
            let residual = retry_signed_out_cleanup(store, secrets, &ownership);
            return Ok(LoadPlan {
                start: None,
                key_lost: ownership.had_identity.then(|| KeyLost {
                    address: ownership.owner,
                    unreadable: true,
                }),
                residual,
            });
        }
    };
    let addr = me.address();
    match decide_loaded_identity(storage, &addr, &ownership) {
        LoadedDecision::Accept => {
            if storage == KeyStorage::Keyring {
                secrets.confirm_loaded_keyring(&me, restored);
            }
            if ownership.owner.as_deref() != Some(addr.as_str()) {
                store
                    .set_meta("identity.address", &addr)
                    .map_err(|e| e.to_string())?;
            }
            if restored {
                store.set_meta("key.restored", "yes").map_err(|e| e.to_string())?;
            }
            let residual = if storage == KeyStorage::Keyring {
                run_fallback_cleanup(store, secrets, &addr, None, ownership.cleanup_pending.as_ref())
            } else {
                false
            };
            Ok(LoadPlan {
                start: Some((me, storage)),
                key_lost: None,
                residual,
            })
        }
        LoadedDecision::RecoverAdoption(pending) => {
            if storage == KeyStorage::Keyring {
                secrets.confirm_loaded_keyring(&me, restored);
            }
            let mut meta_refs: Vec<(&str, &str)> = pending
                .meta
                .iter()
                .map(|(k, v)| (k.as_str(), v.as_str()))
                .collect();
            if restored {
                meta_refs.push(("key.restored", "yes"));
            }
            let updated_forgotten = format_tombstones(&remove_tombstones_matching(
                &ownership.tombstones,
                &[addr.as_str(), "*"],
            ));
            let (did_wipe, replaced_owner) = store
                .commit_adopted_identity(
                    pending.wipe,
                    pending.brand_new,
                    &addr,
                    &meta_refs,
                    updated_forgotten.as_deref(),
                )
                .map_err(|e| e.to_string())?;
            if did_wipe {
                on_forget_push();
                keylog::note(
                    dir,
                    "another identity's local data was removed before taking this one",
                    &[Fact::OtherLocalDataRemoved],
                );
            }
            keylog::note(
                dir,
                "completed an interrupted identity change at startup",
                &[Fact::AdoptionRecovered],
            );
            let residual = if storage == KeyStorage::Keyring {
                run_fallback_cleanup(
                    store,
                    secrets,
                    &addr,
                    replaced_owner.as_deref(),
                    ownership.cleanup_pending.as_ref(),
                )
            } else {
                if ownership.cleanup_pending.is_some() {
                    let _ = store.delete_meta(META_FILE_CLEANUP_PENDING);
                }
                false
            };
            Ok(LoadPlan {
                start: Some((me, storage)),
                key_lost: None,
                residual,
            })
        }
        LoadedDecision::ForgottenKeyring => {
            // Never refresh/restore a forgotten backup; remove the matching
            // keyring entry (and any matching Windows DPAPI backup) while
            // keeping the forget tombstone durable across wallets (H3).
            match secrets.forget_forgotten_keyring(&addr) {
                Ok(true) => {
                    keylog::note(
                        dir,
                        "keyring held a previously forgotten identity; removed from keyring",
                        &[Fact::ForgottenKeyCleared],
                    );
                }
                Ok(false) => {}
                Err(_) => {
                    keylog::note(
                        dir,
                        "keyring held a previously forgotten identity that couldn't be removed; refused",
                        &[Fact::ForgottenKeyCleanupFailed],
                    );
                }
            }
            let residual = retry_signed_out_cleanup(store, secrets, &ownership);
            Ok(LoadPlan {
                start: None,
                key_lost: ownership.had_identity.then(|| KeyLost {
                    address: ownership.owner,
                    unreadable: false,
                }),
                residual,
            })
        }
        LoadedDecision::RejectMismatch { owner } => {
            if storage == KeyStorage::Keyring {
                keylog::note(
                    dir,
                    "keyring holds a different identity than this device's data; refused",
                    &[Fact::KeyringOwnerMismatch],
                );
            } else {
                keylog::note(
                    dir,
                    "permissions-only key file holds a different identity than this device's data; refused",
                    &[Fact::KeyFileOwnerMismatch],
                );
            }
            Ok(LoadPlan {
                start: None,
                key_lost: Some(KeyLost {
                    address: owner,
                    unreadable: false,
                }),
                residual: ownership.cleanup_pending.is_some(),
            })
        }
        LoadedDecision::IgnoreOrphan => {
            if storage == KeyStorage::Keyring {
                keylog::note(
                    dir,
                    "keyring holds an unadopted identity on a previously wiped installation; refused",
                    &[Fact::KeyringOwnerMismatch],
                );
            } else {
                keylog::note(
                    dir,
                    "permissions-only key file ignored without local identity data",
                    &[Fact::KeyFileOrphanIgnored],
                );
            }
            let residual = retry_signed_out_cleanup(store, secrets, &ownership);
            Ok(LoadPlan {
                start: None,
                key_lost: None,
                residual,
            })
        }
    }
}

fn lifecycle_adopt(
    dir: &std::path::Path,
    store: &Store,
    secrets: &impl LifecycleSecrets,
    me: &Identity,
    brand_new: bool,
    meta: &[(&str, &str)],
    on_forget_push: impl FnOnce(),
) -> Result<(KeyStorage, bool), String> {
    let ownership = read_store_ownership(store)?;
    let addr = me.address();
    let wipe_needed = must_wipe(ownership.owner.as_deref(), ownership.had_identity, &addr, brand_new);
    let pending = PendingAdoption {
        address: addr.clone(),
        wipe: wipe_needed,
        brand_new,
        meta: meta.iter().map(|(k, v)| ((*k).to_owned(), (*v).to_owned())).collect(),
    };
    let prev_adopting = store.meta(META_ADOPTING).map_err(|e| e.to_string())?;
    let pending_json = serde_json::to_string(&pending).map_err(|e| e.to_string())?;
    store.set_meta(META_ADOPTING, &pending_json).map_err(|e| e.to_string())?;

    let storage = match secrets.save_staged(me) {
        Ok(s) => s,
        Err(SaveStageError::NotPublished(e)) => {
            match prev_adopting.as_deref() {
                Some(prev) => {
                    let _ = store.set_meta(META_ADOPTING, prev);
                }
                None => {
                    let _ = store.delete_meta(META_ADOPTING);
                }
            }
            return Err(e);
        }
        Err(SaveStageError::Uncertain(e)) => {
            // Publication may have already occurred (e.g. `renameat` succeeded
            // before `fsync_dir` failed, or a keyring `set_password` reply was
            // lost); preserve `META_ADOPTING` so the next launch recovers (M6).
            return Err(e);
        }
    };

    let updated_forgotten = format_tombstones(&remove_tombstones_matching(
        &ownership.tombstones,
        &[addr.as_str(), "*"],
    ));
    let (did_wipe, replaced_owner) = store
        .commit_adopted_identity(
            wipe_needed,
            brand_new,
            &addr,
            meta,
            updated_forgotten.as_deref(),
        )
        .map_err(|e| e.to_string())?;
    if did_wipe {
        on_forget_push();
        keylog::note(
            dir,
            "another identity's local data was removed before taking this one",
            &[Fact::OtherLocalDataRemoved],
        );
    }

    let residual = if storage == KeyStorage::Keyring {
        run_fallback_cleanup(
            store,
            secrets,
            &addr,
            replaced_owner.as_deref(),
            ownership.cleanup_pending.as_ref(),
        )
    } else {
        if ownership.cleanup_pending.is_some() {
            let _ = store.delete_meta(META_FILE_CLEANUP_PENDING);
        }
        false
    };

    keylog::note(
        dir,
        if brand_new { "key saved: a new identity" } else { "key saved: an identity brought to this device" },
        &[if brand_new { Fact::KeySavedNew } else { Fact::KeySavedBrought }],
    );
    Ok((storage, residual))
}

fn lifecycle_forget_begin(
    store: &Store,
    secrets: &impl LifecycleSecrets,
    live_addr: Option<&str>,
) -> Result<ForgetCommit, String> {
    let ownership = read_store_ownership(store)?;
    let target_addr = live_addr
        .map(str::to_owned)
        .or_else(|| ownership.owner.clone())
        .or_else(|| ownership.had_identity.then(|| "*".to_owned()));
    let prev_forgotten = format_tombstones(&ownership.tombstones);
    let prev_wiped = ownership.installation_wiped;
    if let Some(addr) = target_addr.as_deref() {
        let updated = add_tombstone(&ownership.tombstones, addr, hubchat_core::engine::now());
        if let Some(json) = format_tombstones(&updated) {
            store.set_meta(META_FORGOTTEN, &json).map_err(|e| e.to_string())?;
        }
    }
    store
        .set_meta(META_INSTALLATION_WIPED, "1")
        .map_err(|e| e.to_string())?;
    let res = match secrets.forget() {
        Ok(r) => r,
        Err(e) => {
            match prev_forgotten.as_deref() {
                Some(json) => {
                    let _ = store.set_meta(META_FORGOTTEN, json);
                }
                None => {
                    let _ = store.delete_meta(META_FORGOTTEN);
                }
            }
            if !prev_wiped {
                let _ = store.delete_meta(META_INSTALLATION_WIPED);
            }
            return Err(e);
        }
    };
    if res.residual_plaintext {
        let pending = PendingFileCleanup {
            target: target_addr.unwrap_or_else(|| "*".to_owned()),
            signed_out: true,
            file_stamp: secrets.fallback_file_stamp(),
            recorded_at_sec: Some(unix_now_sec()),
        };
        save_pending_cleanup(store, &pending)?;
    } else if ownership.cleanup_pending.is_some() {
        let _ = store.delete_meta(META_FILE_CLEANUP_PENDING);
    }
    Ok(ForgetCommit { res })
}

fn lifecycle_forget_finish(store: &Store, _commit: &ForgetCommit) -> Result<(), String> {
    // Wipe local identity data while retaining device key-lifecycle metadata
    // (`META_FORGOTTEN` and `META_FILE_CLEANUP_PENDING`). Forget tombstones
    // remain durable across empty (`NoEntry`) and distinct wallets until the
    // user explicitly adopts that identity (H3).
    store.wipe().map_err(|e| e.to_string())?;
    Ok(())
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
    #[cfg(target_os = "linux")]
    crate::secrets::ensure_private_app_dir(&dir);
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
        background_checks: Mutex::new(0),
        file_admissions: std::sync::atomic::AtomicUsize::new(0),
        file_lifetime: Mutex::new(()),
        rt,
        host,
        key_lost: Mutex::new(None),
        key_storage: Mutex::new(None),
        key_file_residual: Mutex::new(false),
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
        let current = self.engine.lock().unwrap();
        let owner = current.as_ref().ok_or("No identity")?.me().address();
        if expected.is_some_and(|e| e != owner) || self.store.meta("identity.address").map_err(|e|e.to_string())?.as_deref() != Some(owner.as_str()) {
            return Err("Identity changed".into());
        }
        f(&owner)
    }

    /// Resolve and publish one projection while identity adoption is excluded.
    /// Publishers do only local OS work and must never call back into the core.
    #[cfg(any(windows, target_os = "android"))]
    pub fn with_shortcut_data<T>(&self, open_only: bool, limit: Option<u32>, publish: impl FnOnce(&str, Vec<hubchat_core::store::ShortcutContact>) -> Result<T, String>) -> Result<T, String> {
        let current = self.engine.lock().unwrap();
        let owner = current.as_ref().map(|e|e.me().address()).unwrap_or_default();
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
        let secrets = RealSecrets(&self.dir);
        let plan = lifecycle_load(&self.dir, &self.store, &secrets, || self.forget_push())?;
        if let Some((me, storage)) = plan.start {
            self.start_with(me, storage, plan.residual)?;
            return Ok(true);
        }
        *self.key_storage.lock().unwrap() = None;
        *self.key_lost.lock().unwrap() = plan.key_lost;
        *self.key_file_residual.lock().unwrap() = plan.residual;
        self.host.platform.status("Set up Hubchat to connect");
        // Keep the lost-key UI available while a later cold attach retries the key.
        Ok(false)
    }

    fn start_with(&self, me: Identity, storage: KeyStorage, residual: bool) -> Result<(), String> {
        self.note_owner(&me)?;
        *self.key_lost.lock().unwrap() = None;
        self.start_engine(me)?;
        *self.key_storage.lock().unwrap() = Some(storage);
        *self.key_file_residual.lock().unwrap() = residual;
        Ok(())
    }

    pub fn key_lost(&self) -> Option<KeyLost> {
        self.key_lost.lock().unwrap().clone()
    }

    pub fn key_storage(&self) -> Option<KeyStorage> {
        *self.key_storage.lock().unwrap()
    }

    pub fn key_file_residual(&self) -> bool {
        *self.key_file_residual.lock().unwrap()
    }

    /// Note whose the local data is (lost-key screen; nothing inherits it).
    fn note_owner(&self, me: &Identity) -> Result<(), String> {
        let a = me.address();
        if self
            .store
            .meta("identity.address")
            .map_err(|e| e.to_string())?
            .as_deref()
            != Some(a.as_str())
        {
            self.store
                .set_meta("identity.address", &a)
                .map_err(|e| e.to_string())?;
        }
        Ok(())
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
        let secrets = RealSecrets(&self.dir);
        let commit = lifecycle_forget_begin(&self.store, &secrets, None)?;
        lifecycle_forget_finish(&self.store, &commit)?;
        *self.key_storage.lock().unwrap() = None;
        *self.key_lost.lock().unwrap() = None;
        *self.key_file_residual.lock().unwrap() = commit.res.residual_plaintext;
        keylog::note(&self.dir, "started over: the local data of the lost identity was removed", &[Fact::StartedOver]);
        Ok(())
    }

    /// Save a new or restored identity and start talking to the hubs.
    /// Leave the current identity on this device (switching to another):
    /// off the device lists of v2 hubs, connections stopped, key and local
    /// data forgotten. The address itself stays registered for its other
    /// devices.
    pub async fn leave_identity(&self) -> Result<(), String> {
        let Some(e) = self.engine.lock().unwrap().clone() else {
            return Ok(());
        };
        let addr = e.me().address();
        let secrets = RealSecrets(&self.dir);
        let commit = lifecycle_forget_begin(&self.store, &secrets, Some(&addr))?;
        let Some(e) = self.engine.lock().unwrap().take() else {
            return Ok(());
        };
        self.forget_push();
        *self.key_storage.lock().unwrap() = None;
        *self.key_file_residual.lock().unwrap() = commit.res.residual_plaintext;
        self.host.platform.shortcuts_changed();
        #[cfg(windows)]
        if let Some(app) = self.host.app.get() { crate::tray_recent::reset(app); }
        let e2 = e.clone();
        let _ = self
            .rt
            .spawn(async move { e2.sign_out_this_device().await })
            .await;
        e.shutdown();
        lifecycle_forget_finish(&self.store, &commit)?;
        keylog::note(&self.dir, "left this identity: key and local data forgotten", &[Fact::LeftIdentity]);
        self.host.platform.status("Switching identity");
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
        let secrets = RealSecrets(&self.dir);
        let (storage, residual) = lifecycle_adopt(
            &self.dir,
            &self.store,
            &secrets,
            &me,
            brand_new,
            meta,
            || self.forget_push(),
        )?;
        self.start_with(me, storage, residual)?;
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
        *self.engine.lock().unwrap() = Some(engine);
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
    use std::cell::{Cell, RefCell};
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    use hubchat_core::Identity;

    use super::{
        decide_loaded_identity, lifecycle_adopt, lifecycle_forget_begin, lifecycle_forget_finish,
        lifecycle_load, must_wipe, read_store_ownership, save_pending_cleanup, ForgetTombstone,
        LifecycleSecrets, LoadedDecision, MetaReader, PendingAdoption, PendingFileCleanup, Store,
        StoreOwnership, META_ADOPTING, META_FILE_CLEANUP_PENDING, META_FORGOTTEN,
        META_INSTALLATION_WIPED,
    };
    use crate::secrets::{
        FallbackCleanup, FallbackFileStamp, ForgetResult, KeyStorage, Loaded, SaveStageError,
    };

    static TEST_DIR_SEQ: AtomicU64 = AtomicU64::new(0);

    fn temp_dir(tag: &str) -> PathBuf {
        let n = TEST_DIR_SEQ.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("hubchat-core-{tag}-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn ownership(owner: Option<&str>, had_identity: bool) -> StoreOwnership {
        StoreOwnership {
            owner: owner.map(str::to_owned),
            had_identity,
            ..Default::default()
        }
    }

    #[derive(Clone, Copy, PartialEq, Eq)]
    enum Mode {
        Keyring,
        FileOnly,
    }

    struct FakeSecrets {
        mode: Cell<Mode>,
        keyring: RefCell<Option<Identity>>,
        backup: RefCell<Option<Identity>>,
        file: RefCell<Option<Identity>>,
        file_gen: Cell<u64>,
        save_err: RefCell<Option<SaveStageError>>,
        forget_keyring_err: RefCell<Option<String>>,
        file_cleanup_fails: Cell<bool>,
        restored: Cell<bool>,
        keyring_forget_calls: Cell<usize>,
        confirm_calls: Cell<usize>,
    }

    impl FakeSecrets {
        fn new(mode: Mode) -> Self {
            Self {
                mode: Cell::new(mode),
                keyring: RefCell::new(None),
                backup: RefCell::new(None),
                file: RefCell::new(None),
                file_gen: Cell::new(0),
                save_err: RefCell::new(None),
                forget_keyring_err: RefCell::new(None),
                file_cleanup_fails: Cell::new(false),
                restored: Cell::new(false),
                keyring_forget_calls: Cell::new(0),
                confirm_calls: Cell::new(0),
            }
        }

        fn set_file(&self, id: &Identity) {
            self.file_gen.set(self.file_gen.get() + 1);
            *self.file.borrow_mut() = Some(Identity::from_parts(id.id(), id.secret()).unwrap());
        }
    }

    impl LifecycleSecrets for FakeSecrets {
        fn load(&self) -> Loaded {
            match self.mode.get() {
                Mode::Keyring => match self.keyring.borrow().as_ref() {
                    Some(id) => {
                        let copy = Identity::from_parts(id.id(), id.secret()).unwrap();
                        if self.restored.get() {
                            Loaded::Restored {
                                identity: copy,
                                storage: KeyStorage::Keyring,
                            }
                        } else {
                            Loaded::Found {
                                identity: copy,
                                storage: KeyStorage::Keyring,
                            }
                        }
                    }
                    None => match self.backup.borrow().as_ref() {
                        Some(id) => Loaded::Restored {
                            identity: Identity::from_parts(id.id(), id.secret()).unwrap(),
                            storage: KeyStorage::Keyring,
                        },
                        None => Loaded::Missing,
                    },
                },
                Mode::FileOnly => match self.file.borrow().as_ref() {
                    Some(id) => Loaded::Found {
                        identity: Identity::from_parts(id.id(), id.secret()).unwrap(),
                        storage: KeyStorage::FilePermissionsOnly,
                    },
                    None => Loaded::Missing,
                },
            }
        }

        fn confirm_loaded_keyring(&self, me: &Identity, restored: bool) {
            self.confirm_calls.set(self.confirm_calls.get() + 1);
            let copy = Identity::from_parts(me.id(), me.secret()).unwrap();
            if restored {
                *self.keyring.borrow_mut() = Some(copy);
            } else {
                *self.backup.borrow_mut() = Some(copy);
            }
        }

        fn save_staged(&self, me: &Identity) -> Result<KeyStorage, SaveStageError> {
            let copy = Identity::from_parts(me.id(), me.secret()).unwrap();
            if let Some(err) = self.save_err.borrow().as_ref() {
                if matches!(err, SaveStageError::Uncertain(_)) {
                    // Simulate post-rename fsync_dir or lost keyring reply: credential is published!
                    match self.mode.get() {
                        Mode::Keyring => *self.keyring.borrow_mut() = Some(copy),
                        Mode::FileOnly => self.set_file(&copy),
                    }
                }
                return Err(err.clone());
            }
            match self.mode.get() {
                Mode::Keyring => {
                    *self.keyring.borrow_mut() = Some(copy);
                    Ok(KeyStorage::Keyring)
                }
                Mode::FileOnly => {
                    self.set_file(&copy);
                    Ok(KeyStorage::FilePermissionsOnly)
                }
            }
        }

        fn forget(&self) -> Result<ForgetResult, String> {
            let keyring_cleared = match self.mode.get() {
                Mode::Keyring => {
                    if let Some(e) = self.forget_keyring_err.borrow().as_ref() {
                        return Err(e.clone());
                    }
                    let had = self.keyring.borrow().is_some();
                    *self.keyring.borrow_mut() = None;
                    *self.backup.borrow_mut() = None;
                    had
                }
                Mode::FileOnly => false,
            };
            let had_file = self.file.borrow().is_some();
            if had_file && self.file_cleanup_fails.get() {
                return Ok(ForgetResult {
                    keyring_cleared,
                    residual_plaintext: true,
                });
            }
            *self.file.borrow_mut() = None;
            Ok(ForgetResult {
                keyring_cleared,
                residual_plaintext: false,
            })
        }

        fn forget_forgotten_keyring(&self, forgotten_addr: &str) -> Result<bool, String> {
            self.keyring_forget_calls.set(self.keyring_forget_calls.get() + 1);
            let backup_matches = self.backup.borrow().as_ref().is_some_and(|id| {
                forgotten_addr == "*" || id.address() == forgotten_addr
            });
            let mut backup_cleared = false;
            if backup_matches {
                *self.backup.borrow_mut() = None;
                backup_cleared = true;
            }
            if let Some(e) = self.forget_keyring_err.borrow().as_ref() {
                return Err(e.clone());
            }
            let had = self.keyring.borrow().is_some();
            *self.keyring.borrow_mut() = None;
            Ok(had || backup_cleared)
        }

        fn fallback_file_stamp(&self) -> Option<FallbackFileStamp> {
            self.file.borrow().as_ref().map(|_| FallbackFileStamp {
                dev: 1,
                ino: self.file_gen.get(),
                mtime_sec: 0,
                mtime_nsec: 0,
            })
        }

        fn cleanup_fallback(
            &self,
            allowed_addrs: &[&str],
            allow_any: bool,
            expected_stamp: Option<FallbackFileStamp>,
            _not_newer_than_sec: Option<i64>,
        ) -> FallbackCleanup {
            let existing = self.file.borrow().as_ref().map(|id| id.address());
            let Some(addr) = existing else {
                return FallbackCleanup::NoneExisted;
            };
            if let Some(exp) = expected_stamp {
                let cur = self.fallback_file_stamp();
                if cur != Some(exp) {
                    return FallbackCleanup::Residual;
                }
            }
            if !allow_any && !allowed_addrs.contains(&addr.as_str()) {
                return FallbackCleanup::Residual;
            }
            if self.file_cleanup_fails.get() {
                return FallbackCleanup::Residual;
            }
            *self.file.borrow_mut() = None;
            FallbackCleanup::Removed
        }
    }

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

    #[test]
    fn permissions_only_fallback_never_revives_forgotten_identity_or_attaches_to_other_chats() {
        // Matching recorded owner -> accepted.
        assert_eq!(
            decide_loaded_identity(
                KeyStorage::FilePermissionsOnly,
                "alex.111111",
                &ownership(Some("alex.111111"), true),
            ),
            LoadedDecision::Accept
        );
        // Different recorded owner -> refused with LostKey for recorded owner.
        assert_eq!(
            decide_loaded_identity(
                KeyStorage::FilePermissionsOnly,
                "alex.111111",
                &ownership(Some("pat.222222"), true),
            ),
            LoadedDecision::RejectMismatch {
                owner: Some("pat.222222".into())
            }
        );
        // Existing local data with unknown owner -> refused with LostKey.
        assert_eq!(
            decide_loaded_identity(
                KeyStorage::FilePermissionsOnly,
                "alex.111111",
                &ownership(None, true),
            ),
            LoadedDecision::RejectMismatch { owner: None }
        );
        // Fresh/wiped store (after sign-out or Start over) -> orphan ignored, never revived.
        assert_eq!(
            decide_loaded_identity(
                KeyStorage::FilePermissionsOnly,
                "alex.111111",
                &ownership(None, false),
            ),
            LoadedDecision::IgnoreOrphan
        );
        // Legacy ownerless + tombstoneless profile keeps existing keyring behavior.
        assert_eq!(
            decide_loaded_identity(
                KeyStorage::Keyring,
                "alex.111111",
                &ownership(None, true),
            ),
            LoadedDecision::Accept
        );
        assert_eq!(
            decide_loaded_identity(
                KeyStorage::Keyring,
                "alex.111111",
                &ownership(None, false),
            ),
            LoadedDecision::Accept
        );
    }

    #[test]
    fn store_owner_lifecycle_across_recovery_profile_switch_and_sign_out() {
        let dir = temp_dir("owner");
        let store = Store::open(&dir.join("hubchat.sqlite3")).unwrap();

        // 1. Fresh store: an orphan fallback file is ignored.
        let ow = read_store_ownership(&store).unwrap();
        assert_eq!(
            decide_loaded_identity(KeyStorage::FilePermissionsOnly, "alex.111111", &ow),
            LoadedDecision::IgnoreOrphan
        );

        // 2. Adopt Alice: store records owner; restart with Alice succeeds;
        // a stale fallback holding Bob is refused instead of attaching to Alice's chats.
        store.set_meta("identity.address", "alex.111111").unwrap();
        store.set_meta("device.id", "hc-20261121000000-1234").unwrap();
        let ow = read_store_ownership(&store).unwrap();
        assert_eq!(
            decide_loaded_identity(KeyStorage::FilePermissionsOnly, "alex.111111", &ow),
            LoadedDecision::Accept
        );
        assert_eq!(
            decide_loaded_identity(KeyStorage::FilePermissionsOnly, "bob.222222", &ow),
            LoadedDecision::RejectMismatch {
                owner: Some("alex.111111".into()),
            }
        );

        // 3. Recovery of same identity keeps local data; switching to Bob wipes first.
        assert!(!must_wipe(ow.owner.as_deref(), ow.had_identity, "alex.111111", false));
        assert!(must_wipe(ow.owner.as_deref(), ow.had_identity, "bob.222222", false));

        // 4. Sign-out / Start over wipes store: a stale fallback holding Alice cannot revive.
        store.wipe().unwrap();
        let ow = read_store_ownership(&store).unwrap();
        assert!(!ow.had_identity);
        assert_eq!(
            decide_loaded_identity(KeyStorage::FilePermissionsOnly, "alex.111111", &ow),
            LoadedDecision::IgnoreOrphan
        );

        drop(store);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn h2_returning_keyring_with_different_identity_preserves_both_keys_and_refuses_start() {
        let dir = temp_dir("h2");
        let store = Store::open(&dir.join("hubchat.sqlite3")).unwrap();
        let alice = Identity::generate("alice").unwrap();
        let bob = Identity::generate("bob").unwrap();
        let carol = Identity::generate("carol").unwrap();

        // Desktop wallet has Alice; VR profile & fallback file belong to Bob.
        store.set_meta("identity.address", &bob.address()).unwrap();
        store.set_meta("device.id", "hc-20261121000000-0001").unwrap();
        store.add_hub("http://bob-hub:7370", "t0").unwrap();
        let secrets = FakeSecrets::new(Mode::Keyring);
        *secrets.keyring.borrow_mut() = Some(Identity::from_parts(alice.id(), alice.secret()).unwrap());
        *secrets.file.borrow_mut() = Some(Identity::from_parts(bob.id(), bob.secret()).unwrap());

        let plan = lifecycle_load(&dir, &store, &secrets, || {}).unwrap();
        assert!(plan.start.is_none(), "H2: mismatched keyring identity must NOT start");
        assert_eq!(plan.key_lost.as_ref().and_then(|k| k.address.as_deref()), Some(bob.address().as_str()));
        assert!(!plan.key_lost.as_ref().unwrap().unreadable);
        assert!(secrets.keyring.borrow().is_some(), "H2: keyring entry must NOT be deleted on mismatch");
        assert!(secrets.file.borrow().is_some(), "H2: Bob's fallback file must NOT be deleted on mismatch");
        assert_eq!(store.meta("identity.address").unwrap().as_deref(), Some(bob.address().as_str()));
        assert_eq!(store.hubs().unwrap().len(), 1, "Bob's local data stays intact");

        // Now keyring holds Bob (matches store owner) AND fallback file holds Carol (different!).
        // Bob starts, Carol's fallback file is NOT deleted, and residual plaintext is surfaced!
        *secrets.keyring.borrow_mut() = Some(Identity::from_parts(bob.id(), bob.secret()).unwrap());
        *secrets.file.borrow_mut() = Some(Identity::from_parts(carol.id(), carol.secret()).unwrap());
        let plan = lifecycle_load(&dir, &store, &secrets, || {}).unwrap();
        assert_eq!(plan.start.as_ref().map(|(id, s)| (id.address(), *s)), Some((bob.address(), KeyStorage::Keyring)));
        assert!(secrets.file.borrow().is_some(), "H2: fallback file of another identity must not be deleted");
        assert!(plan.residual, "unremoved fallback file must be surfaced as residual");

        // When fallback file holds Bob (matching keyring identity being started), cleanup deletes it!
        *secrets.file.borrow_mut() = Some(Identity::from_parts(bob.id(), bob.secret()).unwrap());
        let plan = lifecycle_load(&dir, &store, &secrets, || {}).unwrap();
        assert_eq!(plan.start.as_ref().map(|(id, _)| id.address()), Some(bob.address()));
        assert!(secrets.file.borrow().is_none(), "H2: matching fallback file must be deleted on keyring start");
        assert!(!plan.residual);

        drop(store);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn h3_start_over_and_sign_out_persist_tombstone_and_clear_returning_keyring() {
        let dir = temp_dir("h3");
        let store = Store::open(&dir.join("hubchat.sqlite3")).unwrap();
        let alice = Identity::generate("alice").unwrap();
        let secrets = FakeSecrets::new(Mode::Keyring);

        // 1. Desktop creates Alice in keyring.
        lifecycle_adopt(&dir, &store, &secrets, &alice, true, &[("profile.name", "Alice")], || {}).unwrap();
        assert!(secrets.keyring.borrow().is_some());

        // 2. Switch to VR (Secret Service Absent, no fallback file) -> LostKey -> Start over!
        secrets.mode.set(Mode::FileOnly);
        let lost = lifecycle_load(&dir, &store, &secrets, || {}).unwrap();
        assert!(lost.start.is_none());
        assert_eq!(lost.key_lost.as_ref().and_then(|k| k.address.as_deref()), Some(alice.address().as_str()));

        let commit = lifecycle_forget_begin(&store, &secrets, None).unwrap();
        assert!(!commit.res.keyring_cleared);
        lifecycle_forget_finish(&store, &commit).unwrap();
        assert!(store.meta("identity.address").unwrap().is_none());
        let ow = read_store_ownership(&store).unwrap();
        assert!(ow.tombstones.iter().any(|t| t.address == alice.address()));

        // 3. Return to Desktop (Secret Service Present, still holding Alice):
        // Alice must NOT start, keyring entry is deleted, and tombstone stays durable until explicit adoption.
        secrets.mode.set(Mode::Keyring);
        let plan = lifecycle_load(&dir, &store, &secrets, || {}).unwrap();
        assert!(plan.start.is_none(), "H3: forgotten identity must NOT be started when keyring returns");
        assert!(plan.key_lost.is_none());
        assert!(secrets.keyring.borrow().is_none(), "H3: forgotten keyring entry must be deleted once reachable");
        assert_eq!(secrets.keyring_forget_calls.get(), 1);
        assert!(
            read_store_ownership(&store).unwrap().tombstones.iter().any(|t| t.address == alice.address()),
            "H3: tombstone stays durable across wallets until explicit re-adoption"
        );

        // 4. Sign-out while Secret Service is absent (with a fallback file) also tombstones Alice.
        *secrets.keyring.borrow_mut() = Some(Identity::from_parts(alice.id(), alice.secret()).unwrap());
        secrets.mode.set(Mode::FileOnly);
        lifecycle_adopt(&dir, &store, &secrets, &alice, false, &[], || {}).unwrap();
        assert!(read_store_ownership(&store).unwrap().tombstones.is_empty(), "explicit re-adoption clears Alice's tombstone");
        let commit = lifecycle_forget_begin(&store, &secrets, Some(&alice.address())).unwrap();
        lifecycle_forget_finish(&store, &commit).unwrap();
        assert!(secrets.file.borrow().is_none());

        // If keyring delete temporarily fails when Desktop returns, Alice still does NOT start
        // and the tombstone stays for the next launch!
        secrets.mode.set(Mode::Keyring);
        *secrets.forget_keyring_err.borrow_mut() = Some("wallet locked".into());
        let blocked = lifecycle_load(&dir, &store, &secrets, || {}).unwrap();
        assert!(blocked.start.is_none());
        assert!(secrets.keyring.borrow().is_some());
        assert!(!read_store_ownership(&store).unwrap().tombstones.is_empty());

        // Next launch with unlocked wallet clears keyring while keeping tombstone durable.
        *secrets.forget_keyring_err.borrow_mut() = None;
        let cleared = lifecycle_load(&dir, &store, &secrets, || {}).unwrap();
        assert!(cleared.start.is_none());
        assert!(secrets.keyring.borrow().is_none());
        assert!(read_store_ownership(&store).unwrap().tombstones.iter().any(|t| t.address == alice.address()));

        // 5. Untouched legacy profile with no owner record and no tombstone keeps starting from keyring;
        // Start over on a legacy ownerless profile records "*" tombstone + wiped marker so old key doesn't return.
        store.delete_meta(META_FORGOTTEN).unwrap();
        store.delete_meta(META_INSTALLATION_WIPED).unwrap();
        let legacy = Identity::generate("legacy").unwrap();
        store.set_meta("device.id", "hc-old-legacy").unwrap();
        *secrets.keyring.borrow_mut() = Some(Identity::from_parts(legacy.id(), legacy.secret()).unwrap());
        let ok = lifecycle_load(&dir, &store, &secrets, || {}).unwrap();
        assert_eq!(ok.start.as_ref().map(|(id, _)| id.address()), Some(legacy.address()));

        // Reset to ownerless legacy state, then Start over while Secret Service absent.
        store.delete_meta("identity.address").unwrap();
        secrets.mode.set(Mode::FileOnly);
        let commit = lifecycle_forget_begin(&store, &secrets, None).unwrap();
        lifecycle_forget_finish(&store, &commit).unwrap();
        secrets.mode.set(Mode::Keyring);
        let blocked = lifecycle_load(&dir, &store, &secrets, || {}).unwrap();
        assert!(blocked.start.is_none());
        assert!(secrets.keyring.borrow().is_none());
        assert!(read_store_ownership(&store).unwrap().tombstones.iter().any(|t| t.address == "*"));

        drop(store);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn h4_cleanup_failure_never_blocks_adoption_or_sign_out_and_retries_next_start() {
        let dir = temp_dir("h4");
        let store = Store::open(&dir.join("hubchat.sqlite3")).unwrap();
        let alice = Identity::generate("alice").unwrap();
        let secrets = FakeSecrets::new(Mode::FileOnly);

        // Start in VR with Alice in fallback file.
        lifecycle_adopt(&dir, &store, &secrets, &alice, true, &[], || {}).unwrap();
        assert!(secrets.file.borrow().is_some());

        // Secret Service returns, user restores Alice into keyring, but fallback cleanup fails!
        // Adoption MUST succeed, record pending cleanup, and surface residual = true.
        secrets.mode.set(Mode::Keyring);
        secrets.file_cleanup_fails.set(true);
        let (storage, residual) = lifecycle_adopt(&dir, &store, &secrets, &alice, false, &[], || {}).unwrap();
        assert_eq!(storage, KeyStorage::Keyring);
        assert!(residual, "H4: Settings must be told about residual plaintext copy");
        assert!(store.meta(META_FILE_CLEANUP_PENDING).unwrap().is_some());
        assert!(secrets.file.borrow().is_some());

        // Next start retries cleanup; when the underlying path issue is fixed, cleanup clears residual!
        secrets.file_cleanup_fails.set(false);
        let plan = lifecycle_load(&dir, &store, &secrets, || {}).unwrap();
        assert_eq!(plan.start.as_ref().map(|(id, _)| id.address()), Some(alice.address()));
        assert!(!plan.residual);
        assert!(secrets.file.borrow().is_none());
        assert!(store.meta(META_FILE_CLEANUP_PENDING).unwrap().is_none());

        // Sign-out with failing fallback cleanup: keyring cleared, wipe STILL runs, durable sign-out record persisted!
        secrets.set_file(&alice);
        store.add_hub("http://alice-hub:7370", "t0").unwrap();
        secrets.file_cleanup_fails.set(true);
        let commit = lifecycle_forget_begin(&store, &secrets, Some(&alice.address())).unwrap();
        assert!(commit.res.keyring_cleared);
        assert!(commit.res.residual_plaintext);
        lifecycle_forget_finish(&store, &commit).unwrap();
        assert!(store.hubs().unwrap().is_empty(), "H4: local wipe must still happen on sign-out");
        let pending = read_store_ownership(&store).unwrap().cleanup_pending.unwrap();
        assert!(pending.signed_out);
        assert_eq!(pending.target, alice.address());

        // Adopting a new identity Carol in keyring retries and clears the leftover file once fixed.
        let carol = Identity::generate("carol").unwrap();
        secrets.file_cleanup_fails.set(false);
        let (storage, residual) = lifecycle_adopt(&dir, &store, &secrets, &carol, true, &[], || {}).unwrap();
        assert_eq!(storage, KeyStorage::Keyring);
        assert!(!residual);
        assert!(secrets.file.borrow().is_none());
        assert!(store.meta(META_FILE_CLEANUP_PENDING).unwrap().is_none());

        drop(store);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn m6_staged_adoption_preserves_old_data_on_failure_and_recovers_interrupted_commits() {
        let dir = temp_dir("m6");
        let store = Store::open(&dir.join("hubchat.sqlite3")).unwrap();
        let alice = Identity::generate("alice").unwrap();
        let bob = Identity::generate("bob").unwrap();
        let secrets = FakeSecrets::new(Mode::FileOnly);

        // 1. Alice has data and fallback key on disk.
        lifecycle_adopt(&dir, &store, &secrets, &alice, true, &[("profile.name", "Alice")], || {}).unwrap();
        store.add_hub("http://alice-hub:7370", "t0").unwrap();

        // 2. Returning Desktop wallet is locked; user tries to switch to Bob -> save fails.
        // Alice's chats, owner, profile, AND fallback file must ALL remain intact!
        secrets.mode.set(Mode::Keyring);
        *secrets.save_err.borrow_mut() = Some(SaveStageError::NotPublished("wallet locked".into()));
        assert!(lifecycle_adopt(&dir, &store, &secrets, &bob, true, &[("profile.name", "Bob")], || {}).is_err());
        assert_eq!(store.meta("identity.address").unwrap().as_deref(), Some(alice.address().as_str()));
        assert_eq!(store.meta("profile.name").unwrap().as_deref(), Some("Alice"));
        assert_eq!(store.hubs().unwrap().len(), 1);
        assert!(secrets.file.borrow().is_some(), "M6: old fallback key must survive failed replacement");
        assert!(store.meta(META_ADOPTING).unwrap().is_none(), "staged marker cleaned up on save error");
        *secrets.save_err.borrow_mut() = None;

        // 3. Interrupted first adoption (FilePermissionsOnly): staged marker + saved file, crash before commit.
        // Next launch must recover instead of ignoring as an orphan!
        let dir2 = temp_dir("m6-first");
        let store2 = Store::open(&dir2.join("hubchat.sqlite3")).unwrap();
        let secrets2 = FakeSecrets::new(Mode::FileOnly);
        let pending_first = PendingAdoption {
            address: bob.address(),
            wipe: false,
            brand_new: true,
            meta: vec![("profile.name".into(), "Bob".into())],
        };
        store2
            .set_meta(META_ADOPTING, &serde_json::to_string(&pending_first).unwrap())
            .unwrap();
        secrets2.set_file(&bob);
        let recovered = lifecycle_load(&dir2, &store2, &secrets2, || {}).unwrap();
        assert_eq!(
            recovered.start.as_ref().map(|(id, s)| (id.address(), *s)),
            Some((bob.address(), KeyStorage::FilePermissionsOnly))
        );
        assert_eq!(store2.meta("identity.address").unwrap().as_deref(), Some(bob.address().as_str()));
        assert_eq!(store2.meta("profile.name").unwrap().as_deref(), Some("Bob"));
        assert!(store2.meta(META_ADOPTING).unwrap().is_none());

        // 4. Interrupted replacement adoption (Keyring — covers Windows/macOS/Linux):
        // staged marker + new key Bob saved, crash before Alice's chats were wiped!
        // Next launch must wipe Alice's chats atomically, set owner = Bob, remove Alice's fallback file, start Bob!
        let pending_replace = PendingAdoption {
            address: bob.address(),
            wipe: true,
            brand_new: true,
            meta: vec![("profile.name".into(), "Bob2".into())],
        };
        store
            .set_meta(META_ADOPTING, &serde_json::to_string(&pending_replace).unwrap())
            .unwrap();
        *secrets.keyring.borrow_mut() = Some(Identity::from_parts(bob.id(), bob.secret()).unwrap());
        let recovered = lifecycle_load(&dir, &store, &secrets, || {}).unwrap();
        assert_eq!(
            recovered.start.as_ref().map(|(id, s)| (id.address(), *s)),
            Some((bob.address(), KeyStorage::Keyring))
        );
        assert!(store.hubs().unwrap().is_empty(), "M6: Alice's hubs must be wiped during recovery");
        assert_eq!(store.meta("identity.address").unwrap().as_deref(), Some(bob.address().as_str()));
        assert_eq!(store.meta("profile.name").unwrap().as_deref(), Some("Bob2"));
        assert!(secrets.file.borrow().is_none(), "Alice's superseded fallback file cleaned up after recovery");
        assert!(store.meta(META_ADOPTING).unwrap().is_none());

        // 5. Windows/macOS keyring adoption & backup-restored load path writes owner & reloads cleanly.
        let dir3 = temp_dir("m6-win-mac");
        let store3 = Store::open(&dir3.join("hubchat.sqlite3")).unwrap();
        let secrets3 = FakeSecrets::new(Mode::Keyring);
        let (st, res) = lifecycle_adopt(
            &dir3,
            &store3,
            &secrets3,
            &alice,
            true,
            &[("profile.name", "DesktopUser")],
            || {},
        )
        .unwrap();
        assert_eq!(st, KeyStorage::Keyring);
        assert!(!res);
        assert_eq!(store3.meta("identity.address").unwrap().as_deref(), Some(alice.address().as_str()));
        assert_eq!(store3.meta("profile.name").unwrap().as_deref(), Some("DesktopUser"));
        secrets3.restored.set(true);
        let restored = lifecycle_load(&dir3, &store3, &secrets3, || {}).unwrap();
        assert_eq!(restored.start.as_ref().map(|(id, _)| id.address()), Some(alice.address()));
        assert_eq!(store3.meta("key.restored").unwrap().as_deref(), Some("yes"));

        // 6. Database read errors and malformed lifecycle metadata must propagate as Err (never missing!).
        struct BrokenDb;
        impl MetaReader for BrokenDb {
            fn read_meta(&self, _key: &str) -> Result<Option<String>, String> {
                Err("database disk image is malformed".into())
            }
        }
        assert!(read_store_ownership(&BrokenDb).is_err());
        store3.set_meta(META_FORGOTTEN, "{not valid json").unwrap();
        assert!(read_store_ownership(&store3).is_err());
        assert!(lifecycle_load(&dir3, &store3, &secrets3, || {}).is_err());

        drop(store);
        drop(store2);
        drop(store3);
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_dir_all(&dir2);
        let _ = std::fs::remove_dir_all(&dir3);
    }

    #[test]
    fn h2_returning_keyring_with_different_identity_is_rejected_not_started() {
        // Desktop wallet holds Alice while local store belongs to Bob -> must refuse, not start Alice!
        assert_eq!(
            decide_loaded_identity(
                KeyStorage::Keyring,
                "alice.111111",
                &ownership(Some("bob.222222"), true),
            ),
            LoadedDecision::RejectMismatch {
                owner: Some("bob.222222".into())
            }
        );
        // Tombstone takes precedence when keyring identity was forgotten.
        let mut ow = ownership(None, false);
        ow.tombstones.push(ForgetTombstone {
            address: "alice.111111".into(),
            at: "2026-11-21T20:00:00Z".into(),
        });
        assert_eq!(
            decide_loaded_identity(KeyStorage::Keyring, "alice.111111", &ow),
            LoadedDecision::ForgottenKeyring
        );
    }

    // Round 2 re-verification RED tests: H3 (NoEntry & multi-wallet tombstone durability),
    // H4 (signed-out startup retries scheduled cleanup), M6 (older wallet never clears B's staging).

    #[test]
    fn r2_h3_empty_wallet_noentry_and_second_wallet_keep_tombstone_durable() {
        let dir = temp_dir("r2-h3");
        let store = Store::open(&dir.join("hubchat.sqlite3")).unwrap();
        let alice = Identity::generate("alice").unwrap();
        let secrets = FakeSecrets::new(Mode::Keyring);

        // 1. Desktop wallet 1 creates Alice.
        lifecycle_adopt(&dir, &store, &secrets, &alice, true, &[], || {}).unwrap();

        // 2. Switch to an empty separate VR wallet (Keyring present, NoEntry): Start over!
        *secrets.keyring.borrow_mut() = None;
        let commit = lifecycle_forget_begin(&store, &secrets, None).unwrap();
        lifecycle_forget_finish(&store, &commit).unwrap();
        // Tombstone MUST stay so Desktop wallet 1 cannot resurrect Alice.
        assert!(
            read_store_ownership(&store).unwrap().tombstones.iter().any(|t| t.address == alice.address()),
            "H3: Start over against an empty wallet (NoEntry) must keep Alice's tombstone"
        );

        // Return to Desktop wallet 1 holding Alice -> refused & cleared from wallet 1.
        *secrets.keyring.borrow_mut() = Some(Identity::from_parts(alice.id(), alice.secret()).unwrap());
        let p1 = lifecycle_load(&dir, &store, &secrets, || {}).unwrap();
        assert!(p1.start.is_none());
        assert!(secrets.keyring.borrow().is_none());

        // 3. Two wallets both holding Alice: sign-out in wallet 1 deletes wallet 1's entry,
        // and tombstone MUST stay durable so opening wallet 2 (still holding Alice) also refuses & clears it.
        lifecycle_adopt(&dir, &store, &secrets, &alice, false, &[], || {}).unwrap();
        let commit2 = lifecycle_forget_begin(&store, &secrets, Some(&alice.address())).unwrap();
        lifecycle_forget_finish(&store, &commit2).unwrap();
        *secrets.keyring.borrow_mut() = Some(Identity::from_parts(alice.id(), alice.secret()).unwrap());
        let p2 = lifecycle_load(&dir, &store, &secrets, || {}).unwrap();
        assert!(p2.start.is_none(), "H3: second wallet holding forgotten Alice must not sign back in");
        assert!(secrets.keyring.borrow().is_none());

        drop(store);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn r2_h4_signed_out_startup_retries_pending_fallback_cleanup() {
        let dir = temp_dir("r2-h4");
        let store = Store::open(&dir.join("hubchat.sqlite3")).unwrap();
        let alice = Identity::generate("alice").unwrap();
        let secrets = FakeSecrets::new(Mode::Keyring);

        // Sign out while fallback file cannot be removed -> durable sign-out cleanup record persisted.
        lifecycle_adopt(&dir, &store, &secrets, &alice, true, &[], || {}).unwrap();
        secrets.set_file(&alice);
        secrets.file_cleanup_fails.set(true);
        let commit = lifecycle_forget_begin(&store, &secrets, Some(&alice.address())).unwrap();
        lifecycle_forget_finish(&store, &commit).unwrap();
        let pending = read_store_ownership(&store).unwrap().cleanup_pending.unwrap();
        assert!(pending.signed_out);
        assert_eq!(pending.target, alice.address());

        // 1. Signed-out startup with empty keyring (Loaded::Missing) retries cleanup once removable!
        secrets.file_cleanup_fails.set(false);
        let p = lifecycle_load(&dir, &store, &secrets, || {}).unwrap();
        assert!(p.start.is_none());
        assert!(!p.residual, "H4: signed-out startup must retry scheduled cleanup and clear residual");
        assert!(secrets.file.borrow().is_none(), "H4: scheduled leftover file must be deleted on signed-out startup");
        assert!(store.meta(META_FILE_CLEANUP_PENDING).unwrap().is_none());

        // 2. Signed-out startup without Secret Service (LoadedDecision::IgnoreOrphan) also retries!
        secrets.set_file(&alice);
        save_pending_cleanup(
            &store,
            &PendingFileCleanup {
                target: alice.address(),
                signed_out: true,
                file_stamp: secrets.fallback_file_stamp(),
                recorded_at_sec: None,
            },
        )
        .unwrap();
        secrets.mode.set(Mode::FileOnly);
        let p2 = lifecycle_load(&dir, &store, &secrets, || {}).unwrap();
        assert!(p2.start.is_none());
        assert!(!p2.residual, "H4: IgnoreOrphan startup must retry scheduled cleanup");
        assert!(secrets.file.borrow().is_none(), "H4: orphaned leftover file must be deleted when cleanup is scheduled");
        assert!(store.meta(META_FILE_CLEANUP_PENDING).unwrap().is_none());

        drop(store);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn r2_m6_older_wallet_accept_never_clears_another_address_pending_adoption() {
        let dir = temp_dir("r2-m6");
        let store = Store::open(&dir.join("hubchat.sqlite3")).unwrap();
        let alice = Identity::generate("alice").unwrap();
        let bob = Identity::generate("bob").unwrap();
        let secrets = FakeSecrets::new(Mode::Keyring);

        // Store belongs to Alice; Bob's adoption was staged and saved in another session,
        // exiting before SQLite committed.
        lifecycle_adopt(&dir, &store, &secrets, &alice, true, &[("profile.name", "Alice")], || {}).unwrap();
        let pending_bob = PendingAdoption {
            address: bob.address(),
            wipe: true,
            brand_new: true,
            meta: vec![("profile.name".into(), "Bob".into())],
        };
        store
            .set_meta(META_ADOPTING, &serde_json::to_string(&pending_bob).unwrap())
            .unwrap();

        // Next launch happens in an older wallet still holding Alice: Alice starts,
        // and MUST NOT clear Bob's pending adoption record!
        let plan_a = lifecycle_load(&dir, &store, &secrets, || {}).unwrap();
        assert_eq!(plan_a.start.as_ref().map(|(id, _)| id.address()), Some(alice.address()));
        assert!(
            store.meta(META_ADOPTING).unwrap().is_some(),
            "M6: accepting older wallet Alice must NOT clear Bob's pending adoption"
        );

        // Returning to Bob's session still recovers Bob's adoption cleanly.
        *secrets.keyring.borrow_mut() = Some(Identity::from_parts(bob.id(), bob.secret()).unwrap());
        let plan_b = lifecycle_load(&dir, &store, &secrets, || {}).unwrap();
        assert_eq!(plan_b.start.as_ref().map(|(id, _)| id.address()), Some(bob.address()));
        assert_eq!(store.meta("identity.address").unwrap().as_deref(), Some(bob.address().as_str()));
        assert!(store.meta(META_ADOPTING).unwrap().is_none());

        // Post-rename `fsync_dir` failure (`SaveStageError::Uncertain`) on first file adoption:
        // `META_ADOPTING` MUST stay set so the surviving new file recovers on next launch instead
        // of being rejected as an orphan!
        let dir_unc = temp_dir("r2-m6-uncertain");
        let store_unc = Store::open(&dir_unc.join("hubchat.sqlite3")).unwrap();
        let secrets_unc = FakeSecrets::new(Mode::FileOnly);
        *secrets_unc.save_err.borrow_mut() =
            Some(SaveStageError::Uncertain("could not sync directory (errno 5)".into()));
        assert!(
            lifecycle_adopt(&dir_unc, &store_unc, &secrets_unc, &bob, true, &[("profile.name", "Bob")], || {})
                .is_err()
        );
        assert!(
            store_unc.meta(META_ADOPTING).unwrap().is_some(),
            "M6: uncertain post-rename sync failure must preserve META_ADOPTING"
        );
        *secrets_unc.save_err.borrow_mut() = None;
        let rec = lifecycle_load(&dir_unc, &store_unc, &secrets_unc, || {}).unwrap();
        assert_eq!(
            rec.start.as_ref().map(|(id, s)| (id.address(), *s)),
            Some((bob.address(), KeyStorage::FilePermissionsOnly))
        );
        assert_eq!(store_unc.meta("identity.address").unwrap().as_deref(), Some(bob.address().as_str()));
        assert!(store_unc.meta(META_ADOPTING).unwrap().is_none());

        drop(store_unc);
        let _ = std::fs::remove_dir_all(&dir_unc);
        drop(store);
        let _ = std::fs::remove_dir_all(&dir);
    }

    // Round 3 final bounded RED tests:
    // 1. M6 High: recovery recomputes wipe against current owner inside tx.
    // 3. H4 High: signed-out cleanup requires durable sign-out record and protects owned/staged files.
    // 4. H3 High: wiped-installation marker survives later adoption and sign-out.
    // 5. H3 Medium (Windows): forgotten DPAPI backup is deleted, not restored or refreshed.

    #[test]
    fn r3_m6_recovery_recomputes_wipe_against_current_owner() {
        let dir = temp_dir("r3-m6-wipe");
        let store = Store::open(&dir.join("hubchat.sqlite3")).unwrap();
        let alice = Identity::generate("alice").unwrap();
        let bob = Identity::generate("bob").unwrap();
        let secrets = FakeSecrets::new(Mode::Keyring);

        // B's first adoption starts on an empty store (wipe = false) and exits before commit.
        let pending_bob = PendingAdoption {
            address: bob.address(),
            wipe: false,
            brand_new: true,
            meta: vec![("profile.name".into(), "Bob".into())],
        };
        store
            .set_meta(META_ADOPTING, &serde_json::to_string(&pending_bob).unwrap())
            .unwrap();

        // Another wallet with A starts in the ownerless store, claims ownership, and writes chats/drafts/hubs.
        *secrets.keyring.borrow_mut() = Some(Identity::from_parts(alice.id(), alice.secret()).unwrap());
        let plan_a = lifecycle_load(&dir, &store, &secrets, || {}).unwrap();
        assert_eq!(plan_a.start.as_ref().map(|(id, _)| id.address()), Some(alice.address()));
        store.set_meta("identity.address", &alice.address()).unwrap();
        store.add_hub("http://alice-hub:7370", "t0").unwrap();
        store.set_draft("carol.333333", "Alice's private draft").unwrap();
        store
            .queue_outgoing(
                &hubchat_core::store::NewOutgoing {
                    id: "msg-alice-1".into(),
                    peer: "carol.333333".into(),
                    body: "Alice's private message".into(),
                    kind: None,
                    reply_to: None,
                    attachments: vec![],
                },
                "2026-11-21T23:00:00Z",
            )
            .unwrap();
        assert!(store.message("msg-alice-1").unwrap().is_some());

        // B returns: recovery MUST recompute wipe against current owner A inside the tx
        // so B never inherits A's messages, drafts, or hubs.
        *secrets.keyring.borrow_mut() = Some(Identity::from_parts(bob.id(), bob.secret()).unwrap());
        let plan_b = lifecycle_load(&dir, &store, &secrets, || {}).unwrap();
        assert_eq!(plan_b.start.as_ref().map(|(id, _)| id.address()), Some(bob.address()));
        assert_eq!(store.meta("identity.address").unwrap().as_deref(), Some(bob.address().as_str()));
        assert!(store.meta(META_ADOPTING).unwrap().is_none());
        assert!(store.hubs().unwrap().is_empty(), "M6 High: Alice's hubs must be wiped when B recovers");
        assert!(store.draft("carol.333333").unwrap().is_none(), "M6 High: Alice's drafts must be wiped when B recovers");
        assert!(store.message("msg-alice-1").unwrap().is_none(), "M6 High: Alice's messages must be wiped when B recovers");

        drop(store);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn r3_h4_cleanup_requires_signed_out_record_and_protects_owned_and_staged_files() {
        let dir = temp_dir("r3-h4");
        let store = Store::open(&dir.join("hubchat.sqlite3")).unwrap();
        let alice = Identity::generate("alice").unwrap();
        let bob = Identity::generate("bob").unwrap();
        let secrets = FakeSecrets::new(Mode::FileOnly);

        // Case A: Alice owns the local profile; an earlier keyring session failed to delete
        // Alice's fallback file (NOT a sign-out!). Later the current wallet is empty (LostKey)
        // while the path is removable: startup MUST preserve Alice's VR fallback file!
        lifecycle_adopt(&dir, &store, &secrets, &alice, true, &[], || {}).unwrap();
        secrets.mode.set(Mode::Keyring);
        secrets.file_cleanup_fails.set(true);
        let (_, residual) = lifecycle_adopt(&dir, &store, &secrets, &alice, false, &[], || {}).unwrap();
        assert!(residual);

        *secrets.keyring.borrow_mut() = None;
        secrets.file_cleanup_fails.set(false);
        let lost = lifecycle_load(&dir, &store, &secrets, || {}).unwrap();
        assert!(lost.start.is_none());
        assert!(lost.key_lost.is_some());
        assert!(
            secrets.file.borrow().is_some(),
            "H4 High: owned profile with empty wallet must NOT delete Alice's VR fallback file"
        );

        // Case B: Sign-out fails fallback cleanup; later Bob's adoption is staged and publishes
        // Bob's fallback file before exiting; next launch in an empty wallet MUST NOT delete Bob's file!
        secrets.file_cleanup_fails.set(true);
        let commit = lifecycle_forget_begin(&store, &secrets, Some(&alice.address())).unwrap();
        lifecycle_forget_finish(&store, &commit).unwrap();
        let pending_bob = PendingAdoption {
            address: bob.address(),
            wipe: false,
            brand_new: true,
            meta: vec![],
        };
        store
            .set_meta(META_ADOPTING, &serde_json::to_string(&pending_bob).unwrap())
            .unwrap();
        secrets.set_file(&bob);

        secrets.file_cleanup_fails.set(false);
        let empty = lifecycle_load(&dir, &store, &secrets, || {}).unwrap();
        assert!(empty.start.is_none());
        assert!(
            secrets.file.borrow().as_ref().map(|id| id.address()) == Some(bob.address()),
            "H4 High: pending adoption B's newly published fallback file must be protected from old sign-out cleanup"
        );

        // Case C: Even without META_ADOPTING, a newer file generation (new inode/stamp) published
        // after sign-out is NEVER deleted by an older signed-out cleanup record (including wildcard "*").
        store.delete_meta(META_ADOPTING).unwrap();
        let older_stamp = FallbackFileStamp { dev: 1, ino: 1, mtime_sec: 0, mtime_nsec: 0 };
        save_pending_cleanup(
            &store,
            &PendingFileCleanup {
                target: "*".into(),
                signed_out: true,
                file_stamp: Some(older_stamp),
                recorded_at_sec: None,
            },
        )
        .unwrap();
        secrets.set_file(&bob); // newer generation (ino > 1)
        let after_new = lifecycle_load(&dir, &store, &secrets, || {}).unwrap();
        assert!(after_new.residual);
        assert!(
            secrets.file.borrow().as_ref().map(|id| id.address()) == Some(bob.address()),
            "H4 High: signed-out retry must never delete a credential file newer than the cleanup target"
        );

        drop(store);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn r3_h3_wiped_legacy_profile_never_accepts_un_readopted_key_after_later_signout() {
        let dir = temp_dir("r3-h3-wiped");
        let store = Store::open(&dir.join("hubchat.sqlite3")).unwrap();
        let alice = Identity::generate("alice").unwrap();
        let bob = Identity::generate("bob").unwrap();
        let secrets = FakeSecrets::new(Mode::FileOnly);

        // Legacy ownerless profile (device.id without identity.address) runs Start over.
        store.set_meta("device.id", "hc-legacy-0001").unwrap();
        let c1 = lifecycle_forget_begin(&store, &secrets, None).unwrap();
        lifecycle_forget_finish(&store, &c1).unwrap();
        assert_eq!(store.meta(META_INSTALLATION_WIPED).unwrap().as_deref(), Some("1"));

        // User adopts Bob, then later signs out of Bob: wiped marker survives both.
        secrets.mode.set(Mode::Keyring);
        lifecycle_adopt(&dir, &store, &secrets, &bob, true, &[], || {}).unwrap();
        assert_eq!(store.meta(META_INSTALLATION_WIPED).unwrap().as_deref(), Some("1"));
        let c2 = lifecycle_forget_begin(&store, &secrets, Some(&bob.address())).unwrap();
        lifecycle_forget_finish(&store, &c2).unwrap();
        assert_eq!(store.meta(META_INSTALLATION_WIPED).unwrap().as_deref(), Some("1"));

        // Older wallet holding un-readopted Alice returns: wiped installation MUST NOT auto-accept Alice.
        *secrets.keyring.borrow_mut() = Some(Identity::from_parts(alice.id(), alice.secret()).unwrap());
        let plan = lifecycle_load(&dir, &store, &secrets, || {}).unwrap();
        assert!(
            plan.start.is_none(),
            "H3 High: previously wiped installation must never accept an un-readopted identity after later sign-out"
        );
        // Explicit re-adoption of Alice still works cleanly!
        lifecycle_adopt(&dir, &store, &secrets, &alice, false, &[], || {}).unwrap();
        let ok = lifecycle_load(&dir, &store, &secrets, || {}).unwrap();
        assert_eq!(ok.start.as_ref().map(|(id, _)| id.address()), Some(alice.address()));

        drop(store);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn r3_h3_forgotten_backup_is_deleted_not_restored() {
        let dir = temp_dir("r3-h3-win-backup");
        let store = Store::open(&dir.join("hubchat.sqlite3")).unwrap();
        let alice = Identity::generate("alice").unwrap();
        let secrets = FakeSecrets::new(Mode::Keyring);

        // Alice was forgotten while her DPAPI backup remained on disk and Credential Manager is empty.
        lifecycle_adopt(&dir, &store, &secrets, &alice, true, &[], || {}).unwrap();
        let c = lifecycle_forget_begin(&store, &secrets, Some(&alice.address())).unwrap();
        lifecycle_forget_finish(&store, &c).unwrap();
        *secrets.keyring.borrow_mut() = None;
        *secrets.backup.borrow_mut() = Some(Identity::from_parts(alice.id(), alice.secret()).unwrap());

        let plan = lifecycle_load(&dir, &store, &secrets, || {}).unwrap();
        assert!(plan.start.is_none(), "H3 Medium: forgotten backup must not sign back in");
        assert!(secrets.keyring.borrow().is_none(), "H3 Medium: forgotten backup must not be restored to main keyring");
        assert!(secrets.backup.borrow().is_none(), "H3 Medium: forgotten matching backup must be deleted");
        assert_eq!(secrets.confirm_calls.get(), 0, "H3 Medium: forgotten backup must never be confirmed/restored");

        // Also: if forgotten Alice reappears in Credential Manager with no backup, confirm_loaded_keyring
        // must NOT refresh/write a DPAPI backup before tombstone check!
        *secrets.keyring.borrow_mut() = Some(Identity::from_parts(alice.id(), alice.secret()).unwrap());
        let plan2 = lifecycle_load(&dir, &store, &secrets, || {}).unwrap();
        assert!(plan2.start.is_none());
        assert!(secrets.keyring.borrow().is_none());
        assert!(secrets.backup.borrow().is_none(), "H3 Medium: forgotten Credential Manager key must not create a backup");
        assert_eq!(secrets.confirm_calls.get(), 0);

        drop(store);
        let _ = std::fs::remove_dir_all(&dir);
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
