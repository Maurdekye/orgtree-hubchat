use super::*;
use crate::download::DownloadCheckpoint;

/// Private paths stay in the core. The UI sees states and confirmed bytes.
#[derive(Debug, Clone, Serialize)]
pub struct DownloadJob {
    pub local_id: String,
    #[serde(skip)]
    pub partial: String,
    #[serde(skip)]
    pub destination: String,
    #[serde(skip)]
    pub exported: bool,
    pub state: String,
    pub attempt: i64,
    pub checkpoint: DownloadCheckpoint,
    #[serde(skip)]
    pub digest: Option<String>,
    pub error: Option<String>,
}
fn row(r: &rusqlite::Row<'_>) -> rusqlite::Result<DownloadJob> {
    Ok(DownloadJob {
        local_id: r.get(0)?,
        partial: r.get(1)?,
        destination: r.get(2)?,
        state: r.get(3)?,
        attempt: r.get(4)?,
        checkpoint: DownloadCheckpoint {
            confirmed: r.get(5)?,
            validator: r.get(6)?,
            updated_ms: r.get(7)?,
        },
        digest: r.get(8)?,
        error: r.get(9)?,
        exported: r.get(10)?,
    })
}
const COLUMNS: &str =
    "local_id,partial,destination,state,attempt,confirmed,validator,updated_ms,digest,error,exported";
