//! Device linking in Hubchat Web: the apps' link commands
//! (`src-tauri/src/link.rs`, scheme in `hubchat_core::link`) answered by the
//! web shell, under the same names and with the same answers.
//!
//! New browser: `hc_link_start` shows a code (or joins the code of a QR the
//! camera read) and listens on a throwaway address for up to ten minutes;
//! progress goes to the UI as `hc-link` events. Signed-in browser:
//! `hc_link_offer`, `hc_link_lookup` and `hc_link_approve` send another
//! device the sealed identity. Also the key QR and the key file.
//!
//! What differs from the apps, because a page differs:
//! - a page may only call https (spec R2.2): of the addresses a link or an
//!   arriving identity names for a hub, only those are tried and kept;
//! - a browser that has an identity does not switch to another one through
//!   a link (coordinator 2026-11-11): it says `HAS_IDENTITY`;
//! - the key file is text the page reads and saves (`text`, not a path).

// RED: the tests below and what they need to compile. The commands are not written yet.
#![allow(unused_imports, dead_code)]

use std::sync::Arc;
use std::time::Duration;

use hubchat_core::hub::{CancelFlag, Profile};
use hubchat_core::link::{self, Bundle, LinkUrl};
use hubchat_core::{rt, HubAddress, HubClient};
use serde::Serialize;
use serde_json::{json, Value};

use crate::shell::{arg, out, page_can_reach, s, Adoption, Probe, Shell, NEEDS_HTTPS, R};

/// A link whose hub has no address a page may call.
pub const LINK_NEEDS_HTTPS: &str =
    "This link's hub has no https:// address, and Hubchat Web can only reach a hub over HTTPS. Ask the hub's owner for its https:// address.";

/// A link would bring an identity to a browser that has one.
pub const HAS_IDENTITY: &str = "This browser already has an identity.";

/// The name the key file is saved under, as the apps suggest it.
pub const KEY_FILE_NAME: &str = "hubchat-key.hubchat-key";

const LINK_TTL: Duration = Duration::from_secs(600);

/// Progress of `hc_link_start`, as the apps emit it (event `hc-link`).
#[derive(Serialize, Clone)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum LinkEvent {
    Waiting { expires_in_s: u64 },
    /// This browser already IS that identity: nothing to do.
    Same { address: String },
    /// An identity arrived and waits in memory until `hc_link_confirm` or
    /// `hc_link_discard`. `from` is always null here (no switch on the web).
    Review { from: Option<String>, to: String, name: String, hubs: Vec<HubRow> },
    Failed { error: String },
    Expired,
}

/// One hub that comes with a linked identity, as the review shows it.
#[derive(Serialize, Clone)]
pub struct HubRow {
    /// The hub as the other device knows it.
    theirs: String,
    /// The best address for this browser: one that answered, else the first candidate.
    address: String,
    /// Addresses tried, likeliest first: only those a page can call, when there are any.
    candidates: Vec<String>,
    name: Option<String>,
    error: Option<String>,
    reachable: bool,
}

impl Shell {
    pub(crate) fn plan_link(&self, _cmd: &str, _args: &Value) -> R<Option<Adoption>> {
        Ok(None)
    }

    #[cfg(test)]
    pub(crate) async fn receive(&self, _bundle: Bundle, _via: &str, _aliases: &[String], _reception: &link::Reception) {}

    pub(crate) async fn link_call(self: &Arc<Self>, _cmd: &str, _args: &Value) -> Option<R<Value>> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shell::tests::{fresh, take};
    use hubchat_core::Identity;

    fn bundle(id: &str, hubs: &[&str]) -> Bundle {
        let me = Identity::generate(id).unwrap();
        Bundle { id: me.id().into(), secret: me.secret().into(), name: "Maya R".into(), about: "on call".into(), hubs: hubs.iter().map(|h| h.to_string()).collect(), recovery_saved: true }
    }

    fn events(seen: &Arc<std::sync::Mutex<Vec<(String, String)>>>) -> Vec<Value> {
        seen.lock().unwrap().iter().filter(|(n, _)| n == "hc-link").map(|(_, j)| serde_json::from_str(j).unwrap()).collect()
    }

