//! Removing a hub and adding an address again (user 2026-10-13,
//! hubchat-contacts-lost-after-reconnecting-to-a-hu): removal used to keep
//! the hub's sync position while its directory rows went, so the same
//! address added again resumed that position and contacts unchanged since
//! never came back. Store-level checks, no hub needed; readdress_v2 runs the
//! same against a real hub.

use hubchat_core::store::Store;

/// What a hub's sync keeps under its URL, written as the engine and the
/// picture code write it.
fn sync_keys(u: &str) -> [String; 6] {
    [
        format!("sync.cursor.{u}"),
        format!("sync.start_ms.{u}"),
        format!("sync.chat_list_due.{u}"),
        format!("hub.clock_offset_ms.{u}"),
        format!("picture.ack.{u}"),
        format!("picture.peer.{u}\npeer.abc123"),
    ]
}

/// Removal forgets that hub's sync state, and only that hub's.
#[test]
fn removing_a_hub_forgets_what_its_sync_kept() {
    let s = Store::open_in_memory().unwrap();
    let (a, b) = ("http://a:7370", "http://b:7370");
    for u in [a, b] {
        s.add_hub(u, "t0").unwrap();
        for k in sync_keys(u) {
            s.set_meta(&k, "kept").unwrap();
        }
    }
    s.remove_hub(a).unwrap();
    for k in sync_keys(a) {
        assert_eq!(s.meta(&k).unwrap(), None, "{k:?} outlived its hub");
    }
    for k in sync_keys(b) {
        assert_eq!(s.meta(&k).unwrap().as_deref(), Some("kept"), "{k:?} of the other hub");
    }
}

/// A store a removal hit before the fix: adding that address again starts
/// its sync over; adding a hub already saved keeps its position.
#[test]
fn an_address_added_again_starts_its_sync_over() {
    let s = Store::open_in_memory().unwrap();
    let u = "http://pendragon:7370";
    for k in sync_keys(u) {
        s.set_meta(&k, "left behind").unwrap();
    }
    s.add_hub(u, "t1").unwrap();
    for k in sync_keys(u) {
        assert_eq!(s.meta(&k).unwrap(), None, "{k:?} survived the address being added again");
    }
    s.set_meta(&format!("sync.cursor.{u}"), "9-9-0").unwrap();
    s.add_hub(u, "t2").unwrap();
    assert_eq!(s.meta(&format!("sync.cursor.{u}")).unwrap().as_deref(), Some("9-9-0"));
}

/// Migration 17: a store from before the fix fetches each saved hub's whole
/// directory once more, so contacts a removal of the same address dropped
/// come back without the user doing anything. Its sync position stays, and
/// nothing is scheduled twice.
#[test]
fn a_store_from_before_the_fix_fetches_each_directory_once() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("hubchat.sqlite3");
    let due = |u: &str| format!("sync.directory_refresh_due.{u}");
    let hubs = ["http://star-system:7378", "http://pendragon:7370"];
    {
        let s = Store::open(&path).unwrap();
        for u in hubs {
            s.add_hub(u, "t0").unwrap();
            s.set_meta(&format!("sync.cursor.{u}"), "2583-40-0").unwrap();
            s.delete_meta(&due(u)).unwrap();
        }
    }
    // the store as it stood before migration 17
    rusqlite::Connection::open(&path).unwrap().pragma_update(None, "user_version", 16).unwrap();
    let s = Store::open(&path).unwrap();
    for u in hubs {
        assert_eq!(s.meta(&due(u)).unwrap().as_deref(), Some("yes"), "{u}");
        assert_eq!(s.meta(&format!("sync.cursor.{u}")).unwrap().as_deref(), Some("2583-40-0"), "{u}");
    }
    s.delete_meta(&due(hubs[0])).unwrap();
    drop(s);
    let s = Store::open(&path).unwrap();
    assert_eq!(s.meta(&due(hubs[0])).unwrap(), None, "scheduled again");
}
