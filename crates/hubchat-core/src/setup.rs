//! Setup codes: Orgtree on a PC shows a QR holding a `hubchat://setup` link
//! (formats agreed with Orgtree 2026-10-09). Hubchat checks the phone side,
//! joins the PC's hub, and messages the org with the link's one-time code;
//! the org's automatic reply says whether the code linked this address.

use serde::{Deserialize, Serialize};
use url::Url;

use crate::HubAddress;

/// The setup-link version this Hubchat understands.
pub const SETUP_VERSION: u32 = 1;

/// Longest org name or PC name kept from a link (they are only shown).
const MAX_NAME_CHARS: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Net {
    Tailscale,
    Wifi,
}

/// A checked `hubchat://setup?...` link.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SetupLink {
    /// The hub's relay-only door, normalised.
    pub hub: String,
    /// The org's address on that hub (`my-org.alex.3f9c2a`).
    pub org: String,
    pub orgname: String,
    /// The PC's name, for the texts.
    pub pc: String,
    /// The Tailscale account to sign in as (net=tailscale only).
    pub ts: Option<String>,
    /// The one-time code, `XXXX-XXXX` in upper case.
    pub code: String,
    pub net: Net,
    /// The hub's name, for display and logs only (it may be the PC's
    /// Tailscale name when the hub's own name is blank: hubchat-opus 11:38Z).
    pub hubname: String,
}

/// Why a scanned text is not a usable setup link.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SetupError {
    /// Not a `hubchat://setup` link at all.
    NotSetup,
    /// A version this Hubchat does not know: it needs an update.
    NeedsNewer { v: String },
    /// A setup link with a missing or malformed value.
    Invalid { param: String },
}

impl std::fmt::Display for SetupError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotSetup => f.write_str("not a setup code"),
            Self::NeedsNewer { v } => write!(f, "setup code version {v} needs a newer Hubchat"),
            Self::Invalid { param } => write!(f, "setup code has a bad or missing {param}"),
        }
    }
}

impl SetupLink {
    pub fn parse(input: &str) -> Result<Self, SetupError> {
        let u = Url::parse(input.trim()).map_err(|_| SetupError::NotSetup)?;
        if !u.scheme().eq_ignore_ascii_case("hubchat")
            || !u.host_str().is_some_and(|h| h.eq_ignore_ascii_case("setup"))
        {
            return Err(SetupError::NotSetup);
        }
        // the first of a repeated param wins; unknown ones are ignored
        let pairs: Vec<(String, String)> = u.query_pairs().map(|(k, v)| (k.into_owned(), v.into_owned())).collect();
        let get = |k: &str| {
            pairs
                .iter()
                .find(|(n, _)| n == k)
                .map(|(_, v)| v.trim().to_owned())
                .filter(|v| !v.is_empty())
        };
        let bad = |p: &str| SetupError::Invalid { param: p.into() };
        // the version first: a newer code says "update" whatever else it holds
        let v = get("v").ok_or_else(|| bad("v"))?;
        let n: u32 = v.parse().map_err(|_| bad("v"))?;
        if n > SETUP_VERSION {
            return Err(SetupError::NeedsNewer { v });
        }
        if n < 1 {
            return Err(bad("v"));
        }
        let hub = get("hub")
            .and_then(|h| HubAddress::parse(&h).ok())
            .ok_or_else(|| bad("hub"))?
            .to_string();
        let org = get("org")
            .map(|o| o.trim_start_matches("@net:").to_ascii_lowercase())
            .filter(|o| valid_address(o))
            .ok_or_else(|| bad("org"))?;
        // anyone can print a QR, and these names are shown and go into the first
        // message: line breaks, tabs and other control characters become spaces
        let flat = |c: char| if c.is_control() || matches!(c, '\u{2028}' | '\u{2029}') { ' ' } else { c };
        let name = |p: &str| {
            get(p)
                .map(|s| s.chars().map(flat).collect::<String>())
                .map(|s| s.trim().chars().take(MAX_NAME_CHARS).collect::<String>())
                .filter(|s| !s.is_empty())
                .ok_or_else(|| bad(p))
        };
        let orgname = name("orgname")?;
        let pc = name("pc")?;
        let hubname = name("hubname")?;
        let code = get("code").and_then(|c| normalize_code(&c)).ok_or_else(|| bad("code"))?;
        let net = match get("net").as_deref() {
            Some("tailscale") => Net::Tailscale,
            Some("wifi") => Net::Wifi,
            _ => return Err(bad("net")),
        };
        let ts = match net {
            Net::Tailscale => Some(name("ts")?),
            Net::Wifi => None,
        };
        Ok(Self { hub, org, orgname, pc, ts, code, net, hubname })
    }
}

