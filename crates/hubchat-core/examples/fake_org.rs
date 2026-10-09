//! A stand-in for Orgtree's side of Scan setup code, to run the linking flow
//! on a phone or emulator without the real Orgtree (hubchat-opus 14:52Z).
//!
//! cargo run -p hubchat-core --example fake_org -- --hub <url>
//!     [--orgname "Test Org"] [--pc Home-PC] [--net wifi|tailscale]
//!     [--ts alex@example.com] [--answer linked|expired|none]
//!
//! It registers a fresh org on the hub, prints `SETUP-URL hubchat://setup?...`
//! built as Orgtree builds it, waits for a message whose last non-empty line
//! is `Setup code: <code>` (matched as Orgtree matches it: case-insensitive,
//! dash optional), prints `GOT <sender> <first line>`, answers from the org
//! with the agreed last line, prints `ANSWERED <word>` and exits 0. After 5
//! minutes without the code it prints `TIMEOUT` and exits 1.

use std::time::{Duration, Instant};

use hubchat_core::hub::{Envelope, Outgoing, Profile};
use hubchat_core::hub_v2::Change;
use hubchat_core::setup::normalize_code;
use hubchat_core::{HubAddress, HubClient, Identity};
use rand::Rng;

const WAIT: Duration = Duration::from_secs(300);

fn arg(args: &[String], name: &str, default: &str) -> String {
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1))
        .cloned()
        .unwrap_or_else(|| default.to_string())
}

/// Percent-encoded, spaces as %20 (form encoding writes `+`, a literal `+`
/// becomes %2B, so swapping is safe).
fn enc(s: &str) -> String {
    url::form_urlencoded::byte_serialize(s.as_bytes())
        .collect::<String>()
        .replace('+', "%20")
}

/// A code like Orgtree's: 8 letters and digits without look-alikes, XXXX-XXXX.
fn new_code() -> String {
    const ABC: &[u8] = b"ABCDEFGHJKMNPQRSTUVWXYZ23456789";
    let mut r = rand::thread_rng();
    let s: String = (0..8).map(|_| ABC[r.gen_range(0..ABC.len())] as char).collect();
    format!("{}-{}", &s[..4], &s[4..])
}

/// Orgtree's match: the last non-empty line is `Setup code: <code>`.
fn names_code(body: &str, code: &str) -> bool {
    let Some(last) = body.lines().map(str::trim).rfind(|l| !l.is_empty()) else {
        return false;
    };
    let low = last.to_ascii_lowercase();
    low.strip_prefix("setup code:")
        .and_then(|rest| normalize_code(rest.trim()))
        .is_some_and(|c| c == code)
}

/// Messages to the org since the last call (sync on v2 hubs, poll on v1).
async fn next_messages(c: &HubClient, me: &Identity, sync: bool, cursor: &mut Option<String>) -> Vec<Envelope> {
    if sync {
        let Ok(r) = c.sync(me, "fake-org", "Fake org", cursor.as_deref(), 5).await else {
            tokio::time::sleep(Duration::from_secs(1)).await;
            return Vec::new();
        };
        *cursor = Some(r.cursor);
        return r
            .changes
            .into_iter()
            .filter_map(|ch| match ch {
                Change::Message(m) => Some(m.env),
                _ => None,
            })
            .filter(|m| m.from != me.address())
            .collect();
    }
    let Ok(p) = c.poll(me, 5).await else {
        tokio::time::sleep(Duration::from_secs(1)).await;
        return Vec::new();
    };
    let ids: Vec<String> = p.messages.iter().map(|m| m.id.clone()).collect();
    if !ids.is_empty() {
        let _ = c.ack(me, &ids).await;
    }
    p.messages
}

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let hub = arg(&args, "--hub", "");
    let Ok(hub) = HubAddress::parse(&hub) else {
        eprintln!("usage: fake_org --hub <url> [--orgname N] [--pc N] [--net wifi|tailscale] [--ts A] [--answer linked|expired|none]");
        std::process::exit(2);
    };
    let orgname = arg(&args, "--orgname", "Test Org");
    let pc = arg(&args, "--pc", "Home-PC");
    let net = arg(&args, "--net", "tailscale");
    let ts = arg(&args, "--ts", "alex@example.com");
    let answer = arg(&args, "--answer", "linked");
    if !matches!(net.as_str(), "wifi" | "tailscale") || !matches!(answer.as_str(), "linked" | "expired" | "none") {
        eprintln!("--net is wifi or tailscale; --answer is linked, expired or none");
        std::process::exit(2);
    }

    let c = HubClient::new(hub.clone());
    let health = c.healthz().await.unwrap_or_else(|e| {
        eprintln!("the hub doesn't answer: {e}");
        std::process::exit(1);
    });
    let me = Identity::generate("test-org").expect("identity");
    let profile = Profile {
        kind: "org".into(),
        org_name: orgname.clone(),
        username: "test-org".into(),
        blurb: String::new(),
    };
    if let Err(e) = c.register(&me, &profile).await {
        eprintln!("couldn't register on the hub: {e}");
        std::process::exit(1);
    }

    let code = new_code();
    let mut url = format!(
        "hubchat://setup?v=1&hub={}&org={}&orgname={}&pc={}",
        enc(hub.as_str()),
        enc(&me.address()),
        enc(&orgname),
        enc(&pc)
    );
    if net == "tailscale" {
        url += &format!("&ts={}", enc(&ts));
    }
    url += &format!("&code={code}&net={net}&hubname={}", enc(&health.name));
    println!("SETUP-URL {url}");

    let sync = health.supports("sync");
    let mut cursor = None;
    let t0 = Instant::now();
    while t0.elapsed() < WAIT {
        for m in next_messages(&c, &me, sync, &mut cursor).await {
            if !names_code(&m.body, &code) {
                continue;
            }
            let first = m.body.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or("");
            println!("GOT {} {first}", m.from);
            if answer != "none" {
                let text = if answer == "linked" {
                    "Welcome! I know this address is you now."
                } else {
                    "That code didn't work."
                };
                let body = format!("{text}\n\nSetup code: {code} {answer}");
                if let Err(e) = c.send(&me, &Outgoing::new(&m.from, &body)).await {
                    eprintln!("couldn't answer: {e}");
                    std::process::exit(1);
                }
            }
            println!("ANSWERED {answer}");
            return;
        }
    }
    println!("TIMEOUT");
    std::process::exit(1);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_the_code_as_orgtree_does() {
        assert!(names_code("Hi\n\nSetup code: K7QD-4MXP\n\n", "K7QD-4MXP"));
        assert!(names_code("setup CODE: k7qd4mxp", "K7QD-4MXP"));
        assert!(!names_code("Setup code: K7QD-4MXP\nthanks", "K7QD-4MXP"));
        assert!(!names_code("Setup code: AAAA-BBBB", "K7QD-4MXP"));
    }

    #[test]
    fn encodes_like_a_url() {
        assert_eq!(enc("Test Org & Co+"), "Test%20Org%20%26%20Co%2B");
        assert_eq!(normalize_code(&new_code()).is_some(), true);
    }
}
