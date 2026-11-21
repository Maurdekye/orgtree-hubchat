//! Where the identity key lives. Windows: Credential Manager, plus a backup
//! copy in Hubchat's own folder sealed with Windows' DPAPI for this user
//! (user 2026-10-10 08:13Z, after Windows lost every saved sign-in in a
//! crash and Hubchat started over without a word). macOS and Linux: the
//! system keyring. Android: a file in app-private storage sealed with an
//! Android Keystore key (SecretBox.kt), with app backup disabled. The key
//! never goes to the UI or into logs.

use std::path::Path;

use hubchat_core::Identity;

use crate::keylog::{self, Fact};

#[cfg(desktop)]
const SERVICE: &str = "dev.orgtree.hubchat";

/// What the main copy is kept in, for the key log.
const STORE: &str = if cfg!(windows) {
    "Credential Manager"
} else if cfg!(target_os = "android") {
    "the sealed key file"
} else {
    "the keyring"
};

fn encode(me: &Identity) -> String {
    format!("{} {}", me.id(), me.secret())
}

fn decode(s: &str) -> Option<Identity> {
    let (id, secret) = s.trim().split_once(' ')?;
    Identity::from_parts(id, secret).ok()
}

/// Where this device's active identity key is stored.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum KeyStorage {
    /// OS key store (Credential Manager, macOS Keychain, Linux Secret Service, Android Keystore).
    Keyring,
    /// Linux-only fallback when no Secret Service exists: a private file protected
    /// only by user file permissions (0700 dir, 0600 file).
    #[cfg_attr(not(any(target_os = "linux", test)), allow(dead_code))]
    FilePermissionsOnly,
}

/// Whether a Linux Secret Service (`org.freedesktop.secrets`) is reachable.
#[cfg_attr(not(any(target_os = "linux", test)), allow(dead_code))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SecretServiceState {
    /// Owned on the session bus or listed as activatable.
    Present,
    /// Confirmed absent (no session bus, or bus confirmed neither owned nor activatable).
    Absent,
    /// Could not determine (bus error or timeout); fail closed and preserve keyring behavior.
    Indeterminate,
}

#[cfg(any(target_os = "linux", test))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BusEnv {
    Configured,
    Disabled { runtime_bus_exists: bool },
    Missing { runtime_bus_exists: Option<bool> },
}

#[cfg(any(target_os = "linux", test))]
#[allow(dead_code)]
pub(crate) fn classify_dbus_probe(
    bus_env: BusEnv,
    probe: impl FnOnce() -> Result<(bool, Option<Vec<String>>), ()>,
) -> SecretServiceState {
    let _ = (bus_env, probe);
    SecretServiceState::Present
}

#[cfg(any(target_os = "linux", test))]
#[allow(dead_code)]
pub(crate) mod posix_trust {
    use std::path::Path;

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub(crate) struct StatInfo {
        pub is_dir: bool,
        pub is_reg: bool,
        pub is_symlink: bool,
        pub uid: u32,
        pub mode: u32,
        pub nlink: u64,
        pub size: i64,
    }

    pub(crate) const MAX_KEY_BYTES: i64 = 4096;

    pub(crate) fn validate_clean_absolute(_path: &Path) -> Result<(), &'static str> {
        Ok(())
    }

    pub(crate) fn judge_ancestor(_euid: u32, _st: StatInfo) -> Result<(), &'static str> {
        Ok(())
    }

    pub(crate) fn judge_app_dir(_euid: u32, _st: StatInfo) -> Result<(), &'static str> {
        Ok(())
    }

    pub(crate) fn judge_store_dir(_euid: u32, _st: StatInfo) -> Result<(), &'static str> {
        Ok(())
    }

    pub(crate) fn judge_key_file(_euid: u32, _st: StatInfo) -> Result<(), &'static str> {
        Ok(())
    }
}

#[cfg(any(target_os = "linux", test))]
#[allow(dead_code)]
pub(crate) mod linux_policy {
    use super::{KeyStorage, Main, SecretServiceState};
    use hubchat_core::Identity;

    pub(crate) enum PolicyLoaded {
        Found { identity: Identity, storage: KeyStorage },
        Missing,
        Unreadable,
    }

    pub(crate) trait Backend {
        fn probe(&self) -> SecretServiceState;
        fn keyring_load(&self) -> Main;
        fn keyring_save(&self, me: &Identity) -> Result<(), String>;
        fn keyring_forget(&self) -> Result<(), String>;
        fn file_load(&self) -> Result<Option<String>, String>;
        fn file_save(&self, encoded: &str) -> Result<(), String>;
        fn file_forget(&self) -> Result<(), String>;
    }

