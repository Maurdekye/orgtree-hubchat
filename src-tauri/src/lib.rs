#[cfg(target_os = "android")]
mod android;
mod commands;
mod core;
mod link;
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
        fn notify(&self, title: &str, body: &str, peer: &str) {
            let _ = self
                .app
                .notification()
                .builder()
                .title(title)
                .body(body)
                .show();
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
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
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
            commands::hc_probe_hub,
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
            link::hc_link_start,
            link::hc_link_cancel,
            link::hc_link_lookup,
            link::hc_link_offer,
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
