//! Tauri commands: the UI's whole API. Every hub call runs on the core's
//! runtime; the UI listens to `hc` events and re-reads what changed.

use std::future::Future;
use std::time::Duration;

use hubchat_core::engine::{Engine, HubStatus};
use hubchat_core::hub::Health;
use hubchat_core::store::{ChatSummary, Contact, Message, NewOutgoing};
use hubchat_core::{recovery, HubAddress, Identity};
use serde::Serialize;

use crate::core::{self, Core};

type R<T> = Result<T, String>;

/// An error as the text the UI shows. Verbose logging records that a command
/// was refused and, for a core error, its kind (hubchat_core::diag::err_kind);
/// the text itself is never logged.
fn s<E: std::fmt::Display + 'static>(e: E) -> String {
    if hubchat_core::diag::verbose() {
        match (&e as &dyn std::any::Any).downcast_ref::<hubchat_core::Error>() {
            Some(err) => tracing::info!(target: "cmd", "refused: {}", hubchat_core::diag::err_kind(err)),
            None => tracing::info!(target: "cmd", "refused"),
        }
    }
    e.to_string()
}

/// Run a future on the core runtime (where the engine's tasks live).
pub(crate) async fn on_core<T, F>(f: F) -> R<T>
where
    T: Send + 'static,
    F: Future<Output = R<T>> + Send + 'static,
{
    // each piece of UI work is one `ui` request in the log
    let f = tracing::Instrument::instrument(f, hubchat_core::diag::request("ui"));
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
    picture: Option<hubchat_core::pictures::PictureMeta>,
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
    push: Option<core::PushState>,
    file_background_hint: Option<String>,
    hubs: Vec<HubStatus>,
    platform: &'static str,
    /// What other devices see this one called (linking uses it as is).
    device_name: String,
    /// The phone's maker on Android ("samsung": Settings › Notifications
    /// explains how to hide only the background notification); empty elsewhere.
    device_maker: String,
    /// This device had an identity but its key is gone (the lost-key screen).
    key_lost: Option<core::KeyLost>,
    /// The key was restored from Hubchat's backup; say so once.
    key_restored: bool,
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
            picture: c.store.picture_meta(&e.me().address()).ok().flatten(),
        }),
        recovery_saved: meta(c, "recovery.saved") == "yes",
        read_receipts: meta(c, "settings.read_receipts") != "off",
        notifications: c.notify_settings(),
        stay_connected: c.platform().stay_connected(),
        push: c.platform().push_state(),
        file_background_hint: c.platform().file_background_hint(),
        device_name: c.device_name(),
        device_maker: c.platform().device_maker(),
        hubs: engine.map(|e| e.hub_statuses()).unwrap_or_default(),
        key_lost: c.key_lost(),
        key_restored: meta(c, "key.restored") == "yes",
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
    c.adopt_identity(me, true, &[("profile.name", name.trim())])?;
    Ok(address)
}

#[tauri::command]
pub fn hc_restore_words(words: String) -> R<String> {
    let c = core::get()?;
    let me = recovery::from_words(&words).map_err(s)?;
    let address = me.address();
    c.adopt_identity(me, false, &[("recovery.saved", "yes")])?;
    Ok(address)
}

/// The lost-key screen: try the key store again (it may not have answered).
#[tauri::command]
pub fn hc_retry_key() -> R<()> {
    core::get()?.load_key()
}

/// The lost-key screen: remove this device's local data and start afresh.
#[tauri::command]
pub fn hc_start_over() -> R<()> {
    core::get()?.start_over()
}

