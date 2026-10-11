//! Private immutable copies admitted before a message enters the network queue.
use crate::{engine::Host, store::NewOutgoing, Error, Result};
use sha2::{Digest, Sha256};
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::path::PathBuf;

pub(crate) struct Prepared {
    pub message: NewOutgoing,
    pub digests: Vec<String>,
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
    let mut buffer = vec![0u8; 256 * 1024];
    for (index, a) in prepared.message.attachments.iter_mut().enumerate() {
        if a.bytes > i64::MAX as u64 {
            return Err(Error::Invalid("file is too large".into()));
        }
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
        }
        let after = source.metadata()?;
        if total != a.bytes || before.modified().ok() != after.modified().ok() {
            return Err(Error::Invalid(format!(
                "{} changed while preparing; select it again",
                a.name
            )));
        }
        dest.sync_all()?;
        prepared.digests.push(hex::encode(hash.finalize()));
        a.source = path.to_string_lossy().into_owned();
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
