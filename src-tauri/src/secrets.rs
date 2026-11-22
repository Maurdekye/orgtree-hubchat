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

#[cfg(all(desktop, not(target_os = "linux")))]
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

#[cfg(all(desktop, not(target_os = "linux")))]
fn save_main(dir: &Path, me: &Identity) -> Result<(), String> {
    entry(dir)?
        .set_password(&encode(me))
        .map_err(|e| e.to_string())
}

#[cfg(all(desktop, not(target_os = "linux")))]
fn forget_main(dir: &Path) -> Result<(), String> {
    match entry(dir)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(e.to_string()),
    }
}

#[cfg(target_os = "linux")]
fn load_main(dir: &Path) -> Main {
    linux::load_main(dir)
}

#[cfg(target_os = "linux")]
fn save_main(dir: &Path, me: &Identity) -> Result<(), String> {
    linux::save_main(dir, me)
}

#[cfg(target_os = "linux")]
fn forget_main(dir: &Path) -> Result<(), String> {
    linux::forget_main(dir)
}

#[cfg(target_os = "linux")]
pub fn linux_key_storage() -> &'static str {
    linux::key_storage()
}

#[cfg(target_os = "linux")]
pub(crate) mod linux {
    use std::path::Path;

    use super::{classify, entry, encode, Identity, Main};

    #[cfg(test)]
    use std::cell::RefCell;
    #[cfg(test)]
    use std::collections::HashMap;
    #[cfg(test)]
    use std::sync::{Arc, Mutex};

    #[allow(dead_code)]
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub(crate) enum SecretServiceState {
        Present,
        Absent,
        Indeterminate,
    }

    #[allow(dead_code)]
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub(crate) struct BusHints {
        pub address_set: bool,
        pub address_disabled: bool,
        pub runtime_bus_exists: Option<bool>,
        pub has_display: bool,
    }

    #[allow(dead_code)]
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub(crate) enum DbusProbeResult {
        Owned,
        NotOwned { activatable: Vec<String> },
        ConnectError { name: Option<String>, message: Option<String> },
        QueryError,
        TimedOut,
    }

    #[allow(dead_code)]
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

    #[allow(dead_code)]
    pub(crate) fn classify_dbus_probe(
        _hints: BusHints,
        _probe: impl FnOnce() -> DbusProbeResult,
    ) -> SecretServiceState {
        SecretServiceState::Present
    }

    #[allow(dead_code)]
    pub(crate) fn judge_store_dir(_euid: u32, _st: StatInfo) -> Result<(), &'static str> {
        Ok(())
    }

    #[allow(dead_code)]
    pub(crate) fn judge_key_file(_euid: u32, _st: StatInfo) -> Result<(), &'static str> {
        Ok(())
    }

    #[cfg(test)]
    #[derive(Clone)]
    pub(crate) struct MockKeyring {
        entries: Arc<Mutex<HashMap<String, String>>>,
        calls: Arc<std::sync::atomic::AtomicUsize>,
    }

    #[cfg(test)]
    impl MockKeyring {
        pub(crate) fn new() -> Self {
            Self {
                entries: Arc::new(Mutex::new(HashMap::new())),
                calls: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            }
        }
        pub(crate) fn calls(&self) -> usize {
            self.calls.load(std::sync::atomic::Ordering::SeqCst)
        }
        pub(crate) fn get(&self, dir: &Path) -> Main {
            self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            let k = dir.to_string_lossy().into_owned();
            match self.entries.lock().unwrap().get(&k).cloned() {
                Some(v) => classify(Ok(v)),
                None => classify(Err(keyring::Error::NoEntry)),
            }
        }
        pub(crate) fn set(&self, dir: &Path, me: &Identity) -> Result<(), String> {
            self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            self.entries
                .lock()
                .unwrap()
                .insert(dir.to_string_lossy().into_owned(), encode(me));
            Ok(())
        }
        pub(crate) fn delete(&self, dir: &Path) -> Result<(), String> {
            self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            self.entries
                .lock()
                .unwrap()
                .remove(&dir.to_string_lossy().into_owned());
            Ok(())
        }
    }

    #[cfg(test)]
    struct TestOverride {
        _state: SecretServiceState,
        keyring: MockKeyring,
    }

    #[cfg(test)]
    thread_local! {
        static TEST_OVERRIDE: RefCell<Option<TestOverride>> = const { RefCell::new(None) };
    }

    #[cfg(test)]
    pub(crate) fn with_test_backend<R>(
        state: SecretServiceState,
        keyring: MockKeyring,
        f: impl FnOnce() -> R,
    ) -> R {
        struct Reset;
        impl Drop for Reset {
            fn drop(&mut self) {
                TEST_OVERRIDE.with(|c| *c.borrow_mut() = None);
            }
        }
        TEST_OVERRIDE.with(|c| *c.borrow_mut() = Some(TestOverride { _state: state, keyring }));
        let _reset = Reset;
        f()
    }

    pub(super) fn key_storage() -> &'static str {
        "keyring"
    }

    pub(super) fn load_main(dir: &Path) -> Main {
        #[cfg(test)]
        if let Some(m) = TEST_OVERRIDE.with(|c| c.borrow().as_ref().map(|o| o.keyring.get(dir))) {
            return m;
        }
        match entry(dir) {
            Ok(e) => classify(e.get_password()),
            Err(e) => Main::Failed(e),
        }
    }

    pub(super) fn save_main(dir: &Path, me: &Identity) -> Result<(), String> {
        #[cfg(test)]
        if let Some(r) = TEST_OVERRIDE.with(|c| c.borrow().as_ref().map(|o| o.keyring.set(dir, me))) {
            return r;
        }
        entry(dir)?
            .set_password(&encode(me))
            .map_err(|e| e.to_string())
    }

    pub(super) fn forget_main(dir: &Path) -> Result<(), String> {
        #[cfg(test)]
        if let Some(r) = TEST_OVERRIDE.with(|c| c.borrow().as_ref().map(|o| o.keyring.delete(dir))) {
            return r;
        }
        match entry(dir)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(e.to_string()),
        }
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
}

