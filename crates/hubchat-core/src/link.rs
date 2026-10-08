//! Bringing an identity to another device (design D1, user ruling 12:35Z):
//!
//! (a) **Link through a hub.** The new device makes a one-time *link code*
//!     and registers a throwaway address derived from it on a hub. A
//!     signed-in device reads the code (QR or typed), derives the same
//!     address, sees the new device's name in the hub's roster, and — if the
//!     user approves — sends the identity bundle to that address sealed with
//!     a key derived from the code. The hub only ever sees ciphertext. The
//!     code is good for one use and ten minutes (the new device stops
//!     listening and unregisters).
//! (b) **Offline QR.** The signed-in device shows the bundle itself as a QR.
//! (c) **Key file.** The bundle sealed with a passphrase (Argon2id).
//!
//! Bundle = key + id + profile + hub list.

use argon2::Argon2;
use base64::engine::general_purpose::STANDARD_NO_PAD as B64;
use base64::Engine as _;
use chacha20poly1305::aead::{Aead, KeyInit};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{Error, Identity, Result};

/// The identity and what travels with it.
#[derive(Clone, Serialize, Deserialize)]
pub struct Bundle {
    pub id: String,
    pub secret: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub about: String,
    #[serde(default)]
    pub hubs: Vec<String>,
}

impl std::fmt::Debug for Bundle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Bundle")
            .field("id", &self.id)
            .field("hubs", &self.hubs)
            .finish_non_exhaustive()
    }
}

impl Bundle {
    pub fn identity(&self) -> Result<Identity> {
        Identity::from_parts(&self.id, &self.secret)
    }
}

// Crockford base32 without I L O U: easy to read aloud and type.
const ALPHABET: &[u8] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";
/// Typed code: 16 characters = 80 bits, shown as XXXX-XXXX-XXXX-XXXX.
const CODE_CHARS: usize = 16;

/// A fresh link code, formatted for display.
pub fn new_link_code() -> String {
    let mut raw = [0u8; CODE_CHARS];
    rand::thread_rng().fill_bytes(&mut raw);
    let s: String = raw
        .iter()
        .map(|b| ALPHABET[(*b as usize) % 32] as char)
        .collect();
    format!("{}-{}-{}-{}", &s[..4], &s[4..8], &s[8..12], &s[12..])
}

/// Canonical form of a typed code: upper case, no separators, look-alikes
/// mapped (O->0, I/L->1). Errors if it isn't a code.
pub fn normalize_code(input: &str) -> Result<String> {
    let s: String = input
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '-')
        .map(|c| match c.to_ascii_uppercase() {
            'O' => '0',
            'I' | 'L' => '1',
            c => c,
        })
        .collect();
    if s.len() != CODE_CHARS || !s.bytes().all(|b| ALPHABET.contains(&b)) {
        return Err(Error::Invalid(
            "that isn't a link code (16 letters and digits)".into(),
        ));
    }
    Ok(s)
}

fn derive(code: &str, purpose: &[u8]) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(b"hubchat-link-v1\0");
    h.update(purpose);
    h.update(b"\0");
    h.update(code.as_bytes());
    h.finalize().into()
}

/// The throwaway identity a new device listens on while linking. Both sides
/// derive it from the code, so the code alone tells the old device where to
/// send.
pub fn link_identity(code: &str) -> Result<Identity> {
    let code = normalize_code(code)?;
    let secret = hex::encode(&derive(&code, b"address")[..16]);
    Identity::from_parts("link", &secret)
}

fn seal_with(key: &[u8; 32], plain: &[u8]) -> String {
    let mut nonce = [0u8; 24];
    rand::thread_rng().fill_bytes(&mut nonce);
    let ct = XChaCha20Poly1305::new(key.into())
        .encrypt(XNonce::from_slice(&nonce), plain)
        .expect("encrypt");
    let mut out = nonce.to_vec();
    out.extend(ct);
    B64.encode(out)
}

fn open_with(key: &[u8; 32], sealed: &str) -> Result<Vec<u8>> {
    let raw = B64
        .decode(sealed.trim())
        .map_err(|_| Error::Invalid("damaged data".into()))?;
    if raw.len() < 24 + 16 {
        return Err(Error::Invalid("damaged data".into()));
    }
    XChaCha20Poly1305::new(key.into())
        .decrypt(XNonce::from_slice(&raw[..24]), &raw[24..])
        .map_err(|_| Error::Invalid("wrong code or passphrase, or damaged data".into()))
}

/// Prefix of the hub message that carries a sealed bundle.
pub const LINK_MESSAGE_PREFIX: &str = "hubchat-link-v1:";

/// The message body the approving device sends to the link address.
pub fn seal_for_link(code: &str, bundle: &Bundle) -> Result<String> {
    let code = normalize_code(code)?;
    let plain = serde_json::to_vec(bundle).map_err(|e| Error::Invalid(e.to_string()))?;
    Ok(format!(
        "{LINK_MESSAGE_PREFIX}{}",
        seal_with(&derive(&code, b"seal"), &plain)
    ))
}

