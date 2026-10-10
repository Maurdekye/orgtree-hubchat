//! Device linking commands (see hubchat_core::link for the scheme).
//! New device: hc_link_start shows a code and listens on a throwaway address
//! for up to ten minutes; progress goes to the UI as `hc-link` events.
//! Signed-in device: hc_link_lookup finds the waiting device, hc_link_approve
//! sends it the sealed bundle. Also the offline QR and the key file.

use std::str::FromStr;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use hubchat_core::hub::{CancelFlag, Profile};
use hubchat_core::door::{self, Door};
use hubchat_core::link::{self, Bundle, LinkUrl};
use hubchat_core::{HubAddress, HubClient};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Runtime};

use crate::core;

type R<T> = Result<T, String>;

fn s<E: std::fmt::Display>(e: E) -> String {
    e.to_string()
}

const LINK_TTL: Duration = Duration::from_secs(600);

static LISTENING: Mutex<Option<CancelFlag>> = Mutex::new(None);

#[derive(Serialize, Clone)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum LinkEvent {
    Waiting {
        expires_in_s: u64,
    },
    /// This device already IS that identity (user ruling 19:21Z): nothing to do.
    Same {
        address: String,
    },
    /// An identity arrived and waits in memory: the user reviews the hubs
    /// that come with it (user 20:38Z: prefilled, editable, nothing saved
    /// silently) and confirms (hc_link_confirm) or drops it
    /// (hc_link_discard). `from`: this device's identity now, which
    /// confirming leaves (the switch, user ruling 19:21Z).
    Review {
        from: Option<String>,
        to: String,
        name: String,
        hubs: Vec<HubRow>,
    },
    Failed {
        error: String,
    },
    Expired,
}

/// One hub that comes with a linked identity, as the review shows it.
#[derive(Serialize, Clone)]
pub struct HubRow {
    /// The hub as the other device knows it (maybe localhost-style).
    theirs: String,
    /// The best address on this device: one that answered, else the first
    /// candidate.
    address: String,
    /// Addresses tried, likeliest first.
    candidates: Vec<String>,
    /// The hub that answered at `address`.
    name: Option<String>,
    /// Why `address` did not answer.
    error: Option<String>,
    reachable: bool,
}

#[derive(Serialize)]
pub struct LinkStart {
    /// XXXX-XXXX-XXXX-XXXX, to type on the other device.
    code: String,
    /// What the QR carries: `hubchat://link?code=…&hub=…&role=take`.
    qr: String,
    hub: String,
    /// This PC's hub says its relay-only door is off: other devices can't
    /// reach it (the UI says so).
    phone_access_off: bool,
}

/// Put the bundle to use on this device: identity, profile, and exactly
/// `hubs` (what the user confirmed; the bundle's own for a QR or a file).
fn adopt(bundle: Bundle, hubs: Vec<String>) -> R<String> {
    let c = core::get()?;
    let me = bundle.identity().map_err(s)?;
    let address = me.address();
    let mut meta = vec![("profile.name", bundle.name.as_str()), ("profile.about", bundle.about.as_str())];
    if bundle.recovery_saved {
        meta.push(("recovery.saved", "yes"));
    }
    c.adopt_identity(me, false, &meta)?;
    let e = c.engine()?;
    let _g = c.rt.enter();
    for h in hubs {
        let _ = e.add_hub(&h);
    }
    Ok(address)
}

