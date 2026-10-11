use super::*;

pub(crate) struct BodyFetch {
    pub id: String,
    pub peer: String,
    pub reply: bool,
}

impl Store {
    pub(crate) fn pending_bodies(&self, after: &str, now: u64) -> Result<Vec<BodyFetch>> {
        self.with(|c| {
            let mut q = c.prepare("SELECT b.id,m.peer,m.reply_to IS NOT NULL FROM body_fetches b JOIN messages m ON m.id=b.id WHERE b.done=0 AND b.retry_at<=?1 AND b.id>?2 ORDER BY b.id LIMIT 64")?;
            let rows = q.query_map(params![now,after], |r| Ok(BodyFetch {id:r.get(0)?,peer:r.get(1)?,reply:r.get(2)?}))?;
            rows.collect()
        })
    }

    pub(crate) fn finish_body(&self, id: &str, body: &str) -> Result<bool> {
        self.with(|c| {
            let tx = c.transaction()?;
            let changed = tx.execute(
                "UPDATE body_fetches SET done=1,failures=0,retry_at=0 WHERE id=? AND done=0",
                [id],
            )?;
            if changed > 0 {
                tx.execute("UPDATE messages SET body=? WHERE id=?", params![body, id])?;
            }
            tx.commit()?;
            Ok(changed > 0)
        })
    }

    pub(crate) fn defer_body(&self, id: &str, now: u64) -> Result<()> {
        self.with(|c| {
            c.execute("UPDATE body_fetches SET failures=MIN(failures+1,8),retry_at=?1+1000*(1 << MIN(failures+1,8)) WHERE id=?2 AND done=0",params![now,id])?;
            Ok(())
        })
    }
}