    pub(crate) fn load_with(b: &impl Backend) -> PolicyLoaded {
        match b.keyring_load() {
            Main::Found(identity) => PolicyLoaded::Found {
                identity,
                storage: KeyStorage::Keyring,
            },
            Main::Missing(_) => PolicyLoaded::Missing,
            Main::Failed(_) => PolicyLoaded::Unreadable,
        }
    }

    pub(crate) fn save_with(b: &impl Backend, me: &Identity) -> Result<KeyStorage, String> {
        b.keyring_save(me)?;
        Ok(KeyStorage::Keyring)
    }

    pub(crate) fn forget_with(b: &impl Backend) -> Result<(), String> {
        b.keyring_forget()
    }
}

/// What start-up found.
pub enum Loaded {
    Found(Identity),
    /// The main copy was gone (or didn't answer) and the backup carried the
    /// key; the main copy was put back from it where Windows allowed.
    Restored(Identity),
    Missing,
    /// The key store didn't answer: it may still hold the key.
    Unreadable,
}

/// What the main copy gave.
enum Main {
    Found(Identity),
    /// Not there, or there but no longer usable (why, for the key log).
    Missing(String),
    /// The key store failed to answer.
    Failed(String),
}

/// The key, from the main copy or else the backup (start-up and Try again).
pub fn load_identity(dir: &Path) -> Loaded {
    // The key log file keeps the full line (the stores' own words); the normal
    // log gets only the facts (keylog::Fact).
    let (why, failed, main) = match load_main(dir) {
        Main::Found(me) => {
            let (backup, facts) = match keep_backup(dir, &me) {
                Some((text, fact)) => (format!("; {text}"), vec![Fact::KeyFound, fact]),
                None => (String::new(), vec![Fact::KeyFound]),
            };
            keylog::note(dir, &format!("key found in {STORE}{backup}"), &facts);
            return Loaded::Found(me);
        }
        Main::Missing(why) => (why, false, Fact::StoreHasNoKey),
        Main::Failed(why) => (why, true, Fact::StoreDidNotAnswer),
    };
    match backup::load(dir) {
        Ok(Some(text)) => {
            if let Some(me) = decode(&text) {
                let (back, fact) = match save_main(dir, &me) {
                    Ok(()) => (format!("put back into {STORE}"), Fact::PutBackInStore),
                    Err(e) => (format!("{STORE} refused it back ({e}); running from the backup"), Fact::StoreRefusedItBack),
                };
                keylog::note(dir, &format!("{why}; key restored from Hubchat's backup, {back}"), &[main, Fact::RestoredFromBackup, fact]);
                return Loaded::Restored(me);
            }
            keylog::note(dir, &format!("{why}; Hubchat's backup is damaged"), &[main, Fact::BackupDamaged]);
        }
        Ok(None) => keylog::note(dir, &format!("{why}; no backup"), &[main, Fact::NoBackup]),
        Err(e) => keylog::note(dir, &format!("{why}; the backup didn't open ({e})"), &[main, Fact::BackupUnreadable]),
    }
    if failed {
        Loaded::Unreadable
    } else {
        Loaded::Missing
    }
}

/// Keep the key: the main copy must take it; the backup follows where there
/// is one (if it can't, there is no backup, and the key log says why).
pub fn save_identity(dir: &Path, me: &Identity) -> Result<(), String> {
    save_main(dir, me)?;
    if let Err(e) = write_backup(dir, me) {
        keylog::note(dir, &e, &[Fact::BackupNotWritten]);
    }
    Ok(())
}

/// Forget the key: the main copy and the backup.
pub fn forget_identity(dir: &Path) -> Result<(), String> {
    forget_main(dir)?;
    backup::forget(dir)
}

/// While the main copy has the key, the backup holds that same key or none:
/// an older Hubchat made none, and one left from another identity (when a
/// new key's backup couldn't be written) must never come back in its place.
/// Says what it did, for the key log: the file's line and the fact the normal
/// log may repeat.
fn keep_backup(dir: &Path, me: &Identity) -> Option<(String, Fact)> {
    let found = match backup::load(dir) {
        Ok(Some(text)) if text.trim() == encode(me) => return None,
        Ok(Some(_)) => "the backup held another key".to_string(),
        Ok(None) if cfg!(windows) => "there was no backup".to_string(),
        Ok(None) => return None,
        Err(e) => format!("the backup didn't open ({e})"),
    };
    Some(match write_backup(dir, me) {
        Ok(()) => (format!("{found}; a new one was written"), Fact::BackupWritten),
        Err(e) => (format!("{found}; {e}"), Fact::BackupNotWritten),
    })
}