/// The identity a link carries just arrived. This device's own: nothing to
/// do. Otherwise it waits in memory (an unadopted key is never written to
/// disk) while the user reviews its hubs: each tried here, under the
/// addresses the link suggests (`via`: where this device reached the link's
/// hub; `aliases`: what the link called it).
async fn arrived(bundle: Bundle, via: &str, aliases: &[String]) -> LinkEvent {
    let current = core::get()
        .ok()
        .and_then(|c| c.engine().ok())
        .map(|e| e.me().address());
    let to = match bundle.identity() {
        Ok(me) => me.address(),
        Err(e) => return LinkEvent::Failed { error: s(e) },
    };
    if current.as_deref() == Some(to.as_str()) {
        return LinkEvent::Same { address: to };
    }
    let mut set = tokio::task::JoinSet::new();
    for (i, (theirs, candidates)) in link::review_hubs(&bundle.hubs, via, aliases)
        .into_iter()
        .enumerate()
    {
        set.spawn(async move {
            let p = crate::commands::probe_first(candidates.clone(), None, Duration::from_secs(5)).await;
            let mut key: Option<link::HubKey> = None;
            let row = match p {
                crate::commands::Probe::Connected {
                    url,
                    name,
                    version,
                    orgs,
                    ..
                } => {
                    key = Some((name.clone(), version, orgs));
                    HubRow {
                        theirs,
                        address: url,
                        candidates,
                        name: Some(name),
                        error: None,
                        reachable: true,
                    }
                }
                crate::commands::Probe::Unreachable { error, .. }
                | crate::commands::Probe::NotAHub { error, .. }
                | crate::commands::Probe::Invalid { error } => HubRow {
                    theirs,
                    address: candidates.first().cloned().unwrap_or_default(),
                    candidates,
                    name: None,
                    error: Some(error),
                    reachable: false,
                },
            };
            (i, row, key)
        });
    }
    let mut probed: Vec<(usize, HubRow, Option<link::HubKey>)> = Vec::new();
    while let Some(r) = set.join_next().await {
        if let Ok(r) = r {
            probed.push(r);
        }
    }
    probed.sort_by_key(|(i, _, _)| *i);
    // one row per hub: the same hub under two names merges
    let keyed: Vec<(String, Vec<String>, Option<link::HubKey>)> = probed
        .iter()
        .map(|(_, r, k)| (r.theirs.clone(), r.candidates.clone(), k.clone()))
        .collect();
    let rows: Vec<HubRow> = link::merge_same_hubs(&keyed, &bundle.hubs, via)
        .into_iter()
        .map(|(i, theirs, candidates)| HubRow {
            theirs,
            candidates,
            ..probed[i].1.clone()
        })
        .collect();
    let name = bundle.name.clone();
    *PENDING.lock().unwrap() = Some(bundle);
    LinkEvent::Review {
        from: current,
        to,
        name,
        hubs: rows,
    }
}

/// What a link brought, until the user confirms or drops it. Memory only.
static PENDING: Mutex<Option<Bundle>> = Mutex::new(None);

/// The user confirmed the review: on a signed-in device leave the current
/// identity first (off the device lists of v2 hubs; key and local data
/// forgotten), then adopt the one the link brought with exactly `hubs`.
#[tauri::command]
pub async fn hc_link_confirm(hubs: Vec<String>) -> R<String> {
    let bundle = PENDING
        .lock()
        .unwrap()
        .take()
        .ok_or("nothing to confirm: link again")?;
    let c = core::get()?;
    if c.engine().is_ok() {
        c.leave_identity().await?;
    }
    let mut keep: Vec<String> = Vec::new();
    for h in hubs.iter().map(|h| h.trim()).filter(|h| !h.is_empty()) {
        let a = HubAddress::parse(h).map_err(s)?.to_string();
        if !keep.contains(&a) {
            keep.push(a);
        }
    }
    adopt(bundle, keep)
}

/// The user kept things as they were: drop what the link brought.
#[tauri::command]
pub fn hc_link_discard() -> R<()> {
    PENDING.lock().unwrap().take();
    Ok(())
}

fn my_bundle() -> R<Bundle> {
    let c = core::get()?;
    let e = c.engine()?;
    let meta = |k: &str| c.store.meta(k).ok().flatten().unwrap_or_default();
    Ok(Bundle {
        id: e.me().id().into(),
        secret: e.me().secret().into(),
        name: meta("profile.name"),
        about: meta("profile.about"),
        hubs: c
            .store
            .hubs()
            .map_err(s)?
            .into_iter()
            .map(|h| h.url)
            .collect(),
        recovery_saved: meta("recovery.saved") == "yes",
    })
}

// ------------------------------------------------------------ new device

