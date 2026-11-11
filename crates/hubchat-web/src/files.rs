//! Files in Hubchat Web: what the apps' `src-tauri/src/media.rs` and file
//! commands do, with the core's own file layer (`rt::fs`: in a browser, the
//! origin's private file system). Nothing here touches JavaScript, so its
//! tests run natively, in a folder of their own.
//!
//! Under the shell's root:
//! - `picked/<id>/<name>`: a file the user chose (or pasted) to send. A page
//!   has the file itself, not a path: the worker copies it here, and this
//!   path is its `source` from then on, as a path is in the apps.
//! - `outbox/`: the core's private copies of files on their way out.
//! - `downloads/`: files saved from chats.
//! - `previews/<message>/<attachment>`: pictures fetched to be shown.

use std::path::{Path, PathBuf};
use std::time::Duration;

use hubchat_core::rt;
use serde_json::{json, Value};

use crate::shell::{s, Shell, R};

/// The apps' limit and kinds (`src-tauri/src/media.rs`).
pub const MAX_PREVIEW_BYTES: u64 = 20 * 1024 * 1024;
const IMAGE_EXTS: [&str; 5] = ["png", "jpg", "jpeg", "gif", "webp"];
/// A chosen file is kept this long (the apps keep a pasted image as long):
/// until then a sent file can be opened and shown from this browser's copy.
const PICKED_KEEP: Duration = Duration::from_secs(7 * 24 * 3600);
const NAME_MAX: usize = 120;

pub fn is_image(name: &str) -> bool {
    name.rsplit_once('.').is_some_and(|(_, ext)| IMAGE_EXTS.contains(&ext.to_ascii_lowercase().as_str()))
}

/// Our own ids, kept to safe path characters all the same.
fn safe(id: &str) -> String {
    id.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' }).collect()
}

/// The name a chosen file is kept under: its last part, without what a path
/// could not hold. Never empty, never a way out of its folder.
pub fn clean_name(given: &str) -> String {
    let last = given.rsplit(['/', '\\']).next().unwrap_or("");
    let clean: String = last.chars().map(|c| if c.is_control() || "<>:\"|?*".contains(c) { '_' } else { c }).collect();
    let clean = clean.trim_matches(['.', ' ']);
    if clean.is_empty() {
        return "file".into();
    }
    clean.chars().take(NAME_MAX).collect()
}

