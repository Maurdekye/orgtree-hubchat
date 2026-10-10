#[cfg(target_os = "android")]
mod android;
mod appupdate;
mod clip;
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
            // Windows: our own toast, so clicking it opens the chat
            #[cfg(windows)]
            let shown = toast::show(&self.app, title, body, peer, sound).is_ok();
            #[cfg(not(windows))]
            let shown = false;
            if !shown {
                let mut n = self.app.notification().builder().title(title).body(body);
                if sound {
                    // Windows: the toast's own "IM" sound; without one it is silent
                    n = n.sound("IM");
                }
                let _ = n.show();
            }
            // Remember who notified last so clicking the tray opens that chat.
            let _ = self.app.emit("hc-notified", peer);
        }
        #[cfg(windows)]
        fn clear_notification(&self, peer: &str) {
            toast::clear(&self.app, peer);
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

    /// Windows message toasts (user 2026-10-09 06:19Z: clicking one must open
    /// its chat). The notification plugin's toasts carry no click action;
    /// these carry the chat's hubchat:// link with protocol activation, so a
    /// click hands it to the registered Hubchat: the running one through the
    /// single-instance plugin, or a new start, which opens that chat. One
    /// toast per chat (its tag): a newer message replaces it, and reading the
    /// chat anywhere takes it back (user 23:50Z).
    #[cfg(windows)]
    pub mod toast {
        use tauri::AppHandle;
        use windows::core::{h, Result, HSTRING};
        use windows::Data::Xml::Dom::XmlDocument;
        use windows::UI::Notifications::{ToastNotification, ToastNotificationManager};

        /// Windows PowerShell's AppUserModelID: an app run from cargo's
        /// target folder isn't installed, so its toasts show under this one,
        /// as the notification plugin's do.
        const POWERSHELL_APP_ID: &str =
            r"{1AC14E77-02E7-4E5D-B744-2EB1AE5198B7}\WindowsPowerShell\v1.0\powershell.exe";

        /// The AppUserModelID toasts show under: the app's identifier, which
        /// the installer gives the Start menu shortcut.
        fn app_id(app: &AppHandle) -> HSTRING {
            let dir = std::env::current_exe()
                .ok()
                .and_then(|e| e.parent().map(|d| d.to_string_lossy().to_ascii_lowercase()))
                .unwrap_or_default();
            if dir.ends_with(r"\target\debug") || dir.ends_with(r"\target\release") {
                HSTRING::from(POWERSHELL_APP_ID)
            } else {
                HSTRING::from(app.config().identifier.as_str())
            }
        }

        /// The chat's toast tag (Windows allows 64 characters): FNV-1a of the
        /// address, the same in every run.
        fn tag(peer: &str) -> HSTRING {
            let mut x: u64 = 0xcbf2_9ce4_8422_2325;
            for b in peer.bytes() {
                x ^= u64::from(b);
                x = x.wrapping_mul(0x0100_0000_01b3);
            }
            HSTRING::from(format!("{x:016x}"))
        }

        /// hubchat://chat?to=<address>, the profile QR's chat link
        /// (src/lib/chatlink.ts), which opens the chat with that address.
        pub fn chat_link(peer: &str) -> String {
            let mut u = url::Url::parse("hubchat://chat").expect("a valid URL");
            u.query_pairs_mut().append_pair("to", peer);
            u.into()
        }

        /// The toast: the sender, the message, and `link` opened by a click.
        pub fn xml(title: &str, body: &str, link: Option<&str>, sound: bool) -> Result<XmlDocument> {
            let doc = XmlDocument::new()?;
            let toast = doc.CreateElement(h!("toast"))?;
            if let Some(link) = link {
                toast.SetAttribute(h!("activationType"), h!("protocol"))?;
                toast.SetAttribute(h!("launch"), &HSTRING::from(link))?;
            }
            let visual = doc.CreateElement(h!("visual"))?;
            let binding = doc.CreateElement(h!("binding"))?;
            binding.SetAttribute(h!("template"), h!("ToastGeneric"))?;
            for line in [title, body] {
                let text = doc.CreateElement(h!("text"))?;
                text.SetInnerText(&HSTRING::from(line))?;
                binding.AppendChild(&text)?;
            }
            visual.AppendChild(&binding)?;
            toast.AppendChild(&visual)?;
            let audio = doc.CreateElement(h!("audio"))?;
            if sound {
                audio.SetAttribute(h!("src"), h!("ms-winsoundevent:Notification.IM"))?;
            } else {
                audio.SetAttribute(h!("silent"), h!("true"))?;
            }
            toast.AppendChild(&audio)?;
            doc.AppendChild(&toast)?;
            Ok(doc)
        }

        pub fn show(app: &AppHandle, title: &str, body: &str, peer: &str, sound: bool) -> Result<()> {
            // a test build has no hubchat:// of its own: a click would reach
            // the installed Hubchat, so its toasts have no click action
            let link = (!app.config().identifier.ends_with(".test")).then(|| chat_link(peer));
            let n = ToastNotification::CreateToastNotification(&xml(title, body, link.as_deref(), sound)?)?;
            n.SetTag(&tag(peer))?;
            n.SetGroup(h!("messages"))?;
            ToastNotificationManager::CreateToastNotifierWithId(&app_id(app))?.Show(&n)
        }

        /// Takes back the chat's toast, if it is still shown or in the
        /// notification centre.
        pub fn clear(app: &AppHandle, peer: &str) {
            if let Ok(history) = ToastNotificationManager::History() {
                let _ = history.RemoveGroupedTagWithId(&tag(peer), h!("messages"), &app_id(app));
            }
        }

        #[cfg(test)]
        mod tests {
            #[test]
            fn a_toast_opens_its_chat_when_clicked() {
                let link = super::chat_link("pat.99e835");
                assert_eq!(link, "hubchat://chat?to=pat.99e835");
                let x = super::xml("Pat Peer", "a <b> & \"c\"", Some(&link), false).unwrap().GetXml().unwrap().to_string();
                println!("{x}");
                assert!(x.starts_with(r#"<toast activationType="protocol" launch="hubchat://chat?to=pat.99e835">"#), "{x}");
                assert!(x.contains(r#"<binding template="ToastGeneric"><text>Pat Peer</text><text>a &lt;b&gt; &amp; "c"</text></binding>"#), "{x}");
                assert!(x.contains(r#"<audio silent="true"/>"#), "{x}");
                let quiet = super::xml("Pat Peer", "hi", None, true).unwrap().GetXml().unwrap().to_string();
                assert!(!quiet.contains("launch") && quiet.contains("ms-winsoundevent:Notification.IM"), "{quiet}");
                assert_eq!(super::tag("pat.99e835"), super::tag("pat.99e835"));
                assert_ne!(super::tag("pat.99e835"), super::tag("pat.99e836"));
                assert_eq!(super::tag("x").len(), 16);
            }
        }
    }

    /// A hubchat:// link Windows started us with, or handed the running
    /// Hubchat through a second launch, until the UI takes it.
    static PENDING: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);

    /// The hubchat:// link among a launch's arguments, if there is one.
    fn link_arg<I: IntoIterator<Item = String>>(args: I) -> Option<String> {
        args.into_iter()
            .find(|a| a.len() < 4096 && a.to_ascii_lowercase().starts_with("hubchat://"))
    }

    /// Keep the hubchat:// link among a launch's arguments, if there is one.
    /// True when there was.
    pub fn take_link_arg<I: IntoIterator<Item = String>>(args: I) -> bool {
        let link = link_arg(args);
        let found = link.is_some();
        if let Some(l) = link {
            *PENDING.lock().unwrap() = Some(l);
        }
        found
    }

    /// The link this start opens from its own arguments: none on the first
    /// start of a version. The updater's restart hands the new version the
    /// old process's arguments, so a link that started the old process would
    /// open again (an old chat, or a setup or device code long expired) and
    /// win over the chat that was open. A link clicked now still opens once
    /// Hubchat runs.
    pub fn start_link_arg<I: IntoIterator<Item = String>>(
        args: I,
        first_of_version: bool,
    ) -> Option<String> {
        if first_of_version {
            None
        } else {
            link_arg(args)
        }
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

    /// "Start with Windows" starts hidden in the tray, but not the first start
    /// of a version: the updater's restart hands the installer the old
    /// process's arguments, `--hidden` among them, and an update the user just
    /// clicked must come back on screen.
    pub fn stays_hidden<I: IntoIterator<Item = String>>(args: I, first_of_version: bool) -> bool {
        !first_of_version && args.into_iter().any(|a| a == "--hidden")
    }

    /// True when `dir` holds no record of having run `version` before (a new
    /// install, an update, a downgrade); records it either way.
    pub fn first_start_of_version(dir: &std::path::Path, version: &str) -> bool {
        let file = dir.join("last-run-version");
        let first = std::fs::read_to_string(&file).map_or(true, |v| v.trim() != version);
        if first {
            let _ = std::fs::create_dir_all(dir);
            let _ = std::fs::write(&file, version);
        }
        first
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
            commands::hc_set_device_name,
            commands::hc_set_active,
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
            commands::hc_load_older,
            commands::hc_message,
            commands::hc_send,
            commands::hc_retry,
            commands::hc_cancel_transfer,
            commands::hc_download,
            commands::hc_mark_read,
            commands::hc_send_route,
            commands::hc_set_send_hub,
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
            media::hc_file_preview,
            clip::hc_clipboard_text,
            clip::hc_clipboard_image,
            media::hc_save_pasted,
            appupdate::hc_app_update_check,
            appupdate::hc_app_update_install,
            appupdate::hc_app_update_allow,
            appupdate::hc_app_update_state,
            link::hc_link_start,
            link::hc_link_cancel,
            link::hc_link_lookup,
            link::hc_link_offer,
            link::hc_link_confirm,
            link::hc_link_discard,
            link::hc_link_forget,
            link::hc_parse_link,
            commands::hc_take_pending_link,
            commands::hc_parse_setup,
            commands::hc_setup_check,
            commands::hc_app_installed,
            commands::hc_open_app,
            commands::hc_vpn_active,
            commands::hc_setup_start,
            commands::hc_setup_status,
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
                #[cfg(windows)]
                desktop::register_scheme(app.handle());
                let first = desktop::first_start_of_version(
                    &app.path().app_data_dir()?,
                    &app.package_info().version.to_string(),
                );
                desktop::take_link_arg(desktop::start_link_arg(std::env::args(), first));
                if desktop::stays_hidden(std::env::args(), first) {
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

#[cfg(all(test, desktop))]
mod start_tests {
    use super::desktop::{first_start_of_version, start_link_arg, stays_hidden};

    fn args(a: &[&str]) -> Vec<String> {
        a.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn start_with_windows_stays_hidden_after_the_first_start_of_a_version() {
        assert!(stays_hidden(args(&["hubchat.exe", "--hidden"]), false));
    }

    #[test]
    fn the_first_start_of_a_version_shows_the_window_even_with_hidden() {
        assert!(!stays_hidden(args(&["hubchat.exe", "--hidden"]), true));
    }

    #[test]
    fn a_normal_start_shows_the_window() {
        assert!(!stays_hidden(args(&["hubchat.exe"]), false));
        assert!(!stays_hidden(args(&["hubchat.exe"]), true));
    }

    #[test]
    fn a_start_opens_the_link_it_was_started_with() {
        let link = "hubchat://chat?to=pat-peer.050529";
        assert_eq!(start_link_arg(args(&["hubchat.exe", link]), false).as_deref(), Some(link));
        assert_eq!(start_link_arg(args(&["hubchat.exe", "--hidden"]), false), None);
        assert_eq!(start_link_arg(args(&["hubchat.exe"]), false), None);
    }

    #[test]
    fn the_first_start_of_a_version_leaves_the_old_process_link_alone() {
        // the updater's restart: the old process's arguments, its link included
        let old = args(&["hubchat.exe", "hubchat://setup?v=1&code=ABCD-EFGH"]);
        assert_eq!(start_link_arg(old, true), None);
    }

    #[test]
    fn a_version_is_first_once_then_known_until_it_changes() {
        let dir = std::env::temp_dir().join(format!("hubchat-first-start-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        // nothing stored (a new install, or the update from a version before this fix)
        assert!(first_start_of_version(&dir, "1.0.0"));
        // the next start of the same version, e.g. Start with Windows
        assert!(!first_start_of_version(&dir, "1.0.0"));
        assert!(!first_start_of_version(&dir, "1.0.0"));
        // an update, then a downgrade
        assert!(first_start_of_version(&dir, "1.0.1"));
        assert!(!first_start_of_version(&dir, "1.0.1"));
        assert!(first_start_of_version(&dir, "1.0.0"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
