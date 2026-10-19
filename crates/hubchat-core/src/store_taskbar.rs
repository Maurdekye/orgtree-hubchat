//! Taskbar attention uses persisted chat state, independent of loaded UI pages.
use super::*;

const ATTENTION_SQL: &str = "SELECT EXISTS(SELECT 1 FROM messages WHERE outgoing=0 AND seen=0)
    OR EXISTS(SELECT 1 FROM history_marks WHERE old_unread>0)
    OR EXISTS(SELECT 1 FROM taskbar_questions WHERE pending=1)";

impl Store {
    pub fn taskbar_attention(&self) -> Result<bool> {
        self.with(|c| c.query_row(ATTENTION_SQL, [], |r| r.get(0)))
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
        add(&s, "second-answer", true, PEER, serde_json::json!({"reply_to":"q"}));
        s.delete_message("skip").unwrap();
        assert!(!s.taskbar_attention().unwrap(), "another retained answer still settles the card");
        s.delete_message("second-answer").unwrap();
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
    #[test]
    fn taskbar_upgrade_backfills_existing_read_questions_and_answers() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("fixture.sqlite3");
        let s = Store {con:std::sync::Mutex::new(legacy_schema_fixture(&path,18))};
        s.add_hub(HUB, "2026-10-19T00:00:00.000Z").unwrap();
        add(&s, "answered", false, PEER, serde_json::json!({"kind":"question"}));
        add(&s, "answer", true, PEER, serde_json::json!({"reply_to":"answered"}));
        add(&s, "pending", false, PEER, serde_json::json!({"kind":"question"}));
        // Reopen the genuine version18 schema with its retained message data.
        drop(s);
        let s = Store::open(&path).unwrap();
        assert!(s.taskbar_attention().unwrap());
        s.delete_message("pending").unwrap();
        assert!(!s.taskbar_attention().unwrap(), "already answered history was backfilled as settled");
        // Answers can precede the question in a historical import too.
        add(&s, "ahead-answer", true, PEER, serde_json::json!({"reply_to":"later-card"}));
        add(&s, "later-card", false, PEER, serde_json::json!({"kind":"question"}));
        assert!(!s.taskbar_attention().unwrap());
    }
    #[test]
    fn taskbar_reads_only_active_reason_indexes_without_loading_history() {
        let s = store();
        let plan: Vec<String> = s.with(|c| {
            let mut q = c.prepare(&format!("EXPLAIN QUERY PLAN {ATTENTION_SQL}"))?;
            let rows = q.query_map([], |r| r.get(3))?.collect();
            rows
        }).unwrap();
        for index in ["taskbar_unread", "taskbar_old_unread", "taskbar_questions_pending"] {
            assert!(plan.iter().any(|line|line.contains(index)), "{index}: {plan:?}");
        }
    }

