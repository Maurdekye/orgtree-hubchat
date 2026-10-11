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
impl Store {
    pub fn transfer_page(&self, after: &str) -> Result<TransferPage> {
        self.with(|c| {
            let mut q = c.prepare(
                "SELECT a.local_id,a.message_id,m.peer,a.name,
                j.local_id IS NULL,COALESCE(j.state,a.state),COALESCE(j.confirmed,0),a.bytes,
                COALESCE(j.error,a.error,m.error),m.created_at
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
}
