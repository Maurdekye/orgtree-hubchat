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
"#;

/// Schema changes after the first release, in order: `PRAGMA user_version`
/// says how many a store has had. Each runs once, in one transaction.
const MIGRATIONS: &[&str] = &[
    // 1 (B1-B3, the hub picker): every hub holding a message; deletes owed
    // by hubs that were down, one row per message (a chat's too: a hub
    // deletes a conversation as it stands when the request arrives, so a
    // late "delete chat" would take newer messages); the hub each
    // attachment id belongs to; a chat's pinned hub; indexes for the
    // per-send route lookup and relabelling
    r#"
CREATE TABLE IF NOT EXISTS message_hubs (
  id  TEXT NOT NULL REFERENCES messages(id) ON DELETE CASCADE,
  hub TEXT NOT NULL,
  PRIMARY KEY (id, hub)
);
CREATE INDEX IF NOT EXISTS message_hubs_by_hub ON message_hubs(hub);
INSERT OR IGNORE INTO message_hubs(id, hub) SELECT id, hub FROM messages WHERE hub IS NOT NULL;
DELETE FROM meta WHERE key='schema.message_hubs';
DROP TABLE IF EXISTS pending_deletes;
CREATE TABLE pending_deletes (
  hub         TEXT NOT NULL REFERENCES hubs(url) ON DELETE CASCADE,
  id          TEXT NOT NULL,              -- a message id
  queued_at   TEXT NOT NULL,              -- when the user deleted it
  attempts    INTEGER NOT NULL DEFAULT 0,
  next_try_at INTEGER NOT NULL DEFAULT 0, -- unix ms
  PRIMARY KEY (hub, id)
);
CREATE INDEX pending_deletes_due ON pending_deletes(hub, next_try_at);
CREATE INDEX pending_deletes_by_id ON pending_deletes(id);
-- per device, never synced (hubchat-opus 2026-10-09 09:36Z)
CREATE TABLE IF NOT EXISTS chat_hub (
  peer TEXT PRIMARY KEY,
  hub  TEXT NOT NULL REFERENCES hubs(url) ON DELETE CASCADE
);
ALTER TABLE attachments ADD COLUMN hub TEXT;
UPDATE attachments SET hub=(SELECT m.hub FROM messages m WHERE m.id=attachments.message_id)
  WHERE hub_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS attachments_by_message ON attachments(message_id);
CREATE INDEX IF NOT EXISTS messages_by_hub ON messages(hub);
CREATE INDEX IF NOT EXISTS messages_sent_by_peer ON messages(peer, created_at, id)
  WHERE outgoing=1 AND hub IS NOT NULL;
"#,
    // 2 (lazy history): how far back each hub has loaded each chat, for a
    // device that started from now on that hub
    r#"
CREATE TABLE IF NOT EXISTS history_marks (
  hub       TEXT NOT NULL REFERENCES hubs(url) ON DELETE CASCADE,
  peer      TEXT NOT NULL,
  before    TEXT,                       -- the hub's cursor for the next older page
  oldest_ms INTEGER,                    -- hub clock: the oldest message loaded so far
  done      INTEGER NOT NULL DEFAULT 0, -- the chat's start is reached
  -- unread on the hub (its chat list) and not loaded here yet
  old_unread INTEGER NOT NULL DEFAULT 0,
  listed_id  TEXT,                      -- the chat list's newest message (stored at once)
  PRIMARY KEY (hub, peer)
);
CREATE INDEX IF NOT EXISTS messages_by_reply_to ON messages(reply_to) WHERE reply_to IS NOT NULL;
-- a "Delete chat" a hub that was down still owes for messages this device
-- never loaded: on a hub with lazy history, its history before the delete
CREATE TABLE IF NOT EXISTS pending_chat_deletes (
  hub    TEXT NOT NULL REFERENCES hubs(url) ON DELETE CASCADE,
  peer   TEXT NOT NULL,
  before TEXT NOT NULL,                 -- unix ms on the hub's clock, then its cursor
  PRIMARY KEY (hub, peer)
);
"#,
];

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
    /// The hub `hub_id` belongs to (each hub has its own ids).
    pub hub: Option<String>,
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

/// How far back one hub has loaded one chat (lazy history).
#[derive(Debug, Clone, Default, Serialize, PartialEq)]
pub struct HistoryMark {
    /// The hub's exact cursor for the next older page; None before the
    /// first page (start from the device's start time) or once done.
    pub before: Option<String>,
    /// Hub clock, unix ms: the oldest message loaded through this hub.
    pub oldest_ms: Option<i64>,
    /// The chat's start on this hub is reached.
    pub done: bool,
    /// Unread on the hub but not loaded here yet (they count as unread).
    pub old_unread: i64,
    /// The newest message from the hub's chat list, stored with it (so
    /// not among `old_unread`).
    pub listed_id: Option<String>,
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

