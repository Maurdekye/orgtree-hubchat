//! Where the identity key lives. Windows: Credential Manager, plus a backup
//! copy in Hubchat's own folder sealed with Windows' DPAPI for this user
//! (user 2026-10-10 08:13Z, after Windows lost every saved sign-in in a
//! crash and Hubchat started over without a word). macOS and Linux: the
//! system keyring. Android: a file in app-private storage sealed with an
//! Android Keystore key (SecretBox.kt), with app backup disabled. The key
//! never goes to the UI or into logs.

use std::path::Path;

use hubchat_core::Identity;

use crate::keylog;

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
    let (why, failed) = match load_main(dir) {
        Main::Found(me) => {
            // an older Hubchat kept no backup: make one now
            if !backup::exists(dir) {
                note_backup(dir, backup::save(dir, &encode(&me)));
            }
            keylog::note(dir, &format!("key found in {STORE}"));
            return Loaded::Found(me);
        }
        Main::Missing(why) => (why, false),
        Main::Failed(why) => (why, true),
    };
    match backup::load(dir) {
        Ok(Some(text)) => {
            if let Some(me) = decode(&text) {
                let back = match save_main(dir, &me) {
                    Ok(()) => format!("put back into {STORE}"),
                    Err(e) => format!("{STORE} refused it back ({e}); running from the backup"),
                };
                keylog::note(dir, &format!("{why}; key restored from Hubchat's backup, {back}"));
                return Loaded::Restored(me);
            }
            keylog::note(dir, &format!("{why}; Hubchat's backup is damaged"));
        }
        Ok(None) => keylog::note(dir, &format!("{why}; no backup")),
        Err(e) => keylog::note(dir, &format!("{why}; the backup didn't open ({e})")),
    }
    if failed {
        Loaded::Unreadable
    } else {
        Loaded::Missing
    }
}

/// Keep the key: the main copy must take it; the backup follows where there
/// is one (a failed backup only goes to the key log).
pub fn save_identity(dir: &Path, me: &Identity) -> Result<(), String> {
    save_main(dir, me)?;
    note_backup(dir, backup::save(dir, &encode(me)));
    Ok(())
}

/// Forget the key: the main copy and the backup.
pub fn forget_identity(dir: &Path) -> Result<(), String> {
    forget_main(dir)?;
    backup::forget(dir)
}

fn note_backup(dir: &Path, r: Result<(), String>) {
    if let Err(e) = r {
        keylog::note(dir, &format!("the backup couldn't be written ({e})"));
    }
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

    pub fn exists(dir: &Path) -> bool {
        path(dir).is_file()
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

    pub fn exists(_dir: &Path) -> bool {
        true
    }
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
