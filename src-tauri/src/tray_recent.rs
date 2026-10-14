//! Recent chats and attention share the menu's one popup window. Native
//! event leases expire by monotonic time even when the main WebView is hidden.
use std::{collections::HashMap, sync::{Mutex, atomic::{AtomicBool, Ordering}}, time::{Duration, Instant}};
use hubchat_core::engine::Event;
use serde::Serialize;
use tauri::{AppHandle, Manager, PhysicalPosition, WebviewWindow};

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct RecentRow {
    pub peer: String,
    pub name: String,
    pub kind: String,
    pub unread: u64,
    pub typing: bool,
    pub picture: Option<String>,
    pub picture_key: Option<String>,
}

#[derive(Default)]
struct View {
    owner: String,
    rows: Vec<RecentRow>,
    typing: HashMap<String, Instant>,
    clicks: Clicks,
    dot: Option<bool>,
    available: bool,
}

#[derive(Default)]
struct Clicks { serial: u64, suppress: Option<Instant>, blurred: Option<Instant> }
impl Clicks {
    fn cancel(&mut self) { self.serial = self.serial.wrapping_add(1); }
    fn left(&mut self, now: Instant, open: bool) -> Option<(u64, bool)> {
        if self.suppress.is_some_and(|until| now < until) { return None; }
        self.cancel();
        let toggle = open || self.blurred.take().is_some_and(|at|now.duration_since(at)<Duration::from_millis(250));
        Some((self.serial, toggle))
    }
    fn double(&mut self, now: Instant, wait: Duration) { self.cancel(); self.suppress = Some(now + wait); }
}

#[derive(Default)]
pub struct Recent { view: Mutex<View>, pending: AtomicBool }

fn core() -> Result<&'static crate::core::Core, String> { crate::core::get() }

fn typing_live(at: Instant, now: Instant) -> bool { now.saturating_duration_since(at) < Duration::from_secs(1) }

pub fn available(app: &AppHandle) -> bool {
    app.try_state::<Recent>().is_some_and(|r|r.view.lock().map(|v|v.available).unwrap_or(false))
}

pub fn read(app: &AppHandle) -> Result<(String, Vec<RecentRow>), String> {
    let r = app.state::<Recent>();
    let v = r.view.lock().map_err(|_| "Tray unavailable")?;
    let owner = v.owner.clone(); let mut rows = v.rows.clone();
    let now = Instant::now();
    for row in &mut rows { row.typing = v.typing.get(&row.peer).is_some_and(|at|typing_live(*at,now)); }
    drop(v);
    if owner.is_empty() { return Ok((owner, vec![])); }
    core()?.tray_with_identity(Some(&owner), |_| Ok((owner.clone(), rows)))
}

