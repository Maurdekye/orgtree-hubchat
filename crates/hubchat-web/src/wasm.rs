//! The shell in a browser worker: `boot`, `call` and `stop` are what the
//! worker's script (`src/web/core.worker.ts`) calls.
//!
//! The script hands `boot` a `host` object, the worker's side of the bargain:
//!
//! - `emit(name, json)`: an event for the page (`hc`, as the apps emit it);
//! - `loadIdentity()`: the key this browser keeps, `{ id, secret }` or null;
//! - `saveIdentity(id, secret)` and `clearIdentity()`: keep it, forget it;
//! - `deviceName`: what to call this browser among the user's devices.
//!
//! The key is kept by the script (IndexedDB, wrapped by a key the browser
//! will not give out: `src/web/keys.ts`), never by the server: this module
//! only passes it across.

use std::cell::RefCell;
use std::path::Path;
use std::sync::Arc;

use hubchat_core::store::Store;
use hubchat_core::Identity;
use js_sys::{Array, Function, Promise, Reflect};
use serde_json::Value;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;

use crate::shell::{Shell, Sink};

/// The database, in the origin's private file system.
const DATABASE: &str = "/hubchat.db";
/// Its directory there.
const DIRECTORY: &str = "hubchat";
/// The directory of Hubchat Web's files there (attachments on their way out
/// and in), beside the database's.
const FILES: &str = "hubchat-files";

thread_local! {
    // a worker is one thread: these never leave it
    static HOST: RefCell<Option<JsValue>> = const { RefCell::new(None) };
    static SHELL: RefCell<Option<Arc<Shell>>> = const { RefCell::new(None) };
}

fn err<E: std::fmt::Display>(e: E) -> JsValue {
    JsValue::from_str(&e.to_string())
}

/// What a thrown JavaScript value says.
fn thrown(e: &JsValue) -> String {
    if let Some(s) = e.as_string() {
        return s;
    }
    Reflect::get(e, &JsValue::from_str("message")).ok().and_then(|m| m.as_string()).unwrap_or_else(|| format!("{e:?}"))
}

fn host_fn(name: &str) -> Result<(JsValue, Function), JsValue> {
    let host = HOST.with(|h| h.borrow().clone()).ok_or_else(|| err("the core is not started"))?;
    let f = Reflect::get(&host, &JsValue::from_str(name))?.dyn_into::<Function>().map_err(|_| err(format!("host.{name} is missing")))?;
    Ok((host, f))
}

/// Call the worker's script and wait for its answer (a value or a promise).
async fn host_call(name: &str, args: &[JsValue]) -> Result<JsValue, JsValue> {
    let (host, f) = host_fn(name)?;
    let list = Array::new();
    for a in args {
        list.push(a);
    }
    let out = f.apply(&host, &list)?;
    JsFuture::from(Promise::resolve(&out)).await
}

fn emit(name: &str, json: &str) {
    if let Ok((host, f)) = host_fn("emit") {
        let _ = f.call2(&host, &JsValue::from_str(name), &JsValue::from_str(json));
    }
}

fn text_field(of: &JsValue, name: &str) -> Option<String> {
    Reflect::get(of, &JsValue::from_str(name)).ok().and_then(|v| v.as_string())
}

/// Start the core in this worker. Once per worker; an error says why the
/// page cannot use Hubchat here.
#[wasm_bindgen]
pub async fn boot(host: JsValue) -> Result<(), JsValue> {
    console_error_panic_hook::set_once();
    if SHELL.with(|s| s.borrow().is_some()) {
        return Err(err("the core is already started"));
    }
    HOST.with(|h| *h.borrow_mut() = Some(host.clone()));
    // SQLite's files: a pool of sync access handles in the origin's private
    // file system (a dedicated worker only, one owner at a time, no WAL)
    let pool = sqlite_wasm_vfs::sahpool::OpfsSAHPoolCfgBuilder::new().directory(DIRECTORY).build();
    sqlite_wasm_vfs::sahpool::install::<sqlite_wasm_rs::WasmOsCallback>(&pool, true)
        .await
        .map_err(|e| err(format!("This browser didn't give Hubchat its storage ({e}).")))?;
    // The files' directory, read once. A browser that refuses it still chats: file work
    // then answers that there is no file storage here (rt::fs), text is not held up.
    if let Err(e) = hubchat_core::rt::fs::init(FILES).await {
        emit("hc-web-files", &serde_json::json!({ "refused": e.to_string() }).to_string());
    }
    // a database written by a newer build than this one is refused, untouched: the page then
    // asks for the server to be updated (src/web/startError.ts)
    let store = Arc::new(Store::open_refusing_newer(Path::new(DATABASE)).map_err(err)?);
    let kept = host_call("loadIdentity", &[]).await.map_err(|e| err(format!("Hubchat couldn't read its key in this browser: {}", thrown(&e))))?;
    let identity = if kept.is_null() || kept.is_undefined() {
        None
    } else {
        let (id, secret) = (text_field(&kept, "id"), text_field(&kept, "secret"));
        let (Some(id), Some(secret)) = (id, secret) else { return Err(err("the key this browser keeps is not a Hubchat key")) };
        Some(Identity::from_parts(&id, &secret).map_err(err)?)
    };
    let name = text_field(&host, "deviceName").unwrap_or_default();
    let sink: Sink = Arc::new(emit);
    let shell = Shell::open(store, sink, &name, identity).map_err(err)?;
    SHELL.with(|s| *s.borrow_mut() = Some(shell.clone()));
    // previews of messages that are gone, chosen files kept long enough
    hubchat_core::rt::spawn(async move { shell.sweep_files().await });
    Ok(())
}

