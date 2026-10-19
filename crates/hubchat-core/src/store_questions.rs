//! A chat's unsent answers (docs/question-cards.md): what the next message
//! replies to and the options picked on cards. They survive a restart (user
//! 2026-10-11 22:42Z), so they live beside the draft.
use super::*;
use std::collections::BTreeMap;

/// What the composer holds for one chat until its next message goes.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct DraftAnswers {
    #[serde(default)]
    pub replies: Vec<String>,
    /// Card id to the labels picked on it.
    #[serde(default)]
    pub picks: BTreeMap<String, Vec<String>>,
}

/// What one chat may hold (a bound on what the UI hands in, not a rule the
/// user meets).
const MAX_HELD: usize = 200;

/// How many cards one `card_answers` call settles: a bound on what the UI
/// asks for (it names the cards it holds answers for first).
pub const MAX_ASKED: usize = 1000;

/// Hub order, as src/lib/questions.ts sorts answers: the hub's receipt time,
/// then the id; what no hub has received yet comes after, in the order it
/// was written.
const HUB_ORDER: &str = "NULLIF(m.received_at,'') IS NULL, NULLIF(m.received_at,''),
    CASE WHEN NULLIF(m.received_at,'') IS NULL THEN m.created_at END, m.id";

impl Store {
    pub fn draft_answers(&self, peer: &str) -> Result<Option<DraftAnswers>> {
        let s: Option<String> = self.with(|c| c.query_row("SELECT data FROM draft_answers WHERE peer=?", [peer], |r| r.get(0)).optional())?;
        Ok(s.and_then(|s| serde_json::from_str(&s).ok()))
    }

    /// Question cards and what settled them, however far back they are
    /// (docs/question-cards.md): each of `ids` that is a card in this chat,
    /// then the first message from the other side that replies to it (by
    /// reply_to, or as one of a multi-reply's messages). The UI settles cards
    /// from these and the messages it has loaded, so a card or its answer
    /// beyond the loaded page still counts. Each lookup is by index.
    pub fn card_answers(&self, peer: &str, ids: &[String]) -> Result<Vec<Message>> {
        let found: Vec<String> = self.with(|c| {
            let mut card = c.prepare_cached("SELECT outgoing FROM messages WHERE id=?1 AND peer=?2 AND kind='question'")?;
            let mut first = c.prepare_cached(&format!(
                "SELECT m.id FROM messages m
                 WHERE m.id IN (SELECT id FROM messages WHERE reply_to=?1 UNION SELECT message_id FROM reply_links WHERE target=?1)
                   AND m.peer=?2 AND m.outgoing<>?3
                 ORDER BY {HUB_ORDER} LIMIT 1"
            ))?;
            let mut out = Vec::new();
            for id in ids.iter().take(MAX_ASKED) {
                let Some(outgoing) = card.query_row(params![id, peer], |r| r.get::<_, bool>(0)).optional()? else {
                    continue;
                };
                out.push(id.clone());
                out.extend(first.query_row(params![id, peer, outgoing], |r| r.get::<_, String>(0)).optional()?);
            }
            Ok(out)
        })?;
        let mut seen = std::collections::HashSet::new();
        let mut msgs = Vec::new();
        for id in found.iter().filter(|id| seen.insert(id.as_str())) {
            msgs.extend(self.message(id)?);
        }
        Ok(msgs)
    }

