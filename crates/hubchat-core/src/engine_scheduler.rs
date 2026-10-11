//! One bounded owner for queued sends and explicit download requests.
use super::*;
use std::collections::HashMap;
pub(super) fn transient(e: &Error) -> bool {
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
        let mut active = HashMap::<tokio::task::Id, String>::new();
        let mut clean_at = std::time::Instant::now();
        let mut send_cursor: Option<(String, String)> = None;
        let mut download_cursor = String::new();
        let mut suspended = self.suspended.subscribe();
        let mut downloads_first = true;
        loop {
            while let Some(result) = workers.try_join_next_with_id() {
                self.reap_worker(&mut active, result);
            }
            if self.stopped.load(std::sync::atomic::Ordering::Relaxed) {
                // shutdown cancels sockets. Do not abort a filesystem publication.
                while let Some(result) = workers.join_next_with_id().await {
                    self.reap_worker(&mut active, result);
                }
                return;
            }
            if std::time::Instant::now() >= clean_at {
                self.clean_staging().await;
                clean_at = std::time::Instant::now() + Duration::from_secs(30);
            }
            self.deliver_transfer_failures(unix_ms()).await;
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
                            if active.values().any(|value| value == &key) {
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
                            let e = self.clone();
                            let id = m.id.clone();
                            let hub = route.0.clone();
                            let task = workers.spawn(async move {
                                let _permit = permit;
                                match e.send_one_on(&id, route).await {
                                    Ok(false) => {
                                        let _ = e.store.defer_send(&id, &hub, unix_ms());
                                    }
                                    Err(ref err) if transient(err) => {
                                        let _ = e.store.defer_send(&id, &hub, unix_ms());
                                    }
                                    Err(err) => {
                                        let _ = e.fail(&m_peer(&e, &id), &id, &err.to_string());
                                    }
                                    _ => {}
                                }
                            });
                            active.insert(task.id(), key);
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
                result=workers.join_next_with_id(), if !workers.is_empty()=>{if let Some(result)=result{self.reap_worker(&mut active,result);}},
                _=self.queue_changed.notified()=>{},
                _=self.file_slots.changed.notified()=>{},
                _=suspended.changed()=>{},
                _=tokio::time::sleep(if more {Duration::from_millis(1)} else {Duration::from_secs(2)})=>{},
            }
        }
    }
    /// Bounded outbox delivery also runs while connections are suspended.
    async fn deliver_transfer_failures(&self, now:u64) {
        let Ok(notices)=self.store.pending_transfer_failures(now) else{return};
        for notice in notices {
            if !self.store.start_failure_notice(&notice,now).unwrap_or(false) {continue;}
            let host=self.host.clone();let copy=notice.clone();
            if matches!(tokio::task::spawn_blocking(move||host.transfer_failed(&copy)).await,Ok(Ok(()))) {
                let _=self.store.finish_failure_notice(&notice);
            }
        }
    }
    fn reap_worker(
        &self,
        active: &mut HashMap<tokio::task::Id, String>,
        result: std::result::Result<(tokio::task::Id, ()), tokio::task::JoinError>,
    ) {
        let id = match &result {
            Ok((id, _)) => *id,
            Err(error) => error.id(),
        };
        let Some(key) = active.remove(&id) else {
            return;
        };
        if result.is_ok() {
            return;
        }
        if let Some(mid) = key.strip_prefix("send:") {
            if let Ok(Some(m)) = self.store.message(mid) {
                let mut transfers = self.transfers.lock().unwrap();
                for a in &m.attachments {
                    if a.state == "uploading" {
                        transfers.remove(&a.local_id);
                    }
                }
            }
            let _ = self.store.send_worker_failed(mid);
            self.host.event(Event::Chat {
                peer: m_peer(self, mid),
            });
        } else if let Some(lid) = key.strip_prefix("download:") {
            self.transfers.lock().unwrap().remove(lid);
            let _ = self.store.download_worker_failed(lid);
        }
    }
    fn launch_downloads(
        self: &Arc<Self>,
        workers: &mut tokio::task::JoinSet<()>,
        active: &mut HashMap<tokio::task::Id, String>,
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
                    if active.values().any(|value| value == &key)
                        || self.transfers.lock().unwrap().contains_key(lid)
                    {
                        continue;
                    }
                    if !self.hubs.lock().unwrap().contains_key(hub) {
                        continue;
                    }
                    let Some(permit) = self.file_slots.try_acquire(hub) else {
                        continue;
                    };
                    let e = self.clone();
                    let lid = lid.clone();
                    let mid = mid.clone();
                    let task = workers.spawn(async move {
                        if let Err(err) = e.download_with_slot(&mid, &lid, Some(permit)).await {
                            if transient(&err) {
                                let _ = e.store.defer_download(&lid, unix_ms());
                            }
                        }
                    });
                    active.insert(task.id(), key);
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

#[cfg(test)]
mod tests {
    use super::*;
    struct HostStub;
    impl Host for HostStub {
        fn event(&self, _: Event) {}
        fn open_source(&self, path: &str) -> std::io::Result<std::fs::File> {
            std::fs::File::open(path)
        }
        fn download_dir(&self) -> PathBuf {
            std::env::temp_dir()
        }
    }
    #[tokio::test]
    async fn panicked_workers_release_keys_and_slots_and_record_actionable_failure() {
        let store = Arc::new(Store::open_in_memory().unwrap());
        store
            .queue_outgoing(
                &NewOutgoing {
                    id: "message".into(),
                    peer: "peer".into(),
                    body: "text".into(),
                    kind: None,
                    reply_to: None,
                    attachments: vec![crate::store::NewAttachment {
                        name: "file".into(),
                        bytes: 3,
                        source: "unused".into(),
                    }],
                },
                "now",
            )
            .unwrap();
        let lid = store.message("message").unwrap().unwrap().attachments[0]
            .local_id
            .clone();
        let e = Engine::new(
            store.clone(),
            Identity::generate("panic-proof").unwrap(),
            Profile::default(),
            Arc::new(HostStub),
        );
        let mut active = HashMap::new();
        let mut workers = tokio::task::JoinSet::new();
        store
            .set_attachment(&lid, "uploading", None, None, None)
            .unwrap();
        e.transfers
            .lock()
            .unwrap()
            .insert(lid.clone(), CancelFlag::default());
        let permit = e.file_slots.try_acquire("hub").unwrap();
        let task = workers.spawn(async move {
            let _permit = permit;
            panic!("injected worker failure");
        });
        active.insert(task.id(), "send:message".into());
        e.reap_worker(&mut active, workers.join_next_with_id().await.unwrap());
        assert!(active.is_empty());
        assert!(e.file_slots.try_acquire("hub").is_some());
        assert!(e.transfers.lock().unwrap().is_empty());
        assert_eq!(store.message("message").unwrap().unwrap().state, "failed");
        let attachment = &store.message("message").unwrap().unwrap().attachments[0];
        assert_eq!(attachment.state, "failed");
        assert!(attachment.error.as_ref().unwrap().contains("unexpectedly"));
        assert!(store
            .message("message")
            .unwrap()
            .unwrap()
            .error
            .unwrap()
            .contains("unexpectedly"));
        store.begin_download(&lid, "part", "dest").unwrap();
        let task = workers.spawn(async {
            panic!("injected download failure");
        });
        active.insert(task.id(), format!("download:{lid}"));
        e.reap_worker(&mut active, workers.join_next_with_id().await.unwrap());
        assert!(active.is_empty());
        assert_eq!(store.download_job(&lid).unwrap().unwrap().state, "failed");
        assert!(store
            .download_job(&lid)
            .unwrap()
            .unwrap()
            .error
            .unwrap()
            .contains("unexpectedly"));
    }
    #[tokio::test]
    async fn failure_delivery_replays_same_key_after_lost_host_reply_and_never_holds_store() {
        struct NoticeHost { store:Arc<Store>, keys:Mutex<std::collections::HashSet<String>>, fail:std::sync::atomic::AtomicBool }
        impl Host for NoticeHost {
            fn event(&self,_:Event) {}
            fn open_source(&self,_:&str)->std::io::Result<std::fs::File>{unreachable!()}
            fn download_dir(&self)->std::path::PathBuf{std::path::PathBuf::new()}
            fn transfer_failed(&self,n:&crate::store::TransferFailure)->std::io::Result<()> {
                self.store.set_meta("notice-proof","host can use store").unwrap();
                self.keys.lock().unwrap().insert(n.key());
                if self.fail.swap(false,std::sync::atomic::Ordering::Relaxed){Err(std::io::Error::other("lost OS reply"))}else{Ok(())}
            }
        }
        let store=Arc::new(Store::open_in_memory().unwrap());
        store.queue_outgoing(&crate::store::NewOutgoing{id:"notice".into(),peer:"peer".into(),body:"body".into(),kind:None,reply_to:None,attachments:vec![crate::store::NewAttachment{name:"file".into(),bytes:3,source:"missing".into()}]},"now").unwrap();
        store.fail_send("notice","failed").unwrap();
        let host=Arc::new(NoticeHost{store:store.clone(),keys:Mutex::new(Default::default()),fail:std::sync::atomic::AtomicBool::new(true)});
        let e=Engine::new(store.clone(),Identity::generate("failure-proof").unwrap(),Profile::default(),host.clone());
        tokio::time::timeout(Duration::from_secs(2),e.deliver_transfer_failures(0)).await.unwrap();
        assert_eq!(host.keys.lock().unwrap().len(),1);
        // A fresh engine stands in for restart; the host's stable-key collection
        // models replacement by the OS, including an accepted but lost reply.
        let e=Engine::new(store.clone(),Identity::generate("failure-proof").unwrap(),Profile::default(),host.clone());
        e.deliver_transfer_failures(30000).await;e.deliver_transfer_failures(60000).await;
        assert_eq!(host.keys.lock().unwrap().len(),1);
        assert!(store.pending_transfer_failures(u64::MAX/2).unwrap().is_empty());
    }

}
