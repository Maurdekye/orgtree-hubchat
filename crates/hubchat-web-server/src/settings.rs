//! The server's settings, all from the environment (spec R6.2). A setting
//! that makes no sense refuses to start, with a sentence that names it:
//! a typo must not quietly become the default.
use std::net::IpAddr;
use std::path::PathBuf;

/// At most this many hubs are suggested (the page offers no more either).
pub const MAX_HUBS: usize = 8;
/// The label on the welcome screen is at most this long.
pub const MAX_NAME: usize = 48;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settings {
    pub port: u16,
    pub bind: IpAddr,
    /// certificate and key (PEM files): with both the server speaks HTTPS itself
    pub tls: Option<(PathBuf, PathBuf)>,
    /// HUBCHAT_WEB_SUGGEST_HUBS, each as `https://host[:port]`; empty when suggesting is off
    pub suggested_hubs: Vec<String>,
    /// HUBCHAT_WEB_HUB_SUGGESTIONS
    pub hub_suggestions: bool,
    pub name: String,
    /// one line per request
    pub log_requests: bool,
    /// what the operator should know at the start (a list that is ignored)
    pub notes: Vec<String>,
}

fn loopback(host: &str) -> bool {
    matches!(host, "localhost" | "127.0.0.1" | "[::1]")
}

/// One entry of HUBCHAT_WEB_SUGGEST_HUBS as the address the page checks:
/// `https://host[:port]`, or a bare `host[:port]`, taken as https. `http://`
/// only for this machine (localhost, 127.0.0.1, [::1]): a page served over
/// https can call nothing else over http.
pub fn hub_address(entry: &str) -> Result<String, String> {
    let bad = |why: &str| Err(format!("\"{entry}\" is not a hub address ({why}). Write https://host or https://host:port, or just the host."));
    let t = entry.trim().trim_end_matches('/');
    let (scheme, rest) = match t.split_once("://") {
        Some((s, rest)) => (s.to_ascii_lowercase(), rest),
        None => ("https".to_string(), t),
    };
    if scheme != "https" && scheme != "http" {
        return bad("only https");
    }
    if rest.is_empty() {
        return bad("no host");
    }
    if rest.contains(['/', '?', '#', '@', '\\']) || rest.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return bad("only a host and a port, no path");
    }
    let (host, port) = if let Some(end) = rest.strip_prefix('[').and_then(|_| rest.find(']')) {
        // [IPv6] or [IPv6]:port
        let (h, after) = rest.split_at(end + 1);
        if h[1..end].parse::<std::net::Ipv6Addr>().is_err() {
            return bad("the address in [ ] is not IPv6");
        }
        match after.strip_prefix(':') {
            Some(p) => (h, Some(p)),
            None if after.is_empty() => (h, None),
            None => return bad("something follows ]"),
        }
    } else {
        match rest.rsplit_once(':') {
            Some((h, p)) => (h, Some(p)),
            None => (rest, None),
        }
    };
    let host = host.to_ascii_lowercase();
    if !host.starts_with('[') {
        let label = |l: &str| !l.is_empty() && l.len() <= 63 && !l.starts_with('-') && !l.ends_with('-') && l.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-');
        if host.len() > 253 || !host.split('.').all(label) {
            return bad("the host has a character that is not a letter, a digit, - or .");
        }
    }
    let port = match port {
        None => None,
        Some(p) => match p.parse::<u16>() {
            Ok(n) if n > 0 && p.bytes().all(|b| b.is_ascii_digit()) => Some(n),
            _ => return bad("the port is not a number from 1 to 65535"),
        },
    };
    if scheme == "http" && !loopback(&host) {
        return bad("http only for a hub on this machine; a hub elsewhere needs https");
    }
    let default_port = if scheme == "https" { 443 } else { 80 };
    Ok(match port {
        Some(p) if p != default_port => format!("{scheme}://{host}:{p}"),
        _ => format!("{scheme}://{host}"),
    })
}

fn boolean(name: &str, value: &str) -> Result<bool, String> {
    match value {
        "true" | "1" => Ok(true),
        "false" | "0" => Ok(false),
        other => Err(format!("{name}=\"{other}\": write true or false.")),
    }
}

