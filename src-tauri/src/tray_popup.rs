//! Windows tray presentation. The native menu remains the single state/action
//! authority. One pre-created, hidden, idle WebView hosts tray surfaces; it never boots
//! the main UI or gets filesystem/updater capabilities. The recent-chat
//! surface receives bounded native metadata, never message contents.
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
    recent: bool,
    owner: String,
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

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    generation: u32,
    revision: u32,
    rows: Vec<Row>,
    dark: bool,
    submenu_left: bool,
    max_width: f64,
    max_height: f64,
    cursor_x: f64,
    cursor_y: f64,
    kind: &'static str,
    recent: Vec<crate::tray_recent::RecentRow>,
    recent_available: bool,
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

fn snapshot(app: &AppHandle, warm: bool) -> R<Snapshot> {
    let p = app.try_state::<Popup>().ok_or("Tray menu is unavailable")?;
    let mut s = {
        let mut s = p.0.lock().map_err(|_| "Tray menu is unavailable")?;
        if !s.open && !warm { return Err("Tray menu is closed".into()); }
        s.revision = s.revision.wrapping_add(1);
        s.clone()
    };
    if !s.open { s.recent = false; s.owner.clear(); }
    let (rows, recent) = if s.recent {
        let (owner, recent) = crate::tray_recent::read(app)?;
        if owner != s.owner { return Err("Identity changed".into()); }
        (vec![], recent)
    } else {
        let t = app.state::<crate::tray::Tray>();
        (rows(t.menu.items().map_err(|e| e.to_string())?).map_err(|e| e.to_string())?, vec![])
    };
    Ok(Snapshot { generation: s.generation, revision: s.revision, rows,
        dark: dark(), submenu_left: s.left,
        max_width: s.work.2 as f64 / s.scale, max_height: s.work.3 as f64 / s.scale,
        cursor_x: (s.anchor.0 - s.work.0 as f64) / s.scale,
        cursor_y: (s.anchor.1 - s.work.1 as f64) / s.scale,
        kind: if s.recent { "recent" } else { "menu" }, recent,
        recent_available: crate::tray_recent::available(app) })
}

/// Publish only while open, after the existing native item writers finish.
pub fn refresh(app: &AppHandle) {
    if let Ok(view) = snapshot(app, false) {
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
        "hc_tray_popup_close" | "hc_tray_popup_action" | "hc_tray_recent_picture")
}

pub fn close(app: &AppHandle) {
    if let Some(p) = app.try_state::<Popup>() {
        if let Ok(mut s) = p.0.lock() { s.open = false; s.shown = false; }
    }
    if let Some(w) = app.get_webview_window(LABEL) { let _ = w.hide(); }
}

pub fn open(app: &AppHandle, point: PhysicalPosition<f64>) -> R<()> {
    open_surface(app, point, false, String::new())
}

pub fn open_recent(app: &AppHandle, point: PhysicalPosition<f64>) -> R<()> {
    let (owner, _) = crate::tray_recent::read(app)?;
    open_surface(app, point, true, owner)
}

pub fn is_recent(app: &AppHandle) -> bool {
    app.try_state::<Popup>().is_some_and(|p|p.0.lock().map(|s|s.open && s.recent).unwrap_or(false))
}

pub fn close_recent(app: &AppHandle) { if is_recent(app) { close(app); } }

pub fn recent_owner(app: &AppHandle, generation: u32) -> R<String> {
    let p = app.state::<Popup>(); let s = p.0.lock().map_err(|_| "Tray unavailable")?;
    if s.open && s.recent && s.generation == generation { Ok(s.owner.clone()) } else { Err("Popup changed".into()) }
}

fn open_surface(app: &AppHandle, point: PhysicalPosition<f64>, recent: bool, owner: String) -> R<()> {
    let monitor = app.monitor_from_point(point.x, point.y).map_err(|e| e.to_string())?
        .ok_or("No monitor at tray icon")?;
    let r = monitor.work_area();
    let work = (r.position.x, r.position.y, r.size.width, r.size.height);
    let full = (monitor.position().x, monitor.position().y, monitor.size().width, monitor.size().height);
    {
        let p = app.state::<Popup>();
        let mut s = p.0.lock().map_err(|_| "Tray menu is unavailable")?;
        s.generation = s.generation.wrapping_add(1);
        s.revision = 0;
        s.open = true;
        s.shown = false;
        s.anchor = (point.x, point.y);
        s.work = if recent { work } else { menu_bounds((point.x, point.y), work, full) };
        s.scale = monitor.scale_factor();
        s.left = point.x > r.position.x as f64 + r.size.width as f64 / 2.;
        s.recent = recent;
        s.owner = owner;
    }
    let w = ensure_window(app)?;
    let _ = w.hide(); // layout and populate before showing; no old-state flash
    refresh(app);
    Ok(())
}

