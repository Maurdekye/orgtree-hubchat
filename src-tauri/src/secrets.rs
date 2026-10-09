//! Where the identity key lives. Windows: Credential Manager. Android: a file
//! in app-private storage sealed with an Android Keystore key (SecretBox.kt),
//! with app backup disabled. The key never goes to the UI or into logs.

use std::path::Path;

use hubchat_core::Identity;

#[cfg(desktop)]
const SERVICE: &str = "dev.orgtree.hubchat";

fn encode(me: &Identity) -> String {
    format!("{} {}", me.id(), me.secret())
}

fn decode(s: &str) -> Option<Identity> {
    let (id, secret) = s.trim().split_once(' ')?;
    Identity::from_parts(id, secret).ok()
}

#[cfg(desktop)]
fn entry(dir: &Path) -> Result<keyring::Entry, String> {
    // One credential per data folder, so a second profile/dev build can't clash.
    let user = format!("identity:{}", dir.to_string_lossy());
    keyring::Entry::new(SERVICE, &user).map_err(|e| e.to_string())
}

#[cfg(desktop)]
pub fn save_identity(dir: &Path, me: &Identity) -> Result<(), String> {
    entry(dir)?
        .set_password(&encode(me))
        .map_err(|e| e.to_string())
}

#[cfg(desktop)]
pub fn load_identity(dir: &Path) -> Option<Identity> {
    decode(&entry(dir).ok()?.get_password().ok()?)
}

#[cfg(desktop)]
pub fn forget_identity(dir: &Path) -> Result<(), String> {
    match entry(dir)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(e.to_string()),
    }
}

#[cfg(target_os = "android")]
pub fn save_identity(dir: &Path, me: &Identity) -> Result<(), String> {
    let sealed =
        crate::android::seal(&encode(me)).ok_or("the Android Keystore refused to seal the key")?;
    let tmp = dir.join("identity.sealed.tmp");
    std::fs::write(&tmp, sealed).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, dir.join("identity.sealed")).map_err(|e| e.to_string())
}

#[cfg(target_os = "android")]
pub fn load_identity(dir: &Path) -> Option<Identity> {
    let sealed = std::fs::read_to_string(dir.join("identity.sealed")).ok()?;
    decode(&crate::android::open(&sealed)?)
}

#[cfg(target_os = "android")]
pub fn forget_identity(dir: &Path) -> Result<(), String> {
    match std::fs::remove_file(dir.join("identity.sealed")) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.to_string()),
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
                assert!(load_identity(&dir).is_none(), "nothing may be kept in memory");
            }
            "ok" => {
                saved.unwrap();
                let back = load_identity(&dir).expect("load after save");
                assert_eq!(back.id(), me.id());
                assert_eq!(back.secret(), me.secret());
                forget_identity(&dir).unwrap();
                assert!(load_identity(&dir).is_none(), "gone after forget");
            }
            other => panic!("HUBCHAT_KEYRING_EXPECT={other}"),
        }
    }
}