impl Settings {
    /// From the environment; `get` answers one variable.
    pub fn from_env(get: impl Fn(&str) -> Option<String>) -> Result<Settings, String> {
        // set but empty counts as not set
        let var = |name: &str| get(name).map(|v| v.trim().to_string()).filter(|v| !v.is_empty());
        let mut notes = Vec::new();

        let port = match var("HUBCHAT_WEB_PORT") {
            None => 8080,
            Some(p) => match p.parse::<u16>() {
                Ok(n) if n > 0 => n,
                _ => return Err(format!("HUBCHAT_WEB_PORT=\"{p}\": write a port from 1 to 65535.")),
            },
        };
        let bind = match var("HUBCHAT_WEB_BIND") {
            None => IpAddr::from([0, 0, 0, 0]),
            Some(b) => b.parse::<IpAddr>().map_err(|_| format!("HUBCHAT_WEB_BIND=\"{b}\": write an IP address, for example 0.0.0.0 or 127.0.0.1."))?,
        };
        let tls = match (var("HUBCHAT_WEB_TLS_CERT"), var("HUBCHAT_WEB_TLS_KEY")) {
            (Some(c), Some(k)) => Some((PathBuf::from(c), PathBuf::from(k))),
            (None, None) => None,
            (Some(_), None) => return Err("HUBCHAT_WEB_TLS_CERT is set but HUBCHAT_WEB_TLS_KEY is not: HTTPS needs both, plain HTTP neither.".into()),
            (None, Some(_)) => return Err("HUBCHAT_WEB_TLS_KEY is set but HUBCHAT_WEB_TLS_CERT is not: HTTPS needs both, plain HTTP neither.".into()),
        };
        let hub_suggestions = match var("HUBCHAT_WEB_HUB_SUGGESTIONS") {
            None => true,
            Some(v) => boolean("HUBCHAT_WEB_HUB_SUGGESTIONS", &v)?,
        };
        let mut suggested_hubs = Vec::new();
        if let Some(list) = var("HUBCHAT_WEB_SUGGEST_HUBS") {
            if hub_suggestions {
                for entry in list.split(',').map(str::trim).filter(|e| !e.is_empty()) {
                    let address = hub_address(entry).map_err(|e| format!("HUBCHAT_WEB_SUGGEST_HUBS: {e}"))?;
                    // a hub listed twice is offered once
                    if !suggested_hubs.contains(&address) {
                        suggested_hubs.push(address);
                    }
                }
                if suggested_hubs.len() > MAX_HUBS {
                    return Err(format!("HUBCHAT_WEB_SUGGEST_HUBS lists {} hubs: at most {MAX_HUBS}.", suggested_hubs.len()));
                }
            } else {
                notes.push("HUBCHAT_WEB_HUB_SUGGESTIONS is false: HUBCHAT_WEB_SUGGEST_HUBS is ignored, no hub is suggested.".to_string());
            }
        }
        let name = var("HUBCHAT_WEB_NAME").unwrap_or_default();
        if name.chars().count() > MAX_NAME {
            return Err(format!("HUBCHAT_WEB_NAME is {} characters long: at most {MAX_NAME}.", name.chars().count()));
        }
        if name.chars().any(char::is_control) {
            return Err("HUBCHAT_WEB_NAME holds a control character (a line break or a tab): write one plain line.".into());
        }
        let log_requests = match var("HUBCHAT_WEB_LOG").as_deref() {
            None | Some("requests") => true,
            Some("off") => false,
            Some(other) => return Err(format!("HUBCHAT_WEB_LOG=\"{other}\": write requests or off.")),
        };
        Ok(Settings { port, bind, tls, suggested_hubs, hub_suggestions, name, log_requests, notes })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env(pairs: &[(&str, &str)]) -> Result<Settings, String> {
        let pairs: Vec<(String, String)> = pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
        Settings::from_env(move |name| pairs.iter().find(|(k, _)| k == name).map(|(_, v)| v.clone()))
    }

    #[test]
    fn nothing_set_is_the_defaults() {
        let s = env(&[]).unwrap();
        assert_eq!(s.port, 8080);
        assert_eq!(s.bind.to_string(), "0.0.0.0");
        assert_eq!(s.tls, None);
        assert!(s.suggested_hubs.is_empty());
        assert!(s.hub_suggestions);
        assert_eq!(s.name, "");
        assert!(s.log_requests);
        assert!(s.notes.is_empty());
        // set but empty is the same
        assert_eq!(env(&[("HUBCHAT_WEB_PORT", ""), ("HUBCHAT_WEB_SUGGEST_HUBS", " "), ("HUBCHAT_WEB_HUB_SUGGESTIONS", "")]).unwrap(), s);
    }

    #[test]
    fn a_hub_address_is_https_host_and_port() {
        assert_eq!(hub_address("https://hub.example").unwrap(), "https://hub.example");
        assert_eq!(hub_address("hub.example").unwrap(), "https://hub.example");
        assert_eq!(hub_address(" Hub.Example:8443/ ").unwrap(), "https://hub.example:8443");
        assert_eq!(hub_address("HTTPS://hub.example:443").unwrap(), "https://hub.example");
        assert_eq!(hub_address("pc.tail1234.ts.net:9444").unwrap(), "https://pc.tail1234.ts.net:9444");
        assert_eq!(hub_address("[fd7a::1]:7378").unwrap(), "https://[fd7a::1]:7378");
        assert_eq!(hub_address("100.64.0.7").unwrap(), "https://100.64.0.7");
        // http only on this machine
        assert_eq!(hub_address("http://127.0.0.1:7378").unwrap(), "http://127.0.0.1:7378");
        assert_eq!(hub_address("http://localhost").unwrap(), "http://localhost");
        for bad in ["http://hub.example", "ftp://hub.example", "https://", "https://hub.example/path", "https://user@hub.example", "hub.example:0", "hub.example:99999", "hub.example:port", "hub exam.ple", "hub_example", "-hub.example", "hub..example", "[zz]:1", "[::1]x", "https://hub.example?x=1"] {
            let e = hub_address(bad).expect_err(bad);
            assert!(e.contains(bad.trim()) && e.contains("is not a hub address"), "{e}");
        }
    }

    #[test]
    fn the_list_of_hubs() {
        let s = env(&[("HUBCHAT_WEB_SUGGEST_HUBS", "https://one.example, two.example:8443 ,,https://ONE.example/")]).unwrap();
        assert_eq!(s.suggested_hubs, ["https://one.example", "https://two.example:8443"], "in the listed order, a repeat once");
        assert!(s.hub_suggestions);
        let e = env(&[("HUBCHAT_WEB_SUGGEST_HUBS", "one.example,not a hub")]).unwrap_err();
        assert!(e.starts_with("HUBCHAT_WEB_SUGGEST_HUBS: \"not a hub\" is not a hub address"), "{e}");
        let eight = (1..=8).map(|n| format!("h{n}.example")).collect::<Vec<_>>().join(",");
        assert_eq!(env(&[("HUBCHAT_WEB_SUGGEST_HUBS", &eight)]).unwrap().suggested_hubs.len(), 8);
        let e = env(&[("HUBCHAT_WEB_SUGGEST_HUBS", &format!("{eight},h9.example"))]).unwrap_err();
        assert_eq!(e, "HUBCHAT_WEB_SUGGEST_HUBS lists 9 hubs: at most 8.");
    }

    #[test]
    fn suggestions_off_wins_over_the_list() {
        for off in ["false", "0"] {
            // even a list that would not parse: it is not read at all
            let s = env(&[("HUBCHAT_WEB_HUB_SUGGESTIONS", off), ("HUBCHAT_WEB_SUGGEST_HUBS", "one.example,not a hub")]).unwrap();
            assert!(!s.hub_suggestions);
            assert!(s.suggested_hubs.is_empty());
            assert_eq!(s.notes, ["HUBCHAT_WEB_HUB_SUGGESTIONS is false: HUBCHAT_WEB_SUGGEST_HUBS is ignored, no hub is suggested."]);
        }
        assert!(env(&[("HUBCHAT_WEB_HUB_SUGGESTIONS", "false")]).unwrap().notes.is_empty());
        for on in ["true", "1"] {
            assert!(env(&[("HUBCHAT_WEB_HUB_SUGGESTIONS", on)]).unwrap().hub_suggestions);
        }
        for bad in ["yes", "no", "TRUE", "off", "2"] {
            let e = env(&[("HUBCHAT_WEB_HUB_SUGGESTIONS", bad)]).unwrap_err();
            assert_eq!(e, format!("HUBCHAT_WEB_HUB_SUGGESTIONS=\"{bad}\": write true or false."));
        }
    }

    #[test]
    fn a_bad_setting_refuses_with_its_name() {
        for (name, value) in [("HUBCHAT_WEB_PORT", "0"), ("HUBCHAT_WEB_PORT", "70000"), ("HUBCHAT_WEB_PORT", "http"), ("HUBCHAT_WEB_BIND", "localhost"), ("HUBCHAT_WEB_BIND", "0.0.0"), ("HUBCHAT_WEB_LOG", "verbose")] {
            let e = env(&[(name, value)]).unwrap_err();
            assert!(e.starts_with(&format!("{name}=\"{value}\"")), "{e}");
        }
        assert!(env(&[("HUBCHAT_WEB_TLS_CERT", "c.pem")]).unwrap_err().starts_with("HUBCHAT_WEB_TLS_CERT is set but HUBCHAT_WEB_TLS_KEY is not"));
        assert!(env(&[("HUBCHAT_WEB_TLS_KEY", "k.pem")]).unwrap_err().starts_with("HUBCHAT_WEB_TLS_KEY is set but HUBCHAT_WEB_TLS_CERT is not"));
        let s = env(&[("HUBCHAT_WEB_TLS_CERT", "c.pem"), ("HUBCHAT_WEB_TLS_KEY", "k.pem"), ("HUBCHAT_WEB_PORT", "8443"), ("HUBCHAT_WEB_BIND", "::1"), ("HUBCHAT_WEB_LOG", "off")]).unwrap();
        assert_eq!(s.tls, Some((PathBuf::from("c.pem"), PathBuf::from("k.pem"))));
        assert_eq!((s.port, s.bind.to_string(), s.log_requests), (8443, "::1".to_string(), false));
    }

    #[test]
    fn the_name_is_one_short_line() {
        assert_eq!(env(&[("HUBCHAT_WEB_NAME", "  Ada's family  ")]).unwrap().name, "Ada's family");
        let long = "é".repeat(48);
        assert_eq!(env(&[("HUBCHAT_WEB_NAME", &long)]).unwrap().name, long, "48 characters, not bytes");
        assert_eq!(env(&[("HUBCHAT_WEB_NAME", &"x".repeat(49))]).unwrap_err(), "HUBCHAT_WEB_NAME is 49 characters long: at most 48.");
        assert!(env(&[("HUBCHAT_WEB_NAME", "two\nlines")]).unwrap_err().starts_with("HUBCHAT_WEB_NAME holds a control character"));
    }
}
