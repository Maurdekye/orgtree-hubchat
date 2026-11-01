//! Busy presence (Mail hub 2.3.0; user 2026-10-30 23:03Z-23:19Z, docket
//! org-presence-busy-orange-when-no-org-inbox-holde): an address a hub lists
//! online can be busy there. The directory says busy while every hub where
//! the address is online says so. Scratch stores and a client that never
//! connects; never a real hub.
use super::*;
use crate::engine::{Engine, Event, Host};
use crate::hub::Profile;
use crate::{HubAddress, HubClient, Identity};
use std::sync::Arc;

const A: &str = "http://127.0.0.1:7997";
const B: &str = "http://127.0.0.1:7998";
const PAT: &str = "pat.1a2b3c";
const ACME: &str = "acme.4d5e6f";
const ALICE: &str = "alice.111111";
/// The schema version before busy (MIGRATIONS of Hubchat 1.2.6).
const BEFORE_BUSY: usize = 20;

fn store() -> Store {
    let s = Store::open_in_memory().unwrap();
    for h in [A, B] {
        s.add_hub(h, "t0").unwrap();
    }
    s
}

fn entry(slug: &str, kind: &str, online: bool, busy: bool) -> RosterEntry {
    serde_json::from_value(serde_json::json!({ "slug": slug, "kind": kind, "online": online, "busy": busy })).unwrap()
}

/// (online, busy) as the directory lists the address.
fn seen(s: &Store, address: &str) -> (bool, bool) {
    let d = s.directory().unwrap();
    let c = d.iter().find(|c| c.address == address).unwrap_or_else(|| panic!("{address} is not listed"));
    (c.online, c.busy)
}

/// A person's language (Mail hub 2.4.0 puts it on their roster entry; user
/// 2026-11-01 16:13Z: shown when looking at their profile) reaches the
/// directory from a full roster and from a sync's roster changes; the first
/// hub that lists one gives it; none when they chose none.
#[test]
fn a_persons_language_reaches_the_directory() {
    let s = store();
    let with = |slug: &str, language: Option<&str>| -> RosterEntry {
        serde_json::from_value(serde_json::json!({ "slug": slug, "kind": "person", "online": true, "language": language })).unwrap()
    };
    let language = |s: &Store, address: &str| -> serde_json::Value {
        let d = s.directory().unwrap();
        let c = d.iter().find(|c| c.address == address).unwrap();
        serde_json::to_value(c).unwrap().get("language").cloned().unwrap_or(serde_json::Value::Null)
    };
    s.set_roster(A, &[with(PAT, Some("he")), with(ALICE, None)]).unwrap();
    assert_eq!(language(&s, PAT), serde_json::json!("he"));
    assert_eq!(language(&s, ALICE), serde_json::Value::Null, "none chosen: none shown");
    s.upsert_roster(A, &[with(PAT, Some("fr"))]).unwrap();
    assert_eq!(language(&s, PAT), serde_json::json!("fr"), "a sync's change replaces it");
    s.set_roster(B, &[with(ALICE, Some("de"))]).unwrap();
    assert_eq!(language(&s, ALICE), serde_json::json!("de"), "the hub that lists one");
}

#[test]
fn a_roster_row_keeps_busy_and_online_on_one_hub_wins() {
    let s = store();
    s.set_roster(A, &[entry(PAT, "person", true, true), entry(ACME, "org", true, true), entry(ALICE, "person", true, false)]).unwrap();
    assert_eq!(seen(&s, PAT), (true, true), "a roster row's busy is kept");
    assert_eq!(seen(&s, ACME), (true, true));
    assert_eq!(seen(&s, ALICE), (true, false));
    // the same org online and free on another hub: that hub wins
    s.set_roster(B, &[entry(ACME, "org", true, false)]).unwrap();
    assert_eq!(seen(&s, ACME), (true, false), "online and not busy on one hub wins");
    // offline there: busy, as on the hub where it is online
    s.set_online(B, &[]).unwrap();
    s.set_busy(B, &[]).unwrap();
    assert_eq!(seen(&s, ACME), (true, true));
    // busy on both hubs
    s.upsert_roster(B, &[entry(ACME, "org", true, true)]).unwrap();
    assert_eq!(seen(&s, ACME), (true, true));
    // offline everywhere: never busy, whatever a row said before
    s.set_online(A, &[PAT.into(), ALICE.into()]).unwrap();
    s.set_online(B, &[]).unwrap();
    assert_eq!(seen(&s, ACME), (false, false), "offline is never busy");
}