/// A backup that can't be written takes the old one with it, so an older key
/// never outlives a newer one.
fn write_backup(dir: &Path, me: &Identity) -> Result<(), String> {
    let e = match backup::save(dir, &encode(me)) {
        Ok(()) => return Ok(()),
        Err(e) => e,
    };
    Err(match backup::forget(dir) {
        Ok(()) => format!("the backup couldn't be written ({e}), so there is none"),
        Err(f) => format!("the backup couldn't be written ({e}) nor the old one removed ({f})"),
    })
}

// ---------------------------------------------------------------- the main copy

#[cfg(desktop)]
fn entry(dir: &Path) -> Result<keyring::Entry, String> {
    // One credential per data folder, so a second profile/dev build can't clash.
    let user = format!("identity:{}", dir.to_string_lossy());
    keyring::Entry::new(SERVICE, &user).map_err(|e| e.to_string())
}

#[cfg(desktop)]
fn load_main(dir: &Path) -> Main {
    match entry(dir) {
        Ok(e) => classify(e.get_password()),
        Err(e) => Main::Failed(e),
    }
}

/// What the keyring's answer means: no entry is a lost key, any other error
/// a store that didn't answer (it may still have the key).
#[cfg(desktop)]
fn classify(r: keyring::Result<String>) -> Main {
    match r {
        Ok(s) => decode(&s).map_or_else(|| Main::Missing(format!("the key in {STORE} is damaged")), Main::Found),
        Err(keyring::Error::NoEntry) => Main::Missing(format!("no key in {STORE}")),
        Err(e) => Main::Failed(format!("{STORE} failed: {e}")),
    }
}

#[cfg(desktop)]
fn save_main(dir: &Path, me: &Identity) -> Result<(), String> {
    entry(dir)?
        .set_password(&encode(me))
        .map_err(|e| e.to_string())
}

#[cfg(desktop)]
fn forget_main(dir: &Path) -> Result<(), String> {
    match entry(dir)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(e.to_string()),
    }
}

#[cfg(target_os = "android")]
fn load_main(dir: &Path) -> Main {
    let sealed = match std::fs::read_to_string(dir.join("identity.sealed")) {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Main::Missing(format!("no {STORE}")),
        Err(e) => return Main::Failed(format!("{STORE} couldn't be read: {e}")),
    };
    // the Keystore key that sealed it is gone (or changed): as good as lost
    match crate::android::open(&sealed).as_deref().map(decode) {
        Some(Some(me)) => Main::Found(me),
        Some(None) => Main::Missing(format!("{STORE} is damaged")),
        None => Main::Missing("the Android Keystore no longer opens the sealed key".into()),
    }
}

#[cfg(target_os = "android")]
fn save_main(dir: &Path, me: &Identity) -> Result<(), String> {
    let sealed =
        crate::android::seal(&encode(me)).ok_or("the Android Keystore refused to seal the key")?;
    let tmp = dir.join("identity.sealed.tmp");
    std::fs::write(&tmp, sealed).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, dir.join("identity.sealed")).map_err(|e| e.to_string())
}

#[cfg(target_os = "android")]
fn forget_main(dir: &Path) -> Result<(), String> {
    match std::fs::remove_file(dir.join("identity.sealed")) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.to_string()),
    }
}

// ---------------------------------------------------------------- the backup

/// Windows: the key sealed with DPAPI for this Windows user, in Hubchat's own
/// folder. It carries Hubchat through a lost Credential Manager (DPAPI's
/// master keys survived the 2026-10-10 reset), not through a lost DPAPI key
/// (a password reset by an administrator); recovery words cover that.
#[cfg(windows)]
mod backup {
    use std::path::{Path, PathBuf};

    use windows_sys::Win32::Foundation::LocalFree;
    use windows_sys::Win32::Security::Cryptography::{
        CryptProtectData, CryptUnprotectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
    };

    const FILE: &str = "identity-backup.dpapi";
    /// Ties the sealed copy to Hubchat: DPAPI opens it only with these bytes.
    const ENTROPY: &[u8] = b"dev.orgtree.hubchat identity backup v1";

    fn path(dir: &Path) -> PathBuf {
        dir.join(FILE)
    }

    fn blob(b: &[u8]) -> CRYPT_INTEGER_BLOB {
        CRYPT_INTEGER_BLOB {
            cbData: b.len() as u32,
            pbData: b.as_ptr() as *mut u8,
        }
    }