#[tauri::command]
pub async fn hc_link_start<R2: Runtime>(
    app: AppHandle<R2>,
    hub: String,
    device_name: String,
    code: Option<String>,
    aliases: Option<Vec<String>>,
) -> R<LinkStart> {
    let c = core::get()?;
    // A signed-in device may join too: what arrives is held for a decision
    // (same identity: nothing; another one: offer to switch).
    let addr = HubAddress::parse(&hub).map_err(s)?;
    // A code from a scanned/typed QR (the other device made it), or our own.
    let code = match code {
        Some(c) => link::format_code(&link::normalize_code(&c).map_err(s)?),
        None => link::new_link_code(),
    };
    let temp = link::link_identity(&code).map_err(s)?;
    let client = HubClient::new(addr.clone());
    let profile = Profile {
        kind: "chat".into(),
        org_name: if device_name.trim().is_empty() {
            "New device".into()
        } else {
            device_name.trim().chars().take(48).collect()
        },
        username: "link".into(),
        blurb: "Waiting to be linked to a Hubchat identity".into(),
    };
    {
        let (client, temp, profile) = (client.clone(), temp.clone(), profile.clone());
        c.rt.spawn(async move { client.register(&temp, &profile).await })
            .await
            .map_err(s)?
            .map_err(s)?;
    }
    let stop = CancelFlag::default();
    if let Some(old) = LISTENING.lock().unwrap().replace(stop.clone()) {
        old.cancel();
    }
    let code2 = code.clone();
    let via = addr.to_string();
    let aliases = aliases.unwrap_or_default();
    c.rt.spawn(async move {
        let t0 = Instant::now();
        let _ = app.emit(
            "hc-link",
            LinkEvent::Waiting {
                expires_in_s: LINK_TTL.as_secs(),
            },
        );
        let outcome = loop {
            if stop.is_cancelled() {
                break None;
            }
            if t0.elapsed() > LINK_TTL {
                break Some(LinkEvent::Expired);
            }
            let p = match client.poll(&temp, 20).await {
                Ok(p) => p,
                Err(_) => {
                    tokio::time::sleep(Duration::from_secs(3)).await;
                    continue;
                }
            };
            let ids: Vec<String> = p.messages.iter().map(|m| m.id.clone()).collect();
            let found = p.messages.iter().find_map(|m| {
                link::open_from_link(&code2, &m.body)
                    .ok()
                    .map(|b| (m.id.clone(), b))
            });
            if !ids.is_empty() {
                let _ = client.ack(&temp, &ids).await;
            }
            if let Some((id, bundle)) = found {
                // Hubs that keep history (v2) would keep our copy of the
                // sealed identity: delete it (older hubs answer 404).
                let _ = client.delete_message(&temp, &id).await;
                break Some(arrived(bundle, &via, &aliases).await);
            }
        };
        let _ = client.unregister(&temp).await;
        if let Some(ev) = outcome {
            let _ = app.emit("hc-link", ev);
        }
    });
    let hub = addr.to_string();
    let (host, ips) = local_addresses();
    let (hubs, phone_access_off) = aliases_for(&hub, host.as_deref(), &ips).await;
    let qr = LinkUrl {
        code: link::normalize_code(&code).map_err(s)?,
        hubs,
        hub_name: None,
        role: Some("take".into()),
    }
    .to_url();
    Ok(LinkStart {
        qr,
        code,
        hub,
        phone_access_off,
    })
}

#[tauri::command]
pub fn hc_link_cancel() -> R<()> {
    if let Some(f) = LISTENING.lock().unwrap().take() {
        f.cancel();
    }
    Ok(())
}

// ----------------------------------------------------- signed-in device

#[derive(Serialize)]
pub struct LinkLookup {
    code: String,
    address: String,
    /// The waiting device's name, from the hub's roster.
    device_name: Option<String>,
    hubs: Vec<String>,
    /// The hub named in a scanned QR, when it isn't one of ours.
    unknown_hub: Option<String>,
}

/// This machine's name and the addresses other devices may reach it at:
/// the tailnet's (Tailscale) and the one its default route leaves by.
/// Nothing on Android: a phone is not where a hub runs.
fn local_addresses() -> (Option<String>, Vec<std::net::IpAddr>) {
    if cfg!(target_os = "android") {
        return (None, Vec::new());
    }
    let host = std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .ok();
    // Connecting a UDP socket sends nothing; it only picks the route.
    let toward = |dest: &str| -> Option<std::net::IpAddr> {
        let sock = std::net::UdpSocket::bind("0.0.0.0:0").ok()?;
        sock.connect(dest).ok()?;
        Some(sock.local_addr().ok()?.ip())
    };
    let mut ips = Vec::new();
    // Tailscale's resolver address is routed through the tailnet when it is up.
    if let Some(ip) = toward("100.100.100.100:53").filter(door::is_tailnet) {
        ips.push(ip);
    }
    if let Some(ip) = toward("1.1.1.1:53") {
        if !ips.contains(&ip) {
            ips.push(ip);
        }
    }
    (host, ips)
}

