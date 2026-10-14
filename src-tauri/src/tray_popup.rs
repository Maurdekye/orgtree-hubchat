//! Windows tray presentation. The native menu remains the single state/action
//! authority. One lazy, hidden, idle WebView hosts tray surfaces; it never boots
//! the main UI, reads messages or gets filesystem/updater capabilities.
use serde::Serialize;
use std::sync::Mutex;
use tauri::menu::MenuItemKind;
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, WebviewWindow, WebviewWindowBuilder, WebviewUrl, Wry};

const LABEL: &str = "tray-menu";
type R<T> = Result<T, String>;

#[derive(Default)]
pub struct Popup(Mutex<Session>);

#[derive(Default, Clone)]
struct Session {
    generation: u32,
    revision: u32,
    open: bool,
    shown: bool,
    anchor: (f64, f64),
    work: (i32, i32, u32, u32),
    scale: f64,
    left: bool,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Row {
    id: String,
    label: String,
    enabled: bool,
    separator: bool,
    checked: Option<bool>,
    children: Vec<Row>,
}

fn rows(items: Vec<MenuItemKind<Wry>>) -> tauri::Result<Vec<Row>> {
    items.into_iter().map(|item| {
        let id = item.id().as_ref().to_owned();
        let mut row = Row { id, label: String::new(), enabled: false, separator: false, checked: None, children: vec![] };
        match item {
            MenuItemKind::MenuItem(i) => { row.label = i.text()?; row.enabled = i.is_enabled()?; }
            MenuItemKind::Check(i) => { row.label = i.text()?; row.enabled = i.is_enabled()?; row.checked = Some(i.is_checked()?); }
            MenuItemKind::Submenu(i) => { row.label = i.text()?; row.enabled = i.is_enabled()?; row.children = rows(i.items()?)?; }
            MenuItemKind::Predefined(_) => row.separator = true, // tray only creates separators
            MenuItemKind::Icon(i) => { row.label = i.text()?; row.enabled = i.is_enabled()?; }
        }
        Ok(row)
    }).collect()
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    generation: u32,
    revision: u32,
    rows: Vec<Row>,
    dark: bool,
    submenu_left: bool,
    max_width: f64,
    max_height: f64,
}

fn dark() -> bool {
    use winreg::{enums::HKEY_CURRENT_USER, RegKey};
    // Same Windows *app* theme signal Electron follows; independent of the
    // main Hubchat window's manually selected colour scheme.
    RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey(r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize")
        .and_then(|k| k.get_value::<u32, _>("AppsUseLightTheme"))
        .is_ok_and(|light| light == 0)
}

fn snapshot(app: &AppHandle) -> R<Snapshot> {
    let p = app.try_state::<Popup>().ok_or("Tray menu is unavailable")?;
    let s = {
        let mut s = p.0.lock().map_err(|_| "Tray menu is unavailable")?;
        if !s.open { return Err("Tray menu is closed".into()); }
        s.revision = s.revision.wrapping_add(1);
        s.clone()
    };
    let t = app.state::<crate::tray::Tray>();
    let rows = rows(t.menu.items().map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    Ok(Snapshot { generation: s.generation, revision: s.revision, rows,
        dark: dark(), submenu_left: s.left,
        max_width: s.work.2 as f64 / s.scale, max_height: s.work.3 as f64 / s.scale })
}

/// Publish only while open, after the existing native item writers finish.
pub fn refresh(app: &AppHandle) {
    if let Ok(view) = snapshot(app) {
        if let Some(w) = app.get_webview_window(LABEL) { let _ = w.emit("hc-tray-popup", view); }
    }
}

fn caller(w: &WebviewWindow) -> R<()> {
    if w.label() == LABEL { Ok(()) } else { Err("Not a tray popup".into()) }
}

/// Application commands are otherwise available to every local WebView.
/// Keep the auxiliary surface out of the main window's command namespace.
pub fn allowed_command(label: &str, command: &str) -> bool {
    label != LABEL || matches!(command, "hc_tray_popup_snapshot" | "hc_tray_popup_ready" |
        "hc_tray_popup_close" | "hc_tray_popup_action")
}

pub fn close(app: &AppHandle) {
    if let Some(p) = app.try_state::<Popup>() {
        if let Ok(mut s) = p.0.lock() { s.open = false; s.shown = false; }
    }
    if let Some(w) = app.get_webview_window(LABEL) { let _ = w.hide(); }
}

pub fn open(app: &AppHandle, point: PhysicalPosition<f64>) -> R<()> {
    let monitor = app.monitor_from_point(point.x, point.y).map_err(|e| e.to_string())?
        .ok_or("No monitor at tray icon")?;
    let r = monitor.work_area();
    {
        let p = app.state::<Popup>();
        let mut s = p.0.lock().map_err(|_| "Tray menu is unavailable")?;
        s.generation = s.generation.wrapping_add(1);
        s.revision = 0;
        s.open = true;
        s.shown = false;
        s.anchor = (point.x, point.y);
        s.work = (r.position.x, r.position.y, r.size.width, r.size.height);
        s.scale = monitor.scale_factor();
        s.left = point.x > r.position.x as f64 + r.size.width as f64 / 2.;
    }
    if let Some(w) = app.get_webview_window(LABEL) {
        let _ = w.hide(); // layout and populate before showing; no old-state flash
        refresh(app);
    } else {
        let built = WebviewWindowBuilder::new(app, LABEL, WebviewUrl::App("tray-menu.html".into()))
            .title("Hubchat tray menu").visible(false).focused(false)
            .decorations(false).resizable(false).skip_taskbar(true)
            .always_on_top(true).shadow(false).transparent(true)
            .inner_size(640., 480.)
            .on_navigation(|url| url.path() == "/tray-menu.html" &&
                (url.scheme() == "tauri" || matches!(url.host_str(), Some("localhost" | "127.0.0.1" | "tauri.localhost"))))
            .build();
        let w = match built { Ok(w) => w, Err(e) => { close(app); return Err(e.to_string()); } };
        let handle = app.clone();
        w.on_window_event(move |event| match event {
            tauri::WindowEvent::Focused(false) => {
                if let Some(w) = handle.get_webview_window(LABEL) {
                    let shown = handle.state::<Popup>().0.lock().map(|s| s.shown).unwrap_or(false);
                    if shown && !w.is_focused().unwrap_or(false) { close(&handle); }
                }
            }
            tauri::WindowEvent::CloseRequested { api, .. } => { api.prevent_close(); close(&handle); }
            tauri::WindowEvent::ThemeChanged(_) => refresh(&handle),
            _ => {}
        });
    }
    Ok(())
}

#[tauri::command]
pub fn hc_tray_popup_snapshot(window: WebviewWindow, app: AppHandle) -> R<Snapshot> {
    caller(&window)?;
    snapshot(&app)
}

/// Logical WebView measurements become physical desktop bounds using the
/// monitor under the tray click, including negative monitor coordinates.
pub fn placement(work: (i32, i32, u32, u32), anchor: (f64, f64), scale: f64,
    width: f64, height: f64, root_x: f64) -> (i32, i32, u32, u32) {
    let w = (width * scale).ceil().clamp(1., work.2 as f64) as u32;
    let h = (height * scale).ceil().clamp(1., work.3 as f64) as u32;
    let x = (anchor.0 - root_x * scale).round()
        .clamp(work.0 as f64, work.0 as f64 + (work.2 - w) as f64) as i32;
    let below = anchor.1 + h as f64 <= work.1 as f64 + work.3 as f64;
    let y = (if below { anchor.1 } else { anchor.1 - h as f64 }).round()
        .clamp(work.1 as f64, work.1 as f64 + (work.3 - h) as f64) as i32;
    (x, y, w, h)
}

#[tauri::command]
pub fn hc_tray_popup_ready(window: WebviewWindow, app: AppHandle, generation: u32, revision: u32,
    width: f64, height: f64, root_x: f64) -> R<()> {
    caller(&window)?;
    if ![width, height, root_x].iter().all(|v| v.is_finite()) || width <= 0. || height <= 0.
        || root_x < 0. || root_x >= width { return Err("Invalid tray geometry".into()); }
    let s = app.state::<Popup>().0.lock().map_err(|_| "Tray menu is unavailable")?.clone();
    if !s.open || s.generation != generation || s.revision != revision { return Err("Tray menu changed".into()); }
    let (x, y, w, h) = placement(s.work, s.anchor, s.scale, width, height, root_x);
    window.set_position(PhysicalPosition::new(x, y)).map_err(|e| e.to_string())?;
    window.set_size(PhysicalSize::new(w, h)).map_err(|e| e.to_string())?;
    window.show().map_err(|e| e.to_string())?;
    window.set_focus().map_err(|e| e.to_string())?;
    app.state::<Popup>().0.lock().map_err(|_| "Tray menu is unavailable")?.shown = true;
    Ok(())
}

#[tauri::command]
pub fn hc_tray_popup_close(window: WebviewWindow, app: AppHandle, generation: u32) -> R<()> {
    caller(&window)?;
    let current = app.state::<Popup>().0.lock().map_err(|_| "Tray menu is unavailable")?.generation == generation;
    if current { close(&app); }
    Ok(())
}

// Re-resolve the item through the live native tree (including enabled parents).
// A renderer cannot activate a hidden or newly disabled update action, or
// bypass the updater dispatcher by supplying a made-up ID.
fn activate(items: Vec<MenuItemKind<Wry>>, id: &str, checked: Option<bool>) -> tauri::Result<bool> {
    for item in items {
        match item {
            MenuItemKind::Submenu(i) if i.is_enabled()? => {
                if activate(i.items()?, id, checked)? { return Ok(true); }
            }
            MenuItemKind::MenuItem(i) if i.id().as_ref() == id && i.is_enabled()? && checked.is_none() => return Ok(true),
            MenuItemKind::Check(i) if i.id().as_ref() == id && i.is_enabled()? => {
                if let Some(on) = checked { i.set_checked(on)?; return Ok(true); }
            }
            _ => {}
        }
    }
    Ok(false)
}

#[tauri::command]
pub fn hc_tray_popup_action(window: WebviewWindow, app: AppHandle, generation: u32,
    id: String, checked: Option<bool>) -> R<()> {
    caller(&window)?;
    {
        let p = app.state::<Popup>();
        let mut s = p.0.lock().map_err(|_| "Tray menu is unavailable")?;
        if !s.open || !s.shown || s.generation != generation { return Err("Tray menu changed".into()); }
        // One activation per open, including duplicate Enter/clicks.
        s.open = false;
    }
    let t = app.state::<crate::tray::Tray>();
    let result = activate(t.menu.items().map_err(|e| e.to_string())?, &id, checked).map_err(|e| e.to_string());
    close(&app);
    if !result? { return Err("Tray item is unavailable".into()); }
    crate::tray::on_menu(&app, &id);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{placement, allowed_command};

    #[test]
    fn popup_has_no_main_window_command_authority() {
        for command in ["hc_state", "hc_send", "hc_app_update_install", "hc_set_autostart", "hc_file_preview"] {
            assert!(!allowed_command("tray-menu", command));
            assert!(allowed_command("main", command));
        }
        for command in ["hc_tray_popup_snapshot", "hc_tray_popup_ready", "hc_tray_popup_close", "hc_tray_popup_action"] {
            assert!(allowed_command("tray-menu", command));
        }
    }

    #[test]
    fn bounds_cover_all_taskbar_edges_and_negative_monitors_at_scaled_dpi() {
        for scale in [1., 1.25, 1.5, 2.] {
            for work in [(0, 0, 1920, 1040), (-1920, -200, 1880, 1080), (40, 48, 1200, 800)] {
                for anchor in [(work.0 as f64, work.1 as f64), (work.0 as f64 + work.2 as f64, work.1 as f64),
                    (work.0 as f64, work.1 as f64 + work.3 as f64),
                    (work.0 as f64 + work.2 as f64, work.1 as f64 + work.3 as f64)] {
                    let (x, y, w, h) = placement(work, anchor, scale, 640., 520., 330.);
                    assert!(x >= work.0 && y >= work.1);
                    assert!(x as i64 + w as i64 <= work.0 as i64 + work.2 as i64);
                    assert!(y as i64 + h as i64 <= work.1 as i64 + work.3 as i64);
                }
            }
        }
    }
}
