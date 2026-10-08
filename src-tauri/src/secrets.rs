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
#[allow(dead_code)] // used by "Remove this identity" (Settings), not wired yet
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
#[allow(dead_code)] // used by "Remove this identity" (Settings), not wired yet
pub fn forget_identity(dir: &Path) -> Result<(), String> {
    match std::fs::remove_file(dir.join("identity.sealed")) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.to_string()),
    }
}