/// Linux keyring round trip, run by CI (build-linux.yml) across five D-Bus
/// setups: no bus (file), bus without Secret Service (file), activatable locked
/// collection (keyring only, Unreadable, file untouched), unlocked GNOME Keyring
/// (keyring only, file untouched), and hanging D-Bus socket (indeterminate ->
/// keyring only, Unreadable, file untouched).
#[cfg(all(test, target_os = "linux"))]
mod linux_keyring {
    use std::os::unix::fs::PermissionsExt;

    use super::*;

    fn plant_file(dir: &Path, text: &str, mode: u32) -> std::path::PathBuf {
        let store = dir.join("identity-store");
        std::fs::create_dir_all(&store).unwrap();
        std::fs::set_permissions(&store, std::fs::Permissions::from_mode(0o700)).unwrap();
        let file = store.join("identity.key");
        std::fs::write(&file, text).unwrap();
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(mode)).unwrap();
        file
    }

    #[test]
    #[ignore = "needs a chosen D-Bus/keyring setup; run by CI"]
    fn round_trip() {
        let expect = std::env::var("HUBCHAT_KEYRING_EXPECT").expect("HUBCHAT_KEYRING_EXPECT");
        let dir = std::env::temp_dir().join(format!("hubchat-keyring-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let me = Identity::generate("ci").unwrap();
        match expect.as_str() {
            "file" => {
                assert!(matches!(load_identity(&dir), Loaded::Missing), "missing subdir is Missing");
                assert_eq!(linux_key_storage(), "file");
                save_identity(&dir, &me).expect("save to file backend");
                let store = dir.join("identity-store");
                let key = store.join("identity.key");
                assert_eq!(std::fs::metadata(&store).unwrap().permissions().mode() & 0o7777, 0o700);
                assert_eq!(std::fs::metadata(&key).unwrap().permissions().mode() & 0o7777, 0o600);
                assert!(!store.join("identity.key.tmp").exists(), "tmp must not linger");
                let Loaded::Found(back) = load_identity(&dir) else { panic!("load after file save") };
                assert_eq!((back.id(), back.secret()), (me.id(), me.secret()));
                forget_identity(&dir).expect("forget on file backend");
                assert!(!key.exists(), "identity.key removed");
                assert!(store.is_dir(), "identity-store subdir kept");
                assert!(matches!(load_identity(&dir), Loaded::Missing), "gone after forget");
                let log = std::fs::read_to_string(dir.join("identity-log.txt")).unwrap();
                assert!(log.contains("permissions-only file"), "keylog notes file backend: {log}");
                assert!(!log.contains(me.secret()), "keylog must never contain secret");
            }
            "locked" | "hang" => {
                let planted_id = Identity::generate("planted").unwrap();
                let planted_text = encode(&planted_id);
                let key = plant_file(&dir, &planted_text, 0o600);
                let loaded = load_identity(&dir);
                eprintln!("load_identity({expect}): {}", match &loaded {
                    Loaded::Found(_) => "Found",
                    Loaded::Restored(_) => "Restored",
                    Loaded::Missing => "Missing",
                    Loaded::Unreadable => "Unreadable",
                });
                assert!(matches!(loaded, Loaded::Unreadable), "locked/hang must be Unreadable, never file fallback");
                assert_eq!(linux_key_storage(), "keyring");
                let saved = save_identity(&dir, &me);
                eprintln!("save_identity({expect}): {saved:?}");
                assert!(saved.is_err(), "saving must fail when keyring is unavailable");
                assert_eq!(std::fs::read_to_string(&key).unwrap(), planted_text, "planted file untouched");
                if expect == "hang" {
                    let log = std::fs::read_to_string(dir.join("identity-log.txt")).unwrap();
                    assert!(log.contains("indeterminate"), "hang must log indeterminate probe: {log}");
                }
            }
            "ok" => {
                let planted_id = Identity::generate("planted").unwrap();
                let planted_text = encode(&planted_id);
                let key = plant_file(&dir, &planted_text, 0o000);
                save_identity(&dir, &me).expect("save to unlocked keyring");
                assert_eq!(linux_key_storage(), "keyring");
                let Loaded::Found(back) = load_identity(&dir) else { panic!("load after keyring save") };
                assert_eq!((back.id(), back.secret()), (me.id(), me.secret()));
                forget_identity(&dir).expect("forget from unlocked keyring");
                assert!(matches!(load_identity(&dir), Loaded::Missing), "gone after keyring forget");
                assert!(key.exists(), "keyring backend must never delete planted file");
                assert_eq!(std::fs::metadata(&key).unwrap().permissions().mode() & 0o7777, 0o000);
                std::fs::set_permissions(&key, std::fs::Permissions::from_mode(0o600)).unwrap();
                assert_eq!(std::fs::read_to_string(&key).unwrap(), planted_text, "planted file bytes untouched");
            }
            other => panic!("HUBCHAT_KEYRING_EXPECT={other}"),
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[cfg(all(test, target_os = "linux"))]
mod linux_r5_tests {
    use std::os::unix::fs::{symlink, PermissionsExt};
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    use super::linux::{
        classify_dbus_probe, judge_key_file, judge_store_dir, with_test_backend, BusHints,
        DbusProbeResult, MockKeyring, SecretServiceState, StatInfo,
    };
    use super::*;

    static SEQ: AtomicU64 = AtomicU64::new(0);

    fn temp_dir(label: &str) -> PathBuf {
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        let d = std::env::temp_dir().join(format!(
            "hubchat-r5-{label}-{}-{n}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn r5_probe_absent_uses_file_backend_and_never_calls_keyring() {
        let dir = temp_dir("absent");
        let me = Identity::generate("frame").unwrap();
        let kr = MockKeyring::new();
        with_test_backend(SecretServiceState::Absent, kr.clone(), || {
            assert!(matches!(load_identity(&dir), Loaded::Missing));
            assert_eq!(linux_key_storage(), "file");
            save_identity(&dir, &me).expect("file backend save");
            let Loaded::Found(back) = load_identity(&dir) else {
                panic!("file backend load should return Found");
            };
            assert_eq!((back.id(), back.secret()), (me.id(), me.secret()));
        });
        assert_eq!(kr.calls(), 0, "keyring must never be called when Secret Service is absent");
        let store = dir.join("identity-store");
        let key = store.join("identity.key");
        assert_eq!(std::fs::metadata(&store).unwrap().permissions().mode() & 0o7777, 0o700);
        assert_eq!(std::fs::metadata(&key).unwrap().permissions().mode() & 0o7777, 0o600);
        assert!(!store.join("identity.key.tmp").exists());
        let log = std::fs::read_to_string(dir.join("identity-log.txt")).unwrap();
        assert!(log.contains("permissions-only file"), "keylog should note file backend: {log}");
        assert!(!log.contains(me.secret()), "keylog must never contain secret");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn r5_probe_present_and_indeterminate_use_keyring_and_never_open_planted_file() {
        // Probe classification checks for present, absent, indeterminate.
        let no_hints = BusHints {
            address_set: false,
            address_disabled: false,
            runtime_bus_exists: Some(false),
            has_display: false,
        };
        assert_eq!(
            classify_dbus_probe(no_hints, || DbusProbeResult::Owned),
            SecretServiceState::Present
        );
        assert_eq!(
            classify_dbus_probe(no_hints, || DbusProbeResult::NotOwned {
                activatable: vec!["org.freedesktop.secrets".into()]
            }),
            SecretServiceState::Present
        );
        assert_eq!(
            classify_dbus_probe(no_hints, || DbusProbeResult::NotOwned { activatable: vec![] }),
            SecretServiceState::Absent
        );
        assert_eq!(
            classify_dbus_probe(no_hints, || DbusProbeResult::TimedOut),
            SecretServiceState::Indeterminate
        );
        assert_eq!(
            classify_dbus_probe(no_hints, || DbusProbeResult::QueryError),
            SecretServiceState::Indeterminate
        );

        for (state, expect_log) in [
            (SecretServiceState::Present, "Secret Service present"),
            (SecretServiceState::Indeterminate, "indeterminate"),
        ] {
            let dir = temp_dir(if state == SecretServiceState::Present { "present" } else { "indet" });
            let store = dir.join("identity-store");
            std::fs::create_dir_all(&store).unwrap();
            std::fs::set_permissions(&store, std::fs::Permissions::from_mode(0o700)).unwrap();
            let planted = store.join("identity.key");
            std::fs::write(&planted, "unreadable-sentinel").unwrap();
            // Mode 0000: opening this file would fail with EACCES if touched.
            std::fs::set_permissions(&planted, std::fs::Permissions::from_mode(0o000)).unwrap();

            let me = Identity::generate("desktop").unwrap();
            let kr = MockKeyring::new();
            kr.set(&dir, &me).unwrap();
            let before_calls = kr.calls();
            with_test_backend(state, kr.clone(), || {
                assert_eq!(linux_key_storage(), "keyring");
                let Loaded::Found(back) = load_identity(&dir) else {
                    panic!("keyring backend should return Found without touching planted file");
                };
                assert_eq!(back.address(), me.address());
                save_identity(&dir, &me).unwrap();
                forget_identity(&dir).unwrap();
            });
            assert!(kr.calls() > before_calls, "keyring backend must be used");
            assert!(planted.exists(), "planted identity.key must still exist untouched");
            assert_eq!(std::fs::metadata(&planted).unwrap().permissions().mode() & 0o7777, 0o000);
            let log = std::fs::read_to_string(dir.join("identity-log.txt")).unwrap();
            assert!(log.contains(expect_log), "keylog must record backend choice ({expect_log}): {log}");
            std::fs::set_permissions(&planted, std::fs::Permissions::from_mode(0o600)).unwrap();
            assert_eq!(std::fs::read_to_string(&planted).unwrap(), "unreadable-sentinel");
            let _ = std::fs::remove_dir_all(&dir);
        }
    }

    #[test]
    fn r5_file_backend_rejects_bad_mode_owner_symlink_nlink_dir_mode_and_oversize_without_deleting() {
        // Pure stat owner mismatch checks (root uid 0 vs euid 1000).
        assert!(judge_store_dir(
            1000,
            StatInfo { is_dir: true, is_reg: false, is_symlink: false, uid: 0, mode: 0o040700, nlink: 2, size: 0 }
        ).is_err());
        assert!(judge_key_file(
            1000,
            StatInfo { is_dir: false, is_reg: true, is_symlink: false, uid: 0, mode: 0o100600, nlink: 1, size: 64 }
        ).is_err());

        let me = Identity::generate("alex").unwrap();
        let encoded = encode(&me);
        let kr = MockKeyring::new();

        // 1. file mode 0644 -> Unreadable, never Missing, never deleted
        {
            let dir = temp_dir("mode0644");
            let store = dir.join("identity-store");
            std::fs::create_dir_all(&store).unwrap();
            std::fs::set_permissions(&store, std::fs::Permissions::from_mode(0o700)).unwrap();
            let key = store.join("identity.key");
            std::fs::write(&key, &encoded).unwrap();
            std::fs::set_permissions(&key, std::fs::Permissions::from_mode(0o644)).unwrap();
            with_test_backend(SecretServiceState::Absent, kr.clone(), || {
                assert!(matches!(load_identity(&dir), Loaded::Unreadable));
            });
            assert!(key.exists(), "0644 file must not be deleted");
            assert_eq!(std::fs::metadata(&key).unwrap().permissions().mode() & 0o7777, 0o644);
            let _ = std::fs::remove_dir_all(&dir);
        }

        // 2. symlink identity.key -> Unreadable, never deleted
        {
            let dir = temp_dir("symlink");
            let store = dir.join("identity-store");
            std::fs::create_dir_all(&store).unwrap();
            std::fs::set_permissions(&store, std::fs::Permissions::from_mode(0o700)).unwrap();
            let real = dir.join("target.key");
            std::fs::write(&real, &encoded).unwrap();
            std::fs::set_permissions(&real, std::fs::Permissions::from_mode(0o600)).unwrap();
            let key = store.join("identity.key");
            symlink(&real, &key).unwrap();
            with_test_backend(SecretServiceState::Absent, kr.clone(), || {
                assert!(matches!(load_identity(&dir), Loaded::Unreadable));
            });
            assert!(std::fs::symlink_metadata(&key).unwrap().file_type().is_symlink());
            let _ = std::fs::remove_dir_all(&dir);
        }

        // 3. hard link (nlink == 2) -> Unreadable, never deleted
        {
            let dir = temp_dir("nlink2");
            let store = dir.join("identity-store");
            std::fs::create_dir_all(&store).unwrap();
            std::fs::set_permissions(&store, std::fs::Permissions::from_mode(0o700)).unwrap();
            let key = store.join("identity.key");
            std::fs::write(&key, &encoded).unwrap();
            std::fs::set_permissions(&key, std::fs::Permissions::from_mode(0o600)).unwrap();
            let alias = dir.join("alias.key");
            std::fs::hard_link(&key, &alias).unwrap();
            with_test_backend(SecretServiceState::Absent, kr.clone(), || {
                assert!(matches!(load_identity(&dir), Loaded::Unreadable));
            });
            assert!(key.exists() && alias.exists(), "hard-linked files must not be deleted");
            let _ = std::fs::remove_dir_all(&dir);
        }

        // 4. identity-store dir mode 0755 -> Unreadable, save refused, never chmod'd or deleted
        {
            let dir = temp_dir("dir0755");
            let store = dir.join("identity-store");
            std::fs::create_dir_all(&store).unwrap();
            std::fs::set_permissions(&store, std::fs::Permissions::from_mode(0o755)).unwrap();
            let key = store.join("identity.key");
            std::fs::write(&key, &encoded).unwrap();
            std::fs::set_permissions(&key, std::fs::Permissions::from_mode(0o600)).unwrap();
            with_test_backend(SecretServiceState::Absent, kr.clone(), || {
                assert!(matches!(load_identity(&dir), Loaded::Unreadable));
                assert!(save_identity(&dir, &me).is_err());
            });
            assert_eq!(std::fs::metadata(&store).unwrap().permissions().mode() & 0o7777, 0o755);
            assert!(key.exists());
            let _ = std::fs::remove_dir_all(&dir);
        }

        // 5. >64 KiB file -> Unreadable, never deleted
        {
            let dir = temp_dir("oversize");
            let store = dir.join("identity-store");
            std::fs::create_dir_all(&store).unwrap();
            std::fs::set_permissions(&store, std::fs::Permissions::from_mode(0o700)).unwrap();
            let key = store.join("identity.key");
            let big = format!("{} {}", me.id(), "a".repeat(64 * 1024 + 1));
            std::fs::write(&key, &big).unwrap();
            std::fs::set_permissions(&key, std::fs::Permissions::from_mode(0o600)).unwrap();
            with_test_backend(SecretServiceState::Absent, kr.clone(), || {
                assert!(matches!(load_identity(&dir), Loaded::Unreadable));
            });
            assert!(key.exists());
            let _ = std::fs::remove_dir_all(&dir);
        }

        assert_eq!(kr.calls(), 0, "file backend errors must never call keyring");
    }

    #[test]
    fn r5_file_backend_forget_removes_only_key_and_tmp_leaving_subdir_and_siblings() {
        let dir = temp_dir("forget");
        let me = Identity::generate("alex").unwrap();
        let kr = MockKeyring::new();
        with_test_backend(SecretServiceState::Absent, kr.clone(), || {
            save_identity(&dir, &me).expect("save before forget");
            let store = dir.join("identity-store");
            let key = store.join("identity.key");
            let tmp = store.join("identity.key.tmp");
            let other = store.join("keep.txt");
            let sibling = dir.join("hubchat.sqlite3");
            std::fs::write(&tmp, "stale-tmp").unwrap();
            std::fs::write(&other, "keep").unwrap();
            std::fs::write(&sibling, "db").unwrap();

            forget_identity(&dir).expect("forget removes key + tmp");
            assert!(!key.exists(), "identity.key removed");
            assert!(!tmp.exists(), "identity.key.tmp removed");
            assert!(store.is_dir(), "identity-store subdir left in place");
            assert!(other.exists(), "unrelated file in subdir untouched");
            assert!(sibling.exists(), "sibling in data dir untouched");
            // ENOENT on second forget is Ok
            forget_identity(&dir).expect("second forget is Ok on ENOENT");
            assert!(matches!(load_identity(&dir), Loaded::Missing));
        });
        assert_eq!(kr.calls(), 0);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
