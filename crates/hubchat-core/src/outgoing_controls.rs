//! Atomic local controls; the engine serializes retry/admission with send writers.
use super::*;
impl Store {
    pub(crate) fn cancel_upload(&self, id: &str) -> Result<(String, Vec<String>)> {
        self.with(|c| {
            let tx=c.transaction()?;
            let row:Option<(String,String,String,String,bool)>=tx.query_row(
                "SELECT m.id,m.peer,m.state,a.state,m.outgoing FROM attachments a JOIN messages m ON m.id=a.message_id WHERE a.local_id=?",[id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?))).optional()?;
            let Some((mid,peer,state,attachment,outgoing))=row else {return Ok(None)};
            if !outgoing || (!matches!(state.as_str(),"queued"|"sending") && attachment!="cancelled") || !matches!(attachment.as_str(),"pending"|"uploading"|"cancelled") {return Ok(None)}
            tx.execute("UPDATE attachments SET state='cancelled',error=NULL WHERE local_id=?",[id])?;
            tx.execute("UPDATE messages SET state='failed',error='Upload cancelled; retry or remove the file' WHERE id=?",[&mid])?;
            let lids={let mut q=tx.prepare("SELECT local_id FROM attachments WHERE message_id=?")?;let rows=q.query_map([&mid],|r|r.get(0))?.collect::<rusqlite::Result<Vec<String>>>()?;rows};
            tx.commit()?;Ok(Some((peer,lids)))
        })?.ok_or_else(||Error::Invalid("this upload can no longer be cancelled".into()))
    }
    pub(crate) fn requeue_send(&self, id: &str) -> Result<()> {
        self.with(|c| {
            c.execute(
                "UPDATE messages SET state='queued',error=NULL WHERE id=? AND state='sending'",
                [id],
            )
            .map(|_| ())
        })
    }
    pub(crate) fn begin_upload(&self, id: &str) -> Result<()> {
        let n=self.with(|c|c.execute("UPDATE attachments SET state='uploading',error=NULL WHERE local_id=? AND state NOT IN ('cancelled','failed') AND EXISTS(SELECT 1 FROM messages m WHERE m.id=attachments.message_id AND m.state='sending')",[id]))?;
        if n == 0 {
            return Err(Error::Cancelled);
        }
        Ok(())
    }
    pub(crate) fn complete_upload(&self, id: &str, hub_id: &str, hub: &str) -> Result<()> {
        let n=self.with(|c|c.execute("UPDATE attachments SET state='uploaded',hub_id=?,hub=?,upload_confirmed=bytes,error=NULL WHERE local_id=? AND state='uploading' AND EXISTS(SELECT 1 FROM messages m WHERE m.id=attachments.message_id AND m.state='sending')",params![hub_id,hub,id]))?;
        if n == 0 {
            return Err(Error::Cancelled);
        }
        Ok(())
    }
    /// Removing a held file is explicit; its private copy is collected by the
    /// existing deletion trigger. Never turn a file-only message into blank mail.
    pub(crate) fn remove_held_attachment(&self, mid: &str, id: &str) -> Result<String> {
        self.with(|c| {
            let tx=c.transaction()?;
            let row:Option<(String,String)>=tx.query_row("SELECT peer,body FROM messages WHERE id=? AND outgoing=1 AND state='failed'",[mid],|r|Ok((r.get(0)?,r.get(1)?))).optional()?;
            let Some((peer,body))=row else{return Ok(None)};
            let n=tx.execute("DELETE FROM attachments WHERE local_id=? AND message_id=? AND NOT EXISTS(SELECT 1 FROM download_jobs WHERE local_id=?)",params![id,mid,id])?;
            if n==0{return Ok(None)}
            let left:i64=tx.query_row("SELECT count(*) FROM attachments WHERE message_id=?",[mid],|r|r.get(0))?;
            if left==0 && body.trim().is_empty(){return Ok(None)}
            tx.execute("UPDATE messages SET state='queued',error=NULL WHERE id=? AND NOT EXISTS(SELECT 1 FROM attachments WHERE message_id=? AND state IN ('failed','cancelled'))",params![mid,mid])?;
            tx.execute("DELETE FROM transfer_send_state WHERE message_id=?",[mid])?;
            tx.commit()?;Ok(Some(peer))
        })?.ok_or_else(||Error::Invalid("remove a file only from a held message; delete the message if it would be empty".into()))
    }
}
