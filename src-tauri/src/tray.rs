//! The tray icon and its menu (desktop), in line with Orgtree's tray (docket
//! bring-hubchat-tray-menu-in-line-with-orgtree, user 2026-10-11) but made of
//! Hubchat's own settings and behaviour (lead-astra 2026-10-11 22:36Z):
//!
//! - the version;
//! - updates: the signed updater the UI runs (src/lib/updates.ts) reports its
//!   state here (hc_tray_updates), and the update rows ask it to check or to
//!   install ("hc-tray" events). Nothing installs unless the person asks;
//! - Start with Windows: the autostart plugin, the same switch as Settings ›
//!   General;
//! - Exit on close: closing the window quits Hubchat instead of hiding it in
//!   the tray. Off unless turned on, and kept on this device;
//! - Notifications: Settings › Notifications' three switches;
//! - Crash reports… (crash.rs);
//! - Restart Hubchat and Quit Hubchat.
//!
//! A left click shows the window, as before. The menu is built once and
//! never replaced: whatever changes what a row shows (this menu, Settings,
//! Windows' Startup apps, the updater) updates the same live items, so the
//! menu is right whenever it opens, and an open menu is never torn down
//! under the person's cursor. A setting changed here reaches the UI as
//! "hc-settings".

use std::ffi::OsString;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use serde::Deserialize;
use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, Wry};
use tauri_plugin_autostart::ManagerExt as _;
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};
use tauri_plugin_opener::OpenerExt;

use crate::core;

type R<T> = Result<T, String>;

#[cfg(windows)]
const START_LABEL: &str = "Start with Windows";
#[cfg(not(windows))]
const START_LABEL: &str = "Start at login";

/// Where "Restart to update" goes when an update was found: after the
/// status row (Open, separator, version, status).
const INSTALL_AT: usize = 4;

// ------------------------------------------------------------ exit on close

/// Exit on close, as saved on this device. A device setting, not the
/// identity's: a file of its own in the app data folder, because switching
/// identity empties the store's settings.
static EXIT_ON_CLOSE: AtomicBool = AtomicBool::new(false);
const EXIT_ON_CLOSE_FILE: &str = "exit-on-close";

/// Whether closing the window quits Hubchat (lib.rs: the close handler).
pub fn exit_on_close() -> bool {
    EXIT_ON_CLOSE.load(Ordering::Relaxed)
}

fn load_exit_on_close(dir: &Path) -> bool {
    std::fs::read_to_string(dir.join(EXIT_ON_CLOSE_FILE)).is_ok_and(|s| s.trim() == "on")
}

fn save_exit_on_close(dir: &Path, on: bool) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    std::fs::write(dir.join(EXIT_ON_CLOSE_FILE), if on { "on" } else { "off" })
}

// ------------------------------------------------------------------ updates

/// Where the updater is, as the UI reports it.
#[derive(Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Phase {
    /// Not checked in this run (or only a background check failed, which
    /// stays silent).
    Idle,
    Checking,
    /// The last check found nothing newer.
    None,
    /// A check the person asked for failed.
    Error,
    /// Downloading and installing an update the person asked for.
    Installing,
    /// That install failed; the update can be tried again.
    Failed,
}

/// What the UI reports (src/lib/updates.ts trayView).
#[derive(Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Updates {
    pub phase: Phase,
    /// The newer version a check found, if any.
    pub available: Option<String>,
    /// Check when Hubchat starts and every 6 hours (Settings › About).
    pub auto: bool,
}

/// What the update rows show.
#[derive(Debug, PartialEq, Eq)]
pub struct UpdateRows {
    pub status: String,
    /// "Restart to update" is in the menu (a check found an update)...
    pub install: bool,
    /// ...and can be clicked now.
    pub install_enabled: bool,
    pub check_enabled: bool,
}

/// The update rows for what the UI last reported; `None` before it has (the
/// rows that would ask it for something wait until it can answer).
pub fn update_rows(u: Option<&Updates>) -> UpdateRows {
    let Some(u) = u else {
        return UpdateRows {
            status: "Updates have not been checked".into(),
            install: false,
            install_enabled: false,
            check_enabled: false,
        };
    };
    let busy = matches!(u.phase, Phase::Checking | Phase::Installing);
    let status = match (u.phase, u.available.as_deref()) {
        (Phase::Installing, Some(v)) => format!("Updating to Hubchat {v}…"),
        (Phase::Installing, None) => "Updating Hubchat…".into(),
        (Phase::Checking, _) => "Checking for updates…".into(),
        (Phase::Failed, _) => "The update didn't install".into(),
        (Phase::Error, _) => "Couldn't check for updates".into(),
        (_, Some(v)) => format!("Hubchat {v} is available"),
        (Phase::None, None) => "Hubchat is up to date".into(),
        (Phase::Idle, None) => "Updates have not been checked".into(),
    };
    UpdateRows {
        status,
        install: u.available.is_some(),
        install_enabled: u.available.is_some() && !busy,
        check_enabled: !busy,
    }
}

