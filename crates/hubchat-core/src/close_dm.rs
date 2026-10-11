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
