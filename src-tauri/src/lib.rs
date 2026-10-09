#[cfg(target_os = "android")]
mod android;
mod commands;
mod core;
mod link;
mod media;
mod secrets;

#[cfg(desktop)]
mod desktop {
    use std::path::PathBuf;

    use tauri::menu::{Menu, MenuItem};
    use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
    use tauri::{AppHandle, Emitter, Manager};
    use tauri_plugin_notification::NotificationExt;

    pub struct DesktopPlatform {
        pub app: AppHandle,
        pub downloads: PathBuf,
    }

    impl crate::core::Platform for DesktopPlatform {
        fn notify(&self, title: &str, body: &str, peer: &str, sound: bool) {
            let mut n = self.app.notification().builder().title(title).body(body);
            if sound {
                // Windows: the toast's own "IM" sound; without one it is silent
                n = n.sound("IM");
            }
            let _ = n.show();
            // Remember who notified last so clicking the tray opens that chat.
            let _ = self.app.emit("hc-notified", peer);
        }
        fn status(&self, text: &str) {
            if let Some(tray) = self.app.tray_by_id("main") {
                let _ = tray.set_tooltip(Some(format!("Hubchat — {text}")));
            }
        }
        fn open_source(&self, source: &str) -> std::io::Result<std::fs::File> {
            std::fs::File::open(source)
        }
        fn download_dir(&self) -> PathBuf {
            self.downloads.clone()
        }
        fn take_pending_link(&self) -> Option<String> {
            PENDING.lock().unwrap().take()
        }
    }

