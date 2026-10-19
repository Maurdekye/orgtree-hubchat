//! Native taskbar overlay; called by the existing identity-bound tray publisher.
use tauri::{AppHandle, Manager};

pub fn set_dot(app: &AppHandle, on: bool) -> bool {
    let Some(window) = app.get_webview_window("main") else { return false; };
    let icon = on.then(|| tauri::image::Image::new_owned(
        crate::taskbar_dot::rgba(), crate::taskbar_dot::SIDE, crate::taskbar_dot::SIDE));
    if window.set_overlay_icon(icon).is_err() {
        tracing::warn!("could not update taskbar attention");
        return false;
    }
    true
}