/// The notice that the key was restored from Hubchat's backup was seen.
#[tauri::command]
pub fn hc_key_restored_seen() -> R<()> {
    core::get()?.store.set_meta("key.restored", "seen").map_err(s)
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

#[tauri::command]
pub async fn hc_picture(peer:String) -> R<Option<hubchat_core::pictures::PictureRecord>> {
    let e=engine()?;
    on_core(async move { e.picture(&peer).await.map_err(s) }).await
}

static PICTURE_DECODE:tokio::sync::Semaphore=tokio::sync::Semaphore::const_new(1);

#[tauri::command]
pub async fn hc_picture_preview(data:String) -> R<hubchat_core::picture_image::Preview> {
    // Decode off the UI/core runtime. A small semaphore bounds simultaneous
    // untrusted image allocations on mobile as well as desktop.
    let permit=PICTURE_DECODE.acquire().await.map_err(s)?;
    let result=tauri::async_runtime::spawn_blocking(move || {
        // Cancellation does not stop blocking decoding. Keep its slot until exit.
        let _permit=permit;
        let bytes=hubchat_core::picture_image::decode_input(&data).map_err(s)?;
        hubchat_core::picture_image::preview(&bytes).map_err(s)
    }).await.map_err(s)?;
    result
}

#[tauri::command]
pub async fn hc_set_picture(owner:String,data:Option<String>,crop:Option<hubchat_core::picture_image::Crop>) -> R<hubchat_core::pictures::PictureRecord> {
    let e=engine()?;
    let permit=PICTURE_DECODE.acquire().await.map_err(s)?;
    let result=tauri::async_runtime::spawn_blocking(move || {
        let _permit=permit;
        let normalized=data.map(|data| {
            let bytes=hubchat_core::picture_image::decode_input(&data).map_err(s)?;
            hubchat_core::picture_image::normalize(&bytes,crop.unwrap_or_default()).map_err(s)
        }).transpose()?;
        e.edit_picture(&owner,normalized).map_err(s)
    }).await.map_err(s)?;
    result
}

/// Settings › Notifications: notify at all, show the message text, sound.
#[tauri::command]
pub fn hc_set_notifications(app: tauri::AppHandle, enabled: bool, preview: bool, sound: bool) -> R<()> {
    core::get()?.set_notify_settings(core::NotifySettings {
        enabled,
        preview,
        sound,
    })?;
    // the tray's Notifications submenu shows the same switches
    #[cfg(desktop)]
    crate::tray::notifications_changed(&app);
    #[cfg(not(desktop))]
    let _ = app;
    Ok(())
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

#[tauri::command]
pub fn hc_set_push(on: bool, distributor: String) -> R<()> {
    core::get()?.platform().set_push(on, &distributor)
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

/// The composer: our user edited the draft for `peer` (`on`), or stopped
/// writing to it (sent, cleared, chat left, app in the background). While
/// they write, the core sends typing heartbeats, never the draft.
#[tauri::command]
pub fn hc_composing(peer: String, on: bool) -> R<()> {
    let c = core::get()?;
    if let Ok(e) = c.engine() {
        let _g = c.rt.enter();
        e.composing(&peer, on);
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

/// How long a likelier address may still answer once a less likely one has.
const GRACE: Duration = Duration::from_millis(750);

/// `hubs` tried at once, `limit` each: the likeliest that answers (as
/// `name`, when given), else the first's failure. Once one answers, the
/// likelier ones still pending get `GRACE` and no more, so an address that a
/// firewall silently drops cannot hold the result for the whole `limit`.
pub(crate) async fn probe_first(hubs: Vec<String>, name: Option<String>, limit: Duration) -> Probe {
    probe_first_inner(hubs, name, limit, discovery_context(None)).await
}

/// Received-link probes retain both their hub generation and listener attempt.
pub(crate) async fn probe_first_for(hubs: Vec<String>, name: Option<String>, limit: Duration,
    reception: hubchat_core::link::Reception) -> Probe {
    probe_first_inner(hubs, name, limit, discovery_context(Some(reception))).await
}

async fn probe_first_inner(hubs: Vec<String>, name: Option<String>, limit: Duration,
    requests: R<hubchat_core::link::Discovery>) -> Probe {
    let requests = match requests { Ok(requests) => requests, Err(error) => return Probe::Invalid { error } };
    {
        let mut set = tokio::task::JoinSet::new();
        for (i, h) in hubs.iter().cloned().enumerate() {
            let name = name.clone();
            let requests = requests.clone();
            set.spawn(async move { (i, probe_request(&h, name.as_deref(), limit, requests).await) });
        }
        let mut done: Vec<Option<Probe>> = hubs.iter().map(|_| None).collect();
        let connected = |p: &Option<Probe>| matches!(p, Some(Probe::Connected { .. }));
        let mut deadline = None;
        loop {
            let next = match deadline {
                Some(d) => match tokio::time::timeout_at(d, set.join_next()).await {
                    Ok(r) => r,
                    Err(_) => break,
                },
                None => set.join_next().await,
            };
            let Some(r) = next else { break };
            let Ok((i, p)) = r else { continue };
            done[i] = Some(p);
            // the first that answered, once every likelier one has failed
            let w = done.iter().take_while(|p| p.is_some()).position(connected);
            if let Some(w) = w {
                set.abort_all();
                return done[w].take().expect("answered");
            }
            if deadline.is_none() && done.iter().any(connected) {
                deadline = Some(tokio::time::Instant::now() + GRACE);
            }
        }
        set.abort_all();
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
    probe_inner(input, name, limit, None).await
}

async fn probe_inner(input: &str, name: Option<&str>, limit: Duration,
    reception: Option<hubchat_core::link::Reception>) -> Probe {
    let requests = match discovery_context(reception) {
        Ok(requests) => requests,
        Err(error) => return Probe::Invalid { error },
    };
    probe_request(input, name, limit, requests).await
}

async fn probe_request(input: &str, name: Option<&str>, limit: Duration,
    requests: hubchat_core::link::Discovery) -> Probe {
    let addr = match HubAddress::parse(input) {
        Ok(a) => a,
        Err(e) => return Probe::Invalid { error: s(e) },
    };
    let url = addr.to_string();
    let r: Result<Result<Health, _>, _> = tokio::time::timeout(limit, requests.health(addr)).await;
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

pub(crate) fn discovery_context(origin: Option<hubchat_core::link::Reception>) -> R<hubchat_core::link::Discovery> {
    let c = core::get()?;
    let requests = hubchat_core::link::Discovery::new(move |address| {
        hubchat_core::link::start_client(&c.store, address, || c.engine().ok())
    });
    Ok(match origin { Some(origin) => requests.with_origin(origin), None => requests })
}

#[tauri::command]
pub fn hc_add_hub(input: String) -> R<String> {
    let c = core::get()?;
    let e = c.engine()?;
    let _g = c.rt.enter();
    let address = e.add_hub(&input).map(|a| a.to_string()).map_err(s)?;
    c.platform().refresh_push();
    Ok(address)
}

#[tauri::command]
pub fn hc_set_hub_enabled(url: String, enabled: bool) -> R<()> {
    let c = core::get()?;
    let e = c.engine()?;
    let _guard = c.rt.enter();
    e.set_hub_enabled(&url, enabled).map_err(s)?;
    c.platform().refresh_push();
    Ok(())
}

#[tauri::command]
pub async fn hc_remove_hub(url: String, unregister: bool) -> R<()> {
    let e = engine()?;
    on_core(async move { e.remove_hub(&url, unregister).await.map_err(s) }).await?;
    core::get()?.platform().refresh_push();
    Ok(())
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
pub fn hc_shortcut_theme(dark: bool) -> R<()> {
    #[cfg(target_os = "android")]
    { crate::android::shortcut_theme(dark) }
    #[cfg(not(target_os = "android"))]
    { let _ = dark; Ok(()) }
}

#[tauri::command]
pub fn hc_shortcut_avatar(app: tauri::AppHandle, owner: String, peer: String, name: String, kind: String, png: String) -> R<()> {
    #[cfg(windows)]
    { crate::jumplist::admit_avatar(&app, &owner, &peer, &name, &kind, &png) }
    #[cfg(not(windows))]
    { let _ = (app, owner, peer, name, kind, png); Ok(()) }
}

#[tauri::command]
pub fn hc_shortcut_task_icon(app: tauri::AppHandle, view: String, png: String) -> R<()> {
    #[cfg(windows)]
    { crate::jumplist::admit_task_icon(&app, &view, &png) }
    #[cfg(not(windows))]
    { let _ = (app, view, png); Ok(()) }
}

#[tauri::command]
pub fn hc_close_dm(peer: String) -> R<()> {
    engine()?.close_dm(&peer).map_err(s)
}

#[tauri::command]
pub fn hc_open_dm(peer: String) -> R<()> {
    engine()?.open_dm(&peer).map_err(s)
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
    let (before, from) = (
        before.as_ref().map(|(a, b)| (a.as_str(), b.as_str())),
        from.as_ref().map(|(a, b)| (a.as_str(), b.as_str())),
    );
    let limit = limit.unwrap_or(50).min(5000);
    // the engine leaves out what older pages still have to fill in
    match core::get()?.engine() {
        Ok(e) => e.chat(&peer, before, from, limit),
        Err(_) => core::get()?.store.chat(&peer, before, from, limit),
    }
    .map_err(s)
}

/// Scrolling back past what this device holds: one older page of the chat
/// from every hub this device started from now on (lazy history).
#[tauri::command]
pub async fn hc_load_older(peer: String) -> R<hubchat_core::engine::OlderPage> {
    let e = engine()?;
    on_core(async move { e.load_older(&peer).await.map_err(s) }).await
}

#[tauri::command]
pub fn hc_message(id: String) -> R<Option<Message>> {
    core::get()?.store.message(&id).map_err(s)
}

/// What the composer sends: the message, and what a multi-reply answers and
/// the options it picks on question cards (docs/question-cards.md).
#[derive(serde::Deserialize)]
pub struct SendRequest {
    #[serde(flatten)]
    msg: NewOutgoing,
    #[serde(default)]
    replies: Vec<String>,
    #[serde(default)]
    answers: Vec<hubchat_core::questions::Answer>,
}

#[tauri::command]
pub async fn hc_send(msg: SendRequest) -> R<()> {
    let SendRequest { msg, replies, answers } = msg;
    let c=core::get()?;
    let _admission=c.file_admission(!msg.attachments.is_empty() || msg.body.len() as u64>hubchat_core::engine::BODY_INLINE_MAX);
    let e=engine()?;
    let reply = hubchat_core::questions::ReplyMeta { replies, answers };
    // Own preparation even if the invoking view/command goes away.
    c.rt.spawn(async move { let _admission=_admission; e.send_with(msg, reply).await.map_err(s) }).await.map_err(s)?
}

#[tauri::command]
pub async fn hc_remove_attachment(message_id:String,local_id:String) -> R<()> {
    let _admission=core::get()?.file_admission(true);
    engine()?.remove_attachment(&message_id,&local_id).await.map_err(s)
}

#[tauri::command]
pub fn hc_retry(id: String) -> R<()> {
    let _admission=core::get()?.file_admission(true);
    engine()?.retry(&id).map_err(s)
}

#[tauri::command]
pub fn hc_cancel_transfer(local_id: String) -> R<()> {
    engine()?.cancel_transfer(&local_id).map_err(s)
}

#[tauri::command]
pub fn hc_transfers(after: Option<String>) -> R<hubchat_core::store::TransferPage> {
    core::get()?.store.transfer_page(after.as_deref().unwrap_or("")).map_err(s)
}

#[tauri::command]
pub async fn hc_download(message_id: String, local_id: String) -> R<()> {
    let e = engine()?;
    // Admission only. A scheduler-owned worker finishes even if this command
    // or its originating view disappears; no network await in the command.
    let admission=core::get()?.file_admission(true);
    tokio::task::spawn_blocking(move || {
        let _admission=admission;
        e.queue_download(&message_id, &local_id).map(|_| ()).map_err(s)
    })
        .await.map_err(s)?
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

/// A chat's unsent replies and picked options (they survive a restart).
#[tauri::command]
pub fn hc_draft_answers(peer: String) -> R<Option<hubchat_core::store::DraftAnswers>> {
    core::get()?.store.draft_answers(&peer).map_err(s)
}

#[tauri::command]
pub fn hc_set_draft_answers(peer: String, answers: hubchat_core::store::DraftAnswers) -> R<()> {
    core::get()?.store.set_draft_answers(&peer, &answers).map_err(s)
}

/// Question cards in a chat and the first answer to each, however far back
/// (what settles them, beside the loaded page).
#[tauri::command]
pub fn hc_card_answers(peer: String, ids: Vec<String>) -> R<Vec<Message>> {
    core::get()?.store.card_answers(&peer, &ids).map_err(s)
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
    #[cfg(target_os = "android")]
    {
        let _ = (app, reveal); // Android opens the file; it has no desktop reveal action.
        crate::android::open_download(&path, &a.name)
    }
    #[cfg(not(target_os = "android"))]
    {
        use tauri_plugin_opener::OpenerExt;
        if path.starts_with("content://") {
            return app.opener().open_url(path, None::<&str>).map_err(s);
        }
        if reveal {
            app.opener().reveal_item_in_dir(&path).map_err(s)
        } else {
            app.opener().open_path(path, None::<&str>).map_err(s)
        }
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

// ------------------------------------------------------- setup codes

/// A scanned or opened `hubchat://setup` link: the link, or why it isn't one.
#[derive(Serialize)]
pub struct ParsedSetup {
    link: Option<hubchat_core::setup::SetupLink>,
    error: Option<hubchat_core::setup::SetupError>,
}

#[tauri::command]
pub fn hc_parse_setup(input: String) -> ParsedSetup {
    match hubchat_core::setup::SetupLink::parse(&input) {
        Ok(l) => ParsedSetup { link: Some(l), error: None },
        Err(e) => ParsedSetup { link: None, error: Some(e) },
    }
}

#[derive(Serialize)]
pub struct SetupReach {
    reachable: bool,
    /// The name the hub gave, when it answered.
    name: Option<String>,
    error: Option<String>,
}

/// Can this device reach the setup link's hub? Any ok /healthz answer
/// within 5 s passes. Another name than the link's `hubname` is only
/// logged: Orgtree sends the PC's Tailscale name there when the hub's own
/// name is blank (hubchat-opus 11:38Z).
#[tauri::command]
pub async fn hc_setup_check(hub: String, hubname: String) -> R<SetupReach> {
    let addr = HubAddress::parse(&hub).map_err(s)?;
    on_core(async move {
        let requests = discovery_context(None)?;
        let r = tokio::time::timeout(Duration::from_secs(5), requests.health(addr)).await;
        Ok(match r {
            Ok(Ok(h)) if h.ok => {
                if h.name != hubname {
                    tracing::info!(hub = %hubchat_core::diag::tag("hub", &hub), "setup: the hub answers under another name than the setup code says");
                }
                SetupReach { reachable: true, name: Some(h.name), error: None }
            }
            Ok(Ok(h)) => SetupReach { reachable: false, name: Some(h.name), error: Some("the hub says it isn't ok".into()) },
            Ok(Err(e)) => SetupReach { reachable: false, name: None, error: Some(s(e)) },
            Err(_) => SetupReach { reachable: false, name: None, error: Some("no answer within 5 s".into()) },
        })
    })
    .await
}

/// Whether an app is installed (Android); None where it can't be told.
#[tauri::command]
pub fn hc_app_installed(package: String) -> R<Option<bool>> {
    Ok(core::get()?.platform().app_installed(&package))
}

/// Open Tailscale's store page, the Tailscale app or the Wi-Fi settings.
#[tauri::command]
pub fn hc_open_app(what: String) -> R<bool> {
    Ok(core::get()?.platform().open_app(&what))
}

/// Whether this device's traffic goes through a VPN now (Android); None
/// where it can't be told.
#[tauri::command]
pub fn hc_vpn_active() -> R<Option<bool>> {
    Ok(core::get()?.platform().vpn_active())
}

/// Join the setup link's hub and send the org our code (the identity
/// exists by now). Returns the chat to open.
#[tauri::command]
pub async fn hc_setup_start(input: String, name: String) -> R<String> {
    let link = hubchat_core::setup::SetupLink::parse(&input).map_err(s)?;
    let e = engine()?;
    on_core(async move {
        e.setup_join(&link, name.trim(), Duration::from_secs(20))
            .await
            .map_err(s)
    })
    .await
}

/// Where the setup with `org` stands (its chat's note), if one was started.
#[tauri::command]
pub fn hc_setup_status(org: String) -> R<Option<hubchat_core::setup::SetupState>> {
    Ok(engine().ok().and_then(|e| e.setup_state(&org)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    // Probe-order fixtures have no app Core/saved configuration. Inject only
    // their synthetic-hub resolver into the actual production request path.
    async fn probe_first(hubs: Vec<String>, name: Option<String>, limit: Duration) -> Probe {
        let requests = hubchat_core::link::Discovery::new(|a| Ok(hubchat_core::HubClient::new(a)));
        super::probe_first_inner(hubs, name, limit, Ok(requests)).await
    }

    /// A fake hub on loopback: answers /healthz as hub `name`.
    async fn hub(name: &'static str) -> String {
        let l = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", l.local_addr().unwrap());
        tokio::spawn(async move {
            loop {
                let Ok((mut c, _)) = l.accept().await else { return };
                tokio::spawn(async move {
                    let mut buf = [0u8; 1024];
                    let _ = c.read(&mut buf).await;
                    let body = format!(r#"{{"ok":true,"name":"{name}"}}"#);
                    let reply = format!(
                        "HTTP/1.1 200 OK
Content-Type: application/json
Content-Length: {}
Connection: close

{body}",
                        body.len()
                    );
                    let _ = c.write_all(reply.as_bytes()).await;
                });
            }
        });
        url
    }

    /// Accepts and never answers, like a port a firewall silently drops.
    async fn silent() -> String {
        let l = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", l.local_addr().unwrap());
        tokio::spawn(async move {
            let mut held = Vec::new();
            while let Ok((c, _)) = l.accept().await {
                held.push(c);
            }
        });
        url
    }

    /// Answers at once, but not as a hub: a failure that arrives immediately.
    async fn not_a_hub() -> String {
        let l = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", l.local_addr().unwrap());
        tokio::spawn(async move {
            while let Ok((mut c, _)) = l.accept().await {
                tokio::spawn(async move {
                    let mut buf = [0u8; 1024];
                    let _ = c.read(&mut buf).await;
                    let _ = c
                        .write_all(b"HTTP/1.1 404 Not Found
Content-Length: 0
Connection: close

")
                        .await;
                });
            }
        });
        url
    }

    /// Nothing listens here: refused (slowly on Windows, which retries).
    async fn closed() -> String {
        let l = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", l.local_addr().unwrap());
        drop(l);
        url
    }

    fn url_of(p: &Probe) -> &str {
        match p {
            Probe::Connected { url, .. } | Probe::Unreachable { url, .. } | Probe::NotAHub { url, .. } => url,
            Probe::Invalid { .. } => "",
        }
    }

    #[tokio::test]
    async fn a_silent_likelier_address_does_not_hold_the_result() {
        let (quiet, real) = (silent().await, hub("Home").await);
        let t = Instant::now();
        let p = probe_first(vec![quiet, real.clone()], None, Duration::from_secs(10)).await;
        let took = t.elapsed();
        println!("silent first, real second: {took:?}");
        assert!(matches!(p, Probe::Connected { .. }));
        assert_eq!(url_of(&p), real);
        assert!(took >= GRACE && took < Duration::from_millis(1500), "took {took:?}");
    }

    #[tokio::test]
    async fn both_answering_the_likelier_one_wins() {
        let (a, b) = (hub("Home").await, hub("Home").await);
        let p = probe_first(vec![a.clone(), b], None, Duration::from_secs(10)).await;
        assert_eq!(url_of(&p), a);
    }

    #[tokio::test]
    async fn a_likelier_failure_is_not_waited_for() {
        let (dead, real) = (not_a_hub().await, hub("Home").await);
        let t = Instant::now();
        let p = probe_first(vec![dead, real.clone()], None, Duration::from_secs(10)).await;
        assert_eq!(url_of(&p), real);
        assert!(t.elapsed() < GRACE, "took {:?}", t.elapsed());
    }

    #[tokio::test]
    async fn all_failing_gives_the_first_failure() {
        let (a, b) = (closed().await, closed().await);
        let p = probe_first(vec![a.clone(), b], None, Duration::from_secs(10)).await;
        assert!(matches!(p, Probe::Unreachable { .. }));
        assert_eq!(url_of(&p), a);
    }

    #[tokio::test]
    async fn another_hubs_name_is_still_not_a_hub_match() {
        let other = hub("Elsewhere").await;
        let p = probe_first(vec![other], Some("Home".into()), Duration::from_secs(10)).await;
        assert!(matches!(p, Probe::NotAHub { .. }));
    }

    #[tokio::test]
    async fn the_named_hub_is_taken_past_a_different_one() {
        let (other, home) = (hub("Elsewhere").await, hub("Home").await);
        let p = probe_first(vec![other, home.clone()], Some("Home".into()), Duration::from_secs(10)).await;
        assert_eq!(url_of(&p), home);
    }
}