/// Only data changes query SQLite. Typing refreshes reuse the six cached rows.
pub fn sync(app: &AppHandle) {
    let Some(r) = app.try_state::<Recent>() else { return; };
    let loaded = core().and_then(|c| c.tray_with_identity(None, |owner| {
        let (chats, dot) = c.store.tray_overview().map_err(|e|e.to_string())?;
        let directory = c.store.directory().map_err(|e|e.to_string())?;
        let rows = chats.into_iter().map(|chat| {
            let contact = directory.iter().find(|d|d.address == chat.peer);
            let kind = contact.map(|d|d.kind.as_str()).filter(|k|matches!(*k,"org"|"chat")).unwrap_or("person").to_owned();
            let name = contact.map(|d| if !d.org_name.is_empty() { d.org_name.clone() } else { d.username.clone() })
                .filter(|s| !s.is_empty()).unwrap_or_else(||chat.peer.clone());
            let picture_key = if kind == "chat" { None } else {
                c.store.picture_meta(&chat.peer).ok().flatten().filter(|m|m.sha256.is_some())
                    .and_then(|m|serde_json::to_string(&m).ok())
            };
            // Bytes travel once per peer/revision via the bounded picture
            // command, never in each typing/state snapshot.
            RecentRow { peer: chat.peer, name, kind, unread: chat.unread, typing: false, picture: None, picture_key }
        }).collect::<Vec<_>>();
        Ok((owner.to_owned(), rows, dot))
    }));
    let available = loaded.is_ok();
    let (owner, rows, dot) = match loaded {
        Ok((o,r,d)) => (o,r,Some(d)),
        Err(_) => match core().and_then(|c|c.engine()) {
            Ok(e) => (e.me().address(), vec![], None),
            Err(_) => (String::new(), vec![], Some(false))
        }
    };
    let publish = || {
        let mut v = r.view.lock().unwrap();
        let changed_owner = v.owner != owner;
        if changed_owner { v.typing.clear(); v.clicks.cancel(); }
        v.owner = owner.clone(); v.rows = rows; v.available = available;
        let final_dot = if changed_owner && dot.is_none() { Some(false) } else { dot };
        let change_dot = final_dot.is_some() && v.dot != final_dot;
        if final_dot.is_some() { v.dot = final_dot; }
        drop(v);
        if change_dot { set_dot(app, final_dot.unwrap_or(false)); }
        Ok(changed_owner)
    };
    let changed_owner = if owner.is_empty() { publish() } else { core().and_then(|c| c.tray_with_identity(Some(&owner), |_| publish())) };
    let Ok(changed_owner) = changed_owner else { changed(app); return; };
    if changed_owner { crate::tray_popup::close_recent(app); }
    if crate::tray_popup::is_recent(app) { crate::tray_popup::refresh(app); }
}

pub fn changed(app: &AppHandle) {
    let Some(r) = app.try_state::<Recent>() else { return; };
    if r.pending.swap(true, Ordering::AcqRel) { return; }
    let handle = app.clone();
    if app.run_on_main_thread(move || {
        if let Some(r) = handle.try_state::<Recent>() { r.pending.store(false, Ordering::Release); }
        sync(&handle);
    }).is_err() { r.pending.store(false, Ordering::Release); }
}

pub fn reset(app: &AppHandle) {
    if let Some(r) = app.try_state::<Recent>() {
        let mut v = r.view.lock().unwrap();
        v.owner.clear(); v.rows.clear(); v.typing.clear(); v.clicks.cancel(); v.dot = None; v.available = false;
    }
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || { crate::tray_popup::close_recent(&handle); set_dot(&handle, false); });
}

pub fn event(app: &AppHandle, event: &Event) {
    let Some(r) = app.try_state::<Recent>() else { return; };
    match event {
        Event::Typing { peer } => {
            let Ok(c) = core() else { return; };
            let Ok(engine) = c.engine() else { return; };
            let owner = engine.me().address(); let at = Instant::now();
            {
                let mut v = r.view.lock().unwrap();
                if v.owner != owner { return; }
                // Only the six visible chats need native typing leases.
                if !v.rows.iter().any(|row|row.peer == *peer) { return; }
                v.typing.insert(peer.clone(), at);
            }
            refresh_view(app);
            let handle = app.clone(); let peer = peer.clone();
            c.rt.spawn(async move {
                tokio::time::sleep(Duration::from_secs(1)).await;
                if let Some(r) = handle.try_state::<Recent>() {
                    let mut v = r.view.lock().unwrap();
                    if v.owner != owner || v.typing.get(&peer) != Some(&at) { return; }
                    v.typing.remove(&peer);
                }
                refresh_view(&handle);
            });
        }
        Event::Incoming { owner, peer, .. } => {
            let mut v = r.view.lock().unwrap();
            if v.owner == *owner { v.typing.remove(peer); }
            drop(v); changed(app);
        }
        Event::Chat { .. } | Event::Directory | Event::Hub { .. } => changed(app),
        _ => {}
    }
}

fn refresh_view(app: &AppHandle) {
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || {
        if crate::tray_popup::is_recent(&handle) { crate::tray_popup::refresh(&handle); }
    });
}