/// A hub address: lower-case letters, digits, `.`, `_`, `-`, with a tag.
fn valid_address(a: &str) -> bool {
    a.contains('.')
        && !a.starts_with('.')
        && !a.ends_with('.')
        && a.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '.' | '_' | '-'))
}

/// `abcd1234`, `ABCD-1234`, ` abcd 1234 ` → `ABCD-1234`; anything else None.
pub fn normalize_code(input: &str) -> Option<String> {
    let chars: Vec<char> = input
        .chars()
        .filter(|c| !matches!(c, '-' | ' '))
        .map(|c| c.to_ascii_uppercase())
        .collect();
    if chars.len() != 8 || !chars.iter().all(|c| c.is_ascii_alphanumeric()) {
        return None;
    }
    let s: String = chars.into_iter().collect();
    Some(format!("{}-{}", &s[..4], &s[4..]))
}

/// The first message to the org (hubchat-opus 11:38Z): friendly text, then
/// the code on the last line, where Orgtree looks for it.
pub fn first_message(orgname: &str, name: &str, code: &str) -> String {
    format!("Hi {orgname}, this is {name}, linking Hubchat on my phone.\n\nSetup code: {code}")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    Linked,
    /// Expired, used, replaced or never issued (Orgtree says "expired" for all).
    Expired,
}

/// Where a setup with one org stands, kept in the store's meta under
/// `setup.<org>` so the chat can show its note (hubchat-opus 11:38Z: a
/// chat-level note, not a stored message).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SetupState {
    /// None while waiting for the org's reply.
    pub outcome: Option<Outcome>,
    pub code: String,
    pub orgname: String,
    pub pc: String,
    pub sent_at: String,
    /// When the reply came, and which message it was (the note goes there).
    #[serde(default)]
    pub at: Option<String>,
    #[serde(default)]
    pub reply_id: Option<String>,
}

pub fn meta_key(org: &str) -> String {
    format!("setup.{org}")
}