impl Store {
    pub(crate) fn transfer_peer(&self, id: &str) -> Result<Option<String>> {
        self.with(|c| c.query_row("SELECT m.peer FROM attachments a JOIN messages m ON m.id=a.message_id WHERE a.local_id=?", [id], |r| r.get(0)).optional())
    }
    pub fn download_job(&self, id: &str) -> Result<Option<DownloadJob>> {
        self.with(|c| {
            c.query_row(
                &format!("SELECT {COLUMNS} FROM download_jobs WHERE local_id=?"),
                [id],
                row,
            )
            .optional()
        })
    }
    /// The engine's in-memory writer claim must already be held. A publishing
    /// intent is reconciled before starting new network I/O.
    pub(crate) fn begin_download(
        &self,
        id: &str,
        partial: &str,
        dest: &str,
    ) -> Result<DownloadJob> {
        self.with(|c| {
            let tx=c.transaction()?;
            tx.execute("INSERT OR IGNORE INTO download_jobs(local_id,partial,destination) VALUES(?,?,?)",params![id,partial,dest])?;
            tx.execute("UPDATE download_jobs SET attempt=attempt+1,state=CASE WHEN state IN ('publishing','exporting','done') THEN state ELSE 'running' END,error=NULL WHERE local_id=?",[id])?;
            tx.execute("UPDATE attachments SET state='downloading',error=NULL WHERE local_id=? AND EXISTS(SELECT 1 FROM download_jobs WHERE local_id=? AND state!='done')",[id,id])?;
            let job=tx.query_row(&format!("SELECT {COLUMNS} FROM download_jobs WHERE local_id=?"),[id],row)?;
            tx.commit()?;Ok(job)
        })
    }
    pub(crate) fn restart_missing_download(&self, id: &str) -> Result<()> {
        self.with(|c| {
            let tx=c.transaction()?;
            let n = tx.execute("UPDATE download_jobs SET state='queued',confirmed=0,validator=NULL,digest=NULL,error=NULL,exported=0 WHERE local_id=? AND state='done'",[id])?;
            if n > 0 { tx.execute("UPDATE attachments SET local_path=NULL WHERE local_id=?",[id])?; }
            tx.commit()
        })
    }
    pub(crate) fn download_checkpoint(
        &self,
        id: &str,
        attempt: i64,
        cp: &DownloadCheckpoint,
    ) -> Result<()> {
        let changed=self.with(|c|c.execute("UPDATE download_jobs SET confirmed=?,validator=?,updated_ms=? WHERE local_id=? AND attempt=? AND state='running'",params![cp.confirmed,cp.validator,cp.updated_ms,id,attempt]))?;
        if changed == 0 {
            Err(Error::Cancelled)
        } else {
            Ok(())
        }
    }
    pub(crate) fn download_publishing(
        &self,
        id: &str,
        attempt: i64,
        digest: &str,
        dest: &str,
    ) -> Result<()> {
        let n=self.with(|c|c.execute("UPDATE download_jobs SET state='publishing',digest=?,destination=? WHERE local_id=? AND attempt=? AND state IN ('running','publishing')",params![digest,dest,id,attempt]))?;
        if n == 0 {
            Err(Error::Cancelled)
        } else {
            Ok(())
        }
    }
    /// Record the platform's final location atomically with the attachment.
    /// Matching the prior destination rejects a stale export after a retry.
    pub(crate) fn download_exported(&self, id: &str, from: &str, to: &str) -> Result<()> {
        let n = self.with(|c| {
            let tx = c.transaction()?;
            let n = tx.execute("UPDATE download_jobs SET destination=?,exported=1,state='done',error=NULL WHERE local_id=? AND destination=? AND state IN ('exporting','done')", params![to,id,from])?;
            if n > 0 {
                tx.execute("UPDATE attachments SET local_path=?,state='done',error=NULL WHERE local_id=?", params![to,id])?;
            }
            tx.commit()?;
            Ok(n)
        })?;
        if n == 0 {
            Err(Error::Invalid(
                "download changed before export completed".into(),
            ))
        } else {
            Ok(())
        }
    }
    /// Damaged private bytes cannot be published or resumed. Preserve external
    /// destination files and make the next explicit retry fetch fresh bytes.
    pub(crate) fn reset_damaged_download(&self, id: &str, attempt: i64) -> Result<()> {
        self.with(|c| {
            let tx = c.transaction()?;
            let n = tx.execute(
                "UPDATE download_jobs SET state='failed',confirmed=0,validator=NULL,digest=NULL,error='download bytes changed or disappeared; retry from zero' WHERE local_id=? AND attempt=? AND state IN ('running','publishing')",
                params![id, attempt],
            )?;
            if n > 0 {
                tx.execute("UPDATE attachments SET state='failed',error='download bytes changed or disappeared; retry from zero' WHERE local_id=?", [id])?;
            }
            tx.commit()
        })
    }
    pub(crate) fn download_done(&self, id: &str, attempt: i64, path: &str) -> Result<()> {
        let n=self.with(|c|{
            let tx=c.transaction()?;
            let n=tx.execute("UPDATE download_jobs SET state='exporting',error=NULL WHERE local_id=? AND attempt=? AND destination=? AND state='publishing'",params![id,attempt,path])?;
            if n>0 {tx.execute("UPDATE attachments SET state='downloading',error=NULL WHERE local_id=?",[id])?;}
            tx.commit()?;Ok(n)
        })?;
        if n == 0 {
            Err(Error::Cancelled)
        } else {
            Ok(())
        }
    }
    pub(crate) fn download_error(
        &self,
        id: &str,
        attempt: i64,
        state: &str,
        error: &str,
    ) -> Result<()> {
        self.with(|c|{
            let tx=c.transaction()?;
            // User cancellation and publication win against stale worker errors.
            let n=tx.execute("UPDATE download_jobs SET state=CASE WHEN state IN ('publishing','exporting') THEN state ELSE ?1 END,error=?2 WHERE local_id=?3 AND attempt=?4 AND (state='running' OR (state IN ('publishing','exporting') AND ?1!='paused'))",params![state,error,id,attempt])?;
            if n>0 {tx.execute("UPDATE attachments SET state=?,error=? WHERE local_id=?",params![state,error,id])?;}
            tx.commit()
        })
    }
    /// None means this is not a download; false means completion already won.
    pub(crate) fn cancel_download(&self, id: &str) -> Result<Option<bool>> {
        self.with(|c| {
            let tx = c.transaction()?;
            let state: Option<String> = tx
                .query_row(
                    "SELECT state FROM download_jobs WHERE local_id=?",
                    [id],
                    |r| r.get(0),
                )
                .optional()?;
            let Some(state) = state else { return Ok(None) };
            if matches!(state.as_str(), "publishing" | "exporting" | "done") {
                return Ok(Some(false));
            }
            tx.execute(
                "UPDATE download_jobs SET state='cancelled',error=NULL WHERE local_id=?",
                [id],
            )?;
            tx.execute(
                "UPDATE attachments SET state='cancelled',error=NULL WHERE local_id=?",
                [id],
            )?;
            tx.commit()?;
            Ok(Some(true))
        })
    }
}