#[test]
fn a_hubs_busy_list_marks_exactly_these_there() {
    let s = store();
    s.set_roster(A, &[entry(PAT, "person", true, false), entry(ACME, "org", true, false)]).unwrap();
    s.set_roster(B, &[entry(PAT, "person", true, false)]).unwrap();
    s.set_busy(A, &[PAT.into(), ACME.into()]).unwrap();
    assert_eq!(seen(&s, ACME), (true, true));
    assert_eq!(seen(&s, PAT), (true, false), "still free on hub B");
    s.set_busy(B, &[PAT.into()]).unwrap();
    assert_eq!(seen(&s, PAT), (true, true));
    s.set_busy(A, &[ACME.into()]).unwrap();
    assert_eq!(seen(&s, PAT), (true, false), "hub A no longer says so");
    assert_eq!(seen(&s, ACME), (true, true));
}

#[test]
fn a_store_from_before_busy_gains_it_and_keeps_it() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("hubchat.db");
    {
        let c = legacy_schema_fixture(&path, BEFORE_BUSY);
        c.execute_batch(&format!(
            "INSERT INTO hubs(url, added_at) VALUES('{A}', 't0');
             INSERT INTO roster(address, hub, kind, online) VALUES('{ACME}', '{A}', 'org', 1);"
        ))
        .unwrap();
    }
    let s = Store::open(&path).unwrap();
    assert_eq!(seen(&s, ACME), (true, false), "nobody was busy before");
    s.set_busy(A, &[ACME.into()]).unwrap();
    assert_eq!(seen(&s, ACME), (true, true));
    drop(s);
    // opened again: kept, like online (the hub says it again only on a change)
    let s = Store::open(&path).unwrap();
    assert_eq!(seen(&s, ACME), (true, true));
}

struct SilentHost;
impl Host for SilentHost {
    fn event(&self, _: Event) {}
    fn open_source(&self, _: &str) -> std::io::Result<std::fs::File> {
        unreachable!()
    }
    fn download_dir(&self) -> std::path::PathBuf {
        std::path::PathBuf::new()
    }
}

/// What sync admits from a hub's answer (Engine::sync_session).
fn admit(e: &Engine, client: &HubClient, first_page: bool, answer: serde_json::Value) {
    let mut v = answer;
    v["name"] = "Hub A".into();
    v["cursor"] = "c".into();
    let r: crate::hub_v2::SyncResult = serde_json::from_value(v).unwrap();
    e.admit_roster_batch(client, &r.roster, first_page && !r.roster.is_empty(), &r.roster_removed, r.online.as_deref(), r.busy.as_deref(), r.dnd.as_deref())
        .unwrap();
}

#[test]
fn a_sync_answer_says_who_is_busy_and_a_hub_before_busy_says_no_one() {
    let s = Arc::new(store());
    let me = Identity::generate("busy-proof").unwrap();
    s.set_meta("identity.address", &me.address()).unwrap();
    let e = Engine::new(s.clone(), me, Profile::default(), Arc::new(SilentHost));
    e.set_suspended(true);
    let a = HubClient::new(HubAddress::parse(A).unwrap());
    // the first page: the roster, each row with its busy
    admit(&e, &a, true, serde_json::json!({ "roster": [
        { "slug": PAT, "kind": "person", "online": true, "busy": true },
        { "slug": ACME, "kind": "org", "online": true },
        { "slug": ALICE, "kind": "person", "online": true }] }));
    assert_eq!(seen(&s, PAT), (true, true));
    assert_eq!(seen(&s, ACME), (true, false));
    // a later answer: who is online, and which of them are busy
    admit(&e, &a, false, serde_json::json!({ "online": [PAT, ACME, ALICE], "busy": [ACME] }));
    assert_eq!(seen(&s, PAT), (true, false), "online again at once");
    assert_eq!(seen(&s, ACME), (true, true));
    // an answer without the sets changes neither
    admit(&e, &a, false, serde_json::json!({}));
    assert_eq!(seen(&s, ACME), (true, true));
    // a hub before busy sends online alone: no one is busy there
    admit(&e, &a, false, serde_json::json!({ "online": [PAT, ACME, ALICE] }));
    assert_eq!(seen(&s, ACME), (true, false));
    // offline is never busy
    admit(&e, &a, false, serde_json::json!({ "online": [ALICE], "busy": [ACME] }));
    assert_eq!(seen(&s, ACME), (false, false));
}

