//! Bounded metadata-only pages of unfinished transfers for UI reconstruction.
use super::*;
#[derive(Debug, Clone, Serialize)]
pub struct TransferView {
    pub local_id: String,
    pub message_id: String,
    pub created_at: String,
    pub peer: String,
    pub name: String,
    pub upload: bool,
    pub state: String,
    pub done: u64,
    pub total: u64,
    pub error: Option<String>,
}
#[derive(Debug, Serialize)]
pub struct TransferPage {
    pub rows: Vec<TransferView>,
    pub next: Option<String>,
}
/// History never returns message bodies, paths, URIs or checkpoint validators.
#[derive(Debug, Serialize)]
pub struct TransferHistoryView {
    pub local_id: String,
    pub message_id: String,
    pub created_at: String,
    pub peer: String,
    pub name: String,
    pub upload: bool,
    pub state: String,
    pub total: u64,
    pub error: Option<String>,
    pub available: bool,
    pub can_redownload: bool,
    #[serde(skip)]
    pub(crate) source: Option<String>,
}
#[derive(Debug, Serialize)]
pub struct TransferHistoryPage {
    pub rows: Vec<TransferHistoryView>,
    pub next: Option<String>,
}
impl Store {
    pub(crate) fn transfer_history_page(&self, after: &str) -> Result<TransferHistoryPage> {
        // Attachment IDs are generated ASCII UUIDs. Use one indexed range for
        // both the initial and later pages instead of an OR that rescans IDs.
        let after=if after.is_empty() {"\u{10ffff}"} else {after};
        self.with(|c| {
            let mut q = c.prepare("WITH ids AS (
                SELECT local_id FROM (SELECT local_id FROM download_jobs INDEXED BY download_history
                  WHERE local_id<?1 AND (state IN ('done','failed','expired','cancelled') OR (state IN ('publishing','exporting') AND error IS NOT NULL)) ORDER BY local_id DESC LIMIT 101)
                UNION
                SELECT local_id FROM (SELECT a.local_id FROM attachments a INDEXED BY upload_history
                  WHERE a.local_id<?1 AND a.source IS NOT NULL AND a.state IN ('uploaded','failed','cancelled')
                  AND NOT EXISTS(SELECT 1 FROM download_jobs j WHERE j.local_id=a.local_id) ORDER BY a.local_id DESC LIMIT 101))
                SELECT a.local_id,a.message_id,m.created_at,m.peer,a.name,m.outgoing,COALESCE(j.state,a.state),a.bytes,COALESCE(j.error,a.error),CASE WHEN m.outgoing=1 THEN COALESCE(a.source,a.local_path) ELSE a.local_path END,NOT m.outgoing AND a.hub_id IS NOT NULL AND COALESCE(a.hub,m.hub) IS NOT NULL AND COALESCE(j.state,a.state)!='expired'
                FROM ids JOIN attachments a ON a.local_id=ids.local_id JOIN messages m ON m.id=a.message_id LEFT JOIN download_jobs j ON j.local_id=a.local_id
                ORDER BY a.local_id DESC LIMIT 101")?;
            let mut rows = q.query_map([after], |r| Ok(TransferHistoryView {
                local_id:r.get(0)?,message_id:r.get(1)?,created_at:r.get(2)?,peer:r.get(3)?,name:r.get(4)?,upload:r.get(5)?,state:r.get(6)?,total:r.get(7)?,error:r.get(8)?,source:r.get(9)?,can_redownload:r.get(10)?,available:false,
            }))?.collect::<rusqlite::Result<Vec<_>>>()?;
            let next=if rows.len()>100 {rows.pop();rows.last().map(|r| r.local_id.clone())} else {None};
            Ok(TransferHistoryPage {rows,next})
        })
    }
    pub fn transfer_page(&self, after: &str) -> Result<TransferPage> {
        self.with(|c| {
            let mut q = c.prepare(
                "SELECT a.local_id,a.message_id,m.peer,a.name,
                j.local_id IS NULL,COALESCE(j.state,a.state),COALESCE(j.confirmed,a.upload_confirmed),a.bytes,
                COALESCE(j.error,CASE WHEN j.restarted=1 THEN 'Starting over: the earlier progress was more than a day old.' END,a.error,m.error),m.created_at
                FROM attachments a JOIN messages m ON m.id=a.message_id
                LEFT JOIN download_jobs j ON j.local_id=a.local_id
                WHERE a.local_id>?1 AND (
                    (j.state IN ('queued','running','paused','publishing','exporting')
                       AND NOT (j.state IN ('publishing','exporting') AND j.error IS NOT NULL))
                    OR (j.local_id IS NULL AND m.outgoing=1 AND m.state IN ('queued','sending')
                        AND a.state IN ('pending','uploading')))
                ORDER BY a.local_id LIMIT 101",
            )?;
            let mut rows = q
                .query_map([after], |r| {
                    Ok(TransferView {
                        local_id: r.get(0)?,
                        message_id: r.get(1)?,
                        created_at: r.get(9)?,
                        peer: r.get(2)?,
                        name: r.get(3)?,
                        upload: r.get(4)?,
                        state: r.get(5)?,
                        done: r.get(6)?,
                        total: r.get(7)?,
                        error: r.get(8)?,
                    })
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            let next = if rows.len() > 100 {
                rows.pop();
                rows.last().map(|r| r.local_id.clone())
            } else {
                None
            };
            Ok(TransferPage { rows, next })
        })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn transfer_history_pages_are_bounded_private_and_exclude_unfinished_work() {
        let store=Store::open_in_memory().unwrap();
        for i in 0..205 {
            store.queue_outgoing(&NewOutgoing {id:format!("history{i}"),peer:"peer".into(),body:"SECRET-BODY".into(),kind:None,reply_to:None,attachments:vec![NewAttachment {name:format!("file{i}"),bytes:3,source:"SECRET-SOURCE".into()}]},"now").unwrap();
            let a=store.message(&format!("history{i}")).unwrap().unwrap().attachments.remove(0);
            store.set_upload(&a.local_id,"uploaded","remote","hub").unwrap();
        }
        let id=store.message("history0").unwrap().unwrap().attachments[0].local_id.clone();
        store.queue_download_job(&id,"SECRET-PART","SECRET-DEST").unwrap();
        let mut after=String::new();
        let mut ids=std::collections::HashSet::new();
        loop {
            let page=store.transfer_history_page(&after).unwrap();
            assert!(page.rows.len()<=100);
            let json=serde_json::to_string(&page).unwrap();
            assert!(!json.contains("SECRET"));
            for row in page.rows {assert!(ids.insert(row.local_id));assert!(row.upload);assert!(!row.can_redownload);}
            match page.next {Some(next)=>after=next,None=>break}
        }
        assert_eq!(ids.len(),204);
        assert!(!ids.contains(&id));
        store.delete_message("history1").unwrap();
        assert_eq!(store.transfer_history_page("").unwrap().rows.len(),100);
    }
    #[test]
    fn pages_recover_unfinished_transfers_without_bodies_or_private_paths() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("store.db");
        let store = Store::open(&db).unwrap();
        for i in 0..205 {
            store
                .queue_outgoing(
                    &NewOutgoing {
                        id: format!("m{i}"),
                        peer: "peer".into(),
                        body: "private body".into(),
                        kind: None,
                        reply_to: None,
                        attachments: vec![NewAttachment {
                            name: format!("file{i}"),
                            bytes: 9000,
                            source: "private-source".into(),
                        }],
                    },
                    "now",
                )
                .unwrap();
        }
        let id = store.message("m0").unwrap().unwrap().attachments[0]
            .local_id
            .clone();
        store
            .queue_download_job(&id, "private.part", "private.dest")
            .unwrap();
        let job = store
            .begin_download(&id, "private.part", "private.dest")
            .unwrap();
        store
            .download_checkpoint(
                &id,
                job.attempt,
                &crate::download::DownloadCheckpoint {
                    confirmed: 8000,
                    validator: Some("secret-validator".into()),
                    updated_ms: 1,
                },
            )
            .unwrap();
        drop(store);
        let store = Store::open(&db).unwrap();
        let mut after = String::new();
        let mut ids = std::collections::HashSet::new();
        loop {
            let page = store.transfer_page(&after).unwrap();
            assert!(page.rows.len() <= 100);
            let json = serde_json::to_string(&page).unwrap();
            assert!(!json.contains("private"));
            assert!(!json.contains("validator"));
            for row in &page.rows {
                assert!(ids.insert(row.local_id.clone()));
                assert_eq!(row.created_at, "now");
                if row.local_id == id {
                    assert!(!row.upload);
                    assert_eq!(row.done, 8000);
                }
            }
            match page.next {
                Some(next) => after = next,
                None => break,
            }
        }
        assert_eq!(ids.len(), 205);
        store.cancel_download(&id).unwrap();
        let mut after = String::new();
        let mut count = 0;
        loop {
            let page = store.transfer_page(&after).unwrap();
            assert!(!page.rows.iter().any(|r| r.local_id == id));
            count += page.rows.len();
            match page.next {
                Some(next) => after = next,
                None => break,
            }
        }
        assert_eq!(count, 204);
    }
    #[test]
    fn upload_confirmation_survives_reopen_and_is_bound_to_session_and_state() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("db");
        let store = Store::open(&db).unwrap();
        store
            .queue_outgoing(
                &NewOutgoing {
                    id: "message".into(),
                    peer: "peer".into(),
                    body: "".into(),
                    kind: None,
                    reply_to: None,
                    attachments: vec![NewAttachment {
                        name: "file".into(),
                        bytes: 9000,
                        source: "source".into(),
                    }],
                },
                "now",
            )
            .unwrap();
        let id = store.message("message").unwrap().unwrap().attachments[0]
            .local_id
            .clone();
        store
            .set_upload(&id, "uploading", "session1", "hub1")
            .unwrap();
        store
            .upload_checkpoint(&id, "hub1", "session1", 8000)
            .unwrap();
        drop(store);
        let store = Store::open(&db).unwrap();
        assert_eq!(store.transfer_page("").unwrap().rows[0].done, 8000);
        assert_eq!(
            store.message("message").unwrap().unwrap().attachments[0].upload_confirmed,
            8000
        );
        assert!(store
            .upload_checkpoint(&id, "hub2", "session1", 9000)
            .is_err());
        assert!(store.upload_checkpoint(&id, "hub1", "wrong", 9000).is_err());
        assert!(store
            .upload_checkpoint(&id, "hub1", "session1", 9001)
            .is_err());
        store
            .set_attachment(&id, "cancelled", None, None, None)
            .unwrap();
        assert!(store
            .upload_checkpoint(&id, "hub1", "session1", 9000)
            .is_err());
        store.set_attachment(&id,"pending",None,None,None).unwrap();
        store
            .set_upload(&id, "uploading", "session2", "hub1")
            .unwrap();
        assert_eq!(store.transfer_page("").unwrap().rows[0].done, 0);
        store
            .upload_checkpoint(&id, "hub1", "session2", 4000)
            .unwrap();
        // The hub can rewind after checksum failure: its validated status wins.
        store.upload_checkpoint(&id, "hub1", "session2", 0).unwrap();
        store
            .set_upload(&id, "uploaded", "session2", "hub1")
            .unwrap();
        assert_eq!(
            store.message("message").unwrap().unwrap().attachments[0].upload_confirmed,
            9000
        );
    }
}
