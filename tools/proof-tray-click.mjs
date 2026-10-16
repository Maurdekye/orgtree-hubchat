// Run under the shared build lock and a kernel Job. No Cargo dependencies or app.
// Compiles the actual Rust click handlers against a popup/runtime test double.
// Timing ends at open_recent(), NOT native WebView visibility or desktop paint.
import fs from 'node:fs';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import {spawnSync} from 'node:child_process';
const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const [output] = process.argv.slice(2);
if (!output) throw Error('usage: node tools/proof-tray-click.mjs <owned-output>');
fs.mkdirSync(output, {recursive: true});
const source = fs.readFileSync(path.join(root, 'src-tauri/src/tray_recent.rs'), 'utf8').replaceAll('\r\n', '\n');
function between(start, end) {
  const at = source.indexOf(start), until = source.indexOf(end, at);
  if (at < 0 || until < 0) throw Error(`missing source boundary ${start}`);
  return source.slice(at, until);
}
const clicks = between('#[derive(Default)]\nstruct Clicks', '#[derive(Default)]\npub struct Recent');
const handlers = between('pub fn left_click(', 'pub fn open_chat(');
const harness = String.raw`
#![allow(dead_code, unused_imports, non_snake_case)]
use std::{sync::{Arc, Mutex, atomic::{AtomicBool, Ordering}}, time::{Duration, Instant}};
use std::future::Future;
use std::task::{Context, Poll, Wake, Waker};
${clicks}
#[derive(Default)] struct View { clicks: Clicks }
#[derive(Default)] struct Recent { view: Mutex<View> }
#[derive(Default)] struct App {
    recent: Recent, popup: AtomicBool, main: AtomicBool,
    opened: Mutex<Option<Instant>>, workers: Mutex<Vec<std::thread::JoinHandle<()>>>,
}
#[derive(Clone, Default)] pub struct AppHandle(Arc<App>);
impl AppHandle {
    fn state<T>(&self) -> &Recent { &self.0.recent }
    fn try_state<T>(&self) -> Option<&Recent> { Some(&self.0.recent) }
    fn run_on_main_thread(&self, f: impl FnOnce()) -> Result<(), ()> { f(); Ok(()) }
    fn drain(&self) {
        for worker in std::mem::take(&mut *self.0.workers.lock().unwrap()) { worker.join().unwrap(); }
    }
}
#[derive(Clone, Copy)] pub struct PhysicalPosition<T> { x: T, y: T }
fn point() -> PhysicalPosition<f64> { PhysicalPosition { x: 100., y: 700. } }
fn sync(_: &AppHandle) {}
mod tray_popup {
    use super::*;
    pub fn is_recent(app: &AppHandle) -> bool { app.0.popup.load(Ordering::SeqCst) }
    pub fn close(app: &AppHandle) { app.0.popup.store(false, Ordering::SeqCst); }
    pub fn open_recent(app: &AppHandle, _: PhysicalPosition<f64>) -> Result<(), ()> {
        *app.0.opened.lock().unwrap() = Some(Instant::now());
        app.0.popup.store(true, Ordering::SeqCst); Ok(())
    }
}
mod desktop {
    use super::*;
    pub fn show(app: &AppHandle) { app.0.main.store(true, Ordering::SeqCst); }
}
// Each pending sleep in the predecessor is a real wall-clock sleep. Workers
// are joined before each test ends, including on the expected red path.
thread_local! { static CURRENT: std::cell::RefCell<Option<AppHandle>> = const { std::cell::RefCell::new(None) }; }
mod tauri {
    pub mod async_runtime {
        use super::super::*;
        pub fn spawn(f: impl Future<Output = ()> + Send + 'static) {
            let worker = std::thread::spawn(move || {
                struct Noop; impl Wake for Noop { fn wake(self: Arc<Self>) {} }
                let waker = Waker::from(Arc::new(Noop));
                let mut cx = Context::from_waker(&waker); let mut f = Box::pin(f);
                assert!(matches!(f.as_mut().poll(&mut cx), Poll::Ready(())));
            });
            CURRENT.with(|a| a.borrow().as_ref().unwrap().0.workers.lock().unwrap().push(worker));
        }
    }
}
mod tokio { pub mod time {
    pub async fn sleep(wait: std::time::Duration) { std::thread::sleep(wait); }
} }
mod windows_sys { pub mod Win32 { pub mod UI { pub mod Input { pub mod KeyboardAndMouse {
    #[link(name = "user32")] unsafe extern "system" { pub fn GetDoubleClickTime() -> u32; }
} } } } }
${handlers}
fn app() -> AppHandle {
    let app = AppHandle::default(); CURRENT.with(|a| *a.borrow_mut() = Some(app.clone())); app
}
#[test]
fn single_click_reaches_popup_without_waiting_for_a_double_click() {
    let app = app(); let start = Instant::now();
    left_click(&app, point());
    let opened_before_return = app.0.opened.lock().unwrap().is_some();
    app.drain();
    let elapsed = app.0.opened.lock().unwrap().unwrap().duration_since(start);
    let interval = unsafe { windows_sys::Win32::UI::Input::KeyboardAndMouse::GetDoubleClickTime() };
    println!("click_to_popup_request_ms={:.3}; windows_double_click_ms={interval}", elapsed.as_secs_f64()*1000.);
    assert!(opened_before_return, "single click deferred the popup request");
}
#[test]
fn clicking_an_open_list_toggles_it_closed() {
    let app = app(); app.0.popup.store(true, Ordering::SeqCst);
    left_click(&app, point()); app.drain();
    assert!(!tray_popup::is_recent(&app));
    assert!(app.0.opened.lock().unwrap().is_none());
}
#[test]
fn double_click_closes_list_shows_main_and_suppresses_second_release() {
    let app = app(); left_click(&app, point());
    double_click(&app); left_click(&app, point()); app.drain();
    assert!(!tray_popup::is_recent(&app)); assert!(app.0.main.load(Ordering::SeqCst));
}
#[test]
fn immediate_tray_blur_consumes_one_toggle_but_old_blur_does_not() {
    let app = app(); app.0.recent.view.lock().unwrap().clicks.blurred = Some(Instant::now());
    left_click(&app, point()); app.drain();
    assert!(app.0.opened.lock().unwrap().is_none());
    app.0.recent.view.lock().unwrap().clicks.blurred = Some(Instant::now()-Duration::from_secs(1));
    left_click(&app, point()); app.drain(); assert!(tray_popup::is_recent(&app));
}
#[test]
fn cancelled_popup_cannot_reappear_from_a_pending_click() {
    let app = app(); left_click(&app, point());
    cancel_click(&app); tray_popup::close(&app); app.drain();
    assert!(!tray_popup::is_recent(&app));
}
`;
const rust = path.join(output, 'tray-click.rs'), exe = path.join(output, 'tray-click.exe');
fs.writeFileSync(rust, harness.replaceAll('\r\n', '\n'));
const compile = spawnSync('rustc', ['--edition=2021', '--test', rust, '-o', exe], {encoding: 'utf8', windowsHide: true, timeout: 60000});
fs.writeFileSync(path.join(output, 'compile.json'), JSON.stringify({status: compile.status, signal: compile.signal, error: compile.error?.message, stdout: compile.stdout, stderr: compile.stderr}, null, 2));
if (compile.status !== 0) { process.stderr.write(compile.stderr || String(compile.error)); process.exit(1); }
const run = spawnSync(exe, ['--nocapture', '--test-threads=1'], {encoding: 'utf8', windowsHide: true, timeout: 20000});
fs.writeFileSync(path.join(output, 'result.json'), JSON.stringify({scope: 'Actual Rust handlers with stub popup/runtime; no native visibility measurement', status: run.status, signal: run.signal, error: run.error?.message, stdout: run.stdout, stderr: run.stderr}, null, 2));
process.stdout.write(run.stdout || ''); process.stderr.write(run.stderr || '');
process.exit(run.status === 0 && !run.error ? 0 : 1);
