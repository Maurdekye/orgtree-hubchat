//! Replay-safe notification reply admission.
use super::*;
use crate::engine::now;

impl Store {
    /// Read only this admitted reply's delivery state while identity is stable.
    pub(crate) fn notification_reply_state(&self, owner: &str, id: &str, peer: &str) -> Result<&'static str> {
        if owner.is_empty() || peer.is_empty() || uuid::Uuid::parse_str(id).is_err() {
            return Err(Error::Invalid("Invalid notification reply".into()));
        }
        let c = self.con.lock().unwrap_or_else(|p|p.into_inner());
        let current: Option<String> = c.query_row("SELECT value FROM meta WHERE key='identity.address'", [], |r|r.get(0)).optional().map_err(db)?;
        if current.as_deref() != Some(owner) { return Err(Error::Invalid("Notification identity changed".into())); }
        let admitted: bool = c.query_row("SELECT EXISTS(SELECT 1 FROM notification_replies WHERE id=?)", [id], |r|r.get(0)).map_err(db)?;
        if !admitted { return Ok("retry"); }
        let row: Option<(String, bool, String)> = c.query_row("SELECT peer,outgoing,state FROM messages WHERE id=?", [id], |r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional().map_err(db)?;
        match row {
            // A consumed UUID survives an intentional message/chat deletion.
            None => Ok("gone"),
            Some((p, outgoing, state)) if p == peer && outgoing => Ok(match state.as_str() {
                "sent" | "fetched" | "delivered" | "read" => "sent",
                "failed" => "failed",
                _ => "retry",
            }),
            _ => Err(Error::Invalid("Notification reply target changed".into())),
        }
    }
    pub(crate) fn queue_notification_reply(&self, owner: &str, id: &str, peer: &str, body: &str) -> Result<bool> {
        if owner.is_empty() || peer.is_empty() || owner == peer || body.trim().is_empty()
            || uuid::Uuid::parse_str(id).is_err() {
            return Err(Error::Invalid("This notification reply is empty or invalid.".into()));
        }
        {
            let mut c = self.con.lock().unwrap_or_else(|p| p.into_inner());
            let tx = c.transaction().map_err(db)?;
            // The same transaction as admission: an old engine cannot queue into
            // another account's freshly wiped store during an identity change.
            let current: Option<String> = tx.query_row("SELECT value FROM meta WHERE key='identity.address'", [], |r| r.get(0)).optional().map_err(db)?;
            if current.as_deref() != Some(owner) {
                return Err(Error::Invalid("The notification belongs to another identity.".into()));
            }
            let consumed = tx.execute("INSERT OR IGNORE INTO notification_replies(id) VALUES(?)", [id]).map_err(db)?;
            if consumed == 0 { return Ok(false); }
            let at = now();
            tx.execute("INSERT INTO messages(id,peer,outgoing,body,sent_at,created_at,state) VALUES(?,?,1,?,?,?,'queued')", params![id, peer, body, at, at]).map_err(db)?;
            show_local_outgoing(&tx, peer, &at).map_err(db)?;
            tx.commit().map_err(db)?;
            Ok(true)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const OWNER: &str = "alex.123456";
    const PEER: &str = "pat.abcdef";
    const ID: &str = "bb05866b-b96c-46c0-ae3f-236b3dd65fa2";
    fn store() -> (tempfile::TempDir, Store) {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(&dir.path().join("reply.sqlite3")).unwrap();
        store.set_meta("identity.address", OWNER).unwrap();
        (dir, store)
    }
    #[test]
    fn offline_admission_preserves_text_and_existing_draft() {
        let (_dir, s) = store();
        s.set_draft(PEER, "unfinished composer").unwrap();
        let text = "  Hello\n世界 🙂  ";
        assert!(s.queue_notification_reply(OWNER, ID, PEER, text).unwrap());
        let m = s.message(ID).unwrap().unwrap();
        assert!(m.outgoing);
        assert_eq!((m.peer.as_str(), m.body.as_str(), m.state.as_str()), (PEER, text, "queued"));
        assert!(m.attachments.is_empty());
        assert!(m.reply_to.is_none());
        assert_eq!(s.draft(PEER).unwrap().as_deref(), Some("unfinished composer"));
    }
    #[test]
    fn worker_replay_after_restart_and_message_deletion_never_resends() {
        let (dir, s) = store();
        assert!(s.queue_notification_reply(OWNER, ID, PEER, "once").unwrap());
        drop(s);
        let s = Store::open(&dir.path().join("reply.sqlite3")).unwrap();
        assert!(!s.queue_notification_reply(OWNER, ID, PEER, "once").unwrap());
        s.delete_chat(PEER).unwrap();
        assert!(!s.queue_notification_reply(OWNER, ID, PEER, "once").unwrap());
        assert!(s.message(ID).unwrap().is_none());
    }
    #[test]
    fn identity_change_and_invalid_input_cannot_admit() {
        let (_dir, s) = store();
        for (owner, id, peer, body) in [("other.ffffff", ID, PEER, "hello"), (OWNER, ID, OWNER, "hello"), (OWNER, ID, PEER, " \n\t"), (OWNER, "../bad", PEER, "hello")] {
            assert!(s.queue_notification_reply(owner,id,peer,body).is_err());
        }
        s.wipe().unwrap();
        s.set_meta("identity.address", "other.ffffff").unwrap();
        assert!(s.queue_notification_reply(OWNER,ID,PEER,"old reply").is_err());
        assert!(s.message(ID).unwrap().is_none());
    }
    #[test]
    fn receipt_and_queue_commit_or_rollback_together() {
        let (_dir, s) = store();
        s.with(|c| c.execute("INSERT INTO messages(id,peer,outgoing,body,created_at,state) VALUES(?,?,0,'incoming','now','received')", params![ID, PEER]).map(|_| ())).unwrap();
        assert!(s.queue_notification_reply(OWNER,ID,PEER,"reply").is_err());
        s.delete_chat(PEER).unwrap();
        assert!(s.queue_notification_reply(OWNER,ID,PEER,"reply").unwrap());
    }
    #[test]
    fn competing_workers_admit_only_one_message() {
        let (_dir, s) = store();
        let s = std::sync::Arc::new(s);
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(8));
        let jobs: Vec<_> = (0..8).map(|_| {
            let s = s.clone(); let barrier = barrier.clone();
            std::thread::spawn(move || { barrier.wait(); s.queue_notification_reply(OWNER,ID,PEER,"once").unwrap() })
        }).collect();
        assert_eq!(jobs.into_iter().map(|j| j.join().unwrap()).filter(|added| *added).count(), 1);
    }
}
