use super::*;
pub(crate) const BODY_MISSING: &str = "The rest of this message is no longer on the hub.";

pub(crate) struct BodyFetch {
    pub id: String,
    pub peer: String,
    pub reply: bool,
    /// The message's checked meta: its plain-text part goes once the body is whole.
    pub meta: Option<String>,
}

impl Store {
    pub(crate) fn pending_bodies(&self, after: &str, now: u64) -> Result<Vec<BodyFetch>> {
        self.with(|c| {
            let mut q = c.prepare("SELECT b.id,m.peer,m.reply_to IS NOT NULL,m.meta FROM body_fetches b JOIN messages m ON m.id=b.id WHERE b.done=0 AND b.retry_at<=?1 AND b.id>?2 ORDER BY b.id LIMIT 64")?;
            let rows = q.query_map(params![now,after], |r| Ok(BodyFetch {id:r.get(0)?,peer:r.get(1)?,reply:r.get(2)?,meta:r.get(3)?}))?;
            rows.collect()
        })
    }

    pub(crate) fn finish_body(&self, id: &str, body: &str) -> Result<bool> {
        self.with(|c| {
            let tx = c.transaction()?;
            let changed = tx.execute(
                "UPDATE body_fetches SET done=1,failures=0,retry_at=0,unavailable=NULL WHERE id=? AND done=0",
                [id],
            )?;
            if changed > 0 {
                tx.execute("UPDATE messages SET body=? WHERE id=?", params![body, id])?;
            }
            tx.commit()?;
            Ok(changed > 0)
        })
    }

    pub(crate) fn body_routes(&self, id: &str) -> Result<Vec<String>> {
        self.with(|c| {
            let mut q=c.prepare("SELECT h.hub FROM message_hubs h WHERE h.id=? AND NOT EXISTS(SELECT 1 FROM body_missing b WHERE b.id=h.id AND b.hub=h.hub) ORDER BY h.hub")?;
            let rows=q.query_map([id],|r|r.get(0))?;
            rows.collect()
        })
    }

    pub(crate) fn body_copy_missing(&self, id: &str, hub: &str) -> Result<bool> {
        self.with(|c| {
            let tx=c.transaction()?;
            tx.execute("INSERT OR IGNORE INTO body_missing(id,hub) SELECT id,?2 FROM body_fetches WHERE id=?1 AND done=0",params![id,hub])?;
            let n=tx.execute("UPDATE body_fetches SET done=1,retry_at=0,unavailable=?2 WHERE id=?1 AND done=0 AND NOT EXISTS(SELECT 1 FROM message_hubs h WHERE h.id=?1 AND NOT EXISTS(SELECT 1 FROM body_missing b WHERE b.id=h.id AND b.hub=h.hub))",params![id,BODY_MISSING])?;
            tx.commit()?;
            Ok(n>0)
        })
    }

    pub(crate) fn stop_body(&self, id: &str, reason: &str) -> Result<bool> {
        self.with(|c| {
            c.execute(
                "UPDATE body_fetches SET done=1,retry_at=0,unavailable=?2 WHERE id=?1 AND done=0",
                params![id, reason],
            )
            .map(|n| n > 0)
        })
    }

    #[cfg(test)]
    pub(crate) fn body_unavailable(&self, id: &str) -> Result<Option<String>> {
        self.with(|c| {
            c.query_row(
                "SELECT unavailable FROM body_fetches WHERE id=?",
                [id],
                |r| r.get(0),
            )
        })
    }

    pub(crate) fn defer_body(&self, id: &str, now: u64) -> Result<()> {
        self.with(|c| {
            c.execute("UPDATE body_fetches SET failures=MIN(failures+1,8),retry_at=?1+1000*(1 << MIN(failures+1,8)) WHERE id=?2 AND done=0",params![now,id])?;
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn expired_copy_is_not_retried_but_other_and_new_copies_remain_available() {
        let store = Store::open_in_memory().unwrap();
        let m: SyncedMessage = serde_json::from_value(serde_json::json!({
            "id":"m","from":"peer","to":"me","body":"preview",
            "received_at":"2026-10-11T00:00:00Z","body_bytes":100
        }))
        .unwrap();
        for hub in ["a", "b", "c"] {
            store.add_hub(hub, "now").unwrap();
        }
        store.upsert_synced("a", "me", &m).unwrap();
        store.upsert_synced("b", "me", &m).unwrap();
        assert!(!store.body_copy_missing("m", "a").unwrap());
        assert_eq!(store.body_routes("m").unwrap(), vec!["b"]);
        assert_eq!(store.pending_bodies("", 100).unwrap().len(), 1);
        assert!(store.body_copy_missing("m", "b").unwrap());
        assert!(store.pending_bodies("", 100).unwrap().is_empty());
        store.upsert_synced("a", "me", &m).unwrap();
        assert!(store.pending_bodies("", 100).unwrap().is_empty());
        store.upsert_synced("c", "me", &m).unwrap();
        assert_eq!(store.body_routes("m").unwrap(), vec!["c"]);
        assert_eq!(store.pending_bodies("", 100).unwrap().len(), 1);
        store.finish_body("m", "full body").unwrap();
        store.upsert_synced("a", "me", &m).unwrap();
        assert!(store.pending_bodies("", 100).unwrap().is_empty());
        assert_eq!(store.message("m").unwrap().unwrap().body, "full body");
    }
}
