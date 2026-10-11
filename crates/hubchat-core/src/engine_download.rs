//! Stored download checkpoints and recoverable publication. The public await
//! adapter is kept until the scheduler/command slice supplies queued downloads.
use super::*;
use crate::download::{DownloadCheckpoint, DownloadSpec};
use crate::store::DownloadJob;
use sha2::{Digest, Sha256};
use tokio::io::AsyncReadExt;

struct Writer<'a> {
    engine: &'a Engine,
    id: String,
    attempt: i64,
}
impl Drop for Writer<'_> {
    fn drop(&mut self) {
        // Also runs if the caller drops its future: release ownership, preserve
        // confirmed bytes, and never leave a stuck in-memory writer.
        let _ = self.engine.store.download_error(
            &self.id,
            self.attempt,
            "paused",
            "download interrupted; retry to resume",
        );
        self.engine.transfers.lock().unwrap().remove(&self.id);
    }
}
async fn digest(path: &std::path::Path, expected: u64) -> Result<String> {
    let mut file = tokio::fs::File::open(path).await?;
    if file.metadata().await?.len() != expected {
        return Err(Error::Invalid(
            "download bytes are incomplete; retry the download".into(),
        ));
    }
    let mut hash = Sha256::new();
    let mut bytes = vec![0u8; 256 * 1024];
    let mut total = 0;
    loop {
        let n = file.read(&mut bytes).await?;
        if n == 0 {
            break;
        }
        total += n as u64;
        if total > expected {
            return Err(Error::Invalid("download changed during publication".into()));
        }
        hash.update(&bytes[..n]);
    }
    if total != expected {
        return Err(Error::Invalid("download changed during publication".into()));
    }
    Ok(hex::encode(hash.finalize()))
}
fn hide_partial_dir(path: &std::path::Path) -> std::io::Result<()> {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        #[link(name = "Kernel32")]
        extern "system" {
            fn GetFileAttributesW(path: *const u16) -> u32;
            fn SetFileAttributesW(path: *const u16, attributes: u32) -> i32;
        }
        let wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
        if wide[..wide.len() - 1].contains(&0) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "invalid download path",
            ));
        }
        let attrs = unsafe { GetFileAttributesW(wide.as_ptr()) };
        if attrs == u32::MAX || unsafe { SetFileAttributesW(wide.as_ptr(), attrs | 2) } == 0 {
            return Err(std::io::Error::last_os_error());
        }
    }
    #[cfg(not(windows))]
    let _ = path;
    Ok(())
}
async fn remove_empty_partial_dir(partial: &std::path::Path) {
    if let Some(parent) = partial
        .parent()
        .filter(|p| p.file_name().is_some_and(|n| n == ".hubchat-downloads"))
    {
        // Never recursive. Another download's partial keeps the directory alive.
        let _ = tokio::fs::remove_dir(parent).await;
    }
}
async fn publish_file(partial: &std::path::Path, dest: &std::path::Path) -> std::io::Result<()> {
    #[cfg(windows)]
    {
        let (partial, dest) = (partial.to_path_buf(), dest.to_path_buf());
        tokio::task::spawn_blocking(move || {
            use std::os::windows::ffi::OsStrExt;
            #[link(name = "Kernel32")]
            extern "system" {
                fn MoveFileExW(from: *const u16, to: *const u16, flags: u32) -> i32;
            }
            let from: Vec<u16> = partial.as_os_str().encode_wide().chain(Some(0)).collect();
            let to: Vec<u16> = dest.as_os_str().encode_wide().chain(Some(0)).collect();
            if from[..from.len() - 1].contains(&0) || to[..to.len() - 1].contains(&0) {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "invalid download path",
                ));
            }
            // No REPLACE_EXISTING: a racing file is never overwritten, including
            // on FAT volumes which cannot use the Unix hard-link operation.
            if unsafe { MoveFileExW(from.as_ptr(), to.as_ptr(), 0) } == 0 {
                Err(std::io::Error::last_os_error())
            } else {
                Ok(())
            }
        })
        .await
        .map_err(std::io::Error::other)?
    }
    #[cfg(not(windows))]
    {
        // rename on Unix overwrites; hard_link is atomic and never replaces.
        tokio::fs::hard_link(partial, dest).await
    }
}
impl Engine {
    /// Platform export (e.g. Android MediaStore) must move both durable pointers.
    pub fn record_download_export(&self, local_id: &str, from: &str, to: &str) -> Result<()> {
        self.store.download_exported(local_id, from, to)
    }
    /// Queue an explicit user request durably; no network or file copy here.
    pub fn queue_download(&self, message_id: &str, local_id: &str) -> Result<DownloadJob> {
        let m = self
            .store
            .message(message_id)?
            .ok_or_else(|| Error::Invalid("no such message".into()))?;
        let a = m
            .attachments
            .iter()
            .find(|a| a.local_id == local_id)
            .ok_or_else(|| Error::Invalid("no such attachment".into()))?;
        if a.hub_id.is_none() || a.hub.as_ref().or(m.hub.as_ref()).is_none() {
            return Err(Error::Invalid("not on a hub".into()));
        }
        // This synchronous admission runs on the shell's blocking pool. Check
        // platform URIs as well as files before reusing a completed request.
        if let Some(previous) = self.store.download_job(local_id)? {
            if previous.state == "done" && self.host.open_source(&previous.destination).is_err() {
                self.store.restart_missing_download(local_id)?;
            }
        }
        let dir = self.host.download_dir();
        let part = dir
            .join(".hubchat-downloads")
            .join(format!("{}.part", uuid::Uuid::new_v4().simple()));
        let dest = unique_path(&dir, &a.name);
        self.store.queue_download_job(
            local_id,
            &part.to_string_lossy(),
            &dest.to_string_lossy(),
        )?;
        self.queue_changed.notify_one();
        self.host.event(Event::Chat { peer: m.peer });
        self.store
            .download_job(local_id)?
            .ok_or_else(|| Error::Invalid("attachment was deleted".into()))
    }
    pub async fn download(&self, message_id: &str, local_id: &str) -> Result<PathBuf> {
        self.download_with_slot(message_id, local_id, None).await
    }
    pub(super) async fn download_with_slot(
        &self,
        message_id: &str,
        local_id: &str,
        reserved: Option<transfer_slots::Permit>,
    ) -> Result<PathBuf> {
        let m = self
            .store
            .message(message_id)?
            .ok_or_else(|| Error::Invalid("no such message".into()))?;
        let a = m
            .attachments
            .iter()
            .find(|a| a.local_id == local_id)
            .ok_or_else(|| Error::Invalid("no such attachment".into()))?;
        let hub_id = a
            .hub_id
            .clone()
            .ok_or_else(|| Error::Invalid("not on a hub".into()))?;
        let hub = a
            .hub
            .clone()
            .or_else(|| m.hub.clone())
            .ok_or_else(|| Error::Invalid("unknown hub".into()))?;
        let client = self
            .hubs
            .lock()
            .unwrap()
            .get(&hub)
            .map(|h| h.client.clone())
            .ok_or_else(|| Error::Invalid(format!("hub {hub} was removed")))?;
        let cancel = CancelFlag::default();
        let dir = self.host.download_dir();
        let partial_dir = dir.join(".hubchat-downloads");
        let partial = partial_dir.join(format!("{}.part", uuid::Uuid::new_v4().simple()));
        let dest = unique_path(&dir, &a.name);
        let job = {
            let mut writers = self.transfers.lock().unwrap();
            if writers.contains_key(local_id) {
                return Err(Error::Invalid(
                    "this file already has a transfer in progress".into(),
                ));
            }
            if let Some(previous) = self.store.download_job(local_id)? {
                if reserved.is_some()
                    && !matches!(
                        previous.state.as_str(),
                        "queued" | "running" | "publishing" | "exporting" | "paused"
                    )
                {
                    return Err(Error::Cancelled);
                }
                if previous.state == "done" {
                    if self.host.open_source(&previous.destination).is_ok() {
                        return Ok(previous.destination.into());
                    }
                    self.store.restart_missing_download(local_id)?;
                }
            }
            let job = self.store.begin_download(
                local_id,
                &partial.to_string_lossy(),
                &dest.to_string_lossy(),
            )?;
            writers.insert(local_id.into(), cancel.clone());
            job
        };
        let _claim = Writer {
            engine: self,
            id: local_id.into(),
            attempt: job.attempt,
        };
        self.host.event(Event::Transfer {
            local_id: local_id.into(), message_id: message_id.into(), upload: false,
            done: job.checkpoint.confirmed, total: a.bytes,
        });
        self.host.event(Event::Chat {
            peer: m.peer.clone(),
        });
        let result = async {
            let _slot = match reserved {
                Some(slot) => slot,
                None => {
                    cancel
                        .run(async { Ok(self.file_slots.acquire(&hub).await) })
                        .await?
                }
            };
            if job.state == "exporting" {
                return self.export_download(&job, &a.name, a.bytes, false).await;
            }
            if job.state != "publishing" {
                // Reserve the partial while holding the same lock as directory
                // removal. A concurrent completion cannot remove a new writer's
                // empty directory between create_dir and open.
                {
                    let _publication = self.download_publication.lock().await;
                    let path = std::path::Path::new(&job.partial);
                    let parent = path
                        .parent()
                        .ok_or_else(|| Error::Invalid("invalid partial path".into()))?;
                    tokio::fs::create_dir_all(parent).await?;
                    if parent
                        .file_name()
                        .is_some_and(|n| n == ".hubchat-downloads")
                    {
                        // Visibility is cosmetic; some network volumes refuse attributes.
                        let _ = hide_partial_dir(parent);
                    }
                    let _ = tokio::fs::OpenOptions::new()
                        .create(true)
                        .truncate(false)
                        .write(true)
                        .open(path)
                        .await?;
                }
                let store = self.store.clone();
                let lid = local_id.to_owned();
                let attempt = job.attempt;
                let checkpoint_host = self.host.clone();
                let mid = message_id.to_owned();
                let total = a.bytes;
                let save = Arc::new(move |cp: &DownloadCheckpoint| {
                    store.download_checkpoint(&lid, attempt, cp)?;
                    checkpoint_host.event(Event::Transfer {
                        local_id: lid.clone(), message_id: mid.clone(), upload: false,
                        done: cp.confirmed, total,
                    });
                    Ok(())
                });
                client
                    .download_resumable(
                        &self.me,
                        DownloadSpec {
                            attachment_id: &hub_id,
                            partial: std::path::Path::new(&job.partial),
                            total: a.bytes,
                            checkpoint: job.checkpoint.clone(),
                        },
                        save,
                        None, // Confirmed checkpoint events above, not wire bytes.
                        cancel.clone(),
                    )
                    .await?;
            }
            self.finish_download(&job, a.bytes, &dir, &a.name).await?;
            self.host.event(Event::Chat { peer: m.peer.clone() });
            let publication = self
                .store
                .download_job(local_id)?
                .ok_or_else(|| Error::Invalid("attachment was deleted".into()))?;
            self.export_download(&publication, &a.name, a.bytes, true).await
        }
        .await;
        if let Err(ref e) = result {
            let state = if cancel.is_cancelled() || super::engine_scheduler::transient(e) {
                "paused"
            } else if e.status() == Some(410) {
                "expired"
            } else {
                "failed"
            };
            self.store
                .download_error(local_id, job.attempt, state, &e.to_string())?;
        }
        self.host.event(Event::Chat { peer: m.peer });
        result
    }

