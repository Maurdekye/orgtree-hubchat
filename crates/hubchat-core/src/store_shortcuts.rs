//! Launcher metadata only; never loads message bodies or attachment bytes.
use super::*;

#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub struct ShortcutContact {
    pub peer: String,
    pub name: String,
}

impl Store {
    pub fn shortcut_contacts(&self, _open_only: bool, _limit: Option<u32>) -> Result<Vec<ShortcutContact>> {
        Ok(vec![])
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
}