/// A hub's addresses for another device, through its door when there is
/// one (hubchat_core::door): a Tailscale address the door answered on goes
/// first (the phone reaches the PC through Tailscale; hubchat-opus 11:50Z).
/// True with them when the hub says no door runs (phone access is off).
async fn aliases_for(hub: &str, host: Option<&str>, ips: &[std::net::IpAddr]) -> (Vec<String>, bool) {
    let (h, name, i) = (hub.to_string(), host.map(str::to_string), ips.to_vec());
    let run = async move {
        let t = Duration::from_secs(2);
        let d = door::find_door(&h, &i, &door::DOOR_PORTS, t).await;
        (door::qr_hubs(&h, name.as_deref(), &i, &d, t).await, d == Door::Off)
    };
    match core::get() {
        Ok(c) => match c.rt.spawn(run).await {
            Ok(r) => r,
            Err(_) => (link::hub_aliases(hub, host, ips, None), false),
        },
        Err(_) => (link::hub_aliases(hub, host, ips, None), false),
    }
}

#[derive(Serialize)]
pub struct ParsedLink {
    code: String,
    /// The likeliest address of the link's hub.
    hub: Option<String>,
    /// Every address the link names for it, the likeliest first.
    hubs: Vec<String>,
    /// The hub's name, to check the right hub answered.
    hub_name: Option<String>,
    role: Option<String>,
}

/// For the UI: what a scanned or opened link holds.
#[tauri::command]
pub fn hc_parse_link(input: String) -> R<ParsedLink> {
    let l = LinkUrl::parse(&input).map_err(s)?;
    Ok(ParsedLink {
        code: link::format_code(&l.code),
        hub: l.hubs.first().cloned(),
        hubs: l.hubs,
        hub_name: l.hub_name,
        role: l.role,
    })
}

#[derive(Serialize)]
pub struct LinkOffer {
    code: String,
    qr: String,
    /// The hub as this device knows it.
    hub: String,
    /// What the QR names it: as other devices may reach it (Fix 2,
    /// coordinator 20:28Z: localhost is no address for a phone).
    hubs: Vec<String>,
    /// This PC's hub says its relay-only door is off.
    phone_access_off: bool,
}

/// Signed-in device: make a one-time code for a NEW device to scan
/// (user 19:12-19:13Z: the PC shows the QR, the phone scans it). The new
/// device then appears under the code's address; hc_link_lookup sees its
/// name and hc_link_approve sends it the identity, as in the other direction.
#[tauri::command]
pub async fn hc_link_offer(hub: Option<String>) -> R<LinkOffer> {
    let c = core::get()?;
    let e = c.engine()?;
    let statuses = e.hub_statuses();
    let hub = match hub {
        Some(h) => HubAddress::parse(&h).map_err(s)?.to_string(),
        None => statuses
            .iter()
            .find(|h| matches!(h.state, hubchat_core::engine::HubState::Connected))
            .or_else(|| statuses.first())
            .map(|h| h.url.clone())
            .ok_or("add a hub first: the new device links through one")?,
    };
    let hub_name = statuses
        .iter()
        .find(|h| h.url == hub)
        .map(|h| h.name.clone())
        .filter(|n| !n.trim().is_empty());
    let (host, ips) = local_addresses();
    let (hubs, phone_access_off) = aliases_for(&hub, host.as_deref(), &ips).await;
    let code = link::new_link_code();
    let qr = LinkUrl {
        code: link::normalize_code(&code).map_err(s)?,
        hubs: hubs.clone(),
        hub_name,
        role: Some("give".into()),
    }
    .to_url();
    Ok(LinkOffer {
        qr,
        code,
        hub,
        hubs,
        phone_access_off,
    })
}