pub fn blur(app: &AppHandle) {
    if !crate::tray_popup::is_recent(app) { return; }
    let over = app.tray_by_id("main").and_then(|t|t.rect().ok().flatten())
        .zip(app.cursor_position().ok()).is_some_and(|(rect,p)| {
            let pos = rect.position.to_physical::<f64>(1.);
            let size = rect.size.to_physical::<f64>(1.);
            p.x >= pos.x && p.x < pos.x+size.width && p.y >= pos.y && p.y < pos.y+size.height
        });
    if over { if let Some(r) = app.try_state::<Recent>() { r.view.lock().unwrap().clicks.blurred = Some(Instant::now()); } }
}

pub fn left_click(app: &AppHandle, position: PhysicalPosition<f64>) {
    let r = app.state::<Recent>(); let now = Instant::now();
    let mut v = r.view.lock().unwrap();
    let Some((click,toggle)) = v.clicks.left(now, crate::tray_popup::is_recent(app)) else { return; };
    drop(v);
    if toggle { crate::tray_popup::close(app); return; }
    let wait = unsafe { windows_sys::Win32::UI::WindowsAndMessaging::GetDoubleClickTime() } as u64;
    let handle = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_millis(wait)).await;
        let handle2 = handle.clone();
        let _ = handle.run_on_main_thread(move || {
            let current = handle2.state::<Recent>().view.lock().unwrap().clicks.serial;
            if click != current { return; }
            sync(&handle2);
            let _ = crate::tray_popup::open_recent(&handle2, position);
        });
    });
}

pub fn double_click(app: &AppHandle) {
    let r = app.state::<Recent>(); let mut v = r.view.lock().unwrap();
    v.clicks.double(Instant::now(), Duration::from_millis(unsafe { windows_sys::Win32::UI::WindowsAndMessaging::GetDoubleClickTime() } as u64));
    drop(v); crate::tray_popup::close(app); crate::desktop::show(app);
}

pub fn cancel_click(app: &AppHandle) {
    if let Some(r) = app.try_state::<Recent>() { r.view.lock().unwrap().clicks.cancel(); }
}

pub fn open_chat(app: &AppHandle, owner: &str, peer: &str) -> Result<(), String> {
    core()?.tray_with_identity(Some(owner), |_| {
        let (chats, _) = core()?.store.tray_overview().map_err(|e|e.to_string())?;
        if !chats.iter().any(|c|c.peer == peer) { return Err("Chat list changed".into()); }
        crate::desktop::open_chat_from_tray(app, peer); Ok(())
    })
}

#[tauri::command]
pub async fn hc_tray_recent_picture(window: WebviewWindow, app: AppHandle, generation: u32, peer: String, key: String) -> Result<Option<String>, String> {
    if window.label() != "tray-menu" { return Err("Not a tray popup".into()); }
    let owner = crate::tray_popup::recent_owner(&app, generation)?;
    let (_, rows) = read(&app)?;
    if !rows.iter().any(|r|r.peer == peer && r.kind != "chat" && r.picture_key.as_ref() == Some(&key)) { return Ok(None); }
    let c = core()?; let e = c.engine()?;
    if e.me().address() != owner { return Err("Identity changed".into()); }
    let fetching = peer.clone();
    let _ = c.rt.spawn(async move { e.picture(&fetching).await.map_err(|e|e.to_string()) }).await.map_err(|e|e.to_string())??;
    if crate::tray_popup::recent_owner(&app, generation)? != owner { return Err("Popup changed".into()); }
    c.tray_with_identity(Some(&owner), |_| {
        let meta = c.store.picture_meta(&peer).map_err(|e|e.to_string())?;
        if meta.as_ref().and_then(|m|serde_json::to_string(m).ok()).as_ref() != Some(&key) { return Err("Picture changed".into()); }
        Ok(c.store.picture(&peer).map_err(|e|e.to_string())?.and_then(|p|p.data))
    })
}