// --------------------------------------------------------------------- menu

/// The menu's live items.
pub struct Tray {
    menu: Menu<Wry>,
    status: MenuItem<Wry>,
    install: MenuItem<Wry>,
    auto: CheckMenuItem<Wry>,
    check: MenuItem<Wry>,
    autostart: CheckMenuItem<Wry>,
    exit_on_close: CheckMenuItem<Wry>,
    notify_on: CheckMenuItem<Wry>,
    notify_text: CheckMenuItem<Wry>,
    notify_sound: CheckMenuItem<Wry>,
    /// The updater's last report, and whether "Restart to update" is in the
    /// menu now.
    updates: Mutex<(Option<Updates>, bool)>,
}

impl Tray {
    fn show_updates(&self, u: Option<&Updates>, shown: &mut bool) {
        let rows = update_rows(u);
        let _ = self.status.set_text(&rows.status);
        if rows.install != *shown {
            let done = if rows.install {
                self.menu.insert(&self.install, INSTALL_AT)
            } else {
                self.menu.remove(&self.install)
            };
            if done.is_ok() {
                *shown = rows.install;
            }
        }
        let _ = self.install.set_enabled(rows.install_enabled);
        let _ = self.check.set_enabled(rows.check_enabled);
        let _ = self.auto.set_enabled(u.is_some());
        if let Some(u) = u {
            let _ = self.auto.set_checked(u.auto);
        }
    }

    fn show_notifications(&self, n: core::NotifySettings) {
        let _ = self.notify_on.set_checked(n.enabled);
        let _ = self.notify_text.set_checked(n.preview);
        let _ = self.notify_sound.set_checked(n.sound);
        // as Orgtree's: the kinds of notification wait while all are off
        let _ = self.notify_text.set_enabled(n.enabled);
        let _ = self.notify_sound.set_enabled(n.enabled);
    }
}

/// Build the tray icon and its menu (setup, after the core).
pub fn build(app: &AppHandle) -> tauri::Result<()> {
    EXIT_ON_CLOSE.store(load_exit_on_close(&app.path().app_data_dir()?), Ordering::Relaxed);
    let notify = core::get().map(|c| c.notify_settings());
    let n = notify.unwrap_or(core::NotifySettings { enabled: false, preview: false, sound: false });
    let have_core = core::get().is_ok();
    let version = app.package_info().version.to_string();
    let first = update_rows(None);

    let none = None::<&str>;
    let open = MenuItem::with_id(app, "open", "Open Hubchat", true, none)?;
    let version = MenuItem::with_id(app, "version", format!("Version {version}"), false, none)?;
    let status = MenuItem::with_id(app, "update-status", &first.status, false, none)?;
    let install = MenuItem::with_id(app, "update-install", "Restart to update", false, none)?;
    let auto = CheckMenuItem::with_id(app, "update-auto", "Check for updates automatically", false, true, none)?;
    let check = MenuItem::with_id(app, "update-check", "Check for updates", false, none)?;
    let autostart_on = app.autolaunch().is_enabled().unwrap_or(false);
    let autostart = CheckMenuItem::with_id(app, "autostart", START_LABEL, true, autostart_on, none)?;
    let exit = CheckMenuItem::with_id(app, "exit-on-close", "Exit on close", true, exit_on_close(), none)?;
    let notify_on = CheckMenuItem::with_id(app, "notify-on", "Show notifications", have_core, n.enabled, none)?;
    let notify_text = CheckMenuItem::with_id(app, "notify-text", "Show message text", have_core && n.enabled, n.preview, none)?;
    let notify_sound = CheckMenuItem::with_id(app, "notify-sound", "Play a sound", have_core && n.enabled, n.sound, none)?;
    let notifications = Submenu::with_id_and_items(
        app,
        "notifications",
        "Notifications",
        true,
        &[&notify_on, &PredefinedMenuItem::separator(app)?, &notify_text, &notify_sound],
    )?;
    let crash = MenuItem::with_id(app, "crash-reports", "Crash reports…", true, none)?;
    let restart = MenuItem::with_id(app, "restart", "Restart Hubchat", true, none)?;
    let quit = MenuItem::with_id(app, "quit", "Quit Hubchat", true, none)?;
    let menu = Menu::with_items(
        app,
        &[
            &open,
            &PredefinedMenuItem::separator(app)?,
            &version,
            &status,
            &auto,
            &check,
            &PredefinedMenuItem::separator(app)?,
            &autostart,
            &exit,
            &notifications,
            &crash,
            &PredefinedMenuItem::separator(app)?,
            &restart,
            &quit,
        ],
    )?;
    app.manage(Tray {
        menu: menu.clone(),
        status,
        install,
        auto,
        check,
        autostart,
        exit_on_close: exit,
        notify_on,
        notify_text,
        notify_sound,
        updates: Mutex::new((None, false)),
    });
    TrayIconBuilder::with_id("main")
        .icon(app.default_window_icon().cloned().expect("icon"))
        .tooltip("Hubchat")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, ev| on_menu(app, ev.id().as_ref()))
        .on_tray_icon_event(|tray, ev| match ev {
            TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } => crate::desktop::show(tray.app_handle()),
            // Windows' Startup apps can turn Start with Windows off behind
            // our back: read it again whenever the pointer comes to the icon
            TrayIconEvent::Enter { .. } | TrayIconEvent::Click { .. } => {
                show_autostart(tray.app_handle())
            }
            _ => {}
        })
        .build(app)?;
    Ok(())
}