#[tauri::command]
pub fn hc_link_lookup(input: String) -> R<LinkLookup> {
    let c = core::get()?;
    let l = LinkUrl::parse(&input).map_err(s)?;
    let code = l.code;
    let address = link::link_identity(&code).map_err(s)?.address();
    let entry = c
        .store
        .directory()
        .map_err(s)?
        .into_iter()
        .find(|d| d.address == address);
    let mine: Vec<String> = c
        .store
        .hubs()
        .map_err(s)?
        .into_iter()
        .map(|h| h.url)
        .collect();
    let ours = |h: &String| {
        HubAddress::parse(h)
            .map(|a| mine.contains(&a.to_string()))
            .unwrap_or(false)
    };
    let unknown_hub = if l.hubs.iter().any(ours) {
        None
    } else {
        l.hubs.first().cloned()
    };
    Ok(LinkLookup {
        code,
        address,
        device_name: entry.as_ref().map(|e| e.org_name.clone()),
        hubs: entry.map(|e| e.hubs).unwrap_or_default(),
        unknown_hub,
    })
}

#[tauri::command]
pub async fn hc_link_approve(code: String) -> R<String> {
    let c = core::get()?;
    let e = c.engine()?;
    let code = link::normalize_code(&code).map_err(s)?;
    let to = link::link_identity(&code).map_err(s)?.address();
    let body = link::seal_for_link(&code, &my_bundle()?).map_err(s)?;
    let hub = c
        .rt
        .spawn(async move { e.send_unlisted(&to, &body).await })
        .await
        .map_err(s)?
        .map_err(s)?;
    // The new device takes its message and leaves the hub within seconds;
    // if it crashed first, its waiting address would stay on the hub's
    // list for good (seen on the user's hub, 2026-10-08): take it off.
    forget_link_address(&code, Duration::from_secs(180));
    Ok(hub)
}

/// A link code this device offered or approved is done with: after
/// `delay`, take the waiting address it names off every hub we use (the
/// code makes its key, so we can). Quietly does nothing where it is gone.
fn forget_link_address(code: &str, delay: Duration) {
    let (Ok(c), Ok(temp)) = (core::get(), link::link_identity(code)) else {
        return;
    };
    let hubs: Vec<String> = c
        .store
        .hubs()
        .map(|v| v.into_iter().map(|h| h.url).collect())
        .unwrap_or_default();
    c.rt.spawn(async move {
        tokio::time::sleep(delay).await;
        for h in hubs {
            if let Ok(addr) = HubAddress::parse(&h) {
                let _ = HubClient::new(addr).unregister(&temp).await;
            }
        }
    });
}

/// The signed-in device's link offer ended without an approval (closed,
/// denied or expired): no device should stay waiting under its code.
#[tauri::command]
pub fn hc_link_forget(code: String) -> R<()> {
    let code = link::normalize_code(&code).map_err(s)?;
    forget_link_address(&code, Duration::ZERO);
    Ok(())
}

// --------------------------------------------------------- QR and file

/// The offline QR (shown behind a warning in the UI).
#[tauri::command]
pub fn hc_key_qr() -> R<String> {
    Ok(link::to_qr(&my_bundle()?))
}

#[tauri::command]
pub fn hc_restore_qr(text: String) -> R<String> {
    let b = link::from_qr(&text).map_err(s)?;
    let hubs = link::hubs_for_device(&b.hubs, None, &[]);
    adopt(b, hubs)
}

/// Write the passphrase-locked key file to `dest` (a path, or on Android a
/// content:// URI from the save dialog).
#[tauri::command]
pub fn hc_key_file_export<R2: Runtime>(
    app: AppHandle<R2>,
    passphrase: String,
    dest: String,
) -> R<()> {
    use std::io::Write;
    use tauri_plugin_fs::{FilePath, FsExt, OpenOptions};
    let text = link::to_key_file(&my_bundle()?, &passphrase).map_err(s)?;
    let fp = FilePath::from_str(&dest).map_err(s)?;
    let mut f = app
        .fs()
        .open(
            fp,
            OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .clone(),
        )
        .map_err(s)?;
    f.write_all(text.as_bytes()).map_err(s)
}

#[tauri::command]
pub fn hc_key_file_import(source: String, passphrase: String) -> R<String> {
    use std::io::Read;
    let c = core::get()?;
    let mut text = String::new();
    c.platform()
        .open_source(&source)
        .map_err(s)?
        .take(1 << 20)
        .read_to_string(&mut text)
        .map_err(s)?;
    let b = link::from_key_file(&text, &passphrase).map_err(s)?;
    let hubs = link::hubs_for_device(&b.hubs, None, &[]);
    adopt(b, hubs)
}
