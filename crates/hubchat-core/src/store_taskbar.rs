//! Taskbar attention uses persisted chat state, independent of loaded UI pages.
use super::*;

impl Store {
    pub fn taskbar_attention(&self) -> Result<bool> {
        self.tray_overview().map(|(_, unread)| unread)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const HUB: &str = "http://scratch.invalid:7370";
    const ME: &str = "me.000000";
    const PEER: &str = "pat.4d5e6f";
    fn store() -> Store {
        let s = Store::open_in_memory().unwrap();
        s.add_hub(HUB, "2026-10-19T00:00:00.000Z").unwrap();
        s
    }
    fn add(s: &Store, id: &str, outgoing: bool, peer: &str, extra: serde_json::Value) {
        let (from, to) = if outgoing { (ME, peer) } else { (peer, ME) };
        let mut v = serde_json::json!({"id":id,"from":from,"to":to,"body":"", "received_at":"2026-10-19T00:00:00.000Z","read_at":"2026-10-19T00:01:00.000Z"});
        v.as_object_mut().unwrap().extend(extra.as_object().unwrap().clone());
        s.upsert_synced(HUB, ME, &serde_json::from_value(v).unwrap()).unwrap();
    }
    #[test]
    fn taskbar_unread_includes_closed_and_history_only_chats_and_clears() {
        let s = store();
        assert!(!s.taskbar_attention().unwrap());
        add(&s, "unread", false, PEER, serde_json::json!({"read_at":null}));
        s.close_dm(PEER).unwrap();
        assert!(s.chats().unwrap().is_empty());
        assert!(s.taskbar_attention().unwrap());
        s.mark_seen(PEER, "2026-10-19T00:02:00.000Z").unwrap();
        assert!(!s.taskbar_attention().unwrap());
        s.set_old_unread(HUB, "remote.abcdef", 3, None).unwrap();
        assert!(s.taskbar_attention().unwrap());
        s.set_old_unread(HUB, "remote.abcdef", 0, None).unwrap();
        assert!(!s.taskbar_attention().unwrap());
    }
    #[test]
    fn taskbar_read_incoming_question_needs_attention_until_an_outgoing_answer() {
        let s = store();
        add(&s, "q", false, PEER, serde_json::json!({"kind":"question"}));
        assert_eq!(s.unread(PEER).unwrap(), 0);
        s.close_dm(PEER).unwrap();
        assert!(s.taskbar_attention().unwrap(), "read/closed is not answered");
        add(&s, "self", false, PEER, serde_json::json!({"reply_to":"q"}));
        add(&s, "wrong-chat", true, "kim.abcdef", serde_json::json!({"reply_to":"q"}));
        assert!(s.taskbar_attention().unwrap(), "only the other side in this chat settles a question");
        add(&s, "skip", true, PEER, serde_json::json!({"reply_to":"q","body":""}));
        assert!(!s.taskbar_attention().unwrap(), "empty skip settles just as card_answers does");
        s.delete_message("skip").unwrap();
        assert!(s.taskbar_attention().unwrap(), "deleted answer reopens the card");
        s.delete_message("q").unwrap();
        assert!(!s.taskbar_attention().unwrap());
    }
    #[test]
    fn taskbar_multi_reply_and_other_device_answers_clear_all_questions() {
        let s = store();
        add(&s, "ours", true, PEER, serde_json::json!({"kind":"question"}));
        assert!(!s.taskbar_attention().unwrap(), "our questions await somebody else");
        for id in ["q1", "q2"] { add(&s, id, false, PEER, serde_json::json!({"kind":"question"})); }
        assert!(s.taskbar_attention().unwrap());
        add(&s, "answer", true, PEER, serde_json::json!({"reply_to":"q1","meta":{"replies":["q1","q2"]}}));
        assert!(!s.taskbar_attention().unwrap());
        s.delete_message("answer").unwrap();
        assert!(s.taskbar_attention().unwrap());
        s.wipe().unwrap();
        assert!(!s.taskbar_attention().unwrap(), "identity wipe clears every reason");
    }
    #[test]
    fn taskbar_queued_answer_uses_the_same_settlement_as_cards() {
        let s = store();
        add(&s, "q", false, PEER, serde_json::json!({"kind":"question"}));
        assert!(s.taskbar_attention().unwrap());
        s.queue_outgoing(&NewOutgoing {id:"local".into(),peer:PEER.into(),body:"answer".into(),kind:None,reply_to:Some("q".into()),attachments:vec![]}, "2026-10-19T00:03:00.000Z").unwrap();
        assert_eq!(s.card_answers(PEER, &["q".into()]).unwrap().len(), 2);
        assert!(!s.taskbar_attention().unwrap());
    }
}