    /// DPAPI's output, copied and freed (wiped first when it is the key).
    fn take(out: CRYPT_INTEGER_BLOB, wipe: bool) -> Vec<u8> {
        // SAFETY: DPAPI allocated `out` (LocalAlloc) with cbData bytes.
        unsafe {
            let v = std::slice::from_raw_parts(out.pbData, out.cbData as usize).to_vec();
            if wipe {
                std::ptr::write_bytes(out.pbData, 0, out.cbData as usize);
            }
            LocalFree(out.pbData as _);
            v
        }
    }

    pub fn seal(text: &str) -> Result<Vec<u8>, String> {
        let (input, entropy) = (blob(text.as_bytes()), blob(ENTROPY));
        let mut out = CRYPT_INTEGER_BLOB { cbData: 0, pbData: std::ptr::null_mut() };
        // SAFETY: the blobs point at live buffers; DPAPI never prompts here.
        let ok = unsafe {
            CryptProtectData(&input, std::ptr::null(), &entropy, std::ptr::null(), std::ptr::null(), CRYPTPROTECT_UI_FORBIDDEN, &mut out)
        };
        if ok == 0 {
            return Err(format!("DPAPI didn't seal it: {}", std::io::Error::last_os_error()));
        }
        Ok(take(out, false))
    }

    pub fn open(sealed: &[u8]) -> Result<String, String> {
        let (input, entropy) = (blob(sealed), blob(ENTROPY));
        let mut out = CRYPT_INTEGER_BLOB { cbData: 0, pbData: std::ptr::null_mut() };
        // SAFETY: as in seal.
        let ok = unsafe {
            CryptUnprotectData(&input, std::ptr::null_mut(), &entropy, std::ptr::null(), std::ptr::null(), CRYPTPROTECT_UI_FORBIDDEN, &mut out)
        };
        if ok == 0 {
            return Err(format!("DPAPI didn't open it: {}", std::io::Error::last_os_error()));
        }
        String::from_utf8(take(out, true)).map_err(|_| "the backup isn't a key".to_string())
    }

    pub fn save(dir: &Path, text: &str) -> Result<(), String> {
        let sealed = seal(text)?;
        let tmp = dir.join(format!("{FILE}.tmp"));
        std::fs::write(&tmp, sealed).map_err(|e| format!("couldn't write it: {e}"))?;
        std::fs::rename(&tmp, path(dir)).map_err(|e| format!("couldn't put it in place: {e}"))
    }

    /// None: there is no backup.
    pub fn load(dir: &Path) -> Result<Option<String>, String> {
        match std::fs::read(path(dir)) {
            Ok(b) => open(&b).map(Some),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e.to_string()),
        }
    }

    pub fn forget(dir: &Path) -> Result<(), String> {
        match std::fs::remove_file(path(dir)) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e.to_string()),
        }
    }
}

/// Elsewhere there is no backup copy: the keyring or the sealed file stands
/// alone (recovery words and linking bring a lost key back).
#[cfg(not(windows))]
mod backup {
    use std::path::Path;

    pub fn save(_dir: &Path, _text: &str) -> Result<(), String> {
        Ok(())
    }
    pub fn load(_dir: &Path) -> Result<Option<String>, String> {
        Ok(None)
    }
    pub fn forget(_dir: &Path) -> Result<(), String> {
        Ok(())
    }
}

#[cfg(all(test, desktop))]
mod tests {
    use super::*;

    #[test]
    fn a_missing_entry_is_a_lost_key_and_other_errors_a_store_that_did_not_answer() {
        let me = Identity::generate("alex").unwrap();
        assert!(matches!(classify(Ok(encode(&me))), Main::Found(f) if f.address() == me.address()));
        assert!(matches!(classify(Err(keyring::Error::NoEntry)), Main::Missing(_)));
        assert!(matches!(classify(Ok("not a key".into())), Main::Missing(_)));
        let failed = classify(Err(keyring::Error::NoStorageAccess(Box::new(std::io::Error::other("busy")))));
        assert!(matches!(failed, Main::Failed(_)));
    }

