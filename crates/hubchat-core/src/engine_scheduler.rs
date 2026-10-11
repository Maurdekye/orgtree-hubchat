//! One bounded owner for queued sends and explicit download requests.
use super::*;
use std::collections::HashSet;
fn transient(e: &Error) -> bool {
    matches!(e, Error::Unreachable(_))
        || e.status()
            .is_some_and(|s| matches!(s, 408 | 409 | 429 | 500..=599))
}
impl Engine {
    pub(super) async fn sender_loop(self: Arc<Self>) {
        let store = self.store.clone();
        let root = self.host.staging_dir();
        let _ =
            tokio::task::spawn_blocking(move || crate::staging::sweep_orphans(&root, &store)).await;
        let mut workers = tokio::task::JoinSet::new();
        let mut active = HashSet::<String>::new();
        let mut send_cursor: Option<(String, String)> = None;
        let mut download_cursor = String::new();
        let mut suspended = self.suspended.subscribe();
        let mut downloads_first = true;
        loop {
            while let Some(result) = workers.try_join_next() {
                if let Ok(key) = result {
                    active.remove(&key);
                }
            }
            if self.stopped.load(std::sync::atomic::Ordering::Relaxed) {
                // shutdown cancels sockets. Do not abort a filesystem publication.
                while workers.join_next().await.is_some() {}
                return;
            }
            self.clean_staging().await;
            let mut more = false;
            if !*suspended.borrow_and_update() {
                if downloads_first {
                    more |= self.launch_downloads(&mut workers, &mut active, &mut download_cursor);
                }
                // Leave one control lane beside the two file workers. Ordinary
                // mail to another peer can proceed on a hub busy transferring.
                if workers.len() < 3 {
                    if let Ok(rows) = self.store.send_heads(
                        send_cursor
                            .as_ref()
                            .map(|(t, id)| (t.as_str(), id.as_str())),
                        unix_ms(),
                    ) {
                        more |= rows.len() == 64;
                        for m in &rows {
                            if workers.len() >= 3 {
                                break;
                            }
                            send_cursor = Some((m.created_at.clone(), m.id.clone()));
                            let key = format!("send:{}", m.id);
                            if active.contains(&key) {
                                continue;
                            }
                            let route = match self.route(&m.peer, &self.connected_hubs()) {
                                Ok(r) => r.next,
                                Err(_) => None,
                            };
                            let Some(route) = route else {
                                continue;
                            };
                            let files = m.body.len() as u64 > BODY_INLINE_MAX
                                || m.attachments.iter().any(|a| {
                                    a.state != "uploaded" || a.hub.as_deref() != Some(&route.0)
                                });
                            let permit = if files {
                                let Some(p) = self.file_slots.try_acquire(&route.0) else {
                                    continue;
                                };
                                Some(p)
                            } else {
                                None
                            };
                            active.insert(key.clone());
                            let e = self.clone();
                            let id = m.id.clone();
                            workers.spawn(async move {
                                let _permit = permit;
                                match e.send_one_on(&id, route).await {
                                    Ok(false) => {
                                        let _ = e.store.defer_send(&id, unix_ms());
                                    }
                                    Err(ref err) if transient(err) => {
                                        let _ = e.store.defer_send(&id, unix_ms());
                                    }
                                    Err(err) => {
                                        let _ = e.fail(&m_peer(&e, &id), &id, &err.to_string());
                                    }
                                    _ => {}
                                }
                                key
                            });
                        }
                        if rows.len() < 64 {
                            send_cursor = None;
                        }
                    }
                }
                if !downloads_first {
                    more |= self.launch_downloads(&mut workers, &mut active, &mut download_cursor);
                }
                downloads_first = !downloads_first;
            }
            tokio::select! {
                result=workers.join_next(), if !workers.is_empty()=>{if let Some(Ok(key))=result{active.remove(&key);}},
                _=self.queue_changed.notified()=>{},
                _=self.file_slots.changed.notified()=>{},
                _=suspended.changed()=>{},
                _=tokio::time::sleep(if more {Duration::from_millis(1)} else {Duration::from_secs(2)})=>{},
            }
        }
    }
    fn launch_downloads(
        self: &Arc<Self>,
        workers: &mut tokio::task::JoinSet<String>,
        active: &mut HashSet<String>,
        download_cursor: &mut String,
    ) -> bool {
        let mut more = false;
        if workers.len() < 3 {
            if let Ok(rows) = self.store.queued_downloads(download_cursor, unix_ms()) {
                more |= rows.len() == 64;
                for (lid, mid, hub) in &rows {
                    if workers.len() >= 3 {
                        break;
                    }
                    *download_cursor = lid.clone();
                    let key = format!("download:{lid}");
                    if active.contains(&key) || self.transfers.lock().unwrap().contains_key(lid) {
                        continue;
                    }
                    if !self.hubs.lock().unwrap().contains_key(hub) {
                        continue;
                    }
                    let Some(permit) = self.file_slots.try_acquire(hub) else {
                        continue;
                    };
                    active.insert(key.clone());
                    let e = self.clone();
                    let lid = lid.clone();
                    let mid = mid.clone();
                    workers.spawn(async move {
                        if let Err(err) = e.download_with_slot(&mid, &lid, Some(permit)).await {
                            if transient(&err) {
                                let _ = e.store.defer_download(&lid, unix_ms());
                            }
                        }
                        key
                    });
                }
                if rows.len() < 64 {
                    download_cursor.clear();
                }
            }
        }
        more
    }
}
fn m_peer(e: &Engine, id: &str) -> String {
    e.store
        .message(id)
        .ok()
        .flatten()
        .map(|m| m.peer)
        .unwrap_or_default()
}
