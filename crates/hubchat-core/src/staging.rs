//! Private immutable copies admitted before a message enters the network queue.
use crate::{engine::Host, store::NewOutgoing, Error, Result};
use sha2::{Digest, Sha256};
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::path::PathBuf;

pub(crate) struct Prepared {
    pub message: NewOutgoing,
    pub digests: Vec<String>,
    pub paths: Vec<String>,
    directory: Option<PathBuf>,
}
impl Prepared {
    // Called immediately after the queue transaction, with no cancellation point.
    pub fn commit(mut self) -> NewOutgoing {
        self.directory = None;
        self.message.clone()
    }
}
impl Drop for Prepared {
    fn drop(&mut self) {
        // Only our newly-created UUID directory; never a caller-supplied path.
        if let Some(dir) = &self.directory {
            let _ = fs::remove_dir_all(dir);
        }
    }
}

pub(crate) fn prepare(host: &dyn Host, message: NewOutgoing) -> Result<Prepared> {
    let mut prepared = Prepared {
        message,
        digests: Vec::new(),
        paths: Vec::new(),
        directory: None,
    };
    if prepared.message.attachments.is_empty() {
        return Ok(prepared);
    }
    let root = host.staging_dir();
    fs::create_dir_all(&root)?;
    let dir = root.join(uuid::Uuid::new_v4().simple().to_string());
    fs::create_dir(&dir)?;
    prepared.directory = Some(dir.clone());
    let grand_total = prepared
        .message
        .attachments
        .iter()
        .try_fold(0u64, |n, a| n.checked_add(a.bytes))
        .ok_or_else(|| Error::Invalid("attachments are too large".into()))?;
    let mut copied = 0u64;
    let mut tick = std::time::Instant::now();
    let report = |name: &str, done| {
        host.event(crate::engine::Event::Preparing {
            message_id: prepared.message.id.clone(),
            peer: prepared.message.peer.clone(),
            name: name.into(),
            done,
            total: grand_total,
        })
    };
    let mut buffer = vec![0u8; 256 * 1024];
    for (index, a) in prepared.message.attachments.iter().enumerate() {
        if a.bytes > i64::MAX as u64 {
            return Err(Error::Invalid("file is too large".into()));
        }
        report(&a.name, copied);
        let mut source = host.open_source(&a.source)?;
        let before = source.metadata()?;
        let path = dir.join(format!("{index}.bin"));
        let mut dest = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&path)?;
        let mut total = 0u64;
        let mut hash = Sha256::new();
        loop {
            // Bound reading even if the selected source keeps growing.
            let want = buffer
                .len()
                .min((a.bytes - total + 1).min(buffer.len() as u64) as usize);
            let n = source.read(&mut buffer[..want])?;
            if n == 0 {
                break;
            }
            total += n as u64;
            if total > a.bytes {
                return Err(Error::Invalid(format!(
                    "{} changed size; select it again",
                    a.name
                )));
            }
            dest.write_all(&buffer[..n])?;
            hash.update(&buffer[..n]);
            if tick.elapsed() >= std::time::Duration::from_millis(100) {
                report(&a.name, copied + total);
                tick = std::time::Instant::now();
            }
        }
        let after = source.metadata()?;
        if total != a.bytes || (before.is_file() && before.modified().ok() != after.modified().ok())
        {
            return Err(Error::Invalid(format!(
                "{} changed while preparing; select it again",
                a.name
            )));
        }
        dest.sync_all()?;
        copied += total;
        report(&a.name, copied);
        prepared.digests.push(hex::encode(hash.finalize()));
        prepared.paths.push(path.to_string_lossy().into_owned());
    }
    sync_dir(&dir)?;
    sync_dir(&root)?;
    Ok(prepared)
}

fn sync_dir(path: &std::path::Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        std::fs::File::open(path)?.sync_all()?;
    }
    #[cfg(not(unix))]
    {
        let _ = path;
    }
    Ok(())
}

fn private_file(root: &std::path::Path, path: &std::path::Path) -> bool {
    let Some(dir) = path.parent() else {
        return false;
    };
    dir.parent() == Some(root)
        && dir
            .file_name()
            .and_then(|x| x.to_str())
            .is_some_and(uuid_name)
        && path
            .file_stem()
            .and_then(|x| x.to_str())
            .is_some_and(|s| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()))
        && path.extension().is_some_and(|x| x == "bin")
        && !dir
            .symlink_metadata()
            .is_ok_and(|m| m.file_type().is_symlink())
}
fn uuid_name(s: &str) -> bool {
    s.len() == 32 && s.bytes().all(|b| b.is_ascii_hexdigit())
}
fn ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

/// A durable queue is filled in the same transaction as send/deletion. Failed
/// removals stay due for retry; a missing file is already collected.
pub(crate) fn collect(root: &std::path::Path, store: &crate::store::Store) -> Result<()> {
    let at = ms();
    for path in store.staging_due(at)? {
        let file = std::path::Path::new(&path);
        if !private_file(root, file) || store.staging_in_use(&path)? {
            store.staging_retry(&path, at + 30_000)?;
            continue;
        }
        match fs::remove_file(file) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => {
                store.staging_retry(&path, at + 30_000)?;
                continue;
            }
        }
        store.staging_removed(&path)?;
        // Removes only an empty UUID folder; other files are never unlinked here.
        if let Some(dir) = file.parent() {
            let _ = fs::remove_dir(dir);
        }
    }
    Ok(())
}

/// Startup removes old abandoned pre-admission copies. The 24-hour grace also
/// protects a concurrent send still preparing a folder with no database row.
pub(crate) fn sweep_orphans(root: &std::path::Path, store: &crate::store::Store) -> Result<()> {
    let entries = match fs::read_dir(root) {
        Ok(e) => e,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e.into()),
    };
    for entry in entries {
        let entry = entry?;
        if !entry.file_type()?.is_dir() || !entry.file_name().to_str().is_some_and(uuid_name) {
            continue;
        }
        let age = entry.metadata()?.modified()?.elapsed().unwrap_or_default();
        if age < std::time::Duration::from_secs(24 * 60 * 60) {
            continue;
        }
        let dir = entry.path();
        let prefix = format!("{}{}", dir.display(), std::path::MAIN_SEPARATOR);
        if !store.staging_directory_referenced(&prefix)? {
            let _ = fs::remove_dir_all(dir);
        }
    }
    collect(root, store)
}

/// Name storage exhaustion plainly instead of leaking an opaque OS error.
pub(crate) fn explain_error(e: Error) -> Error {
    if let Error::Io(ref io) = e {
        #[cfg(windows)]
        let full = matches!(io.raw_os_error(), Some(39 | 112));
        #[cfg(not(windows))]
        let full = io.raw_os_error() == Some(28);
        if full {
            return Error::Invalid(
                "not enough space to prepare these files; free space and try again".into(),
            );
        }
    }
    e
}
