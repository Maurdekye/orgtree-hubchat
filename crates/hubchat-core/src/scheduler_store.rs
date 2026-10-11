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
               AND p.state IN ('queued','sending','failed') AND (p.created_at,p.id)<(m.created_at,m.id))
             ORDER BY m.created_at,m.id LIMIT 64", params![at,id,now])
    }
    pub(crate) fn defer_send(&self, id: &str, now: u64) -> Result<()> {
        self.with(|c| {
            let tx=c.transaction()?;
            tx.execute("INSERT INTO transfer_send_state(message_id,failures) SELECT id,1 FROM messages WHERE id=? AND state IN ('queued','sending') ON CONFLICT(message_id) DO UPDATE SET failures=MIN(failures+1,8)",[id])?;
            tx.execute("UPDATE transfer_send_state SET retry_at=?1+MIN(300000,1000*(1 << failures)) WHERE message_id=?2",params![now,id])?;
            tx.execute("UPDATE messages SET state='queued',error=COALESCE(error,'Connection interrupted; waiting to retry') WHERE id=? AND state IN ('queued','sending')",[id])?;
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
        assert_eq!(next.len(), 1);
        assert_eq!(next[0].id, "m129");
        store.defer_send("m001", 1000).unwrap();
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
}
