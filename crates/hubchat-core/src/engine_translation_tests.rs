//! Translation between people (engine_translation; Mail hub 2.4.0, feature
//! `translate`; Orgtree docket mail-hub-hubchat-automatic-translation-between-u):
//! scratch stores and a fake hub on 127.0.0.1; never a real hub, provider or
//! key.
use super::*;
use crate::hub_v2::Change;

const A: &str = "http://127.0.0.1:7997";
const B: &str = "http://127.0.0.1:7998";
const PEER: &str = "peer-tr@hub-tr";

struct Events(Mutex<Vec<Event>>);
impl Host for Events {
    fn event(&self, ev: Event) { self.0.lock().unwrap().push(ev); }
    fn open_source(&self, _: &str) -> std::io::Result<std::fs::File> { unreachable!() }
    fn download_dir(&self) -> PathBuf { PathBuf::new() }
}

fn engine_with(hubs: &[&str]) -> (Arc<Engine>, Arc<Events>, Arc<Store>) {
    let store = Arc::new(Store::open_in_memory().unwrap());
    let me = Identity::generate("translation-proof").unwrap();
    store.set_meta("identity.address", &me.address()).unwrap();
    for (i, h) in hubs.iter().enumerate() {
        store.add_hub(h, &format!("t{i}")).unwrap();
    }
    let events = Arc::new(Events(Mutex::new(Vec::new())));
    let e = Engine::new(store.clone(), me, Profile::default(), events.clone());
    (e, events, store)
}

fn told_window(ev: &Events) -> usize {
    ev.0.lock().unwrap().iter().filter(|e| matches!(e, Event::Status)).count()
}
/// The notifications the shell was asked for: (id, preview).
fn notices(ev: &Events) -> Vec<(String, String)> {
    ev.0.lock().unwrap().iter().filter_map(|e| match e {
        Event::Incoming { id, preview, .. } => Some((id.clone(), preview.clone())),
        _ => None,
    }).collect()
}
fn owed(s: &Store, url: &str) -> Option<String> { s.meta(&format!("language.owed.{url}")).unwrap() }
fn said(s: &Store, url: &str) -> Option<String> { s.meta(&format!("language.said.{url}")).unwrap() }
fn some(s: &str) -> Option<String> { Some(s.into()) }

/// A message change as a sync answer has it; `extra` adds fields
/// (`, "translation": {...}`).
fn change(id: &str, to: &str, body: &str, received_at: &str, extra: &str) -> Change {
    let json = format!(
        r#"{{"type":"message","message":{{"id":"{id}","from":"{PEER}","to":"{to}","body":{},"received_at":"{received_at}"{extra}}}}}"#,
        serde_json::to_string(body).unwrap()
    );
    serde_json::from_str(&json).unwrap()
}
fn synced(id: &str, to: &str, body: &str, received_at: &str, extra: &str) -> crate::hub_v2::SyncedMessage {
    match change(id, to, body, received_at, extra) {
        Change::Message(m) => *m,
        other => panic!("not a message change: {other:?}"),
    }
}
fn shown(s: &Store, id: &str) -> Option<crate::hub_v2::Translation> { s.message(id).unwrap().unwrap().translation }

const PENDING: &str = r#", "translation": {"lang": "he", "status": "pending"}"#;
const DONE: &str = r#", "translation": {"lang": "he", "source_lang": "en", "status": "translated", "body": "שלום"}"#;