fn on_menu(app: &AppHandle, id: &str) {
    let Some(t) = app.try_state::<Tray>() else {
        return;
    };
    // A check item has already flipped its tick when the click arrives: the
    // tick is what the person asked for, and is set back to what really
    // happened once it has.
    match id {
        "open" => crate::desktop::show(app),
        "update-install" => ask_ui(app, serde_json::json!({ "k": "install" })),
        "update-check" => ask_ui(app, serde_json::json!({ "k": "check" })),
        "update-auto" => {
            let on = t.auto.is_checked().unwrap_or(true);
            ask_ui(app, serde_json::json!({ "k": "auto", "on": on }));
        }
        "autostart" => {
            let want = t.autostart.is_checked().unwrap_or(false);
            match set_autostart(app, want) {
                Ok(_) => settings_changed(app),
                Err(e) => {
                    show_autostart(app);
                    warn(app, format!("Hubchat couldn't change {START_LABEL}: {e}"));
                }
            }
        }
        "exit-on-close" => {
            let want = t.exit_on_close.is_checked().unwrap_or(false);
            match set_exit_on_close(app, want) {
                Ok(_) => settings_changed(app),
                Err(e) => {
                    let _ = t.exit_on_close.set_checked(exit_on_close());
                    warn(app, format!("Hubchat couldn't change Exit on close: {e}"));
                }
            }
        }
        "notify-on" | "notify-text" | "notify-sound" => {
            let r = core::get().and_then(|c| {
                let mut n = c.notify_settings();
                match id {
                    "notify-on" => n.enabled = t.notify_on.is_checked().unwrap_or(n.enabled),
                    "notify-text" => n.preview = t.notify_text.is_checked().unwrap_or(n.preview),
                    _ => n.sound = t.notify_sound.is_checked().unwrap_or(n.sound),
                }
                c.set_notify_settings(n)
            });
            notifications_changed(app);
            match r {
                Ok(()) => settings_changed(app),
                Err(e) => warn(app, format!("Hubchat couldn't change its notifications: {e}")),
            }
        }
        "crash-reports" => show_crash_reports(app),
        "restart" => restart(app),
        "quit" => app.exit(0),
        _ => {}
    }
}

/// The updater runs in the UI: ask it (src/lib/tray.ts).
fn ask_ui(app: &AppHandle, what: serde_json::Value) {
    let _ = app.emit("hc-tray", what);
}

/// The UI re-reads its settings (Settings › General and Notifications).
fn settings_changed(app: &AppHandle) {
    let _ = app.emit("hc-settings", ());
}

fn warn(app: &AppHandle, text: String) {
    app.dialog()
        .message(text)
        .title("Hubchat")
        .kind(MessageDialogKind::Warning)
        .show(|_| {});
}

/// The Notifications submenu shows what is saved (after a change anywhere).
pub fn notifications_changed(app: &AppHandle) {
    if let (Some(t), Ok(c)) = (app.try_state::<Tray>(), core::get()) {
        t.show_notifications(c.notify_settings());
    }
}

fn show_autostart(app: &AppHandle) {
    if let (Some(t), Ok(on)) = (app.try_state::<Tray>(), app.autolaunch().is_enabled()) {
        let _ = t.autostart.set_checked(on);
    }
}

