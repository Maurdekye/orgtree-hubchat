//! Images in the chat: previews of image attachments in the bubble (user
//! 23:29Z, design att-img), and images pasted into the composer (user 23:40Z).
//!
//! A preview comes from a copy in the app's own previews folder: what we
//! sent is copied there the first time it shows, and an incoming image this
//! device hasn't downloaded is fetched there once, so showing it never puts
//! a file in Downloads (Download still does). Deleting the message deletes
//! its copies.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Once;

use crate::commands::on_core;
use crate::core;

/// Images bigger than this keep the file card (and its Download).
pub const MAX_PREVIEW_BYTES: u64 = 20 * 1024 * 1024;
const IMAGE_EXTS: [&str; 5] = ["png", "jpg", "jpeg", "gif", "webp"];
/// Pasted images nobody sent are swept after this long.
const PASTED_KEEP: std::time::Duration = std::time::Duration::from_secs(7 * 24 * 3600);

type R<T> = Result<T, String>;

fn s<E: std::fmt::Display>(e: E) -> String {
    e.to_string()
}

pub fn is_image(name: &str) -> bool {
    name.rsplit_once('.')
        .is_some_and(|(_, ext)| IMAGE_EXTS.contains(&ext.to_ascii_lowercase().as_str()))
}

/// Our own ids, kept to safe path characters all the same.
fn safe(id: &str) -> String {
    id.chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
        .collect()
}

fn previews_dir(c: &core::Core) -> PathBuf {
    c.dir.join("previews")
}

fn preview_path(c: &core::Core, message_id: &str, local_id: &str) -> PathBuf {
    previews_dir(c).join(safe(message_id)).join(safe(local_id))
}

/// A picked or downloaded file: a path, or on Android a content:// URI.
fn read_source(c: &core::Core, source: &str) -> R<Vec<u8>> {
    let mut f = c.platform().open_source(source).map_err(s)?;
    let mut b = Vec::new();
    f.read_to_end(&mut b).map_err(s)?;
    Ok(b)
}

/// Drop the copies of messages that are gone.
fn sweep_previews(c: &core::Core) {
    if let Ok(rd) = std::fs::read_dir(previews_dir(c)) {
        for e in rd.flatten() {
            let id = e.file_name().to_string_lossy().into_owned();
            if matches!(c.store.message(&id), Ok(None)) {
                let _ = std::fs::remove_dir_all(e.path());
            }
        }
    }
}

/// Once per process: the copies of messages that went while we weren't
/// looking (deleted on another device, say), and pasted images older than a
/// week.
fn sweep(c: &core::Core) {
    sweep_previews(c);
    if let Ok(rd) = std::fs::read_dir(c.dir.join("pasted")) {
        for e in rd.flatten() {
            let old = e
                .metadata()
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| t.elapsed().ok())
                .is_some_and(|age| age > PASTED_KEEP);
            if old {
                let _ = std::fs::remove_file(e.path());
            }
        }
    }
}

/// The bytes of an image attachment, for its preview in the chat.
#[tauri::command]
pub async fn hc_attachment_preview(message_id: String, local_id: String) -> R<tauri::ipc::Response> {
    let c = core::get()?;
    static SWEEP: Once = Once::new();
    SWEEP.call_once(|| sweep(c));
    let m = c.store.message(&message_id).map_err(s)?.ok_or("no such message")?;
    let a = m
        .attachments
        .iter()
        .find(|a| a.local_id == local_id)
        .ok_or("no such attachment")?;
    if !is_image(&a.name) || a.bytes > MAX_PREVIEW_BYTES {
        return Err("no preview for this file".into());
    }
    let copy = preview_path(c, &message_id, &local_id);
    if let Ok(b) = std::fs::read(&copy) {
        return Ok(tauri::ipc::Response::new(b));
    }
    if m.outgoing {
        let src = a.source.clone().or(a.local_path.clone()).ok_or("the file isn't on this device")?;
        let b = read_source(c, &src)?;
        // kept, so the preview outlives the original (moved, deleted, or an
        // Android picker's permission that ends with the app)
        if let Some(dir) = copy.parent() {
            if std::fs::create_dir_all(dir).is_ok() {
                let _ = std::fs::write(&copy, &b);
            }
        }
        return Ok(tauri::ipc::Response::new(b));
    }
    // downloaded already: read it from where downloads went, unless the
    // user has moved or deleted it since
    if let Some(p) = &a.local_path {
        if let Ok(b) = read_source(c, p) {
            return Ok(tauri::ipc::Response::new(b));
        }
    }
    let e = c.engine()?;
    let dest = copy.clone();
    on_core(async move { e.fetch_preview(&message_id, &local_id, &dest).await.map_err(s) }).await?;
    std::fs::read(&copy).map(tauri::ipc::Response::new).map_err(s)
}

