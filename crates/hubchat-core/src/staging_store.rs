use super::*;
impl Store {
    pub(crate) fn staging_due(&self, at: i64) -> Result<Vec<String>> {
        self.with(|c| {
            let mut q=c.prepare("SELECT path FROM staging_gc WHERE retry_after<=? ORDER BY retry_after,path LIMIT 32")?;
            let rows=q.query_map([at],|r|r.get(0))?; rows.collect()
        })
    }
    pub(crate) fn staging_in_use(&self, path: &str) -> Result<bool> {
        self.with(|c|c.query_row("SELECT EXISTS(SELECT 1 FROM attachments a JOIN messages m ON m.id=a.message_id WHERE a.staged_path=? AND m.state IN ('queued','sending','failed'))",[path],|r|r.get(0)))
    }
    pub(crate) fn staging_directory_referenced(&self, prefix: &str) -> Result<bool> {
        self.with(|c|c.query_row("SELECT EXISTS(SELECT 1 FROM attachments WHERE substr(staged_path,1,length(?1))=?1)",[prefix],|r|r.get(0)))
    }
    pub(crate) fn staging_retry(&self, path: &str, at: i64) -> Result<()> {
        self.with(|c| {
            c.execute(
                "UPDATE staging_gc SET retry_after=? WHERE path=?",
                params![at, path],
            )
            .map(|_| ())
        })
    }
    pub(crate) fn staging_removed(&self, path: &str) -> Result<()> {
        self.with(|c| {
            let tx = c.transaction()?;
            tx.execute(
                "UPDATE attachments SET staged_path=NULL WHERE staged_path=?",
                [path],
            )?;
            tx.execute("DELETE FROM staging_gc WHERE path=?", [path])?;
            tx.commit()
        })
    }
}