    async fn export_download(&self, job: &DownloadJob, name: &str, total: u64, freshly_published: bool) -> Result<PathBuf> {
        let host = self.host.clone();
        let source = PathBuf::from(&job.destination);
        let path = source.clone();
        let name = name.to_owned();
        let key = format!("{}-{}", job.local_id, job.digest.as_deref().unwrap_or(""));
        // The scheduler owns this future through shutdown: a UI command does
        // not own the copy, and a blocking platform call never owns a DB lease.
        let export_key = key.clone();
        let exported =
            tokio::task::spawn_blocking(move || host.publish_download(&export_key, &path, &name))
                .await
                .map_err(std::io::Error::other)?;
        // Give a journalled platform export the first chance to recover its
        // completed public URI, even when private bytes disappeared meanwhile.
        // Without such a receipt, damaged private bytes need a fresh download.
        if !(matches!(&exported, Ok(Some(path)) if path != &job.destination)
            || freshly_published && exported.is_ok())
        {
            let damaged = match digest(&source, total).await {
                Ok(hash) => job.digest.as_ref() != Some(&hash),
                Err(Error::Io(e)) if e.kind() == std::io::ErrorKind::NotFound => true,
                Err(Error::Invalid(_)) => true,
                Err(Error::Io(_)) => return Err(Error::LocalBusy),
                Err(e) => return Err(e),
            };
            if damaged {
                self.store.reset_damaged_download(&job.local_id, job.attempt)?;
                return Err(Error::Invalid(
                    "download bytes changed or disappeared; retry from zero".into(),
                ));
            }
        }
        let destination = exported?.unwrap_or_else(|| job.destination.clone());
        self.store
            .download_exported(&job.local_id, &job.destination, &destination)?;
        let host = self.host.clone();
        let _ = tokio::task::spawn_blocking(move || host.download_export_committed(&key)).await;
        if destination != job.destination {
            // Only our private completed download, never the original picked file.
            let _ = tokio::fs::remove_file(source).await;
        }
        Ok(destination.into())
    }

