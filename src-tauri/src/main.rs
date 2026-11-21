// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    #[cfg(target_os = "linux")]
    hubchat_lib::linux_gamescope::reexec_if_needed();
    hubchat_lib::run()
}
