//! Launcher metadata only; never loads message bodies or attachment bytes.
use super::*;

#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub struct ShortcutContact {
    pub peer: String,
    pub name: String,
}

impl Store {
    pub fn shortcut_contacts(&self, open_only: bool, limit: Option<u32>) -> Result<Vec<ShortcutContact>> {
        self.with(|c| {
            let mut q = c.prepare("SELECT peer,
                COALESCE((SELECT NULLIF(MAX(NULLIF(org_name,'')), '') FROM roster WHERE address=m.peer),
                         (SELECT NULLIF(MAX(NULLIF(username,'')), '') FROM roster WHERE address=m.peer), peer)
                FROM messages m
                WHERE (?1=0 OR NOT EXISTS(SELECT 1 FROM closed_dms d WHERE d.peer=m.peer))
                  AND NOT EXISTS(SELECT 1 FROM roster r WHERE r.address=m.peer AND r.kind='chat')
                GROUP BY peer ORDER BY MAX(created_at) DESC, peer ASC LIMIT ?2")?;
            let rows = q.query_map(params![open_only, limit.map(i64::from).unwrap_or(-1)], |r|
                Ok(ShortcutContact { peer:r.get(0)?, name:r.get(1)? }))?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            Ok(rows)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> Store {
        let s = Store::open_in_memory().unwrap();
        s.with(|c| {
            for i in 0..9 {
                c.execute("INSERT INTO messages(id,peer,outgoing,body,kind,state,created_at,seen)
                    VALUES(?1,?2,?3,'private body','message','received',?4,0)",
                    params![format!("m{i}"), format!("p{i}.123456"), i % 2, format!("2026-10-16T00:00:0{i}Z")])?;
            }
            c.execute("INSERT INTO closed_dms(peer) VALUES('p8.123456')", [])?;
            Ok(())
        }).unwrap();
        s
    }

    #[test]
    fn desktop_lists_every_open_dm_in_message_order() {
        let s = fixture();
        let rows = s.shortcut_contacts(true, None).unwrap();
        assert_eq!(rows.iter().map(|r|r.peer.as_str()).collect::<Vec<_>>(),
            (0..8).rev().map(|i|format!("p{i}.123456")).collect::<Vec<_>>());
    }

    #[test]
    fn android_last_four_include_closed_contacts_and_both_directions() {
        let s = fixture();
        let rows = s.shortcut_contacts(false, Some(4)).unwrap();
        assert_eq!(rows.iter().map(|r|r.peer.as_str()).collect::<Vec<_>>(),
            vec!["p8.123456", "p7.123456", "p6.123456", "p5.123456"]);
        // Receipt changes don't make an old contact recent.
        s.with(|c| { c.execute("UPDATE messages SET seen=1 WHERE peer='p0.123456'", [])?; Ok(()) }).unwrap();
        assert_eq!(s.shortcut_contacts(false, Some(4)).unwrap(), rows);
    }

    #[test]
    fn deletion_and_identity_wipe_remove_launcher_contacts() {
        let s = fixture();
        s.delete_chat("p8.123456").unwrap();
        assert_eq!(s.shortcut_contacts(false, Some(4)).unwrap()[0].peer, "p7.123456");
        s.wipe().unwrap();
        assert!(s.shortcut_contacts(false, Some(4)).unwrap().is_empty());
    }

    #[test]
    fn names_merge_hubs_and_groups_are_not_direct_contacts() {
        let s = fixture();
        s.with(|c| {
            c.execute("INSERT INTO hubs(url,name,added_at) VALUES('http://scratch.invalid','','t')", [])?;
            c.execute("INSERT INTO roster(address,hub,kind,org_name,username) VALUES
                ('p8.123456','http://scratch.invalid','chat','Group',''),
                ('p7.123456','http://scratch.invalid','org','Example Org','fallback'),
                ('p6.123456','http://scratch.invalid','person','','Person')", [])?;
            Ok(())
        }).unwrap();
        let rows = s.shortcut_contacts(false, Some(4)).unwrap();
        assert_eq!(rows.len(), 4);
        assert_eq!((&rows[0].peer, rows[0].name.as_str()), (&"p7.123456".to_owned(), "Example Org"));
        assert_eq!(rows[1].name, "Person");
        assert_eq!(rows[2].name, rows[2].peer);
    }
}