    /// A hubchat:// link Windows started us with, or handed the running
    /// Hubchat through a second launch, until the UI takes it.
    static PENDING: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);

    /// Keep the hubchat:// link among a launch's arguments, if there is one.
    /// True when there was.
    pub fn take_link_arg<I: IntoIterator<Item = String>>(args: I) -> bool {
        let link = args
            .into_iter()
            .find(|a| a.len() < 4096 && a.to_ascii_lowercase().starts_with("hubchat://"));
        let found = link.is_some();
        if let Some(l) = link {
            *PENDING.lock().unwrap() = Some(l);
        }
        found
    }

    /// Windows: hubchat:// links (a profile QR's chat link, a device link)
    /// open this Hubchat (user 23:53Z). Per user, rewritten only when it
    /// points elsewhere; a test build leaves the real install's alone.
    #[cfg(windows)]
    pub fn register_scheme(app: &AppHandle) {
        use winreg::enums::HKEY_CURRENT_USER;
        use winreg::RegKey;
        if app.config().identifier.ends_with(".test") {
            return;
        }
        let Ok(exe) = std::env::current_exe() else {
            return;
        };
        let exe = exe.to_string_lossy().into_owned();
        let cmd = format!("\"{exe}\" \"%1\"");
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let Ok((key, _)) = hkcu.create_subkey(r"Software\Classes\hubchat") else {
            return;
        };
        let now: Option<String> = key
            .open_subkey(r"shell\open\command")
            .and_then(|k| k.get_value(""))
            .ok();
        if now.as_deref() == Some(cmd.as_str()) {
            return;
        }
        let _ = key.set_value("", &"URL:Hubchat link");
        let _ = key.set_value("URL Protocol", &"");
        if let Ok((icon, _)) = key.create_subkey("DefaultIcon") {
            let _ = icon.set_value("", &format!("\"{exe}\",0"));
        }
        if let Ok((c, _)) = key.create_subkey(r"shell\open\command") {
            let _ = c.set_value("", &cmd);
        }
    }

    pub fn show(app: &AppHandle) {
        if let Some(w) = app.get_webview_window("main") {
            let _ = w.unminimize();
            let _ = w.show();
            let _ = w.set_focus();
        }
    }

    /// Tray icon: click shows the window; menu has Open and Quit. Closing the
    /// window hides it, so messages and transfers keep going (design §7).
    pub fn tray(app: &AppHandle) -> tauri::Result<()> {
        let open = MenuItem::with_id(app, "open", "Open Hubchat", true, None::<&str>)?;
        let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
        let menu = Menu::with_items(app, &[&open, &quit])?;
        TrayIconBuilder::with_id("main")
            .icon(app.default_window_icon().cloned().expect("icon"))
            .tooltip("Hubchat")
            .menu(&menu)
            .show_menu_on_left_click(false)
            .on_menu_event(|app, ev| match ev.id().as_ref() {
                "open" => show(app),
                "quit" => app.exit(0),
                _ => {}
            })
            .on_tray_icon_event(|tray, ev| {
                if let TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    ..
                } = ev
                {
                    show(tray.app_handle());
                }
            })
            .build(app)?;
        Ok(())
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = tauri::Builder::default();
    // QR codes: the "offline QR" identity link and scanning someone's address.
    #[cfg(mobile)]
    let builder = builder.plugin(tauri_plugin_barcode_scanner::init());
    // Windows updates itself from GitHub Releases (signed); Android does not.
    #[cfg(desktop)]
    let builder = builder
        // A second launch (Start menu, autostart) shows the running window
        // instead of starting another Hubchat. Registered first, per the plugin.
        .plugin(tauri_plugin_single_instance::init(|app, args, _cwd| {
            // a hubchat:// link opened while we run: the UI takes it
            if desktop::take_link_arg(args) {
                use tauri::Emitter;
                let _ = app.emit("hc-link-pending", ());
            }
            desktop::show(app)
        }))
        .plugin(tauri_plugin_updater::Builder::new().build())
        // "Start with Windows" (Settings): starts hidden in the tray.
        .plugin(
            tauri_plugin_autostart::Builder::new()
                .args(["--hidden"])
                .build(),
        )
        .plugin(tauri_plugin_notification::init())
        .on_window_event(|w, ev| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = ev {
                api.prevent_close();
                let _ = w.hide();
            }
        });
    builder
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            commands::hc_state,
            commands::hc_ui_state,
            commands::hc_check_id,
            commands::hc_create_identity,
            commands::hc_restore_words,
            commands::hc_recovery_words,
            commands::hc_recovery_saved,
            commands::hc_set_profile,
            commands::hc_set_read_receipts,
            commands::hc_set_notifications,
            commands::hc_set_stay_connected,
            commands::hc_probe_hub,
            commands::hc_probe_link_hubs,
            commands::hc_add_hub,
            commands::hc_remove_hub,
            commands::hc_retry_now,
            commands::hc_directory,
            commands::hc_resolve,
            commands::hc_chats,
            commands::hc_chat,
            commands::hc_message,
            commands::hc_send,
            commands::hc_retry,
            commands::hc_cancel_transfer,
            commands::hc_download,
            commands::hc_mark_read,
            commands::hc_delete_message,
            commands::hc_delete_chat,
            commands::hc_devices,
            commands::hc_save_recovery,
            commands::hc_draft,
            commands::hc_set_draft,
            commands::hc_file_info,
            commands::hc_take_pending_chat,
            commands::hc_open_attachment,
            media::hc_attachment_preview,
            media::hc_save_pasted,
            link::hc_link_start,
            link::hc_link_cancel,
            link::hc_link_lookup,
            link::hc_link_offer,
            link::hc_link_confirm,
            link::hc_link_discard,
            link::hc_link_forget,
            link::hc_parse_link,
            commands::hc_take_pending_link,
            link::hc_link_approve,
            link::hc_key_qr,
            link::hc_restore_qr,
            link::hc_key_file_export,
            link::hc_key_file_import,
        ])
        .setup(|app| {
            // On Android the ConnectionService creates the core (it outlives
            // the activity); here we only attach the UI to it.
            #[cfg(desktop)]
            {
                use tauri::Manager;
                let dir = app.path().app_data_dir()?;
                let downloads = app
                    .path()
                    .download_dir()
                    .unwrap_or_else(|_| dir.join("downloads"));
                let platform = desktop::DesktopPlatform {
                    app: app.handle().clone(),
                    downloads,
                };
                core::init(dir, std::sync::Arc::new(platform))?;
                desktop::tray(app.handle())?;
                desktop::take_link_arg(std::env::args());
                #[cfg(windows)]
                desktop::register_scheme(app.handle());
                if std::env::args().any(|a| a == "--hidden") {
                    if let Some(w) = app.get_webview_window("main") {
                        let _ = w.hide();
                    }
                }
            }
            let handle = app.handle().clone();
            std::thread::spawn(move || {
                // Android: the service may still be creating the core.
                for _ in 0..200 {
                    if let Ok(c) = core::get() {
                        c.attach_ui(handle);
                        return;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(50));
                }
            });
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
