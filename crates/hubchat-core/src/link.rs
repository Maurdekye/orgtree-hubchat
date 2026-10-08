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
    /// The sending device has the recovery words saved: they are the same
    /// words here, so the receiving device doesn't ask again (user 22:14Z).
    /// Left out when false (older bundles have no such field).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub recovery_saved: bool,
}

impl std::fmt::Debug for Bundle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Bundle")
            .field("id", &self.id)
            .field("hubs", &self.hubs)
            .field("recovery_saved", &self.recovery_saved)
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

// ------------------------------------------------- the link QR as a URL

/// What a link QR carries: a URL a phone camera opens in Hubchat (user
/// 19:12Z), `hubchat://link?code=…&hub=…[&hub=…][&name=…]&role=give|take`.
/// Several `hub`s are one hub under the addresses another device may reach
/// it at, the likeliest first; `name` is that hub's name, so the device
/// that scans can tell it reached the right one (coordinator 20:28Z).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkUrl {
    /// Normalised (16 characters, no separators).
    pub code: String,
    pub hubs: Vec<String>,
    pub hub_name: Option<String>,
    /// `give`: a signed-in device offers its identity (the scanner joins);
    /// `take`: a new device asks for one (the scanner approves). None for a
    /// typed code.
    pub role: Option<String>,
}

/// XXXX-XXXX-XXXX-XXXX from a normalised code.
pub fn format_code(c: &str) -> String {
    if c.len() != CODE_CHARS {
        return c.to_string();
    }
    format!("{}-{}-{}-{}", &c[..4], &c[4..8], &c[8..12], &c[12..])
}

impl LinkUrl {
    pub fn to_url(&self) -> String {
        let enc = |v: &str| url::form_urlencoded::byte_serialize(v.as_bytes()).collect::<String>();
        let mut s = format!("hubchat://link?code={}", enc(&format_code(&self.code)));
        for h in &self.hubs {
            s.push_str(&format!("&hub={}", enc(h)));
        }
        if let Some(n) = &self.hub_name {
            s.push_str(&format!("&name={}", enc(n)));
        }
        if let Some(r) = &self.role {
            s.push_str(&format!("&role={}", enc(r)));
        }
        s
    }

    /// A typed code, the link URL, or the older `hubchat-link:CODE@HUB` text
    /// (a waiting new device's: role `take`).
    pub fn parse(input: &str) -> Result<Self> {
        let t = input.trim();
        let damaged = || Error::Invalid("damaged link QR code".into());
        if let Some(rest) = t.strip_prefix("hubchat-link:") {
            let (code, hub) = rest.split_once('@').ok_or_else(damaged)?;
            return Ok(LinkUrl {
                code: normalize_code(code)?,
                hubs: vec![hub.to_string()],
                hub_name: None,
                role: Some("take".into()),
            });
        }
        if t.starts_with("hubchat://") {
            let u = url::Url::parse(t).map_err(|_| damaged())?;
            let all = |k: &str| -> Vec<String> {
                u.query_pairs()
                    .filter(|(n, _)| n == k)
                    .map(|(_, v)| v.into_owned())
                    .filter(|v| !v.trim().is_empty())
                    .collect()
            };
            let code = all("code")
                .into_iter()
                .next()
                .ok_or_else(|| Error::Invalid("the link has no code".into()))?;
            return Ok(LinkUrl {
                code: normalize_code(&code)?,
                hubs: all("hub"),
                hub_name: all("name").into_iter().next(),
                role: all("role")
                    .into_iter()
                    .next()
                    .filter(|r| matches!(r.as_str(), "give" | "take")),
            });
        }
        Ok(LinkUrl {
            code: normalize_code(t)?,
            hubs: Vec::new(),
            hub_name: None,
            role: None,
        })
    }
}

/// True for a hub address that names the machine it is used on (localhost,
/// 127.0.0.0/8, ::1, 0.0.0.0): on another device it would name that device.
pub fn is_loopback_hub(url: &str) -> bool {
    let Ok(a) = crate::HubAddress::parse(url) else {
        return false;
    };
    let Ok(u) = url::Url::parse(&a.to_string()) else {
        return false;
    };
    match u.host() {
        Some(url::Host::Domain(d)) => {
            let d = d.trim_end_matches('.').to_ascii_lowercase();
            d == "localhost" || d.ends_with(".localhost")
        }
        Some(url::Host::Ipv4(ip)) => ip.is_loopback() || ip.is_unspecified(),
        Some(url::Host::Ipv6(ip)) => ip.is_loopback() || ip.is_unspecified(),
        None => false,
    }
}

