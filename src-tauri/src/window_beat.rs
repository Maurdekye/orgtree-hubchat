//! Desktop adapter; no page or main-thread timer is trusted to detect a stuck UI.
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use crate::window_watch::WindowWatch;

pub struct Watch(Arc<Mutex<WindowWatch>>);

pub fn start() -> Watch {
    let state = Arc::new(Mutex::new(WindowWatch::new(Instant::now())));
    let worker = state.clone();
    std::thread::Builder::new().name("hubchat-window-watch".into()).spawn(move || loop {
        std::thread::sleep(Duration::from_secs(5));
        let mut watch = worker.lock().unwrap_or_else(|p| p.into_inner());
        for line in watch.tick(Instant::now()) {
            tracing::warn!(target: "hubchat.window", "{}", line);
        }
    }).expect("start desktop window watch");
    Watch(state)
}

#[tauri::command]
pub fn ui_beat(state: tauri::State<'_, Watch>, visible: bool) {
    let mut watch = state.inner().0.lock().unwrap_or_else(|p| p.into_inner());
    if let Some(line) = watch.beat(Instant::now(), visible) {
        tracing::info!(target: "hubchat.window", "{}", line);
    }
}
