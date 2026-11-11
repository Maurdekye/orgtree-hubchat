//! `staging` in a browser: the same private copies, made with `rt::fs`
//! (Hubchat Web's own files), where nothing can block and wait.
//!
//! A source is itself one of Hubchat Web's files: the copy the web shell made
//! when the file was picked. It is still copied, as the apps copy theirs, so a
//! message that isn't queued after all leaves the picked file where it was.
use super::{ms, private_file, uuid_name, Prepared};
use crate::{engine::Host, store::NewOutgoing, Error, Result};
use sha2::{Digest, Sha256};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

pub(crate) async fn prepare(host: &dyn Host, message: NewOutgoing) -> Result<Prepared> {
    let mut prepared = Prepared { message, digests: Vec::new(), paths: Vec::new(), directory: None };
    if prepared.message.attachments.is_empty() {
        return Ok(prepared);
    }
    let root = host.staging_dir();
    crate::rt::fs::create_dir_all(&root).await?;
    let dir = root.join(uuid::Uuid::new_v4().simple().to_string());
    crate::rt::fs::create_dir(&dir).await?;
    prepared.directory = Some(dir.clone());
    let grand_total = prepared
        .message
        .attachments
        .iter()
        .try_fold(0u64, |n, a| n.checked_add(a.bytes))
        .ok_or_else(|| Error::Invalid("attachments are too large".into()))?;
    let mut copied = 0u64;
    let mut tick = crate::rt::Instant::now();
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
    let mut digests = Vec::new();
    let mut paths = Vec::new();
    for (index, a) in prepared.message.attachments.iter().enumerate() {
        if a.bytes > i64::MAX as u64 {
            return Err(Error::Invalid("file is too large".into()));
        }
        report(&a.name, copied);
        let mut source = crate::engine::open_source_file(host, &a.source).await?;
        let before = source.metadata_now()?;
        let path = dir.join(format!("{index}.bin"));
        let mut dest = crate::rt::fs::OpenOptions::new().create_new(true).write(true).open(&path).await?;
        let mut total = 0u64;
        let mut hash = Sha256::new();
        loop {
            let want = buffer.len().min((a.bytes - total + 1).min(buffer.len() as u64) as usize);
            let n = source.read(&mut buffer[..want]).await?;
            if n == 0 {
                break;
            }
            total += n as u64;
            if total > a.bytes {
                return Err(Error::Invalid(format!("{} changed size; select it again", a.name)));
            }
            dest.write_all(&buffer[..n]).await?;
            hash.update(&buffer[..n]);
            if tick.elapsed() >= std::time::Duration::from_millis(100) {
                report(&a.name, copied + total);
                tick = crate::rt::Instant::now();
                // the one thread also runs the chat: let it, between pieces
                crate::rt::time::sleep(std::time::Duration::ZERO).await;
            }
        }
        let after = source.metadata_now()?;
        if total != a.bytes || before.modified().ok() != after.modified().ok() {
            return Err(Error::Invalid(format!("{} changed while preparing; select it again", a.name)));
        }
        dest.sync_all().await?;
        copied += total;
        report(&a.name, copied);
        digests.push(hex::encode(hash.finalize()));
        paths.push(path.to_string_lossy().into_owned());
    }
    prepared.digests = digests;
    prepared.paths = paths;
    Ok(prepared)
}

pub(crate) async fn collect(root: &std::path::Path, store: &crate::store::Store) -> Result<()> {
    let at = ms();
    for path in store.staging_due(at)? {
        let file = std::path::Path::new(&path);
        if !private_file(root, file) || store.staging_in_use(&path)? {
            store.staging_retry(&path, at + 30_000)?;
            continue;
        }
        match crate::rt::fs::remove_file(file).await {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => {
                store.staging_retry(&path, at + 30_000)?;
                continue;
            }
        }
        store.staging_removed(&path)?;
        if let Some(dir) = file.parent() {
            let _ = crate::rt::fs::remove_dir(dir).await;
        }
    }
    Ok(())
}

pub(crate) async fn sweep_orphans(root: &std::path::Path, store: &crate::store::Store) -> Result<()> {
    let entries = match crate::rt::fs::read_dir_now(root) {
        Ok(e) => e,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e.into()),
    };
    let now = crate::rt::system_now();
    for entry in entries {
        if !entry.metadata.is_dir() || !uuid_name(&entry.name) {
            continue;
        }
        let age = now.duration_since(entry.metadata.modified()?).unwrap_or_default();
        if age < std::time::Duration::from_secs(24 * 60 * 60) {
            continue;
        }
        let prefix = format!("{}{}", entry.path.display(), std::path::MAIN_SEPARATOR);
        if !store.staging_directory_referenced(&prefix)? {
            let _ = crate::rt::fs::remove_dir_all(&entry.path).await;
        }
    }
    collect(root, store).await
}