    async fn finish_download(
        &self,
        job: &DownloadJob,
        total: u64,
        dir: &std::path::Path,
        name: &str,
    ) -> Result<PathBuf> {
        let part = std::path::Path::new(&job.partial);
        let existing = std::path::Path::new(&job.destination);
        // The final path can exist after a crash between publishing bytes and
        // committing the local_path. Validate full contents, never just its size.
        if job.state == "publishing" {
            if let Some(want) = &job.digest {
                if digest(existing, total).await.is_ok_and(|got| got == *want) {
                    self.store
                        .download_done(&job.local_id, job.attempt, &job.destination)?;
                    let _publication = self.download_publication.lock().await;
                    let _ = tokio::fs::remove_file(part).await;
                    remove_empty_partial_dir(part).await;
                    return Ok(existing.into());
                }
            }
        }
        let hash = match digest(part, total).await {
            Ok(hash) if job.state != "publishing" || job.digest.as_ref() == Some(&hash) => hash,
            _ => {
                self.store
                    .reset_damaged_download(&job.local_id, job.attempt)?;
                return Err(Error::Invalid(
                    "download bytes changed or disappeared; retry from zero".into(),
                ));
            }
        };
        // Serializes our own name selection only. The filesystem operation also
        // refuses to overwrite a file created externally after that selection.
        let _publication = self.download_publication.lock().await;
        for _ in 0..32 {
            let dest = unique_path(dir, name);
            self.store.download_publishing(
                &job.local_id,
                job.attempt,
                &hash,
                &dest.to_string_lossy(),
            )?;
            match publish_file(part, &dest).await {
                Ok(()) => {
                    #[cfg(unix)]
                    {
                        std::fs::File::open(dir)?.sync_all()?;
                    }
                    self.store.download_done(
                        &job.local_id,
                        job.attempt,
                        &dest.to_string_lossy(),
                    )?;
                    let _ = tokio::fs::remove_file(part).await;
                    remove_empty_partial_dir(part).await;
                    return Ok(dest);
                }
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(e) => return Err(e.into()),
            }
        }
        Err(Error::Invalid(
            "download destination keeps changing; retry".into(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::NewAttachment;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;
    struct LocalHost(PathBuf);
    impl Host for LocalHost {
        fn event(&self, _: Event) {}
        fn open_source(&self, p: &str) -> std::io::Result<std::fs::File> {
            std::fs::File::open(p)
        }
        fn download_dir(&self) -> PathBuf {
            self.0.clone()
        }
    }
    fn fixture(store: Arc<Store>, dir: &std::path::Path, url: &str) -> Arc<Engine> {
        fixture_host(store, url, Arc::new(LocalHost(dir.into())))
    }
    fn fixture_host(store: Arc<Store>, url: &str, host: Arc<dyn Host>) -> Arc<Engine> {
        let e = Engine::new(
            store,
            Identity::generate("download-proof").unwrap(),
            Profile::default(),
            host,
        );
        e.hubs.lock().unwrap().insert(
            url.into(),
            HubRuntime {
                client: HubClient::new(HubAddress::parse(url).unwrap()),
                status: HubStatus {
                    url: url.into(),
                    name: "scratch".into(),
                    state: HubState::Connected,
                    error: None,
                    retry_at_ms: None,
                    max_attachment_bytes: 1073741824,
                    features: vec!["uploads".into()],
                    version: None,
                    answered_ms: None,
                    waiting_since_ms: None,
                    clock_offset_ms: None,
                },
                retry_now: Arc::new(Notify::new()),
                kick: Arc::new(Notify::new()),
                stop: CancelFlag::default(),
            },
        );
        e
    }
    fn attach(store: &Store, url: &str, n: u64) -> String {
        store
            .queue_outgoing(
                &NewOutgoing {
                    id: "message".into(),
                    peer: "other.123456".into(),
                    body: "file".into(),
                    kind: None,
                    reply_to: None,
                    attachments: vec![NewAttachment {
                        name: "proof.bin".into(),
                        bytes: n,
                        source: "unused".into(),
                    }],
                },
                &now(),
            )
            .unwrap();
        let lid = store.message("message").unwrap().unwrap().attachments[0]
            .local_id
            .clone();
        store
            .set_upload(&lid, "uploaded", "remote-file", url)
            .unwrap();
        lid
    }
    #[test]
    fn queued_download_projection_survives_reopen_and_keeps_private_paths_hidden() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("store.db");
        let store = Arc::new(Store::open(&db).unwrap());
        let lid = attach(&store, "http://127.0.0.1:9", 9 * 1024 * 1024);
        let e = fixture(store.clone(), dir.path(), "http://127.0.0.1:9");
        let queued = e.queue_download("message", &lid).unwrap();
        assert_eq!(queued.state, "queued");
        assert!(!PathBuf::from(&queued.partial).exists());
        let j = store.begin_download(&lid, &queued.partial, &queued.destination).unwrap();
        store.download_checkpoint(&lid, j.attempt, &DownloadCheckpoint {
            confirmed: 8 * 1024 * 1024, validator: Some("private-validator".into()), updated_ms: 1,
        }).unwrap();
        drop(e); drop(store);
        let store = Store::open(&db).unwrap();
        let m = store.message("message").unwrap().unwrap();
        let d = m.attachments[0].download.as_ref().unwrap();
        assert_eq!(d.state, "running");
        assert_eq!(d.confirmed, 8 * 1024 * 1024);
        let public = serde_json::to_string(&m).unwrap();
        assert!(!public.contains("private-validator"));
        assert!(!public.contains(".hubchat-downloads"));
        store.cancel_download(&lid).unwrap();
        assert_eq!(store.message("message").unwrap().unwrap().attachments[0].download.as_ref().unwrap().state, "cancelled");
    }

    async fn headers(s: &mut tokio::net::TcpStream) -> String {
        let mut out = Vec::new();
        while !out.ends_with(b"\r\n\r\n") {
            let mut b = [0];
            s.read_exact(&mut b).await.unwrap();
            out.push(b[0]);
            assert!(out.len() < 16000);
        }
        String::from_utf8(out).unwrap()
    }
    async fn until(mut f: impl FnMut() -> bool) {
        tokio::time::timeout(Duration::from_secs(5), async {
            while !f() {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
    }
    #[tokio::test]
    async fn stored_download_resumes_after_disconnect_and_store_reopen() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("store.db");
        let downloads = dir.path().join("downloads");
        std::fs::create_dir(&downloads).unwrap();
        std::fs::write(downloads.join("proof.bin"), b"keep me").unwrap();
        let store = Arc::new(Store::open(&db).unwrap());
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let total = 9 * 1024 * 1024;
        let lid = attach(&store, &url, total);
        let (cut, wait) = tokio::sync::oneshot::channel();
        let server = tokio::spawn(async move {
            let (mut s, _) = listener.accept().await.unwrap();
            let h = headers(&mut s).await;
            assert!(!h.to_lowercase().contains("range:"));
            s.write_all(format!("HTTP/1.1 200 OK\r\nContent-Length: {total}\r\nETag: \"fixed\"\r\nConnection: close\r\n\r\n").as_bytes()).await.unwrap();
            s.write_all(&vec![19; 8 * 1024 * 1024]).await.unwrap();
            let _ = wait.await;
            drop(s);
            let (mut s, _) = listener.accept().await.unwrap();
            let h = headers(&mut s).await.to_lowercase();
            assert!(h.contains("range: bytes=8388608-"));
            assert!(h.contains("if-range: \"fixed\""));
            s.write_all(b"HTTP/1.1 206 Partial Content\r\nContent-Length: 1048576\r\nContent-Range: bytes 8388608-9437183/9437184\r\nETag: \"fixed\"\r\nConnection: close\r\n\r\n").await.unwrap();
            s.write_all(&vec![19; 1024 * 1024]).await.unwrap();
        });
        let e = fixture(store.clone(), &downloads, &url);
        let e2 = e.clone();
        let lid2 = lid.clone();
        let task = tokio::spawn(async move { e2.download("message", &lid2).await });
        until(|| {
            store
                .download_job(&lid)
                .unwrap()
                .is_some_and(|j| j.checkpoint.confirmed == 8 * 1024 * 1024)
        })
        .await;
        assert!(!downloads.join("proof (2).bin").exists());
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            assert_ne!(
                std::fs::metadata(downloads.join(".hubchat-downloads"))
                    .unwrap()
                    .file_attributes()
                    & 2,
                0
            );
        }
        cut.send(()).unwrap();
        assert!(task.await.unwrap().is_err());
        assert_eq!(store.download_job(&lid).unwrap().unwrap().state, "failed");
        drop(e);
        drop(store);
        let store = Arc::new(Store::open(&db).unwrap());
        let e = fixture(store.clone(), &downloads, &url);
        let path = e.download("message", &lid).await.unwrap();
        assert_eq!(std::fs::read(path).unwrap(), vec![19; total as usize]);
        assert_eq!(
            std::fs::read(downloads.join("proof.bin")).unwrap(),
            b"keep me"
        );
        let job = store.download_job(&lid).unwrap().unwrap();
        assert_eq!(job.state, "done");
        assert_eq!(job.attempt, 2);
        assert!(!std::path::Path::new(&job.partial).exists());
        assert!(!downloads.join(".hubchat-downloads").exists());
        server.await.unwrap();
    }
    #[tokio::test]
    async fn stored_cancel_stops_socket_and_rejects_stale_checkpoints() {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(Store::open_in_memory().unwrap());
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let lid = attach(&store, &url, 3);
        let (entered, wait) = tokio::sync::oneshot::channel();
        let server = tokio::spawn(async move {
            let (mut s, _) = listener.accept().await.unwrap();
            headers(&mut s).await;
            entered.send(()).unwrap();
            let mut b = [0];
            assert_eq!(s.read(&mut b).await.unwrap(), 0);
        });
        let e = fixture(store.clone(), dir.path(), &url);
        let e2 = e.clone();
        let lid2 = lid.clone();
        let task = tokio::spawn(async move { e2.download("message", &lid2).await });
        wait.await.unwrap();
        e.cancel_transfer(&lid).unwrap();
        assert!(matches!(task.await.unwrap(), Err(Error::Cancelled)));
        server.await.unwrap();
        let j = store.download_job(&lid).unwrap().unwrap();
        assert_eq!(j.state, "cancelled");
        assert!(store
            .download_checkpoint(&lid, j.attempt, &DownloadCheckpoint::default())
            .is_err());
        let retry = store
            .begin_download(&lid, &j.partial, &j.destination)
            .unwrap();
        assert_eq!(retry.attempt, j.attempt + 1);
        assert!(store
            .download_checkpoint(&lid, j.attempt, &DownloadCheckpoint::default())
            .is_err());
    }
    #[tokio::test]
    async fn publication_recovers_after_move_without_network_or_overwrite() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("store.db");
        let store = Arc::new(Store::open(&db).unwrap());
        let url = "http://127.0.0.1:9";
        let lid = attach(&store, url, 3);
        let part = dir.path().join("private.part");
        let dest = dir.path().join("proof.bin");
        std::fs::write(&part, b"abc").unwrap();
        std::fs::write(&dest, b"untouched").unwrap();
        assert!(publish_file(&part, &dest).await.is_err());
        assert_eq!(std::fs::read(&dest).unwrap(), b"untouched");
        let dest = dir.path().join("recovered.bin");
        let j = store
            .begin_download(&lid, &part.to_string_lossy(), &dest.to_string_lossy())
            .unwrap();
        let hash = digest(&part, 3).await.unwrap();
        store
            .download_publishing(&lid, j.attempt, &hash, &dest.to_string_lossy())
            .unwrap();
        publish_file(&part, &dest).await.unwrap();
        drop(store);
        let store = Arc::new(Store::open(&db).unwrap());
        let e = fixture(store.clone(), dir.path(), url);
        assert_eq!(e.download("message", &lid).await.unwrap(), dest);
        assert_eq!(store.download_job(&lid).unwrap().unwrap().state, "done");
        assert_eq!(
            store.message("message").unwrap().unwrap().attachments[0]
                .local_path
                .as_deref(),
            dest.to_str()
        );
    }
    #[tokio::test]
    async fn queued_downloads_reopen_share_slots_and_leave_control_mail_responsive() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("store.db");
        let store = Arc::new(Store::open(&db).unwrap());
        let mut listeners = Vec::new();
        let mut urls = Vec::new();
        for _ in 0..2 {
            let l = TcpListener::bind("127.0.0.1:0").await.unwrap();
            urls.push(format!("http://{}", l.local_addr().unwrap()));
            listeners.push(l);
        }
        let e = fixture(store.clone(), dir.path(), &urls[0]);
        let mut jobs = Vec::new();
        for (i, hub) in [0, 0, 1].into_iter().enumerate() {
            let mid = format!("file{i}");
            store
                .queue_outgoing(
                    &NewOutgoing {
                        id: mid.clone(),
                        peer: format!("peer{i}"),
                        body: "file".into(),
                        kind: None,
                        reply_to: None,
                        attachments: vec![NewAttachment {
                            name: format!("file{i}.bin"),
                            bytes: 3,
                            source: "unused".into(),
                        }],
                    },
                    &now(),
                )
                .unwrap();
            let lid = store.message(&mid).unwrap().unwrap().attachments[0]
                .local_id
                .clone();
            store
                .set_upload(&lid, "uploaded", &format!("remote{i}"), &urls[hub])
                .unwrap();
            store.set_state(&mid, "sent", None).unwrap();
            assert_eq!(e.queue_download(&mid, &lid).unwrap().state, "queued");
            jobs.push(lid);
        }
        drop(e);
        drop(store);
        let store = Arc::new(Store::open(&db).unwrap());
        let e = fixture(store.clone(), dir.path(), &urls[0]);
        let other = fixture(store.clone(), dir.path(), &urls[1]);
        let rt = other.hubs.lock().unwrap().remove(&urls[1]).unwrap();
        e.hubs.lock().unwrap().insert(urls[1].clone(), rt);
        let stop = CancelFlag::default();
        let (release, _) = tokio::sync::watch::channel(false);
        let (entered, mut arrivals) = tokio::sync::mpsc::unbounded_channel();
        let peak = Arc::new(AtomicUsize::new(0));
        let current = Arc::new(AtomicUsize::new(0));
        let mut servers = Vec::new();
        for (hub, l) in listeners.into_iter().enumerate() {
            let stop = stop.clone();
            let release = release.clone();
            let entered = entered.clone();
            let peak = peak.clone();
            let current = current.clone();
            servers.push(tokio::spawn(async move {
                let local=Arc::new(AtomicUsize::new(0));let mut requests=tokio::task::JoinSet::new();
                while let Ok(accepted)=stop.run(async {Ok(l.accept().await?)}).await {
                    let (mut sock,_)=accepted;let mut release=release.subscribe();let entered=entered.clone();let local=local.clone();let current=current.clone();let peak=peak.clone();
                    requests.spawn(async move {
                        let h=headers(&mut sock).await;
                        if h.starts_with("GET ") {
                            assert_eq!(local.fetch_add(1,Ordering::SeqCst),0,"one file per hub");
                            let n=current.fetch_add(1,Ordering::SeqCst)+1;peak.fetch_max(n,Ordering::SeqCst);
                            entered.send(hub).unwrap();
                            while !*release.borrow_and_update() {release.changed().await.unwrap();}
                            sock.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 3\r\nConnection: close\r\n\r\nabc").await.unwrap();
                            current.fetch_sub(1,Ordering::SeqCst);local.fetch_sub(1,Ordering::SeqCst);
                        } else {
                            assert!(h.starts_with("POST /api/send "),"{h}");
                            let len=h.lines().find_map(|line|line.to_lowercase().strip_prefix("content-length: ").and_then(|n|n.parse::<usize>().ok())).unwrap();
                            sock.read_exact(&mut vec![0;len]).await.unwrap();
                            let body=r#"{"id":"control","received_at":"2026-10-11T00:00:00Z"}"#;
                            sock.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).as_bytes()).await.unwrap();
                        }
                    });
                }
                while let Some(r)=requests.join_next().await {r.unwrap();}
            }));
        }
        let runner = tokio::spawn(e.clone().sender_loop());
        let a = tokio::time::timeout(Duration::from_secs(5), arrivals.recv())
            .await
            .unwrap()
            .unwrap();
        let b = tokio::time::timeout(Duration::from_secs(5), arrivals.recv())
            .await
            .unwrap()
            .unwrap();
        assert_ne!(a, b);
        assert_eq!(peak.load(Ordering::SeqCst), 2);
        store
            .queue_outgoing(
                &NewOutgoing {
                    id: "control".into(),
                    peer: "someone-else".into(),
                    body: "ordinary mail".into(),
                    kind: None,
                    reply_to: None,
                    attachments: vec![],
                },
                &now(),
            )
            .unwrap();
        e.queue_changed.notify_one();
        until(|| store.message("control").unwrap().unwrap().state == "sent").await;
        assert!(jobs
            .iter()
            .all(|id| store.download_job(id).unwrap().unwrap().state != "done"));
        release.send(true).unwrap();
        until(|| {
            jobs.iter()
                .all(|id| store.download_job(id).unwrap().unwrap().state == "done")
        })
        .await;
        for lid in &jobs {
            let j = store.download_job(lid).unwrap().unwrap();
            assert_eq!(std::fs::read(j.destination).unwrap(), b"abc");
        }
        assert_eq!(peak.load(Ordering::SeqCst), 2);
        e.shutdown();
        tokio::time::timeout(Duration::from_secs(5), runner)
            .await
            .unwrap()
            .unwrap();
        stop.cancel();
        for s in servers {
            s.await.unwrap();
        }
    }
    #[tokio::test]
    async fn queued_export_recovers_lost_platform_reply_without_network_or_duplicate() {
        struct ExportHost(PathBuf);
        impl Host for ExportHost {
            fn event(&self, _: Event) {}
            fn download_export_committed(&self, _: &str) -> std::io::Result<()> {
                std::fs::remove_file(self.0.join("receipt"))
            }
            fn download_dir(&self) -> PathBuf {
                self.0.clone()
            }
            fn open_source(&self, source: &str) -> std::io::Result<std::fs::File> {
                if source == "content://proof/export" {
                    std::fs::File::open(self.0.join("shared.bin"))
                } else {
                    std::fs::File::open(source)
                }
            }
            fn publish_download(
                &self,
                key: &str,
                path: &std::path::Path,
                _: &str,
            ) -> std::io::Result<Option<String>> {
                let receipt = self.0.join("receipt");
                if receipt.exists() {
                    assert_eq!(std::fs::read_to_string(receipt)?, key);
                    return Ok(Some("content://proof/export".into()));
                }
                std::fs::copy(path, self.0.join("shared.bin"))?;
                std::fs::write(receipt, key)?;
                Err(std::io::Error::other("injected lost platform reply"))
            }
        }
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("store.db");
        let url = "http://127.0.0.1:9";
        let private = dir.path().join("private.bin");
        std::fs::write(&private, b"abc").unwrap();
        let store = Arc::new(Store::open(&db).unwrap());
        let lid = attach(&store, url, 3);
        store.set_state("message", "sent", None).unwrap();
        let j = store
            .begin_download(&lid, "part", &private.to_string_lossy())
            .unwrap();
        let hash = digest(&private, 3).await.unwrap();
        store
            .download_publishing(&lid, j.attempt, &hash, &private.to_string_lossy())
            .unwrap();
        store
            .download_done(&lid, j.attempt, &private.to_string_lossy())
            .unwrap();
        let e = fixture_host(store.clone(), url, Arc::new(ExportHost(dir.path().into())));
        assert!(e
            .download("message", &lid)
            .await
            .unwrap_err()
            .to_string()
            .contains("lost platform"));
        assert!(
            private.exists(),
            "private bytes remain until pointer committed"
        );
        let j = store.download_job(&lid).unwrap().unwrap();
        assert_eq!(j.state, "exporting");
        assert!(j.error.is_some());
        assert!(
            store.queued_downloads("", u64::MAX / 2).unwrap().is_empty(),
            "permanent export failure waits for Retry"
        );
        drop(e);
        drop(store);
        std::fs::remove_file(&private).unwrap();
        // The user changed the completed public file after the lost response.
        // Recovery must preserve it, not copy private bytes over it.
        std::fs::write(dir.path().join("shared.bin"), b"user-edited").unwrap();
        let store = Arc::new(Store::open(&db).unwrap());
        let e = fixture_host(store.clone(), url, Arc::new(ExportHost(dir.path().into())));
        e.queue_download("message", &lid).unwrap();
        let runner = tokio::spawn(e.clone().sender_loop());
        until(|| store.download_job(&lid).unwrap().unwrap().state == "done").await;
        until(|| !e.transfers.lock().unwrap().contains_key(&lid)).await;
        let a = &store.message("message").unwrap().unwrap().attachments[0];
        assert_eq!(a.local_path.as_deref(), Some("content://proof/export"));
        assert_eq!(
            std::fs::read(dir.path().join("shared.bin")).unwrap(),
            b"user-edited"
        );
        assert!(!private.exists());
        assert!(!dir.path().join("receipt").exists());
        until(|| !e.transfers.lock().unwrap().contains_key(&lid)).await;
        let attempt = store.download_job(&lid).unwrap().unwrap().attempt;
        assert_eq!(
            e.download("message", &lid).await.unwrap().to_string_lossy(),
            "content://proof/export"
        );
        assert_eq!(store.download_job(&lid).unwrap().unwrap().attempt, attempt);
        e.shutdown();
        runner.await.unwrap();
    }