#[test]
fn the_send_route_says_on_which_hub_they_are_busy() {
    let s = Arc::new(store());
    let me = Identity::generate("busy-route-proof").unwrap();
    s.set_meta("identity.address", &me.address()).unwrap();
    let e = Engine::new(s.clone(), me, Profile::default(), Arc::new(SilentHost));
    e.set_suspended(true);
    s.set_roster(A, &[entry(PAT, "person", true, true), entry(ALICE, "person", true, false)]).unwrap();
    s.set_roster(B, &[entry(PAT, "person", false, false), entry(ALICE, "person", true, false)]).unwrap();
    let hubs = |peer: &str| e.send_route(peer).unwrap().hubs.into_iter().map(|h| (h.url, h.online, h.busy)).collect::<Vec<_>>();
    assert_eq!(hubs(PAT), [(A.to_string(), true, true), (B.to_string(), false, false)]);
    assert_eq!(hubs(ALICE), [(A.to_string(), true, false), (B.to_string(), true, false)]);
}

// Do not disturb (Mail hub 2.3.0; the docket's addition, user 2026-10-31
// 08:51Z): an org whose outside mail is set to Do not disturb. A hub lists a
// DND address in busy too. The directory says DND while every hub where the
// address is online says so, so the most available presence any hub lists
// wins (online over busy over DND); offline wins over DND (09:03Z, "Offline
// for both").

/// The schema version before Do not disturb (MIGRATIONS with busy).
const BEFORE_DND: usize = 21;
const DELTA: &str = "delta.7a8b9c";

fn entry_dnd(slug: &str, online: bool, busy: bool, dnd: bool) -> RosterEntry {
    serde_json::from_value(serde_json::json!({ "slug": slug, "kind": "org", "online": online, "busy": busy, "dnd": dnd })).unwrap()
}

/// (online, busy, dnd) as the directory lists the address.
fn seen3(s: &Store, address: &str) -> (bool, bool, bool) {
    let d = s.directory().unwrap();
    let c = d.iter().find(|c| c.address == address).unwrap_or_else(|| panic!("{address} is not listed"));
    (c.online, c.busy, c.dnd)
}

#[test]
fn a_roster_row_keeps_dnd_and_the_most_available_hub_wins() {
    let s = store();
    s.set_roster(A, &[entry_dnd(DELTA, true, true, true), entry_dnd(ACME, true, true, false)]).unwrap();
    assert_eq!(seen3(&s, DELTA), (true, true, true), "a roster row's dnd is kept (and busy, a part of it)");
    assert_eq!(seen3(&s, ACME), (true, true, false));
    // busy and not DND on another hub: busy wins over DND
    s.set_roster(B, &[entry_dnd(DELTA, true, true, false)]).unwrap();
    assert_eq!(seen3(&s, DELTA), (true, true, false), "busy on one hub wins over DND");
    // online and free there: online wins
    s.upsert_roster(B, &[entry_dnd(DELTA, true, false, false)]).unwrap();
    assert_eq!(seen3(&s, DELTA), (true, false, false));
    // offline there: DND, as on the hub where it is online
    s.set_online(B, &[]).unwrap();
    s.set_busy(B, &[]).unwrap();
    s.set_dnd(B, &[]).unwrap();
    assert_eq!(seen3(&s, DELTA), (true, true, true));
    // offline everywhere: offline wins over DND
    s.set_online(A, &[ACME.into()]).unwrap();
    assert_eq!(seen3(&s, DELTA), (false, false, false), "offline wins over DND");
}