/// Pay WebView creation/navigation at startup, never at the first tray click.
/// The initial snapshot renders while hidden; ready still requires an open generation.
pub fn prepare(app: &AppHandle) -> R<()> {
    let monitor = app.primary_monitor().map_err(|e|e.to_string())?.ok_or("No monitor for tray warm-up")?;
    {
        let r = monitor.work_area();
        let p = app.state::<Popup>();
        let mut s = p.0.lock().map_err(|_| "Tray menu is unavailable")?;
        if !s.open {
            s.work = (r.position.x, r.position.y, r.size.width, r.size.height);
            s.scale = monitor.scale_factor();
        }
    }
    ensure_window(app).map(|_| ())
}

fn ensure_window(app: &AppHandle) -> R<WebviewWindow> {
    if let Some(w) = app.get_webview_window(LABEL) { return Ok(w); }
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
                    if shown && !w.is_focused().unwrap_or(false) {
                        crate::tray_recent::blur(&handle);
                        close(&handle);
                    }
                }
            }
            tauri::WindowEvent::CloseRequested { api, .. } => { api.prevent_close(); close(&handle); }
            tauri::WindowEvent::ThemeChanged(_) => refresh(&handle),
            _ => {}
        });
    Ok(w)
}

#[tauri::command]
pub fn hc_tray_popup_snapshot(window: WebviewWindow, app: AppHandle) -> R<Snapshot> {
    caller(&window)?;
    snapshot(&app, true)
}

