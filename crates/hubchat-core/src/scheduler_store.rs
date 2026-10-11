use super::*;
impl Store {
    /// Keyset scan: offline/held heads cannot permanently hide later peers.
    pub(crate) fn send_heads(&self, after: Option<(&str, &str)>, now: u64) -> Result<Vec<Message>> {
        let (at, id) = after.unzip();
        self.messages_where(
            "m.outgoing=1 AND m.state IN ('queued','sending')
             AND (?1 IS NULL OR (m.created_at,m.id)>(?1,?2))
             AND NOT EXISTS(SELECT 1 FROM transfer_send_state s WHERE s.message_id=m.id AND s.retry_at>?3)
             AND NOT EXISTS(SELECT 1 FROM messages p WHERE p.outgoing=1 AND p.peer=m.peer
               AND p.state IN ('queued','sending') AND (p.created_at,p.id)<(m.created_at,m.id))
             ORDER BY m.created_at,m.id LIMIT 64", params![at,id,now])
    }
    pub(crate) fn defer_send(&self, id: &str, hub: &str, now: u64) -> Result<()> {
        self.with(|c| {
            let tx=c.transaction()?;
            tx.execute("INSERT INTO transfer_send_state(message_id,hub,failures) SELECT id,?2,1 FROM messages WHERE id=?1 AND state IN ('queued','sending') ON CONFLICT(message_id) DO UPDATE SET hub=excluded.hub,failures=MIN(failures+1,8)",params![id,hub])?;
            tx.execute("UPDATE transfer_send_state SET retry_at=?1+MIN(300000,1000*(1 << failures)) WHERE message_id=?2",params![now,id])?;
            tx.execute("UPDATE messages SET state='queued',error=COALESCE(error,'Connection interrupted; waiting to retry') WHERE id=? AND state IN ('queued','sending')",[id])?;
            tx.commit()
        })
    }
    /// A user retry clears every delay; a reconnect clears only that hub's.
    /// Failure counters remain, so renewed server failures still back off.
    pub(crate) fn wake_transfer_retries(&self, hub: Option<&str>) -> Result<()> {
        self.with(|c| {
            let tx=c.transaction()?;
            tx.execute("UPDATE transfer_send_state SET retry_at=0 WHERE ?1 IS NULL OR hub=?1",[hub])?;
            tx.execute("UPDATE download_jobs SET retry_at=0 WHERE ?1 IS NULL OR local_id IN (SELECT a.local_id FROM attachments a JOIN messages m ON m.id=a.message_id WHERE COALESCE(a.hub,m.hub)=?1)",[hub])?;
            tx.commit()
        })
    }
    pub(crate) fn send_worker_failed(&self, id: &str) -> Result<()> {
        self.with(|c| {
            let tx=c.transaction()?;
            tx.execute("UPDATE attachments SET state='failed',error='Transfer worker stopped unexpectedly; retry' WHERE message_id=? AND state='uploading'",[id])?;
            tx.execute("UPDATE messages SET state='failed',error='Transfer worker stopped unexpectedly; retry' WHERE id=? AND state IN ('queued','sending')",[id])?;
            tx.commit()
        })
    }
    pub(crate) fn download_worker_failed(&self, id: &str) -> Result<()> {
        self.with(|c| {
            let tx=c.transaction()?;
            let n=tx.execute("UPDATE download_jobs SET state=CASE WHEN state='publishing' THEN state ELSE 'failed' END,error='Transfer worker stopped unexpectedly; retry' WHERE local_id=? AND state NOT IN ('done','cancelled')",[id])?;
            if n>0 {tx.execute("UPDATE attachments SET state='failed',error='Transfer worker stopped unexpectedly; retry' WHERE local_id=?",[id])?;}
            tx.commit()
        })
    }
    pub(crate) fn clear_send_retry(&self, id: &str) -> Result<()> {
        self.with(|c| {
            c.execute("DELETE FROM transfer_send_state WHERE message_id=?", [id])
                .map(|_| ())
        })
    }
    pub(crate) fn queued_downloads(
        &self,
        after: &str,
        now: u64,
    ) -> Result<Vec<(String, String, String)>> {
        self.with(|c| {
            let mut q=c.prepare("SELECT j.local_id,a.message_id,COALESCE(a.hub,m.hub) FROM download_jobs j JOIN attachments a ON a.local_id=j.local_id JOIN messages m ON m.id=a.message_id WHERE j.state IN ('queued','running','publishing','paused') AND j.retry_at<=?1 AND j.local_id>?2 ORDER BY j.local_id LIMIT 64")?;
            let rows=q.query_map(params![now,after],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?)))?.collect();rows
        })
    }
    pub(crate) fn queue_download_job(&self, id: &str, partial: &str, dest: &str) -> Result<()> {
        self.with(|c| {
            let tx=c.transaction()?;
            tx.execute("INSERT OR IGNORE INTO download_jobs(local_id,partial,destination) VALUES(?,?,?)",params![id,partial,dest])?;
            tx.execute("UPDATE download_jobs SET state='queued',retry_at=0,error=NULL WHERE local_id=? AND state IN ('failed','expired','cancelled','paused')",[id])?;
            tx.commit()
        })
    }
    pub(crate) fn defer_download(&self, id: &str, now: u64) -> Result<()> {
        self.with(|c| c.execute("UPDATE download_jobs SET state='queued',retry_at=?1+MIN(300000,1000*(1 << MIN(attempt,8))) WHERE local_id=?2 AND state='failed'",params![now,id]).map(|_|()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn head_scan_advances_past_held_peers_and_backoff_survives_reopen() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("store.db");
        let store = Store::open(&path).unwrap();
        for i in 0..130 {
            store
                .queue_outgoing(
                    &NewOutgoing {
                        id: format!("m{i:03}"),
                        peer: format!("p{i:03}"),
                        body: "text".into(),
                        kind: None,
                        reply_to: None,
                        attachments: vec![],
                    },
                    "same-time",
                )
                .unwrap();
        }
        store
            .queue_outgoing(
                &NewOutgoing {
                    id: "later".into(),
                    peer: "p000".into(),
                    body: "must wait".into(),
                    kind: None,
                    reply_to: None,
                    attachments: vec![],
                },
                "zzz",
            )
            .unwrap();
        store.set_state("m000", "failed", None).unwrap();
        let first = store.send_heads(None, 1000).unwrap();
        assert_eq!(first.len(), 64);
        assert_eq!(first[0].id, "m001");
        let last = first.last().unwrap();
        let next = store
            .send_heads(Some((&last.created_at, &last.id)), 1000)
            .unwrap();
        assert_eq!(next.len(), 64);
        let last = next.last().unwrap();
        let next = store
            .send_heads(Some((&last.created_at, &last.id)), 1000)
            .unwrap();
        assert_eq!(next.len(), 2);
        assert_eq!(next[0].id, "m129");
        assert_eq!(
            next[1].id, "later",
            "failed messages do not hold later mail"
        );
        store.defer_send("m001", "hub-a", 1000).unwrap();
        drop(store);
        let store = Store::open(&path).unwrap();
        assert!(!store
            .send_heads(None, 2999)
            .unwrap()
            .iter()
            .any(|m| m.id == "m001"));
        assert_eq!(store.send_heads(None, 3000).unwrap()[0].id, "m001");
        assert!(store
            .message("m001")
            .unwrap()
            .unwrap()
            .error
            .unwrap()
            .contains("retry"));
    }
    #[test]
    fn reconnect_clears_only_its_hubs_delays_and_success_removes_retry_state() {
        let store = Store::open_in_memory().unwrap();
        let mut lids = Vec::new();
        for (id, hub) in [("a", "hub-a"), ("b", "hub-b")] {
            store
                .queue_outgoing(
                    &NewOutgoing {
                        id: id.into(),
                        peer: id.into(),
                        body: "text".into(),
                        kind: None,
                        reply_to: None,
                        attachments: vec![NewAttachment {
                            name: "file".into(),
                            bytes: 3,
                            source: "unused".into(),
                        }],
                    },
                    "now",
                )
                .unwrap();
            let lid = store.message(id).unwrap().unwrap().attachments[0]
                .local_id
                .clone();
            store.set_upload(&lid, "uploaded", "remote", hub).unwrap();
            let j = store.begin_download(&lid, "part", "dest").unwrap();
            store
                .download_error(&lid, j.attempt, "failed", "temporary")
                .unwrap();
            store.defer_download(&lid, 1000).unwrap();
            store.defer_send(id, hub, 1000).unwrap();
            lids.push(lid);
        }
        assert!(store.send_heads(None, 1001).unwrap().is_empty());
        assert!(store.queued_downloads("", 1001).unwrap().is_empty());
        store.wake_transfer_retries(Some("hub-a")).unwrap();
        assert_eq!(store.send_heads(None, 1001).unwrap()[0].id, "a");
        let jobs = store.queued_downloads("", 1001).unwrap();
        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs[0].0, lids[0]);
        store.defer_send("a", "hub-a", 1001).unwrap();
        assert!(
            store
                .send_heads(None, 5000)
                .unwrap()
                .iter()
                .all(|m| m.id != "a"),
            "failure count kept: next delay is four seconds"
        );
        store.wake_transfer_retries(None).unwrap();
        assert_eq!(store.send_heads(None, 1002).unwrap().len(), 2);
        assert_eq!(store.queued_downloads("", 1002).unwrap().len(), 2);
        store.mark_sent("a", "hub-a", "now").unwrap();
        let n: i64 = store
            .with(|c| {
                c.query_row(
                    "SELECT count(*) FROM transfer_send_state WHERE message_id='a'",
                    [],
                    |r| r.get(0),
                )
            })
            .unwrap();
        assert_eq!(n, 0);
    }
}
