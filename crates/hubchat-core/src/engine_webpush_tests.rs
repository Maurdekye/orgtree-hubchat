//! Notifications for Hubchat Web (engine_webpush): scratch stores and a fake
//! hub on 127.0.0.1 that answers the push registration; never a real hub or
//! a real push service.
use super::*;

struct Events(Mutex<Vec<Event>>);
impl Host for Events {
    fn event(&self, ev: Event) { self.0.lock().unwrap().push(ev); }
    fn open_source(&self, _: &str) -> std::io::Result<std::fs::File> { unreachable!() }
    fn download_dir(&self) -> PathBuf { PathBuf::new() }
}

fn engine_with(hubs: &[&str]) -> (Arc<Engine>, Arc<Store>) {
    let store = Arc::new(Store::open_in_memory().unwrap());
    let me = Identity::generate("push-proof").unwrap();
    store.set_meta("identity.address", &me.address()).unwrap();
    for (i, h) in hubs.iter().enumerate() {
        store.add_hub(h, &format!("t{i}")).unwrap();
    }
    let e = Engine::new(store.clone(), me, Profile::default(), Arc::new(Events(Mutex::new(Vec::new()))));
    (e, store)
}

fn subscription(preview: bool) -> WebPush {
    WebPush {
        endpoint: "https://push.example/secret-endpoint".into(),
        p256dh: "p".repeat(87),
        auth: "a".repeat(22),
        vapid: "v".repeat(43),
        navigate: "https://chat.example/".into(),
        preview,
    }
}

/// A fake hub: `POST /api/push` answered with `code`, `DELETE /api/push`
/// with `delete_code`. Every request's method, path and body is kept.
struct FakeHub {
    url: String,
    seen: Arc<Mutex<Vec<(String, String, String)>>>,
    task: tokio::task::JoinHandle<()>,
}
impl Drop for FakeHub {
    fn drop(&mut self) { self.task.abort(); }
}
impl FakeHub {
    fn seen(&self, method: &str) -> Vec<String> {
        self.seen.lock().unwrap().iter().filter(|(m, p, _)| m == method && p == "/api/push").map(|(_, _, b)| b.clone()).collect()
    }
}

async fn fake_hub(code: u16, delete_code: u16) -> FakeHub {
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
                let method = head.split(' ').next().unwrap_or("").to_string();
                let path = head.split(' ').nth(1).unwrap_or("").split('?').next().unwrap_or("").to_string();
                log.lock().unwrap().push((method.clone(), path.clone(), body));
                let (code, json) = match (method.as_str(), path.as_str()) {
                    ("POST", "/api/push") => (code, r#"{"registered":true,"detail":"as asked"}"#),
                    ("DELETE", "/api/push") => (delete_code, r#"{"registered":false,"detail":"as asked"}"#),
                    _ => (404, r#"{"detail":"not found"}"#),
                };
                let resp = format!("HTTP/1.1 {code} X\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{json}", json.len());
                let _ = stream.write_all(resp.as_bytes()).await;
            });
        }
    });
    FakeHub { url, seen, task }
}

fn state(e: &Engine) -> Vec<&'static str> {
    e.web_push_hubs().unwrap().into_iter().map(|h| h.state).collect()
}