    #[tokio::test]
    async fn exported_location_survives_reopen_and_repeated_save_without_network() {
        struct UriHost {
            dir: PathBuf,
            file: PathBuf,
        }
        impl Host for UriHost {
            fn event(&self, _: Event) {}
            fn open_source(&self, source: &str) -> std::io::Result<std::fs::File> {
                if source == "content://downloads/proof" {
                    std::fs::File::open(&self.file)
                } else {
                    std::fs::File::open(source)
                }
            }
            fn download_dir(&self) -> PathBuf {
                self.dir.clone()
            }
        }
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("store.db");
        let private = dir.path().join("private.bin");
        let public = dir.path().join("public.bin");
        let url = "http://127.0.0.1:9";
        let store = Arc::new(Store::open(&db).unwrap());
        let lid = attach(&store, url, 3);
        std::fs::write(&private, b"abc").unwrap();
        let j = store
            .begin_download(&lid, "unused.part", &private.to_string_lossy())
            .unwrap();
        store
            .download_publishing(&lid, j.attempt, "digest", &private.to_string_lossy())
            .unwrap();
        store
            .download_done(&lid, j.attempt, &private.to_string_lossy())
            .unwrap();
        let e = fixture_host(
            store.clone(),
            url,
            Arc::new(UriHost {
                dir: dir.path().into(),
                file: public.clone(),
            }),
        );
        std::fs::rename(&private, &public).unwrap();
        e.record_download_export(
            &lid,
            &private.to_string_lossy(),
            "content://downloads/proof",
        )
        .unwrap();
        assert!(e
            .record_download_export(&lid, "stale.bin", "wrong")
            .is_err());
        drop(e);
        drop(store);
        let store = Arc::new(Store::open(&db).unwrap());
        let e = fixture_host(
            store.clone(),
            url,
            Arc::new(UriHost {
                dir: dir.path().into(),
                file: public,
            }),
        );
        for _ in 0..2 {
            assert_eq!(
                e.download("message", &lid).await.unwrap(),
                PathBuf::from("content://downloads/proof")
            );
        }
        assert_eq!(e.queue_download("message", &lid).unwrap().state, "done");
        let job = store.download_job(&lid).unwrap().unwrap();
        assert!(job.exported);
        assert_eq!(job.state, "done");
        assert_eq!(job.attempt, 1);
        assert_eq!(
            store.message("message").unwrap().unwrap().attachments[0]
                .local_path
                .as_deref(),
            Some("content://downloads/proof")
        );
        std::fs::remove_file(dir.path().join("public.bin")).unwrap();
        let missing = e.queue_download("message", &lid).unwrap();
        assert_eq!(missing.state, "queued");
        assert_eq!(missing.checkpoint.confirmed, 0);
        assert!(store.message("message").unwrap().unwrap().attachments[0].local_path.is_none());
    }
    #[tokio::test]
    async fn damaged_publication_restarts_from_zero_and_preserves_external_files() {
        for bytes in [None, Some(&b"xyz"[..])] {
            let dir = tempfile::tempdir().unwrap();
            let store = Arc::new(Store::open_in_memory().unwrap());
            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let url = format!("http://{}", listener.local_addr().unwrap());
            let lid = attach(&store, &url, 3);
            let part = dir.path().join("private.part");
            let dest = dir.path().join("proof.bin");
            std::fs::write(&part, b"abc").unwrap();
            std::fs::write(&dest, b"keep me").unwrap();
            let j = store
                .begin_download(&lid, &part.to_string_lossy(), &dest.to_string_lossy())
                .unwrap();
            let hash = digest(&part, 3).await.unwrap();
            store
                .download_checkpoint(
                    &lid,
                    j.attempt,
                    &DownloadCheckpoint {
                        confirmed: 3,
                        validator: Some("old".into()),
                        updated_ms: 1,
                    },
                )
                .unwrap();
            store
                .download_publishing(&lid, j.attempt, &hash, &dest.to_string_lossy())
                .unwrap();
            if let Some(bytes) = bytes {
                std::fs::write(&part, bytes).unwrap();
            } else {
                std::fs::remove_file(&part).unwrap();
            }
            let e = fixture(store.clone(), dir.path(), &url);
            assert!(e
                .download("message", &lid)
                .await
                .unwrap_err()
                .to_string()
                .contains("retry from zero"));
            let j = store.download_job(&lid).unwrap().unwrap();
            assert_eq!(j.state, "failed");
            assert_eq!(j.checkpoint.confirmed, 0);
            assert!(j.digest.is_none());
            let server = tokio::spawn(async move {
                let (mut s, _) = listener.accept().await.unwrap();
                assert!(!headers(&mut s).await.to_lowercase().contains("range:"));
                s.write_all(
                    b"HTTP/1.1 200 OK\r\nContent-Length: 3\r\nConnection: close\r\n\r\nabc",
                )
                .await
                .unwrap();
            });
            let path = e.download("message", &lid).await.unwrap();
            assert_eq!(std::fs::read(path).unwrap(), b"abc");
            assert_eq!(std::fs::read(&dest).unwrap(), b"keep me");
            server.await.unwrap();
        }
    }
    #[tokio::test]
    async fn orphan_scan_pages_old_files_and_preserves_referenced_recent_and_unrelated_bytes() {
        let dir=tempfile::tempdir().unwrap();let root=dir.path().join(".hubchat-downloads");
        std::fs::create_dir(&root).unwrap();
        let old=std::time::SystemTime::now()-Duration::from_secs(90000);
        let create=|path:&std::path::Path,age:bool| {
            std::fs::write(path,b"keep or collect").unwrap();
            if age {std::fs::File::options().write(true).open(path).unwrap().set_times(std::fs::FileTimes::new().set_modified(old)).unwrap();}
        };
        let mut abandoned=Vec::new();
        for _ in 0..70 {
            let path=root.join(format!("{}.part",uuid::Uuid::new_v4().simple()));
            create(&path,true);abandoned.push(path);
        }
        let unrelated=root.join("notes.txt");create(&unrelated,true);
        let recent=root.join(format!("{}.part",uuid::Uuid::new_v4().simple()));create(&recent,false);
        let referenced=root.join(format!("{}.part",uuid::Uuid::new_v4().simple()));create(&referenced,true);
        let db=dir.path().join("db");let store=Arc::new(Store::open(&db).unwrap());
        let lid=attach(&store,"http://127.0.0.1:9",3);
        store.begin_download(&lid,&referenced.to_string_lossy(),"unused").unwrap();
        let e=fixture(store.clone(),dir.path(),"http://127.0.0.1:9");
        e.clean_downloads(0).await;
        assert!(abandoned.iter().filter(|p|p.exists()).count()>=38,"only32 names scanned per pass");
        drop(e);drop(store);
        let store=Arc::new(Store::open(&db).unwrap());
        let e=fixture(store.clone(),dir.path(),"http://127.0.0.1:9");
        for _ in 0..3 {e.clean_downloads(0).await;}
        assert!(abandoned.iter().all(|p|!p.exists()));
        for kept in [unrelated,recent,referenced] {assert_eq!(std::fs::read(kept).unwrap(),b"keep or collect");}
    }