fn text(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

impl Shell {
    fn picked_dir(&self) -> PathBuf {
        self.root().join("picked")
    }

    fn preview_path(&self, message_id: &str, local_id: &str) -> PathBuf {
        self.root().join("previews").join(safe(message_id)).join(safe(local_id))
    }

    /// A place for a file the user chose: an empty file under its own name,
    /// in a folder of its own. The caller writes it and closes it; its path
    /// is then the file's `source`.
    pub async fn new_picked(&self, name: &str) -> R<(String, rt::fs::File)> {
        let dir = self.picked_dir().join(format!("{:032x}", rand::random::<u128>()));
        rt::fs::create_dir_all(&dir).await.map_err(s)?;
        let path = dir.join(clean_name(name));
        let file = rt::fs::OpenOptions::new().write(true).create_new(true).open(&path).await.map_err(s)?;
        Ok((text(&path), file))
    }

    /// Forget a chosen file that did not arrive whole.
    pub async fn drop_picked(&self, path: &str) {
        if let Some(dir) = Path::new(path).parent().filter(|d| d.parent() == Some(self.picked_dir().as_path())) {
            let _ = rt::fs::remove_dir_all(dir).await;
        }
    }

    /// `hc_file_info`: a chosen file's name and size.
    pub(crate) async fn file_info(&self, source: &str) -> R<Value> {
        let meta = rt::fs::metadata(source).await.map_err(|_| "Choose the file again.".to_string())?;
        if meta.is_dir() {
            return Err("it's a folder. Hubchat sends files, not folders.".into());
        }
        let name = Path::new(source).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "file".into());
        Ok(json!({ "name": name, "bytes": meta.len() }))
    }

    /// `hc_file_preview`: the bytes of an image the composer holds.
    async fn file_preview(&self, source: &str, name: &str) -> R<Vec<u8>> {
        if !is_image(name) {
            return Err("no preview for this file".into());
        }
        if rt::fs::metadata(source).await.map_err(s)?.len() > MAX_PREVIEW_BYTES {
            return Err("too big for a preview".into());
        }
        rt::fs::read(source).await.map_err(s)
    }

    /// `hc_attachment_preview`: an image attachment's bytes, from this
    /// browser's copy or once from the hub (never into Downloads).
    async fn attachment_preview(&self, message_id: &str, local_id: &str) -> R<Vec<u8>> {
        let m = self.store().message(message_id).map_err(s)?.ok_or("no such message")?;
        let a = m.attachments.iter().find(|a| a.local_id == local_id).ok_or("no such attachment")?;
        if !is_image(&a.name) || a.bytes > MAX_PREVIEW_BYTES {
            return Err("no preview for this file".into());
        }
        let copy = self.preview_path(message_id, local_id);
        if let Ok(b) = rt::fs::read(&copy).await {
            return Ok(b);
        }
        // sent from this browser: the chosen file, while it is kept; its
        // preview is kept apart, so it outlives that
        if m.outgoing {
            if let Some(src) = &a.source {
                if let Ok(b) = rt::fs::read(src).await {
                    if let Some(dir) = copy.parent() {
                        if rt::fs::create_dir_all(dir).await.is_ok() {
                            let _ = rt::fs::write(&copy, &b).await;
                        }
                    }
                    return Ok(b);
                }
            }
        }
        if let Some(p) = &a.local_path {
            if let Ok(b) = rt::fs::read(p).await {
                return Ok(b);
            }
        }
        self.engine()?.fetch_preview(message_id, local_id, &copy).await.map_err(s)?;
        rt::fs::read(&copy).await.map_err(s)
    }

    /// Where this browser's copy of an attachment is, and the name to hand
    /// it over under (`hc_open_attachment` in the apps).
    pub fn attachment_file(&self, message_id: &str, local_id: &str) -> R<(String, String)> {
        self.engine()?.attachment_source(message_id, local_id).map_err(s)
    }

    /// A command whose answer is bytes, not JSON.
    pub async fn bytes(&self, cmd: &str, args: &Value) -> R<Vec<u8>> {
        let word = |name: &str| args.get(name).and_then(Value::as_str).map(str::to_owned).ok_or_else(|| format!("missing argument: {name}"));
        match cmd {
            "hc_file_preview" => self.file_preview(&word("source")?, &word("name")?).await,
            "hc_attachment_preview" => self.attachment_preview(&word("messageId")?, &word("localId")?).await,
            _ => Err(crate::shell::NOT_YET.into()),
        }
    }

    /// At start: the previews of messages that are gone, and chosen files
    /// kept long enough.
    pub async fn sweep_files(&self) {
        if let Ok(entries) = rt::fs::read_dir_now(self.root().join("previews")) {
            for e in entries {
                if matches!(self.store().message(&e.name), Ok(None)) {
                    let _ = rt::fs::remove_dir_all(&e.path).await;
                }
            }
        }
        let now = rt::system_now();
        if let Ok(entries) = rt::fs::read_dir_now(self.picked_dir()) {
            for e in entries {
                // a folder has no date of its own in a browser: its file's
                let newest = rt::fs::read_dir_now(&e.path).ok().and_then(|inside| inside.iter().filter_map(|f| f.metadata.modified().ok()).max());
                let old = newest.is_none_or(|t| now.duration_since(t).is_ok_and(|age| age > PICKED_KEEP));
                if old {
                    let _ = rt::fs::remove_dir_all(&e.path).await;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shell::tests::{fresh_in, take};
    use tokio::io::AsyncWriteExt;

    fn folder() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("hubchat-web-files-{:016x}", rand::random::<u64>()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_chosen_file_keeps_its_name_and_stays_in_its_folder() {
        for (given, kept) in [
            ("holiday photo.PNG", "holiday photo.PNG"),
            ("../../etc/passwd", "passwd"),
            ("C:\\Users\\maya\\notes.txt", "notes.txt"),
            ("a:b?.txt", "a_b_.txt"),
            ("..", "file"),
            ("", "file"),
            (" . ", "file"),
        ] {
            assert_eq!(clean_name(given), kept, "{given}");
        }
        assert_eq!(clean_name(&"x".repeat(500)).len(), NAME_MAX);
        assert!(is_image("a.JPG") && is_image("b.webp") && !is_image("c.pdf") && !is_image("png"));
    }

    #[tokio::test]
    async fn a_chosen_picture_is_described_shown_and_sent() {
        let root = folder();
        let (sh, _) = fresh_in(root.clone());
        take(&sh, "hc_create_identity", json!({ "id": "maya", "name": "Maya" })).await;
        let data = b"\x89PNG\r\n\x1a\nnot really a picture".to_vec();
        let (path, mut f) = sh.new_picked("../holiday photo.PNG").await.unwrap();
        f.write_all(&data).await.unwrap();
        f.flush().await.unwrap();
        drop(f);
        assert!(Path::new(&path).starts_with(root.join("picked")), "{path}");
        assert_eq!(sh.call("hc_file_info", &json!({ "source": path })).await.unwrap(), json!({ "name": "holiday photo.PNG", "bytes": data.len() }));
        assert_eq!(sh.call("hc_file_info", &json!({ "source": text(&root.join("picked")) })).await.unwrap_err(), "it's a folder. Hubchat sends files, not folders.");
        assert_eq!(sh.call("hc_file_info", &json!({ "source": text(&root.join("nothing")) })).await.unwrap_err(), "Choose the file again.");
        assert_eq!(sh.bytes("hc_file_preview", &json!({ "source": path, "name": "holiday photo.PNG" })).await.unwrap(), data);
        assert_eq!(sh.bytes("hc_file_preview", &json!({ "source": path, "name": "notes.txt" })).await.unwrap_err(), "no preview for this file");

        // sent: the core takes its own copy, and the message carries the file
        sh.call("hc_send", &json!({ "msg": { "id": "m1", "peer": "ann.111111", "body": "", "attachments": [{ "name": "holiday photo.PNG", "bytes": data.len(), "source": path }] } }))
            .await
            .unwrap();
        let page = sh.call("hc_chat", &json!({ "peer": "ann.111111", "limit": 50 })).await.unwrap();
        let a = &page[0]["attachments"][0];
        assert_eq!(a["name"], "holiday photo.PNG", "{page}");
        assert_eq!(a["bytes"], data.len());
        let local_id = a["local_id"].as_str().unwrap().to_owned();
        assert!(std::fs::read_dir(root.join("outbox")).unwrap().count() >= 1, "the core's own copy");
        assert_eq!(sh.attachment_file("m1", &local_id).unwrap(), (path.clone(), "holiday photo.PNG".to_string()));

        // shown from the chosen file, and still shown when that is gone
        let args = json!({ "messageId": "m1", "localId": local_id });
        assert_eq!(sh.bytes("hc_attachment_preview", &args).await.unwrap(), data);
        sh.drop_picked(&path).await;
        assert!(!Path::new(&path).exists());
        assert_eq!(sh.bytes("hc_attachment_preview", &args).await.unwrap(), data);
        assert!(sh.attachment_file("m1", &local_id).is_err());

        // a file that is not there is not sent
        let e = sh
            .call("hc_send", &json!({ "msg": { "id": "m2", "peer": "ann.111111", "body": "", "attachments": [{ "name": "a.txt", "bytes": 1, "source": path }] } }))
            .await
            .unwrap_err();
        assert!(!e.is_empty());
        assert_eq!(sh.call("hc_message", &json!({ "id": "m2" })).await.unwrap(), Value::Null);
        sh.shutdown();
        let _ = std::fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn the_start_sweeps_what_nothing_needs() {
        let root = folder();
        let (sh, _) = fresh_in(root.clone());
        let gone = root.join("previews").join("no-such-message");
        std::fs::create_dir_all(&gone).unwrap();
        std::fs::write(gone.join("a1"), b"x").unwrap();
        let (fresh_file, mut f) = sh.new_picked("new.txt").await.unwrap();
        f.write_all(b"new").await.unwrap();
        f.flush().await.unwrap();
        drop(f);
        let empty = root.join("picked").join("left-empty");
        std::fs::create_dir_all(&empty).unwrap();
        sh.sweep_files().await;
        assert!(!gone.exists(), "a preview of a message that is gone");
        assert!(!empty.exists(), "a folder whose file never arrived");
        assert!(Path::new(&fresh_file).exists(), "a file chosen just now");
        sh.shutdown();
        let _ = std::fs::remove_dir_all(root);
    }
}