/// The addresses another device may reach a hub at. A hub this device
/// reaches by a loopback address is offered under `hostname` and each of
/// `ips` (same scheme and port), then under the loopback address itself
/// (for a second app on this machine). Any other address is offered as is.
pub fn hub_aliases(url: &str, hostname: Option<&str>, ips: &[std::net::IpAddr]) -> Vec<String> {
    let norm = crate::HubAddress::parse(url)
        .map(|a| a.to_string())
        .unwrap_or_else(|_| url.to_string());
    if !is_loopback_hub(&norm) {
        return vec![norm];
    }
    let Ok(u) = url::Url::parse(&norm) else {
        return vec![norm];
    };
    let port = u.port().map(|p| format!(":{p}")).unwrap_or_default();
    let mut out: Vec<String> = Vec::new();
    let mut push = |host: String| {
        let a = format!("{}://{host}{port}", u.scheme());
        if let Ok(a) = crate::HubAddress::parse(&a) {
            let a = a.to_string();
            if !out.contains(&a) && !is_loopback_hub(&a) {
                out.push(a);
            }
        }
    };
    if let Some(h) = hostname.map(str::trim).filter(|h| !h.is_empty()) {
        push(h.to_ascii_lowercase());
    }
    for ip in ips {
        if ip.is_loopback() || ip.is_unspecified() || ip.is_multicast() {
            continue;
        }
        push(match ip {
            std::net::IpAddr::V4(v4) => v4.to_string(),
            std::net::IpAddr::V6(v6) => format!("[{v6}]"),
        });
    }
    out.push(norm);
    out
}

/// The hubs a device that received an identity through a link should use:
/// `via` (where it reached the link's hub) first, then the bundle's hubs,
/// except other names for that same hub (`aliases`, from the link) and,
/// once `via` is known, loopback addresses (on this device they would name
/// this device; user setup 2026-10-08: the PC reaches its own hub as
/// localhost:7370, the phone through Tailscale).
pub fn hubs_for_device(bundle_hubs: &[String], via: Option<&str>, aliases: &[String]) -> Vec<String> {
    let norm = |h: &str| {
        crate::HubAddress::parse(h)
            .map(|a| a.to_string())
            .unwrap_or_else(|_| h.to_string())
    };
    let aliases: Vec<String> = aliases.iter().map(|a| norm(a)).collect();
    let mut out: Vec<String> = Vec::new();
    if let Some(v) = via {
        out.push(norm(v));
    }
    for h in bundle_hubs {
        let h = norm(h);
        if out.contains(&h) {
            continue;
        }
        if via.is_some() && (aliases.contains(&h) || is_loopback_hub(&h)) {
            continue;
        }
        out.push(h);
    }
    out
}

