//! Tauri commands: the UI's whole API. Every hub call runs on the core's
//! runtime; the UI listens to `hc` events and re-reads what changed.

use std::future::Future;
use std::time::Duration;

use hubchat_core::engine::{Engine, HubStatus};
use hubchat_core::hub::Health;
use hubchat_core::store::{ChatSummary, Contact, Message, NewOutgoing};
use hubchat_core::{recovery, HubAddress, HubClient, Identity};
use serde::Serialize;

use crate::core::{self, Core};

type R<T> = Result<T, String>;

fn s<E: std::fmt::Display>(e: E) -> String {
    e.to_string()
}

/// Run a future on the core runtime (where the engine's tasks live).
pub(crate) async fn on_core<T, F>(f: F) -> R<T>
where
    T: Send + 'static,
    F: Future<Output = R<T>> + Send + 'static,
{
    core::get()?.rt.spawn(f).await.map_err(s)?
}

fn engine() -> R<std::sync::Arc<Engine>> {
    core::get()?.engine()
}

// ------------------------------------------------------------------ state

#[derive(Serialize)]
pub struct Me {
    id: String,
    address: String,
    name: String,
    about: String,
}

#[derive(Serialize)]
pub struct State {
    me: Option<Me>,
    recovery_saved: bool,
    read_receipts: bool,
    notifications: core::NotifySettings,
    /// Android (design D6): stay connected (true) or check about every 15
    /// minutes (false); null on desktop.
    stay_connected: Option<bool>,
    hubs: Vec<HubStatus>,
    platform: &'static str,
    /// What other devices see this one called (linking uses it as is).
    device_name: String,
}

fn meta(c: &Core, k: &str) -> String {
    c.store.meta(k).ok().flatten().unwrap_or_default()
}

#[tauri::command]
pub fn hc_state() -> R<State> {
    let c = core::get()?;
    let engine = c.engine().ok();
    Ok(State {
        me: engine.as_ref().map(|e| Me {
            id: e.me().id().into(),
            address: e.me().address(),
            name: meta(c, "profile.name"),
            about: meta(c, "profile.about"),
        }),
        recovery_saved: meta(c, "recovery.saved") == "yes",
        read_receipts: meta(c, "settings.read_receipts") != "off",
        notifications: c.notify_settings(),
        stay_connected: c.platform().stay_connected(),
        device_name: c.device_name(),
        hubs: engine.map(|e| e.hub_statuses()).unwrap_or_default(),
        platform: if cfg!(target_os = "android") {
            "android"
        } else {
            "desktop"
        },
    })
}

#[tauri::command]
pub fn hc_ui_state(foreground: bool, chat: Option<String>) -> R<()> {
    let c = core::get()?;
    let came_forward = c.set_ui_state(foreground, chat);
    // Android checking every 15 minutes: the process may have been frozen
    // since it was last on screen, so connect afresh once it is back (D6)
    if came_forward && c.platform().stay_connected() == Some(false) {
        if let Ok(e) = c.engine() {
            e.kick();
        }
    }
    Ok(())
}

// --------------------------------------------------------------- identity

#[derive(Serialize)]
pub struct IdCheck {
    ok: bool,
    error: Option<String>,
    /// What the address will look like (the tag is a placeholder until the
    /// key exists).
    max_len: usize,
}

#[tauri::command]
pub fn hc_check_id(id: String) -> IdCheck {
    let r = hubchat_core::identity::validate_id(&id);
    IdCheck {
        ok: r.is_ok(),
        error: r.err().map(s),
        max_len: recovery::MAX_ID_LEN,
    }
}

#[tauri::command]
pub fn hc_create_identity(id: String, name: String) -> R<String> {
    let c = core::get()?;
    let me = Identity::generate(&id).map_err(s)?;
    let address = me.address();
    c.store.set_meta("profile.name", name.trim()).map_err(s)?;
    c.adopt_identity(me)?;
    Ok(address)
}

#[tauri::command]
pub fn hc_restore_words(words: String) -> R<String> {
    let c = core::get()?;
    let me = recovery::from_words(&words).map_err(s)?;
    let address = me.address();
    c.store.set_meta("recovery.saved", "yes").map_err(s)?;
    c.adopt_identity(me)?;
    Ok(address)
}

#[tauri::command]
pub fn hc_recovery_words() -> R<Vec<String>> {
    recovery::to_words(engine()?.me()).map_err(s)
}

