use super::*;
impl Store {
    pub(crate) fn stale_downloads(&self, now: u64) -> Result<Vec<(String, String)>> {
        self.with(|c| {
            let mut q=c.prepare("SELECT local_id,partial FROM download_jobs WHERE updated_ms<=?1 AND cleanup_at<=?2 AND state NOT IN ('publishing','exporting','done') ORDER BY cleanup_at,local_id LIMIT 32")?;
            let rows=q.query_map(params![now.saturating_sub(86400000),now],|r|Ok((r.get(0)?,r.get(1)?)))?.collect(); rows
        })
    }
    pub(crate) fn check_download_later(&self, id: &str, now: u64) -> Result<()> {
        self.with(|c| {
            c.execute(
                "UPDATE download_jobs SET cleanup_at=? WHERE local_id=?",
                params![now + 30000, id],
            )
            .map(|_| ())
        })
    }
    pub(crate) fn expire_download_partial(
        &self,
        id: &str,
        old: &str,
        new: &str,
        now: u64,
    ) -> Result<bool> {
        self.with(|c|{
            let tx=c.transaction()?;
            let n=tx.execute("UPDATE download_jobs SET partial=?1,confirmed=0,validator=NULL,updated_ms=?2,cleanup_at=?2,digest=NULL,error='Partial download expired; restarting from zero' WHERE local_id=?3 AND partial=?4 AND updated_ms<=?5 AND state NOT IN ('publishing','exporting','done')",params![new,now,id,old,now.saturating_sub(86400000)])?;
            tx.commit()?;Ok(n>0)
        })
    }
    pub(crate) fn download_gc_due(&self, now: u64) -> Result<Vec<(String, String)>> {
        self.with(|c|{
            let mut q=c.prepare("SELECT path,local_id FROM download_gc WHERE retry_after<=? ORDER BY retry_after,path LIMIT 32")?;
            let rows=q.query_map([now],|r|Ok((r.get(0)?,r.get(1)?)))?.collect();rows
        })
    }
    pub(crate) fn download_partial_referenced(&self, path: &str) -> Result<bool> {
        self.with(|c| {
            c.query_row(
                "SELECT EXISTS(SELECT 1 FROM download_jobs WHERE partial=?)",
                [path],
                |r| r.get(0),
            )
        })
    }
    pub(crate) fn download_gc_removed(&self, path: &str) -> Result<()> {
        self.with(|c| {
            c.execute("DELETE FROM download_gc WHERE path=?", [path])
                .map(|_| ())
        })
    }
    pub(crate) fn download_gc_retry(&self, path: &str, now: u64) -> Result<()> {
        self.with(|c| {
            c.execute(
                "UPDATE download_gc SET retry_after=? WHERE path=?",
                params![now + 30000, path],
            )
            .map(|_| ())
        })
    }
}
