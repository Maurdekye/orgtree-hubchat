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
        self.with(|c| c.execute("DELETE FROM hubs WHERE url=?", [url]).map(|_| ()))
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

    /// Hubs whose roster lists `address`.
    pub fn hubs_reaching(&self, address: &str) -> Result<Vec<String>> {
        self.with(|c| {
            let mut st =
                c.prepare("SELECT hub FROM roster WHERE address=? ORDER BY online DESC, hub")?;
            let rows = st.query_map([address], |r| r.get(0))?;
            rows.collect()
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
            c.execute(
                "UPDATE messages SET state=CASE WHEN state IN ('queued','sending','failed') THEN 'sent' ELSE state END,
                 hub=?, received_at=?, error=NULL WHERE id=?",
                params![hub, received_at, id],
            )
            .map(|_| ())
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
    pub fn chat(&self, peer: &str, before: Option<&str>, limit: u32) -> Result<Vec<Message>> {
        let mut v = self.messages_where(
            "m.peer=? AND (? IS NULL OR m.created_at < ?) ORDER BY m.created_at DESC LIMIT ?",
            params![peer, before, before, limit],
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
            if let Some(last) = self.chat(&peer, None, 1)?.pop() {
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
                    tx.execute(
                        "UPDATE messages SET hub=?, received_at=COALESCE(received_at, ?),
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
            tx.commit()?;
            Ok(fresh && !outgoing && m.read_at.is_none())
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
    /// from it so the device rebuilds its copy.
    pub fn forget_hub_messages(&self, hub: &str) -> Result<()> {
        self.with(|c| {
            c.execute(
                "DELETE FROM messages WHERE hub=? AND state<>'queued'",
                [hub],
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
}
