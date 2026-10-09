//! The relay-only door of a hub this device reaches by a loopback address
//! (gap 5 of the linking design): another device reaches Orgtree's hub only
//! through its door, so a device-link QR made on the hub's own PC must name
//! the door's port, not 7370.
//!
//! Mail hub v2.0.1 says where its door listens (hubchat-opus 12:02Z): with
//! the feature "door", /healthz on the main port carries `door` {port,
//! bind}, or leaves it out when no door runs (phone access is off). Older
//! hubs say nothing, so the door is looked for: 7371 (Orgtree's) then 7378
//! (a Docker hub's), on loopback and this device's addresses ("Turn on
//! phone access" binds it to the Tailscale address only). Either way an
//! address goes in a QR only after it answered /healthz as the same hub
//! (its name and address count); an advertised door that doesn't answer
//! falls back to looking (Docker publishes 7371 as 7378, a router or tunnel
//! changes it too).

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::time::Duration;

use crate::hub::Health;
use crate::{HubAddress, HubClient};

/// The ports looked for when the hub doesn't name its door.
pub const DOOR_PORTS: [u16; 2] = [7371, 7378];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Door {
    /// The door listens on `port`; `on`: the addresses it answered on.
    Found { port: u16, on: Vec<IpAddr> },
    /// The hub says no door runs: other devices can't reach it.
    Off,
    /// Not a loopback hub, or no door found: offer the hub as it is.
    Unknown,
}

/// Find the door of `hub` (a loopback address; anything else is Unknown).
/// `ips`: this device's addresses; `ports`: where to look when the hub
/// doesn't say (DOOR_PORTS); `limit`: per /healthz.
pub async fn find_door(hub: &str, ips: &[IpAddr], ports: &[u16], limit: Duration) -> Door {
    if !crate::link::is_loopback_hub(hub) {
        return Door::Unknown;
    }
    let Ok(addr) = HubAddress::parse(hub) else {
        return Door::Unknown;
    };
    let own_port = url::Url::parse(addr.as_str()).ok().and_then(|u| u.port());
    let Ok(Ok(me)) = tokio::time::timeout(limit, HubClient::new(addr).healthz()).await else {
        return Door::Unknown;
    };
    let mut hosts: Vec<IpAddr> = vec![Ipv4Addr::LOCALHOST.into()];
    hosts.extend(ips.iter().copied().filter(|ip| !ip.is_loopback()));
    if me.supports("door") {
        let Some(d) = &me.door else {
            return Door::Off;
        };
        if let Ok(bind) = d.bind.parse::<IpAddr>() {
            let at: Vec<IpAddr> = if bind.is_unspecified() { hosts.clone() } else { vec![bind] };
            let on = answering(&me, at.iter().map(|ip| (*ip, d.port)).collect(), limit).await;
            if !on.is_empty() {
                return Door::Found { port: d.port, on: on.into_iter().map(|(ip, _)| ip).collect() };
            }
        }
    }
    // look for it: the first port (in order) that answers anywhere
    let tries: Vec<(IpAddr, u16)> = ports
        .iter()
        .filter(|p| Some(**p) != own_port)
        .flat_map(|p| hosts.iter().map(move |ip| (*ip, *p)))
        .collect();
    let found = answering(&me, tries, limit).await;
    let Some(port) = ports.iter().find(|p| found.iter().any(|(_, fp)| fp == *p)) else {
        return Door::Unknown;
    };
    Door::Found {
        port: *port,
        on: found.into_iter().filter(|(_, p)| p == port).map(|(ip, _)| ip).collect(),
    }
}

