//! Device-local DM visibility. Never modifies messages, custody or sync cursors.
use super::Store;
use crate::Result;

pub(super) fn reopen(tx: &rusqlite::Transaction<'_>, peer: &str) -> rusqlite::Result<()> {
    tx.execute("DELETE FROM closed_dms WHERE peer=?", [peer])?;
    Ok(())
}

impl Store {
    pub fn close_dm(&self, peer: &str) -> Result<()> {
        self.with(|c| c.execute("INSERT OR IGNORE INTO closed_dms(peer) VALUES(?)", [peer]).map(|_| ()))
    }

    pub fn open_dm(&self, peer: &str) -> Result<bool> {
        self.with(|c| c.execute("DELETE FROM closed_dms WHERE peer=?", [peer]).map(|n| n > 0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hub::Envelope;
    use crate::hub_v2::SyncedMessage;
    const PEER: &str = "peer.111111";
    const HUB: &str = "http://scratch.invalid:7370";
    const ME: &str = "me.000000";

    fn env(id: &str) -> Envelope {
        serde_json::from_value(serde_json::json!({
            "id": id, "from": PEER, "to": ME, "body": "history kept",
            "received_at": "2026-10-11T10:00:00.000Z",
            "attachments": [{"id": "file1", "name": "kept.txt", "bytes": 42}]
        })).unwrap()
    }
    fn synced(id: &str) -> SyncedMessage {
        serde_json::from_value(serde_json::json!({
            "id": id, "from": PEER, "to": ME, "body": "history kept",
            "received_at": "2026-10-11T10:00:00.000Z", "state": "received",
            "attachments": [{"id": "file1", "name": "kept.txt", "bytes": 42}]
        })).unwrap()
    }
    fn setup(s: &Store) {
        s.add_hub(HUB, "t0").unwrap();
        s.insert_incoming(HUB, &env("old"), "t1").unwrap();
    }

    #[test]
    fn close_dm_preserves_history_files_drafts_unread_and_custody() {
        let s = Store::open_in_memory().unwrap();
        setup(&s);
        for i in 0..75 { s.upsert_synced(HUB, ME, &synced(&format!("history-{i:03}"))).unwrap(); }
        s.set_draft(PEER, "unfinished").unwrap();
        s.set_send_hub(PEER, Some(HUB)).unwrap();
        s.set_meta("sync.cursor", "keep").unwrap();
        s.set_history_mark(HUB, PEER, &super::super::HistoryMark { before: Some("cursor".into()), old_unread: 3, ..Default::default() }).unwrap();
        let before = serde_json::to_value(s.chat(PEER, None, None, 100).unwrap()).unwrap();
        assert_eq!(before.as_array().unwrap().len(), 76, "history extends beyond a UI page");
        s.close_dm(PEER).unwrap();
        s.close_dm(PEER).unwrap();
        assert!(s.chats().unwrap().is_empty());
        assert_eq!(serde_json::to_value(s.chat(PEER, None, None, 100).unwrap()).unwrap(), before);
        assert_eq!(s.unread(PEER).unwrap(), 79);
        assert_eq!(s.draft(PEER).unwrap().as_deref(), Some("unfinished"));
        assert_eq!(s.message_hubs("old").unwrap(), [HUB]);
        assert_eq!(s.send_hub(PEER).unwrap().as_deref(), Some(HUB));
        assert_eq!(s.meta("sync.cursor").unwrap().as_deref(), Some("keep"));
        assert_eq!(s.history_mark(HUB, PEER).unwrap().unwrap().before.as_deref(), Some("cursor"));
        assert!(s.pending_deletes(HUB).unwrap().is_empty());
        assert!(s.chat_deletes(HUB).unwrap().is_empty());
        assert!(s.open_dm(PEER).unwrap());
        assert!(!s.open_dm(PEER).unwrap());
        assert_eq!(s.chats().unwrap().len(), 1);
        assert_eq!(serde_json::to_value(s.chat(PEER, None, None, 100).unwrap()).unwrap(), before);
    }

    #[test]
    fn close_dm_only_new_live_incoming_reopens() {
        let s = Store::open_in_memory().unwrap();
        setup(&s);
        s.close_dm(PEER).unwrap();
        assert!(!s.insert_incoming(HUB, &env("old"), "t2").unwrap());
        assert!(s.chats().unwrap().is_empty(), "duplicate live delivery must stay closed");
        s.upsert_synced_at(HUB, ME, &synced("history"), 0).unwrap();
        assert!(s.chats().unwrap().is_empty(), "older history never reopens");
        s.upsert_live_synced_at(HUB, ME, &synced("old"), 0).unwrap();
        assert!(s.chats().unwrap().is_empty(), "receipt updates never reopen");
        let mut outgoing = synced("other-device-outgoing");
        outgoing.env.from = ME.into(); outgoing.env.to = PEER.into();
        s.upsert_live_synced_at(HUB, ME, &outgoing, 0).unwrap();
        assert!(s.chats().unwrap().is_empty(), "another device's sends don't reopen");
        assert!(s.insert_incoming(HUB, &env("live-v1"), "t3").unwrap());
        assert_eq!(s.chats().unwrap().len(), 1);
        s.close_dm(PEER).unwrap();
        let mut read_elsewhere = synced("live-v2");
        read_elsewhere.read_at = Some("2026-10-11T10:00:01.000Z".into());
        s.upsert_live_synced_at(HUB, ME, &read_elsewhere, 0).unwrap();
        assert_eq!(s.chats().unwrap().len(), 1, "new incoming already read elsewhere still reopens");
        assert_eq!(s.chat(PEER, None, None, 100).unwrap().len(), 5);
    }

    #[test]
    fn close_dm_survives_restart_is_device_local_and_wipe_clears_it() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("scratch.db");
        let s = Store::open(&path).unwrap(); setup(&s); s.close_dm(PEER).unwrap(); drop(s);
        let s = Store::open(&path).unwrap();
        assert!(s.chats().unwrap().is_empty());
        assert_eq!(s.chat(PEER, None, None, 100).unwrap().len(), 1);
        let other_device = Store::open_in_memory().unwrap(); setup(&other_device);
        assert_eq!(other_device.chats().unwrap().len(), 1);
        s.wipe().unwrap(); s.add_hub(HUB, "t0").unwrap();
        // History loading after an identity switch must not inherit the old closure.
        s.upsert_synced(HUB, ME, &synced("after-wipe")).unwrap();
        assert_eq!(s.chats().unwrap().len(), 1);
    }

    #[test]
    fn close_dm_migration_preserves_existing_version14_history() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("scratch.db");
        let s = Store::open(&path).unwrap(); setup(&s); drop(s);
        let c = rusqlite::Connection::open(&path).unwrap();
        c.execute_batch("DROP TABLE closed_dms; PRAGMA user_version=14;").unwrap(); drop(c);
        let s = Store::open(&path).unwrap();
        assert_eq!(s.chats().unwrap().len(), 1);
        s.close_dm(PEER).unwrap(); drop(s);
        let s = Store::open(&path).unwrap();
        assert!(s.chats().unwrap().is_empty()); s.open_dm(PEER).unwrap();
        assert_eq!(s.message("old").unwrap().unwrap().attachments[0].name, "kept.txt");
    }

    #[test]
    fn close_dm_incoming_failure_rolls_back_reopen() {
        let s = Store::open_in_memory().unwrap(); setup(&s); s.close_dm(PEER).unwrap();
        s.with(|c| c.execute_batch("CREATE TRIGGER scratch_fail BEFORE INSERT ON attachments BEGIN SELECT RAISE(ABORT,'scratch attachment failure'); END;")).unwrap();
        assert!(s.insert_incoming(HUB, &env("failed-v1"), "t2").is_err());
        assert!(s.upsert_live_synced_at(HUB, ME, &synced("failed-v2"), 0).is_err());
        assert!(s.chats().unwrap().is_empty(), "failed admission must not reopen");
        assert!(s.message("failed-v1").unwrap().is_none());
        assert!(s.message("failed-v2").unwrap().is_none());
        assert!(s.message("old").unwrap().is_some());
    }

    #[test]
    fn close_dm_does_not_hide_unrelated_peers() {
        let s = Store::open_in_memory().unwrap(); setup(&s);
        let mut other = env("unrelated"); other.from = "another.222222".into();
        s.insert_incoming(HUB, &other, "t2").unwrap();
        s.close_dm(PEER).unwrap();
        assert_eq!(s.chats().unwrap()[0].peer, other.from);
        s.close_dm(&other.from).unwrap(); s.open_dm(PEER).unwrap();
        assert_eq!(s.chats().unwrap()[0].peer, PEER);
    }
}
