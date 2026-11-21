//! Where the identity key lives. Windows: Credential Manager, plus a backup
//! copy in Hubchat's own folder sealed with Windows' DPAPI for this user
//! (user 2026-10-10 08:13Z, after Windows lost every saved sign-in in a
//! crash and Hubchat started over without a word). macOS: Keychain. Linux:
//! the desktop Secret Service keyring, or — ONLY when no Secret Service is
//! owned or activatable on the session bus — a private file in a user-owned
//! 0700 folder with 0600 permissions (user ruling 2026-11-21 `mcf5eec9b9660`).
//! Android: a file in app-private storage sealed with an Android Keystore key
//! (SecretBox.kt), with app backup disabled. The key never goes to the UI or
//! into logs.

use std::path::Path;

use hubchat_core::Identity;

use crate::keylog::{self, Fact};

#[cfg(desktop)]
const SERVICE: &str = "dev.orgtree.hubchat";

#[cfg(any(target_os = "linux", test))]
const SECRETS_BUS_NAME: &str = "org.freedesktop.secrets";

/// What the main copy is kept in, for the key log.
const STORE: &str = if cfg!(windows) {
    "Credential Manager"
} else if cfg!(target_os = "android") {
    "the sealed key file"
} else {
    "the keyring"
};

/// Overwrite sensitive bytes in place with a volatile write and fence so the
/// compiler cannot elide the wipe.
fn wipe_bytes(bytes: &mut [u8]) {
    for b in bytes.iter_mut() {
        // SAFETY: `b` points to a valid initialized mutable byte.
        unsafe { std::ptr::write_volatile(b, 0) };
    }
    std::sync::atomic::compiler_fence(std::sync::atomic::Ordering::SeqCst);
}

/// Sensitive UTF-8 text wiped from memory on drop.
pub(crate) struct SecretText(String);

impl SecretText {
    fn new(s: String) -> Self {
        Self(s)
    }
    fn as_str(&self) -> &str {
        &self.0
    }
}

impl Drop for SecretText {
    fn drop(&mut self) {
        // SAFETY: writing all-zero bytes keeps the buffer valid UTF-8.
        wipe_bytes(unsafe { self.0.as_bytes_mut() });
    }
}

fn encode(me: &Identity) -> SecretText {
    SecretText::new(format!("{} {}", me.id(), me.secret()))
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
    /// Linux-only fallback used solely when no Secret Service exists: a private
    /// file in Hubchat's data folder protected only by user file permissions (0700/0600).
    #[cfg_attr(not(any(target_os = "linux", test)), allow(dead_code))]
    FilePermissionsOnly,
}

/// What start-up found.
pub enum Loaded {
    Found {
        identity: Identity,
        storage: KeyStorage,
    },
    /// The main copy was gone (or didn't answer) and the backup carried the
    /// key; the main copy was put back from it where Windows allowed.
    #[cfg_attr(not(windows), allow(dead_code))]
    Restored {
        identity: Identity,
        storage: KeyStorage,
    },
    Missing,
    /// The key store didn't answer: it may still hold the key.
    Unreadable,
}

/// What the main copy gave.
pub(crate) enum Main {
    Found(Identity),
    /// Not there, or there but no longer usable (why, for the key log).
    Missing(String),
    /// The key store failed to answer.
    Failed(String),
}

/// Whether a Linux Secret Service (`org.freedesktop.secrets`) is reachable.
#[cfg(any(target_os = "linux", test))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SecretServiceState {
    /// Owned on the session bus or listed in `ListActivatableNames`.
    Present,
    /// Confirmed absent (no session bus, or bus confirmed neither owned nor activatable).
    Absent,
    /// Could not determine (bus error or timeout); fail closed and preserve keyring behavior.
    Indeterminate,
}

#[cfg(any(target_os = "linux", test))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BusEnv {
    /// `DBUS_SESSION_BUS_ADDRESS` is set to a non-empty address other than `disabled:`.
    Configured,
    /// `DBUS_SESSION_BUS_ADDRESS=disabled:` (e.g. inherited from Chromium with no bus).
    Disabled { runtime_bus_exists: bool },
    /// `DBUS_SESSION_BUS_ADDRESS` is unset or empty; `None` means stat on `$XDG_RUNTIME_DIR/bus` errored.
    Missing { runtime_bus_exists: Option<bool> },
}

#[cfg(any(target_os = "linux", test))]
pub(crate) fn classify_dbus_probe(
    bus_env: BusEnv,
    probe: impl FnOnce() -> Result<(bool, Option<Vec<String>>), ()>,
) -> SecretServiceState {
    match bus_env {
        BusEnv::Missing {
            runtime_bus_exists: Some(false),
        } => return SecretServiceState::Absent,
        BusEnv::Missing {
            runtime_bus_exists: None,
        } => return SecretServiceState::Indeterminate,
        BusEnv::Disabled {
            runtime_bus_exists: false,
        } => return SecretServiceState::Absent,
        BusEnv::Disabled {
            runtime_bus_exists: true,
        } => return SecretServiceState::Indeterminate,
        BusEnv::Configured
        | BusEnv::Missing {
            runtime_bus_exists: Some(true),
        } => {}
    }
    match probe() {
        Ok((true, _)) => SecretServiceState::Present,
        Ok((false, Some(names))) => {
            if names.iter().any(|n| n == SECRETS_BUS_NAME) {
                SecretServiceState::Present
            } else {
                SecretServiceState::Absent
            }
        }
        Ok((false, None)) | Err(()) => SecretServiceState::Indeterminate,
    }
}

#[cfg(any(target_os = "linux", test))]
pub(crate) mod posix_trust {
    use std::path::{Component, Path};

    pub(crate) const MAX_KEY_BYTES: i64 = 4096;

    // Guarantee every valid `Identity` (`"<id> <secret>"`) fits within `MAX_KEY_BYTES`.
    const _: () = assert!(
        (hubchat_core::recovery::MAX_ID_LEN + 1 + hubchat_core::identity::MAX_SECRET_LEN)
            <= MAX_KEY_BYTES as usize
    );

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

    /// Require a clean POSIX absolute path (leading `/`, at least one normal
    /// component, no `.` or `..` components, no Windows-style prefix).
    pub(crate) fn validate_clean_absolute(path: &Path) -> Result<(), &'static str> {
        let mut comps = path.components();
        if !matches!(comps.next(), Some(Component::RootDir)) {
            return Err("path must be absolute");
        }
        let mut has_normal = false;
        for c in comps {
            match c {
                Component::Normal(_) => has_normal = true,
                Component::ParentDir | Component::CurDir | Component::Prefix(_) | Component::RootDir => {
                    return Err("path contains disallowed components");
                }
            }
        }
        if !has_normal {
            return Err("path cannot be filesystem root");
        }
        Ok(())
    }

    /// Every ancestor directory above the app data dir must be owned by either
    /// the effective user or root; group/world-writable ancestors are rejected
    /// unless root-owned with the sticky bit set (such as `/tmp`).
    pub(crate) fn judge_ancestor(euid: u32, st: StatInfo) -> Result<(), &'static str> {
        if !st.is_dir || st.is_symlink {
            return Err("ancestor is not a real directory");
        }
        if st.uid != euid && st.uid != 0 {
            return Err("ancestor is owned by another user");
        }
        let others_write = (st.mode & 0o022) != 0;
        let root_sticky = st.uid == 0 && (st.mode & 0o1000) != 0;
        if others_write && !root_sticky {
            return Err("ancestor is writable by other users");
        }
        Ok(())
    }

    /// Hubchat's data dir (or an `XDG_DATA_HOME` root) must be owned by the
    /// effective user and not group/world-writable.
    pub(crate) fn judge_app_dir(euid: u32, st: StatInfo) -> Result<(), &'static str> {
        if !st.is_dir || st.is_symlink {
            return Err("data directory is not a real directory");
        }
        if st.uid != euid {
            return Err("data directory is not owned by the effective user");
        }
        if (st.mode & 0o022) != 0 {
            return Err("data directory is writable by other users");
        }
        Ok(())
    }

    /// The dedicated key-store subdirectory must be strictly `0700` (no
    /// group/other or special bits) and owned by the effective user.
    pub(crate) fn judge_store_dir(euid: u32, st: StatInfo) -> Result<(), &'static str> {
        if !st.is_dir || st.is_symlink {
            return Err("key store directory is not a real directory");
        }
        if st.uid != euid {
            return Err("key store directory is not owned by the effective user");
        }
        if (st.mode & 0o7777) != 0o700 {
            return Err("key store directory must have 0700 permissions");
        }
        Ok(())
    }

    /// The key file must be a regular file owned by the effective user, with
    /// no group/other/exec/special permissions, no hard links, and bounded size.
    pub(crate) fn judge_key_file(euid: u32, st: StatInfo) -> Result<(), &'static str> {
        if !st.is_reg || st.is_symlink || st.is_dir {
            return Err("key path is not a regular file");
        }
        if st.uid != euid {
            return Err("key file is not owned by the effective user");
        }
        if (st.mode & 0o7177) != 0 {
            return Err("key file permissions are accessible to other users");
        }
        if st.nlink == 0 {
            return Err("key file was unlinked during open");
        }
        if st.nlink > 1 {
            return Err("key file must not have additional hard links");
        }
        if st.size < 0 || st.size > MAX_KEY_BYTES {
            return Err("key file size is out of bounds");
        }
        Ok(())
    }
}

#[cfg(any(target_os = "linux", test))]
pub(crate) mod linux_policy {
    use super::{decode, encode, KeyStorage, Main, SecretServiceState, SecretText};
    use crate::keylog::Fact;
    use hubchat_core::Identity;

    pub(crate) enum PolicyLoaded {
        Found { identity: Identity, storage: KeyStorage },
        Missing,
        Unreadable,
    }

    pub(crate) struct LoadOutcome {
        pub loaded: PolicyLoaded,
        pub log_line: String,
        pub facts: Vec<Fact>,
    }