/// The hubs that come with an identity a link brought, as rows the user
/// reviews before anything is saved (user 20:38Z): for each hub as the
/// other device knows it, the addresses to try on this device, likeliest
/// first. The link's hub (`via`, where this device reached it; `aliases`,
/// what the link called it) starts with `via`. Another loopback hub of the
/// other device is tried on the hosts the link named, at its own port. If
/// no bundle hub is recognisably the link's hub, the first loopback one is
/// taken to be it; failing that, `via` gets a row of its own, first.
pub fn review_hubs(bundle_hubs: &[String], via: &str, aliases: &[String]) -> Vec<(String, Vec<String>)> {
    let norm = |h: &str| {
        crate::HubAddress::parse(h)
            .map(|a| a.to_string())
            .unwrap_or_else(|_| h.to_string())
    };
    let via = norm(via);
    let aliases: Vec<String> = aliases.iter().map(|a| norm(a)).collect();
    let hubs: Vec<String> = bundle_hubs.iter().map(|h| norm(h)).collect();
    let link = hubs
        .iter()
        .position(|h| *h == via || aliases.contains(h))
        .or_else(|| hubs.iter().position(|h| is_loopback_hub(h)));
    // Hosts the link reached the other device by (no loopback ones).
    let hosts: Vec<(String, String)> = std::iter::once(via.clone())
        .chain(aliases.iter().cloned())
        .filter(|a| !is_loopback_hub(a))
        .filter_map(|a| {
            let u = url::Url::parse(&a).ok()?;
            Some((u.scheme().to_string(), u.host_str()?.to_string()))
        })
        .collect();
    let mut rows: Vec<(String, Vec<String>)> = Vec::new();
    if link.is_none() {
        rows.push((via.clone(), vec![via.clone()]));
    }
    for (i, h) in hubs.iter().enumerate() {
        if rows.iter().any(|(t, _)| t == h) {
            continue;
        }
        let mut c: Vec<String> = Vec::new();
        let mut add = |a: String| {
            if !c.contains(&a) {
                c.push(a);
            }
        };
        if Some(i) == link {
            add(via.clone());
            for a in aliases.iter().filter(|a| !is_loopback_hub(a)) {
                add(a.clone());
            }
        } else if is_loopback_hub(h) {
            if let Some(port) = url::Url::parse(h).ok().and_then(|u| u.port()) {
                for (scheme, host) in &hosts {
                    if let Ok(a) = crate::HubAddress::parse(&format!("{scheme}://{host}:{port}")) {
                        add(a.to_string());
                    }
                }
            }
        }
        add(h.clone());
        rows.push((h.clone(), c));
    }
    rows
}

/// What a hub's /healthz answers with: name, version, how many addresses it
/// holds. Two addresses answering alike at the same moment lead to one hub.
pub type HubKey = (String, Option<String>, Option<u64>);

fn same_hub(a: &HubKey, b: &HubKey) -> bool {
    a.0 == b.0 && a.1 == b.1 && (a.2.is_none() || b.2.is_none() || a.2 == b.2)
}

