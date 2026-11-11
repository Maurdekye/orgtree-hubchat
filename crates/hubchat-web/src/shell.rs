//! The shell of Hubchat Web, without the browser: one store, one identity,
//! one engine, and the UI's commands answered from them.
//!
//! NOT WRITTEN YET (the red commit): this is only the shape the tests below
//! are written against. Every command answers `NOT_YET`.

use std::sync::Arc;
#[allow(unused_imports)]
use std::sync::Mutex;

use hubchat_core::engine::Engine;
use hubchat_core::store::Store;
use hubchat_core::Identity;
#[allow(unused_imports)]
use serde_json::{json, Value};

/// A command this shell does not do yet.
pub const NOT_YET: &str = "This isn't available in Hubchat Web yet.";

/// Spec R2.2: a page served over https may only call https.
pub const NEEDS_HTTPS: &str = "Hubchat Web can only reach a hub over HTTPS. Ask the hub's owner for its https:// address.";

/// Where events go: the event's name (`hc`, as the apps emit it) and its JSON.
pub type Sink = Arc<dyn Fn(&str, &str) + Send + Sync>;

type R<T> = Result<T, String>;

/// An identity about to be taken on this device.
pub struct Adoption {
    pub me: Identity,
    pub brand_new: bool,
}

pub struct Shell {
    store: Arc<Store>,
}

/// A hub address a page can call.
pub fn page_can_reach(_url: &str) -> bool {
    false
}

impl Shell {
    pub fn open(store: Arc<Store>, _sink: Sink, _own_name: &str, _identity: Option<Identity>) -> R<Arc<Shell>> {
        Ok(Arc::new(Shell { store }))
    }

    pub fn store(&self) -> &Arc<Store> {
        &self.store
    }

    pub fn engine(&self) -> R<Arc<Engine>> {
        Err(NOT_YET.into())
    }

    pub fn plan_identity(&self, _cmd: &str, _args: &Value) -> R<Option<Adoption>> {
        Ok(None)
    }

    pub fn adopt(&self, _plan: Adoption) -> R<String> {
        Err(NOT_YET.into())
    }

    pub fn start_over(&self) -> R<()> {
        Err(NOT_YET.into())
    }

    pub async fn call(self: &Arc<Self>, _cmd: &str, _args: &Value) -> R<Value> {
        Err(NOT_YET.into())
    }

    pub fn shutdown(&self) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shell(store: Arc<Store>, identity: Option<Identity>) -> (Arc<Shell>, Arc<Mutex<Vec<(String, String)>>>) {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let sink_seen = seen.clone();
        let sink: Sink = Arc::new(move |name, json| sink_seen.lock().unwrap().push((name.to_owned(), json.to_owned())));
        (Shell::open(store, sink, "Test browser", identity).unwrap(), seen)
    }

    fn fresh() -> (Arc<Shell>, Arc<Mutex<Vec<(String, String)>>>) {
        shell(Arc::new(Store::open_in_memory().unwrap()), None)
    }

    async fn take(shell: &Arc<Shell>, cmd: &str, args: Value) -> String {
        let plan = shell.plan_identity(cmd, &args).unwrap().expect("an identity command");
        shell.adopt(plan).unwrap()
    }

