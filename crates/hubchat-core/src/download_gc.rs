//! Bounded private-partial cleanup; never deletes a published destination.
use super::*;
use std::path::Path;
struct Claim<'a>(&'a Engine, String);
impl Drop for Claim<'_> {
    fn drop(&mut self) {
        self.0.transfers.lock().unwrap().remove(&self.1);
    }
}
fn claim<'a>(e: &'a Engine, id: &str) -> Option<Claim<'a>> {
    let mut writers = e.transfers.lock().unwrap();
    if writers.contains_key(id) {
        return None;
    }
    writers.insert(id.into(), CancelFlag::default());
    Some(Claim(e, id.into()))
}
fn private_partial(root: &Path, path: &Path) -> bool {
    path.parent() == Some(root)
        && path.extension().is_some_and(|s| s == "part")
        && path
            .file_stem()
            .and_then(|s| s.to_str())
            .is_some_and(|s| s.len() == 32 && s.bytes().all(|b| b.is_ascii_hexdigit()))
        && !root
            .symlink_metadata()
            .is_ok_and(|m| m.file_type().is_symlink())
        && !path
            .symlink_metadata()
            .is_ok_and(|m| m.file_type().is_symlink())
}
// Scan names in bounded keyset pages without retaining an unbounded directory
// listing. A persisted cursor also advances past recent and unrelated entries.
fn discover_orphans(root: &Path, store: &Store) -> Result<()> {
    let after = store.meta("download.gc.cursor")?.unwrap_or_default();
    let entries = match std::fs::read_dir(root) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e.into()),
    };
    let mut names = std::collections::BTreeSet::new();
    for entry in entries {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if name > after {
            names.insert(name);
            if names.len() > 32 {
                names.pop_last();
            }
        }
    }
    for name in &names {
        let path = root.join(name);
        if !private_partial(root, &path) {
            continue;
        }
        let Ok(meta) = path.symlink_metadata() else {
            continue;
        };
        if !meta.is_file() {
            continue;
        }
        if meta
            .modified()
            .ok()
            .and_then(|t| t.elapsed().ok())
            .is_some_and(|age| age.as_secs() >= 86400)
        {
            store.queue_orphan_partial(&path.to_string_lossy())?;
        }
    }
    store.set_meta(
        "download.gc.cursor",
        if names.len() == 32 {
            names.last().unwrap()
        } else {
            ""
        },
    )
}
impl Engine {
    pub(super) async fn clean_downloads(&self, now: u64) {
        let root = self.host.download_dir().join(".hubchat-downloads");
        let scan_root = root.clone();
        let store = self.store.clone();
        let _ = tokio::task::spawn_blocking(move || discover_orphans(&scan_root, &store)).await;
        if let Ok(rows) = self.store.stale_downloads(now) {
            for (id, old) in rows {
                // Advance even skipped writers so a busy head cannot starve later jobs.
                let _ = self.store.check_download_later(&id, now);
                let Some(_claim) = claim(self, &id) else {
                    continue;
                };
                if !private_partial(&root, Path::new(&old)) {
                    continue;
                }
                let new = root.join(format!("{}.part", uuid::Uuid::new_v4().simple()));
                if self
                    .store
                    .expire_download_partial(&id, &old, &new.to_string_lossy(), now)
                    .unwrap_or(false)
                {
                    if let Ok(Some(peer)) = self.store.transfer_peer(&id) {
                        self.host.event(Event::Chat { peer });
                    }
                }
            }
        }
        if let Ok(rows) = self.store.download_gc_due(now) {
            for (path, id) in rows {
                let Some(_claim) = claim(self, &id) else {
                    let _ = self.store.download_gc_retry(&path, now);
                    continue;
                };
                // Reject unexpected paths permanently, never operate outside the private directory.
                if !private_partial(&root, Path::new(&path)) {
                    let _ = self.store.download_gc_removed(&path);
                    continue;
                }
                if self
                    .store
                    .download_partial_referenced(&path)
                    .unwrap_or(true)
                {
                    let _ = self.store.download_gc_retry(&path, now);
                    continue;
                }
                let _publication = self.download_publication.lock().await;
                match tokio::fs::remove_file(&path).await {
                    Ok(()) => {
                        let _ = self.store.download_gc_removed(&path);
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                        let _ = self.store.download_gc_removed(&path);
                    }
                    Err(_) => {
                        let _ = self.store.download_gc_retry(&path, now);
                    }
                }
                let _ = tokio::fs::remove_dir(&root).await; // Empty only, never recursive.
            }
        }
    }
}
