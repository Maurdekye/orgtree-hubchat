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

impl Store {
    pub fn draft_answers(&self, peer: &str) -> Result<Option<DraftAnswers>> {
        let s: Option<String> = self.with(|c| c.query_row("SELECT data FROM draft_answers WHERE peer=?", [peer], |r| r.get(0)).optional())?;
        Ok(s.and_then(|s| serde_json::from_str(&s).ok()))
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
}
