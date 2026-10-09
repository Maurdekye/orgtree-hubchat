//! The local message store (SQLite). Everything the UI shows comes from here;
//! the sync engine writes hub events into it. One connection behind a mutex:
//! a chat app's write rate is tiny and every statement is short.

use std::path::Path;
use std::sync::Mutex;

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::hub::{AttachmentMeta, Envelope, Receipt, RosterEntry};
use crate::hub_v2::SyncedMessage;
use crate::{Error, Result};

const SCHEMA: &str = r#"
PRAGMA journal_mode = WAL;
PRAGMA foreign_keys = ON;
CREATE TABLE IF NOT EXISTS meta (
  key   TEXT PRIMARY KEY,
  value TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS hubs (
  url                  TEXT PRIMARY KEY,
  name                 TEXT NOT NULL DEFAULT '',
  added_at             TEXT NOT NULL,
  max_attachment_bytes INTEGER,
  last_ok_at           TEXT
);
-- one row per (address, hub): the merged directory groups by address
CREATE TABLE IF NOT EXISTS roster (
  address   TEXT NOT NULL,
  hub       TEXT NOT NULL REFERENCES hubs(url) ON DELETE CASCADE,
  kind      TEXT NOT NULL DEFAULT '',
  org_name  TEXT NOT NULL DEFAULT '',
  username  TEXT NOT NULL DEFAULT '',
  blurb     TEXT NOT NULL DEFAULT '',
  online    INTEGER NOT NULL DEFAULT 0,
  last_seen TEXT,
  PRIMARY KEY (address, hub)
);
CREATE TABLE IF NOT EXISTS messages (
  id           TEXT PRIMARY KEY,           -- client-minted (ours) or the sender's id
  peer         TEXT NOT NULL,              -- the other side's address: the chat key
  outgoing     INTEGER NOT NULL,
  hub          TEXT,                       -- hub it travelled through (NULL = not yet routed)
  body         TEXT NOT NULL,
  kind         TEXT,
  reply_to     TEXT,
  sent_at      TEXT,                       -- sender's clock
  received_at  TEXT,                       -- hub's clock: ordering authority once known
  created_at   TEXT NOT NULL,              -- local clock: ordering before the hub has it
  -- outgoing: queued | sending | sent | fetched | delivered | read | failed
  -- incoming: received
  state        TEXT NOT NULL,
  fetched_at   TEXT,
  delivered_at TEXT,
  read_at      TEXT,
  error        TEXT,
  seen         INTEGER NOT NULL DEFAULT 0  -- incoming: shown to the user
);
CREATE INDEX IF NOT EXISTS messages_by_peer ON messages(peer, created_at);
CREATE INDEX IF NOT EXISTS messages_queued ON messages(state) WHERE state IN ('queued','sending');
CREATE TABLE IF NOT EXISTS attachments (
  local_id     TEXT PRIMARY KEY,
  message_id   TEXT NOT NULL REFERENCES messages(id) ON DELETE CASCADE,
  position     INTEGER NOT NULL,
  hub_id       TEXT,                       -- the hub's attachment id once uploaded
  name         TEXT NOT NULL,
  bytes        INTEGER NOT NULL,
  source       TEXT,                       -- outgoing: path or content:// URI to upload
  local_path   TEXT,                       -- downloaded copy
  -- pending | uploading | uploaded | downloading | done | failed | cancelled | expired
  state        TEXT NOT NULL,
  error        TEXT
);
CREATE TABLE IF NOT EXISTS drafts (
  peer TEXT PRIMARY KEY,
  body TEXT NOT NULL
);
-- every hub known to hold a copy of a message (B2, B3): a message sent
-- again through another hub, or synced from two, is on more than one
CREATE TABLE IF NOT EXISTS message_hubs (
  id  TEXT NOT NULL REFERENCES messages(id) ON DELETE CASCADE,
  hub TEXT NOT NULL,
  PRIMARY KEY (id, hub)
);
CREATE INDEX IF NOT EXISTS message_hubs_by_hub ON message_hubs(hub);
-- deletes a hub still owes us because it was down (B2): kind 'message'
-- (target = a message id) or 'chat' (target = the peer's address)
CREATE TABLE IF NOT EXISTS pending_deletes (
  hub    TEXT NOT NULL REFERENCES hubs(url) ON DELETE CASCADE,
  kind   TEXT NOT NULL,
  target TEXT NOT NULL,
  PRIMARY KEY (hub, kind, target)
);
-- the hub a chat is pinned to (the hub picker); no row = Automatic.
-- Per device, never synced (hubchat-opus 2026-10-09 09:36Z).
CREATE TABLE IF NOT EXISTS chat_hub (
  peer TEXT PRIMARY KEY,
  hub  TEXT NOT NULL REFERENCES hubs(url) ON DELETE CASCADE
);
"#;

pub struct Store {
    con: Mutex<Connection>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Hub {
    pub url: String,
    pub name: String,
    pub max_attachment_bytes: Option<u64>,
    pub last_ok_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Attachment {
    pub local_id: String,
    pub hub_id: Option<String>,
    pub name: String,
    pub bytes: u64,
    pub source: Option<String>,
    pub local_path: Option<String>,
    pub state: String,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Message {
    pub id: String,
    pub peer: String,
    pub outgoing: bool,
    pub hub: Option<String>,
    pub body: String,
    pub kind: Option<String>,
    pub reply_to: Option<String>,
    pub sent_at: Option<String>,
    pub received_at: Option<String>,
    pub created_at: String,
    pub state: String,
    pub fetched_at: Option<String>,
    pub delivered_at: Option<String>,
    pub read_at: Option<String>,
    pub error: Option<String>,
    pub seen: bool,
    pub attachments: Vec<Attachment>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ChatSummary {
    pub peer: String,
    pub last: Message,
    pub unread: u64,
}

/// A directory row: one address, merged across hubs.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Contact {
    pub address: String,
    pub kind: String,
    pub org_name: String,
    pub username: String,
    pub blurb: String,
    pub online: bool,
    pub last_seen: Option<String>,
    pub hubs: Vec<String>,
}

fn db(e: rusqlite::Error) -> Error {
    Error::Store(e.to_string())
}

impl Store {
    pub fn open(path: &Path) -> Result<Self> {
        let con = Connection::open(path).map_err(db)?;
        Self::init(con)
    }

    pub fn open_in_memory() -> Result<Self> {
        Self::init(Connection::open_in_memory().map_err(db)?)
    }

    fn init(con: Connection) -> Result<Self> {
        con.busy_timeout(std::time::Duration::from_secs(5))
            .map_err(db)?;
        con.execute_batch(SCHEMA).map_err(db)?;
        // once, for a store from before message_hubs: its one hub label
        con.execute_batch(
            "BEGIN;
             INSERT OR IGNORE INTO message_hubs(id, hub)
               SELECT id, hub FROM messages
               WHERE hub IS NOT NULL AND NOT EXISTS (SELECT 1 FROM meta WHERE key='schema.message_hubs');
             INSERT OR IGNORE INTO meta(key, value) VALUES('schema.message_hubs', '1');
             COMMIT;",
        )
        .map_err(db)?;
        Ok(Self {
            con: Mutex::new(con),
        })
    }

    fn with<T>(&self, f: impl FnOnce(&mut Connection) -> rusqlite::Result<T>) -> Result<T> {
        let mut con = self.con.lock().unwrap_or_else(|p| p.into_inner());
        f(&mut con).map_err(db)
    }

    // ------------------------------------------------------------- meta

    pub fn meta(&self, key: &str) -> Result<Option<String>> {
        self.with(|c| {
            c.query_row("SELECT value FROM meta WHERE key=?", [key], |r| r.get(0))
                .optional()
        })
    }

    /// The name a contact goes by on any of our hubs (None: no name known).
    pub fn display_name(&self, address: &str) -> Result<Option<String>> {
        self.with(|c| {
            c.query_row(
                "SELECT org_name FROM roster WHERE address=? AND org_name <> '' ORDER BY hub LIMIT 1",
                [address],
                |r| r.get(0),
            )
            .optional()
        })
    }

    pub fn set_meta(&self, key: &str, value: &str) -> Result<()> {
        self.with(|c| {
            c.execute("INSERT INTO meta(key,value) VALUES(?,?) ON CONFLICT(key) DO UPDATE SET value=excluded.value", [key, value])
                .map(|_| ())
        })
    }

    // ------------------------------------------------------------- hubs

    pub fn add_hub(&self, url: &str, now: &str) -> Result<()> {
        self.with(|c| {
            c.execute(
                "INSERT OR IGNORE INTO hubs(url, added_at) VALUES(?,?)",
                [url, now],
            )
            .map(|_| ())
        })
    }

    pub fn remove_hub(&self, url: &str) -> Result<()> {
        self.with(|c| {
            let tx = c.transaction()?;
            tx.execute("DELETE FROM message_hubs WHERE hub=?", [url])?;
            tx.execute("DELETE FROM hubs WHERE url=?", [url])?;
            tx.commit()
        })
    }

    pub fn hub_ok(&self, url: &str, name: &str, max: Option<u64>, now: &str) -> Result<()> {
        self.with(|c| {
            c.execute(
                "UPDATE hubs SET name=?, max_attachment_bytes=COALESCE(?, max_attachment_bytes), last_ok_at=? WHERE url=?",
                params![name, max, now, url],
            )
            .map(|_| ())
        })
    }

    pub fn hubs(&self) -> Result<Vec<Hub>> {
        self.with(|c| {
            let mut st = c.prepare(
                "SELECT url, name, max_attachment_bytes, last_ok_at FROM hubs ORDER BY added_at",
            )?;
            let rows = st.query_map([], |r| {
                Ok(Hub {
                    url: r.get(0)?,
                    name: r.get(1)?,
                    max_attachment_bytes: r.get(2)?,
                    last_ok_at: r.get(3)?,
                })
            })?;
            rows.collect()
        })
    }

    // ----------------------------------------------------------- roster

    /// Replace one hub's roster with what its poll just returned.
    pub fn set_roster(&self, hub: &str, entries: &[RosterEntry]) -> Result<()> {
        self.with(|c| {
            let tx = c.transaction()?;
            tx.execute("DELETE FROM roster WHERE hub=?", [hub])?;
            {
                let mut st = tx.prepare(
                    "INSERT INTO roster(address, hub, kind, org_name, username, blurb, online, last_seen) VALUES(?,?,?,?,?,?,?,?)",
                )?;
                for e in entries {
                    st.execute(params![e.slug, hub, e.kind, e.org_name, e.username, e.blurb, e.online, e.last_seen])?;
                }
            }
            tx.commit()
        })
    }

    /// The merged directory: one row per address, online if online on any hub.
    pub fn directory(&self) -> Result<Vec<Contact>> {
        self.with(|c| {
            let mut st = c.prepare(
                "SELECT address, kind, org_name, username, blurb, online, last_seen, hub FROM roster ORDER BY address, hub",
            )?;
            let mut out: Vec<Contact> = Vec::new();
            let rows = st.query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, String>(4)?,
                    r.get::<_, bool>(5)?,
                    r.get::<_, Option<String>>(6)?,
                    r.get::<_, String>(7)?,
                ))
            })?;
            for row in rows {
                let (address, kind, org_name, username, blurb, online, last_seen, hub) = row?;
                match out.last_mut() {
                    Some(c) if c.address == address => {
                        c.online |= online;
                        if last_seen > c.last_seen {
                            c.last_seen = last_seen;
                        }
                        c.hubs.push(hub);
                    }
                    _ => out.push(Contact { address, kind, org_name, username, blurb, online, last_seen, hubs: vec![hub] }),
                }
            }
            Ok(out)
        })
    }

    /// Hubs whose roster lists `address`, with whether it is online there:
    /// online ones first, then by URL.
    pub fn hubs_reaching(&self, address: &str) -> Result<Vec<(String, bool)>> {
        self.with(|c| {
            let mut st = c.prepare(
                "SELECT hub, online FROM roster WHERE address=? ORDER BY online DESC, hub",
            )?;
            let rows = st.query_map([address], |r| Ok((r.get(0)?, r.get(1)?)))?;
            rows.collect()
        })
    }

    /// The hub our newest sent message in this chat went through.
    pub fn last_send_hub(&self, peer: &str) -> Result<Option<String>> {
        self.with(|c| {
            c.query_row(
                "SELECT hub FROM messages WHERE peer=? AND outgoing=1 AND hub IS NOT NULL
                 ORDER BY created_at DESC, id DESC LIMIT 1",
                [peer],
                |r| r.get(0),
            )
            .optional()
        })
    }

    /// The hub this chat is pinned to (None: Automatic).
    pub fn send_hub(&self, peer: &str) -> Result<Option<String>> {
        self.with(|c| {
            c.query_row("SELECT hub FROM chat_hub WHERE peer=?", [peer], |r| r.get(0))
                .optional()
        })
    }

    /// Pin a chat to a hub, or back to Automatic with None. Removing the hub
    /// drops the pin (the chat goes back to Automatic).
    pub fn set_send_hub(&self, peer: &str, hub: Option<&str>) -> Result<()> {
        self.with(|c| {
            match hub {
                Some(h) => c.execute(
                    "INSERT INTO chat_hub(peer, hub) VALUES(?,?) ON CONFLICT(peer) DO UPDATE SET hub=excluded.hub",
                    [peer, h],
                ),
                None => c.execute("DELETE FROM chat_hub WHERE peer=?", [peer]),
            }
            .map(|_| ())
        })
    }

    // --------------------------------------------------------- messages

    /// Queue an outgoing message (and its attachments) for the sender task.
    pub fn queue_outgoing(&self, msg: &NewOutgoing, now: &str) -> Result<()> {
        self.with(|c| {
            let tx = c.transaction()?;
            tx.execute(
                "INSERT INTO messages(id, peer, outgoing, body, kind, reply_to, sent_at, created_at, state)
                 VALUES(?,?,1,?,?,?,?,?,'queued')",
                params![msg.id, msg.peer, msg.body, msg.kind, msg.reply_to, now, now],
            )?;
            for (i, a) in msg.attachments.iter().enumerate() {
                tx.execute(
                    "INSERT INTO attachments(local_id, message_id, position, name, bytes, source, state)
                     VALUES(?,?,?,?,?,?,'pending')",
                    params![uuid::Uuid::new_v4().simple().to_string(), msg.id, i as i64, a.name, a.bytes, a.source],
                )?;
            }
            tx.commit()
        })
    }

    /// Store an incoming message. Returns false for a duplicate (the hub is
    /// at-least-once: a lost ack redelivers).
    pub fn insert_incoming(&self, hub: &str, env: &Envelope, now: &str) -> Result<bool> {
        let body = match env.reply_to {
            Some(_) => crate::engine::strip_quote(&env.body),
            None => env.body.as_str(),
        };
        self.with(|c| {
            let tx = c.transaction()?;
            let n = tx.execute(
                "INSERT OR IGNORE INTO messages(id, peer, outgoing, hub, body, kind, reply_to, sent_at, received_at, created_at, state)
                 VALUES(?,?,0,?,?,?,?,?,?,?,'received')",
                params![env.id, env.from, hub, body, env.kind, env.reply_to, env.sent_at, env.received_at, now],
            )?;
            if n > 0 {
                for (i, a) in env.attachments.iter().enumerate() {
                    insert_remote_attachment(&tx, &env.id, i, a)?;
                }
            }
            held_by(&tx, &env.id, hub)?;
            tx.commit()?;
            Ok(n > 0)
        })
    }

    pub fn set_state(&self, id: &str, state: &str, error: Option<&str>) -> Result<()> {
        self.with(|c| {
            c.execute(
                "UPDATE messages SET state=?, error=? WHERE id=?",
                params![state, error, id],
            )
            .map(|_| ())
        })
    }

    pub fn mark_sent(&self, id: &str, hub: &str, received_at: &str) -> Result<()> {
        self.with(|c| {
            let tx = c.transaction()?;
            let n = tx.execute(
                "UPDATE messages SET state=CASE WHEN state IN ('queued','sending','failed') THEN 'sent' ELSE state END,
                 hub=?, received_at=?, error=NULL WHERE id=?",
                params![hub, received_at, id],
            )?;
            if n > 0 {
                held_by(&tx, id, hub)?;
            }
            tx.commit()
        })
    }

    /// Apply a receipt from the hub. States only move forward.
    pub fn apply_receipt(&self, r: &Receipt) -> Result<()> {
        self.with(|c| {
            c.execute(
                "UPDATE messages SET
                   fetched_at   = COALESCE(?, fetched_at),
                   delivered_at = COALESCE(?, delivered_at),
                   read_at      = COALESCE(?, read_at),
                   state = CASE
                     WHEN ? = 'read' OR read_at IS NOT NULL THEN 'read'
                     WHEN ? = 'delivered' AND state NOT IN ('read') THEN 'delivered'
                     WHEN ? = 'fetched' AND state IN ('queued','sending','sent') THEN 'fetched'
                     ELSE state END
                 WHERE id=? AND outgoing=1",
                params![
                    r.fetched_at,
                    r.delivered_at,
                    r.read_at,
                    r.state,
                    r.state,
                    r.state,
                    r.id
                ],
            )
            .map(|_| ())
        })
    }

    pub fn queued(&self) -> Result<Vec<Message>> {
        self.messages_where(
            "m.state IN ('queued','sending') AND m.outgoing=1 ORDER BY m.created_at",
            params![],
        )
    }

    pub fn message(&self, id: &str) -> Result<Option<Message>> {
        Ok(self
            .messages_where("m.id=?", params![id])?
            .into_iter()
            .next())
    }

    /// One chat, oldest first, at most `limit` messages before `before` (created_at).
    /// A page of a chat, oldest first: the newest `limit` messages before
    /// `before` (a message's created_at and id), and none older than `from`
    /// (inclusive; a chat re-read keeps the range already shown). Ties on the
    /// time go by id, so a page never skips one.
    pub fn chat(
        &self,
        peer: &str,
        before: Option<(&str, &str)>,
        from: Option<(&str, &str)>,
        limit: u32,
    ) -> Result<Vec<Message>> {
        let (bt, bid) = before.unzip();
        let (ft, fid) = from.unzip();
        let mut v = self.messages_where(
            "m.peer=? AND (? IS NULL OR m.created_at < ? OR (m.created_at = ? AND m.id < ?))
               AND (? IS NULL OR m.created_at > ? OR (m.created_at = ? AND m.id >= ?))
             ORDER BY m.created_at DESC, m.id DESC LIMIT ?",
            params![peer, bt, bt, bt, bid, ft, ft, ft, fid, limit],
        )?;
        v.reverse();
        Ok(v)
    }

    /// Mark incoming messages in a chat as seen; returns the ids that changed
    /// (they owe the sender a read receipt).
    /// Mark a chat's incoming messages read, keeping when (Message info shows
    /// it; a v2 hub's read time replaces it when it syncs). Returns the
    /// newly read ones, for the receipts.
    pub fn mark_seen(&self, peer: &str, now: &str) -> Result<Vec<(String, Option<String>)>> {
        self.with(|c| {
            let tx = c.transaction()?;
            let ids: Vec<(String, Option<String>)> = {
                let mut st = tx.prepare(
                    "SELECT id, hub FROM messages WHERE peer=? AND outgoing=0 AND seen=0",
                )?;
                let rows = st.query_map([peer], |r| Ok((r.get(0)?, r.get(1)?)))?;
                rows.collect::<rusqlite::Result<_>>()?
            };
            tx.execute(
                "UPDATE messages SET seen=1, read_at=COALESCE(read_at, ?) WHERE peer=? AND outgoing=0 AND seen=0",
                [now, peer],
            )?;
            tx.commit()?;
            Ok(ids)
        })
    }

    /// How many of a chat's incoming messages are not read yet (on any of
    /// our devices: a v2 hub's read time marks them read here too).
    pub fn unread(&self, peer: &str) -> Result<u64> {
        self.with(|c| {
            c.query_row(
                "SELECT COUNT(*) FROM messages WHERE peer=? AND outgoing=0 AND seen=0",
                [peer],
                |r| r.get(0),
            )
        })
    }

    pub fn chats(&self) -> Result<Vec<ChatSummary>> {
        let peers: Vec<(String, String, u64)> = self.with(|c| {
            let mut st = c.prepare(
                "SELECT peer, MAX(created_at) AS last, SUM(CASE WHEN outgoing=0 AND seen=0 THEN 1 ELSE 0 END)
                 FROM messages GROUP BY peer ORDER BY last DESC",
            )?;
            let rows = st.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get::<_, i64>(2)? as u64)))?;
            rows.collect()
        })?;
        let mut out = Vec::with_capacity(peers.len());
        for (peer, _, unread) in peers {
            if let Some(last) = self.chat(&peer, None, None, 1)?.pop() {
                out.push(ChatSummary { peer, last, unread });
            }
        }
        Ok(out)
    }

    pub fn delete_message(&self, id: &str) -> Result<()> {
        self.with(|c| {
            c.execute("DELETE FROM messages WHERE id=?", [id])
                .map(|_| ())
        })
    }

    pub fn delete_chat(&self, peer: &str) -> Result<()> {
        self.with(|c| {
            c.execute("DELETE FROM messages WHERE peer=?", [peer])
                .map(|_| ())
        })
    }

    /// Forget everything (switching to another identity): messages, files'
    /// records, hubs, directory, drafts and settings.
    pub fn wipe(&self) -> Result<()> {
        self.with(|c| {
            c.execute_batch(
                "DELETE FROM attachments; DELETE FROM messages; DELETE FROM roster; DELETE FROM hubs;
                 DELETE FROM drafts; DELETE FROM meta;",
            )
        })
    }

    // ------------------------------------------------------------- sync (v2)

    /// Store a message as a v2 hub's sync reports it: ours (sent from any of
    /// our devices) or theirs, with its receipts as they stand now. Returns
    /// whether this is a new incoming message nobody has read yet.
    pub fn upsert_synced(&self, hub: &str, me: &str, m: &SyncedMessage) -> Result<bool> {
        let env = &m.env;
        let outgoing = env.from == me;
        let peer = if outgoing {
            env.to.as_str()
        } else {
            env.from.as_str()
        };
        let body = match env.reply_to {
            Some(_) => crate::engine::strip_quote(&env.body),
            None => env.body.as_str(),
        };
        let hub_state = if !outgoing {
            "received"
        } else if m.read_at.is_some() {
            "read"
        } else if m.delivered_at.is_some() {
            "delivered"
        } else if m.fetched_at.is_some() {
            "fetched"
        } else {
            "sent"
        };
        self.with(|c| {
            let tx = c.transaction()?;
            let existing: Option<String> =
                tx.query_row("SELECT state FROM messages WHERE id=?", [&env.id], |r| r.get(0)).optional()?;
            let fresh = existing.is_none();
            match existing {
                None => {
                    tx.execute(
                        "INSERT INTO messages(id, peer, outgoing, hub, body, kind, reply_to, sent_at, received_at,
                           created_at, state, fetched_at, delivered_at, read_at, seen)
                         VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)",
                        params![
                            env.id,
                            peer,
                            outgoing,
                            hub,
                            body,
                            env.kind,
                            env.reply_to,
                            env.sent_at,
                            env.received_at,
                            // history sorts by the hub's clock
                            env.received_at,
                            hub_state,
                            m.fetched_at,
                            m.delivered_at,
                            m.read_at,
                            !outgoing && m.read_at.is_some(),
                        ],
                    )?;
                    for (i, a) in env.attachments.iter().enumerate() {
                        insert_remote_attachment(&tx, &env.id, i, a)?;
                    }
                }
                Some(state) => {
                    let state = if rank(hub_state) > rank(&state) { hub_state.to_owned() } else { state };
                    // the label stays the hub it first came through (its
                    // attachments' ids are that hub's); message_hubs keeps
                    // every hub that holds it
                    tx.execute(
                        "UPDATE messages SET hub=COALESCE(hub, ?), received_at=COALESCE(received_at, ?),
                           fetched_at=COALESCE(?, fetched_at), delivered_at=COALESCE(?, delivered_at),
                           read_at=COALESCE(?, read_at), state=?, error=CASE WHEN ?='failed' THEN error ELSE NULL END,
                           seen = seen OR ?
                         WHERE id=?",
                        params![
                            hub,
                            env.received_at,
                            m.fetched_at,
                            m.delivered_at,
                            m.read_at,
                            state,
                            state,
                            !outgoing && m.read_at.is_some(),
                            env.id
                        ],
                    )?;
                }
            }
            held_by(&tx, &env.id, hub)?;
            tx.commit()?;
            Ok(fresh && !outgoing && m.read_at.is_none())
        })
    }

    /// The hubs known to hold a copy of a message, by address.
    pub fn message_hubs(&self, id: &str) -> Result<Vec<String>> {
        self.with(|c| {
            let mut st = c.prepare("SELECT hub FROM message_hubs WHERE id=? ORDER BY hub")?;
            let rows = st.query_map([id], |r| r.get(0))?;
            rows.collect()
        })
    }

    /// A delete a hub that is down gets when it is back.
    pub fn queue_delete(&self, hub: &str, kind: &str, target: &str) -> Result<()> {
        self.with(|c| {
            c.execute(
                "INSERT OR IGNORE INTO pending_deletes(hub, kind, target) VALUES(?,?,?)",
                [hub, kind, target],
            )
            .map(|_| ())
        })
    }

    /// Deletes this hub still owes us: (kind, target), a bounded batch.
    pub fn pending_deletes(&self, hub: &str) -> Result<Vec<(String, String)>> {
        self.with(|c| {
            let mut st = c.prepare(
                "SELECT kind, target FROM pending_deletes WHERE hub=? ORDER BY kind, target LIMIT 200",
            )?;
            let rows = st.query_map([hub], |r| Ok((r.get(0)?, r.get(1)?)))?;
            rows.collect()
        })
    }

    pub fn delete_done(&self, hub: &str, kind: &str, target: &str) -> Result<()> {
        self.with(|c| {
            c.execute(
                "DELETE FROM pending_deletes WHERE hub=? AND kind=? AND target=?",
                [hub, kind, target],
            )
            .map(|_| ())
        })
    }

    /// Replace a message's body (a long one fetched whole).
    pub fn set_body(&self, id: &str, body: &str) -> Result<()> {
        self.with(|c| {
            c.execute("UPDATE messages SET body=? WHERE id=?", params![body, id])
                .map(|_| ())
        })
    }

    /// A v2 hub says its sync cursor is not ours any more: forget what came
    /// from it so the device rebuilds its copy. A message another hub still
    /// holds stays (B3); queued outgoing messages always stay.
    pub fn forget_hub_messages(&self, hub: &str) -> Result<()> {
        self.with(|c| {
            let tx = c.transaction()?;
            tx.execute(
                "DELETE FROM messages WHERE state<>'queued'
                   AND id IN (SELECT id FROM message_hubs WHERE hub=?1)
                   AND NOT EXISTS (SELECT 1 FROM message_hubs o WHERE o.id=messages.id AND o.hub<>?1)",
                [hub],
            )?;
            tx.execute("DELETE FROM message_hubs WHERE hub=?", [hub])?;
            // what stays is now about a hub that still has it
            tx.execute(
                "UPDATE messages SET hub=(SELECT o.hub FROM message_hubs o WHERE o.id=messages.id ORDER BY o.hub LIMIT 1)
                 WHERE hub=?1 AND EXISTS (SELECT 1 FROM message_hubs o WHERE o.id=messages.id)",
                [hub],
            )?;
            tx.commit()
        })
    }

    /// Roster entries that joined or changed (sync), merged into this hub's list.
    pub fn upsert_roster(&self, hub: &str, entries: &[RosterEntry]) -> Result<()> {
        self.with(|c| {
            let tx = c.transaction()?;
            {
                let mut st = tx.prepare(
                    "INSERT INTO roster(address, hub, kind, org_name, username, blurb, online, last_seen) VALUES(?,?,?,?,?,?,?,?)
                     ON CONFLICT(address, hub) DO UPDATE SET kind=excluded.kind, org_name=excluded.org_name,
                       username=excluded.username, blurb=excluded.blurb, online=excluded.online, last_seen=excluded.last_seen",
                )?;
                for e in entries {
                    st.execute(params![e.slug, hub, e.kind, e.org_name, e.username, e.blurb, e.online, e.last_seen])?;
                }
            }
            tx.commit()
        })
    }

    pub fn remove_roster(&self, hub: &str, addresses: &[String]) -> Result<()> {
        self.with(|c| {
            let tx = c.transaction()?;
            for a in addresses {
                tx.execute(
                    "DELETE FROM roster WHERE hub=? AND address=?",
                    params![hub, a],
                )?;
            }
            tx.commit()
        })
    }

    /// Exactly these addresses are online on this hub now.
    pub fn set_online(&self, hub: &str, online: &[String]) -> Result<()> {
        self.with(|c| {
            let tx = c.transaction()?;
            tx.execute("UPDATE roster SET online=0 WHERE hub=?", [hub])?;
            for a in online {
                tx.execute(
                    "UPDATE roster SET online=1 WHERE hub=? AND address=?",
                    params![hub, a],
                )?;
            }
            tx.commit()
        })
    }

    // ------------------------------------------------------ attachments

    pub fn set_attachment(
        &self,
        local_id: &str,
        state: &str,
        hub_id: Option<&str>,
        local_path: Option<&str>,
        error: Option<&str>,
    ) -> Result<()> {
        self.with(|c| {
            c.execute(
                "UPDATE attachments SET state=?, hub_id=COALESCE(?, hub_id), local_path=COALESCE(?, local_path), error=? WHERE local_id=?",
                params![state, hub_id, local_path, error, local_id],
            )
            .map(|_| ())
        })
    }

    // ------------------------------------------------------------ drafts

    pub fn set_draft(&self, peer: &str, body: &str) -> Result<()> {
        self.with(|c| {
            if body.is_empty() {
                c.execute("DELETE FROM drafts WHERE peer=?", [peer]).map(|_| ())
            } else {
                c.execute("INSERT INTO drafts(peer, body) VALUES(?,?) ON CONFLICT(peer) DO UPDATE SET body=excluded.body", [peer, body])
                    .map(|_| ())
            }
        })
    }

    pub fn draft(&self, peer: &str) -> Result<Option<String>> {
        self.with(|c| {
            c.query_row("SELECT body FROM drafts WHERE peer=?", [peer], |r| r.get(0))
                .optional()
        })
    }

    // ----------------------------------------------------------- helpers

    fn messages_where(&self, cond: &str, p: impl rusqlite::Params) -> Result<Vec<Message>> {
        self.with(|c| {
            let sql = format!(
                "SELECT m.id, m.peer, m.outgoing, m.hub, m.body, m.kind, m.reply_to, m.sent_at, m.received_at, m.created_at,
                        m.state, m.fetched_at, m.delivered_at, m.read_at, m.error, m.seen
                 FROM messages m WHERE {cond}"
            );
            let mut st = c.prepare(&sql)?;
            let mut msgs: Vec<Message> = st
                .query_map(p, |r| {
                    Ok(Message {
                        id: r.get(0)?,
                        peer: r.get(1)?,
                        outgoing: r.get(2)?,
                        hub: r.get(3)?,
                        body: r.get(4)?,
                        kind: r.get(5)?,
                        reply_to: r.get(6)?,
                        sent_at: r.get(7)?,
                        received_at: r.get(8)?,
                        created_at: r.get(9)?,
                        state: r.get(10)?,
                        fetched_at: r.get(11)?,
                        delivered_at: r.get(12)?,
                        read_at: r.get(13)?,
                        error: r.get(14)?,
                        seen: r.get(15)?,
                        attachments: Vec::new(),
                    })
                })?
                .collect::<rusqlite::Result<_>>()?;
            let mut st = c.prepare(
                "SELECT local_id, hub_id, name, bytes, source, local_path, state, error FROM attachments
                 WHERE message_id=? ORDER BY position",
            )?;
            for m in &mut msgs {
                m.attachments = st
                    .query_map([&m.id], |r| {
                        Ok(Attachment {
                            local_id: r.get(0)?,
                            hub_id: r.get(1)?,
                            name: r.get(2)?,
                            bytes: r.get::<_, i64>(3)? as u64,
                            source: r.get(4)?,
                            local_path: r.get(5)?,
                            state: r.get(6)?,
                            error: r.get(7)?,
                        })
                    })?
                    .collect::<rusqlite::Result<_>>()?;
            }
            Ok(msgs)
        })
    }
}