/// The addresses a device-link QR names for `hub` (this device's
/// addresses: `hostname`, `ips`). With a door found, only addresses that
/// answered /healthz as the same hub: the ones the door answered on
/// (Tailscale's first, as phone access binds the door there), `hostname` on
/// the door's port if it answers there too, then `hub` itself for a second
/// app on this device. Otherwise the hub's aliases as they are.
pub async fn qr_hubs(hub: &str, hostname: Option<&str>, ips: &[IpAddr], door: &Door, limit: Duration) -> Vec<String> {
    let Door::Found { port, on } = door else {
        return crate::link::hub_aliases(hub, hostname, ips, None);
    };
    let Ok(addr) = HubAddress::parse(hub) else {
        return vec![hub.to_string()];
    };
    let mut on: Vec<IpAddr> = on.iter().copied().filter(|ip| !ip.is_loopback()).collect();
    on.sort_by_key(|ip| !is_tailnet(ip)); // stable: otherwise as found
    let mut out = crate::link::hub_aliases(hub, None, &on, Some(*port));
    let host = hostname.map(|h| h.trim().to_ascii_lowercase()).filter(|h| !h.is_empty());
    if let Some(h) = host.and_then(|h| HubAddress::parse(&format!("http://{h}:{port}")).ok()) {
        if !crate::link::is_loopback_hub(h.as_str()) {
            let me = tokio::time::timeout(limit, HubClient::new(addr).healthz()).await;
            if let Ok(Ok(me)) = me {
                if same_hub(&me, h.clone(), limit).await && !out.contains(&h.to_string()) {
                    out.insert(out.len().saturating_sub(1), h.to_string());
                }
            }
        }
    }
    out
}

/// 100.64.0.0/10, where Tailscale gives out addresses.
pub fn is_tailnet(ip: &IpAddr) -> bool {
    matches!(ip, IpAddr::V4(v4) if v4.octets()[0] == 100 && (v4.octets()[1] & 0xC0) == 64)
}

/// Whether `a` answers /healthz as the hub `me` (same name and address count).
async fn same_hub(me: &Health, a: HubAddress, limit: Duration) -> bool {
    match tokio::time::timeout(limit, HubClient::new(a).healthz()).await {
        Ok(Ok(h)) => h.name == me.name && h.orgs == me.orgs,
        _ => false,
    }
}