    pub(crate) trait Backend {
        fn probe(&self) -> SecretServiceState;
        fn keyring_load(&self) -> Main;
        fn keyring_save(&self, me: &Identity) -> Result<(), String>;
        fn keyring_forget(&self) -> Result<(), String>;
        fn file_load(&self) -> Result<Option<SecretText>, String>;
        fn file_save(&self, encoded: &str) -> Result<(), String>;
        /// Returns `Ok(true)` if a fallback file was removed, `Ok(false)` if none existed.
        fn file_forget(&self) -> Result<bool, String>;
        fn file_exists_hint(&self) -> bool {
            false
        }
    }

    pub(crate) fn load_with(b: &impl Backend) -> LoadOutcome {
        let state = b.probe();
        match state {
            SecretServiceState::Present | SecretServiceState::Indeterminate => {
                let indet = state == SecretServiceState::Indeterminate;
                match b.keyring_load() {
                    Main::Found(identity) => {
                        let (extra, mut facts) = (String::new(), vec![Fact::KeyFound]);
                        let (extra, extra_fact) = match b.file_forget() {
                            Ok(true) => ("; stale permissions-only key file removed".to_string(), Some(Fact::KeyFileRemoved)),
                            Ok(false) => (extra, None),
                            Err(e) => (format!("; stale permissions-only key file couldn't be removed ({e})"), Some(Fact::KeyFileCleanupFailed)),
                        };
                        if let Some(f) = extra_fact {
                            facts.push(f);
                        }
                        LoadOutcome {
                            loaded: PolicyLoaded::Found {
                                identity,
                                storage: KeyStorage::Keyring,
                            },
                            log_line: format!("key found in the keyring{extra}"),
                            facts,
                        }
                    }
                    Main::Missing(why) => {
                        let mut facts = Vec::new();
                        if indet {
                            facts.push(Fact::SecretServiceIndeterminate);
                        }
                        facts.push(Fact::StoreHasNoKey);
                        let mut line = why;
                        if b.file_exists_hint() {
                            line.push_str("; permissions-only key file ignored while Secret Service is available");
                            facts.push(Fact::KeyFileIgnoredWithSecretService);
                        }
                        LoadOutcome {
                            loaded: PolicyLoaded::Missing,
                            log_line: line,
                            facts,
                        }
                    }
                    Main::Failed(why) => {
                        let mut facts = Vec::new();
                        if indet {
                            facts.push(Fact::SecretServiceIndeterminate);
                        }
                        facts.push(Fact::StoreDidNotAnswer);
                        let mut line = why;
                        if b.file_exists_hint() {
                            line.push_str("; permissions-only key file ignored while Secret Service is available");
                            facts.push(Fact::KeyFileIgnoredWithSecretService);
                        }
                        LoadOutcome {
                            loaded: PolicyLoaded::Unreadable,
                            log_line: line,
                            facts,
                        }
                    }
                }
            }
            SecretServiceState::Absent => match b.file_load() {
                Ok(Some(text)) => match decode(text.as_str()) {
                    Some(identity) => LoadOutcome {
                        loaded: PolicyLoaded::Found {
                            identity,
                            storage: KeyStorage::FilePermissionsOnly,
                        },
                        log_line: "key found in Hubchat's permissions-only key file (no Secret Service available)".into(),
                        facts: vec![Fact::NoSecretService, Fact::KeyFoundInFile],
                    },
                    None => LoadOutcome {
                        loaded: PolicyLoaded::Missing,
                        log_line: "no Secret Service available; Hubchat's permissions-only key file is damaged".into(),
                        facts: vec![Fact::NoSecretService, Fact::KeyFileDamaged],
                    },
                },
                Ok(None) => LoadOutcome {
                    loaded: PolicyLoaded::Missing,
                    log_line: "no Secret Service available; no permissions-only key file".into(),
                    facts: vec![Fact::NoSecretService, Fact::KeyFileMissing],
                },
                Err(why) => LoadOutcome {
                    loaded: PolicyLoaded::Unreadable,
                    log_line: format!(
                        "no Secret Service available; Hubchat's permissions-only key file wasn't trusted or couldn't open ({why})"
                    ),
                    facts: vec![Fact::NoSecretService, Fact::KeyFileUnreadable],
                },
            },
        }
    }

    pub(crate) struct SaveOutcome {
        pub storage: KeyStorage,
        pub log_line: Option<String>,
        pub facts: Vec<Fact>,
    }

    pub(crate) fn save_with(b: &impl Backend, me: &Identity) -> Result<SaveOutcome, String> {
        match b.probe() {
            SecretServiceState::Present | SecretServiceState::Indeterminate => {
                b.keyring_save(me)?;
                match b.file_forget() {
                    Ok(true) => Ok(SaveOutcome {
                        storage: KeyStorage::Keyring,
                        log_line: Some("key saved in the keyring; stale permissions-only key file removed".into()),
                        facts: vec![Fact::KeyFileRemoved],
                    }),
                    Ok(false) => Ok(SaveOutcome {
                        storage: KeyStorage::Keyring,
                        log_line: None,
                        facts: vec![],
                    }),
                    Err(e) => Err(format!("key saved in the keyring, but could not remove stale key file: {e}")),
                }
            }
            SecretServiceState::Absent => {
                let encoded = encode(me);
                b.file_save(encoded.as_str())?;
                Ok(SaveOutcome {
                    storage: KeyStorage::FilePermissionsOnly,
                    log_line: Some("key saved in Hubchat's permissions-only key file (no Secret Service available)".into()),
                    facts: vec![Fact::NoSecretService, Fact::KeySavedInFile],
                })
            }
        }
    }

    pub(crate) fn forget_with(b: &impl Backend) -> Result<(), String> {
        match b.probe() {
            SecretServiceState::Present | SecretServiceState::Indeterminate => {
                b.keyring_forget()?;
                b.file_forget()?;
                Ok(())
            }
            SecretServiceState::Absent => {
                b.file_forget()?;
                Ok(())
            }
        }
    }
}

/// The key, from the main copy or else the backup (start-up and Try again).
#[cfg(not(target_os = "linux"))]
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
            return Loaded::Found {
                identity: me,
                storage: KeyStorage::Keyring,
            };
        }
        Main::Missing(why) => (why, false, Fact::StoreHasNoKey),
        Main::Failed(why) => (why, true, Fact::StoreDidNotAnswer),
    };
    match backup::load(dir) {
        Ok(Some(text)) => {
            if let Some(me) = decode(text.as_str()) {
                let (back, fact) = match save_main(dir, &me) {
                    Ok(()) => (format!("put back into {STORE}"), Fact::PutBackInStore),
                    Err(e) => (
                        format!("{STORE} refused it back ({e}); running from the backup"),
                        Fact::StoreRefusedItBack,
                    ),
                };
                keylog::note(
                    dir,
                    &format!("{why}; key restored from Hubchat's backup, {back}"),
                    &[main, Fact::RestoredFromBackup, fact],
                );
                return Loaded::Restored {
                    identity: me,
                    storage: KeyStorage::Keyring,
                };
            }
            keylog::note(
                dir,
                &format!("{why}; Hubchat's backup is damaged"),
                &[main, Fact::BackupDamaged],
            );
        }
        Ok(None) => keylog::note(dir, &format!("{why}; no backup"), &[main, Fact::NoBackup]),
        Err(e) => keylog::note(
            dir,
            &format!("{why}; the backup didn't open ({e})"),
            &[main, Fact::BackupUnreadable],
        ),
    }
    if failed {
        Loaded::Unreadable
    } else {
        Loaded::Missing
    }
}

/// Linux: probe Secret Service on the session D-Bus; only when confirmed absent,
/// use Hubchat's permissions-only 0700/0600 file.
#[cfg(target_os = "linux")]
pub fn load_identity(dir: &Path) -> Loaded {
    let backend = LinuxBackend { dir };
    let out = linux_policy::load_with(&backend);
    keylog::note(dir, &out.log_line, &out.facts);
    match out.loaded {
        linux_policy::PolicyLoaded::Found { identity, storage } => Loaded::Found { identity, storage },
        linux_policy::PolicyLoaded::Missing => Loaded::Missing,
        linux_policy::PolicyLoaded::Unreadable => Loaded::Unreadable,
    }
}

/// Keep the key: the main copy must take it; the backup follows where there
/// is one (if it can't, there is no backup, and the key log says why).
#[cfg(not(target_os = "linux"))]
pub fn save_identity(dir: &Path, me: &Identity) -> Result<KeyStorage, String> {
    save_main(dir, me)?;
    if let Err(e) = write_backup(dir, me) {
        keylog::note(dir, &e, &[Fact::BackupNotWritten]);
    }
    Ok(KeyStorage::Keyring)
}

#[cfg(target_os = "linux")]
pub fn save_identity(dir: &Path, me: &Identity) -> Result<KeyStorage, String> {
    let backend = LinuxBackend { dir };
    match linux_policy::save_with(&backend, me) {
        Ok(out) => {
            if let Some(line) = out.log_line {
                keylog::note(dir, &line, &out.facts);
            }
            Ok(out.storage)
        }
        Err(e) => {
            if e.contains("stale key file") {
                keylog::note(dir, &e, &[Fact::KeyFileCleanupFailed]);
            }
            Err(e)
        }
    }
}

/// Forget the key: the main copy and any backup or fallback file.
#[cfg(not(target_os = "linux"))]
pub fn forget_identity(dir: &Path) -> Result<(), String> {
    forget_main(dir)?;
    backup::forget(dir)
}

#[cfg(target_os = "linux")]
pub fn forget_identity(dir: &Path) -> Result<(), String> {
    let backend = LinuxBackend { dir };
    linux_policy::forget_with(&backend)
}

/// Remove any Linux permissions-only fallback file (no-op elsewhere). Called
/// before wiping local data when switching to another identity so an old
/// fallback file can never linger next to a wiped store.
pub fn forget_fallback_file(dir: &Path) -> Result<(), String> {
    #[cfg(target_os = "linux")]
    {
        linux_file::forget(dir).map(|_| ())
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = dir;
        Ok(())
    }
}