/// The rectangle the right-click menu is placed in. Chromium's menus (and so
/// Orgtree's tray menu) use the work area, or the whole monitor when the
/// anchor lies outside the work area but on the monitor. A tray click always
/// does: the cursor is on the taskbar. Clamping to the work area there kept
/// the menu above the taskbar instead of at the cursor.
pub fn menu_bounds(anchor: (f64, f64), work: (i32, i32, u32, u32), monitor: (i32, i32, u32, u32))
    -> (i32, i32, u32, u32) {
    let inside = |r: (i32, i32, u32, u32)| anchor.0 >= r.0 as f64 && anchor.0 < r.0 as f64 + r.2 as f64
        && anchor.1 >= r.1 as f64 && anchor.1 < r.1 as f64 + r.3 as f64;
    if !inside(work) && inside(monitor) { monitor } else { work }
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

fn cursor_placement(work: (i32, i32, u32, u32), scale: f64,
    width: f64, height: f64, origin: (f64, f64)) -> (i32, i32, u32, u32) {
    let w = (width * scale).ceil().clamp(1., work.2 as f64) as u32;
    let h = (height * scale).ceil().clamp(1., work.3 as f64) as u32;
    let x = (work.0 as f64 + origin.0 * scale).round()
        .clamp(work.0 as f64, work.0 as f64 + (work.2-w) as f64) as i32;
    let y = (work.1 as f64 + origin.1 * scale).round()
        .clamp(work.1 as f64, work.1 as f64 + (work.3-h) as f64) as i32;
    (x,y,w,h)
}

#[tauri::command]
pub fn hc_tray_popup_ready(window: WebviewWindow, app: AppHandle, generation: u32, revision: u32,
    width: f64, height: f64, root_x: f64, origin_x: Option<f64>, origin_y: Option<f64>) -> R<()> {
    caller(&window)?;
    if ![width, height, root_x].iter().all(|v| v.is_finite()) || width <= 0. || height <= 0.
        || root_x < 0. || root_x >= width { return Err("Invalid tray geometry".into()); }
    let s = app.state::<Popup>().0.lock().map_err(|_| "Tray menu is unavailable")?.clone();
    if !s.open || s.generation != generation || s.revision != revision { return Err("Tray menu changed".into()); }
    let (x, y, w, h) = if s.recent {
        placement(s.work, s.anchor, s.scale, width, height, root_x)
    } else {
        let (ox, oy) = origin_x.zip(origin_y).ok_or("Missing cursor geometry")?;
        if !ox.is_finite() || !oy.is_finite() || ox < 0. || oy < 0. {
            return Err("Invalid cursor geometry".into());
        }
        // Root pane is already cursor-clamped independently by the renderer.
        // Clamp the transparent submenu envelope without moving that root.
        cursor_placement(s.work, s.scale, width, height, (ox,oy))
    };
    let show = || {
        window.set_position(PhysicalPosition::new(x, y)).map_err(|e| e.to_string())?;
        window.set_size(PhysicalSize::new(w, h)).map_err(|e| e.to_string())?;
        window.show().map_err(|e| e.to_string())?;
        window.set_focus().map_err(|e| e.to_string())
    };
    if s.recent && !s.owner.is_empty() { crate::core::get()?.tray_with_identity(Some(&s.owner), |_|show())?; }
    else { show()?; }
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
    let session = {
        let p = app.state::<Popup>();
        let mut s = p.0.lock().map_err(|_| "Tray menu is unavailable")?;
        if !s.open || !s.shown || s.generation != generation { return Err("Tray menu changed".into()); }
        // One activation per open, including duplicate Enter/clicks.
        s.open = false;
        s.clone()
    };
    if session.recent {
        close(&app);
        if checked.is_some() { return Err("Invalid chat action".into()); }
        return crate::tray_recent::open_chat(&app, &session.owner, &id);
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
    use super::{placement, cursor_placement, allowed_command, menu_bounds};

    fn contains(r: (i32, i32, u32, u32), p: (f64, f64)) -> bool {
        p.0 >= r.0 as f64 && p.0 < r.0 as f64 + r.2 as f64 && p.1 >= r.1 as f64 && p.1 < r.1 as f64 + r.3 as f64
    }

    /// A tray click lands on the taskbar, outside the work area. Like
    /// Chromium's menus (Orgtree's tray), the menu then uses the whole
    /// monitor so its corner can sit on the cursor; clamping to the work
    /// area left it floating above the taskbar, off the cursor.
    #[test]
    fn taskbar_cursor_keeps_the_menu_corner_on_the_cursor_at_every_edge_and_scale() {
        for scale in [1f64, 1.25, 1.5, 1.75] {
            for (mx, my) in [(0, 0), (-2560, -300), (1920, 0)] {
                let (mw, mh) = (2560u32, 1440u32);
                let bar = (48. * scale).round() as u32;
                let mid = |a: i32, len: u32| a as f64 + len as f64 * 0.8;
                // (work area, cursor in the taskbar) for bottom, top, left, right
                let edges = [
                    ((mx, my, mw, mh - bar), (mid(mx, mw), (my + mh as i32) as f64 - bar as f64 / 2.)),
                    ((mx, my + bar as i32, mw, mh - bar), (mid(mx, mw), my as f64 + bar as f64 / 2.)),
                    ((mx + bar as i32, my, mw - bar, mh), (mx as f64 + bar as f64 / 2., mid(my, mh))),
                    ((mx, my, mw - bar, mh), ((mx + mw as i32) as f64 - bar as f64 / 2., mid(my, mh))),
                ];
                for (work, cursor) in edges {
                    let b = menu_bounds(cursor, work, (mx, my, mw, mh));
                    assert!(contains(b, cursor), "cursor {cursor:?} outside bounds {b:?} at scale {scale}");
                    // The renderer flips left/up when the root cannot fit
                    // right/below (traymenu-view.ts); its corner stays on the
                    // cursor only if no edge clamp then moves it.
                    let (cx, cy) = ((cursor.0 - b.0 as f64) / scale, (cursor.1 - b.1 as f64) / scale);
                    let (maxw, maxh) = (b.2 as f64 / scale, b.3 as f64 / scale);
                    let (menu_w, menu_h) = (312., 375.);
                    let x = if cx + menu_w <= maxw - 12. { cx } else { cx - menu_w };
                    let y = if cy + menu_h <= maxh - 8. { cy } else { cy - menu_h };
                    assert!(x >= 12. && x + menu_w <= maxw - 12., "x {x} clamped at scale {scale}");
                    assert!(y >= 8. && y + menu_h <= maxh - 8., "y {y} clamped at scale {scale}");
                }
            }
        }
    }

    #[test]
    fn cursor_inside_the_work_area_keeps_the_work_area() {
        let work = (0, 0, 1920, 1032);
        assert_eq!(menu_bounds((900., 500.), work, (0, 0, 1920, 1080)), work);
        // No monitor contains it either: nothing better than the work area.
        assert_eq!(menu_bounds((5000., 500.), work, (0, 0, 1920, 1080)), work);
    }

    #[test]
    fn cursor_window_origin_preserves_monitor_coordinates_and_dpi() {
        assert_eq!(cursor_placement((-1920,-200,1920,1040),1.5,648.,420.,(200.,100.)),
            (-1620,-50,972,630));
        assert_eq!(cursor_placement((40,48,1200,800),2.,648.,420.,(1100.,700.)),
            (40,48,1200,800));
    }

    #[test]
    fn flipped_cursor_corner_survives_physical_window_rounding_at_scaled_origins() {
        for scale in [1., 1.25, 1.5] {
            for work in [(0, 0, 1920, 1040), (-1920, -200, 1920, 1040), (40, -1080, 1200, 800)] {
                let max_x = work.2 as f64 / scale;
                let max_y = work.3 as f64 / scale;
                for offset in [32., 100., 200.] {
                    let cursor = (max_x - offset, max_y - 32.);
                    // The DOM flips the visible 312x375 root left/up, then
                    // reserves the 648x391 submenu envelope around it.
                    let root = (cursor.0 - 312., cursor.1 - 375.);
                    let origin = ((root.0 - 324.).clamp(0., max_x - 648.),
                        (root.1 - 8.).clamp(0., max_y - 391.));
                    let (x,y,w,h) = cursor_placement(work, scale, 648., 391., origin);
                    let right = x as f64 + (root.0 - origin.0 + 312.) * scale;
                    let bottom = y as f64 + (root.1 - origin.1 + 375.) * scale;
                    assert!((right - (work.0 as f64 + cursor.0 * scale)).abs() <= 0.501);
                    assert!((bottom - (work.1 as f64 + cursor.1 * scale)).abs() <= 0.501);
                    assert!(x >= work.0 && y >= work.1);
                    assert!(x as i64 + w as i64 <= work.0 as i64 + work.2 as i64);
                    assert!(y as i64 + h as i64 <= work.1 as i64 + work.3 as i64);
                }
            }
        }
    }

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