    #[tokio::test]
    async fn a_link_is_read_and_only_https_addresses_are_tried() {
        let (sh, _) = fresh();
        let url = LinkUrl { code: link::normalize_code(&link::new_link_code()).unwrap(), hubs: vec!["http://100.64.0.7:7378".into(), "https://pc.example.ts.net:8443".into()], hub_name: Some("home".into()), role: Some("give".into()) }.to_url();
        let p = sh.call("hc_parse_link", &json!({ "input": url })).await.unwrap();
        assert_eq!(p["role"], "give");
        assert_eq!(p["hub_name"], "home");
        assert_eq!(p["hub"], "http://100.64.0.7:7378");
        assert_eq!(p["hubs"], json!(["http://100.64.0.7:7378", "https://pc.example.ts.net:8443"]));
        assert_eq!(p["code"].as_str().unwrap().len(), 19, "{p}");
        // a link whose hub has no https address says so, before any request
        let none = sh.call("hc_probe_link_hubs", &json!({ "hubs": ["http://100.64.0.7:7378", "http://pc.lan:7378"], "name": "home" })).await.unwrap();
        assert_eq!(none, json!({ "result": "invalid", "error": LINK_NEEDS_HTTPS }));
        assert_eq!(sh.call("hc_probe_link_hubs", &json!({ "hubs": [], "name": null })).await.unwrap()["error"], "the link names no hub");
        // with an https address among them, that one is tried (nothing answers at .invalid)
        let tried = sh.call("hc_probe_link_hubs", &json!({ "hubs": ["http://100.64.0.7:7378", "https://hub.invalid"], "name": null })).await.unwrap();
        assert_eq!((tried["result"].clone(), tried["url"].clone()), (json!("unreachable"), json!("https://hub.invalid")), "{tried}");
        // and a link is not started through an address a page cannot call
        let e = sh.call("hc_link_start", &json!({ "hub": "http://100.64.0.7:7378", "deviceName": "", "code": null, "aliases": null })).await.unwrap_err();
        assert_eq!(e, NEEDS_HTTPS);
        sh.shutdown();
    }

    #[tokio::test]
    async fn what_a_link_brings_waits_for_the_review_and_is_taken_on_confirm() {
        let (sh, seen) = fresh();
        let b = bundle("maya", &["http://localhost:7370", "https://hub.invalid"]);
        let address = b.identity().unwrap().address();
        let reception = link::Reception::new(HubClient::new(HubAddress::parse("https://hub.invalid").unwrap()), CancelFlag::default());
        sh.receive(b, "https://hub.invalid", &[], &reception).await;
        let ev = events(&seen);
        assert_eq!(ev.len(), 1, "{ev:?}");
        assert_eq!((ev[0]["state"].clone(), ev[0]["from"].clone(), ev[0]["to"].clone(), ev[0]["name"].clone()), (json!("review"), Value::Null, json!(address), json!("Maya R")));
        // every row names an address a page can call, or says why it cannot
        for row in ev[0]["hubs"].as_array().unwrap() {
            let a = row["address"].as_str().unwrap();
            assert!(page_can_reach(a) || row["error"] == NEEDS_HTTPS, "{row}");
        }
        assert!(ev[0]["hubs"].as_array().unwrap().iter().any(|r| r["address"] == "https://hub.invalid"), "{}", ev[0]["hubs"]);
        // nothing is taken before the user confirms
        assert_eq!(sh.call("hc_state", &json!({})).await.unwrap()["me"], Value::Null);
        // a hub a page cannot call is refused, and the identity still waits
        assert_eq!(sh.call("hc_link_confirm", &json!({ "hubs": ["http://10.0.0.5:7378"] })).await.unwrap_err(), NEEDS_HTTPS);
        assert_eq!(sh.call("hc_link_confirm", &json!({ "hubs": ["https://hub.invalid", " https://hub.invalid/ "] })).await.unwrap(), json!(address));
        let st = sh.call("hc_state", &json!({})).await.unwrap();
        assert_eq!((st["me"]["address"].clone(), st["me"]["name"].clone(), st["me"]["about"].clone(), st["recovery_saved"].clone()), (json!(address), json!("Maya R"), json!("on call"), json!(true)));
        assert_eq!(st["hubs"].as_array().unwrap().iter().map(|h| h["url"].clone()).collect::<Vec<_>>(), vec![json!("https://hub.invalid")]);
        // it was taken once
        assert_eq!(sh.call("hc_link_confirm", &json!({ "hubs": [] })).await.unwrap_err(), HAS_IDENTITY);
        sh.shutdown();

        // dropped: nothing to confirm afterwards
        let (sh, _) = fresh();
        sh.receive(bundle("omar", &["https://hub.invalid"]), "https://hub.invalid", &[], &reception).await;
        sh.call("hc_link_discard", &json!({})).await.unwrap();
        assert_eq!(sh.call("hc_link_confirm", &json!({ "hubs": [] })).await.unwrap_err(), "nothing to confirm: link again");
        sh.shutdown();
    }