/// Ensure Hubchat's Linux data directory is created with user-only `0700`
/// permissions before crash/logging/core setup creates it under a looser umask.
#[cfg(target_os = "linux")]
pub fn ensure_private_app_dir(dir: &Path) {
    use std::os::unix::fs::DirBuilderExt;
    if std::fs::symlink_metadata(dir).is_ok() {
        return;
    }
    if let Some(parent) = dir.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::DirBuilder::new().mode(0o700).create(dir);
}

/// While the main copy has the key, the backup holds that same key or none:
/// an older Hubchat made none, and one left from another identity (when a
/// new key's backup couldn't be written) must never come back in its place.
/// Says what it did, for the key log: the file's line and the fact the normal
/// log may repeat.
#[cfg(not(target_os = "linux"))]
fn keep_backup(dir: &Path, me: &Identity) -> Option<(String, Fact)> {
    let encoded = encode(me);
    let found = match backup::load(dir) {
        Ok(Some(text)) if text.as_str().trim() == encoded.as_str() => return None,
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
#[cfg(not(target_os = "linux"))]
fn write_backup(dir: &Path, me: &Identity) -> Result<(), String> {
    let encoded = encode(me);
    let e = match backup::save(dir, encoded.as_str()) {
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
        Ok(s) => {
            let secret = SecretText::new(s);
            decode(secret.as_str())
                .map_or_else(|| Main::Missing(format!("the key in {STORE} is damaged")), Main::Found)
        }
        Err(keyring::Error::NoEntry) => Main::Missing(format!("no key in {STORE}")),
        Err(e) => Main::Failed(format!("{STORE} failed: {e}")),
    }
}

#[cfg(desktop)]
fn save_main(dir: &Path, me: &Identity) -> Result<(), String> {
    let encoded = encode(me);
    entry(dir)?
        .set_password(encoded.as_str())
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
    match crate::android::open(&sealed).map(SecretText::new) {
        Some(open) => match decode(open.as_str()) {
            Some(me) => Main::Found(me),
            None => Main::Missing(format!("{STORE} is damaged")),
        },
        None => Main::Missing("the Android Keystore no longer opens the sealed key".into()),
    }
}

#[cfg(target_os = "android")]
fn save_main(dir: &Path, me: &Identity) -> Result<(), String> {
    let encoded = encode(me);
    let sealed = crate::android::seal(encoded.as_str())
        .ok_or("the Android Keystore refused to seal the key")?;
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

// ------------------------------------------------ Linux Secret Service + file

#[cfg(target_os = "linux")]
struct LinuxBackend<'a> {
    dir: &'a Path,
}

#[cfg(target_os = "linux")]
impl linux_policy::Backend for LinuxBackend<'_> {
    fn probe(&self) -> SecretServiceState {
        linux_dbus::probe_secret_service()
    }
    fn keyring_load(&self) -> Main {
        load_main(self.dir)
    }
    fn keyring_save(&self, me: &Identity) -> Result<(), String> {
        save_main(self.dir, me)
    }
    fn keyring_forget(&self) -> Result<(), String> {
        forget_main(self.dir)
    }
    fn file_load(&self) -> Result<Option<SecretText>, String> {
        linux_file::load(self.dir)
    }
    fn file_save(&self, encoded: &str) -> Result<(), String> {
        linux_file::save(self.dir, encoded)
    }
    fn file_forget(&self) -> Result<bool, String> {
        linux_file::forget(self.dir)
    }
    fn file_exists_hint(&self) -> bool {
        linux_file::exists_hint(self.dir)
    }
}

#[cfg(target_os = "linux")]
mod linux_dbus {
    use std::path::Path;
    use std::time::Duration;

    use dbus::blocking::Connection;

    use super::{classify_dbus_probe, BusEnv, SecretServiceState, SECRETS_BUS_NAME};

    const PROBE_TIMEOUT: Duration = Duration::from_secs(2);

    fn runtime_bus_stat() -> Option<bool> {
        let Some(runtime) = std::env::var_os("XDG_RUNTIME_DIR").filter(|v| !v.is_empty()) else {
            return Some(false);
        };
        match std::fs::symlink_metadata(Path::new(&runtime).join("bus")) {
            Ok(_) => Some(true),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Some(false),
            Err(_) => None,
        }
    }

    fn current_bus_env() -> BusEnv {
        match std::env::var("DBUS_SESSION_BUS_ADDRESS") {
            Ok(addr) if addr == "disabled:" => BusEnv::Disabled {
                runtime_bus_exists: runtime_bus_stat().unwrap_or(true),
            },
            Ok(addr) if !addr.is_empty() => BusEnv::Configured,
            _ => BusEnv::Missing {
                runtime_bus_exists: runtime_bus_stat(),
            },
        }
    }

    pub(super) fn probe_secret_service() -> SecretServiceState {
        classify_dbus_probe(current_bus_env(), || {
            let conn = Connection::new_session().map_err(|_| ())?;
            let proxy = conn.with_proxy("org.freedesktop.DBus", "/org/freedesktop/DBus", PROBE_TIMEOUT);
            let (owned,): (bool,) = proxy
                .method_call("org.freedesktop.DBus", "NameHasOwner", (SECRETS_BUS_NAME,))
                .map_err(|_| ())?;
            if owned {
                return Ok((true, None));
            }
            let (activatable,): (Vec<String>,) = proxy
                .method_call("org.freedesktop.DBus", "ListActivatableNames", ())
                .map_err(|_| ())?;
            Ok((false, Some(activatable)))
        })
    }
}

#[cfg(target_os = "linux")]
mod linux_file {
    use std::collections::VecDeque;
    use std::ffi::{CString, OsStr, OsString};
    use std::io::{Read, Write};
    use std::os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd};
    use std::os::unix::ffi::{OsStrExt, OsStringExt};
    use std::path::{Component, Path, PathBuf};
    use std::sync::atomic::{AtomicU64, Ordering};

    use rand::RngCore;

    use super::posix_trust::{
        judge_ancestor, judge_app_dir, judge_key_file, judge_store_dir, validate_clean_absolute,
        StatInfo, MAX_KEY_BYTES,
    };
    use super::{wipe_bytes, SecretText};

    pub(super) const STORE_DIR: &str = "identity-store";
    pub(super) const KEY_FILE: &str = "identity.key";
    const MAX_SYMLINK_HOPS: usize = 8;

    static TMP_SEQ: AtomicU64 = AtomicU64::new(0);

    fn euid() -> u32 {
        // SAFETY: `geteuid` has no preconditions and never fails.
        unsafe { libc::geteuid() }
    }

    fn to_cstring(s: &OsStr) -> Result<CString, String> {
        CString::new(s.as_bytes()).map_err(|_| "path component contains NUL byte".into())
    }

    fn stat_to_info(st: &libc::stat) -> StatInfo {
        let mode = st.st_mode as u32;
        let kind = mode & (libc::S_IFMT as u32);
        StatInfo {
            is_dir: kind == (libc::S_IFDIR as u32),
            is_reg: kind == (libc::S_IFREG as u32),
            is_symlink: kind == (libc::S_IFLNK as u32),
            uid: st.st_uid,
            mode,
            nlink: st.st_nlink as u64,
            size: st.st_size as i64,
        }
    }

    fn fstat_fd(fd: RawFd) -> Result<StatInfo, String> {
        let mut st: libc::stat = unsafe { std::mem::zeroed() };
        let rc = unsafe { libc::fstat(fd, &mut st) };
        if rc != 0 {
            return Err("fstat failed".into());
        }
        Ok(stat_to_info(&st))
    }

    fn fstatat_nofollow(dir_fd: RawFd, name: &CString) -> Result<Option<StatInfo>, String> {
        let mut st: libc::stat = unsafe { std::mem::zeroed() };
        let rc = unsafe { libc::fstatat(dir_fd, name.as_ptr(), &mut st, libc::AT_SYMLINK_NOFOLLOW) };
        if rc != 0 {
            let err = std::io::Error::last_os_error();
            if err.raw_os_error() == Some(libc::ENOENT) {
                return Ok(None);
            }
            return Err("fstatat failed".into());
        }
        Ok(Some(stat_to_info(&st)))
    }

    fn open_root() -> Result<(OwnedFd, StatInfo), String> {
        let root = CString::new("/").unwrap();
        let fd = unsafe {
            libc::open(
                root.as_ptr(),
                libc::O_RDONLY | libc::O_DIRECTORY | libc::O_CLOEXEC,
            )
        };
        if fd < 0 {
            return Err("could not open filesystem root".into());
        }
        // SAFETY: `fd` is a freshly opened valid descriptor.
        let owned = unsafe { OwnedFd::from_raw_fd(fd) };
        let st = fstat_fd(owned.as_raw_fd())?;
        Ok((owned, st))
    }

    fn openat_dir_nofollow(dir_fd: RawFd, name: &CString) -> Result<OwnedFd, i32> {
        let fd = unsafe {
            libc::openat(
                dir_fd,
                name.as_ptr(),
                libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            )
        };
        if fd < 0 {
            return Err(std::io::Error::last_os_error().raw_os_error().unwrap_or(libc::EIO));
        }
        // SAFETY: `fd` is a freshly opened valid descriptor.
        Ok(unsafe { OwnedFd::from_raw_fd(fd) })
    }

    fn readlinkat_path(dir_fd: RawFd, name: &CString) -> Result<PathBuf, String> {
        let mut buf = vec![0u8; 4096];
        let n = unsafe {
            libc::readlinkat(
                dir_fd,
                name.as_ptr(),
                buf.as_mut_ptr().cast::<libc::c_char>(),
                buf.len(),
            )
        };
        if n <= 0 || (n as usize) >= buf.len() {
            return Err("could not read ancestor symbolic link".into());
        }
        buf.truncate(n as usize);
        Ok(PathBuf::from(OsString::from_vec(buf)))
    }

    fn push_components_front(queue: &mut VecDeque<OsString>, target: &Path) -> Result<(), String> {
        let mut parts = Vec::new();
        for c in target.components() {
            match c {
                Component::RootDir => {}
                Component::Normal(p) => parts.push(p.to_os_string()),
                Component::ParentDir | Component::CurDir | Component::Prefix(_) => {
                    return Err("ancestor symbolic link contains disallowed components".into());
                }
            }
        }
        if parts.is_empty() {
            return Err("ancestor symbolic link target is empty".into());
        }
        for p in parts.into_iter().rev() {
            queue.push_front(p);
        }
        Ok(())
    }

    fn path_to_queue(path: &Path) -> Result<VecDeque<OsString>, String> {
        validate_clean_absolute(path).map_err(str::to_string)?;
        let mut q = VecDeque::new();
        for c in path.components() {
            if let Component::Normal(p) = c {
                q.push_back(p.to_os_string());
            }
        }
        Ok(q)
    }

    /// Walk `path` component-by-component from `/` using `openat(..., O_DIRECTORY | O_NOFOLLOW | O_CLOEXEC)`
    /// and verify every opened descriptor with `fstat`.
    ///
    /// - `require_final_owned`: if `true`, the final component must satisfy `judge_app_dir(euid, st)`;
    ///   otherwise all existing components are checked and the deepest existing directory is returned.
    /// - `create_missing`: if `true`, creates missing leaf directories with mode `0o700` when the
    ///   parent is safe.
    fn open_dir_trusted(
        uid: u32,
        path: &Path,
        create_missing: bool,
        stop_on_enoent_check_leaf: bool,
    ) -> Result<Option<(OwnedFd, StatInfo)>, String> {
        let mut queue = path_to_queue(path)?;
        let (mut cur_fd, mut cur_st) = open_root()?;
        judge_ancestor(uid, cur_st).map_err(str::to_string)?;
        let mut hops = 0usize;

        while let Some(comp) = queue.pop_front() {
            let is_final = queue.is_empty();
            let c_name = to_cstring(&comp)?;
            match openat_dir_nofollow(cur_fd.as_raw_fd(), &c_name) {
                Ok(next_fd) => {
                    let next_st = fstat_fd(next_fd.as_raw_fd())?;
                    if is_final {
                        judge_app_dir(uid, next_st).map_err(str::to_string)?;
                    } else {
                        judge_ancestor(uid, next_st).map_err(str::to_string)?;
                    }
                    cur_fd = next_fd;
                    cur_st = next_st;
                }
                Err(libc::ENOENT) if !create_missing => {
                    if stop_on_enoent_check_leaf {
                        judge_app_dir(uid, cur_st).map_err(str::to_string)?;
                    }
                    return Ok(None);
                }
                Err(libc::ENOENT) => {
                    let parent_safe = (cur_st.uid == uid && (cur_st.mode & 0o022) == 0)
                        || (is_final && cur_st.uid == 0 && (cur_st.mode & 0o1000) != 0);
                    if !parent_safe {
                        return Err("cannot create directory inside untrusted parent".into());
                    }
                    let rc = unsafe { libc::mkdirat(cur_fd.as_raw_fd(), c_name.as_ptr(), 0o700 as libc::mode_t) };
                    if rc != 0 {
                        let errno = std::io::Error::last_os_error().raw_os_error().unwrap_or(libc::EIO);
                        if errno != libc::EEXIST {
                            return Err("could not create private directory".into());
                        }
                    }
                    let next_fd = openat_dir_nofollow(cur_fd.as_raw_fd(), &c_name)
                        .map_err(|_| "could not open newly created directory safely".to_string())?;
                    let next_st = fstat_fd(next_fd.as_raw_fd())?;
                    judge_app_dir(uid, next_st).map_err(str::to_string)?;
                    let _ = unsafe { libc::fsync(cur_fd.as_raw_fd()) };
                    cur_fd = next_fd;
                    cur_st = next_st;
                }
                Err(libc::ELOOP) => {
                    if is_final {
                        return Err("data directory cannot be a symbolic link".into());
                    }
                    // Allow only a root-owned symlink inside a root-owned non-writable
                    // system directory (e.g. `/home -> var/home` on atomic Linux distros).
                    let link_st = fstatat_nofollow(cur_fd.as_raw_fd(), &c_name)?
                        .ok_or_else(|| "ancestor vanished during open".to_string())?;
                    let trusted_root_link = link_st.is_symlink
                        && link_st.uid == 0
                        && cur_st.uid == 0
                        && (cur_st.mode & 0o022) == 0;
                    if !trusted_root_link {
                        return Err("untrusted symbolic link in path ancestor".into());
                    }
                    hops += 1;
                    if hops > MAX_SYMLINK_HOPS {
                        return Err("too many symbolic links in path".into());
                    }
                    let target = readlinkat_path(cur_fd.as_raw_fd(), &c_name)?;
                    if target.is_absolute() {
                        let (rfd, rst) = open_root()?;
                        judge_ancestor(uid, rst).map_err(str::to_string)?;
                        cur_fd = rfd;
                        cur_st = rst;
                    }
                    push_components_front(&mut queue, &target)?;
                }
                Err(_) => return Err("could not open directory component safely".into()),
            }
        }
        Ok(Some((cur_fd, cur_st)))
    }

    pub(super) fn check_env_roots_with(
        uid: u32,
        home_env: Option<&OsStr>,
        xdg_data_env: Option<&OsStr>,
        account_home: Option<&Path>,
    ) -> Result<(), String> {
        if let Some(home) = home_env.filter(|v| !v.is_empty()) {
            if let Some(pw_home) = account_home {
                if !crate::linux_scheme::same_folder(Path::new(home), pw_home) {
                    return Err("HOME does not match the effective user's account home".into());
                }
            } else {
                let _ = open_dir_trusted(uid, Path::new(home), false, true)?;
            }
        }
        if let Some(xdg) = xdg_data_env.filter(|v| !v.is_empty()) {
            let _ = open_dir_trusted(uid, Path::new(xdg), false, true)?;
        }
        Ok(())
    }

    fn open_store_dir(dir_fd: RawFd, uid: u32, create: bool) -> Result<Option<OwnedFd>, String> {
        let name = CString::new(STORE_DIR).unwrap();
        let mut created = false;
        if create {
            let rc = unsafe { libc::mkdirat(dir_fd, name.as_ptr(), 0o700 as libc::mode_t) };
            if rc == 0 {
                created = true;
            } else {
                let errno = std::io::Error::last_os_error().raw_os_error().unwrap_or(libc::EIO);
                if errno != libc::EEXIST {
                    return Err("could not create key store directory".into());
                }
            }
        }
        let store_fd = match openat_dir_nofollow(dir_fd, &name) {
            Ok(fd) => fd,
            Err(libc::ENOENT) if !create => return Ok(None),
            Err(libc::ELOOP) => return Err("key store directory cannot be a symbolic link".into()),
            Err(_) => return Err("could not open key store directory safely".into()),
        };
        let st = fstat_fd(store_fd.as_raw_fd())?;
        judge_store_dir(uid, st).map_err(str::to_string)?;
        if created {
            let _ = unsafe { libc::fsync(dir_fd) };
        }
        Ok(Some(store_fd))
    }

    struct TempGuard {
        dir_fd: RawFd,
        name: CString,
        committed: bool,
    }

    impl TempGuard {
        fn commit(&mut self) {
            self.committed = true;
        }
    }

    impl Drop for TempGuard {
        fn drop(&mut self) {
            if !self.committed {
                unsafe {
                    libc::unlinkat(self.dir_fd, self.name.as_ptr(), 0);
                }
            }
        }
    }

    pub(super) fn load_with_env(
        dir: &Path,
        home_env: Option<&OsStr>,
        xdg_data_env: Option<&OsStr>,
        account_home: Option<&Path>,
    ) -> Result<Option<SecretText>, String> {
        let uid = euid();
        check_env_roots_with(uid, home_env, xdg_data_env, account_home)?;
        let Some((dir_fd, _)) = open_dir_trusted(uid, dir, false, false)? else {
            return Ok(None);
        };
        let Some(store_fd) = open_store_dir(dir_fd.as_raw_fd(), uid, false)? else {
            return Ok(None);
        };
        let key_name = CString::new(KEY_FILE).unwrap();
        const MAX_REPLACE_RETRIES: usize = 32;
        let key_fd = {
            let mut opened = None;
            for _ in 0..MAX_REPLACE_RETRIES {
                let fd = unsafe {
                    libc::openat(
                        store_fd.as_raw_fd(),
                        key_name.as_ptr(),
                        libc::O_RDONLY | libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC,
                    )
                };
                if fd < 0 {
                    let errno = std::io::Error::last_os_error().raw_os_error().unwrap_or(libc::EIO);
                    if errno == libc::ENOENT {
                        return Ok(None);
                    }
                    if errno == libc::ELOOP {
                        return Err("key file cannot be a symbolic link".into());
                    }
                    return Err("could not open key file".into());
                }
                // SAFETY: `fd` is a freshly opened valid descriptor.
                let cand = unsafe { OwnedFd::from_raw_fd(fd) };
                let st = fstat_fd(cand.as_raw_fd())?;
                if st.is_reg && st.nlink == 0 {
                    // Replaced by a concurrent atomic rename or unlinked between openat and fstat.
                    continue;
                }
                judge_key_file(uid, st).map_err(str::to_string)?;
                opened = Some(cand);
                break;
            }
            opened.ok_or_else(|| "key file changed concurrently".to_string())?
        };

        let mut file = std::fs::File::from(key_fd);
        let mut buf = Vec::new();
        let read_res = (&mut file).take((MAX_KEY_BYTES as u64) + 1).read_to_end(&mut buf);
        if read_res.is_err() {
            wipe_bytes(&mut buf);
            return Err("could not read key file".into());
        }
        if (buf.len() as i64) > MAX_KEY_BYTES {
            wipe_bytes(&mut buf);
            return Err("key file size is out of bounds".into());
        }
        match String::from_utf8(buf) {
            Ok(s) => Ok(Some(SecretText::new(s))),
            Err(e) => {
                let mut raw = e.into_bytes();
                wipe_bytes(&mut raw);
                Err("key file is not valid UTF-8".into())
            }
        }
    }

    pub(super) fn save_with_env(
        dir: &Path,
        text: &str,
        home_env: Option<&OsStr>,
        xdg_data_env: Option<&OsStr>,
        account_home: Option<&Path>,
    ) -> Result<(), String> {
        if text.is_empty() || (text.len() as i64) > MAX_KEY_BYTES {
            return Err("key payload size is out of bounds".into());
        }
        let uid = euid();
        check_env_roots_with(uid, home_env, xdg_data_env, account_home)?;
        let (dir_fd, _) = open_dir_trusted(uid, dir, true, false)?
            .ok_or_else(|| "could not prepare data directory".to_string())?;
        let store_fd = open_store_dir(dir_fd.as_raw_fd(), uid, true)?
            .ok_or_else(|| "could not prepare key store directory".to_string())?;

        let key_name = CString::new(KEY_FILE).unwrap();
        const MAX_REPLACE_RETRIES: usize = 32;
        for attempt in 0..MAX_REPLACE_RETRIES {
            match fstatat_nofollow(store_fd.as_raw_fd(), &key_name)? {
                None => break,
                Some(existing) if existing.is_reg && existing.nlink == 0 && attempt + 1 < MAX_REPLACE_RETRIES => {
                    continue;
                }
                Some(existing) => {
                    judge_key_file(uid, existing).map_err(str::to_string)?;
                    break;
                }
            }
        }

        let seq = TMP_SEQ.fetch_add(1, Ordering::Relaxed);
        let rand_val = rand::rngs::OsRng.next_u64();
        let tmp_name = CString::new(format!(
            ".{KEY_FILE}.{}.{seq}.{rand_val:016x}.tmp",
            std::process::id()
        ))
        .unwrap();

        let tmp_raw = unsafe {
            libc::openat(
                store_fd.as_raw_fd(),
                tmp_name.as_ptr(),
                libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL | libc::O_NOFOLLOW | libc::O_CLOEXEC,
                0o600 as libc::c_uint,
            )
        };
        if tmp_raw < 0 {
            return Err("could not create temporary key file".into());
        }
        // SAFETY: `tmp_raw` is a freshly created valid descriptor.
        let tmp_fd = unsafe { OwnedFd::from_raw_fd(tmp_raw) };
        let mut guard = TempGuard {
            dir_fd: store_fd.as_raw_fd(),
            name: tmp_name.clone(),
            committed: false,
        };

        let tmp_st = fstat_fd(tmp_fd.as_raw_fd())?;
        if !tmp_st.is_reg || tmp_st.uid != uid || (tmp_st.mode & 0o7777) != 0o600 || tmp_st.nlink != 1 {
            return Err("temporary key file did not get exclusive 0600 permissions".into());
        }

        {
            let mut file = std::fs::File::from(tmp_fd);
            file.write_all(text.as_bytes())
                .map_err(|_| "could not write temporary key file".to_string())?;
            file.sync_all()
                .map_err(|_| "could not sync temporary key file".to_string())?;
        }

        let rc = unsafe {
            libc::renameat(
                store_fd.as_raw_fd(),
                tmp_name.as_ptr(),
                store_fd.as_raw_fd(),
                key_name.as_ptr(),
            )
        };
        if rc != 0 {
            return Err("could not atomically install key file".into());
        }
        guard.commit();
        let _ = unsafe { libc::fsync(store_fd.as_raw_fd()) };
        Ok(())
    }

    pub(super) fn forget_with_env(
        dir: &Path,
        home_env: Option<&OsStr>,
        xdg_data_env: Option<&OsStr>,
        account_home: Option<&Path>,
    ) -> Result<bool, String> {
        let uid = euid();
        check_env_roots_with(uid, home_env, xdg_data_env, account_home)?;
        let Some((dir_fd, _)) = open_dir_trusted(uid, dir, false, false)? else {
            return Ok(false);
        };
        let Some(store_fd) = open_store_dir(dir_fd.as_raw_fd(), uid, false)? else {
            return Ok(false);
        };
        let key_name = CString::new(KEY_FILE).unwrap();
        let Some(st) = fstatat_nofollow(store_fd.as_raw_fd(), &key_name)? else {
            return Ok(false);
        };
        if !st.is_reg || st.is_symlink || st.uid != uid {
            return Err("refusing to remove untrusted key path".into());
        }
        let rc = unsafe { libc::unlinkat(store_fd.as_raw_fd(), key_name.as_ptr(), 0) };
        if rc != 0 {
            let errno = std::io::Error::last_os_error().raw_os_error().unwrap_or(libc::EIO);
            if errno == libc::ENOENT {
                return Ok(false);
            }
            return Err("could not remove key file".into());
        }
        let _ = unsafe { libc::fsync(store_fd.as_raw_fd()) };
        Ok(true)
    }

    pub(super) fn load(dir: &Path) -> Result<Option<SecretText>, String> {
        let home = std::env::var_os("HOME");
        let xdg = std::env::var_os("XDG_DATA_HOME");
        let pw = crate::linux_scheme::account_home();
        load_with_env(dir, home.as_deref(), xdg.as_deref(), pw.as_deref())
    }

    pub(super) fn save(dir: &Path, text: &str) -> Result<(), String> {
        let home = std::env::var_os("HOME");
        let xdg = std::env::var_os("XDG_DATA_HOME");
        let pw = crate::linux_scheme::account_home();
        save_with_env(dir, text, home.as_deref(), xdg.as_deref(), pw.as_deref())
    }

    pub(super) fn forget(dir: &Path) -> Result<bool, String> {
        let home = std::env::var_os("HOME");
        let xdg = std::env::var_os("XDG_DATA_HOME");
        let pw = crate::linux_scheme::account_home();
        forget_with_env(dir, home.as_deref(), xdg.as_deref(), pw.as_deref())
    }

    /// Lightweight presence hint for the key log only (never opens or reads the file).
    pub(super) fn exists_hint(dir: &Path) -> bool {
        std::fs::symlink_metadata(dir.join(STORE_DIR).join(KEY_FILE)).is_ok()
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

    use super::{wipe_bytes, SecretText};

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
        let mut out = CRYPT_INTEGER_BLOB {
            cbData: 0,
            pbData: std::ptr::null_mut(),
        };
        // SAFETY: the blobs point at live buffers; DPAPI never prompts here.
        let ok = unsafe {
            CryptProtectData(
                &input,
                std::ptr::null(),
                &entropy,
                std::ptr::null(),
                std::ptr::null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut out,
            )
        };
        if ok == 0 {
            return Err(format!("DPAPI didn't seal it: {}", std::io::Error::last_os_error()));
        }
        Ok(take(out, false))
    }

    pub fn open(sealed: &[u8]) -> Result<SecretText, String> {
        let (input, entropy) = (blob(sealed), blob(ENTROPY));
        let mut out = CRYPT_INTEGER_BLOB {
            cbData: 0,
            pbData: std::ptr::null_mut(),
        };
        // SAFETY: as in seal.
        let ok = unsafe {
            CryptUnprotectData(
                &input,
                std::ptr::null_mut(),
                &entropy,
                std::ptr::null(),
                std::ptr::null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut out,
            )
        };
        if ok == 0 {
            return Err(format!("DPAPI didn't open it: {}", std::io::Error::last_os_error()));
        }
        match String::from_utf8(take(out, true)) {
            Ok(s) => Ok(SecretText::new(s)),
            Err(e) => {
                let mut raw = e.into_bytes();
                wipe_bytes(&mut raw);
                Err("the backup isn't a key".to_string())
            }
        }
    }

    pub fn save(dir: &Path, text: &str) -> Result<(), String> {
        let sealed = seal(text)?;
        let tmp = dir.join(format!("{FILE}.tmp"));
        std::fs::write(&tmp, sealed).map_err(|e| format!("couldn't write it: {e}"))?;
        std::fs::rename(&tmp, path(dir)).map_err(|e| format!("couldn't put it in place: {e}"))
    }

    /// None: there is no backup.
    pub fn load(dir: &Path) -> Result<Option<SecretText>, String> {
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

/// Elsewhere (macOS, Android) there is no backup copy: the keyring or sealed
/// file stands alone (recovery words and linking bring a lost key back).
#[cfg(not(any(windows, target_os = "linux")))]
mod backup {
    use std::path::Path;

    use super::SecretText;

    pub fn save(_dir: &Path, _text: &str) -> Result<(), String> {
        Ok(())
    }
    pub fn load(_dir: &Path) -> Result<Option<SecretText>, String> {
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
        assert!(matches!(classify(Ok(encode(&me).as_str().to_string())), Main::Found(f) if f.address() == me.address()));
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
        assert!(backup::load(&dir).unwrap().is_none(), "no backup yet");
        backup::save(&dir, encode(&me).as_str()).unwrap();
        let file = std::fs::read(dir.join("identity-backup.dpapi")).unwrap();
        assert!(!String::from_utf8_lossy(&file).contains(me.secret()), "the file holds no plain key");
        let loaded = backup::load(&dir).unwrap().unwrap();
        let back = decode(loaded.as_str()).unwrap();
        assert_eq!((back.id(), back.secret()), (me.id(), me.secret()));
        let mut bent = file.clone();
        let mid = bent.len() / 2;
        bent[mid] ^= 0xff;
        assert!(backup::open(&bent).is_err(), "a changed file doesn't open");
        backup::forget(&dir).unwrap();
        assert!(backup::load(&dir).unwrap().is_none(), "gone after forget");
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
        backup::save(&dir, encode(&other).as_str()).unwrap();
        assert!(keep_backup(&dir, &me).unwrap().0.starts_with("the backup held another key; a new one"));
        assert_eq!(backup::load(&dir).unwrap().unwrap().as_str(), encode(&me).as_str());
        std::fs::write(dir.join("identity-backup.dpapi"), b"not sealed").unwrap();
        assert!(keep_backup(&dir, &me).unwrap().0.starts_with("the backup didn't open"));
        assert_eq!(backup::load(&dir).unwrap().unwrap().as_str(), encode(&me).as_str());
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
        assert!(validate_clean_absolute(Path::new("/")).is_err());

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

        // Key store subdir: strictly euid-owned 0700 dir (no group/other or special bits).
        assert!(judge_store_dir(euid, dir_ok(euid, 0o700)).is_ok());
        assert!(judge_store_dir(euid, dir_ok(euid, 0o2700)).is_err());
        assert!(judge_store_dir(euid, dir_ok(euid, 0o755)).is_err());
        assert!(judge_store_dir(euid, dir_ok(euid, 0o750)).is_err());
        assert!(judge_store_dir(euid, dir_ok(euid, 0o770)).is_err());
        assert!(judge_store_dir(euid, dir_ok(0, 0o700)).is_err());
        assert!(judge_store_dir(euid, file_ok(euid, 0o700, 1, 0)).is_err());

        // Key file: regular, euid-owned, no group/other/exec/special bits, nlink==1, bounded size.
        assert!(judge_key_file(euid, file_ok(euid, 0o600, 1, 128)).is_ok());
        assert!(judge_key_file(euid, file_ok(euid, 0o400, 1, 128)).is_ok());
        assert!(judge_key_file(euid, file_ok(euid, 0o700, 1, 128)).is_err());
        assert!(judge_key_file(euid, file_ok(euid, 0o4600, 1, 128)).is_err());
        assert!(judge_key_file(euid, file_ok(euid, 0o640, 1, 128)).is_err());
        assert!(judge_key_file(euid, file_ok(euid, 0o644, 1, 128)).is_err());
        assert!(judge_key_file(euid, file_ok(euid, 0o600, 0, 128)).is_err());
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
            *self.keyring.borrow_mut() = Ok(Some(encode(me).as_str().to_string()));
            Ok(())
        }
        fn keyring_forget(&self) -> Result<(), String> {
            if let Some(e) = self.keyring_forget_err.borrow().as_ref() {
                return Err(e.clone());
            }
            *self.keyring.borrow_mut() = Ok(None);
            Ok(())
        }
        fn file_load(&self) -> Result<Option<SecretText>, String> {
            self.file_reads.set(self.file_reads.get() + 1);
            match self.file.borrow().as_ref() {
                Ok(Some(s)) => Ok(Some(SecretText::new(s.clone()))),
                Ok(None) => Ok(None),
                Err(e) => Err(e.clone()),
            }
        }
        fn file_save(&self, encoded: &str) -> Result<(), String> {
            self.file_writes.set(self.file_writes.get() + 1);
            if let Some(e) = self.file_save_err.borrow().as_ref() {
                return Err(e.clone());
            }
            *self.file.borrow_mut() = Ok(Some(encoded.to_string()));
            Ok(())
        }
        fn file_forget(&self) -> Result<bool, String> {
            if let Some(e) = self.file_forget_err.borrow().as_ref() {
                return Err(e.clone());
            }
            let had = matches!(self.file.borrow().as_ref(), Ok(Some(_)));
            *self.file.borrow_mut() = Ok(None);
            Ok(had)
        }
        fn file_exists_hint(&self) -> bool {
            matches!(self.file.borrow().as_ref(), Ok(Some(_)))
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
            *b.file.borrow_mut() = Ok(Some(encode(&stale).as_str().to_string()));
            let out = load_with(&b);
            assert!(matches!(out.loaded, PolicyLoaded::Missing), "state={state:?}");
            assert!(out.facts.contains(&Fact::KeyFileIgnoredWithSecretService));
            assert_eq!(b.file_reads.get(), 0, "must not read file when state={state:?}");

            // 2. Keyring locked / denied / timeout / bus error + older fallback file -> Unreadable
            *b.keyring.borrow_mut() = Err("collection locked".into());
            assert!(matches!(load_with(&b).loaded, PolicyLoaded::Unreadable), "state={state:?}");
            assert_eq!(b.file_reads.get(), 0);

            // 3. Keyring damaged credential + older fallback file -> Missing
            *b.keyring.borrow_mut() = Ok(Some("not-a-key".into()));
            assert!(matches!(load_with(&b).loaded, PolicyLoaded::Missing), "state={state:?}");
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

        // File save error surfaces cleanly.
        *b.file_save_err.borrow_mut() = Some("disk full".into());
        assert!(save_with(&b, &alice).is_err());
        *b.file_save_err.borrow_mut() = None;

        // Save under Absent -> file storage, never touches keyring.
        let saved = save_with(&b, &alice).unwrap();
        assert_eq!(saved.storage, KeyStorage::FilePermissionsOnly);
        assert!(saved.facts.contains(&Fact::KeySavedInFile));
        let save_line = saved.log_line.as_deref().unwrap_or("");
        assert!(save_line.contains("permissions-only"));
        assert!(!save_line.contains(alice.secret()));
        assert!(!save_line.contains(&alice.address()));
        assert_eq!(b.keyring_writes.get(), 0);
        assert_eq!(b.file_writes.get(), 2);

        // Load under Absent -> Found from FilePermissionsOnly.
        let loaded = load_with(&b);
        assert!(!loaded.log_line.contains(alice.secret()));
        assert!(!loaded.log_line.contains(&alice.address()));
        match loaded.loaded {
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
        assert!(matches!(load_with(&b).loaded, PolicyLoaded::Missing));
        assert_eq!(b.file_reads.get(), reads_before);
        assert!(b.file.borrow().as_ref().unwrap().is_some(), "file preserved until user acts");

        // If stale fallback file cannot be removed during keyring save -> surface error!
        *b.file_forget_err.borrow_mut() = Some("untrusted key path".into());
        assert!(save_with(&b, &alice).is_err());
        *b.file_forget_err.borrow_mut() = None;

        // User restores Alice into Secret Service -> saved in keyring AND stale file removed!
        assert_eq!(save_with(&b, &alice).unwrap().storage, KeyStorage::Keyring);
        assert_eq!(*b.file.borrow(), Ok(None), "stale fallback file must be removed on keyring save");

        // Keyring load also sweeps any lingering stale file when keyring holds the active key.
        *b.file.borrow_mut() = Ok(Some(encode(&alice).as_str().to_string()));
        let loaded = load_with(&b);
        assert!(matches!(loaded.loaded, PolicyLoaded::Found { storage: KeyStorage::Keyring, .. }));
        assert!(loaded.facts.contains(&Fact::KeyFileRemoved));
        assert_eq!(*b.file.borrow(), Ok(None));

        // If Secret Service later disappears again -> stale file is gone, returns Missing.
        b.probe.set(SecretServiceState::Absent);
        assert!(matches!(load_with(&b).loaded, PolicyLoaded::Missing));

        // Save under Absent, then forget under Absent -> succeeds without keyring and removes file.
        assert_eq!(save_with(&b, &alice).unwrap().storage, KeyStorage::FilePermissionsOnly);
        forget_with(&b).unwrap();
        assert_eq!(*b.file.borrow(), Ok(None));
        assert!(matches!(load_with(&b).loaded, PolicyLoaded::Missing));

        // Forget under Present removes BOTH keyring and any leftover fallback file.
        b.probe.set(SecretServiceState::Present);
        *b.keyring.borrow_mut() = Ok(Some(encode(&alice).as_str().to_string()));
        *b.file.borrow_mut() = Ok(Some(encode(&alice).as_str().to_string()));
        forget_with(&b).unwrap();
        assert_eq!(*b.keyring.borrow(), Ok(None));
        assert_eq!(*b.file.borrow(), Ok(None));

        // Corrupt or untrusted file under Absent -> Missing / Unreadable.
        b.probe.set(SecretServiceState::Absent);
        *b.file.borrow_mut() = Ok(Some("corrupt".into()));
        assert!(matches!(load_with(&b).loaded, PolicyLoaded::Missing));
        *b.file.borrow_mut() = Err("unsafe permissions".into());
        assert!(matches!(load_with(&b).loaded, PolicyLoaded::Unreadable));
    }

    // Round 1 review RED tests: H1 (missing bus hints must still try session discovery),
    // L15 (indeterminate probe must not claim Secret Service is available).

    #[test]
    fn h1_missing_bus_hints_still_invokes_session_discovery() {
        // Without DBUS_SESSION_BUS_ADDRESS or $XDG_RUNTIME_DIR/bus, X11 session discovery
        // can still locate a usable Secret Service bus and MUST NOT short-circuit to Absent.
        let mut probed = false;
        let state = classify_dbus_probe(BusEnv::Missing { runtime_bus_exists: Some(false) }, || {
            probed = true;
            Ok((true, None))
        });
        assert!(probed, "discovery probe must actually run when address hints are missing");
        assert_eq!(state, SecretServiceState::Present);
    }

    #[test]
    fn l15_indeterminate_probe_log_reports_indeterminate_not_available() {
        use linux_policy::*;
        let stale = Identity::generate("stale").unwrap();
        let b = FakeBackend::new(SecretServiceState::Indeterminate);
        *b.file.borrow_mut() = Ok(Some(encode(&stale).as_str().to_string()));

        // Keyring Missing under Indeterminate probe: must not claim Secret Service "is available".
        let out_missing = load_with(&b);
        assert!(matches!(out_missing.loaded, PolicyLoaded::Missing));
        assert!(
            !out_missing.log_line.contains("Secret Service is available"),
            "log line mislabeled indeterminate probe as available: {}",
            out_missing.log_line
        );

        // Keyring Failed under Indeterminate probe: must not claim Secret Service "is available".
        *b.keyring.borrow_mut() = Err("bus timed out".into());
        let out_failed = load_with(&b);
        assert!(matches!(out_failed.loaded, PolicyLoaded::Unreadable));
        assert!(
            !out_failed.log_line.contains("Secret Service is available"),
            "log line mislabeled indeterminate probe as available: {}",
            out_failed.log_line
        );
    }
}

#[cfg(all(test, target_os = "linux"))]
mod linux_file_tests {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;
    use std::os::unix::fs::{symlink, PermissionsExt};
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::Arc;

    use super::*;

    static DIR_SEQ: AtomicU64 = AtomicU64::new(0);

    fn temp_dir(tag: &str) -> PathBuf {
        let n = DIR_SEQ.fetch_add(1, Ordering::Relaxed);
        let d = std::env::temp_dir().join(format!("hubchat-keyfile-{tag}-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    fn mode_of(path: &Path) -> u32 {
        std::fs::symlink_metadata(path).unwrap().permissions().mode() & 0o777
    }

    #[test]
    fn round_trip_enforces_0700_0600_leaves_no_tmp_and_keeps_secrets_out_of_keylog() {
        let dir = temp_dir("rt");
        let me = Identity::generate("deck").unwrap();
        let pw = crate::linux_scheme::account_home();
        let home = pw.as_deref().map(|p| p.as_os_str());

        assert!(linux_file::load_with_env(&dir, home, None, pw.as_deref()).unwrap().is_none());
        linux_file::save_with_env(&dir, encode(&me).as_str(), home, None, pw.as_deref()).unwrap();

        let store = dir.join(linux_file::STORE_DIR);
        let key = store.join(linux_file::KEY_FILE);
        assert_eq!(mode_of(&dir), 0o700, "newly created app dir is 0700");
        assert_eq!(mode_of(&store), 0o700, "identity-store is 0700");
        assert_eq!(mode_of(&key), 0o600, "identity.key is 0600");

        let entries: Vec<_> = std::fs::read_dir(&store).unwrap().collect();
        assert_eq!(entries.len(), 1, "no temporary files left behind");

        let loaded = linux_file::load_with_env(&dir, home, None, pw.as_deref()).unwrap().unwrap();
        let back = decode(loaded.as_str()).unwrap();
        assert_eq!((back.id(), back.secret()), (me.id(), me.secret()));

        keylog::note(
            &dir,
            "key saved in Hubchat's permissions-only key file (no Secret Service available)",
            &[Fact::NoSecretService, Fact::KeySavedInFile],
        );
        let log = std::fs::read_to_string(dir.join("identity-log.txt")).unwrap();
        assert!(!log.contains(me.secret()), "secret must never reach keylog");
        assert!(!log.contains(&me.address()), "address must never reach keylog");

        assert!(linux_file::forget_with_env(&dir, home, None, pw.as_deref()).unwrap());
        assert!(!key.exists());
        assert!(!linux_file::forget_with_env(&dir, home, None, pw.as_deref()).unwrap());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn refuses_symlinks_fifo_hard_links_loose_modes_and_foreign_env_roots() {
        let base = temp_dir("sec");
        std::fs::create_dir_all(&base).unwrap();
        std::fs::set_permissions(&base, std::fs::Permissions::from_mode(0o700)).unwrap();
        let me = Identity::generate("sec").unwrap();
        let pw = crate::linux_scheme::account_home();
        let home = pw.as_deref().map(|p| p.as_os_str());

        // 1. Foreign HOME refused.
        let fake_home = std::ffi::OsStr::new("/nonexistent-foreign-home-for-test");
        let dir1 = base.join("d1");
        assert!(linux_file::save_with_env(&dir1, encode(&me).as_str(), Some(fake_home), None, pw.as_deref()).is_err());

        // 2. Unsafe / root / relative / symlink XDG_DATA_HOME refused.
        assert!(linux_file::save_with_env(&dir1, encode(&me).as_str(), home, Some(std::ffi::OsStr::new("/tmp")), pw.as_deref()).is_err());
        assert!(linux_file::save_with_env(&dir1, encode(&me).as_str(), home, Some(std::ffi::OsStr::new("relative")), pw.as_deref()).is_err());
        let xdg_link = base.join("xdg-link");
        let xdg_real = base.join("xdg-real");
        std::fs::create_dir(&xdg_real).unwrap();
        symlink(&xdg_real, &xdg_link).unwrap();
        assert!(linux_file::save_with_env(&dir1, encode(&me).as_str(), home, Some(xdg_link.as_os_str()), pw.as_deref()).is_err());

        // 3. Symlink at data dir, store dir, and key file refused.
        let real_d = base.join("real-d");
        std::fs::create_dir(&real_d).unwrap();
        std::fs::set_permissions(&real_d, std::fs::Permissions::from_mode(0o700)).unwrap();
        let sym_d = base.join("sym-d");
        symlink(&real_d, &sym_d).unwrap();
        assert!(linux_file::save_with_env(&sym_d, encode(&me).as_str(), home, None, pw.as_deref()).is_err());
        assert!(linux_file::load_with_env(&sym_d, home, None, pw.as_deref()).is_err());
        assert!(linux_file::forget_with_env(&sym_d, home, None, pw.as_deref()).is_err());

        let dir_store_sym = base.join("store-sym");
        std::fs::create_dir(&dir_store_sym).unwrap();
        std::fs::set_permissions(&dir_store_sym, std::fs::Permissions::from_mode(0o700)).unwrap();
        symlink(&real_d, dir_store_sym.join(linux_file::STORE_DIR)).unwrap();
        assert!(linux_file::save_with_env(&dir_store_sym, encode(&me).as_str(), home, None, pw.as_deref()).is_err());
        assert!(linux_file::load_with_env(&dir_store_sym, home, None, pw.as_deref()).is_err());
        assert!(linux_file::forget_with_env(&dir_store_sym, home, None, pw.as_deref()).is_err());

        let dir_key_sym = base.join("key-sym");
        let decoy = base.join("decoy");
        std::fs::write(&decoy, encode(&me).as_str()).unwrap();
        std::fs::set_permissions(&decoy, std::fs::Permissions::from_mode(0o600)).unwrap();
        std::fs::create_dir_all(dir_key_sym.join(linux_file::STORE_DIR)).unwrap();
        std::fs::set_permissions(&dir_key_sym, std::fs::Permissions::from_mode(0o700)).unwrap();
        std::fs::set_permissions(dir_key_sym.join(linux_file::STORE_DIR), std::fs::Permissions::from_mode(0o700)).unwrap();
        symlink(&decoy, dir_key_sym.join(linux_file::STORE_DIR).join(linux_file::KEY_FILE)).unwrap();
        assert!(linux_file::load_with_env(&dir_key_sym, home, None, pw.as_deref()).is_err());
        assert!(linux_file::save_with_env(&dir_key_sym, encode(&me).as_str(), home, None, pw.as_deref()).is_err());
        assert!(linux_file::forget_with_env(&dir_key_sym, home, None, pw.as_deref()).is_err());

        // 4. FIFO at identity.key refused without hanging (O_NONBLOCK + fstat S_ISREG).
        let dir_fifo = base.join("fifo");
        let fifo_store = dir_fifo.join(linux_file::STORE_DIR);
        std::fs::create_dir_all(&fifo_store).unwrap();
        std::fs::set_permissions(&dir_fifo, std::fs::Permissions::from_mode(0o700)).unwrap();
        std::fs::set_permissions(&fifo_store, std::fs::Permissions::from_mode(0o700)).unwrap();
        let fifo_path = fifo_store.join(linux_file::KEY_FILE);
        let c_fifo = CString::new(fifo_path.as_os_str().as_bytes()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(c_fifo.as_ptr(), 0o600) }, 0);
        assert!(linux_file::load_with_env(&dir_fifo, home, None, pw.as_deref()).is_err());
        assert!(linux_file::save_with_env(&dir_fifo, encode(&me).as_str(), home, None, pw.as_deref()).is_err());
        assert!(linux_file::forget_with_env(&dir_fifo, home, None, pw.as_deref()).is_err());

        // 5. Hard link (nlink == 2) refused on load and save.
        let dir_hl = base.join("hardlink");
        linux_file::save_with_env(&dir_hl, encode(&me).as_str(), home, None, pw.as_deref()).unwrap();
        let key_hl = dir_hl.join(linux_file::STORE_DIR).join(linux_file::KEY_FILE);
        std::fs::hard_link(&key_hl, base.join("leaked-hardlink")).unwrap();
        assert!(linux_file::load_with_env(&dir_hl, home, None, pw.as_deref()).is_err());
        assert!(linux_file::save_with_env(&dir_hl, encode(&me).as_str(), home, None, pw.as_deref()).is_err());

        // 6. Loose permissions on key file (0640), store dir (0755), app dir (0775), and ancestor (0777).
        let dir_perm = base.join("perm");
        linux_file::save_with_env(&dir_perm, encode(&me).as_str(), home, None, pw.as_deref()).unwrap();
        let perm_store = dir_perm.join(linux_file::STORE_DIR);
        let perm_key = perm_store.join(linux_file::KEY_FILE);
        std::fs::set_permissions(&perm_key, std::fs::Permissions::from_mode(0o640)).unwrap();
        assert!(linux_file::load_with_env(&dir_perm, home, None, pw.as_deref()).is_err());
        std::fs::set_permissions(&perm_key, std::fs::Permissions::from_mode(0o600)).unwrap();
        std::fs::set_permissions(&perm_store, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert!(linux_file::load_with_env(&dir_perm, home, None, pw.as_deref()).is_err());
        std::fs::set_permissions(&perm_store, std::fs::Permissions::from_mode(0o700)).unwrap();
        std::fs::set_permissions(&dir_perm, std::fs::Permissions::from_mode(0o775)).unwrap();
        assert!(linux_file::load_with_env(&dir_perm, home, None, pw.as_deref()).is_err());
        std::fs::set_permissions(&dir_perm, std::fs::Permissions::from_mode(0o700)).unwrap();

        let bad_anc = base.join("world-anc");
        let under_bad = bad_anc.join("app");
        std::fs::create_dir_all(&under_bad).unwrap();
        std::fs::set_permissions(&bad_anc, std::fs::Permissions::from_mode(0o777)).unwrap();
        std::fs::set_permissions(&under_bad, std::fs::Permissions::from_mode(0o700)).unwrap();
        assert!(linux_file::save_with_env(&under_bad, encode(&me).as_str(), home, None, pw.as_deref()).is_err());

        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn concurrent_saves_and_reads_always_observe_a_complete_0600_identity_and_no_tmp_leftovers() {
        let dir = Arc::new(temp_dir("conc"));
        let pw = Arc::new(crate::linux_scheme::account_home());
        let ids: Arc<Vec<Identity>> = Arc::new(
            (0..4)
                .map(|i| Identity::generate(&format!("user{i}")).unwrap())
                .collect(),
        );
        let valid_addrs: Arc<Vec<String>> = Arc::new(ids.iter().map(|i| i.address()).collect());

        linux_file::save_with_env(
            &dir,
            encode(&ids[0]).as_str(),
            pw.as_deref().map(|p| p.as_os_str()),
            None,
            pw.as_deref(),
        )
        .unwrap();

        let mut handles = Vec::new();
        for w in 0..4 {
            let dir = dir.clone();
            let pw = pw.clone();
            let ids = ids.clone();
            handles.push(std::thread::spawn(move || {
                for _ in 0..20 {
                    let text = encode(&ids[w]);
                    linux_file::save_with_env(
                        &dir,
                        text.as_str(),
                        pw.as_deref().map(|p| p.as_os_str()),
                        None,
                        pw.as_deref(),
                    )
                    .unwrap();
                }
            }));
        }
        for _ in 0..2 {
            let dir = dir.clone();
            let pw = pw.clone();
            let valid_addrs = valid_addrs.clone();
            handles.push(std::thread::spawn(move || {
                for _ in 0..40 {
                    let loaded = linux_file::load_with_env(
                        &dir,
                        pw.as_deref().map(|p| p.as_os_str()),
                        None,
                        pw.as_deref(),
                    )
                    .unwrap()
                    .expect("identity must always exist during atomic replace");
                    let got = decode(loaded.as_str()).expect("loaded identity must never be partial");
                    assert!(valid_addrs.contains(&got.address()));
                }
            }));
        }
        for h in handles {
            h.join().unwrap();
        }

        let store = dir.join(linux_file::STORE_DIR);
        let entries: Vec<_> = std::fs::read_dir(&store).unwrap().collect();
        assert_eq!(entries.len(), 1, "no leftover temporary files after concurrent saves");
        assert_eq!(mode_of(&store), 0o700);
        assert_eq!(mode_of(&store.join(linux_file::KEY_FILE)), 0o600);
        let _ = std::fs::remove_dir_all(&*dir);
    }

    #[test]
    fn m5_m8_l16_system_symlink_temp_sweep_hardlink_forget_and_unresolved_account_home() {
        let base = temp_dir("m5m8l16");
        std::fs::create_dir_all(&base).unwrap();
        std::fs::set_permissions(&base, std::fs::Permissions::from_mode(0o700)).unwrap();
        let me = Identity::generate("deck").unwrap();
        let pw = crate::linux_scheme::account_home();
        let home = pw.as_deref().map(|p| p.as_os_str());

        // M8: interrupted temporary plaintext file must be removed by forget_with_env.
        let dir_tmp = base.join("tmp-leftover");
        linux_file::save_with_env(&dir_tmp, encode(&me).as_str(), home, None, pw.as_deref()).unwrap();
        let orphan_tmp = dir_tmp
            .join(linux_file::STORE_DIR)
            .join(".identity.key.999999.0.0123456789abcdef.tmp");
        std::fs::write(&orphan_tmp, encode(&me).as_str()).unwrap();
        std::fs::set_permissions(&orphan_tmp, std::fs::Permissions::from_mode(0o600)).unwrap();
        assert!(linux_file::forget_with_env(&dir_tmp, home, None, pw.as_deref()).unwrap());
        assert!(!orphan_tmp.exists(), "forget must sweep interrupted temp plaintext files");

        // M8: multiply-linked key file (nlink > 1) must be refused by forget_with_env too.
        let dir_hl = base.join("forget-hl");
        linux_file::save_with_env(&dir_hl, encode(&me).as_str(), home, None, pw.as_deref()).unwrap();
        let key_hl = dir_hl.join(linux_file::STORE_DIR).join(linux_file::KEY_FILE);
        std::fs::hard_link(&key_hl, base.join("extra-link")).unwrap();
        assert!(
            linux_file::forget_with_env(&dir_hl, home, None, pw.as_deref()).is_err(),
            "forget must reject multiply-linked key file"
        );

        // L16: unresolved account_home (None) must fail closed even if HOME is euid-owned.
        let alt_home = base.join("alt-home");
        std::fs::create_dir_all(&alt_home).unwrap();
        std::fs::set_permissions(&alt_home, std::fs::Permissions::from_mode(0o700)).unwrap();
        let dir_l16 = base.join("l16");
        assert!(
            linux_file::save_with_env(&dir_l16, encode(&me).as_str(), Some(alt_home.as_os_str()), None, None).is_err(),
            "unresolved account_home must fail closed rather than trusting an arbitrary euid-owned HOME"
        );

        // M5: root-owned system symlink (e.g. /var/run -> /run or /lib -> usr/lib) must be
        // traversed cleanly even when openat(O_DIRECTORY|O_NOFOLLOW) returns ENOTDIR on Linux.
        let sys_link_child = [
            Path::new("/var/run"),
            Path::new("/bin"),
            Path::new("/lib"),
        ]
        .into_iter()
        .find_map(|p| {
            let meta = std::fs::symlink_metadata(p).ok()?;
            if !meta.file_type().is_symlink() {
                return None;
            }
            // Pick a child directory inside the symlinked system root so the symlink is an ancestor.
            let entries = std::fs::read_dir(p).ok()?;
            for entry in entries.flatten() {
                if entry.file_type().is_ok_and(|t| t.is_dir()) {
                    return Some(p.join(entry.file_name()));
                }
            }
            Some(p.join("nonexistent-child-for-ancestor-walk"))
        });
        if let Some(sys_path) = sys_link_child {
            // Walking through a root-owned system symlink to a missing leaf returns Ok(None), NOT Err(ENOTDIR).
            let probe_path = sys_path.join("hubchat-nonexistent-leaf-check");
            let res = linux_file::load_with_env(&probe_path, home, None, pw.as_deref());
            assert!(
                res.is_ok(),
                "root-owned system symlink ancestor {} must be accepted, got {res:?}",
                probe_path.display()
            );
        }

        let _ = std::fs::remove_dir_all(&base);
    }
}

/// Linux keyring / fallback round trip, run by CI (build-linux.yml) across
/// four real D-Bus setups:
/// - `file`: no D-Bus session, or a session D-Bus with no `org.freedesktop.secrets`
///   provider (owned or activatable), matching SteamOS VR Gamescope.
/// - `fail`: a session D-Bus with an activatable (locked) Secret Service —
///   must preserve keyring failure and never read or overwrite a pre-existing
///   fallback file.
/// - `ok`: unlocked GNOME Keyring — ignores any pre-existing stale fallback
///   file on load, removes it on save, and completes keyring save/load/forget.
#[cfg(all(test, target_os = "linux"))]
mod linux_keyring {
    use std::os::unix::fs::PermissionsExt;

    use super::*;

    #[test]
    #[ignore = "needs a chosen D-Bus/keyring setup; run by CI"]
    fn round_trip() {
        let expect = std::env::var("HUBCHAT_KEYRING_EXPECT").expect("HUBCHAT_KEYRING_EXPECT");
        let dir = std::env::temp_dir().join(format!("hubchat-keyring-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let me = Identity::generate("ci").unwrap();
        let stale = Identity::generate("stale").unwrap();
        let key_path = dir.join(linux_file::STORE_DIR).join(linux_file::KEY_FILE);

        match expect.as_str() {
            "file" => {
                let saved = save_identity(&dir, &me);
                eprintln!("save_identity(file): {saved:?}");
                assert_eq!(saved.unwrap(), KeyStorage::FilePermissionsOnly);
                let store_mode = std::fs::symlink_metadata(dir.join(linux_file::STORE_DIR))
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777;
                let file_mode = std::fs::symlink_metadata(&key_path)
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777;
                assert_eq!(store_mode, 0o700);
                assert_eq!(file_mode, 0o600);
                let Loaded::Found {
                    identity: back,
                    storage,
                } = load_identity(&dir)
                else {
                    panic!("load after file save")
                };
                assert_eq!(storage, KeyStorage::FilePermissionsOnly);
                assert_eq!(back.id(), me.id());
                assert_eq!(back.secret(), me.secret());
                forget_identity(&dir).unwrap();
                assert!(!key_path.exists());
                assert!(matches!(load_identity(&dir), Loaded::Missing), "gone after forget");
            }
            "fail" => {
                // Pre-seed a valid fallback file and prove an activatable/locked
                // Secret Service NEVER reads or overwrites it.
                linux_file::save(&dir, encode(&stale).as_str()).unwrap();
                assert!(
                    !matches!(load_identity(&dir), Loaded::Found { .. }),
                    "must not read fallback file while Secret Service is activatable"
                );
                let saved = save_identity(&dir, &me);
                eprintln!("save_identity(fail): {saved:?}");
                assert!(saved.is_err(), "saving must fail when Secret Service refuses");
                let still_stale = linux_file::load(&dir).unwrap().unwrap();
                assert_eq!(decode(still_stale.as_str()).unwrap().address(), stale.address());
            }
            "ok" => {
                // Pre-seed a stale fallback file and prove unlocked Secret Service
                // ignores it on load, removes it on save, and uses keyring exclusively.
                linux_file::save(&dir, encode(&stale).as_str()).unwrap();
                assert!(
                    matches!(load_identity(&dir), Loaded::Missing),
                    "must ignore stale fallback file while Secret Service is present"
                );
                let saved = save_identity(&dir, &me);
                eprintln!("save_identity(ok): {saved:?}");
                assert_eq!(saved.unwrap(), KeyStorage::Keyring);
                assert!(!key_path.exists(), "stale fallback file must be removed after keyring save");
                let Loaded::Found {
                    identity: back,
                    storage,
                } = load_identity(&dir)
                else {
                    panic!("load after keyring save")
                };
                assert_eq!(storage, KeyStorage::Keyring);
                assert_eq!(back.id(), me.id());
                assert_eq!(back.secret(), me.secret());
                forget_identity(&dir).unwrap();
                assert!(matches!(load_identity(&dir), Loaded::Missing), "gone after forget");
            }
            other => panic!("HUBCHAT_KEYRING_EXPECT={other}"),
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}
