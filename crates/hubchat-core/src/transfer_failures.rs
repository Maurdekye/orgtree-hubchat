//! Local-only durable failure intents. Never synced to the identity's other devices.
use super::*;

#[derive(Debug, Clone)]
pub struct TransferFailure {
    pub message_id: String,
    pub peer: String,
    pub episode: i64,
    pub attempts: i64,
    pub error: String,
}
impl TransferFailure {
    /// Stable platform replacement key; replay cannot add a second notification.
    pub fn key(&self) -> String {
        format!("transfer-{}-{}", self.message_id, self.episode)
    }
}

pub(super) fn queue(
    tx: &rusqlite::Transaction<'_>,
    mid: &str,
    error: &str,
) -> rusqlite::Result<()> {
    tx.execute("INSERT OR IGNORE INTO transfer_failures(message_id) SELECT id FROM messages WHERE id=? AND EXISTS(SELECT 1 FROM attachments WHERE message_id=messages.id)",[mid])?;
    tx.execute("UPDATE transfer_failures SET pending=1,error=COALESCE(error,?2) WHERE message_id=?1 AND delivered=0",params![mid,error])?;
    Ok(())
}
pub(super) fn retry(tx: &rusqlite::Transaction<'_>, mid: &str) -> rusqlite::Result<()> {
    tx.execute("UPDATE transfer_failures SET episode=episode+1,pending=0,delivered=0,attempts=0,retry_at=0,error=NULL WHERE message_id=?",[mid])?;
    Ok(())
}
impl Store {
    pub(crate) fn pending_transfer_failures(&self, now: u64) -> Result<Vec<TransferFailure>> {
        self.with(|c| {
            let mut q=c.prepare("SELECT f.message_id,m.peer,f.episode,f.attempts,f.error FROM transfer_failures f JOIN messages m ON m.id=f.message_id WHERE f.pending=1 AND f.retry_at<=? ORDER BY f.retry_at,f.message_id LIMIT 16")?;
            let rows=q.query_map([now],|r|Ok(TransferFailure{message_id:r.get(0)?,peer:r.get(1)?,episode:r.get(2)?,attempts:r.get(3)?,error:r.get(4)?}))?.collect();rows
        })
    }
    /// Reserve before calling the host, with no DB lease held during delivery.
    pub(crate) fn start_failure_notice(&self, n: &TransferFailure, now: u64) -> Result<bool> {
        self.with(|c|c.execute("UPDATE transfer_failures SET attempts=attempts+1,retry_at=?1+30000 WHERE message_id=?2 AND episode=?3 AND pending=1 AND retry_at<=?1",params![now,n.message_id,n.episode]).map(|n|n>0))
    }
    pub(crate) fn finish_failure_notice(&self, n: &TransferFailure) -> Result<()> {
        self.with(|c|c.execute("UPDATE transfer_failures SET pending=0,delivered=1 WHERE message_id=? AND episode=? AND pending=1",params![n.message_id,n.episode]).map(|_|()))
    }
    pub(crate) fn retry_send(&self, id: &str) -> Result<()> {
        self.with(|c|{
            let tx=c.transaction()?;
            let n=tx.execute("UPDATE messages SET state='queued',error=NULL WHERE id=? AND outgoing=1 AND state='failed'",[id])?;
            if n>0 {
                tx.execute("UPDATE attachments SET state='pending',error=NULL WHERE message_id=? AND state!='uploaded'",[id])?;
                tx.execute("DELETE FROM transfer_send_state WHERE message_id=?",[id])?;
                retry(&tx,id)?;
            }
            tx.commit()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn message(store: &Store, id: &str) -> Vec<String> {
        store
            .queue_outgoing(
                &NewOutgoing {
                    id: id.into(),
                    peer: "peer".into(),
                    body: "body".into(),
                    kind: None,
                    reply_to: None,
                    attachments: vec![
                        NewAttachment {
                            name: "a".into(),
                            bytes: 3,
                            source: "unused".into(),
                        },
                        NewAttachment {
                            name: "b".into(),
                            bytes: 3,
                            source: "unused".into(),
                        },
                    ],
                },
                "now",
            )
            .unwrap();
        store
            .message(id)
            .unwrap()
            .unwrap()
            .attachments
            .into_iter()
            .map(|a| a.local_id)
            .collect()
    }
    #[test]
    fn failure_intent_is_durable_aggregated_and_retry_scoped() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("db");
        let s = Store::open(&db).unwrap();
        message(&s, "m");
        s.fail_send("m", "upload refused").unwrap();
        drop(s);
        let s = Store::open(&db).unwrap();
        let n = s.pending_transfer_failures(0).unwrap();
        assert_eq!(n.len(), 1);
        assert_eq!(n[0].error, "upload refused");
        let key = n[0].key();
        assert!(s.start_failure_notice(&n[0], 0).unwrap());
        assert!(s.pending_transfer_failures(29999).unwrap().is_empty());
        drop(s);
        let s = Store::open(&db).unwrap();
        let replay = s.pending_transfer_failures(30000).unwrap();
        assert_eq!(replay[0].key(), key);
        assert_eq!(replay[0].attempts, 1);
        s.finish_failure_notice(&replay[0]).unwrap();
        s.fail_send("m", "late failure").unwrap();
        assert!(s.pending_transfer_failures(60000).unwrap().is_empty());
        s.retry_send("m").unwrap();
        s.fail_send("m", "second attempt failed").unwrap();
        let next = s.pending_transfer_failures(0).unwrap();
        assert_ne!(next[0].key(), key);
        s.finish_failure_notice(&n[0]).unwrap();
        assert_eq!(
            s.pending_transfer_failures(0).unwrap().len(),
            1,
            "old acknowledgement cannot erase new episode"
        );
        s.delete_message("m").unwrap();
        assert!(s.pending_transfer_failures(0).unwrap().is_empty());
    }
    #[test]
    fn user_cancel_success_and_temporary_download_are_silent() {
        let s = Store::open_in_memory().unwrap();
        let ids = message(&s, "m");
        s.cancel_upload(&ids[0]).unwrap();
        s.fail_send("m", "stopping writer").unwrap();
        s.send_worker_failed("m").unwrap();
        assert!(s.pending_transfer_failures(0).unwrap().is_empty());
        s.retry_send("m").unwrap();
        s.mark_sent("m", "hub", "now").unwrap();
        assert!(s.pending_transfer_failures(0).unwrap().is_empty());
        let j = s.begin_download(&ids[0], "part", "dest").unwrap();
        s.download_error(&ids[0], j.attempt, "paused", "network lost")
            .unwrap();
        s.defer_download(&ids[0], 0).unwrap();
        assert!(s.pending_transfer_failures(0).unwrap().is_empty());
        s.cancel_download(&ids[0]).unwrap();
        s.download_error(&ids[0], j.attempt, "failed", "late error")
            .unwrap();
        s.download_worker_failed(&ids[0]).unwrap();
        assert!(s.pending_transfer_failures(0).unwrap().is_empty());
    }
    #[test]
    fn sibling_download_failures_coalesce_and_publication_failure_waits_for_retry() {
        let s = Store::open_in_memory().unwrap();
        let ids = message(&s, "m");
        for id in &ids {
            let j = s.begin_download(id, "part", "dest").unwrap();
            s.download_error(id, j.attempt, "failed", "disk full")
                .unwrap();
        }
        assert_eq!(s.pending_transfer_failures(0).unwrap().len(), 1);
        s.queue_download_job(&ids[0], "part", "dest").unwrap();
        let j = s.begin_download(&ids[0], "part", "dest").unwrap();
        s.download_publishing(&ids[0], j.attempt, "digest", "dest")
            .unwrap();
        s.download_error(&ids[0], j.attempt, "failed", "cannot publish")
            .unwrap();
        assert!(s.queued_downloads("", 0).unwrap().is_empty());
        let n = s.pending_transfer_failures(0).unwrap();
        assert_eq!(n.len(), 1);
        assert_eq!(n[0].episode, 1);
        s.queue_download_job(&ids[0], "part", "dest").unwrap();
        assert!(s.download_job(&ids[0]).unwrap().unwrap().error.is_none());
        assert!(s.pending_transfer_failures(0).unwrap().is_empty());
    }
}
