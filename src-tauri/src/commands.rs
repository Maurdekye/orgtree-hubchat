//! Tauri commands: the UI's whole API. Every hub call runs on the core's
//! runtime; the UI listens to `hc` events and re-reads what changed.

use std::future::Future;

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
async fn on_core<T, F>(f: F) -> R<T>
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
    hubs: Vec<HubStatus>,
    platform: &'static str,
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
    core::get()?.set_ui_state(foreground, chat);
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
    let addr = match HubAddress::parse(&input) {
        Ok(a) => a,
        Err(e) => return Ok(Probe::Invalid { error: s(e) }),
    };
    let url = addr.to_string();
    on_core(async move {
        let r: Result<Health, _> = HubClient::new(addr).healthz().await;
        Ok(match r {
            Ok(h) => Probe::Connected {
                url,
                max_attachment_bytes: h.max_attachment_bytes(),
                name: h.name,
                features: h.features,
            },
            Err(hubchat_core::Error::NotAHub(e)) => Probe::NotAHub { url, error: e },
            Err(e) => Probe::Unreachable { url, error: s(e) },
        })
    })
    .await
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
pub fn hc_chat(peer: String, before: Option<String>, limit: Option<u32>) -> R<Vec<Message>> {
    core::get()?
        .store
        .chat(&peer, before.as_deref(), limit.unwrap_or(200))
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
    on_core(async move {
        e.download(&message_id, &local_id)
            .await
            .map(|p| p.to_string_lossy().into_owned())
            .map_err(s)
    })
    .await
}

#[tauri::command]
pub async fn hc_mark_read(peer: String) -> R<()> {
    let e = engine()?;
    on_core(async move { e.mark_read(&peer).await.map_err(s) }).await
}

/// "Delete for me": local only until mail hub v2.0 (G4) can delete the
/// hub copy too.
#[tauri::command]
pub fn hc_delete_message(id: String) -> R<()> {
    core::get()?.store.delete_message(&id).map_err(s)
}

#[tauri::command]
pub fn hc_delete_chat(peer: String) -> R<()> {
    core::get()?.store.delete_chat(&peer).map_err(s)
}

#[tauri::command]
pub fn hc_draft(peer: String) -> R<Option<String>> {
    core::get()?.store.draft(&peer).map_err(s)
}

#[tauri::command]
pub fn hc_set_draft(peer: String, body: String) -> R<()> {
    core::get()?.store.set_draft(&peer, &body).map_err(s)
}