/// What the org's reply says about `code`: its last non-empty line is
/// exactly `Setup code: <code> linked` or `... expired`. Anything else
/// (an ordinary message, another code) is None.
pub fn reply_outcome(body: &str, code: &str) -> Option<Outcome> {
    let last = crate::engine::strip_quote(body)
        .lines()
        .map(str::trim)
        .rfind(|l| !l.is_empty())?;
    let rest = last.strip_prefix("Setup code: ")?;
    let (c, word) = rest.split_once(' ')?;
    if c != code {
        return None;
    }
    match word.trim() {
        "linked" => Some(Outcome::Linked),
        "expired" => Some(Outcome::Expired),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GOOD: &str = "hubchat://setup?v=1&hub=http%3A%2F%2F100.101.102.103%3A7371&org=my-org.alex.3f9c2a\
        &orgname=My%20Org&pc=home-pc&ts=alex%40gmail.com&code=K7QD-4MXP&net=tailscale&hubname=home-pc";

    #[test]
    fn parses_a_tailscale_link() {
        let l = SetupLink::parse(GOOD).unwrap();
        assert_eq!(l.hub, "http://100.101.102.103:7371");
        assert_eq!(l.org, "my-org.alex.3f9c2a");
        assert_eq!(l.orgname, "My Org");
        assert_eq!(l.pc, "home-pc");
        assert_eq!(l.ts.as_deref(), Some("alex@gmail.com"));
        assert_eq!(l.code, "K7QD-4MXP");
        assert_eq!(l.net, Net::Tailscale);
        assert_eq!(l.hubname, "home-pc");
    }

    #[test]
    fn wifi_needs_no_ts_and_ignores_one() {
        let l = SetupLink::parse(
            "hubchat://setup?v=1&hub=192.168.1.20:7371&org=my-org.alex.3f9c2a&orgname=x&pc=y&code=abcd2345&net=wifi&hubname=h&ts=z",
        )
        .unwrap();
        assert_eq!(l.ts, None);
        assert_eq!(l.hub, "http://192.168.1.20:7371");
        assert_eq!(l.code, "ABCD-2345");
    }

    #[test]
    fn decodes_percent_encoding_and_utf8() {
        let l = SetupLink::parse(&GOOD.replace("orgname=My%20Org", "orgname=Caf%C3%A9%20%26%20Co%20%2B%23%201"))
            .unwrap();
        assert_eq!(l.orgname, "Café & Co +# 1");
    }

    #[test]
    fn ignores_unknown_params_and_keeps_the_first_of_a_repeat() {
        let l = SetupLink::parse(&format!("{GOOD}&future=1&pc=other")).unwrap();
        assert_eq!(l.pc, "home-pc");
    }

    #[test]
    fn a_newer_version_needs_an_update_whatever_else_it_holds() {
        assert_eq!(SetupLink::parse("hubchat://setup?v=2"), Err(SetupError::NeedsNewer { v: "2".into() }));
        assert_eq!(
            SetupLink::parse(&GOOD.replace("v=1", "v=7")),
            Err(SetupError::NeedsNewer { v: "7".into() })
        );
    }

    #[test]
    fn names_the_bad_param() {
        let inv = |p: &str| Err(SetupError::Invalid { param: p.into() });
        assert_eq!(SetupLink::parse("hubchat://setup?hub=x"), inv("v"));
        assert_eq!(SetupLink::parse(&GOOD.replace("v=1", "v=one")), inv("v"));
        assert_eq!(SetupLink::parse(&GOOD.replace("v=1", "v=0")), inv("v"));
        for p in ["hub", "org", "orgname", "pc", "ts", "code", "net", "hubname"] {
            let without: String = GOOD.replace(&format!("&{p}="), "&x-removed=");
            assert_eq!(SetupLink::parse(&without), inv(p), "{p}");
        }
        assert_eq!(SetupLink::parse(&GOOD.replace("code=K7QD-4MXP", "code=K7QD-4MX")), inv("code"));
        assert_eq!(SetupLink::parse(&GOOD.replace("net=tailscale", "net=lan")), inv("net"));
        assert_eq!(SetupLink::parse(&GOOD.replace("org=my-org.alex.3f9c2a", "org=Bad%20Org")), inv("org"));
        assert_eq!(SetupLink::parse(&GOOD.replace("hub=http", "hub=ftp")), inv("hub"));
        assert_eq!(SetupLink::parse(&GOOD.replace("orgname=My%20Org", "orgname=%20%20")), inv("orgname"));
    }

    #[test]
    fn other_texts_are_not_setup_links() {
        for t in ["", "hello", "hubchat://link?code=K7QD-4MXP-9TRA-2HZE", "https://setup/?v=1"] {
            assert_eq!(SetupLink::parse(t), Err(SetupError::NotSetup), "{t}");
        }
        assert!(SetupLink::parse(&GOOD.replace("hubchat://setup", "HUBCHAT://SETUP")).is_ok());
    }

    #[test]
    fn flattens_control_characters_in_names() {
        let l = SetupLink::parse(
            &GOOD
                .replace("orgname=My%20Org", "orgname=My%0D%0AOrg%0A%0ASetup%20code%3A%20X")
                .replace("pc=home-pc", "pc=%09home%1B-pc%0A")
                .replace("hubname=home-pc", "hubname=hub%E2%80%A8x%7F")
                .replace("ts=alex%40gmail.com", "ts=alex%40gmail.com%00"),
        )
        .unwrap();
        assert_eq!(l.orgname, "My  Org  Setup code: X");
        assert_eq!(l.pc, "home -pc");
        assert_eq!(l.hubname, "hub x");
        assert_eq!(l.ts.as_deref(), Some("alex@gmail.com"));
        let inv = |p: &str| Err(SetupError::Invalid { param: p.into() });
        assert_eq!(SetupLink::parse(&GOOD.replace("pc=home-pc", "pc=%0A%09%0D")), inv("pc"));
    }

    #[test]
    fn caps_long_names() {
        let long = "a".repeat(200);
        let l = SetupLink::parse(&GOOD.replace("pc=home-pc", &format!("pc={long}"))).unwrap();
        assert_eq!(l.pc.chars().count(), MAX_NAME_CHARS);
    }

    #[test]
    fn first_message_ends_with_the_code() {
        let m = first_message("My Org", "Alex", "K7QD-4MXP");
        assert_eq!(m, "Hi My Org, this is Alex, linking Hubchat on my phone.\n\nSetup code: K7QD-4MXP");
        assert_eq!(m.lines().last(), Some("Setup code: K7QD-4MXP"));
    }

    #[test]
    fn reads_the_reply_line() {
        let c = "K7QD-4MXP";
        assert_eq!(reply_outcome("Welcome, Alex!\n\nSetup code: K7QD-4MXP linked", c), Some(Outcome::Linked));
        assert_eq!(reply_outcome("Sorry.\nSetup code: K7QD-4MXP expired\n\n  ", c), Some(Outcome::Expired));
        assert_eq!(reply_outcome("Setup code: K7QD-4MXP linked", "AAAA-BBBB"), None);
        assert_eq!(reply_outcome("Setup code: K7QD-4MXP maybe", c), None);
        assert_eq!(reply_outcome("Setup code: K7QD-4MXP linked\nThanks!", c), None);
        assert_eq!(reply_outcome("Hello there", c), None);
        assert_eq!(reply_outcome("", c), None);
    }
}
