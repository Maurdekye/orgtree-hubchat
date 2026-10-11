use super::*;
impl Store {
    pub(crate) fn install_legacy_snapshot(
        &self,
        mid: &str,
        old: &[Attachment],
        paths: &[String],
        digests: &[String],
    ) -> Result<bool> {
        if old.len() != paths.len() || old.len() != digests.len() {
            return Err(Error::Invalid("incomplete file preparation".into()));
        }
        self.with(|c| {
            let tx=c.transaction()?;
            for (i,a) in old.iter().enumerate() {
                let n=tx.execute("UPDATE attachments SET staged_path=?1,source_sha256=?2,hub_id=NULL,hub=NULL,upload_confirmed=0,state='pending',error=NULL WHERE local_id=?3 AND message_id=?4 AND staged_path IS NULL AND source IS ?5 AND bytes=?6 AND state!='cancelled' AND EXISTS(SELECT 1 FROM messages WHERE id=?4 AND state='sending')",params![paths[i],digests[i],a.local_id,mid,a.source,a.bytes])?;
                if n!=1 {return Ok(false);}
            }
            tx.commit()?;Ok(true)
        })
    }

    pub(crate) fn fail_legacy_snapshot(
        &self,
        mid: &str,
        old: &[Attachment],
        why: &str,
    ) -> Result<()> {
        self.with(|c| {
            let tx=c.transaction()?;
            let n=tx.execute("UPDATE messages SET state='failed',error=?2 WHERE id=?1 AND state='sending'",params![mid,why])?;
            if n>0 {
                for a in old {
                    tx.execute("UPDATE attachments SET state='failed',error=?2 WHERE local_id=?1 AND staged_path IS NULL AND state!='cancelled'",params![a.local_id,why])?;
                }
                transfer_failures::queue(&tx,mid,why,false)?;
            }
            tx.commit()
        })
    }

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
