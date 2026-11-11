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

/// The taskbar button's hover text says why the dot is on (docket hubchat-notification-pip-stuck-on-with-no-unread).
pub fn hint(unread: u64, questions: u64) -> String {
    let mut parts = vec![];
    if questions > 0 { parts.push(format!("{questions} question{} await{} your answer", if questions == 1 { "" } else { "s" }, if questions == 1 { "s" } else { "" })); }
    if unread > 0 { parts.push(format!("{unread} unread")); }
    if parts.is_empty() { "Hubchat".into() } else { format!("Hubchat \u{2014} {}", parts.join(" \u{b7} ")) }
}

pub fn set_hint(app: &AppHandle, text: &str) {
    if let Some(window) = app.get_webview_window("main") {
        if window.title().map(|t| t != text).unwrap_or(true) && window.set_title(text).is_err() {
            tracing::warn!("could not update the taskbar hover text");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::hint;
    #[test]
    fn the_hover_text_says_why_the_dot_is_on() {
        assert_eq!(hint(0, 0), "Hubchat");
        assert_eq!(hint(0, 1), "Hubchat \u{2014} 1 question awaits your answer");
        assert_eq!(hint(0, 2), "Hubchat \u{2014} 2 questions await your answer");
        assert_eq!(hint(3, 1), "Hubchat \u{2014} 1 question awaits your answer \u{b7} 3 unread");
        assert_eq!(hint(5, 0), "Hubchat \u{2014} 5 unread");
    }
}
