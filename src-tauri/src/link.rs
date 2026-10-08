//! Device linking commands (see hubchat_core::link for the scheme).
//! New device: hc_link_start shows a code and listens on a throwaway address
//! for up to ten minutes; progress goes to the UI as `hc-link` events.
//! Signed-in device: hc_link_lookup finds the waiting device, hc_link_approve
//! sends it the sealed bundle. Also the offline QR and the key file.

use std::str::FromStr;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use hubchat_core::hub::{CancelFlag, Profile};
use hubchat_core::link::{self, Bundle};
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
    Waiting { expires_in_s: u64 },
    Done { address: String },
    Failed { error: String },
    Expired,
}

#[derive(Serialize)]
pub struct LinkStart {
    /// XXXX-XXXX-XXXX-XXXX, to type on the other device.
    code: String,
    /// What the QR carries: `hubchat-link:<code>@<hub url>`.
    qr: String,
    hub: String,
}

/// Put the bundle to use on this device: identity, profile, hubs.
fn adopt(bundle: Bundle) -> R<String> {
    let c = core::get()?;
    let me = bundle.identity().map_err(s)?;
    let address = me.address();
    c.store.set_meta("profile.name", &bundle.name).map_err(s)?;
    c.store
        .set_meta("profile.about", &bundle.about)
        .map_err(s)?;
    c.adopt_identity(me)?;
    let e = c.engine()?;
    let _g = c.rt.enter();
    for h in &bundle.hubs {
        let _ = e.add_hub(h);
    }
    Ok(address)
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
    })
}

// ------------------------------------------------------------ new device

#[tauri::command]
pub async fn hc_link_start<R2: Runtime>(
    app: AppHandle<R2>,
    hub: String,
    device_name: String,
    code: Option<String>,
) -> R<LinkStart> {
    let c = core::get()?;
    if c.has_identity() {
        return Err("this device already has an identity".into());
    }
    let addr = HubAddress::parse(&hub).map_err(s)?;
    // A code from a scanned/typed QR (the other device made it), or our own.
    let code = match code {
        Some(c) => format_code(&link::normalize_code(&c).map_err(s)?),
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
            let found = p
                .messages
                .iter()
                .find_map(|m| link::open_from_link(&code2, &m.body).ok());
            if !ids.is_empty() {
                let _ = client.ack(&temp, &ids).await;
            }
            if let Some(bundle) = found {
                break Some(match adopt(bundle) {
                    Ok(address) => LinkEvent::Done { address },
                    Err(error) => LinkEvent::Failed { error },
                });
            }
        };
        let _ = client.unregister(&temp).await;
        if let Some(ev) = outcome {
            let _ = app.emit("hc-link", ev);
        }
    });
    let hub = addr.to_string();
    Ok(LinkStart {
        qr: format!("hubchat-link:{code}@{hub}"),
        code,
        hub,
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

/// The link QR is a URL so a phone camera opens Hubchat with it
/// (user 19:12Z): `hubchat://link?code=XXXX-XXXX-XXXX-XXXX&hub=<url>`.
pub fn link_url(code: &str, hub: &str) -> String {
    let enc = |v: &str| url::form_urlencoded::byte_serialize(v.as_bytes()).collect::<String>();
    format!("hubchat://link?code={}&hub={}", enc(code), enc(hub))
}

fn format_code(c: &str) -> String {
    format!("{}-{}-{}-{}", &c[..4], &c[4..8], &c[8..12], &c[12..])
}

/// A typed code, the link URL, or the older `hubchat-link:CODE@HUB` text.
pub fn parse_code_or_qr(input: &str) -> R<(String, Option<String>)> {
    let t = input.trim();
    if let Some(rest) = t.strip_prefix("hubchat-link:") {
        let (code, hub) = rest.split_once('@').ok_or("damaged link QR code")?;
        return Ok((
            link::normalize_code(code).map_err(s)?,
            Some(hub.to_string()),
        ));
    }
    if t.starts_with("hubchat://") {
        let u = url::Url::parse(t).map_err(|_| "damaged link QR code")?;
        let get = |k: &str| {
            u.query_pairs()
                .find(|(n, _)| n == k)
                .map(|(_, v)| v.into_owned())
        };
        let code = get("code").ok_or("the link has no code")?;
        return Ok((link::normalize_code(&code).map_err(s)?, get("hub")));
    }
    Ok((link::normalize_code(t).map_err(s)?, None))
}

#[derive(Serialize)]
pub struct ParsedLink {
    code: String,
    hub: Option<String>,
}

/// For the UI: what a scanned or opened link holds.
#[tauri::command]
pub fn hc_parse_link(input: String) -> R<ParsedLink> {
    let (code, hub) = parse_code_or_qr(&input)?;
    Ok(ParsedLink {
        code: format_code(&code),
        hub,
    })
}

#[derive(Serialize)]
pub struct LinkOffer {
    code: String,
    qr: String,
    hub: String,
}

/// Signed-in device: make a one-time code for a NEW device to scan
/// (user 19:12-19:13Z: the PC shows the QR, the phone scans it). The new
/// device then appears under the code's address; hc_link_lookup sees its
/// name and hc_link_approve sends it the identity, as in the other direction.
#[tauri::command]
pub fn hc_link_offer(hub: Option<String>) -> R<LinkOffer> {
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
    let code = link::new_link_code();
    Ok(LinkOffer {
        qr: link_url(&code, &hub),
        code,
        hub,
    })
}

#[tauri::command]
pub fn hc_link_lookup(input: String) -> R<LinkLookup> {
    let c = core::get()?;
    let (code, hub) = parse_code_or_qr(&input)?;
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
    let unknown_hub = hub.filter(|h| {
        HubAddress::parse(h)
            .map(|a| !mine.contains(&a.to_string()))
            .unwrap_or(true)
    });
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
    c.rt.spawn(async move { e.send_unlisted(&to, &body).await })
        .await
        .map_err(s)?
        .map_err(s)
}

// --------------------------------------------------------- QR and file

/// The offline QR (shown behind a warning in the UI).
#[tauri::command]
pub fn hc_key_qr() -> R<String> {
    Ok(link::to_qr(&my_bundle()?))
}

#[tauri::command]
pub fn hc_restore_qr(text: String) -> R<String> {
    adopt(link::from_qr(&text).map_err(s)?)
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
    adopt(link::from_key_file(&text, &passphrase).map_err(s)?)
}