/// One review row per hub (rows from review_hubs, each with the key of the
/// hub that answered, None when nothing did). A row that answers as a hub an
/// earlier row already reached is that hub under another name (a PC's
/// localhost hub as its phone knows it) and merges into the earlier row:
/// its candidates are added there, and an earlier row made for `via` alone
/// (the bundle doesn't name it) takes its name for the hub as the other
/// device knows it. Returns (index of the kept row, theirs, candidates).
pub fn merge_same_hubs(
    rows: &[(String, Vec<String>, Option<HubKey>)],
    bundle_hubs: &[String],
    via: &str,
) -> Vec<(usize, String, Vec<String>)> {
    let norm = |h: &str| {
        crate::HubAddress::parse(h)
            .map(|a| a.to_string())
            .unwrap_or_else(|_| h.to_string())
    };
    let via = norm(via);
    let made_for_via = !bundle_hubs.iter().any(|h| norm(h) == via);
    let mut out: Vec<(usize, String, Vec<String>)> = Vec::new();
    for (i, (theirs, candidates, key)) in rows.iter().enumerate() {
        let earlier = key.as_ref().and_then(|k| {
            out.iter()
                .position(|(j, _, _)| rows[*j].2.as_ref().is_some_and(|o| same_hub(o, k)))
        });
        match earlier {
            Some(e) => {
                let kept = &mut out[e];
                if made_for_via && kept.1 == via {
                    kept.1 = theirs.clone();
                }
                for c in candidates {
                    if !kept.2.contains(c) {
                        kept.2.push(c.clone());
                    }
                }
            }
            None => out.push((i, theirs.clone(), candidates.clone())),
        }
    }
    out
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
            recovery_saved: false,
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

    #[test]
    fn link_url_round_trip_with_several_hubs() {
        let code = normalize_code(&new_link_code()).unwrap();
        let l = LinkUrl {
            code: code.clone(),
            hubs: vec!["http://home-pc:7370".into(), "http://100.101.102.103:7370".into()],
            hub_name: Some("maurdekye net".into()),
            role: Some("give".into()),
        };
        let url = l.to_url();
        assert!(url.starts_with("hubchat://link?code="));
        assert!(url.contains(&format_code(&code)));
        assert_eq!(LinkUrl::parse(&url).unwrap(), l);
        // A typed code, and the older text form.
        let typed = LinkUrl::parse(&format_code(&code).to_lowercase()).unwrap();
        assert_eq!((typed.code, typed.hubs.len(), typed.role), (code.clone(), 0, None));
        let old = LinkUrl::parse(&format!("hubchat-link:{code}@http://hub:7370")).unwrap();
        assert_eq!(old.hubs, vec!["http://hub:7370".to_string()]);
        assert_eq!(old.role.as_deref(), Some("take"));
        // An unknown role is no role; a link without a code is refused.
        let odd = LinkUrl::parse(&format!("hubchat://link?code={code}&role=steal")).unwrap();
        assert_eq!(odd.role, None);
        assert!(LinkUrl::parse("hubchat://link?hub=x").is_err());
    }

    #[test]
    fn loopback_hubs() {
        for h in [
            "localhost:7370",
            "http://localhost:7370",
            "127.0.0.1:7397",
            "http://127.8.0.1",
            "[::1]:7370",
            "0.0.0.0:7370",
            "LOCALHOST",
        ] {
            assert!(is_loopback_hub(h), "{h}");
        }
        for h in [
            "home-pc:7370",
            "100.101.102.103:7370",
            "https://hub.example.org",
            "192.168.1.20:7370",
            "not a hub ::",
        ] {
            assert!(!is_loopback_hub(h), "{h}");
        }
    }

    #[test]
    fn aliases_for_a_loopback_hub() {
        let ips: Vec<std::net::IpAddr> = vec![
            "100.101.102.103".parse().unwrap(),
            "192.168.1.20".parse().unwrap(),
            "127.0.0.1".parse().unwrap(),
            "100.101.102.103".parse().unwrap(),
        ];
        assert_eq!(
            hub_aliases("http://localhost:7370", Some("HOME-PC"), &ips),
            vec![
                "http://home-pc:7370",
                "http://100.101.102.103:7370",
                "http://192.168.1.20:7370",
                "http://localhost:7370",
            ]
        );
        // Another hub is offered as it is; no hostname or IPs: just itself.
        assert_eq!(
            hub_aliases("star-system:7370", Some("home-pc"), &ips),
            vec!["http://star-system:7370"]
        );
        assert_eq!(
            hub_aliases("127.0.0.1:7397", None, &[]),
            vec!["http://127.0.0.1:7397"]
        );
    }

    #[test]
    fn hubs_after_a_link() {
        let bundle = vec![
            "http://localhost:7370".to_string(),
            "http://star-system:7370".to_string(),
        ];
        // The phone reached the PC's localhost hub through Tailscale.
        assert_eq!(
            hubs_for_device(
                &bundle,
                Some("home-pc:7370"),
                &["http://home-pc:7370".into(), "http://localhost:7370".into()]
            ),
            vec!["http://home-pc:7370", "http://star-system:7370"]
        );
        // The user typed another address for a hub the bundle names otherwise.
        assert_eq!(
            hubs_for_device(
                &["http://hub.lan:7370".to_string()],
                Some("100.1.2.3:7370"),
                &["http://hub.lan:7370".into()]
            ),
            vec!["http://100.1.2.3:7370"]
        );
        // Same machine (tests): the loopback hub is the one it reached.
        assert_eq!(
            hubs_for_device(
                &["http://127.0.0.1:7397".to_string()],
                Some("http://127.0.0.1:7397"),
                &[]
            ),
            vec!["http://127.0.0.1:7397"]
        );
        // No link (QR, key file): the bundle as it is.
        assert_eq!(hubs_for_device(&bundle, None, &[]), bundle);
    }

    #[test]
    fn hub_review_rows() {
        let aliases: Vec<String> = vec![
            "http://home-pc:7370".into(),
            "http://100.101.102.103:7370".into(),
            "http://localhost:7370".into(),
        ];
        // The user's PC: its hub as localhost, a second local hub, a remote one.
        let rows = review_hubs(
            &["http://localhost:7370".into(), "localhost:7380".into(), "star-system:7370".into()],
            "home-pc:7370",
            &aliases,
        );
        assert_eq!(
            rows,
            vec![
                (
                    "http://localhost:7370".to_string(),
                    vec!["http://home-pc:7370".to_string(), "http://100.101.102.103:7370".into(), "http://localhost:7370".into()]
                ),
                (
                    "http://localhost:7380".to_string(),
                    vec!["http://home-pc:7380".to_string(), "http://100.101.102.103:7380".into(), "http://localhost:7380".into()]
                ),
                ("http://star-system:7370".to_string(), vec!["http://star-system:7370".to_string()]),
            ]
        );
        // A typed address for a hub the bundle names otherwise (no loopback).
        let rows = review_hubs(&["http://hub.lan:7370".into()], "100.1.2.3:7370", &["http://hub.lan:7370".into()]);
        assert_eq!(
            rows,
            vec![("http://hub.lan:7370".to_string(), vec!["http://100.1.2.3:7370".to_string(), "http://hub.lan:7370".into()])]
        );
        // This device made the code (no aliases): its hub is the PC's localhost one.
        let rows = review_hubs(&["http://localhost:7370".into()], "home-pc:7370", &[]);
        assert_eq!(rows[0].1[0], "http://home-pc:7370");
        assert_eq!(rows.len(), 1);
        // Nothing in the bundle matches: the reached hub gets its own row, first.
        let rows = review_hubs(&["http://elsewhere:7370".into()], "home-pc:7370", &[]);
        assert_eq!(rows[0], ("http://home-pc:7370".to_string(), vec!["http://home-pc:7370".to_string()]));
        assert_eq!(rows.len(), 2);
    }

    #[test]
    fn one_row_per_hub() {
        let key = |n: &str, orgs: u64| Some((n.to_string(), Some("2.0.0".to_string()), Some(orgs)));
        // The PC re-linked from its phone: it joined through localhost, the
        // phone's bundle names the same hub as home-pc.
        let bundle = vec!["http://home-pc:7370".to_string()];
        let rows = review_hubs(&bundle, "localhost:7370", &[]);
        assert_eq!(rows.len(), 2);
        let probed: Vec<_> = rows
            .into_iter()
            .map(|(t, c)| (t, c, key("maurdekye-net", 16)))
            .collect();
        assert_eq!(
            merge_same_hubs(&probed, &bundle, "localhost:7370"),
            vec![(
                0,
                "http://home-pc:7370".to_string(),
                vec!["http://localhost:7370".to_string(), "http://home-pc:7370".into()]
            )]
        );
        // Two hubs on one machine with the same name but other address
        // counts, and one that didn't answer: three rows stay.
        let probed = vec![
            ("http://a:7370".to_string(), vec!["http://a:7370".to_string()], key("hub", 3)),
            ("http://a:7380".to_string(), vec!["http://a:7380".to_string()], key("hub", 9)),
            ("http://b:7370".to_string(), vec!["http://b:7370".to_string()], None),
        ];
        assert_eq!(merge_same_hubs(&probed, &probed.iter().map(|r| r.0.clone()).collect::<Vec<_>>(), "a:7370").len(), 3);
        // A bundle naming one hub twice: the later row folds into the first,
        // which keeps its own name (it isn't a row made for via).
        let probed = vec![
            ("http://localhost:7370".to_string(), vec!["http://home-pc:7370".to_string()], key("h", 2)),
            ("http://home-pc:7370".to_string(), vec!["http://home-pc:7370".to_string()], key("h", 2)),
        ];
        let bundle: Vec<String> = probed.iter().map(|r| r.0.clone()).collect();
        assert_eq!(
            merge_same_hubs(&probed, &bundle, "home-pc:7370"),
            vec![(0, "http://localhost:7370".to_string(), vec!["http://home-pc:7370".to_string()])]
        );
    }

    #[test]
    fn recovery_saved_travels_with_the_bundle() {
        let mut b = bundle();
        // not saved: the field isn't written (older readers see what they knew)
        assert!(!serde_json::to_string(&b).unwrap().contains("recovery_saved"));
        b.recovery_saved = true;
        let code = new_link_code();
        assert!(open_from_link(&code, &seal_for_link(&code, &b).unwrap()).unwrap().recovery_saved);
        assert!(from_qr(&to_qr(&b)).unwrap().recovery_saved);
        // a bundle from before the field: not saved
        let old = r#"{"id":"alex","secret":"s","name":"Alex","hubs":[]}"#;
        assert!(!serde_json::from_str::<Bundle>(old).unwrap().recovery_saved);
    }
}