/// Start with Windows on or off; the value now in effect.
fn set_autostart(app: &AppHandle, on: bool) -> R<bool> {
    let a = app.autolaunch();
    if a.is_enabled().map_err(|e| e.to_string())? != on {
        let changed = if on { a.enable() } else { a.disable() };
        changed.map_err(|e| e.to_string())?;
    }
    let now = a.is_enabled().map_err(|e| e.to_string())?;
    if let Some(t) = app.try_state::<Tray>() {
        let _ = t.autostart.set_checked(now);
    }
    Ok(now)
}

fn set_exit_on_close(app: &AppHandle, on: bool) -> R<bool> {
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    save_exit_on_close(&dir, on).map_err(|e| format!("the setting couldn't be saved ({e})"))?;
    EXIT_ON_CLOSE.store(on, Ordering::Relaxed);
    if let Some(t) = app.try_state::<Tray>() {
        let _ = t.exit_on_close.set_checked(on);
    }
    Ok(on)
}

fn show_crash_reports(app: &AppHandle) {
    let Ok(data) = app.path().app_data_dir() else {
        return;
    };
    let folder = crate::diagnostics::crash_reports(&data);
    let reports = crate::crash::count_reports(&folder);
    #[cfg(windows)]
    let webview = app.path().app_local_data_dir().ok().map(|d| {
        let dumps = crate::crash::webview_dumps(&d);
        let n = crate::crash::count_dumps(&dumps);
        (dumps, n)
    });
    #[cfg(not(windows))]
    let webview: Option<(std::path::PathBuf, usize)> = None;
    let text = crate::crash::dialog_text(reports, &folder, webview.as_ref().map(|(d, n)| (d.as_path(), *n)));
    let handle = app.clone();
    app.dialog()
        .message(text)
        .title("Crash reports")
        .kind(MessageDialogKind::Info)
        .buttons(MessageDialogButtons::OkCancelCustom("Open folder".into(), "Close".into()))
        .show(move |open| {
            if open {
                let _ = std::fs::create_dir_all(&folder);
                let _ = handle.opener().open_path(folder.to_string_lossy().into_owned(), None::<&str>);
            }
        });
}

/// What must run before Restart Hubchat restarts, which skips the Exit event
/// (logging flushes its file): set once, by logging.
static BEFORE_RESTART: std::sync::OnceLock<fn()> = std::sync::OnceLock::new();

/// Run `f` just before Restart Hubchat restarts (it must not panic).
#[allow(dead_code)] // logging sets it
pub fn before_restart(f: fn()) {
    let _ = BEFORE_RESTART.set(f);
}

/// What a restart starts the new process with: nothing, or `--hidden` when
/// the window is hidden, so Hubchat comes back as it was. Never the old
/// process's own arguments: a hubchat:// link it was started with (a setup
/// or device code) must not open again.
pub fn restart_args(argv0: OsString, hidden: bool) -> Vec<OsString> {
    let mut args = vec![argv0];
    if hidden {
        args.push("--hidden".into());
    }
    args
}

fn restart(app: &AppHandle) {
    let hidden = app
        .get_webview_window("main")
        .map_or(true, |w| !w.is_visible().unwrap_or(false));
    let mut env = app.env();
    let argv0 = env.args_os.first().cloned().unwrap_or_default();
    env.args_os = restart_args(argv0, hidden);
    if let Some(f) = BEFORE_RESTART.get() {
        f();
    }
    // Free the one-instance lock first. The new process would otherwise find
    // this one still running, hand it its start and quit (Tauri frees it on
    // the Exit event, which a restart from the main thread never sends).
    tauri_plugin_single_instance::destroy(app);
    app.cleanup_before_exit();
    tauri::process::restart(&env);
}

// ----------------------------------------------------------------- commands

/// The updater's state, from the UI (src/lib/tray.ts), each time it changes.
#[tauri::command]
pub fn hc_tray_updates(app: AppHandle, updates: Updates) {
    if let Some(t) = app.try_state::<Tray>() {
        let mut g = t.updates.lock().unwrap();
        let shown = &mut g.1;
        t.show_updates(Some(&updates), shown);
        g.0 = Some(updates);
    }
}

/// Settings › General: Start with Windows on or off; the value now in effect.
#[tauri::command]
pub fn hc_set_autostart(app: AppHandle, on: bool) -> R<bool> {
    set_autostart(&app, on)
}

/// Settings › General: whether closing the window quits Hubchat.
#[tauri::command]
pub fn hc_exit_on_close() -> bool {
    exit_on_close()
}