#[tauri::command]
pub fn hc_recovery_saved() -> R<()> {
    core::get()?
        .store
        .set_meta("recovery.saved", "yes")
        .map_err(s)
}

#[tauri::command]
pub async fn hc_set_profile(name: String, about: String) -> R<()> {
    let c = core::get()?;
    c.store.set_meta("profile.name", name.trim()).map_err(s)?;
    c.store.set_meta("profile.about", about.trim()).map_err(s)?;
    let e = engine()?;
    on_core(async move { e.set_profile(name.trim(), about.trim()).await.map_err(s) }).await
}

/// Settings › Notifications: notify at all, show the message text, sound.
#[tauri::command]
pub fn hc_set_notifications(enabled: bool, preview: bool, sound: bool) -> R<()> {
    core::get()?.set_notify_settings(core::NotifySettings {
        enabled,
        preview,
        sound,
    })
}

/// Android (design D6): stay connected, or check about every 15 minutes.
#[tauri::command]
pub fn hc_set_stay_connected(on: bool) -> R<()> {
    let c = core::get()?;
    c.platform().set_stay_connected(on)?;
    if let (true, Ok(e)) = (on, c.engine()) {
        // the new ongoing notification starts at "Connecting…"
        c.platform()
            .status(&format!("Connected as {}", e.me().address()));
    }
    Ok(())
}

/// Whether this device is in use (focused or on screen, touched in the last
/// few minutes): while it is, our other devices don't notify (user 23:50Z).
#[tauri::command]
pub fn hc_set_active(on: bool) -> R<()> {
    let c = core::get()?;
    if let Ok(e) = c.engine() {
        let _g = c.rt.enter();
        e.set_active(on);
    }
    Ok(())
}

/// Settings › Devices: rename this device; returns the name it now has.
#[tauri::command]
pub fn hc_set_device_name(name: String) -> R<String> {
    core::get()?.set_device_name(&name)
}

#[tauri::command]
pub fn hc_set_read_receipts(on: bool) -> R<()> {
    let c = core::get()?;
    c.store
        .set_meta("settings.read_receipts", if on { "on" } else { "off" })
        .map_err(s)?;
    if let Ok(e) = c.engine() {
        e.set_read_receipts(on);
    }
    Ok(())
}

// ------------------------------------------------------------------- hubs

#[derive(Serialize)]
#[serde(tag = "result", rename_all = "snake_case")]
pub enum Probe {
    Connected {
        url: String,
        name: String,
        max_attachment_bytes: u64,
        features: Vec<String>,
        version: Option<String>,
        /// How many addresses it holds (to tell one hub under two names).
        orgs: Option<u64>,
        /// Found by trying a bare host's known ports ("Found X on port N").
        discovered: bool,
    },
    Unreachable {
        url: String,
        error: String,
    },
    NotAHub {
        url: String,
        error: String,
    },
    Invalid {
        error: String,
    },
}

/// Check an address before adding it (onboarding and Settings › Hubs).
#[tauri::command]
pub async fn hc_probe_hub(input: String) -> R<Probe> {
    let cands = match HubAddress::candidates(&input) {
        Ok(c) => c,
        Err(e) => return Ok(Probe::Invalid { error: s(e) }),
    };
    if cands.len() == 1 {
        return on_core(async move { Ok(probe(&input, None, Duration::from_secs(30)).await) }).await;
    }
    // a bare host: the hub's known ports and https, all at once, the main
    // port preferred (user 23:46Z: no port to type)
    let urls: Vec<String> = cands.iter().map(|a| a.to_string()).collect();
    on_core(async move {
        let mut p = probe_first(urls, None, Duration::from_secs(10)).await;
        match &mut p {
            Probe::Connected { discovered, .. } => *discovered = true,
            Probe::Unreachable { error, .. } | Probe::NotAHub { error, .. } => {
                *error = format!("no mail hub answered on port 7370, 7378 or 7371, or over https; on 7370: {error}")
            }
            Probe::Invalid { .. } => {}
        }
        Ok(p)
    })
    .await
}

/// A link's hub under each address the link names, the likeliest first
/// (coordinator 20:28Z): the likeliest that answers as the hub the link
/// names wins; when none does, the first address's failure. All are tried
/// at once, 5 s each at most.
#[tauri::command]
pub async fn hc_probe_link_hubs(hubs: Vec<String>, name: Option<String>) -> R<Probe> {
    if hubs.is_empty() {
        return Ok(Probe::Invalid {
            error: "the link names no hub".into(),
        });
    }
    on_core(async move { Ok(probe_first(hubs, name, Duration::from_secs(5)).await) }).await
}

