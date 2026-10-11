//! Reading long messages has its own bounded lane, independent of files and
//! previews. Durable admission happens in the same transaction as sync/history.
use super::*;

impl Engine {
    pub(super) async fn body_loop(self: Arc<Self>) {
        let mut workers = tokio::task::JoinSet::new();
        let mut active = HashMap::<tokio::task::Id, String>::new();
        let mut cursor = String::new();
        let mut suspended = self.suspended.subscribe();
        loop {
            while let Some(result) = workers.try_join_next_with_id() {
                self.reap_body(&mut active, result);
            }
            if self.stopped.load(std::sync::atomic::Ordering::Relaxed) {
                while let Some(result) = workers.join_next_with_id().await {
                    self.reap_body(&mut active, result);
                }
                return;
            }
            let mut more = false;
            if !*suspended.borrow_and_update() && workers.len() < 2 {
                if let Ok(rows) = self.store.pending_bodies(&cursor, unix_ms()) {
                    more = rows.len() == 64;
                    for row in &rows {
                        if workers.len() >= 2 {
                            break;
                        }
                        cursor = row.id.clone();
                        if active.values().any(|id| id == &row.id) {
                            continue;
                        }
                        // A message can exist on several hubs; a disconnected
                        // first route must not hide a currently connected copy.
                        let Ok(routes) = self.store.message_hubs(&row.id) else {
                            continue;
                        };
                        let route = {
                            let hubs = self.hubs.lock().unwrap();
                            routes.iter().find_map(|url| {
                                hubs.get(url)
                                    .filter(|h| matches!(h.status.state, HubState::Connected))
                                    .map(|h| (h.client.clone(), h.stop.clone()))
                            })
                        };
                        let Some((client, stop)) = route else {
                            continue;
                        };
                        let e = self.clone();
                        let id = row.id.clone();
                        let peer = row.peer.clone();
                        let reply = row.reply;
                        let task = workers.spawn(async move {
                            e.fetch_body(&id, &peer, reply, client, stop).await;
                        });
                        active.insert(task.id(), row.id.clone());
                    }
                    if rows.len() < 64 {
                        cursor.clear();
                    }
                }
            }
            tokio::select! {
                result=workers.join_next_with_id(),if !workers.is_empty()=>{
                    if let Some(result)=result {self.reap_body(&mut active,result);}
                },
                _=self.body_changed.notified()=>{},
                _=suspended.changed()=>{},
                _=tokio::time::sleep(if more {Duration::from_millis(1)} else {Duration::from_secs(2)})=>{},
            }
        }
    }

    fn reap_body(
        &self,
        active: &mut HashMap<tokio::task::Id, String>,
        result: std::result::Result<(tokio::task::Id, ()), tokio::task::JoinError>,
    ) {
        let id = match &result {
            Ok((id, _)) => *id,
            Err(e) => e.id(),
        };
        if let Some(id) = active.remove(&id) {
            if result.is_err() {
                let _ = self.store.defer_body(&id, unix_ms());
            }
        }
    }

    async fn fetch_body(
        &self,
        id: &str,
        peer: &str,
        reply: bool,
        client: HubClient,
        stop: CancelFlag,
    ) {
        let mut suspended = self.suspended.subscribe();
        if *suspended.borrow_and_update() || self.stopped.load(std::sync::atomic::Ordering::Relaxed)
        {
            return;
        }
        let result = tokio::select! {
            biased;
            _=suspended.changed()=>Err(Error::Cancelled),
            result=stop.run(client.message_body(&self.me,id,LONG_BODY_FETCH_MAX))=>result,
        };
        match result {
            Ok(body) => {
                let body = if reply { strip_quote(&body) } else { &body };
                if self.store.finish_body(id, body).unwrap_or(false) {
                    self.host.event(Event::Chat { peer: peer.into() });
                }
            }
            Err(Error::Cancelled) => {}
            Err(_) => {
                let _ = self.store.defer_body(id, unix_ms());
            }
        }
    }
}