    #[tokio::test]
    async fn partial_cleanup_is_bounded_skips_writers_and_survives_deleted_messages() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join(".hubchat-downloads");
        std::fs::create_dir(&root).unwrap();
        let db = dir.path().join("store.db");
        let store = Arc::new(Store::open(&db).unwrap());
        store.queue_outgoing(&NewOutgoing {
            id: "message".into(), peer: "peer".into(), body: "files".into(), kind: None, reply_to: None,
            attachments: (0..36).map(|i|NewAttachment{name:format!("{i}.bin"),bytes:3,source:"unused".into()}).collect(),
        }, &now()).unwrap();
        let attachments = store.message("message").unwrap().unwrap().attachments;
        let mut paths = Vec::new();
        for a in &attachments {
            let part = root.join(format!("{}.part", uuid::Uuid::new_v4().simple()));
            std::fs::write(&part,b"abc").unwrap();
            let j = store.begin_download(&a.local_id,&part.to_string_lossy(),"published-do-not-delete").unwrap();
            assert!(j.checkpoint.updated_ms>0,"new jobs get a real age before any checkpoint");
            store.download_checkpoint(&a.local_id,j.attempt,&DownloadCheckpoint {
                confirmed:3,validator:Some("etag".into()),updated_ms:1,
            }).unwrap();
            store.cancel_download(&a.local_id).unwrap();
            paths.push(part);
        }
        let e=fixture(store.clone(),dir.path(),"http://127.0.0.1:9");
        let active=&attachments[0].local_id;
        e.transfers.lock().unwrap().insert(active.clone(),CancelFlag::default());
        let at=86400002;
        e.clean_downloads(at).await;
        assert!(paths[0].exists(),"expiry never removes an active writer");
        assert!(paths.iter().filter(|p|p.exists()).count()>=4,"one pass is bounded");
        e.clean_downloads(at+30001).await;
        assert_eq!(paths.iter().filter(|p|p.exists()).count(),1);
        let expired=store.download_job(&attachments[1].local_id).unwrap().unwrap();
        assert_eq!(expired.checkpoint.confirmed,0);
        assert_eq!(expired.state,"cancelled","cleanup does not revive user cancellation");
        assert!(expired.error.unwrap().contains("expired"));
        store.queue_download_job(&attachments[1].local_id,"unused","unused").unwrap();
        let restarted=store.begin_download(&attachments[1].local_id,"unused","unused").unwrap();
        assert!(store.message("message").unwrap().unwrap().attachments.iter().find(|a|a.local_id==attachments[1].local_id).unwrap().download.as_ref().unwrap().restarted,"restart explanation survives admission and begin");
        store.download_checkpoint(&attachments[1].local_id,restarted.attempt,&DownloadCheckpoint {confirmed:1,validator:Some("new".into()),updated_ms:at+30001}).unwrap();
        assert!(!store.message("message").unwrap().unwrap().attachments.iter().find(|a|a.local_id==attachments[1].local_id).unwrap().download.as_ref().unwrap().restarted);