/// `hubs` tried at once, `limit` each: the likeliest that answers (as
/// `name`, when given), else the first's failure.
pub(crate) async fn probe_first(hubs: Vec<String>, name: Option<String>, limit: Duration) -> Probe {
    {
        let mut set = tokio::task::JoinSet::new();
        for (i, h) in hubs.iter().cloned().enumerate() {
            let name = name.clone();
            set.spawn(async move { (i, probe(&h, name.as_deref(), limit).await) });
        }
        let mut done: Vec<Option<Probe>> = hubs.iter().map(|_| None).collect();
        let connected = |p: &Option<Probe>| matches!(p, Some(Probe::Connected { .. }));
        while let Some(r) = set.join_next().await {
            let Ok((i, p)) = r else { continue };
            done[i] = Some(p);
            // the first that answered, once every likelier one has failed
            let w = done.iter().take_while(|p| p.is_some()).position(connected);
            if let Some(w) = w {
                set.abort_all();
                return done[w].take().expect("answered");
            }
        }
        if let Some(w) = done.iter().position(connected) {
            return done[w].take().expect("answered");
        }
        done.into_iter()
            .next()
            .flatten()
            .unwrap_or(Probe::Invalid {
                error: "no address answered".into(),
            })
    }
}

/// /healthz at `input` within `limit`; `name`: the hub that must answer.
async fn probe(input: &str, name: Option<&str>, limit: Duration) -> Probe {
    let addr = match HubAddress::parse(input) {
        Ok(a) => a,
        Err(e) => return Probe::Invalid { error: s(e) },
    };
    let url = addr.to_string();
    let r: Result<Result<Health, _>, _> =
        tokio::time::timeout(limit, HubClient::new(addr).healthz()).await;
    match r {
        Err(_) => Probe::Unreachable {
            url,
            error: format!("no answer within {} s", limit.as_secs()),
        },
        Ok(Ok(h)) if name.is_some_and(|n| n != h.name) => Probe::NotAHub {
            url,
            error: format!("another hub answers there ({})", h.name),
        },
        Ok(Ok(h)) => Probe::Connected {
            url,
            max_attachment_bytes: h.max_attachment_bytes(),
            name: h.name,
            features: h.features,
            version: h.version,
            orgs: h.orgs,
            discovered: false,
        },
        Ok(Err(hubchat_core::Error::NotAHub(e))) => Probe::NotAHub { url, error: e },
        Ok(Err(e)) => Probe::Unreachable { url, error: s(e) },
    }
}

#[tauri::command]
pub fn hc_add_hub(input: String) -> R<String> {
    let c = core::get()?;
    let e = c.engine()?;
    let _g = c.rt.enter();
    e.add_hub(&input).map(|a| a.to_string()).map_err(s)
}

#[tauri::command]
pub async fn hc_remove_hub(url: String, unregister: bool) -> R<()> {
    let e = engine()?;
    on_core(async move { e.remove_hub(&url, unregister).await.map_err(s) }).await
}

#[tauri::command]
pub fn hc_retry_now() -> R<()> {
    engine()?.retry_now();
    Ok(())
}

// -------------------------------------------------------- directory/chats

#[tauri::command]
pub fn hc_directory() -> R<Vec<Contact>> {
    core::get()?.store.directory().map_err(s)
}

#[derive(Serialize)]
pub struct Resolved {
    /// The input as an address, when it is one exactly.
    exact: Option<Contact>,
    /// Directory entries whose id matches (bare id typed).
    matches: Vec<Contact>,
    is_me: bool,
    valid: bool,
    address: String,
}

/// New chat's To: field: `@net:slug`, `slug` or a bare id.
#[tauri::command]
pub fn hc_resolve(input: String) -> R<Resolved> {
    let c = core::get()?;
    let me = c.engine().map(|e| e.me().address()).unwrap_or_default();
    let raw = input.trim().trim_start_matches("@net:").to_lowercase();
    let valid = !raw.is_empty()
        && raw.chars().all(|ch| {
            ch.is_ascii_lowercase() || ch.is_ascii_digit() || matches!(ch, '.' | '_' | '-')
        });
    let dir = c.store.directory().map_err(s)?;
    let exact = dir.iter().find(|d| d.address == raw).cloned();
    let prefix = format!("{raw}.");
    let matches = if exact.is_none() && valid {
        dir.into_iter()
            .filter(|d| d.address.starts_with(&prefix))
            .collect()
    } else {
        Vec::new()
    };
    Ok(Resolved {
        is_me: raw == me,
        exact,
        matches,
        valid,
        address: raw,
    })
}