#[test]
fn a_hubs_dnd_list_marks_exactly_these_there() {
    let s = store();
    s.set_roster(A, &[entry_dnd(DELTA, true, true, false), entry_dnd(ACME, true, true, false)]).unwrap();
    s.set_dnd(A, &[DELTA.into()]).unwrap();
    assert_eq!(seen3(&s, DELTA), (true, true, true));
    assert_eq!(seen3(&s, ACME), (true, true, false));
    s.set_dnd(A, &[ACME.into()]).unwrap();
    assert_eq!(seen3(&s, DELTA), (true, true, false), "hub A no longer says so");
    assert_eq!(seen3(&s, ACME), (true, true, true));
}

#[test]
fn a_store_from_before_dnd_gains_it_and_keeps_it() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("hubchat.db");
    {
        let c = legacy_schema_fixture(&path, BEFORE_DND);
        c.execute_batch(&format!(
            "INSERT INTO hubs(url, added_at) VALUES('{A}', 't0');
             INSERT INTO roster(address, hub, kind, online, busy) VALUES('{DELTA}', '{A}', 'org', 1, 1);"
        ))
        .unwrap();
    }
    let s = Store::open(&path).unwrap();
    assert_eq!(seen3(&s, DELTA), (true, true, false), "nobody was on Do not disturb before");
    s.set_dnd(A, &[DELTA.into()]).unwrap();
    assert_eq!(seen3(&s, DELTA), (true, true, true));
    drop(s);
    // opened again: kept, like busy
    let s = Store::open(&path).unwrap();
    assert_eq!(seen3(&s, DELTA), (true, true, true));
}

#[test]
fn a_sync_answer_says_who_is_dnd_and_a_hub_before_dnd_says_no_one() {
    let s = Arc::new(store());
    let me = Identity::generate("dnd-proof").unwrap();
    s.set_meta("identity.address", &me.address()).unwrap();
    let e = Engine::new(s.clone(), me, Profile::default(), Arc::new(SilentHost));
    e.set_suspended(true);
    let a = HubClient::new(HubAddress::parse(A).unwrap());
    // the first page: the roster, each row with its dnd
    admit(&e, &a, true, serde_json::json!({ "roster": [
        { "slug": DELTA, "kind": "org", "online": true, "busy": true, "dnd": true },
        { "slug": ACME, "kind": "org", "online": true, "busy": true }] }));
    assert_eq!(seen3(&s, DELTA), (true, true, true));
    assert_eq!(seen3(&s, ACME), (true, true, false));
    // a later answer: who is online, which are busy, which of them on DND
    admit(&e, &a, false, serde_json::json!({ "online": [DELTA, ACME], "busy": [DELTA, ACME], "dnd": [ACME] }));
    assert_eq!(seen3(&s, DELTA), (true, true, false), "busy, no longer on DND");
    assert_eq!(seen3(&s, ACME), (true, true, true));
    // an answer without the sets changes neither
    admit(&e, &a, false, serde_json::json!({}));
    assert_eq!(seen3(&s, ACME), (true, true, true));
    // a hub that knows busy but not DND sends online and busy alone: no one is on DND there
    admit(&e, &a, false, serde_json::json!({ "online": [DELTA, ACME], "busy": [DELTA, ACME] }));
    assert_eq!(seen3(&s, ACME), (true, true, false));
    // offline wins over DND
    admit(&e, &a, false, serde_json::json!({ "online": [DELTA], "busy": [DELTA, ACME], "dnd": [ACME] }));
    assert_eq!(seen3(&s, ACME), (false, false, false));
}

#[test]
fn the_send_route_says_on_which_hub_they_are_on_dnd() {
    let s = Arc::new(store());
    let me = Identity::generate("dnd-route-proof").unwrap();
    s.set_meta("identity.address", &me.address()).unwrap();
    let e = Engine::new(s.clone(), me, Profile::default(), Arc::new(SilentHost));
    e.set_suspended(true);
    s.set_roster(A, &[entry_dnd(DELTA, true, true, true)]).unwrap();
    s.set_roster(B, &[entry_dnd(DELTA, true, true, false)]).unwrap();
    let hubs = e.send_route(DELTA).unwrap().hubs.into_iter().map(|h| (h.url, h.online, h.busy, h.dnd)).collect::<Vec<_>>();
    assert_eq!(hubs, [(A.to_string(), true, true, true), (B.to_string(), true, true, false)]);
}