        // A cascading message delete records cleanup even while its writer stops.
        store.delete_message("message").unwrap();
        e.clean_downloads(at+60002).await;
        assert!(paths[0].exists());
        drop(e);drop(store);
        let store=Arc::new(Store::open(&db).unwrap());
        let e=fixture(store.clone(),dir.path(),"http://127.0.0.1:9");
        e.clean_downloads(at+90003).await;
        e.clean_downloads(at+120004).await;
        assert!(!paths[0].exists(),"durable removal resumes after reopen");
        assert!(!root.exists(),"empty private directory is removed");
        // Unexpected paths are never deleted, including real completed files.
        let visible=dir.path().join("keep.bin");std::fs::write(&visible,b"keep").unwrap();
        rusqlite::Connection::open(&db).unwrap().execute("INSERT INTO download_gc(path,local_id) VALUES(?,'other')",[visible.to_string_lossy().as_ref()]).unwrap();
        e.clean_downloads(at+150005).await;
        assert_eq!(std::fs::read(visible).unwrap(),b"keep");
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn deleted_partial_unlink_failure_is_retried_after_reopen() {
        use std::os::windows::fs::OpenOptionsExt;
        let dir=tempfile::tempdir().unwrap();let root=dir.path().join(".hubchat-downloads");
        std::fs::create_dir(&root).unwrap();
        let db=dir.path().join("store.db");let store=Arc::new(Store::open(&db).unwrap());
        let lid=attach(&store,"http://127.0.0.1:9",3);
        let part=root.join(format!("{}.part",uuid::Uuid::new_v4().simple()));
        std::fs::write(&part,b"abc").unwrap();
        store.begin_download(&lid,&part.to_string_lossy(),"never-delete").unwrap();
        store.delete_message("message").unwrap();
        let held=std::fs::OpenOptions::new().read(true).share_mode(0).open(&part).unwrap();
        let e=fixture(store.clone(),dir.path(),"http://127.0.0.1:9");
        e.clean_downloads(100).await;assert!(part.exists());
        assert!(store.download_gc_due(100).unwrap().is_empty());
        drop(e);drop(store);drop(held);
        let store=Arc::new(Store::open(&db).unwrap());
        let e=fixture(store.clone(),dir.path(),"http://127.0.0.1:9");
        e.clean_downloads(30100).await;assert!(!part.exists());
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn live_publication_skips_extra_read_and_locked_recovery_backs_off() {
        use std::os::windows::fs::OpenOptionsExt;
        for fresh in [true, false] {
            let dir = tempfile::tempdir().unwrap();
            let store = Arc::new(Store::open_in_memory().unwrap());
            let url = "http://127.0.0.1:9";
            let lid = attach(&store, url, 3);
            let part = dir.path().join("private.part");
            let dest = dir.path().join("proof.bin");
            std::fs::write(&part, b"abc").unwrap();
            let e = fixture(store.clone(), dir.path(), url);
            let j = store.begin_download(&lid, &part.to_string_lossy(), &dest.to_string_lossy()).unwrap();
            e.finish_download(&j, 3, dir.path(), "proof.bin").await.unwrap();
            let j = store.download_job(&lid).unwrap().unwrap();
            // Simulate an indexer opening the published file without read sharing.
            let held = std::fs::OpenOptions::new().read(true).share_mode(0).open(&j.destination).unwrap();
            if fresh {
                assert!(e.export_download(&j, "proof.bin", 3, true).await.is_ok());
                assert_eq!(store.download_job(&lid).unwrap().unwrap().state, "done");
            } else {
                assert!(matches!(e.download("message", &lid).await, Err(Error::LocalBusy)));
                assert!(store.pending_transfer_failures(0).unwrap().is_empty());
                store.defer_download(&lid, 1000).unwrap();
                assert!(store.queued_downloads("", 1000).unwrap().is_empty());
                let resumed = store.download_job(&lid).unwrap().unwrap();
                assert_eq!(resumed.state, "exporting");
                assert_eq!(resumed.digest, j.digest);
                assert_eq!(store.queued_downloads("", 300000).unwrap().len(), 1);
            }
            drop(held);
            if !fresh {
                assert_eq!(e.download("message", &lid).await.unwrap(), PathBuf::from(&j.destination));
            }
            assert_eq!(std::fs::read(&j.destination).unwrap(), b"abc");
        }
    }

    #[tokio::test]
    async fn damaged_export_retries_from_zero_and_does_not_publish_missing_bytes() {
        for bytes in [None, Some(&b"bad"[..])] {
            let dir = tempfile::tempdir().unwrap();
            let db = dir.path().join("store.db");
            let store = Arc::new(Store::open(&db).unwrap());
            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let url = format!("http://{}", listener.local_addr().unwrap());
            let lid = attach(&store, &url, 3);
            let private = dir.path().join("proof.bin");
            let part = dir.path().join("private.part");
            std::fs::write(&private, b"abc").unwrap();
            let j = store.begin_download(&lid, &part.to_string_lossy(), &private.to_string_lossy()).unwrap();
            let hash = digest(&private, 3).await.unwrap();
            store.download_checkpoint(&lid, j.attempt, &DownloadCheckpoint {
                confirmed: 3, validator: Some("old".into()), updated_ms: 1,
            }).unwrap();
            store.download_publishing(&lid, j.attempt, &hash, &private.to_string_lossy()).unwrap();
            store.download_done(&lid, j.attempt, &private.to_string_lossy()).unwrap();
            if let Some(bytes) = bytes { std::fs::write(&private, bytes).unwrap(); }
            else { std::fs::remove_file(&private).unwrap(); }
            drop(store);
            let store = Arc::new(Store::open(&db).unwrap());
            let e = fixture(store.clone(), dir.path(), &url);
            assert!(e.download("message", &lid).await.unwrap_err().to_string().contains("retry from zero"));
            let j = store.download_job(&lid).unwrap().unwrap();
            assert_eq!(j.state, "failed");
            assert_eq!(j.checkpoint.confirmed, 0);
            assert!(j.digest.is_none());
            assert_eq!(store.pending_transfer_failures(0).unwrap().len(), 1);
            let server = tokio::spawn(async move {
                let (mut socket, _) = listener.accept().await.unwrap();
                assert!(!headers(&mut socket).await.to_lowercase().contains("range:"));
                socket.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 3\r\nConnection: close\r\n\r\nabc").await.unwrap();
            });
            e.queue_download("message", &lid).unwrap();
            let saved = e.download("message", &lid).await.unwrap();
            assert_eq!(std::fs::read(saved).unwrap(), b"abc");
            if let Some(bytes) = bytes { assert_eq!(std::fs::read(&private).unwrap(), bytes); }
            server.await.unwrap();
        }
    }

    #[tokio::test]
    async fn dropped_download_future_releases_writer_and_keeps_job_paused() {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(Store::open_in_memory().unwrap());
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let lid = attach(&store, &url, 3);
        let (entered, wait) = tokio::sync::oneshot::channel();
        let server = tokio::spawn(async move {
            let (mut s, _) = listener.accept().await.unwrap();
            headers(&mut s).await;
            entered.send(()).unwrap();
            let mut b = [0];
            assert_eq!(s.read(&mut b).await.unwrap(), 0);
        });
        let e = fixture(store.clone(), dir.path(), &url);
        let e2 = e.clone();
        let lid2 = lid.clone();
        let task = tokio::spawn(async move { e2.download("message", &lid2).await });
        wait.await.unwrap();
        assert!(e
            .download("message", &lid)
            .await
            .unwrap_err()
            .to_string()
            .contains("already has"));
        task.abort();
        let _ = task.await;
        server.await.unwrap();
        assert!(!e.transfers.lock().unwrap().contains_key(&lid));
        assert_eq!(store.download_job(&lid).unwrap().unwrap().state, "paused");
    }
}