#[tauri::command]
pub fn hc_chats() -> R<Vec<ChatSummary>> {
    core::get()?.store.chats().map_err(s)
}

#[tauri::command]
/// A page of a chat (lazy history, user 23:46Z): the newest `limit` before
/// the message (`before_at`, `before_id`), or, with `from_*`, everything
/// from that message on (a re-read of what the chat shows).
pub fn hc_chat(
    peer: String,
    before_at: Option<String>,
    before_id: Option<String>,
    from_at: Option<String>,
    from_id: Option<String>,
    limit: Option<u32>,
) -> R<Vec<Message>> {
    let pair = |a: &Option<String>, b: &Option<String>| match (a, b) {
        (Some(a), Some(b)) => Some((a.clone(), b.clone())),
        _ => None,
    };
    let (before, from) = (pair(&before_at, &before_id), pair(&from_at, &from_id));
    core::get()?
        .store
        .chat(
            &peer,
            before.as_ref().map(|(a, b)| (a.as_str(), b.as_str())),
            from.as_ref().map(|(a, b)| (a.as_str(), b.as_str())),
            limit.unwrap_or(50).min(5000),
        )
        .map_err(s)
}

#[tauri::command]
pub fn hc_message(id: String) -> R<Option<Message>> {
    core::get()?.store.message(&id).map_err(s)
}

#[tauri::command]
pub fn hc_send(msg: NewOutgoing) -> R<()> {
    engine()?.send(msg).map_err(s)
}

#[tauri::command]
pub fn hc_retry(id: String) -> R<()> {
    engine()?.retry(&id).map_err(s)
}

#[tauri::command]
pub fn hc_cancel_transfer(local_id: String) -> R<()> {
    engine()?.cancel_transfer(&local_id);
    Ok(())
}

#[tauri::command]
pub async fn hc_download(message_id: String, local_id: String) -> R<String> {
    let e = engine()?;
    let (mid, lid) = (message_id.clone(), local_id.clone());
    let path = on_core(async move { e.download(&mid, &lid).await.map_err(s) }).await?;
    let c = core::get()?;
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    match c.platform().publish_download(&path, &name) {
        Some(moved) => {
            c.store
                .set_attachment(&local_id, "done", None, Some(&moved), None)
                .map_err(s)?;
            Ok(moved)
        }
        None => Ok(path.to_string_lossy().into_owned()),
    }
}

/// A hubchat:// link the system opened us with (Android), taken once.
#[tauri::command]
pub fn hc_take_pending_link() -> R<Option<String>> {
    Ok(core::get()?.platform().take_pending_link())
}

/// The chat a tapped notification asked for (Android), taken once.
#[tauri::command]
pub fn hc_take_pending_chat() -> R<Option<String>> {
    Ok(core::get()?.platform().take_pending_chat())
}

/// The hub picker: the chat's pinned hub, Automatic's choice, the next hub
/// and the hubs that list the peer.
#[tauri::command]
pub async fn hc_send_route(peer: String) -> R<hubchat_core::engine::SendRoute> {
    // off the main thread: it waits for the store
    let e = engine()?;
    on_core(async move { e.send_route(&peer).map_err(s) }).await
}

/// Pin a chat to one of our hubs; null = Automatic.
#[tauri::command]
pub fn hc_set_send_hub(peer: String, hub: Option<String>) -> R<()> {
    engine()?.set_send_hub(&peer, hub.as_deref()).map_err(s)
}

#[tauri::command]
pub async fn hc_mark_read(peer: String) -> R<()> {
    let e = engine()?;
    on_core(async move { e.mark_read(&peer).await.map_err(s) }).await
}

/// "Delete for me": our copy goes from this device and, on a mail hub v2.0,
/// from the hub and so from all our devices. The other side keeps theirs.
#[tauri::command]
pub async fn hc_delete_message(id: String) -> R<()> {
    let e = engine()?;
    let mid = id.clone();
    on_core(async move { e.delete_message(&mid).await.map_err(s) }).await?;
    crate::media::forget(&[id]);
    Ok(())
}

#[tauri::command]
pub async fn hc_delete_chat(peer: String) -> R<()> {
    let e = engine()?;
    on_core(async move { e.delete_chat(&peer).await.map_err(s) }).await?;
    crate::media::forget_gone();
    Ok(())
}