const WITH: &[&str] = &["sync", "webpush"];
fn features(list: &[&str]) -> Vec<String> {
    list.iter().map(|f| f.to_string()).collect()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_hub_is_handed_the_subscription_once_and_again_when_it_changes() {
    let hub = fake_hub(200, 200).await;
    let (e, _) = engine_with(&[&hub.url]);
    let client = HubClient::new(HubAddress::parse(&hub.url).unwrap());
    // notifications off: nothing is said
    e.tell_web_push(&client, &hub.url, &features(WITH)).await.unwrap();
    assert!(hub.seen("POST").is_empty() && hub.seen("DELETE").is_empty());
    assert_eq!(state(&e), ["waiting"]);

    e.set_web_push(Some(&subscription(true))).unwrap();
    assert!(e.web_push() == Some(subscription(true)));
    e.tell_web_push(&client, &hub.url, &features(WITH)).await.unwrap();
    let sent: serde_json::Value = serde_json::from_str(&hub.seen("POST")[0]).unwrap();
    assert_eq!(
        sent,
        serde_json::json!({"kind": "webpush", "device_id": e.device().0, "endpoint": "https://push.example/secret-endpoint",
            "p256dh": "p".repeat(87), "auth": "a".repeat(22), "vapid": "v".repeat(43), "navigate": "https://chat.example/", "preview": true})
    );
    assert_eq!(state(&e), ["registered"]);
    // the same again is not sent again
    e.tell_web_push(&client, &hub.url, &features(WITH)).await.unwrap();
    assert_eq!(hub.seen("POST").len(), 1);
    // "Show message text" switched off: the hub is told
    e.set_web_push(Some(&subscription(false))).unwrap();
    assert_eq!(state(&e), ["waiting"]);
    e.tell_web_push(&client, &hub.url, &features(WITH)).await.unwrap();
    assert_eq!(hub.seen("POST").len(), 2);
    assert!(hub.seen("POST")[1].contains(r#""preview":false"#));
    // notifications off: the hub drops it, once
    e.set_web_push(None).unwrap();
    e.tell_web_push(&client, &hub.url, &features(WITH)).await.unwrap();
    e.tell_web_push(&client, &hub.url, &features(WITH)).await.unwrap();
    assert_eq!(hub.seen("DELETE").len(), 1);
    assert_eq!(state(&e), ["waiting"]);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_hub_that_cannot_take_it_is_named_and_asked_again_when_it_can() {
    let hub = fake_hub(200, 200).await;
    let (e, _) = engine_with(&[&hub.url]);
    let client = HubClient::new(HubAddress::parse(&hub.url).unwrap());
    e.set_web_push(Some(&subscription(true))).unwrap();
    // a hub from before Web Push: not asked, and named
    e.tell_web_push(&client, &hub.url, &features(&["sync", "unifiedpush"])).await.unwrap();
    assert!(hub.seen("POST").is_empty());
    assert_eq!(state(&e), ["needs_update"]);
    // updated since: it gets the subscription by itself
    e.tell_web_push(&client, &hub.url, &features(WITH)).await.unwrap();
    assert_eq!((hub.seen("POST").len(), state(&e)), (1, vec!["registered"]));

    for (code, after, asked_again) in [(422, "refused", false), (409, "waiting", true), (503, "waiting", true)] {
        let hub = fake_hub(code, 200).await;
        let (e, _) = engine_with(&[&hub.url]);
        let client = HubClient::new(HubAddress::parse(&hub.url).unwrap());
        e.set_web_push(Some(&subscription(true))).unwrap();
        e.tell_web_push(&client, &hub.url, &features(WITH)).await.unwrap();
        assert_eq!(state(&e), [after], "after a {code}");
        e.tell_web_push(&client, &hub.url, &features(WITH)).await.unwrap();
        assert_eq!(hub.seen("POST").len(), if asked_again { 2 } else { 1 }, "after a {code}");
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_removed_hub_that_could_not_be_told_means_a_new_subscription() {
    // told: nothing more to do
    let hub = fake_hub(200, 200).await;
    let (e, _) = engine_with(&[&hub.url]);
    let client = HubClient::new(HubAddress::parse(&hub.url).unwrap());
    e.set_web_push(Some(&subscription(true))).unwrap();
    e.tell_web_push(&client, &hub.url, &features(WITH)).await.unwrap();
    e.forget_web_push(&client, &hub.url).await;
    assert_eq!(hub.seen("DELETE").len(), 1);
    assert!(!e.web_push_must_resubscribe());

    // not told: the page is to make a new subscription
    let hub = fake_hub(200, 503).await;
    let (e, _) = engine_with(&[&hub.url]);
    let client = HubClient::new(HubAddress::parse(&hub.url).unwrap());
    e.set_web_push(Some(&subscription(true))).unwrap();
    e.forget_web_push(&client, &hub.url).await;
    assert!(hub.seen("DELETE").is_empty() && !e.web_push_must_resubscribe(), "a hub that never had it is not asked");
    e.tell_web_push(&client, &hub.url, &features(WITH)).await.unwrap();
    e.forget_web_push(&client, &hub.url).await;
    assert!(e.web_push_must_resubscribe());
    // the new one clears it
    e.set_web_push(Some(&WebPush { endpoint: "https://push.example/another".into(), ..subscription(true) })).unwrap();
    assert!(!e.web_push_must_resubscribe());
}