/// Same geometry/colours as Orgtree's tray-dot.ts; Tauri uses straight RGBA.
pub fn attention_dot(bytes: &[u8], width: u32, height: u32) -> Vec<u8> {
    let mut out = bytes.to_vec(); let size = width.min(height) as f64;
    let radius = 2.5f64.max(size * 0.22); let gap = 0.5f64.max(size * 0.03);
    let (cx,cy,ring) = (width as f64-radius-gap, radius+gap, 1f64.max(size*0.06));
    for y in 0..height { for x in 0..width {
        let d = (x as f64+0.5-cx).hypot(y as f64+0.5-cy);
        let coverage = (radius+0.5-d).clamp(0.,1.); if coverage == 0. { continue; }
        let colour = if d > radius-ring { [242.,242.,242.] } else { [229.,72.,77.] };
        let at = ((y*width+x)*4) as usize;
        let alpha = out[at+3] as f64/255.; let next = coverage + alpha*(1.-coverage);
        for c in 0..3 { out[at+c] = ((colour[c]*coverage + out[at+c] as f64*alpha*(1.-coverage))/next).round() as u8; }
        out[at+3] = (255.*next).round() as u8;
    }} out
}

fn set_dot(app: &AppHandle, dot: bool) {
    if let (Some(tray),Some(base)) = (app.tray_by_id("main"), app.default_window_icon()) {
        let icon = if dot { tauri::image::Image::new_owned(attention_dot(base.rgba(),base.width(),base.height()),base.width(),base.height()) } else { base.clone() };
        let _ = tray.set_icon(Some(icon));
    }
}

#[cfg(test)]
mod tests {
    use super::{attention_dot, Clicks, Instant, Duration, typing_live};
    #[test]
    fn heartbeat_expires_at_one_second_without_a_stop_event() {
        let at=Instant::now();assert!(typing_live(at,at+Duration::from_millis(999)));
        assert!(!typing_live(at,at+Duration::from_secs(1)));
        assert!(!typing_live(at,at+Duration::from_secs(10)));
        assert!(typing_live(at+Duration::from_secs(2),at+Duration::from_millis(2500)));
    }
    #[test]
    fn double_click_invalidates_pending_single_and_suppresses_the_second_release() {
        let mut c = Clicks::default(); let now = Instant::now();
        let (first,toggle) = c.left(now,false).unwrap(); assert!(!toggle);
        c.double(now+Duration::from_millis(80), Duration::from_millis(500));
        assert_ne!(first,c.serial);
        assert!(c.left(now+Duration::from_millis(120),false).is_none());
        assert!(c.left(now+Duration::from_millis(900),false).is_some());
    }
    #[test]
    fn toggle_consumes_only_an_immediate_tray_blur_and_cancels_a_pending_open() {
        let mut c = Clicks::default(); let now = Instant::now();
        let (pending,_) = c.left(now,false).unwrap(); c.cancel(); assert_ne!(pending,c.serial);
        c.blurred=Some(now);
        assert!(c.left(now+Duration::from_millis(100),false).unwrap().1);
        assert!(!c.left(now+Duration::from_millis(200),false).unwrap().1);
        c.blurred=Some(now);
        assert!(!c.left(now+Duration::from_secs(1),false).unwrap().1);
        assert!(c.left(now+Duration::from_secs(2),true).unwrap().1);
    }
    #[test]
    fn dot_is_rgba_red_with_a_light_rim_and_preserves_the_rest() {
        let base = vec![0u8;16*16*4]; let marked = attention_dot(&base,16,16);
        assert_eq!(&marked[0..4], &[0,0,0,0]);
        assert!(marked.chunks_exact(4).any(|p|p == [229,72,77,255]));
        assert!(marked.chunks_exact(4).any(|p|p == [242,242,242,255]));
        assert_eq!(base, vec![0;16*16*4]);
    }
}