/// The bytes of an image the composer holds (picked, dropped or pasted, not
/// sent yet), for its thumbnail (user 2026-10-09 07:06Z): the same images
/// and size limit as the chat's previews.
#[tauri::command]
pub async fn hc_file_preview(source: String, name: String) -> R<tauri::ipc::Response> {
    if !is_image(&name) {
        return Err("no preview for this file".into());
    }
    let c = core::get()?;
    let f = c.platform().open_source(&source).map_err(s)?;
    let mut b = Vec::new();
    f.take(MAX_PREVIEW_BYTES + 1).read_to_end(&mut b).map_err(s)?;
    if b.len() as u64 > MAX_PREVIEW_BYTES {
        return Err("too big for a preview".into());
    }
    Ok(tauri::ipc::Response::new(b))
}

/// After Delete chat: the copies of every message that is gone.
pub fn forget_gone() {
    if let Ok(c) = core::get() {
        sweep_previews(c);
    }
}

/// Delete the preview copies of these messages (after Delete for me).
pub fn forget(message_ids: &[String]) {
    let Ok(c) = core::get() else { return };
    for id in message_ids {
        let _ = std::fs::remove_dir_all(previews_dir(c).join(safe(id)));
    }
}

/// A pasted image (raw bytes in the request body, its name in the
/// `x-name` header) saved as a file the composer can attach. Returns the path.
#[tauri::command]
pub fn hc_save_pasted(request: tauri::ipc::Request<'_>) -> R<String> {
    let data = pasted_bytes(request.body()).ok_or("expected the image's bytes")?;
    if data.is_empty() {
        return Err("the pasted image is empty".into());
    }
    let name = request
        .headers()
        .get("x-name")
        .and_then(|v| v.to_str().ok())
        .map(|n| Path::new(n).file_name().map(|f| f.to_string_lossy().into_owned()).unwrap_or_default())
        .filter(|n| !n.is_empty() && is_image(n))
        .unwrap_or_else(|| "pasted.png".into());
    let dir = core::get()?.dir.join("pasted");
    std::fs::create_dir_all(&dir).map_err(s)?;
    let (stem, ext) = name.rsplit_once('.').unwrap_or((&name, "png"));
    let path = (1..)
        .map(|i| if i == 1 { dir.join(&name) } else { dir.join(format!("{stem}-{i}.{ext}")) })
        .find(|p| !p.exists())
        .expect("unbounded");
    std::fs::write(&path, data).map_err(s)?;
    Ok(path.to_string_lossy().into_owned())
}

/// A pasted image's bytes as they arrive: raw on desktop, but on Android,
/// whose WebView can't read request bodies, Tauri sends them as a JSON array
/// of numbers.
fn pasted_bytes(body: &tauri::ipc::InvokeBody) -> Option<std::borrow::Cow<'_, [u8]>> {
    match body {
        tauri::ipc::InvokeBody::Raw(data) => Some(data.as_slice().into()),
        tauri::ipc::InvokeBody::Json(serde_json::Value::Array(items)) => items
            .iter()
            .map(|v| v.as_u64().and_then(|n| u8::try_from(n).ok()))
            .collect::<Option<Vec<u8>>>()
            .map(Into::into),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn images_by_extension() {
        assert!(is_image("Screenshot 2026-10-09.PNG"));
        assert!(is_image("a.jpeg") && is_image("b.jpg") && is_image("c.gif") && is_image("d.webp"));
        assert!(!is_image("notes.txt") && !is_image("png") && !is_image("archive.png.zip"));
    }

    #[test]
    fn pasted_bytes_come_raw_or_as_a_json_array() {
        use tauri::ipc::InvokeBody;
        let raw = InvokeBody::Raw(vec![137, 80, 78, 71]);
        assert_eq!(pasted_bytes(&raw).as_deref(), Some(&[137u8, 80, 78, 71][..]));
        let json = InvokeBody::Json(serde_json::json!([137, 80, 78, 71]));
        assert_eq!(pasted_bytes(&json).as_deref(), Some(&[137u8, 80, 78, 71][..]));
        assert_eq!(pasted_bytes(&InvokeBody::Json(serde_json::json!([1, 256]))), None);
        assert_eq!(pasted_bytes(&InvokeBody::Json(serde_json::json!({"a": 1}))), None);
    }

    #[test]
    fn ids_stay_inside_the_folder() {
        assert_eq!(safe("../x"), "___x");
        assert_eq!(safe("m-1_a"), "m-1_a");
    }
}