/// A chosen file is read this much at a time (a page holds no more of it).
const PUT_PIECE: f64 = 1024.0 * 1024.0;

/// Copy a file the user chose (a `Blob`) into Hubchat Web's own files: its
/// path there, which is its `source` from then on.
async fn put(shell: &Arc<Shell>, name: &str, blob: &JsValue) -> Result<String, JsValue> {
    use tokio::io::AsyncWriteExt;
    let size = Reflect::get(blob, &JsValue::from_str("size"))?.as_f64().ok_or_else(|| err("expected a file"))?;
    let slice = Reflect::get(blob, &JsValue::from_str("slice"))?.dyn_into::<Function>().map_err(|_| err("expected a file"))?;
    let (path, mut file) = shell.new_picked(name).await.map_err(err)?;
    let copied: Result<(), JsValue> = async {
        let mut at = 0.0;
        while at < size {
            let end = (at + PUT_PIECE).min(size);
            let piece = slice.call2(blob, &JsValue::from_f64(at), &JsValue::from_f64(end))?;
            let read = Reflect::get(&piece, &JsValue::from_str("arrayBuffer"))?.dyn_into::<Function>().map_err(|_| err("expected a file"))?.call0(&piece)?;
            let bytes = js_sys::Uint8Array::new(&JsFuture::from(Promise::resolve(&read)).await?).to_vec();
            file.write_all(&bytes).await.map_err(err)?;
            at = end;
        }
        file.flush().await.map_err(err)
    }
    .await;
    drop(file);
    match copied {
        Ok(()) => Ok(path),
        Err(e) => {
            shell.drop_picked(&path).await;
            Err(err(format!("Hubchat couldn't keep that file to send it: {}", thrown(&e))))
        }
    }
}

/// A command that takes or gives more than JSON:
/// - `hc_web_put` {name} with `blob`: keep a chosen file; its path.
/// - `hc_file_preview`, `hc_attachment_preview`: the picture's bytes.
/// - `hc_web_file` {messageId, localId}: `{ name, file }`, this browser's
///   copy of an attachment as the browser's own `File`.
#[wasm_bindgen]
pub async fn raw(cmd: String, args: String, blob: JsValue) -> Result<JsValue, JsValue> {
    let shell = SHELL.with(|s| s.borrow().clone()).ok_or_else(|| err("the core is not started"))?;
    let args: Value = if args.is_empty() { Value::Null } else { serde_json::from_str(&args).map_err(err)? };
    let word = |name: &str| args.get(name).and_then(Value::as_str).map(str::to_owned).ok_or_else(|| err(format!("missing argument: {name}")));
    match cmd.as_str() {
        "hc_web_put" => Ok(JsValue::from_str(&put(&shell, &word("name")?, &blob).await?)),
        "hc_web_file" => {
            let (path, name) = shell.attachment_file(&word("messageId")?, &word("localId")?).map_err(err)?;
            let file = hubchat_core::rt::fs::browser_file(&path).await.map_err(err)?;
            let out = js_sys::Object::new();
            Reflect::set(&out, &JsValue::from_str("name"), &JsValue::from_str(&name))?;
            Reflect::set(&out, &JsValue::from_str("file"), &file)?;
            Ok(out.into())
        }
        _ => Ok(js_sys::Uint8Array::from(shell.bytes(&cmd, &args).await.map_err(err)?.as_slice()).into()),
    }
}

/// One command of the UI: its name and its arguments as JSON; the answer as
/// JSON. An error is the sentence the UI shows.
#[wasm_bindgen]
pub async fn call(cmd: String, args: String) -> Result<String, JsValue> {
    let shell = SHELL.with(|s| s.borrow().clone()).ok_or_else(|| err("the core is not started"))?;
    let args: Value = if args.is_empty() { Value::Null } else { serde_json::from_str(&args).map_err(err)? };
    let out = match shell.plan_identity(&cmd, &args).map_err(err)? {
        // the key is kept before the identity is taken, as in the apps
        Some(plan) => {
            host_call("saveIdentity", &[JsValue::from_str(plan.me.id()), JsValue::from_str(plan.me.secret())])
                .await
                .map_err(|e| err(format!("Hubchat couldn't keep your key in this browser: {}", thrown(&e))))?;
            Value::String(shell.adopt(plan).map_err(err)?)
        }
        None if cmd == "hc_start_over" => {
            // the key first: local data without its key is the lost-key
            // screen again, a key without its data would be a silent restore
            host_call("clearIdentity", &[]).await.map_err(|e| err(thrown(&e)))?;
            shell.start_over().map_err(err)?;
            Value::Null
        }
        None if cmd == "hc_sign_out" => {
            // the key first, as for Start over; the engine still holds it to tell the hubs
            shell.engine().map_err(err)?;
            host_call("clearIdentity", &[]).await.map_err(|e| err(thrown(&e)))?;
            shell.sign_out().await.map_err(err)?;
            Value::Null
        }
        None => shell.call(&cmd, &args).await.map_err(err)?,
    };
    serde_json::to_string(&out).map_err(err)
}

/// The text of a QR code in a camera frame of `width` x `height` grey pixels
/// (`scan.rs`). Null: none in this frame.
#[wasm_bindgen]
pub fn scan(width: u32, height: u32, grey: &[u8]) -> Option<String> {
    crate::scan::decode(width as usize, height as usize, grey)
}

/// The page is going away, or another tab takes over: stop the engine.
#[wasm_bindgen]
pub fn stop() {
    if let Some(shell) = SHELL.with(|s| s.borrow_mut().take()) {
        shell.shutdown();
    }
}
