//! Blocking (Mail hub 2.6.0, feature `blocking`; user 2026-11-24 18:56Z and
//! 19:06Z): the directory marks a contact we block (`blocked`) and one that
//! blocks us (`blocked_by`), from every saved hub's last word, with blocks
//! picked here and still owed applied at once. Scratch stores; never a hub.
use super::*;

const A: &str = "http://127.0.0.1:7997";
const B: &str = "http://127.0.0.1:7998";
const ANN: &str = "ann.111111";
const BOB: &str = "bob.222222";

fn store() -> Store {
    let s = Store::open_in_memory().unwrap();
    for h in [A, B] {
        s.add_hub(h, "t0").unwrap();
        let roster: Vec<RosterEntry> = [ANN, BOB]
            .iter()
            .map(|a| serde_json::from_value(serde_json::json!({ "slug": a, "kind": "person", "online": true })).unwrap())
            .collect();
        s.set_roster(h, &roster).unwrap();
    }
    s
}

fn marks(s: &Store, address: &str) -> (bool, bool) {
    let d = s.directory().unwrap();
    let c = d.iter().find(|c| c.address == address).unwrap();
    (c.blocked, c.blocked_by)
}

#[test]
fn the_directory_marks_whom_we_block_and_who_blocks_us() {
    let s = store();
    assert_eq!(marks(&s, ANN), (false, false));
    // a hub's word: we block Ann there; Bob blocks us on the other
    s.set_meta(&blocks_mine_key(A), &serde_json::to_string(&[ANN]).unwrap()).unwrap();
    s.set_meta(&blocks_by_key(B), &serde_json::to_string(&[BOB]).unwrap()).unwrap();
    assert_eq!(marks(&s, ANN), (true, false), "blocked on one hub is blocked");
    assert_eq!(marks(&s, BOB), (false, true));
}

#[test]
fn a_block_picked_here_shows_before_the_hubs_take_it() {
    let s = store();
    s.set_meta(&blocks_owed_key(A), r#"{"bob.222222":true}"#).unwrap();
    assert_eq!(marks(&s, BOB), (true, false));
    // an unblock owed to every hub wins over their last word
    s.set_meta(&blocks_mine_key(A), &serde_json::to_string(&[ANN]).unwrap()).unwrap();
    s.set_meta(&blocks_mine_key(B), &serde_json::to_string(&[ANN]).unwrap()).unwrap();
    for h in [A, B] {
        s.set_meta(&blocks_owed_key(h), r#"{"ann.111111":false}"#).unwrap();
    }
    assert_eq!(marks(&s, ANN), (false, false));
    // a hub no longer saved says nothing
    s.delete_meta(&blocks_owed_key(A)).unwrap();
    s.delete_meta(&blocks_owed_key(B)).unwrap();
    s.set_meta(&blocks_by_key("http://gone.example:7370"), &serde_json::to_string(&[ANN]).unwrap()).unwrap();
    assert_eq!(marks(&s, ANN), (true, false));
}