    #[tokio::test]
    async fn a_new_device_makes_an_identity_and_keeps_it() {
        let (sh, _) = fresh();
        let st = sh.call("hc_state", &json!({})).await.unwrap();
        assert_eq!(st["me"], Value::Null);
        assert_eq!(st["key_lost"], Value::Null);
        assert_eq!(st["shell"], "web");
        assert_eq!(st["platform"], "android");
        assert_eq!(st["hubs"], json!([]));
        // the id is checked as the apps check it
        assert_eq!(sh.call("hc_check_id", &json!({ "id": "Maya" })).await.unwrap()["ok"], false);
        let ok = sh.call("hc_check_id", &json!({ "id": "maya" })).await.unwrap();
        assert_eq!((ok["ok"].clone(), ok["max_len"].clone()), (json!(true), json!(24)));
        // nothing that needs an identity works before there is one
        assert_eq!(sh.call("hc_recovery_words", &json!({})).await.unwrap_err(), "no identity yet");

        let address = take(&sh, "hc_create_identity", json!({ "id": "maya", "name": "  Maya Lin " })).await;
        assert!(address.starts_with("maya."), "{address}");
        let st = sh.call("hc_state", &json!({})).await.unwrap();
        assert_eq!(st["me"]["address"], json!(address));
        assert_eq!(st["me"]["id"], "maya");
        assert_eq!(st["me"]["name"], "Maya Lin");
        assert_eq!(st["recovery_saved"], false);
        assert_eq!(st["device_name"], "Test browser");
        assert_eq!(sh.store().meta("identity.address").unwrap().as_deref(), Some(address.as_str()));
        assert!(sh.store().meta("device.id").unwrap().is_some_and(|d| d.starts_with("hc-")), "a device id is made at the first start");
        // one identity per device
        assert!(sh.plan_identity("hc_create_identity", &json!({ "id": "other", "name": "" })).is_err());

        let words: Vec<String> = serde_json::from_value(sh.call("hc_recovery_words", &json!({})).await.unwrap()).unwrap();
        assert_eq!(words.len(), 24);
        // the recovery file is the apps' text; the browser saves it, so reading it marks nothing saved
        let file = sh.call("hc_recovery_file", &json!({})).await.unwrap();
        assert_eq!(file["name"], "hubchat-recovery-maya.txt");
        let text = file["text"].as_str().unwrap();
        assert!(text.starts_with("Hubchat recovery words\r\n") && text.contains(&format!("Address: @net:{address}\r\n")), "{text}");
        assert!(text.contains(&format!(" 1. {}\r\n", words[0])) && text.contains(&format!("24. {}\r\n", words[23])), "{text}");
        assert_eq!(sh.call("hc_state", &json!({})).await.unwrap()["recovery_saved"], false);
        sh.call("hc_recovery_saved", &json!({})).await.unwrap();
        assert_eq!(sh.call("hc_state", &json!({})).await.unwrap()["recovery_saved"], true);

        // the same words on another device are the same address
        let (other, _) = fresh();
        let again = take(&other, "hc_restore_words", json!({ "words": words.join(" ") })).await;
        assert_eq!(again, address);
        assert_eq!(other.call("hc_state", &json!({})).await.unwrap()["recovery_saved"], true);
        sh.shutdown();
        other.shutdown();
    }

    #[tokio::test]
    async fn a_store_whose_key_is_gone_shows_the_lost_key_screen() {
        let store = Arc::new(Store::open_in_memory().unwrap());
        let (sh, _) = shell(store.clone(), None);
        let address = take(&sh, "hc_create_identity", json!({ "id": "pat", "name": "Pat" })).await;
        let me = sh.engine().unwrap().me().clone();
        sh.shutdown();
        // the browser's key store was cleared, its database was not
        let (lost, _) = shell(store.clone(), None);
        let st = lost.call("hc_state", &json!({})).await.unwrap();
        assert_eq!(st["me"], Value::Null);
        assert_eq!(st["key_lost"], json!({ "address": address, "unreadable": false }));
        // with the key, it is the same identity again, nothing lost
        let (back, _) = shell(store.clone(), Some(me));
        assert_eq!(back.call("hc_state", &json!({})).await.unwrap()["me"]["address"], json!(address));
        back.shutdown();
        // Start over forgets the old data, and a new identity inherits nothing
        let (lost, _) = shell(store, None);
        lost.call("hc_set_draft", &json!({ "peer": "ann.111111", "body": "left over" })).await.unwrap();
        lost.call("hc_start_over", &json!({})).await.unwrap();
        assert_eq!(lost.call("hc_state", &json!({})).await.unwrap()["key_lost"], Value::Null);
        assert_eq!(lost.call("hc_draft", &json!({ "peer": "ann.111111" })).await.unwrap(), Value::Null);
    }

    #[tokio::test]
    async fn commands_take_the_apps_names_and_arguments() {
        let (sh, _) = fresh();
        take(&sh, "hc_create_identity", json!({ "id": "maya", "name": "Maya" })).await;
        // camelCase, as the page's invoke() sends them
        let page = sh.call("hc_chat", &json!({ "peer": "ann.111111", "beforeAt": null, "beforeId": null, "fromAt": null, "fromId": null, "limit": null })).await.unwrap();
        assert_eq!(page, json!([]));
        assert_eq!(sh.call("hc_chats", &json!({})).await.unwrap(), json!([]));
        assert_eq!(sh.call("hc_directory", &json!({})).await.unwrap(), json!([]));
        assert_eq!(sh.call("hc_transfers", &json!({ "after": null })).await.unwrap()["rows"], json!([]));
        sh.call("hc_set_draft", &json!({ "peer": "ann.111111", "body": "half a thought" })).await.unwrap();
        assert_eq!(sh.call("hc_draft", &json!({ "peer": "ann.111111" })).await.unwrap(), "half a thought");
        let r = sh.call("hc_resolve", &json!({ "input": "@net:Ann.111111" })).await.unwrap();
        assert_eq!((r["address"].clone(), r["valid"].clone(), r["is_me"].clone()), (json!("ann.111111"), json!(true), json!(false)));
        sh.call("hc_set_notifications", &json!({ "enabled": true, "preview": false, "sound": false })).await.unwrap();
        let st = sh.call("hc_state", &json!({})).await.unwrap();
        assert_eq!(st["notifications"], json!({ "enabled": true, "preview": false, "sound": false }));
        assert_eq!(sh.call("hc_set_device_name", &json!({ "name": " Kitchen tablet " })).await.unwrap(), "Kitchen tablet");
        assert_eq!(sh.call("hc_set_device_name", &json!({ "name": "" })).await.unwrap(), "Test browser");
        sh.call("hc_ui_state", &json!({ "foreground": true, "chat": null })).await.unwrap();
        // a missing argument is said, not guessed
        assert!(sh.call("hc_set_draft", &json!({ "peer": "ann.111111" })).await.unwrap_err().contains("body"));
        // what a browser can't do yet says so
        assert_eq!(sh.call("hc_key_file_export", &json!({ "passphrase": "x", "dest": "y" })).await.unwrap_err(), NOT_YET);
        assert_eq!(sh.call("hc_no_such_command", &json!({})).await.unwrap_err(), NOT_YET);
        sh.shutdown();
    }

