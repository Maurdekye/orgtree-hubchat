//! Recovery words (user ruling 2026-10-08, option B): 24 BIP-39 English words
//! carry the 128-bit key AND the id, so recovering needs only the words.
//!
//! The 256 bits of entropy are `key (16 bytes) || id (16 bytes)`. The id is a
//! base-40 number, least significant digit first: 0 = end/padding, a-z = 1-26,
//! 0-9 = 27-36, '.' = 37, '_' = 38, '-' = 39. 40^24 < 2^128, so ids of up to
//! [`MAX_ID_LEN`] = 24 characters fit. BIP-39's own 8-bit checksum catches a
//! mistyped or swapped word.

use crate::identity::validate_id;
use crate::{Error, Identity, Result};

/// Longest id that fits in the recovery words.
pub const MAX_ID_LEN: usize = 24;

const ALPHABET: &[u8] = b"abcdefghijklmnopqrstuvwxyz0123456789._-";

fn id_to_u128(id: &str) -> Result<u128> {
    if id.len() > MAX_ID_LEN {
        return Err(Error::Invalid(format!(
            "an id can be at most {MAX_ID_LEN} characters"
        )));
    }
    let mut n: u128 = 0;
    for b in id.bytes().rev() {
        let d = ALPHABET.iter().position(|&a| a == b).ok_or_else(|| {
            Error::Invalid("id has a character recovery words can't carry".into())
        })?;
        n = n * 40 + (d as u128 + 1);
    }
    Ok(n)
}

fn u128_to_id(mut n: u128) -> Result<String> {
    let mut s = String::new();
    while n > 0 {
        let d = (n % 40) as usize;
        n /= 40;
        if d == 0 {
            return Err(Error::Invalid("these words don't hold a valid id".into()));
        }
        s.push(ALPHABET[d - 1] as char);
    }
    Ok(s)
}

/// The 24 words for an identity. Only identities with a 128-bit key (32 hex
/// characters, what `Identity::generate` makes) can be written as words.
pub fn to_words(me: &Identity) -> Result<Vec<String>> {
    let key = hex::decode(me.secret())
        .ok()
        .filter(|k| k.len() == 16)
        .ok_or_else(|| {
            Error::Invalid("this identity's key can't be written as recovery words".into())
        })?;
    let mut entropy = [0u8; 32];
    entropy[..16].copy_from_slice(&key);
    entropy[16..].copy_from_slice(&id_to_u128(me.id())?.to_be_bytes());
    let m = bip39::Mnemonic::from_entropy(&entropy).map_err(|e| Error::Invalid(e.to_string()))?;
    Ok(m.words().map(str::to_owned).collect())
}

/// The saved recovery text, shared by desktop/Android and synthetic fixtures.
pub fn to_file(me: &Identity, saved_at: &str) -> Result<String> {
    let words = to_words(me)?;
    let mut text = String::new();
    text.push_str("Hubchat recovery words\r\n\r\n");
    text.push_str(&format!("Address: @net:{}\r\n", me.address()));
    text.push_str(&format!("Saved:   {saved_at}\r\n\r\n"));
    for (i, w) in words.iter().enumerate() {
        text.push_str(&format!("{:>2}. {w}\r\n", i + 1));
    }
    text.push_str(
        "\r\nThese 24 words ARE your Hubchat identity: they bring back your address on a new\r\n",
    );
    text.push_str(
        "device (Hubchat > I already use Hubchat > Recovery words). Anyone who has them can\r\n",
    );
    text.push_str(
        "be you. Keep this file somewhere safe and private, or print it and delete it.\r\n",
    );
    Ok(text)
}

/// Rebuild the identity from its 24 words (any spacing and case).
pub fn from_words(words: &str) -> Result<Identity> {
    let norm = words
        .split_whitespace()
        .map(str::to_lowercase)
        .collect::<Vec<_>>()
        .join(" ");
    let count = norm.split(' ').filter(|w| !w.is_empty()).count();
    if count != 24 {
        return Err(Error::Invalid(format!(
            "recovery needs all 24 words; got {count}"
        )));
    }
    let m = bip39::Mnemonic::parse_normalized(&norm).map_err(|e| match e {
        bip39::Error::UnknownWord(i) => {
            Error::Invalid(format!("word {} isn't a recovery word", i + 1))
        }
        bip39::Error::InvalidChecksum => {
            Error::Invalid("these words don't check out: one is mistyped or out of order".into())
        }
        e => Error::Invalid(e.to_string()),
    })?;
    let entropy = m.to_entropy();
    if entropy.len() != 32 {
        return Err(Error::Invalid("recovery needs all 24 words".into()));
    }
    let id = u128_to_id(u128::from_be_bytes(entropy[16..].try_into().unwrap()))?;
    validate_id(&id)?;
    Identity::from_parts(&id, &hex::encode(&entropy[..16]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saved_file_keeps_the_existing_export_format() {
        let me = Identity::from_parts("recovery-proof", "00112233445566778899aabbccddeeff").unwrap();
        let text = to_file(&me, "2026-10-25T00:00:00Z").unwrap();
        let lines: Vec<_> = text.split("\r\n").collect();
        assert_eq!(lines[0], "Hubchat recovery words");
        assert_eq!(lines[2], format!("Address: @net:{}", me.address()));
        assert_eq!(lines[3], "Saved:   2026-10-25T00:00:00Z");
        let mut words = Vec::new();
        for (i, line) in lines[5..29].iter().enumerate() {
            let prefix = format!("{:>2}. ", i + 1);
            words.push(line.strip_prefix(&prefix).unwrap());
        }
        assert_eq!(from_words(&words.join(" ")).unwrap().address(), me.address());
        assert_eq!(lines.len(), 34);
        assert_eq!(lines[30], "These 24 words ARE your Hubchat identity: they bring back your address on a new");
        assert_eq!(lines[31], "device (Hubchat > I already use Hubchat > Recovery words). Anyone who has them can");
        assert_eq!(lines[32], "be you. Keep this file somewhere safe and private, or print it and delete it.");
        assert!(lines[33].is_empty());
    }

    #[test]
    fn round_trips_ids_up_to_the_limit() {
        for id in ["a", "alex", "maya.lin_2-x", "z9", &"9".repeat(MAX_ID_LEN)] {
            let me = Identity::generate(id).unwrap();
            let words = to_words(&me).unwrap();
            assert_eq!(words.len(), 24);
            let back = from_words(&words.join("  ").to_uppercase()).unwrap();
            assert_eq!(back.address(), me.address());
            assert_eq!(back.secret(), me.secret());
        }
    }

    #[test]
    fn largest_id_fits_in_128_bits() {
        assert!(id_to_u128(&"-".repeat(MAX_ID_LEN)).is_ok()); // all digits 39: 40^24 - 1
        assert!(id_to_u128(&"a".repeat(MAX_ID_LEN + 1)).is_err());
    }

    #[test]
    fn catches_mistakes() {
        let me = Identity::generate("alex").unwrap();
        let mut w = to_words(&me).unwrap();
        assert!(from_words(&w[..23].join(" "))
            .unwrap_err()
            .to_string()
            .contains("24"));
        w[5] = "notaword".into();
        assert!(from_words(&w.join(" "))
            .unwrap_err()
            .to_string()
            .contains("word 6"));
    }
}