    fn at(s: &Store, id: &str, outgoing: bool, peer: &str, second: u8, question: bool) {
        add(s, id, outgoing, peer, serde_json::json!({
            "received_at":format!("2026-10-19T00:00:{second:02}.000Z"),
            "kind":if question {Some("question")} else {None},
        }));
    }
    #[test]
    fn taskbar_later_ordinary_message_settles_every_earlier_question_in_its_chat() {
        let s = store();
        at(&s,"q1",false,PEER,1,true);
        at(&s,"q2",false,PEER,2,true);
        at(&s,"incoming",false,PEER,3,false);
        at(&s,"wrong-chat",true,"kim.abcdef",4,false);
        assert!(s.taskbar_attention().unwrap());
        let before=s.taskbar_attention().unwrap();
        at(&s,"ordinary",true,PEER,5,false);
        assert!(!s.taskbar_attention().unwrap(),"a later ordinary message deals with both questions");
        let after=s.taskbar_attention().unwrap();
        assert_eq!(s.card_answers(PEER,&["q1".into()]).unwrap().len(),1,"attention does not invent a structured answer");
        at(&s,"future",false,PEER,6,true);
        assert!(s.taskbar_attention().unwrap(),"earlier outgoing messages do not settle newer questions");
        if let Ok(path)=std::env::var("HUBCHAT_TASKBAR_CASES") {
            std::fs::write(path,serde_json::to_vec(&serde_json::json!([
                ["Read questions awaiting reply",before],
                ["Later message sent: cleared",after],
                ["Newer question: attention",s.taskbar_attention().unwrap()]
            ])).unwrap()).unwrap();
        }
    }
    #[test]
    fn taskbar_queued_later_message_clears_and_deletion_reopens_only_without_another_answer() {
        let s = store();
        at(&s,"q",false,PEER,1,true);
        s.queue_outgoing(&NewOutgoing{id:"queued".into(),peer:PEER.into(),body:"".into(),kind:None,reply_to:None,attachments:vec![]},"2026-10-19T00:00:02.000Z").unwrap();
        assert!(!s.taskbar_attention().unwrap(),"even an attachment-only/empty later send settles attention");
        s.delete_message("queued").unwrap();
        assert!(s.taskbar_attention().unwrap());
        at(&s,"ordinary",true,PEER,3,false);
        add(&s,"explicit",true,PEER,serde_json::json!({"reply_to":"q"}));
        s.delete_message("ordinary").unwrap();
        assert!(!s.taskbar_attention().unwrap(),"an explicit answer still settles regardless of timestamp");
        s.delete_message("explicit").unwrap();
        assert!(s.taskbar_attention().unwrap());
    }
    #[test]
    fn taskbar_history_import_and_retiming_keep_later_message_settlement_correct() {
        let s = store();
        at(&s,"later",true,PEER,5,false);
        at(&s,"old-question",false,PEER,2,true);
        assert!(!s.taskbar_attention().unwrap(),"a question imported after its later outgoing message stays settled");
        at(&s,"later",true,PEER,1,false);
        assert!(s.taskbar_attention().unwrap(),"sync retimes the outgoing message before the question");
        at(&s,"old-question",false,PEER,0,true);
        assert!(!s.taskbar_attention().unwrap(),"retiming a question refreshes its attention too");
    }
    #[test]
    fn taskbar_later_send_does_not_clear_unread_and_reply_link_removal_retains_later_settlement() {
        let s = store();
        at(&s,"q",false,PEER,1,true);
        add(&s,"unread",false,PEER,serde_json::json!({"read_at":null}));
        at(&s,"ordinary",true,PEER,5,false);
        s.with(|c| {c.execute("INSERT INTO reply_links(target,message_id) VALUES('q','ordinary')",[])?;
            c.execute("DELETE FROM reply_links WHERE target='q'",[])?;Ok(())}).unwrap();
        assert!(s.taskbar_attention().unwrap(),"unread stays an independent reason");
        s.mark_seen(PEER,"2026-10-19T00:01:00.000Z").unwrap();
        assert!(!s.taskbar_attention().unwrap(),"removing a reply link cannot undo later-message settlement");
    }
    #[test]
    fn taskbar_version19_upgrade_clears_existing_later_replies() {
        let dir=tempfile::tempdir().unwrap();let path=dir.path().join("legacy.sqlite3");
        let c=legacy_schema_fixture(&path,19);
        for i in 0..18 {
            c.execute("INSERT INTO messages(id,peer,outgoing,body,kind,state,created_at,seen)
                VALUES(?1,?2,0,'','question','received','2026-10-19T00:00:01.000Z',1)",params![format!("q{i}"),format!("peer{i}")]).unwrap();
            c.execute("INSERT INTO messages(id,peer,outgoing,body,state,created_at,seen)
                VALUES(?1,?2,1,'','sent','2026-10-19T00:00:02.000Z',1)",params![format!("a{i}"),format!("peer{i}")]).unwrap();
        }
        let pending:u64=c.query_row("SELECT count(*) FROM taskbar_questions WHERE pending=1",[],|r|r.get(0)).unwrap();
        assert_eq!(pending,18,"real version19 triggers reproduce the stuck attention");drop(c);
        let s=Store::open(&path).unwrap();
        assert!(!s.taskbar_attention().unwrap(),"upgrade backfills all eighteen without changing read state");
        at(&s,"new-question",false,PEER,3,true);assert!(s.taskbar_attention().unwrap());
    }
}