    fn init(mut con: Connection) -> Result<Self> {
        con.busy_timeout(std::time::Duration::from_secs(5))
            .map_err(db)?;
        con.execute_batch(SCHEMA).map_err(db)?;
        migrate(&mut con).map_err(db)?;
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

    pub fn delete_meta(&self, key: &str) -> Result<()> {
        self.with(|c| c.execute("DELETE FROM meta WHERE key=?", [key]).map(|_| ()))
    }

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
            relabel(&tx, url)?;
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
                    insert_remote_attachment(&tx, hub, &env.id, i, a)?;
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

    /// A send went through. False: the message was deleted meanwhile, so
    /// the hub's new copy is to be deleted too.
    pub fn mark_sent(&self, id: &str, hub: &str, received_at: &str) -> Result<bool> {
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
            tx.commit()?;
            Ok(n > 0)
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
            // what the hubs' chat lists counted but isn't loaded: seen too
            tx.execute("UPDATE history_marks SET old_unread=0 WHERE peer=? AND old_unread<>0", [peer])?;
            tx.commit()?;
            Ok(ids)
        })
    }

    /// How many of a chat's incoming messages are not read yet (on any of
    /// our devices: a v2 hub's read time marks them read here too).
    pub fn unread(&self, peer: &str) -> Result<u64> {
        self.with(|c| {
            c.query_row(
                "SELECT (SELECT COUNT(*) FROM messages WHERE peer=?1 AND outgoing=0 AND seen=0)
                   + (SELECT COALESCE(SUM(old_unread), 0) FROM history_marks WHERE peer=?1)",
                [peer],
                |r| r.get(0),
            )
        })
    }

    pub fn chats(&self) -> Result<Vec<ChatSummary>> {
        let peers: Vec<(String, String, u64)> = self.with(|c| {
            let mut st = c.prepare(
                "SELECT peer, MAX(created_at) AS last, SUM(CASE WHEN outgoing=0 AND seen=0 THEN 1 ELSE 0 END)
                   + (SELECT COALESCE(SUM(h.old_unread), 0) FROM history_marks h WHERE h.peer=messages.peer)
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
        self.upsert_synced_at(hub, me, m, 0)
    }

    /// `upsert_synced` with the hub's clock offset (its clock minus ours,
    /// ms): a message sorts by when the hub received it, on our clock, and
    /// one held on several hubs by the earliest of those times.
    pub fn upsert_synced_at(&self, hub: &str, me: &str, m: &SyncedMessage, offset_ms: i64) -> Result<bool> {
        let env = &m.env;
        let at = shift_ms(&env.received_at, -offset_ms).unwrap_or_else(|| env.received_at.clone());
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
            // deleted here, its delete still owed to some hub: not back
            let deleting: bool = tx.query_row(
                "SELECT EXISTS(SELECT 1 FROM pending_deletes WHERE id=?)",
                [&env.id],
                |r| r.get(0),
            )?;
            if deleting {
                return Ok(false);
            }
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
                            // history sorts by the hub's clock, made ours
                            at,
                            hub_state,
                            m.fetched_at,
                            m.delivered_at,
                            m.read_at,
                            !outgoing && m.read_at.is_some(),
                        ],
                    )?;
                    for (i, a) in env.attachments.iter().enumerate() {
                        insert_remote_attachment(&tx, hub, &env.id, i, a)?;
                    }
                }
                Some(state) => {
                    let state = if rank(hub_state) > rank(&state) { hub_state.to_owned() } else { state };
                    // the label stays the hub it first came through;
                    // message_hubs keeps every hub that holds it
                    tx.execute(
                        "UPDATE messages SET hub=COALESCE(hub, ?), received_at=COALESCE(received_at, ?),
                           created_at=MIN(created_at, ?),
                           fetched_at=COALESCE(?, fetched_at), delivered_at=COALESCE(?, delivered_at),
                           read_at=COALESCE(?, read_at), state=?, error=CASE WHEN ?='failed' THEN error ELSE NULL END,
                           seen = seen OR ?
                         WHERE id=?",
                        params![
                            hub,
                            env.received_at,
                            at,
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
            after_its_parent(&tx, &env.id)?;
            // attachment ids are per hub: ids of a hub that no longer holds
            // the message (it started over, or was removed) become this one's
            for (i, a) in env.attachments.iter().enumerate() {
                tx.execute(
                    "UPDATE attachments SET hub_id=?, hub=? WHERE message_id=? AND position=?
                       AND state NOT IN ('pending','uploading')
                       AND (hub IS NULL OR hub NOT IN (SELECT o.hub FROM message_hubs o WHERE o.id=?))",
                    params![a.id, hub, env.id, i as i64, env.id],
                )?;
            }
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

    /// "Delete for me", here: the message goes, and each of `hubs` owes
    /// its delete until it is sent (B2). Returns the message's attachments'
    /// local ids (a transfer still running for them is to stop).
    pub fn forget_message(&self, id: &str, hubs: &[String], now: &str) -> Result<Vec<String>> {
        self.with(|c| {
            let tx = c.transaction()?;
            let lids = attachment_ids(&tx, "a.message_id=?", id)?;
            owe(&tx, hubs, "SELECT ?2 AS id", id, now)?;
            tx.execute("DELETE FROM messages WHERE id=?", [id])?;
            tx.commit()?;
            Ok(lids)
        })
    }

    /// "Delete chat", here: its messages go, and each of `hubs` owes each
    /// message's delete (never the chat's: a hub deletes a conversation as
    /// it stands when the request arrives, newer messages too). Returns the
    /// chat's message ids and its attachments' local ids.
    pub fn forget_chat(&self, peer: &str, hubs: &[String], now: &str) -> Result<(Vec<String>, Vec<String>)> {
        self.with(|c| {
            let tx = c.transaction()?;
            let lids = attachment_ids(&tx, "m.peer=?", peer)?;
            let ids: Vec<String> = {
                let mut st = tx.prepare("SELECT id FROM messages WHERE peer=?")?;
                let rows = st.query_map([peer], |r| r.get(0))?;
                rows.collect::<rusqlite::Result<_>>()?
            };
            owe(&tx, hubs, "SELECT id FROM messages WHERE peer=?2", peer, now)?;
            tx.execute("DELETE FROM messages WHERE peer=?", [peer])?;
            tx.commit()?;
            Ok((ids, lids))
        })
    }

    /// Deletes `hub` owes: these message ids.
    pub fn queue_deletes(&self, hub: &str, ids: &[String], now: &str) -> Result<()> {
        self.with(|c| {
            let tx = c.transaction()?;
            for id in ids {
                owe(&tx, &[hub.to_owned()], "SELECT ?2 AS id", id, now)?;
            }
            tx.commit()
        })
    }

    /// Deletes this hub owes that are due by `now_ms`: a bounded batch, the
    /// longest waiting first.
    pub fn due_deletes(&self, hub: &str, now_ms: u64) -> Result<Vec<String>> {
        self.with(|c| {
            let mut st = c.prepare(
                "SELECT id FROM pending_deletes WHERE hub=? AND next_try_at<=?
                 ORDER BY next_try_at, queued_at LIMIT 100",
            )?;
            let rows = st.query_map(params![hub, now_ms.min(i64::MAX as u64) as i64], |r| r.get(0))?;
            rows.collect()
        })
    }

    /// Deletes this hub still owes, due or not (a bounded batch).
    pub fn pending_deletes(&self, hub: &str) -> Result<Vec<String>> {
        self.due_deletes(hub, u64::MAX)
    }

    pub fn delete_done(&self, hub: &str, id: &str) -> Result<()> {
        self.with(|c| {
            c.execute("DELETE FROM pending_deletes WHERE hub=? AND id=?", [hub, id])
                .map(|_| ())
        })
    }

    /// A delete the hub could not take now: try again later, waiting longer
    /// each time (30 s, doubling, at most an hour).
    pub fn delete_later(&self, hub: &str, id: &str, now_ms: u64) -> Result<()> {
        self.with(|c| {
            c.execute(
                "UPDATE pending_deletes SET attempts=attempts+1,
                   next_try_at=?3 + MIN(30000 * (1 << MIN(attempts, 7)), 3600000)
                 WHERE hub=?1 AND id=?2",
                params![hub, id, now_ms as i64],
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
            // what was paged back from it is gone too: page again
            tx.execute("DELETE FROM history_marks WHERE hub=?", [hub])?;
            relabel(&tx, hub)?;
            tx.commit()
        })
    }

    // ------------------------------------------------------- lazy history

    /// A hub's chat list says `n` of the chat's messages, not loaded here,
    /// are unread; `listed_id` is its newest message, stored with the list.
    pub fn set_old_unread(&self, hub: &str, peer: &str, n: i64, listed_id: Option<&str>) -> Result<()> {
        self.with(|c| {
            c.execute(
                "INSERT INTO history_marks(hub, peer, old_unread, listed_id) VALUES(?,?,?,?)
                 ON CONFLICT(hub, peer) DO UPDATE SET old_unread=excluded.old_unread, listed_id=excluded.listed_id",
                params![hub, peer, n.max(0), listed_id],
            )
            .map(|_| ())
        })
    }

    /// `hub` owes the chat with `peer` a delete of what it received before
    /// `before` (unix ms, the hub's clock). A later one replaces it.
    pub fn queue_chat_delete(&self, hub: &str, peer: &str, before: i64) -> Result<()> {
        self.set_chat_delete(hub, peer, &before.to_string())
    }

    /// Where a hub's owed chat delete has got to (`before`: a time or the
    /// hub's history cursor).
    pub fn set_chat_delete(&self, hub: &str, peer: &str, before: &str) -> Result<()> {
        self.with(|c| {
            c.execute(
                "INSERT INTO pending_chat_deletes(hub, peer, before) VALUES(?,?,?)
                 ON CONFLICT(hub, peer) DO UPDATE SET before=excluded.before",
                [hub, peer, before],
            )
            .map(|_| ())
        })
    }

    /// The chat deletes `hub` owes: (peer, before).
    pub fn chat_deletes(&self, hub: &str) -> Result<Vec<(String, String)>> {
        self.with(|c| {
            let mut st = c.prepare("SELECT peer, before FROM pending_chat_deletes WHERE hub=? LIMIT 100")?;
            let rows = st.query_map([hub], |r| Ok((r.get(0)?, r.get(1)?)))?;
            rows.collect()
        })
    }

    pub fn chat_delete_done(&self, hub: &str, peer: &str) -> Result<()> {
        self.with(|c| {
            c.execute("DELETE FROM pending_chat_deletes WHERE hub=? AND peer=?", [hub, peer])
                .map(|_| ())
        })
    }

    /// The hub starts this device over: page every chat again.
    pub fn clear_history_marks(&self, hub: &str) -> Result<()> {
        self.with(|c| c.execute("DELETE FROM history_marks WHERE hub=?", [hub]).map(|_| ()))
    }

    /// How far back `hub` has loaded the chat with `peer` (None: not yet).
    pub fn history_mark(&self, hub: &str, peer: &str) -> Result<Option<HistoryMark>> {
        self.with(|c| {
            c.query_row(
                "SELECT before, oldest_ms, done, old_unread, listed_id FROM history_marks WHERE hub=? AND peer=?",
                [hub, peer],
                |r| {
                    Ok(HistoryMark {
                        before: r.get(0)?,
                        oldest_ms: r.get(1)?,
                        done: r.get(2)?,
                        old_unread: r.get(3)?,
                        listed_id: r.get(4)?,
                    })
                },
            )
            .optional()
        })
    }

    pub fn set_history_mark(&self, hub: &str, peer: &str, m: &HistoryMark) -> Result<()> {
        self.with(|c| {
            c.execute(
                "INSERT INTO history_marks(hub, peer, before, oldest_ms, done, old_unread, listed_id)
                 VALUES(?,?,?,?,?,?,?)
                 ON CONFLICT(hub, peer) DO UPDATE SET before=excluded.before, oldest_ms=excluded.oldest_ms,
                   done=excluded.done, old_unread=excluded.old_unread, listed_id=excluded.listed_id",
                params![hub, peer, m.before, m.oldest_ms, m.done, m.old_unread, m.listed_id],
            )
            .map(|_| ())
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

    /// An upload's id on `hub` (a finished file, or a resumable session).
    pub fn set_upload(&self, local_id: &str, state: &str, hub_id: &str, hub: &str) -> Result<()> {
        self.with(|c| {
            c.execute(
                "UPDATE attachments SET state=?, hub_id=?, hub=?, error=NULL WHERE local_id=?",
                params![state, hub_id, hub, local_id],
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
                "SELECT local_id, hub_id, hub, name, bytes, source, local_path, state, error FROM attachments
                 WHERE message_id=? ORDER BY position",
            )?;
            for m in &mut msgs {
                m.attachments = st
                    .query_map([&m.id], |r| {
                        Ok(Attachment {
                            local_id: r.get(0)?,
                            hub_id: r.get(1)?,
                            hub: r.get(2)?,
                            name: r.get(3)?,
                            bytes: r.get::<_, i64>(4)? as u64,
                            source: r.get(5)?,
                            local_path: r.get(6)?,
                            state: r.get(7)?,
                            error: r.get(8)?,
                        })
                    })?
                    .collect::<rusqlite::Result<_>>()?;
            }
            Ok(msgs)
        })
    }
}

/// Each of `hubs` still in the store owes the delete of the ids `ids_sql`
/// selects (?2 = `arg`). A hub removed meanwhile owes nothing.
fn owe(tx: &rusqlite::Transaction, hubs: &[String], ids_sql: &str, arg: &str, now: &str) -> rusqlite::Result<()> {
    let sql = format!(
        "INSERT OR IGNORE INTO pending_deletes(hub, id, queued_at)
         SELECT h.url, d.id, ?3 FROM hubs h, ({ids_sql}) d WHERE h.url=?1"
    );
    let mut st = tx.prepare(&sql)?;
    for h in hubs {
        st.execute(params![h, arg, now])?;
    }
    Ok(())
}

/// Local ids of the attachments of the messages `cond` selects (?1 = `arg`).
fn attachment_ids(tx: &rusqlite::Transaction, cond: &str, arg: &str) -> rusqlite::Result<Vec<String>> {
    let mut st = tx.prepare(&format!(
        "SELECT a.local_id FROM attachments a JOIN messages m ON m.id=a.message_id WHERE {cond}"
    ))?;
    let rows = st.query_map([arg], |r| r.get(0))?;
    rows.collect()
}

/// Bring a store up to the newest schema (MIGRATIONS).
fn migrate(con: &mut Connection) -> rusqlite::Result<()> {
    let have: usize = con.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    for (i, sql) in MIGRATIONS.iter().enumerate().skip(have) {
        let tx = con.transaction()?;
        tx.execute_batch(sql)?;
        tx.pragma_update(None, "user_version", i + 1)?;
        tx.commit()?;
    }
    Ok(())
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

/// Messages labelled with `hub`, which no longer holds them, now name a hub
/// that does (read receipts go there). Their attachments keep their own hub.
fn relabel(tx: &rusqlite::Transaction, hub: &str) -> rusqlite::Result<usize> {
    tx.execute(
        "UPDATE messages SET hub=(SELECT o.hub FROM message_hubs o WHERE o.id=messages.id ORDER BY o.hub LIMIT 1)
         WHERE hub=?1 AND EXISTS (SELECT 1 FROM message_hubs o WHERE o.id=messages.id)",
        [hub],
    )
}

/// A reply never sorts above the message it answers (hubs' clocks differ):
/// it goes 1 ms after its parent, and the replies below it (to a bounded
/// depth) after it in turn.
fn after_its_parent(tx: &rusqlite::Transaction, id: &str) -> rusqlite::Result<()> {
    let Some((mut at, reply_to)) = tx
        .query_row(
            "SELECT created_at, reply_to FROM messages WHERE id=?",
            [id],
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?)),
        )
        .optional()?
    else {
        return Ok(());
    };
    if let Some(parent) = reply_to {
        let p: Option<String> = tx
            .query_row("SELECT created_at FROM messages WHERE id=?", [&parent], |r| r.get(0))
            .optional()?;
        if let Some(next) = p.filter(|p| *p >= at).and_then(|p| shift_ms(&p, 1)) {
            tx.execute("UPDATE messages SET created_at=? WHERE id=?", [&next, id])?;
            at = next;
        }
    }
    // a reply to a reply stored before them both moves down the chain too
    let mut level = vec![(id.to_owned(), at)];
    for _ in 0..REPLY_CHAIN_DEPTH {
        let mut next_level = Vec::new();
        for (parent, at) in &level {
            let Some(next) = shift_ms(at, 1) else { continue };
            let moved: Vec<String> = {
                let mut st = tx.prepare("SELECT id FROM messages WHERE reply_to=? AND created_at<=? LIMIT 100")?;
                let rows = st.query_map([parent, at], |r| r.get(0))?;
                rows.collect::<rusqlite::Result<_>>()?
            };
            for child in moved {
                tx.execute("UPDATE messages SET created_at=? WHERE id=?", [&next, &child])?;
                next_level.push((child, next.clone()));
            }
        }
        if next_level.is_empty() {
            break;
        }
        level = next_level;
    }
    Ok(())
}

/// How far down a reply chain `after_its_parent` moves replies.
const REPLY_CHAIN_DEPTH: usize = 8;

/// An RFC 3339 time `ms` milliseconds later (earlier when negative), in
/// the form the store keeps (`now()`'s).
pub(crate) fn shift_ms(t: &str, ms: i64) -> Option<String> {
    let st = humantime::parse_rfc3339_weak(t).ok()?;
    let d = std::time::Duration::from_millis(ms.unsigned_abs());
    let st = if ms >= 0 { st.checked_add(d)? } else { st.checked_sub(d)? };
    Some(humantime::format_rfc3339_millis(st).to_string())
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
    hub: &str,
    msg: &str,
    i: usize,
    a: &AttachmentMeta,
) -> rusqlite::Result<usize> {
    tx.execute(
        "INSERT INTO attachments(local_id, message_id, position, hub_id, hub, name, bytes, state) VALUES(?,?,?,?,?,?,?,'remote')",
        params![uuid::Uuid::new_v4().simple().to_string(), msg, i as i64, a.id, hub, a.name, a.bytes as i64],
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
        // the label stays the hub it came through first
        assert_eq!(s.message("m1").unwrap().unwrap().hub.as_deref(), Some(a));
        // the delete is owed by the hubs named, a hub we don't have owes nothing
        let gone = "http://gone:7370".to_string();
        let lids = s.forget_message("m1", &[b.to_string(), gone.clone()], "t2").unwrap();
        assert_eq!(lids.len(), 1, "its attachment's transfer is to stop");
        assert!(s.message("m1").unwrap().is_none());
        assert!(s.message_hubs("m1").unwrap().is_empty());
        assert_eq!(s.pending_deletes(b).unwrap(), ["m1"]);
        assert!(s.pending_deletes(a).unwrap().is_empty());
        assert!(s.pending_deletes(&gone).unwrap().is_empty());
        s.forget_message("m1", &[b.to_string()], "t3").unwrap();
        assert_eq!(s.pending_deletes(b).unwrap(), ["m1"]);
        // a sync can't bring back a message whose delete is still owed
        assert!(!s.upsert_synced(a, "me.000000", &synced("m1")).unwrap());
        assert!(s.message("m1").unwrap().is_none());
        s.delete_done(b, "m1").unwrap();
        assert!(s.pending_deletes(b).unwrap().is_empty());
        // a removed hub owes nothing and holds nothing; the label moves on
        s.upsert_synced(a, "me.000000", &synced("m2")).unwrap();
        s.queue_deletes(b, &["x".into()], "t4").unwrap();
        s.remove_hub(b).unwrap();
        assert!(s.pending_deletes(b).unwrap().is_empty());
        assert_eq!(s.message_hubs("m2").unwrap(), [a]);
        assert_eq!(s.message("m2").unwrap().unwrap().hub.as_deref(), Some(a));
        // a hub removed while a delete is being queued: nothing to owe, no error
        s.queue_deletes(b, &["y".into()], "t5").unwrap();
    }

    /// Review finding 1: a chat deleted while a hub is down is owed message
    /// by message: a message that comes later is not in it.
    #[test]
    fn a_chat_delete_is_owed_message_by_message() {
        let s = Store::open_in_memory().unwrap();
        let b = "http://b:7370";
        s.add_hub(b, "t0").unwrap();
        s.insert_incoming(b, &env("c1", "maya.111111"), "t1").unwrap();
        s.insert_incoming(b, &env("c2", "maya.111111"), "t1").unwrap();
        s.insert_incoming(b, &env("p1", "pat.222222"), "t1").unwrap();
        let (mut ids, lids) = s.forget_chat("maya.111111", &[b.to_string()], "t2").unwrap();
        ids.sort();
        assert_eq!(ids, ["c1", "c2"]);
        assert_eq!(lids.len(), 2);
        let mut owed = s.pending_deletes(b).unwrap();
        owed.sort();
        assert_eq!(owed, ["c1", "c2"]);
        assert!(s.chat("maya.111111", None, None, 10).unwrap().is_empty());
        assert!(s.message("p1").unwrap().is_some());
        // maya writes again: the new message stays, and isn't owed
        s.insert_incoming(b, &env("c3", "maya.111111"), "t3").unwrap();
        assert_eq!(s.pending_deletes(b).unwrap().len(), 2);
        assert!(s.message("c3").unwrap().is_some());
    }

    /// Review finding 4: a delete the hub can't take now waits longer each
    /// time, and doesn't hold up the others.
    #[test]
    fn a_failing_delete_backs_off_without_blocking_the_rest() {
        let s = Store::open_in_memory().unwrap();
        let b = "http://b:7370";
        s.add_hub(b, "t0").unwrap();
        s.queue_deletes(b, &["d1".into(), "d2".into()], "t1").unwrap();
        assert_eq!(s.due_deletes(b, 1000).unwrap().len(), 2);
        s.delete_later(b, "d1", 1000).unwrap();
        assert_eq!(s.due_deletes(b, 1000).unwrap(), ["d2"]);
        assert_eq!(s.due_deletes(b, 31_000).unwrap(), ["d2", "d1"]);
        s.delete_later(b, "d1", 31_000).unwrap(); // second miss: 60 s
        assert_eq!(s.due_deletes(b, 90_999).unwrap(), ["d2"]);
        assert_eq!(s.due_deletes(b, 91_000).unwrap(), ["d2", "d1"]);
        for _ in 0..20 {
            s.delete_later(b, "d1", 0).unwrap();
        }
        assert!(s.due_deletes(b, 3_599_999).unwrap() == ["d2"], "an hour at most");
        assert_eq!(s.due_deletes(b, 3_600_000).unwrap(), ["d2", "d1"]);
        assert_eq!(s.pending_deletes(b).unwrap().len(), 2);
    }

    /// Review finding 3: a message deleted while it was being sent.
    #[test]
    fn a_send_landing_after_a_delete_says_so() {
        let s = Store::open_in_memory().unwrap();
        let b = "http://b:7370";
        s.add_hub(b, "t0").unwrap();
        let out = NewOutgoing {
            id: "o1".into(),
            peer: "maya.111111".into(),
            body: "hi".into(),
            kind: None,
            reply_to: None,
            attachments: vec![],
        };
        s.queue_outgoing(&out, "t1").unwrap();
        assert!(s.mark_sent("o1", b, "t2").unwrap());
        s.forget_message("o1", &[], "t3").unwrap();
        assert!(!s.mark_sent("o1", b, "t4").unwrap());
        assert!(s.message("o1").unwrap().is_none());
    }

    /// Review finding 5: attachment ids belong to the hub they came from;
    /// another holder's ids replace them once that hub no longer holds it.
    #[test]
    fn attachment_ids_follow_a_hub_that_holds_the_message() {
        let s = Store::open_in_memory().unwrap();
        let (a, b) = ("http://a:7370", "http://b:7370");
        for h in [a, b] {
            s.add_hub(h, "t0").unwrap();
        }
        let me = "me.000000";
        let on = |id: &str| {
            let mut m = synced("m1");
            m.env.attachments[0].id = id.into();
            m
        };
        s.upsert_synced(a, me, &on("fa")).unwrap();
        s.upsert_synced(b, me, &on("fb")).unwrap();
        let att = |s: &Store| {
            let a = s.message("m1").unwrap().unwrap().attachments.remove(0);
            (a.hub_id.unwrap(), a.hub.unwrap())
        };
        assert_eq!(att(&s), ("fa".to_string(), a.to_string()), "A still holds it");
        // A starts over: the message stays (B), labelled B, ids still A's
        // until a hub that holds it says otherwise
        s.forget_hub_messages(a).unwrap();
        assert_eq!(s.message("m1").unwrap().unwrap().hub.as_deref(), Some(b));
        s.upsert_synced(b, me, &on("fb")).unwrap();
        assert_eq!(att(&s), ("fb".to_string(), b.to_string()));
        // A has it again: B's ids stay (B holds it)
        s.upsert_synced(a, me, &on("fa")).unwrap();
        assert_eq!(att(&s), ("fb".to_string(), b.to_string()));
        // an upload records its hub
        let lid = s.message("m1").unwrap().unwrap().attachments[0].local_id.clone();
        s.set_upload(&lid, "uploaded", "up1", a).unwrap();
        assert_eq!(att(&s), ("up1".to_string(), a.to_string()));
    }

    /// Queued outgoing messages never leave when a hub starts over.
    #[test]
    fn queued_messages_survive_a_hub_starting_over() {
        let s = Store::open_in_memory().unwrap();
        let a = "http://a:7370";
        s.add_hub(a, "t0").unwrap();
        let out = NewOutgoing {
            id: "q1".into(),
            peer: "maya.111111".into(),
            body: "waiting".into(),
            kind: None,
            reply_to: None,
            attachments: vec![],
        };
        s.queue_outgoing(&out, "t1").unwrap();
        s.upsert_synced(a, "me.000000", &synced("s1")).unwrap();
        s.forget_hub_messages(a).unwrap();
        assert!(s.message("q1").unwrap().is_some());
        assert!(s.message("s1").unwrap().is_none());
    }

    /// A store from before the multi-hub fixes (main's schema, with data)
    /// is brought up once: holders and attachment hubs from the old label.
    #[test]
    fn an_existing_store_is_migrated_once() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("hubchat.db");
        {
            let c = Connection::open(&path).unwrap();
            c.execute_batch(SCHEMA).unwrap();
            c.execute_batch(
                "INSERT INTO hubs(url, added_at) VALUES('http://a:7370', 't0');
                 INSERT INTO messages(id, peer, outgoing, hub, body, created_at, state)
                   VALUES('old1', 'maya.111111', 0, 'http://a:7370', 'hi', 't1', 'received');
                 INSERT INTO messages(id, peer, outgoing, body, created_at, state)
                   VALUES('old2', 'maya.111111', 1, 'queued', 't2', 'queued');
                 INSERT INTO attachments(local_id, message_id, position, hub_id, name, bytes, state)
                   VALUES('l1', 'old1', 0, 'f1', 'f.txt', 3, 'remote');",
            )
            .unwrap();
        }
        let version = |s: &Store| -> usize {
            s.with(|c| c.query_row("PRAGMA user_version", [], |r| r.get(0))).unwrap()
        };
        let s = Store::open(&path).unwrap();
        assert_eq!(version(&s), MIGRATIONS.len());
        assert_eq!(s.message_hubs("old1").unwrap(), ["http://a:7370"]);
        assert!(s.message_hubs("old2").unwrap().is_empty());
        let att = s.message("old1").unwrap().unwrap().attachments.remove(0);
        assert_eq!(att.hub.as_deref(), Some("http://a:7370"));
        s.queue_deletes("http://a:7370", &["x".into()], "t3").unwrap();
        drop(s);
        // opened again: nothing runs twice, nothing is lost
        let s = Store::open(&path).unwrap();
        assert_eq!(version(&s), MIGRATIONS.len());
        assert_eq!(s.pending_deletes("http://a:7370").unwrap(), ["x"]);
        // a wipe (another identity) doesn't run the migration again
        s.wipe().unwrap();
        drop(s);
        let s = Store::open(&path).unwrap();
        assert_eq!(version(&s), MIGRATIONS.len());
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
    fn history_marks_are_per_hub_and_chat_and_go_when_the_hub_starts_over() {
        let s = Store::open_in_memory().unwrap();
        let (a, b) = ("http://a:7370", "http://b:7370");
        for h in [a, b] {
            s.add_hub(h, "t0").unwrap();
        }
        assert_eq!(s.history_mark(a, "pat").unwrap(), None);
        let m = HistoryMark {
            before: Some("1760000000123-7".into()),
            oldest_ms: Some(1760000000123),
            done: false,
            old_unread: 0,
            listed_id: None,
        };
        s.set_history_mark(a, "pat", &m).unwrap();
        s.set_history_mark(b, "pat", &HistoryMark { done: true, ..Default::default() })
            .unwrap();
        assert_eq!(s.history_mark(a, "pat").unwrap(), Some(m));
        assert_eq!(s.history_mark(a, "sam").unwrap(), None);
        // a hub starting over pages again; the other hub's mark stays
        s.forget_hub_messages(a).unwrap();
        assert_eq!(s.history_mark(a, "pat").unwrap(), None);
        assert!(s.history_mark(b, "pat").unwrap().unwrap().done);
        s.remove_hub(b).unwrap();
        assert_eq!(s.history_mark(b, "pat").unwrap(), None);
    }

    fn synced_at(id: &str, received_at: &str, reply_to: Option<&str>) -> SyncedMessage {
        let mut m = synced(id);
        m.env.received_at = received_at.into();
        m.env.reply_to = reply_to.map(Into::into);
        m.env.attachments.clear();
        m
    }

    fn created(s: &Store, id: &str) -> String {
        s.message(id).unwrap().unwrap().created_at
    }

    /// Lazy history ordering: hub times on our clock, a message on two hubs
    /// at the earliest of its times, a reply never above its parent.
    #[test]
    fn messages_sort_by_corrected_hub_time_earliest_copy_and_after_their_parent() {
        let s = Store::open_in_memory().unwrap();
        let (a, b) = ("http://a:7370", "http://b:7370");
        for h in [a, b] {
            s.add_hub(h, "t0").unwrap();
        }
        let me = "me.000000";
        // hub A runs 5 s fast: its 10:00:05 is our 10:00:00
        s.upsert_synced_at(a, me, &synced_at("m1", "2026-10-08T10:00:05.000Z", None), 5000)
            .unwrap();
        assert_eq!(created(&s, "m1"), "2026-10-08T10:00:00.000Z");
        // the same message from hub B (on time) a second later: the earliest stays
        s.upsert_synced_at(b, me, &synced_at("m1", "2026-10-08T10:00:01.000Z", None), 0)
            .unwrap();
        assert_eq!(created(&s, "m1"), "2026-10-08T10:00:00.000Z");
        // and earlier through hub B: that is the time now
        s.upsert_synced_at(b, me, &synced_at("m1", "2026-10-08T09:59:59.500Z", None), 0)
            .unwrap();
        assert_eq!(created(&s, "m1"), "2026-10-08T09:59:59.500Z");
        // a reply whose hub's clock puts it before its parent goes after it
        s.upsert_synced_at(b, me, &synced_at("r1", "2026-10-08T09:59:58.000Z", Some("m1")), 0)
            .unwrap();
        assert_eq!(created(&s, "r1"), "2026-10-08T09:59:59.501Z");
        // a reply stored before its parent moves when the parent comes
        s.upsert_synced_at(a, me, &synced_at("r2", "2026-10-08T08:00:00.000Z", Some("p2")), 0)
            .unwrap();
        s.upsert_synced_at(a, me, &synced_at("p2", "2026-10-08T08:00:00.000Z", None), 0)
            .unwrap();
        assert_eq!(created(&s, "r2"), "2026-10-08T08:00:00.001Z");
        // a reply to a reply, both stored before the first message: the chain follows
        s.upsert_synced_at(a, me, &synced_at("g3", "2026-10-08T06:00:00.000Z", Some("c3")), 0)
            .unwrap();
        s.upsert_synced_at(a, me, &synced_at("c3", "2026-10-08T06:00:00.000Z", Some("p3")), 0)
            .unwrap();
        s.upsert_synced_at(a, me, &synced_at("p3", "2026-10-08T06:30:00.000Z", None), 0)
            .unwrap();
        assert_eq!(created(&s, "c3"), "2026-10-08T06:30:00.001Z");
        assert_eq!(created(&s, "g3"), "2026-10-08T06:30:00.002Z");
        // equal times: by id
        s.upsert_synced_at(a, me, &synced_at("t-b", "2026-10-08T07:00:00.000Z", None), 0)
            .unwrap();
        s.upsert_synced_at(a, me, &synced_at("t-a", "2026-10-08T07:00:00.000Z", None), 0)
            .unwrap();
        let ids: Vec<String> = s
            .chat("maya.111111", None, None, 10)
            .unwrap()
            .into_iter()
            .map(|m| m.id)
            .collect();
        assert_eq!(ids, ["p3", "c3", "g3", "t-a", "t-b", "p2", "r2", "m1", "r1"]);
    }

    /// A hub's chat list counts unread messages not loaded here: they count
    /// as unread until loaded or the chat is read.
    #[test]
    fn unread_counts_what_a_hub_lists_but_isnt_loaded() {
        let s = Store::open_in_memory().unwrap();
        let (a, b) = ("http://a:7370", "http://b:7370");
        for h in [a, b] {
            s.add_hub(h, "t0").unwrap();
        }
        let peer = "maya.111111";
        assert!(s.upsert_synced(a, "me.000000", &synced_at("n1", "2026-10-08T10:00:00.000Z", None)).unwrap());
        s.set_old_unread(a, peer, 4, Some("n1")).unwrap();
        // each hub's own (a message both hold counts twice until loaded)
        s.set_old_unread(b, peer, 3, None).unwrap();
        assert_eq!(s.unread(peer).unwrap(), 8);
        assert_eq!(s.chats().unwrap()[0].unread, 8);
        // setting the count keeps the paging mark
        assert_eq!(s.history_mark(a, peer).unwrap().unwrap().old_unread, 4);
        s.mark_seen(peer, "t9").unwrap();
        assert_eq!(s.unread(peer).unwrap(), 0);
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
