//! Lockdown (Mail hub 2.6.0; Orgtree docket org-mailbox-lockdown-allow-list-mode-that-denies, user 2026-11-25
//! 13:26Z: "a little lock icon in the corner of their status while enabled to anyone with access, and to anyone
//! without access, their whole status symbol is replaced with a lock, and they cannot be messaged"). Each hub's
//! sync says who is in lockdown there, as we see it (`locked`: open when their list counts us, else closed), and
//! our own lockdown (`lockdown`). The directory merges the hubs: closed on any hub wins (review I9: a client does
//! not reroute around it). Scratch stores; never a hub.
use super::*;

const A: &str = "http://127.0.0.1:7997";
const B: &str = "http://127.0.0.1:7998";
const ANN: &str = "ann.111111";
const BOB: &str = "bob.222222";
const CY: &str = "cy.333333";

fn store() -> Store {
    let s = Store::open_in_memory().unwrap();
    for h in [A, B] {
        s.add_hub(h, "t0").unwrap();
        let roster: Vec<RosterEntry> = [ANN, BOB, CY]
            .iter()
            .map(|a| serde_json::from_value(serde_json::json!({ "slug": a, "kind": "org", "online": true })).unwrap())
            .collect();
        s.set_roster(h, &roster).unwrap();
    }
    s
}

fn lock(s: &Store, address: &str) -> Option<String> {
    s.directory().unwrap().into_iter().find(|c| c.address == address).unwrap().locked
}

fn map(v: serde_json::Value) -> std::collections::BTreeMap<String, String> {
    serde_json::from_value(v).unwrap()
}

#[test]
fn the_directory_shows_who_is_in_lockdown_closed_on_any_hub_wins() {
    let s = store();
    assert_eq!(lock(&s, ANN), None);
    s.hear_locked(A, Some(&map(serde_json::json!({ ANN: "closed", BOB: "open" }))), None).unwrap();
    s.hear_locked(B, Some(&map(serde_json::json!({ ANN: "open", BOB: "open" }))), None).unwrap();
    assert_eq!(lock(&s, ANN).as_deref(), Some("closed"), "closed on one hub is closed");
    assert_eq!(lock(&s, BOB).as_deref(), Some("open"));
    assert_eq!(lock(&s, CY), None);
}

#[test]
fn a_hubs_empty_or_missing_word_lifts_what_it_said_before() {
    // review (Astra 11): an empty map, or presence without the field (a hub without lockdown), is an explicit "none"
    let s = store();
    assert!(s.hear_locked(A, Some(&map(serde_json::json!({ ANN: "closed" }))), None).unwrap(), "a change");
    assert!(!s.hear_locked(A, Some(&map(serde_json::json!({ ANN: "closed" }))), None).unwrap(), "the same word again: no change");
    assert!(s.hear_locked(A, Some(&map(serde_json::json!({}))), None).unwrap());
    assert_eq!(lock(&s, ANN), None, "the last unlock clears it");
    s.hear_locked(A, Some(&map(serde_json::json!({ ANN: "closed" }))), None).unwrap();
    s.hear_locked(A, None, None).unwrap();
    assert_eq!(lock(&s, ANN), None, "no field: none there");
    s.hear_locked(A, Some(&map(serde_json::json!({ ANN: "maybe" }))), None).unwrap();
    assert_eq!(lock(&s, ANN), None, "only open and closed count");
}

#[test]
fn our_own_lockdown_is_kept_per_hub() {
    let s = store();
    assert!(s.own_lockdown().unwrap().is_empty());
    s.hear_locked(B, None, Some(&serde_json::json!({ "on": true, "allow": [ANN], "since": "2026-11-25T15:00:00Z", "rev": 2 }))).unwrap();
    assert_eq!(s.own_lockdown().unwrap(), vec![B.to_string()], "the hubs where our address is in lockdown");
    s.hear_locked(B, None, None).unwrap();
    assert!(s.own_lockdown().unwrap().is_empty());
}
