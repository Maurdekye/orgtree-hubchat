//! Review R1 (docs/question-cards.md, `fallback`): a received plain-text
//! range is left out of what Hubchat shows only where the accepted `meta`
//! stands for it. These checks use only `questions::incoming`, which every
//! version has, so the same file shows the reviewed version cutting readable
//! text and this one keeping it; the last test is the positive control.

use hubchat_core::engine::strip_quote;
use hubchat_core::questions::incoming;
use serde_json::{json, Value};

fn shown(body: &str, kind: Option<&str>, reply_to: Option<&str>, meta: Value) -> String {
    incoming(body, kind, reply_to, Some(&meta)).0
}

#[test]
fn a_card_whose_options_are_refused_keeps_its_list() {
    let body = "Q?\n\nPick one:\n1. Same\n2. same";
    let at = body.find("\n\nPick").unwrap();
    assert_eq!(shown(body, Some("question"), None, json!({"question": {"options": ["Same", "same"]}, "fallback": [at, body.len()]})), body);
}

#[test]
fn a_range_alone_or_on_the_wrong_kind_removes_nothing() {
    let body = "Q?\n\nPick one:\n1. Beta\n2. Stable";
    let at = body.find("\n\nPick").unwrap();
    for (kind, meta) in [
        (Some("question"), json!({"fallback": [at, body.len()]})),
        (None, json!({"question": {"options": ["Beta", "Stable"]}, "fallback": [at, body.len()]})),
        (None, json!({"fallback": [0, body.len()]})),
        (Some("question"), json!({"question": {"options": ["Beta", "Stable"]}, "fallback": [0, body.len()]})),
    ] {
        assert_eq!(shown(body, kind, None, meta.clone()), body, "{meta}");
    }
}

#[test]
fn accepted_options_remove_only_their_own_list() {
    let body = "Q?\n\nPick one:\n1. Beta\n2. Stable";
    let at = body.find("\n\nPick").unwrap();
    for (q, r) in [
        (json!({"options": ["Beta", "Other"]}), [at, body.len()]),
        (json!({"options": ["Stable", "Beta"]}), [at, body.len()]),
        (json!({"options": ["Beta", "Stable"], "multiple": true}), [at, body.len()]),
        (json!({"options": ["Beta", "Stable"]}), [at + 1, body.len()]),
    ] {
        assert_eq!(shown(body, Some("question"), None, json!({"question": q, "fallback": r})), body, "{q} {r:?}");
    }
}

#[test]
fn a_reply_keeps_text_its_range_does_not_stand_for() {
    // with no other meta, a range over a whole reply would make it a skip
    for (b, meta) in [
        ("> q\nreal answer", json!({"fallback": [0, 15]})),
        ("> q\nSkipped\nmore", json!({"fallback": [0, 12]})),
        ("> q\nAnswer: Beta\n", json!({"fallback": [0, 17]})),
        ("> q\nAnswer: Beta\n", json!({"answers": [{"question": "q", "picked": ["Stable"]}], "fallback": [0, 17]})),
        ("> q\n> r\nSkipped\n", json!({"fallback": [0, 16]})),
        ("> q\n> r\n", json!({"replies": ["q", "r", "s"], "fallback": [0, 8]})),
    ] {
        assert_eq!(shown(b, None, Some("q"), meta.clone()), strip_quote(b), "{b:?} {meta}");
    }
}

#[test]
fn what_meta_stands_for_still_goes() {
    // a card's own list, as Orgtree writes it
    let body = "Which channel?\n\nPick one or more:\n1. Beta\n2. Stable";
    let at = body.find("\n\nPick").unwrap();
    let card = json!({"question": {"options": ["Beta", "Stable"], "multiple": true}, "fallback": [at, body.len()]});
    assert_eq!(shown(body, Some("question"), None, card), "Which channel?");
    // a reply's lines, as Hubchat writes them: a skip, picks, a multi-reply
    for (b, meta, want) in [
        ("> q\nSkipped\n", json!({"fallback": [0, 12]}), ""),
        ("> q\nAnswer: Beta, Gamma\nwhy", json!({"answers": [{"question": "q", "picked": ["Beta", "Gamma"]}], "fallback": [0, 24]}), "why"),
        ("> q\n> r\nSkipped\n", json!({"replies": ["q", "r"], "fallback": [0, 16]}), ""),
        ("> q\n> r\nhi", json!({"replies": ["q", "r"], "fallback": [0, 8]}), "hi"),
    ] {
        assert_eq!(shown(b, None, Some("q"), meta), want, "{b:?}");
    }
    // older clients send no meta: the one quote line goes as before
    assert_eq!(incoming("> q\nok", None, Some("q"), None).0, "ok");
}
