#[cfg(target_os = "android")]
mod android;
mod connection;
mod spike;

#[cfg(desktop)]
struct DesktopPlatform;

#[cfg(desktop)]
impl connection::Platform for DesktopPlatform {
    fn notify(&self, title: &str, body: &str) {
        println!("[notify] {title}: {body}");
    }
    fn status(&self, text: &str) {
        println!("[status] {text}");
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = tauri::Builder::default();
    // Windows updates itself from GitHub Releases (signed); Android does not.
    #[cfg(desktop)]
    let builder = builder.plugin(tauri_plugin_updater::Builder::new().build());
    builder
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            spike::spike_info,
            spike::spike_roundtrip,
            spike::spike_save
        ])
        .setup(|_app| {
            // On Android the ConnectionService starts the core (it outlives the
            // activity); on desktop the app process does.
            #[cfg(desktop)]
            {
                use tauri::Manager;
                let dir = _app.path().app_data_dir()?;
                connection::start(dir, std::sync::Arc::new(DesktopPlatform));
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