/// Which of `at` answer /healthz as the hub `me` (same name and address
/// count), all at once; in the order given.
async fn answering(me: &Health, at: Vec<(IpAddr, u16)>, limit: Duration) -> Vec<(IpAddr, u16)> {
    let mut tries = tokio::task::JoinSet::new();
    for (i, (ip, port)) in at.iter().copied().enumerate() {
        let Ok(a) = HubAddress::parse(&SocketAddr::new(ip, port).to_string()) else {
            continue;
        };
        let me = me.clone();
        tries.spawn(async move { same_hub(&me, a, limit).await.then_some(i) });
    }
    let mut ok: Vec<usize> = Vec::new();
    while let Some(r) = tries.join_next().await {
        if let Ok(Some(i)) = r {
            ok.push(i);
        }
    }
    ok.sort_unstable();
    ok.into_iter().map(|i| at[i]).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    /// A fake hub: answers every request with `body` as /healthz JSON.
    async fn fake(body: String) -> u16 {
        let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = l.local_addr().unwrap().port();
        tokio::spawn(async move {
            loop {
                let Ok((mut s, _)) = l.accept().await else { return };
                let body = body.clone();
                tokio::spawn(async move {
                    let mut buf = [0u8; 2048];
                    let _ = s.read(&mut buf).await;
                    let r = format!(
                        "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                        body.len()
                    );
                    let _ = s.write_all(r.as_bytes()).await;
                });
            }
        });
        port
    }

    fn health(name: &str, features: &[&str], door: Option<(u16, &str)>) -> String {
        let mut v = serde_json::json!({ "ok": true, "name": name, "orgs": 3, "features": features });
        if let Some((port, bind)) = door {
            v["door"] = serde_json::json!({ "port": port, "bind": bind });
        }
        v.to_string()
    }

    const T: Duration = Duration::from_secs(2);
    const LO: IpAddr = IpAddr::V4(Ipv4Addr::LOCALHOST);

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn the_hub_names_its_door() {
        let door = fake(health("home-pc", &["door"], None)).await;
        let hub = fake(health("home-pc", &["door"], Some((door, "127.0.0.1")))).await;
        let d = find_door(&format!("localhost:{hub}"), &[], &[], T).await;
        assert_eq!(d, Door::Found { port: door, on: vec![LO] });
        // every address: tried on this device's own
        let hub = fake(health("home-pc", &["door"], Some((door, "0.0.0.0")))).await;
        let d = find_door(&format!("localhost:{hub}"), &[], &[], T).await;
        assert_eq!(d, Door::Found { port: door, on: vec![LO] });
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_named_door_that_doesnt_answer_is_looked_for() {
        // Docker: the hub says 7371 inside, the door is published elsewhere
        let door = fake(health("home-pc", &["door"], None)).await;
        let hub = fake(health("home-pc", &["door"], Some((1, "127.0.0.1")))).await;
        let d = find_door(&format!("localhost:{hub}"), &[], &[door], T).await;
        assert_eq!(d, Door::Found { port: door, on: vec![LO] });
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn no_door_running_means_off_without_looking() {
        let other = fake(health("home-pc", &[], None)).await;
        let hub = fake(health("home-pc", &["door"], None)).await;
        assert_eq!(find_door(&format!("localhost:{hub}"), &[], &[other], T).await, Door::Off);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn an_older_hub_is_looked_for_on_the_door_ports() {
        let door = fake(health("home-pc", &[], None)).await;
        let stranger = fake(health("other-hub", &[], None)).await;
        let hub = fake(health("home-pc", &[], None)).await;
        // the first port that answers as this hub wins; another hub doesn't count
        let d = find_door(&format!("localhost:{hub}"), &[], &[stranger, door], T).await;
        assert_eq!(d, Door::Found { port: door, on: vec![LO] });
        assert_eq!(find_door(&format!("localhost:{hub}"), &[], &[stranger], T).await, Door::Unknown);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_qr_names_only_addresses_that_answered() {
        // the door answers on loopback only; 192.0.2.7 (a documentation
        // address) is one of this PC's addresses that doesn't answer
        let door = fake(health("home-pc", &[], None)).await;
        let hub = fake(health("home-pc", &[], None)).await;
        let hub = format!("http://localhost:{hub}");
        let lan: IpAddr = "192.0.2.7".parse().unwrap();
        let short = Duration::from_millis(500);
        let d = find_door(&hub, &[lan], &[door], short).await;
        assert_eq!(d, Door::Found { port: door, on: vec![LO] });
        // so the QR names no other address: just the hub, for this PC
        assert_eq!(qr_hubs(&hub, Some("home-pc.invalid"), &[lan], &d, short).await, vec![hub.clone()]);
        // answered on the LAN and Tailscale: Tailscale first, and the other
        // addresses of the PC and a hostname that doesn't answer are left out
        let ts: IpAddr = "100.64.1.2".parse().unwrap();
        let d = Door::Found { port: 7371, on: vec![LO, lan, ts] };
        let other: IpAddr = "192.0.2.8".parse().unwrap();
        assert_eq!(
            qr_hubs(&hub, Some("home-pc.invalid"), &[lan, other, ts], &d, short).await,
            vec!["http://100.64.1.2:7371".to_string(), "http://192.0.2.7:7371".into(), hub.clone()]
        );
        // without a door the hub's aliases are as before
        assert_eq!(
            qr_hubs(&hub, Some("home-pc"), &[lan], &Door::Unknown, short).await,
            crate::link::hub_aliases(&hub, Some("home-pc"), &[lan], None)
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_hostname_counts_only_as_the_same_hub() {
        let me: Health = serde_json::from_str(&health("home-pc", &[], None)).unwrap();
        let same = fake(health("home-pc", &[], None)).await;
        let stranger = fake(health("other-hub", &[], None)).await;
        let at = |p: u16| HubAddress::parse(&format!("http://localhost:{p}")).unwrap();
        assert!(same_hub(&me, at(same), T).await);
        assert!(!same_hub(&me, at(stranger), T).await);
        assert!(!same_hub(&me, at(1), T).await);
    }

    #[test]
    fn tailnet_addresses() {
        for (ip, ts) in [("100.64.0.1", true), ("100.127.255.254", true), ("100.128.0.1", false), ("192.168.1.2", false)] {
            assert_eq!(is_tailnet(&ip.parse().unwrap()), ts, "{ip}");
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn only_a_loopback_hub_has_a_door_to_find() {
        let hub = fake(health("home-pc", &["door"], None)).await;
        assert_eq!(find_door(&format!("home-pc:{hub}"), &[], &[], T).await, Door::Unknown);
        // a loopback address where nothing answers
        assert_eq!(find_door("localhost:1", &[], &[], T).await, Door::Unknown);
    }
}