/// The new device opens what arrived on its link address.
pub fn open_from_link(code: &str, body: &str) -> Result<Bundle> {
    let code = normalize_code(code)?;
    let sealed = body
        .strip_prefix(LINK_MESSAGE_PREFIX)
        .ok_or_else(|| Error::Invalid("not a link message".into()))?;
    let plain = open_with(&derive(&code, b"seal"), sealed)?;
    serde_json::from_slice(&plain).map_err(|_| Error::Invalid("damaged data".into()))
}

/// Offline QR (b): the bundle in the clear, behind a warning in the UI.
pub const QR_PREFIX: &str = "hubchat-key-v1:";

pub fn to_qr(bundle: &Bundle) -> String {
    format!(
        "{QR_PREFIX}{}",
        B64.encode(serde_json::to_vec(bundle).expect("json"))
    )
}

pub fn from_qr(text: &str) -> Result<Bundle> {
    let b = text
        .trim()
        .strip_prefix(QR_PREFIX)
        .ok_or_else(|| Error::Invalid("that QR code isn't a Hubchat key".into()))?;
    let raw = B64
        .decode(b)
        .map_err(|_| Error::Invalid("damaged QR code".into()))?;
    serde_json::from_slice(&raw).map_err(|_| Error::Invalid("damaged QR code".into()))
}

/// Key file (c): passphrase of at least 8 characters (design §9).
pub const MIN_PASSPHRASE: usize = 8;

#[derive(Serialize, Deserialize)]
struct KeyFile {
    hubchat_key_file: u32,
    kdf: String,
    salt: String,
    data: String,
}

fn passphrase_key(pass: &str, salt: &[u8]) -> Result<[u8; 32]> {
    let mut key = [0u8; 32];
    Argon2::default()
        .hash_password_into(pass.as_bytes(), salt, &mut key)
        .map_err(|e| Error::Invalid(e.to_string()))?;
    Ok(key)
}

pub fn to_key_file(bundle: &Bundle, passphrase: &str) -> Result<String> {
    if passphrase.chars().count() < MIN_PASSPHRASE {
        return Err(Error::Invalid(format!(
            "use a passphrase of at least {MIN_PASSPHRASE} characters"
        )));
    }
    let mut salt = [0u8; 16];
    rand::thread_rng().fill_bytes(&mut salt);
    let key = passphrase_key(passphrase, &salt)?;
    let plain = serde_json::to_vec(bundle).map_err(|e| Error::Invalid(e.to_string()))?;
    let f = KeyFile {
        hubchat_key_file: 1,
        kdf: "argon2id".into(),
        salt: B64.encode(salt),
        data: seal_with(&key, &plain),
    };
    serde_json::to_string_pretty(&f).map_err(|e| Error::Invalid(e.to_string()))
}

pub fn from_key_file(contents: &str, passphrase: &str) -> Result<Bundle> {
    let f: KeyFile = serde_json::from_str(contents)
        .map_err(|_| Error::Invalid("that file isn't a Hubchat key file".into()))?;
    if f.hubchat_key_file != 1 || f.kdf != "argon2id" {
        return Err(Error::Invalid(
            "this key file comes from a newer Hubchat".into(),
        ));
    }
    let salt = B64
        .decode(&f.salt)
        .map_err(|_| Error::Invalid("damaged key file".into()))?;
    let plain = open_with(&passphrase_key(passphrase, &salt)?, &f.data)?;
    serde_json::from_slice(&plain).map_err(|_| Error::Invalid("damaged key file".into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bundle() -> Bundle {
        let me = Identity::generate("alex").unwrap();
        Bundle {
            id: me.id().into(),
            secret: me.secret().into(),
            name: "Alex".into(),
            about: String::new(),
            hubs: vec!["http://hub:7370".into()],
        }
    }

    #[test]
    fn link_round_trip_and_wrong_code() {
        let code = new_link_code();
        assert_eq!(code.len(), 19);
        // Both sides derive the same address, tolerant of how it was typed.
        let typed = code.to_lowercase().replace('-', " ");
        assert_eq!(
            link_identity(&code).unwrap().address(),
            link_identity(&typed).unwrap().address()
        );
        let b = bundle();
        let msg = seal_for_link(&code, &b).unwrap();
        assert!(!msg.contains(&b.secret), "the hub never sees the key");
        assert_eq!(open_from_link(&typed, &msg).unwrap().secret, b.secret);
        let other = new_link_code();
        assert!(open_from_link(&other, &msg).is_err());
        assert!(normalize_code("ABC").is_err());
    }

    #[test]
    fn qr_and_key_file_round_trip() {
        let b = bundle();
        assert_eq!(from_qr(&to_qr(&b)).unwrap().secret, b.secret);
        assert!(to_key_file(&b, "short").is_err());
        let f = to_key_file(&b, "correct horse").unwrap();
        assert!(!f.contains(&b.secret));
        assert_eq!(
            from_key_file(&f, "correct horse")
                .unwrap()
                .identity()
                .unwrap()
                .address(),
            b.identity().unwrap().address()
        );
        assert!(from_key_file(&f, "wrong horse!").is_err());
    }
}