    // DPAPI needs a signed-in Windows user: a batch or service logon without
    // the password can't open the user's DPAPI keys ("Access is denied"),
    // as measured 2026-10-10 from a session-0 batch logon
    #[cfg(windows)]
    #[test]
    #[ignore = "needs DPAPI: run with --ignored in a signed-in Windows session"]
    fn the_backup_opens_only_what_it_sealed() {
        let dir = std::env::temp_dir().join(format!("hubchat-backup-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let me = Identity::generate("pat").unwrap();
        assert_eq!(backup::load(&dir).unwrap(), None, "no backup yet");
        backup::save(&dir, &encode(&me)).unwrap();
        let file = std::fs::read(dir.join("identity-backup.dpapi")).unwrap();
        assert!(!String::from_utf8_lossy(&file).contains(me.secret()), "the file holds no plain key");
        let back = decode(&backup::load(&dir).unwrap().unwrap()).unwrap();
        assert_eq!((back.id(), back.secret()), (me.id(), me.secret()));
        let mut bent = file.clone();
        let mid = bent.len() / 2;
        bent[mid] ^= 0xff;
        assert!(backup::open(&bent).is_err(), "a changed file doesn't open");
        backup::forget(&dir).unwrap();
        assert_eq!(backup::load(&dir).unwrap(), None, "gone after forget");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(windows)]
    #[test]
    #[ignore = "needs DPAPI: run with --ignored in a signed-in Windows session"]
    fn the_backup_always_holds_the_key_in_use() {
        let dir = std::env::temp_dir().join(format!("hubchat-backup-kept-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let (me, other) = (Identity::generate("pat").unwrap(), Identity::generate("alex").unwrap());
        assert!(keep_backup(&dir, &me).unwrap().0.starts_with("there was no backup; a new one"));
        assert_eq!(keep_backup(&dir, &me), None, "the same key: left alone");
        backup::save(&dir, &encode(&other)).unwrap();
        assert!(keep_backup(&dir, &me).unwrap().0.starts_with("the backup held another key; a new one"));
        assert_eq!(backup::load(&dir).unwrap().as_deref(), Some(encode(&me).as_str()));
        std::fs::write(dir.join("identity-backup.dpapi"), b"not sealed").unwrap();
        assert!(keep_backup(&dir, &me).unwrap().0.starts_with("the backup didn't open"));
        assert_eq!(backup::load(&dir).unwrap().as_deref(), Some(encode(&me).as_str()));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn dbus_probe_confirms_absence_only_when_no_bus_or_not_owned_and_not_activatable() {
        // No session bus configured and no $XDG_RUNTIME_DIR/bus -> confirmed absent.
        assert_eq!(
            classify_dbus_probe(BusEnv::Missing { runtime_bus_exists: Some(false) }, || {
                panic!("must not connect when no session bus exists")
            }),
            SecretServiceState::Absent
        );
        // Chromium DBUS_SESSION_BUS_ADDRESS=disabled: without $XDG_RUNTIME_DIR/bus -> absent.
        assert_eq!(
            classify_dbus_probe(BusEnv::Disabled { runtime_bus_exists: false }, || {
                panic!("must not connect when bus is disabled without runtime bus")
            }),
            SecretServiceState::Absent
        );
        // DBUS_SESSION_BUS_ADDRESS=disabled: WITH an existing runtime bus -> fail closed.
        assert_eq!(
            classify_dbus_probe(BusEnv::Disabled { runtime_bus_exists: true }, || Ok((false, Some(vec![])))),
            SecretServiceState::Indeterminate
        );
        // $XDG_RUNTIME_DIR/bus stat error (e.g. permission denied) -> fail closed.
        assert_eq!(
            classify_dbus_probe(BusEnv::Missing { runtime_bus_exists: None }, || Ok((false, Some(vec![])))),
            SecretServiceState::Indeterminate
        );
        // Configured bus: NameHasOwner=true -> Present.
        assert_eq!(
            classify_dbus_probe(BusEnv::Configured, || Ok((true, None))),
            SecretServiceState::Present
        );
        // Configured bus: idle but activatable org.freedesktop.secrets -> Present.
        assert_eq!(
            classify_dbus_probe(BusEnv::Configured, || {
                Ok((false, Some(vec!["org.freedesktop.Notifications".into(), "org.freedesktop.secrets".into()])))
            }),
            SecretServiceState::Present
        );
        // Configured bus: neither owned nor activatable (Steam Frame VR bus) -> Absent.
        assert_eq!(
            classify_dbus_probe(BusEnv::Configured, || {
                Ok((false, Some(vec!["org.freedesktop.Notifications".into()])))
            }),
            SecretServiceState::Absent
        );
        // Configured bus: connect or query error / timeout -> Indeterminate (fail closed).
        assert_eq!(
            classify_dbus_probe(BusEnv::Configured, || Err(())),
            SecretServiceState::Indeterminate
        );
        // Runtime bus exists, connect fails -> Indeterminate (fail closed).
        assert_eq!(
            classify_dbus_probe(BusEnv::Missing { runtime_bus_exists: Some(true) }, || Err(())),
            SecretServiceState::Indeterminate
        );
    }

    #[test]
    fn posix_trust_enforces_user_owned_0700_dir_0600_file_ancestors_links_and_types() {
        use posix_trust::*;
        let euid = 1000;
        let dir_ok = |uid, mode| StatInfo {
            is_dir: true,
            is_reg: false,
            is_symlink: false,
            uid,
            mode,
            nlink: 2,
            size: 4096,
        };
        let file_ok = |uid, mode, nlink, size| StatInfo {
            is_dir: false,
            is_reg: true,
            is_symlink: false,
            uid,
            mode,
            nlink,
            size,
        };

        assert!(validate_clean_absolute(Path::new("/home/deck/.local/share/dev.orgtree.hubchat")).is_ok());
        assert!(validate_clean_absolute(Path::new("relative/dir")).is_err());
        assert!(validate_clean_absolute(Path::new("/home/deck/../other/.local/share")).is_err());

        // Ancestors: root 0755, root sticky /tmp 01777, euid 0755/0700 ok;
        // foreign uid, non-sticky 0777/0775, euid 0775/0777 rejected.
        assert!(judge_ancestor(euid, dir_ok(0, 0o755)).is_ok());
        assert!(judge_ancestor(euid, dir_ok(0, 0o1777)).is_ok());
        assert!(judge_ancestor(euid, dir_ok(euid, 0o755)).is_ok());
        assert!(judge_ancestor(euid, dir_ok(euid, 0o700)).is_ok());
        assert!(judge_ancestor(euid, dir_ok(1001, 0o755)).is_err());
        assert!(judge_ancestor(euid, dir_ok(0, 0o777)).is_err());
        assert!(judge_ancestor(euid, dir_ok(0, 0o775)).is_err());
        assert!(judge_ancestor(euid, dir_ok(euid, 0o775)).is_err());
        assert!(judge_ancestor(euid, dir_ok(euid, 0o777)).is_err());

        // App dir: must be euid-owned dir, not group/world-writable.
        assert!(judge_app_dir(euid, dir_ok(euid, 0o700)).is_ok());
        assert!(judge_app_dir(euid, dir_ok(euid, 0o755)).is_ok());
        assert!(judge_app_dir(euid, dir_ok(0, 0o1777)).is_err());
        assert!(judge_app_dir(euid, dir_ok(0, 0o755)).is_err());
        assert!(judge_app_dir(euid, dir_ok(1001, 0o700)).is_err());
        assert!(judge_app_dir(euid, dir_ok(euid, 0o775)).is_err());

        // Key store subdir: strictly euid-owned 0700 dir.
        assert!(judge_store_dir(euid, dir_ok(euid, 0o700)).is_ok());
        assert!(judge_store_dir(euid, dir_ok(euid, 0o755)).is_err());
        assert!(judge_store_dir(euid, dir_ok(euid, 0o750)).is_err());
        assert!(judge_store_dir(euid, dir_ok(euid, 0o770)).is_err());
        assert!(judge_store_dir(euid, dir_ok(0, 0o700)).is_err());
        assert!(judge_store_dir(euid, file_ok(euid, 0o700, 1, 0)).is_err());

        // Key file: regular, euid-owned, no group/other bits, nlink==1, bounded size.
        assert!(judge_key_file(euid, file_ok(euid, 0o600, 1, 128)).is_ok());
        assert!(judge_key_file(euid, file_ok(euid, 0o400, 1, 128)).is_ok());
        assert!(judge_key_file(euid, file_ok(euid, 0o640, 1, 128)).is_err());
        assert!(judge_key_file(euid, file_ok(euid, 0o644, 1, 128)).is_err());
        assert!(judge_key_file(euid, file_ok(euid, 0o600, 2, 128)).is_err());
        assert!(judge_key_file(euid, file_ok(1001, 0o600, 1, 128)).is_err());
        assert!(judge_key_file(euid, file_ok(euid, 0o600, 1, MAX_KEY_BYTES + 1)).is_err());
        // FIFO / non-regular object rejected.
        assert!(judge_key_file(
            euid,
            StatInfo {
                is_dir: false,
                is_reg: false,
                is_symlink: false,
                uid: euid,
                mode: 0o600,
                nlink: 1,
                size: 0,
            }
        )
        .is_err());
    }

    struct FakeBackend {
        probe: std::cell::Cell<SecretServiceState>,
        keyring: std::cell::RefCell<Result<Option<String>, String>>,
        keyring_save_err: std::cell::RefCell<Option<String>>,
        keyring_forget_err: std::cell::RefCell<Option<String>>,
        file: std::cell::RefCell<Result<Option<String>, String>>,
        file_save_err: std::cell::RefCell<Option<String>>,
        file_forget_err: std::cell::RefCell<Option<String>>,
        file_reads: std::cell::Cell<usize>,
        file_writes: std::cell::Cell<usize>,
        keyring_writes: std::cell::Cell<usize>,
    }

    impl FakeBackend {
        fn new(probe: SecretServiceState) -> Self {
            Self {
                probe: std::cell::Cell::new(probe),
                keyring: std::cell::RefCell::new(Ok(None)),
                keyring_save_err: std::cell::RefCell::new(None),
                keyring_forget_err: std::cell::RefCell::new(None),
                file: std::cell::RefCell::new(Ok(None)),
                file_save_err: std::cell::RefCell::new(None),
                file_forget_err: std::cell::RefCell::new(None),
                file_reads: std::cell::Cell::new(0),
                file_writes: std::cell::Cell::new(0),
                keyring_writes: std::cell::Cell::new(0),
            }
        }
    }

    impl linux_policy::Backend for FakeBackend {
        fn probe(&self) -> SecretServiceState {
            self.probe.get()
        }
        fn keyring_load(&self) -> Main {
            match self.keyring.borrow().as_ref() {
                Ok(Some(s)) => decode(s).map_or_else(|| Main::Missing("damaged".into()), Main::Found),
                Ok(None) => Main::Missing("no key in keyring".into()),
                Err(e) => Main::Failed(e.clone()),
            }
        }
        fn keyring_save(&self, me: &Identity) -> Result<(), String> {
            self.keyring_writes.set(self.keyring_writes.get() + 1);
            if let Some(e) = self.keyring_save_err.borrow().as_ref() {
                return Err(e.clone());
            }
            *self.keyring.borrow_mut() = Ok(Some(encode(me)));
            Ok(())
        }
        fn keyring_forget(&self) -> Result<(), String> {
            if let Some(e) = self.keyring_forget_err.borrow().as_ref() {
                return Err(e.clone());
            }
            *self.keyring.borrow_mut() = Ok(None);
            Ok(())
        }
        fn file_load(&self) -> Result<Option<String>, String> {
            self.file_reads.set(self.file_reads.get() + 1);
            self.file.borrow().clone()
        }
        fn file_save(&self, encoded: &str) -> Result<(), String> {
            self.file_writes.set(self.file_writes.get() + 1);
            if let Some(e) = self.file_save_err.borrow().as_ref() {
                return Err(e.clone());
            }
            *self.file.borrow_mut() = Ok(Some(encoded.to_string()));
            Ok(())
        }
        fn file_forget(&self) -> Result<(), String> {
            if let Some(e) = self.file_forget_err.borrow().as_ref() {
                return Err(e.clone());
            }
            *self.file.borrow_mut() = Ok(None);
            Ok(())
        }
    }

    #[test]
    fn linux_policy_never_uses_file_when_secret_service_is_present_or_indeterminate() {
        use linux_policy::*;
        let stale = Identity::generate("stale").unwrap();
        let fresh = Identity::generate("fresh").unwrap();

        for state in [SecretServiceState::Present, SecretServiceState::Indeterminate] {
            // 1. Keyring NoEntry + older fallback file -> Missing (file NEVER read)
            let b = FakeBackend::new(state);
            *b.file.borrow_mut() = Ok(Some(encode(&stale)));
            assert!(matches!(load_with(&b), PolicyLoaded::Missing), "state={state:?}");
            assert_eq!(b.file_reads.get(), 0, "must not read file when state={state:?}");

            // 2. Keyring locked / denied / timeout / bus error + older fallback file -> Unreadable
            *b.keyring.borrow_mut() = Err("collection locked".into());
            assert!(matches!(load_with(&b), PolicyLoaded::Unreadable), "state={state:?}");
            assert_eq!(b.file_reads.get(), 0);

            // 3. Keyring damaged credential + older fallback file -> Missing
            *b.keyring.borrow_mut() = Ok(Some("not-a-key".into()));
            assert!(matches!(load_with(&b), PolicyLoaded::Missing), "state={state:?}");
            assert_eq!(b.file_reads.get(), 0);

            // 4. Keyring save failure -> Err (NEVER falls back to file)
            *b.keyring_save_err.borrow_mut() = Some("unlock denied".into());
            assert!(save_with(&b, &fresh).is_err(), "state={state:?}");
            assert_eq!(b.file_writes.get(), 0);

            // 5. Keyring forget failure -> Err (preserves keyring error)
            *b.keyring_forget_err.borrow_mut() = Some("bus timeout".into());
            assert!(forget_with(&b).is_err(), "state={state:?}");
        }
    }

    #[test]
    fn linux_policy_uses_file_only_when_secret_service_is_confirmed_absent_and_handles_appearance() {
        use linux_policy::*;
        let alice = Identity::generate("alice").unwrap();
        let b = FakeBackend::new(SecretServiceState::Absent);

        // Save under Absent -> file storage, never touches keyring.
        assert_eq!(save_with(&b, &alice).unwrap(), KeyStorage::FilePermissionsOnly);
        assert_eq!(b.keyring_writes.get(), 0);
        assert_eq!(b.file_writes.get(), 1);

        // Load under Absent -> Found from FilePermissionsOnly.
        match load_with(&b) {
            PolicyLoaded::Found { identity, storage } => {
                assert_eq!(storage, KeyStorage::FilePermissionsOnly);
                assert_eq!(identity.id(), alice.id());
                assert_eq!(identity.secret(), alice.secret());
            }
            _ => panic!("expected Found from FilePermissionsOnly"),
        }

        // Secret Service appears later -> load does NOT silently read or auto-migrate the file;
        // returns Missing (LostKey screen with chats intact), keeping file untouched until user acts.
        b.probe.set(SecretServiceState::Present);
        let reads_before = b.file_reads.get();
        assert!(matches!(load_with(&b), PolicyLoaded::Missing));
        assert_eq!(b.file_reads.get(), reads_before);
        assert!(b.file.borrow().as_ref().unwrap().is_some(), "file preserved until user acts");

        // User restores Alice into Secret Service -> saved in keyring AND stale file removed!
        assert_eq!(save_with(&b, &alice).unwrap(), KeyStorage::Keyring);
        assert_eq!(*b.file.borrow(), Ok(None), "stale fallback file must be removed on keyring save");

        // If Secret Service later disappears again -> stale file is gone, returns Missing.
        b.probe.set(SecretServiceState::Absent);
        assert!(matches!(load_with(&b), PolicyLoaded::Missing));

        // Save under Absent, then forget under Absent -> succeeds without keyring and removes file.
        assert_eq!(save_with(&b, &alice).unwrap(), KeyStorage::FilePermissionsOnly);
        forget_with(&b).unwrap();
        assert_eq!(*b.file.borrow(), Ok(None));
        assert!(matches!(load_with(&b), PolicyLoaded::Missing));

        // Forget under Present removes BOTH keyring and any leftover fallback file.
        b.probe.set(SecretServiceState::Present);
        *b.keyring.borrow_mut() = Ok(Some(encode(&alice)));
        *b.file.borrow_mut() = Ok(Some(encode(&alice)));
        forget_with(&b).unwrap();
        assert_eq!(*b.keyring.borrow(), Ok(None));
        assert_eq!(*b.file.borrow(), Ok(None));

        // Corrupt or untrusted file under Absent -> Missing / Unreadable.
        b.probe.set(SecretServiceState::Absent);
        *b.file.borrow_mut() = Ok(Some("corrupt".into()));
        assert!(matches!(load_with(&b), PolicyLoaded::Missing));
        *b.file.borrow_mut() = Err("unsafe permissions".into());
        assert!(matches!(load_with(&b), PolicyLoaded::Unreadable));
    }
}

/// Linux keyring round trip, run by CI (build-linux.yml) in three setups:
/// no D-Bus session, a session without a keyring daemon, and a session
/// with an unlocked GNOME Keyring. HUBCHAT_KEYRING_EXPECT says which
/// outcome is right: `fail` (saving fails plainly and nothing is kept in
/// memory) or `ok` (save, load and forget all work).
#[cfg(all(test, target_os = "linux"))]
mod linux_keyring {
    use super::*;

    #[test]
    #[ignore = "needs a chosen D-Bus/keyring setup; run by CI"]
    fn round_trip() {
        let expect = std::env::var("HUBCHAT_KEYRING_EXPECT").expect("HUBCHAT_KEYRING_EXPECT");
        let dir = std::env::temp_dir().join(format!("hubchat-keyring-{}", std::process::id()));
        let me = Identity::generate("ci").unwrap();
        let saved = save_identity(&dir, &me);
        eprintln!("save_identity: {saved:?}");
        match expect.as_str() {
            "fail" => {
                assert!(saved.is_err(), "saving must fail without a keyring");
                assert!(!matches!(load_identity(&dir), Loaded::Found(_)), "nothing may be kept in memory");
            }
            "ok" => {
                saved.unwrap();
                let Loaded::Found(back) = load_identity(&dir) else { panic!("load after save") };
                assert_eq!(back.id(), me.id());
                assert_eq!(back.secret(), me.secret());
                forget_identity(&dir).unwrap();
                assert!(matches!(load_identity(&dir), Loaded::Missing), "gone after forget");
            }
            other => panic!("HUBCHAT_KEYRING_EXPECT={other}"),
        }
    }
}