    #[tokio::test]
    async fn a_message_is_kept_and_queued_until_a_hub_takes_it() {
        let (sh, seen) = fresh();
        take(&sh, "hc_create_identity", json!({ "id": "maya", "name": "Maya" })).await;
        sh.call("hc_send", &json!({ "msg": { "id": "m1", "peer": "ann.111111", "body": "hello Ann", "kind": null, "reply_to": null } })).await.unwrap();
        let page = sh.call("hc_chat", &json!({ "peer": "ann.111111", "limit": 50 })).await.unwrap();
        assert_eq!(page.as_array().unwrap().len(), 1, "{page}");
        assert_eq!(page[0]["body"], "hello Ann");
        assert_eq!(page[0]["outgoing"], true);
        assert!(matches!(page[0]["state"].as_str(), Some("queued" | "sending")), "{}", page[0]["state"]);
        let chats = sh.call("hc_chats", &json!({})).await.unwrap();
        assert_eq!(chats[0]["peer"], "ann.111111");
        // the UI was told through the same `hc` events the apps emit
        assert!(seen.lock().unwrap().iter().any(|(name, json)| name == "hc" && json.contains(r#""type":"chat""#) && json.contains("ann.111111")), "{:?}", seen.lock().unwrap());
        // files wait for a later version, and say so
        let e = sh
            .call("hc_send", &json!({ "msg": { "id": "m2", "peer": "ann.111111", "body": "", "attachments": [{ "name": "a.txt", "bytes": 1, "source": "x" }] } }))
            .await
            .unwrap_err();
        assert_eq!(e, "Files can't be sent from Hubchat Web yet.");
        sh.shutdown();
    }

    #[tokio::test]
    async fn only_addresses_a_page_can_reach_are_hubs() {
        for (url, ok) in [
            ("https://pc.example.ts.net", true),
            ("https://pc.example.ts.net:8443/hub", true),
            ("http://localhost:7371", true),
            ("http://127.0.0.1:7378", true),
            ("http://[::1]:7371", true),
            ("http://10.0.4.2:7371", false),
            ("http://pc.example.ts.net:7371", false),
            ("http://localhost.evil.example:7371", false),
            ("http://127.0.0.1.evil.example", false),
        ] {
            assert_eq!(page_can_reach(url), ok, "{url}");
        }
        let (sh, _) = fresh();
        take(&sh, "hc_create_identity", json!({ "id": "maya", "name": "Maya" })).await;
        // refused before any request is made
        let p = sh.call("hc_probe_hub", &json!({ "input": "http://10.0.4.2:7371" })).await.unwrap();
        assert_eq!(p, json!({ "result": "invalid", "error": NEEDS_HTTPS }));
        assert_eq!(sh.call("hc_add_hub", &json!({ "input": "http://10.0.4.2:7371" })).await.unwrap_err(), NEEDS_HTTPS);
        assert_eq!(sh.call("hc_add_hub", &json!({ "input": "pc.example.ts.net:7371" })).await.unwrap_err(), NEEDS_HTTPS);
        assert_eq!(sh.call("hc_state", &json!({})).await.unwrap()["hubs"], json!([]));
        // an https hub is taken (it connects when it can)
        assert_eq!(sh.call("hc_add_hub", &json!({ "input": "https://hub.invalid" })).await.unwrap(), "https://hub.invalid");
        let hubs = sh.call("hc_state", &json!({})).await.unwrap()["hubs"].clone();
        assert_eq!(hubs[0]["url"], "https://hub.invalid");
        sh.shutdown();
    }
}
