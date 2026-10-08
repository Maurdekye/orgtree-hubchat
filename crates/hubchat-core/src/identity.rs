//! Identity: a secret key plus a chosen id. The address tag is derived from the
//! secret the same way the hub fingerprints it (sha256), so the same identity
//! has the same address `id.tag` on every hub.

use rand::RngCore;
use sha2::{Digest, Sha256};

use crate::{Error, Result};

#[derive(Clone)]
pub struct Identity {
    id: String,
    secret: String,
}

// Never print the secret.
impl std::fmt::Debug for Identity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Identity").field("address", &self.address()).finish_non_exhaustive()
    }
}

impl Identity {
    /// A brand-new identity with a fresh 256-bit secret.
    pub fn generate(id: &str) -> Result<Self> {
        let mut key = [0u8; 32];
        rand::thread_rng().fill_bytes(&mut key);
        Self::from_parts(id, &hex::encode(key))
    }

    /// Restore an identity from its id and secret (linking, recovery).
    pub fn from_parts(id: &str, secret: &str) -> Result<Self> {
        validate_id(id)?;
        if secret.len() < 32 || secret.chars().any(char::is_whitespace) {
            return Err(Error::Invalid("secret key is malformed".into()));
        }
        Ok(Self { id: id.to_owned(), secret: secret.to_owned() })
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn secret(&self) -> &str {
        &self.secret
    }

    /// First 6 hex characters of sha256(secret): the hub's display suffix.
    pub fn tag(&self) -> String {
        hex::encode(Sha256::digest(self.secret.as_bytes()))[..6].to_owned()
    }

    /// The hub slug, `id.tag`. Shown to people as `@net:id.tag`.
    pub fn address(&self) -> String {
        format!("{}.{}", self.id, self.tag())
    }

    /// Value for the `X-Org-Auth` header.
    pub(crate) fn auth_header(&self) -> String {
        format!("{}:{}", self.address(), self.secret)
    }
}

/// An id is the part of a hub slug before the tag: lower-case letters, digits,
/// `.`, `_`, `-`; starts with a letter or digit. With the 7-char `.tag` suffix
/// the whole slug must fit the hub's 128-char limit.
pub fn validate_id(id: &str) -> Result<()> {
    let ok_first = id.chars().next().is_some_and(|c| c.is_ascii_lowercase() || c.is_ascii_digit());
    let ok_rest = id
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '.' | '_' | '-'));
    if id.is_empty() || !ok_first || !ok_rest || id.len() > 121 {
        return Err(Error::Invalid(
            "an id uses a-z, 0-9, '.', '_' or '-', starts with a letter or digit, max 121 characters".into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn address_is_id_dot_sha256_prefix() {
        let me = Identity::from_parts("alex", "0123456789abcdef0123456789abcdef").unwrap();
        let fp = hex::encode(Sha256::digest(b"0123456789abcdef0123456789abcdef"));
        assert_eq!(me.address(), format!("alex.{}", &fp[..6]));
    }

    #[test]
    fn rejects_bad_ids() {
        for bad in ["", "Alex", "-a", "a b", "a@b"] {
            assert!(validate_id(bad).is_err(), "{bad:?}");
        }
        assert!(validate_id("maya.lin_2").is_ok());
    }

    #[test]
    fn debug_hides_secret() {
        let me = Identity::generate("alex").unwrap();
        assert!(!format!("{me:?}").contains(me.secret()));
    }
}