#[test]
fn a_translation_is_kept_and_follows_its_outcome() {
    let (e, _, s) = engine_with(&[A]);
    let me = e.me().address();
    let at = now();
    s.upsert_live_synced_at(A, &me, &synced("tr-1", &me, "hello", &at, PENDING), 0).unwrap();
    let t = shown(&s, "tr-1").expect("a pending translation is kept");
    assert_eq!((t.lang.as_str(), t.status.as_str(), t.body), ("he", "pending", None));
    // its outcome: the hub sends the message again, with it
    let fresh = s.upsert_live_synced_at(A, &me, &synced("tr-1", &me, "hello", &at, DONE), 0).unwrap();
    assert!(!fresh, "the same message again is not new mail");
    let t = shown(&s, "tr-1").expect("the outcome replaces it");
    assert_eq!((t.status.as_str(), t.source_lang.as_deref(), t.body.as_deref()), ("translated", Some("en"), Some("שלום")));
    assert_eq!(s.message("tr-1").unwrap().unwrap().body, "hello", "the original stays the message's body");
    // the same message without it (a page from before, another hub's copy): kept
    s.upsert_synced_at(A, &me, &synced("tr-1", &me, "hello", &at, ""), 0).unwrap();
    assert_eq!(shown(&s, "tr-1").map(|t| t.status), some("translated"));
    // each outcome the hub has
    for status in ["too_long", "limit", "failed"] {
        let extra = format!(r#", "translation": {{"lang": "he", "status": "{status}"}}"#);
        s.upsert_live_synced_at(A, &me, &synced("tr-1", &me, "hello", &at, &extra), 0).unwrap();
        assert_eq!(shown(&s, "tr-1").map(|t| t.status), some(status));
    }
}

/// Received data is untrusted (AGENTS.md, `meta`): a translation that fails
/// its checks is left out, never the message.
#[test]
fn a_translation_that_fails_its_checks_is_left_out_not_the_message() {
    let me = "me@hub-tr";
    let at = now();
    for (bad, why) in [
        (r#", "translation": {"lang": "he", "status": "done"}"#, "an unknown outcome"),
        (r#", "translation": {"lang": "not a tag", "status": "pending"}"#, "not a language tag"),
        (r#", "translation": {"lang": "he", "status": "translated"}"#, "translated with no text"),
        (r#", "translation": {"lang": "he", "status": "translated", "body": 7}"#, "text that is not text"),
        (r#", "translation": {"lang": "he", "source_lang": "??", "status": "translated", "body": "x"}"#, "a bad source language"),
        (r#", "translation": "he""#, "not an object"),
    ] {
        let m = synced("tr-bad", me, "hello", &at, bad);
        assert_eq!(m.translation, None, "{why}");
        assert_eq!(m.env.body, "hello");
    }
    let m = synced("tr-ok", me, "hello", &at, DONE);
    assert_eq!(m.translation.map(|t| t.body), Some(some("שלום")), "a good one is taken");
    let m = synced("tr-ok", me, "hello", &at, r#", "translation": {"lang": "pt-BR", "status": "pending"}"#);
    assert_eq!(m.translation.map(|t| t.lang), some("pt-BR"));
}

/// A reply's body starts with a one-line quote that Hubchat shows from the
/// message it answers; its translation is shown the same way.
#[test]
fn a_reply_shows_its_translation_without_the_quote() {
    let (e, _, s) = engine_with(&[A]);
    let me = e.me().address();
    let at = now();
    let extra = r#", "reply_to": "tr-0", "translation": {"lang": "he", "source_lang": "en", "status": "translated", "body": "> היי\nשלום"}"#;
    s.upsert_live_synced_at(A, &me, &synced("tr-2", &me, "> hi\nhello", &at, extra), 0).unwrap();
    assert_eq!(s.message("tr-2").unwrap().unwrap().body, "hello");
    assert_eq!(shown(&s, "tr-2").and_then(|t| t.body), some("שלום"));
}

/// `/healthz` on the hub's main port says whether it translates; a hub
/// without the feature, or through its door, says nothing.
#[test]
fn a_hubs_word_on_translation_is_read() {
    let health = |json: &str| -> crate::hub::Health { serde_json::from_str(json).unwrap() };
    let on = health(r#"{"ok":true,"name":"h","features":["sync","translate"],"translation":{"enabled":true,"state":"key_refused","model":"m"}}"#);
    assert_eq!(on.translation_state(), some("key_refused"));
    let off = health(r#"{"ok":true,"name":"h","features":["sync","translate"],"translation":{"enabled":false,"state":"off"}}"#);
    assert_eq!(off.translation_state(), some("off"));
    let door = health(r#"{"ok":true,"name":"h","features":["sync","translate"]}"#);
    assert_eq!(door.translation_state(), None, "the door says nothing");
    let older = health(r#"{"ok":true,"name":"h","features":["sync"],"translation":{"enabled":true,"state":"ok"}}"#);
    assert_eq!(older.translation_state(), None, "only a hub with the feature");
    let odd = health(r#"{"ok":true,"name":"h","features":["translate"],"translation":"yes"}"#);
    assert_eq!(odd.translation_state(), None, "a word it doesn't know");
    let odd = health(r#"{"ok":true,"name":"h","features":["translate"],"translation":{"state":"sideways"}}"#);
    assert_eq!(odd.translation_state(), None);
}

#[test]
fn our_language_is_kept_and_owed_to_every_saved_hub() {
    let (e, ev, s) = engine_with(&[A, B]);
    assert_eq!(e.language(), "", "none until one is picked");
    e.set_language("he").unwrap();
    assert_eq!((e.language(), owed(&s, A), owed(&s, B)), ("he".into(), some("he"), some("he")));
    assert_eq!(told_window(&ev), 1, "the window hears of it");
    assert!(e.set_language("not a tag").is_err());
    assert!(e.set_language("<b>").is_err());
    assert_eq!((e.language(), told_window(&ev)), ("he".into(), 1), "a refused pick changes nothing");
    e.set_language("").unwrap();
    assert_eq!((e.language(), owed(&s, A)), ("".into(), some("")), "none is a pick too: told as none");
}

#[test]
fn a_language_picked_on_another_of_our_devices_is_taken() {
    let (e, ev, s) = engine_with(&[A, B]);
    e.hub_said_language(A, Some("he"), false).unwrap();
    assert_eq!((e.language(), said(&s, A), told_window(&ev)), ("he".into(), some("he"), 1), "a first word that is a choice");
    e.hub_said_language(A, Some("he"), false).unwrap();
    assert_eq!(told_window(&ev), 1, "the same word again is no change");
    e.hub_said_language(A, Some("fr"), false).unwrap();
    assert_eq!(e.language(), "fr", "picked on another device");
    e.hub_said_language(B, None, false).unwrap();
    assert_eq!((e.language(), owed(&s, B)), ("fr".into(), some("fr")), "none from a hub that never heard ours: it is told ours");
    e.hub_said_language(A, None, false).unwrap();
    assert_eq!(e.language(), "", "none picked on another device");
    e.hub_said_language(A, Some("<b>"), false).unwrap();
    assert_eq!(e.language(), "", "a word that is not a tag is not taken");
    // our pick on its way: only its own word ends the wait
    e.set_language("de").unwrap();
    e.hub_said_language(A, Some("fr"), false).unwrap();
    assert_eq!((e.language(), owed(&s, A)), ("de".into(), some("de")));
    e.hub_said_language(A, Some("de"), false).unwrap();
    assert_eq!((owed(&s, A), said(&s, A)), (None, some("de")));
}

/// A fake hub answering `/api/profile` with `code`; every request's path
/// and body is kept.
async fn profile_hub(code: u16) -> (String, Arc<Mutex<Vec<(String, String)>>>, tokio::task::JoinHandle<()>) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let seen = Arc::new(Mutex::new(Vec::new()));
    let log = seen.clone();
    let task = tokio::spawn(async move {
        while let Ok((mut stream, _)) = listener.accept().await {
            let log = log.clone();
            tokio::spawn(async move {
                let (mut buf, mut chunk) = (Vec::new(), [0u8; 4096]);
                let (head, body) = loop {
                    let n = stream.read(&mut chunk).await.unwrap_or(0);
                    if n == 0 {
                        return;
                    }
                    buf.extend_from_slice(&chunk[..n]);
                    let Some(at) = buf.windows(4).position(|w| w == b"\r\n\r\n") else { continue };
                    let head = String::from_utf8_lossy(&buf[..at]).to_string();
                    let len = head
                        .lines()
                        .find_map(|l| l.to_ascii_lowercase().strip_prefix("content-length:").map(|v| v.trim().parse::<usize>().unwrap_or(0)))
                        .unwrap_or(0);
                    while buf.len() < at + 4 + len {
                        let n = stream.read(&mut chunk).await.unwrap_or(0);
                        if n == 0 {
                            return;
                        }
                        buf.extend_from_slice(&chunk[..n]);
                    }
                    break (head, String::from_utf8_lossy(&buf[at + 4..at + 4 + len]).to_string());
                };
                let path = head.split(' ').nth(1).unwrap_or("").to_string();
                log.lock().unwrap().push((path.clone(), body));
                let (code, json) = if path == "/api/profile" { (code, r#"{"language":"he"}"#) } else { (404, r#"{"detail":"not found"}"#) };
                let resp = format!("HTTP/1.1 {code} X\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{json}", json.len());
                let _ = stream.write_all(resp.as_bytes()).await;
            });
        }
    });
    (url, seen, task)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_language_is_told_only_to_a_hub_that_translates() {
    for (code, owed_after, said_after) in [(200, None, some("he")), (403, None, None), (503, some("he"), None)] {
        let (url, seen, task) = profile_hub(code).await;
        let (e, _, s) = engine_with(&[&url]);
        let client = HubClient::new(HubAddress::parse(&url).unwrap());
        let profile_posts = || seen.lock().unwrap().iter().filter(|(p, _)| p == "/api/profile").map(|(_, b)| b.clone()).collect::<Vec<_>>();
        e.set_language("he").unwrap();
        e.tell_language(&client, &url, &["sync".to_string(), "profile".to_string()]).await.unwrap();
        assert_eq!((profile_posts().len(), owed(&s, &url)), (0, None), "a hub that doesn't translate is not asked");
        e.set_language("he").unwrap();
        e.tell_language(&client, &url, &["sync".to_string(), "translate".to_string()]).await.unwrap();
        assert_eq!(profile_posts(), vec![r#"{"language":"he"}"#.to_string()], "told ({code}): the language alone");
        assert_eq!((owed(&s, &url), said(&s, &url)), (owed_after, said_after), "after a {code}");
        task.abort();
    }
}

/// User 2026-11-01 12:15Z ("Wait up to 4 s"): a notification waits for its
/// message's translation, and shows it.
#[test]
fn a_notification_waits_for_its_translation() {
    let (e, ev, s) = engine_with(&[A]);
    let me = e.me().address();
    let at = now();
    let m = synced("tr-3", &me, "hello", &at, PENDING);
    s.upsert_live_synced_at(A, &me, &m, 0).unwrap();
    e.notice_incoming(A, &m, false);
    assert_eq!(notices(&ev), vec![], "held while its translation is on its way");
    let started = unix_ms();
    assert!(e.release_held(started).is_some_and(|t| t > started && t <= started + 4000), "it waits at most 4 s");
    assert_eq!(notices(&ev), vec![]);
    s.upsert_live_synced_at(A, &me, &synced("tr-3", &me, "hello", &at, DONE), 0).unwrap();
    assert_eq!(e.release_held(unix_ms()), None, "nothing else waits");
    assert_eq!(notices(&ev), vec![("tr-3".to_string(), "שלום".to_string())], "notified with the translation");
}

#[test]
fn a_notification_waits_no_longer_than_four_seconds() {
    let (e, ev, s) = engine_with(&[A]);
    let me = e.me().address();
    let m = synced("tr-4", &me, "hello", &now(), PENDING);
    s.upsert_live_synced_at(A, &me, &m, 0).unwrap();
    let t0 = unix_ms();
    e.notice_incoming(A, &m, false);
    e.release_held(t0 + 3000);
    assert_eq!(notices(&ev), vec![]);
    assert_eq!(e.release_held(t0 + 4100), None);
    assert_eq!(notices(&ev), vec![("tr-4".to_string(), "hello".to_string())], "then notified with the original");
    // a message deleted while it waited is not notified
    let m = synced("tr-5", &me, "bye", &now(), PENDING);
    s.upsert_live_synced_at(A, &me, &m, 0).unwrap();
    e.notice_incoming(A, &m, false);
    s.delete_message("tr-5").unwrap();
    e.release_held(unix_ms() + 5000);
    assert_eq!(notices(&ev).len(), 1);
}

/// A push wake comes after the hub waited for the translation already: no
/// second wait here.
#[test]
fn a_message_that_waited_at_the_hub_is_not_held_again() {
    let (e, ev, s) = engine_with(&[A]);
    let me = e.me().address();
    let long_ago = humantime::format_rfc3339_millis(SystemTime::now() - Duration::from_secs(5)).to_string();
    let m = synced("tr-6", &me, "hello", &long_ago, PENDING);
    s.upsert_live_synced_at(A, &me, &m, 0).unwrap();
    e.notice_incoming(A, &m, false);
    assert_eq!(notices(&ev), vec![("tr-6".to_string(), "hello".to_string())], "the hub's 4 s are over: at once");
    let m = synced("tr-7", &me, "hello", &now(), DONE);
    s.upsert_live_synced_at(A, &me, &m, 0).unwrap();
    e.notice_incoming(A, &m, false);
    assert_eq!(notices(&ev)[1], ("tr-7".to_string(), "שלום".to_string()), "translated when it came: at once, translated");
    let m = synced("tr-8", &me, "hello", &now(), "");
    e.notice_incoming(A, &m, false);
    assert_eq!(notices(&ev)[2], ("tr-8".to_string(), "hello".to_string()), "nothing to translate: at once");
}