/// How far along the receipt ladder a state is; states only move forward.
fn rank(state: &str) -> u8 {
    match state {
        "sending" | "failed" => 1,
        "sent" => 2,
        "fetched" => 3,
        "delivered" => 4,
        "read" => 5,
        _ => 0, // queued, received
    }
}

/// Record that `hub` holds a copy of message `id`.
fn held_by(tx: &rusqlite::Transaction, id: &str, hub: &str) -> rusqlite::Result<usize> {
    tx.execute(
        "INSERT OR IGNORE INTO message_hubs(id, hub) VALUES(?,?)",
        [id, hub],
    )
}

fn insert_remote_attachment(
    tx: &rusqlite::Transaction,
    msg: &str,
    i: usize,
    a: &AttachmentMeta,
) -> rusqlite::Result<usize> {
    tx.execute(
        "INSERT INTO attachments(local_id, message_id, position, hub_id, name, bytes, state) VALUES(?,?,?,?,?,?,'remote')",
        params![uuid::Uuid::new_v4().simple().to_string(), msg, i as i64, a.id, a.name, a.bytes as i64],
    )
}

/// What the UI hands the core to send.
#[derive(Debug, Clone, Deserialize)]
pub struct NewOutgoing {
    pub id: String,
    pub peer: String,
    pub body: String,
    pub kind: Option<String>,
    pub reply_to: Option<String>,
    #[serde(default)]
    pub attachments: Vec<NewAttachment>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct NewAttachment {
    pub name: String,
    pub bytes: u64,
    /// A filesystem path or (Android) a content:// URI.
    pub source: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env(id: &str, from: &str) -> Envelope {
        Envelope {
            id: id.into(),
            from: from.into(),
            to: "me.000000".into(),
            body: "hi".into(),
            kind: None,
            thread_id: None,
            sent_at: None,
            received_at: "2026-10-08T10:00:00.000Z".into(),
            reply_to: None,
            attachments: vec![AttachmentMeta {
                id: "a1".into(),
                name: "f.txt".into(),
                bytes: 3,
            }],
        }
    }

    #[test]
    fn incoming_is_deduplicated_and_counted_unread() {
        let s = Store::open_in_memory().unwrap();
        s.add_hub("http://h:7370", "t0").unwrap();
        assert!(s
            .insert_incoming("http://h:7370", &env("m1", "maya.111111"), "t1")
            .unwrap());
        assert!(!s
            .insert_incoming("http://h:7370", &env("m1", "maya.111111"), "t1")
            .unwrap());
        let chats = s.chats().unwrap();
        assert_eq!(chats.len(), 1);
        assert_eq!(chats[0].unread, 1);
        assert_eq!(chats[0].last.attachments[0].hub_id.as_deref(), Some("a1"));
        assert_eq!(s.mark_seen("maya.111111", "2026-10-09T00:00:00Z").unwrap().len(), 1);
        assert_eq!(s.chats().unwrap()[0].unread, 0);
        // when it was read is kept (Message info)
        let m = s.message("m1").unwrap().unwrap();
        assert_eq!(m.read_at.as_deref(), Some("2026-10-09T00:00:00Z"));
        assert!(s.mark_seen("maya.111111", "2026-10-09T01:00:00Z").unwrap().is_empty());
    }

    #[test]
    fn pages_of_history_skip_nothing_even_on_equal_times() {
        let s = Store::open_in_memory().unwrap();
        s.add_hub("http://h:7370", "t0").unwrap();
        // one poll's worth: the same local time for all five
        for id in ["m1", "m2", "m3", "m4", "m5"] {
            s.insert_incoming("http://h:7370", &env(id, "maya.111111"), "t1").unwrap();
        }
        let ids = |v: &[Message]| v.iter().map(|m| m.id.clone()).collect::<Vec<_>>();
        let p1 = s.chat("maya.111111", None, None, 2).unwrap();
        assert_eq!(ids(&p1), ["m4", "m5"]);
        let cur = |m: &Message| (m.created_at.clone(), m.id.clone());
        let (t, id) = cur(&p1[0]);
        let p2 = s.chat("maya.111111", Some((&t, &id)), None, 2).unwrap();
        assert_eq!(ids(&p2), ["m2", "m3"]);
        let (t, id) = cur(&p2[0]);
        let p3 = s.chat("maya.111111", Some((&t, &id)), None, 2).unwrap();
        assert_eq!(ids(&p3), ["m1"]);
        // a re-read from the oldest shown keeps exactly that range
        let (t, id) = cur(&p2[0]);
        assert_eq!(ids(&s.chat("maya.111111", None, Some((&t, &id)), 5000).unwrap()), ["m2", "m3", "m4", "m5"]);
    }

    #[test]
    fn receipts_only_move_forward() {
        let s = Store::open_in_memory().unwrap();
        let m = NewOutgoing {
            id: "o1".into(),
            peer: "maya.111111".into(),
            body: "x".into(),
            kind: None,
            reply_to: None,
            attachments: vec![],
        };
        s.queue_outgoing(&m, "t1").unwrap();
        assert_eq!(s.queued().unwrap().len(), 1);
        s.mark_sent("o1", "http://h:7370", "t2").unwrap();
        let r = |state: &str| Receipt {
            id: "o1".into(),
            state: state.into(),
            fetched_at: Some("t3".into()),
            delivered_at: None,
            read_at: None,
        };
        s.apply_receipt(&r("read")).unwrap();
        s.apply_receipt(&r("fetched")).unwrap();
        assert_eq!(s.message("o1").unwrap().unwrap().state, "read");
        assert!(s.queued().unwrap().is_empty());
    }

    #[test]
    fn directory_merges_hubs() {
        let s = Store::open_in_memory().unwrap();
        for h in ["http://a:7370", "http://b:7370"] {
            s.add_hub(h, "t0").unwrap();
        }
        let e = |online| RosterEntry {
            slug: "maya.111111".into(),
            org_name: String::new(),
            username: "maya".into(),
            blurb: String::new(),
            online,
            last_seen: None,
            kind: "chat".into(),
        };
        s.set_roster("http://a:7370", &[e(false)]).unwrap();
        s.set_roster("http://b:7370", &[e(true)]).unwrap();
        let d = s.directory().unwrap();
        assert_eq!(d.len(), 1);
        assert!(d[0].online);
        assert_eq!(d[0].hubs.len(), 2);
        s.remove_hub("http://b:7370").unwrap();
        assert_eq!(
            s.directory().unwrap()[0].hubs,
            vec!["http://a:7370".to_string()]
        );
    }

    fn synced(id: &str) -> SyncedMessage {
        SyncedMessage {
            env: env(id, "maya.111111"),
            fetched_at: None,
            delivered_at: None,
            read_at: None,
            body_bytes: None,
        }
    }

    /// B2: every hub that holds a message is known (the label alone was
    /// overwritten by each sync), and deletes owed by a hub that is down wait.
    #[test]
    fn every_hub_holding_a_message_is_known() {
        let s = Store::open_in_memory().unwrap();
        let (a, b) = ("http://a:7370", "http://b:7370");
        for h in [a, b] {
            s.add_hub(h, "t0").unwrap();
        }
        s.upsert_synced(a, "me.000000", &synced("m1")).unwrap();
        s.upsert_synced(b, "me.000000", &synced("m1")).unwrap();
        s.insert_incoming(b, &env("m2", "maya.111111"), "t1").unwrap();
        assert_eq!(s.message_hubs("m1").unwrap(), [a, b]);
        assert_eq!(s.message_hubs("m2").unwrap(), [b]);
        // the label stays the hub it came through first (its attachment ids)
        assert_eq!(s.message("m1").unwrap().unwrap().hub.as_deref(), Some(a));
        s.queue_delete(b, "message", "m1").unwrap();
        s.queue_delete(b, "message", "m1").unwrap();
        s.queue_delete(b, "chat", "maya.111111").unwrap();
        assert_eq!(s.pending_deletes(b).unwrap().len(), 2);
        assert!(s.pending_deletes(a).unwrap().is_empty());
        s.delete_done(b, "message", "m1").unwrap();
        assert_eq!(s.pending_deletes(b).unwrap(), [("chat".to_string(), "maya.111111".to_string())]);
        s.delete_message("m1").unwrap();
        assert!(s.message_hubs("m1").unwrap().is_empty());
        // a removed hub owes nothing and holds nothing
        s.remove_hub(b).unwrap();
        assert!(s.pending_deletes(b).unwrap().is_empty());
        assert!(s.message_hubs("m2").unwrap().is_empty());
        assert!(s.message("m2").unwrap().is_some());
    }

    /// B3: a hub that starts its sync over drops only what no other hub
    /// still holds.
    #[test]
    fn a_hub_starting_over_keeps_what_another_hub_holds() {
        let s = Store::open_in_memory().unwrap();
        let (a, b) = ("http://a:7370", "http://b:7370");
        for h in [a, b] {
            s.add_hub(h, "t0").unwrap();
        }
        let me = "me.000000";
        // on both hubs (synced from A, then from B), on A only, on B only
        s.upsert_synced(a, me, &synced("both")).unwrap();
        s.upsert_synced(b, me, &synced("both")).unwrap();
        s.upsert_synced(a, me, &synced("a-only")).unwrap();
        s.upsert_synced(b, me, &synced("b-only")).unwrap();
        s.forget_hub_messages(b).unwrap();
        assert!(s.message("both").unwrap().is_some(), "hub A still holds it");
        assert!(s.message("a-only").unwrap().is_some());
        assert!(s.message("b-only").unwrap().is_none(), "no hub holds it any more");
        // B's sync from the start brings back what it has; then A starts over
        s.upsert_synced(b, me, &synced("both")).unwrap();
        s.upsert_synced(b, me, &synced("b-only")).unwrap();
        s.forget_hub_messages(a).unwrap();
        assert!(s.message("both").unwrap().is_some(), "hub B still holds it");
        assert!(s.message("a-only").unwrap().is_none());
        assert!(s.message("b-only").unwrap().is_some());
    }

    #[test]
    fn a_pinned_hub_is_kept_per_chat_and_goes_with_its_hub() {
        let s = Store::open_in_memory().unwrap();
        for h in ["http://a:7370", "http://b:7370"] {
            s.add_hub(h, "t0").unwrap();
        }
        assert_eq!(s.send_hub("maya.111111").unwrap(), None);
        s.set_send_hub("maya.111111", Some("http://a:7370")).unwrap();
        s.set_send_hub("maya.111111", Some("http://b:7370")).unwrap();
        s.set_send_hub("pat.222222", Some("http://a:7370")).unwrap();
        assert_eq!(s.send_hub("maya.111111").unwrap().as_deref(), Some("http://b:7370"));
        // a hub we don't have can't be pinned
        assert!(s.set_send_hub("maya.111111", Some("http://zz:7370")).is_err());
        // removing the hub: back to Automatic
        s.remove_hub("http://b:7370").unwrap();
        assert_eq!(s.send_hub("maya.111111").unwrap(), None);
        s.set_send_hub("pat.222222", None).unwrap();
        assert_eq!(s.send_hub("pat.222222").unwrap(), None);
    }
}
