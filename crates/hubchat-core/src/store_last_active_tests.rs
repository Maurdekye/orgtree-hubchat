//! Last active (Mail hub 2.6.0, feature `last_active`; docket
//! hubchat-last-seen-time-for-orgs-people-and-indep): a hub's roster rows say
//! `active` and `active_at`, and its sync answers `active_now`. The directory
//! takes the latest time any hub says and active from any hub where they are
//! online; a store from before it fetches each hub's roster again. Scratch
//! stores only; never a real hub.
use super::*;

const A: &str = "http://127.0.0.1:7997";
const B: &str = "http://127.0.0.1:7998";
const ACME: &str = "acme.4d5e6f";
/// The schema version before last active (MIGRATIONS of Hubchat 1.3.14).
const BEFORE_LAST_ACTIVE: usize = 24;

fn store() -> Store {
    let s = Store::open_in_memory().unwrap();
    for h in [A, B] {
        s.add_hub(h, "t0").unwrap();
    }
    s
}

fn row(online: bool, active: bool, at: Option<&str>) -> RosterEntry {
    serde_json::from_value(serde_json::json!({ "slug": ACME, "kind": "org", "online": online, "active": active, "active_at": at })).unwrap()
}

/// (active, active_at) as the directory lists it.
fn seen(s: &Store) -> (bool, Option<String>) {
    let d = s.directory().unwrap();
    let c = d.iter().find(|c| c.address == ACME).unwrap();
    (c.active, c.active_at.clone())
}

#[test]
fn the_directory_takes_the_latest_time_and_active_from_any_hub() {
    let s = store();
    s.set_roster(A, &[row(true, false, Some("2026-11-24T10:40:00.000Z"))]).unwrap();
    s.set_roster(B, &[row(true, false, Some("2026-11-24T11:05:00.000Z"))]).unwrap();
    assert_eq!(seen(&s), (false, Some("2026-11-24T11:05:00.000Z".into())), "the latest any hub says");
    s.set_active(A, &[ACME.into()]).unwrap();
    assert_eq!(seen(&s).0, true, "active on one hub where it is online: active");
    s.set_active(A, &[]).unwrap();
    assert_eq!(seen(&s).0, false, "active_now is the truth when it comes: the set replaces the rows' snapshot");
    s.upsert_roster(A, &[row(false, true, Some("2026-11-24T12:00:00.000Z"))]).unwrap();
    s.set_roster(B, &[row(false, false, None)]).unwrap();
    assert_eq!(seen(&s), (false, Some("2026-11-24T12:00:00.000Z".into())), "offline is never active; a hub with no time doesn't hide another's");
    s.set_roster(A, &[row(true, false, None)]).unwrap();
    assert_eq!(seen(&s), (false, None), "no hub says a time: none");
}

#[test]
fn a_store_from_before_fetches_each_directory_again() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("hubchat.db");
    {
        let c = legacy_schema_fixture(&path, BEFORE_LAST_ACTIVE);
        c.execute_batch(&format!(
            "INSERT INTO hubs(url, added_at) VALUES('{A}', 't0'), ('{B}', 't0');
             INSERT INTO roster(address, hub, kind, online) VALUES('{ACME}', '{A}', 'org', 1);
             INSERT INTO meta(key, value) VALUES('sync.cursor.{A}', '120-57-1e9b06e71ee91fc6');"
        ))
        .unwrap();
    }
    let s = Store::open(&path).unwrap();
    for h in [A, B] {
        assert_eq!(s.meta(&directory_refresh_key(h)).unwrap().as_deref(), Some("yes"), "{h}: its whole directory once more");
    }
    assert_eq!(s.meta(&cursor_key(A)).unwrap().as_deref(), Some("120-57-1e9b06e71ee91fc6"), "its sync position stays");
    assert_eq!(seen(&s), (false, None), "rows from before: not active, no time, until the hub sends them again");
}
