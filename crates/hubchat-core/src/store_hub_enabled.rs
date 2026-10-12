//! Device-local saved-hub policy, kept apart from shared roster/history.
use super::*;

pub(super) fn disabled_key(url: &str) -> String {
    format!("hub.disabled.{url}")
}

impl Store {
    pub fn hub_enabled(&self, url: &str) -> Result<bool> {
        self.with(|c| {
            let exists: bool = c.query_row("SELECT EXISTS(SELECT 1 FROM hubs WHERE url=?)", [url], |r| r.get(0))?;
            if !exists { return Ok(false); }
            let disabled: Option<String> = c.query_row("SELECT value FROM meta WHERE key=?", [disabled_key(url)], |r| r.get(0)).optional()?;
            Ok(disabled.as_deref() != Some("yes"))
        })
    }

    pub fn set_hub_enabled(&self, url: &str, enabled: bool) -> Result<()> {
        self.set_hub_enabled_for(None, url, enabled)
    }

    pub(crate) fn set_hub_enabled_for(&self, owner: Option<&str>, url: &str, enabled: bool) -> Result<()> {
        self.with(|c| {
            let tx = c.transaction()?;
            if let Some(owner) = owner {
                let current: Option<String> = tx.query_row("SELECT value FROM meta WHERE key='identity.address'", [], |r| r.get(0)).optional()?;
                if current.as_deref() != Some(owner) { return Err(rusqlite::Error::QueryReturnedNoRows); }
            }
            let exists: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM hubs WHERE url=?)", [url], |r| r.get(0))?;
            if !exists { return Err(rusqlite::Error::QueryReturnedNoRows); }
            if enabled {
                tx.execute("DELETE FROM meta WHERE key=?", [disabled_key(url)])?;
            } else {
                tx.execute("INSERT INTO meta(key,value) VALUES(?,'yes') ON CONFLICT(key) DO UPDATE SET value='yes'", [disabled_key(url)])?;
            }
            tx.commit()
        })
    }
}