    pub fn set_draft_answers(&self, peer: &str, d: &DraftAnswers) -> Result<()> {
        if d.replies.len() > MAX_HELD || d.picks.len() > MAX_HELD || d.picks.values().any(|p| p.len() > crate::questions::MAX_OPTIONS) {
            return Err(Error::Invalid("too many replies or answers at once".into()));
        }
        self.with(|c| {
            if d.replies.is_empty() && d.picks.is_empty() {
                c.execute("DELETE FROM draft_answers WHERE peer=?", [peer]).map(|_| ())
            } else {
                let data = serde_json::to_string(d).unwrap_or_default();
                c.execute("INSERT INTO draft_answers(peer, data) VALUES(?,?) ON CONFLICT(peer) DO UPDATE SET data=excluded.data", params![peer, data])
                    .map(|_| ())
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unsent_answers_survive_a_reopen_and_go_when_cleared() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("s.sqlite3");
        let d = DraftAnswers { replies: vec!["m1".into(), "m2".into()], picks: BTreeMap::from([("q1".to_string(), vec!["Beta".to_string()])]) };
        {
            let s = Store::open(&path).unwrap();
            assert_eq!(s.draft_answers("pat.4d5e6f").unwrap(), None);
            s.set_draft_answers("pat.4d5e6f", &d).unwrap();
        }
        let s = Store::open(&path).unwrap();
        assert_eq!(s.draft_answers("pat.4d5e6f").unwrap(), Some(d));
        s.set_draft_answers("pat.4d5e6f", &DraftAnswers::default()).unwrap();
        assert_eq!(s.draft_answers("pat.4d5e6f").unwrap(), None);
        let many = DraftAnswers { replies: vec!["m".into(); MAX_HELD + 1], picks: BTreeMap::new() };
        assert!(s.set_draft_answers("pat.4d5e6f", &many).is_err());
    }

    const HUB: &str = "http://scratch.invalid:7370";
    const ME: &str = "me.000000";
    const PAT: &str = "pat.4d5e6f";

    /// A message as a hub's sync gives it: from Pat, or ours from any of our
    /// devices.
    fn synced(s: &Store, id: &str, ours: bool, received_at: &str, extra: serde_json::Value) {
        let (from, to) = if ours { (ME, PAT) } else { (PAT, ME) };
        let mut v = serde_json::json!({ "id": id, "from": from, "to": to, "body": "", "received_at": received_at });
        v.as_object_mut().unwrap().extend(extra.as_object().unwrap().clone());
        let m: SyncedMessage = serde_json::from_value(v).unwrap();
        s.upsert_synced(HUB, ME, &m).unwrap();
    }

    fn ids(v: &[Message]) -> Vec<&str> {
        v.iter().map(|m| m.id.as_str()).collect()
    }

    fn unsent(s: &Store, id: &str, reply_to: &str, at: &str) {
        let m = NewOutgoing { id: id.into(), peer: PAT.into(), body: "x".into(), kind: None, reply_to: Some(reply_to.into()), attachments: vec![] };
        s.queue_outgoing(&m, at).unwrap();
    }

    #[test]
    fn a_cards_first_answer_is_found_however_far_back() {
        let dir = tempfile::tempdir().unwrap();
        let s = Store::open(&dir.path().join("s.sqlite3")).unwrap();
        s.add_hub(HUB, "t0").unwrap();
        let card = |s: &Store, id: &str, at: &str| synced(s, id, false, at, serde_json::json!({ "kind": "question", "body": "Which?" }));
        card(&s, "q1", "2026-10-12T09:00:00.000Z");
        // Pat's own reply doesn't answer Pat's card; ours received later does
        synced(&s, "pat-own", false, "2026-10-12T09:01:00.000Z", serde_json::json!({ "reply_to": "q1", "body": "hm" }));
        synced(&s, "late", true, "2026-10-12T09:30:00.000Z", serde_json::json!({ "reply_to": "q1", "body": "late" }));
        synced(&s, "first", true, "2026-10-12T09:10:00.000Z", serde_json::json!({ "reply_to": "q1", "body": "first" }));
        // hundreds of later messages: nothing about paging matters here
        for i in 0..300 {
            let at = format!("2026-10-12T{:02}:{:02}:00.000Z", 10 + i / 60, i % 60);
            synced(&s, &format!("chat-{i:03}"), i % 2 == 0, &at, serde_json::json!({ "body": "chat" }));
        }
        assert_eq!(ids(&s.card_answers(PAT, &["q1".into()]).unwrap()), ["q1", "first"]);
        // a multi-reply where the card isn't the first message it answers
        card(&s, "q2", "2026-10-12T09:02:00.000Z");
        let multi = serde_json::json!({ "reply_to": "chat-001", "meta": { "replies": ["chat-001", "q2"] } });
        synced(&s, "multi", true, "2026-10-12T09:20:00.000Z", multi);
        // the same received time: the lower id first
        card(&s, "q3", "2026-10-12T09:03:00.000Z");
        synced(&s, "tie-b", true, "2026-10-12T09:40:00.000Z", serde_json::json!({ "reply_to": "q3", "body": "b" }));
        synced(&s, "tie-a", true, "2026-10-12T09:40:00.000Z", serde_json::json!({ "reply_to": "q3", "body": "a" }));
        // a card nobody answered, an ordinary message, an unknown id, another chat
        card(&s, "q4", "2026-10-12T09:04:00.000Z");
        let asked: Vec<String> = ["q1", "q2", "q3", "q4", "chat-001", "nope"].map(String::from).into();
        assert_eq!(ids(&s.card_answers(PAT, &asked).unwrap()), ["q1", "first", "q2", "multi", "q3", "tie-a", "q4"]);
        assert!(s.card_answers("kim.19ac02", &asked).unwrap().is_empty());
        // what no hub has yet comes after every received answer, in writing order
        card(&s, "q5", "2026-10-12T09:05:00.000Z");
        unsent(&s, "unsent-2", "q5", "2026-10-12T08:00:00.000Z");
        unsent(&s, "unsent-1", "q5", "2026-10-12T08:00:01.000Z");
        assert_eq!(ids(&s.card_answers(PAT, &["q5".into()]).unwrap()), ["q5", "unsent-2"]);
        synced(&s, "on-hub", true, "2026-10-12T23:00:00.000Z", serde_json::json!({ "reply_to": "q5", "body": "z" }));
        assert_eq!(ids(&s.card_answers(PAT, &["q5".into()]).unwrap()), ["q5", "on-hub"]);
        // a deleted answer takes its links along: the next one settles the card
        s.delete_message("multi").unwrap();
        assert_eq!(ids(&s.card_answers(PAT, &["q2".into()]).unwrap()), ["q2"]);
        s.delete_message("first").unwrap();
        assert_eq!(ids(&s.card_answers(PAT, &["q1".into()]).unwrap()), ["q1", "late"]);
        let links: i64 = s.with(|c| c.query_row("SELECT COUNT(*) FROM reply_links", [], |r| r.get(0))).unwrap();
        assert_eq!(links, 0);
    }

    #[test]
    fn migration_16_keeps_version_15_history() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("s.sqlite3");
        let c = super::super::legacy_schema_fixture(&path, 15);
        c.execute("INSERT INTO hubs(url,added_at) VALUES(?1,'t0')", [HUB]).unwrap();
        c.execute("INSERT INTO messages(id,peer,outgoing,hub,body,reply_to,created_at,state)
            VALUES('old',?1,0,?2,'kept','q1','t1','received')", params![PAT,HUB]).unwrap();
        c.execute("INSERT INTO attachments(local_id,message_id,position,hub_id,hub,name,bytes,state)
            VALUES('fixture-file','old',0,'f1',?1,'kept.txt',4,'remote')", [HUB]).unwrap();
        drop(c);
        let s = Store::open(&path).unwrap();
        let m = s.message("old").unwrap().unwrap();
        assert_eq!((m.body.as_str(), m.reply_to.as_deref(), m.attachments[0].name.as_str()), ("kept", Some("q1"), "kept.txt"));
        assert!(m.qa.is_empty());
        s.set_draft_answers("pat.4d5e6f", &DraftAnswers { replies: vec!["old".into()], picks: BTreeMap::new() }).unwrap();
        assert_eq!(s.draft_answers("pat.4d5e6f").unwrap().unwrap().replies, ["old"]);
    }
}