#[derive(Serialize)]
pub struct Devices {
    this_device: String,
    devices: Vec<hubchat_core::hub_v2::DeviceEntry>,
}

/// The devices using this identity (from v2 hubs; empty on older hubs).
#[tauri::command]
pub async fn hc_devices() -> R<Devices> {
    let e = engine()?;
    let this_device = e.device().0;
    let devices = on_core(async move { Ok(e.devices().await) }).await?;
    Ok(Devices {
        this_device,
        devices,
    })
}

#[tauri::command]
pub fn hc_draft(peer: String) -> R<Option<String>> {
    core::get()?.store.draft(&peer).map_err(s)
}

#[tauri::command]
pub fn hc_set_draft(peer: String, body: String) -> R<()> {
    core::get()?.store.set_draft(&peer, &body).map_err(s)
}

#[derive(Serialize)]
pub struct FileInfo {
    name: String,
    bytes: u64,
}

/// Name and size of a picked or dropped file (a path, or on Android a
/// content:// URI), so the composer can check the hub's limit before anything
/// uploads.
#[tauri::command]
pub fn hc_file_info(source: String) -> R<FileInfo> {
    // a folder dropped on the chat (Windows can't open one as a file anyway)
    if !source.starts_with("content://") && std::path::Path::new(&source).is_dir() {
        return Err("it's a folder. Hubchat sends files, not folders.".into());
    }
    let c = core::get()?;
    let f = c.platform().open_source(&source).map_err(s)?;
    let bytes = f.metadata().map_err(s)?.len();
    let name = c
        .platform()
        .source_name(&source)
        .unwrap_or_else(|| "file".into());
    Ok(FileInfo { name, bytes })
}

/// Open a downloaded attachment with the system's default app (or, with
/// `reveal`, show it in its folder on desktop).
#[tauri::command]
pub fn hc_open_attachment<RT: tauri::Runtime>(
    app: tauri::AppHandle<RT>,
    message_id: String,
    local_id: String,
    reveal: bool,
) -> R<()> {
    use tauri_plugin_opener::OpenerExt;
    let m = core::get()?
        .store
        .message(&message_id)
        .map_err(s)?
        .ok_or("no such message")?;
    let a = m
        .attachments
        .into_iter()
        .find(|a| a.local_id == local_id)
        .ok_or("no such attachment")?;
    let path = a.local_path.ok_or("not downloaded yet")?;
    if path.starts_with("content://") {
        return app.opener().open_url(path, None::<&str>).map_err(s);
    }
    if reveal {
        app.opener().reveal_item_in_dir(&path).map_err(s)
    } else {
        app.opener().open_path(path, None::<&str>).map_err(s)
    }
}

/// The recovery words as a text file (user 19:08Z). `dest` is the save
/// dialog's choice on desktop; without it (Android) the file goes to the
/// shared Downloads. Saving counts as "saved". Returns where it went.
#[tauri::command]
pub fn hc_save_recovery<RT: tauri::Runtime>(
    app: tauri::AppHandle<RT>,
    dest: Option<String>,
) -> R<String> {
    use std::io::Write;
    let c = core::get()?;
    let e = c.engine()?;
    let words = recovery::to_words(e.me()).map_err(s)?;
    let mut text = String::new();
    text.push_str("Hubchat recovery words\r\n\r\n");
    text.push_str(&format!("Address: @net:{}\r\n", e.me().address()));
    text.push_str(&format!("Saved:   {}\r\n\r\n", hubchat_core::engine::now()));
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
    let file_name = format!("hubchat-recovery-{}.txt", e.me().id());
    let place = match dest {
        Some(dest) => {
            use std::str::FromStr;
            use tauri_plugin_fs::{FilePath, FsExt, OpenOptions};
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
            f.write_all(text.as_bytes()).map_err(s)?;
            dest
        }
        None => {
            let tmp = c.platform().download_dir().join(&file_name);
            std::fs::create_dir_all(tmp.parent().unwrap()).map_err(s)?;
            std::fs::write(&tmp, text.as_bytes()).map_err(s)?;
            match c.platform().publish_download(&tmp, &file_name) {
                Some(_) => format!("Downloads/{file_name}"),
                None => tmp.to_string_lossy().into_owned(),
            }
        }
    };
    c.store.set_meta("recovery.saved", "yes").map_err(s)?;
    Ok(place)
}
