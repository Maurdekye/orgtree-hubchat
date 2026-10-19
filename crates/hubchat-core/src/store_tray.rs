//! Bounded tray metadata: no message body or attachment leaves this query.
use super::*;

#[derive(Clone, Debug, PartialEq)]
pub struct TrayChat {
    pub peer: String,
    pub unread: u64,
}

impl Store {
    /// Open chats, newest message in either direction first, independent of
    /// receipt/update timestamps. The dot also covers closed/history-only chats.
    pub fn tray_overview(&self) -> Result<(Vec<TrayChat>, bool)> {
        self.with(|c| {
            let any: bool = c.query_row("SELECT EXISTS(SELECT 1 FROM messages WHERE outgoing=0 AND seen=0)
                OR EXISTS(SELECT 1 FROM history_marks WHERE old_unread>0)", [], |r| r.get(0))?;
            let mut q = c.prepare("SELECT peer, SUM(CASE WHEN outgoing=0 AND seen=0 THEN 1 ELSE 0 END)
                + (SELECT COALESCE(SUM(old_unread),0) FROM history_marks h WHERE h.peer=m.peer)
                FROM messages m WHERE NOT EXISTS(SELECT 1 FROM closed_dms d WHERE d.peer=m.peer)
                GROUP BY peer ORDER BY MAX(created_at) DESC, peer ASC LIMIT 6")?;
            let rows = q.query_map([], |r| Ok(TrayChat { peer: r.get(0)?, unread: r.get::<_, i64>(1)?.max(0) as u64 }))?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            Ok((rows, any))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recent_chat_order_ignores_direction_and_receipt_activity_and_counts_all_unread() {
        let s = Store::open_in_memory().unwrap();
        // Existing schema's required message fields, without constructing wire messages.
        s.with(|c| {
            for i in 0..8 {
                c.execute("INSERT INTO messages(id,peer,outgoing,body,kind,state,created_at,seen)
                    VALUES(?1,?2,?3,'','message','received',?4,?5)",
                    params![format!("m{i}"), format!("p{i}"), i % 2, format!("2026-10-14T00:00:0{i}Z"), i != 0])?;
            }
            Ok(())
        }).unwrap();
        let (rows, any) = s.tray_overview().unwrap();
        assert_eq!(rows.iter().map(|r|r.peer.as_str()).collect::<Vec<_>>(), vec!["p7","p6","p5","p4","p3","p2","p1","p0"]);
        assert!(any);
        s.with(|c| { c.execute("UPDATE messages SET seen=1", [])?; Ok(()) }).unwrap();
        assert!(!s.tray_overview().unwrap().1);
    }

    #[test]
    fn recent_chat_scroll_can_reach_every_open_chat_beyond_ten() {
        let s = Store::open_in_memory().unwrap();
        s.with(|c| {
            for i in 0..16 {
                c.execute("INSERT INTO messages(id,peer,outgoing,body,state,created_at,seen)
                    VALUES(?1,?2,?3,'','read',?4,1)",
                    params![format!("m{i}"), format!("p{i:02}"), i % 2, format!("2026-10-19T00:00:{i:02}Z")])?;
            }
            // A receipt arriving later must not move an old chat to the top.
            c.execute("UPDATE messages SET read_at='2026-10-20T00:00:00Z' WHERE id='m0'", [])?;
            c.execute("INSERT INTO closed_dms(peer) VALUES('p14')", [])?;
            Ok(())
        }).unwrap();
        let (rows, any) = s.tray_overview().unwrap();
        let expected: Vec<_> = (0..16).rev().filter(|i| *i != 14).map(|i|format!("p{i:02}")).collect();
        assert_eq!(rows.iter().map(|r|r.peer.clone()).collect::<Vec<_>>(), expected);
        assert!(!any);
    }

    #[test]
    fn closed_chats_stay_hidden_but_keep_the_notice_dot() {
        let s = Store::open_in_memory().unwrap();
        s.with(|c| {
            c.execute("INSERT INTO messages(id,peer,outgoing,body,kind,state,created_at,seen)
                VALUES('m','closed',0,'','message','received','2026-10-14T00:00:00Z',0)", [])?;
            c.execute("INSERT INTO closed_dms(peer) VALUES('closed')", [])?;
            Ok(())
        }).unwrap();
        let (rows, any) = s.tray_overview().unwrap();
        assert!(rows.is_empty() && any);
    }

    #[test]
    fn history_only_unread_lights_the_dot_without_inventing_a_recent_row() {
        let s = Store::open_in_memory().unwrap();
        s.with(|c| { c.execute("INSERT INTO hubs(url,name,added_at) VALUES('http://scratch.invalid','fixture','2026-10-14T00:00:00Z')", [])?; Ok(()) }).unwrap();
        s.set_old_unread("http://scratch.invalid", "remote", 4, None).unwrap();
        let (rows,any) = s.tray_overview().unwrap(); assert!(rows.is_empty() && any);
        s.set_old_unread("http://scratch.invalid", "remote", 0, None).unwrap();
        assert!(!s.tray_overview().unwrap().1);
    }
}