#[tauri::command]
pub fn hc_set_exit_on_close(app: AppHandle, on: bool) -> R<bool> {
    set_exit_on_close(&app, on)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn u(phase: Phase, available: Option<&str>) -> Updates {
        Updates { phase, available: available.map(Into::into), auto: true }
    }

    #[test]
    fn the_update_rows_wait_for_the_ui() {
        let r = update_rows(None);
        assert_eq!(r.status, "Updates have not been checked");
        assert!(!r.install && !r.install_enabled && !r.check_enabled);
    }

    #[test]
    fn the_update_rows_follow_the_updater() {
        let r = update_rows(Some(&u(Phase::Idle, None)));
        assert_eq!((r.status.as_str(), r.install, r.check_enabled), ("Updates have not been checked", false, true));
        let r = update_rows(Some(&u(Phase::Checking, None)));
        assert_eq!((r.status.as_str(), r.check_enabled), ("Checking for updates…", false));
        let r = update_rows(Some(&u(Phase::None, None)));
        assert_eq!((r.status.as_str(), r.install, r.check_enabled), ("Hubchat is up to date", false, true));
        let r = update_rows(Some(&u(Phase::Error, None)));
        assert_eq!((r.status.as_str(), r.install), ("Couldn't check for updates", false));
        let r = update_rows(Some(&u(Phase::None, Some("1.2.0"))));
        assert_eq!(r, UpdateRows { status: "Hubchat 1.2.0 is available".into(), install: true, install_enabled: true, check_enabled: true });
    }

    #[test]
    fn nothing_new_starts_while_a_check_or_an_install_is_under_way() {
        // a re-check may replace the update found before: it can't be
        // installed meanwhile, but its row stays rather than flickering out
        let r = update_rows(Some(&u(Phase::Checking, Some("1.2.0"))));
        assert_eq!(r, UpdateRows { status: "Checking for updates…".into(), install: true, install_enabled: false, check_enabled: false });
        let r = update_rows(Some(&u(Phase::Installing, Some("1.2.0"))));
        assert_eq!(r, UpdateRows { status: "Updating to Hubchat 1.2.0…".into(), install: true, install_enabled: false, check_enabled: false });
    }

    #[test]
    fn a_failed_install_can_be_tried_again() {
        let r = update_rows(Some(&u(Phase::Failed, Some("1.2.0"))));
        assert_eq!(r, UpdateRows { status: "The update didn't install".into(), install: true, install_enabled: true, check_enabled: true });
        // a failed re-check keeps the update it had found
        let r = update_rows(Some(&u(Phase::Error, Some("1.2.0"))));
        assert_eq!((r.status.as_str(), r.install, r.install_enabled), ("Couldn't check for updates", true, true));
    }

    #[test]
    fn the_ui_reports_in_its_own_words() {
        let v: Updates = serde_json::from_str(r#"{"phase":"installing","available":"1.2.0","auto":false}"#).unwrap();
        assert_eq!(v, Updates { phase: Phase::Installing, available: Some("1.2.0".into()), auto: false });
        let v: Updates = serde_json::from_str(r#"{"phase":"none","available":null,"auto":true}"#).unwrap();
        assert_eq!(v.phase, Phase::None);
    }

    #[test]
    fn a_restart_comes_back_as_it_was_without_the_old_link() {
        let exe = OsString::from(r"C:\Program Files\Hubchat\hubchat.exe");
        assert_eq!(restart_args(exe.clone(), false), vec![exe.clone()]);
        assert_eq!(restart_args(exe.clone(), true), vec![exe.clone(), OsString::from("--hidden")]);
        // what the new process makes of them (lib.rs)
        let as_strings = |a: Vec<OsString>| a.into_iter().map(|s| s.to_string_lossy().into_owned()).collect::<Vec<_>>();
        assert!(crate::desktop::stays_hidden(as_strings(restart_args(exe.clone(), true)), false));
        assert!(!crate::desktop::stays_hidden(as_strings(restart_args(exe.clone(), false)), false));
        assert_eq!(crate::desktop::start_link_arg(as_strings(restart_args(exe, true)), false), None);
    }

    #[test]
    fn exit_on_close_is_off_until_turned_on_and_kept_on_the_device() {
        let dir = std::env::temp_dir().join(format!("hubchat-exit-on-close-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        assert!(!load_exit_on_close(&dir));
        save_exit_on_close(&dir, true).unwrap();
        assert!(load_exit_on_close(&dir));
        save_exit_on_close(&dir, false).unwrap();
        assert!(!load_exit_on_close(&dir));
        std::fs::write(dir.join(EXIT_ON_CLOSE_FILE), "something else").unwrap();
        assert!(!load_exit_on_close(&dir));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