    #[tokio::test]
    async fn a_browser_with_an_identity_does_not_switch_through_a_link() {
        let (sh, seen) = fresh();
        let mine = take(&sh, "hc_create_identity", json!({ "id": "maya", "name": "Maya" })).await;
        assert_eq!(sh.call("hc_link_start", &json!({ "hub": "https://hub.invalid", "deviceName": "", "code": null, "aliases": null })).await.unwrap_err(), HAS_IDENTITY);
        let reception = link::Reception::new(HubClient::new(HubAddress::parse("https://hub.invalid").unwrap()), CancelFlag::default());
        sh.receive(bundle("omar", &["https://hub.invalid"]), "https://hub.invalid", &[], &reception).await;
        assert_eq!(events(&seen), vec![json!({ "state": "failed", "error": HAS_IDENTITY })]);
        assert_eq!(sh.call("hc_restore_qr", &json!({ "text": link::to_qr(&bundle("omar", &[])) })).await.unwrap_err(), HAS_IDENTITY);
        assert_eq!(sh.call("hc_state", &json!({})).await.unwrap()["me"]["address"], json!(mine));
        sh.shutdown();
    }

    #[tokio::test]
    async fn a_key_qr_and_a_key_file_bring_the_identity_with_its_https_hubs() {
        let b = bundle("maya", &["http://192.168.1.20:7370", "https://hub.invalid"]);
        let address = b.identity().unwrap().address();

        let (sh, _) = fresh();
        assert!(sh.call("hc_restore_qr", &json!({ "text": "hubchat-key:nonsense" })).await.is_err());
        assert_eq!(take(&sh, "hc_restore_qr", json!({ "text": link::to_qr(&b) })).await, address);
        let st = sh.call("hc_state", &json!({})).await.unwrap();
        assert_eq!((st["me"]["name"].clone(), st["recovery_saved"].clone()), (json!("Maya R"), json!(true)));
        assert_eq!(st["hubs"].as_array().unwrap().iter().map(|h| h["url"].clone()).collect::<Vec<_>>(), vec![json!("https://hub.invalid")]);
        sh.shutdown();

        let (sh, _) = fresh();
        let file = link::to_key_file(&b, "correct horse").unwrap();
        assert!(sh.call("hc_key_file_import", &json!({ "text": file, "passphrase": "wrong" })).await.is_err());
        assert_eq!(sh.call("hc_state", &json!({})).await.unwrap()["me"], Value::Null);
        assert_eq!(take(&sh, "hc_key_file_import", json!({ "text": file, "passphrase": "correct horse" })).await, address);
        assert_eq!(sh.call("hc_state", &json!({})).await.unwrap()["me"]["address"], json!(address));
        sh.shutdown();
    }

    #[tokio::test]
    async fn a_signed_in_browser_offers_a_link_and_gives_its_key() {
        let (sh, _) = fresh();
        let mine = take(&sh, "hc_create_identity", json!({ "id": "maya", "name": "Maya" })).await;
        // no hub yet: a new device links through one
        assert_eq!(sh.call("hc_link_offer", &json!({ "hub": null })).await.unwrap_err(), "add a hub first: the new device links through one");
        sh.call("hc_add_hub", &json!({ "input": "https://hub.invalid" })).await.unwrap();
        let offer = sh.call("hc_link_offer", &json!({ "hub": null })).await.unwrap();
        assert_eq!((offer["hub"].clone(), offer["hubs"].clone(), offer["phone_access_off"].clone()), (json!("https://hub.invalid"), json!(["https://hub.invalid"]), json!(false)));
        let l = LinkUrl::parse(offer["qr"].as_str().unwrap()).unwrap();
        assert_eq!((l.role.as_deref(), l.hubs.clone(), link::format_code(&l.code)), (Some("give"), vec!["https://hub.invalid".to_string()], offer["code"].as_str().unwrap().to_string()));
        // the device that scans it waits under the code's address; nobody is there yet
        let look = sh.call("hc_link_lookup", &json!({ "input": offer["qr"] })).await.unwrap();
        assert_eq!((look["address"].clone(), look["device_name"].clone(), look["unknown_hub"].clone()), (json!(link::link_identity(&l.code).unwrap().address()), Value::Null, Value::Null));
        sh.call("hc_link_forget", &json!({ "code": offer["code"] })).await.unwrap();
        // the key as a QR and as a passphrase-locked file: both give this identity back
        let qr = sh.call("hc_key_qr", &json!({})).await.unwrap();
        assert_eq!(link::from_qr(qr.as_str().unwrap()).unwrap().identity().unwrap().address(), mine);
        let file = sh.call("hc_key_file_text", &json!({ "passphrase": "correct horse" })).await.unwrap();
        assert_eq!(file["name"], KEY_FILE_NAME);
        let back = link::from_key_file(file["text"].as_str().unwrap(), "correct horse").unwrap();
        assert_eq!((back.identity().unwrap().address(), back.hubs), (mine, vec!["https://hub.invalid".to_string()]));
        assert!(link::from_key_file(file["text"].as_str().unwrap(), "wrong").is_err());
        sh.shutdown();
    }
}
