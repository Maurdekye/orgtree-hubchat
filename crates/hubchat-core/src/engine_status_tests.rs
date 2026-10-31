//! Our status (engine_status; user 2026-10-31 08:52Z): scratch stores and a
//! fake hub on 127.0.0.1 that answers the few calls a session makes; never a
//! real hub.
use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};

const A: &str = "http://127.0.0.1:7997";
const B: &str = "http://127.0.0.1:7998";

struct Events(Mutex<Vec<Event>>);
impl Host for Events {
    fn event(&self, ev: Event) { self.0.lock().unwrap().push(ev); }
    fn open_source(&self, _: &str) -> std::io::Result<std::fs::File> { unreachable!() }
    fn download_dir(&self) -> PathBuf { PathBuf::new() }
}

fn engine_with(hubs: &[&str]) -> (Arc<Engine>, Arc<Events>, Arc<Store>) {
    let store = Arc::new(Store::open_in_memory().unwrap());
    let me = Identity::generate("status-proof").unwrap();
    store.set_meta("identity.address", &me.address()).unwrap();
    for (i, h) in hubs.iter().enumerate() {
        store.add_hub(h, &format!("t{i}")).unwrap();
    }
    let events = Arc::new(Events(Mutex::new(Vec::new())));
    let e = Engine::new(store.clone(), me, Profile::default(), events.clone());
    (e, events, store)
}

/// How many times the window heard that our status changed.
fn told_window(ev: &Events) -> usize {
    ev.0.lock().unwrap().iter().filter(|e| matches!(e, Event::Status)).count()
}
fn owed(s: &Store, url: &str) -> Option<String> { s.meta(&format!("status.owed.{url}")).unwrap() }
fn said(s: &Store, url: &str) -> Option<String> { s.meta(&format!("status.said.{url}")).unwrap() }
fn some(s: &str) -> Option<String> { Some(s.into()) }

#[test]
fn a_pick_is_kept_and_owed_to_every_saved_hub() {
    let (e, ev, s) = engine_with(&[A, B]);
    assert_eq!(e.status(), "auto", "Automatic until one is picked");
    e.set_status("dnd").unwrap();
    assert_eq!((e.status(), owed(&s, A), owed(&s, B)), ("dnd".into(), some("dnd"), some("dnd")));
    assert_eq!(chosen_status(&s), "dnd", "what the shell reads for each notification");
    assert_eq!(told_window(&ev), 1, "the window hears of it");
    assert!(e.set_status("away").is_err(), "not a status");
    assert_eq!((e.status(), told_window(&ev)), ("dnd".into(), 1), "a refused pick changes nothing");
}

#[test]
fn a_changed_word_from_a_hub_is_a_pick_on_another_of_our_devices() {
    let (e, ev, s) = engine_with(&[A]);
    e.hub_said_status(A, "auto", false).unwrap();
    assert_eq!((e.status(), said(&s, A), owed(&s, A), told_window(&ev)), ("auto".into(), some("auto"), None, 0), "a first word, as ours");
    e.hub_said_status(A, "dnd", false).unwrap();
    assert_eq!((e.status(), told_window(&ev)), ("dnd".into(), 1), "picked on another device: taken");
    e.hub_said_status(A, "dnd", false).unwrap();
    assert_eq!(told_window(&ev), 1, "the same word again is no change");
    e.hub_said_status(A, "busy", false).unwrap();
    assert_eq!((e.status(), told_window(&ev)), ("busy".into(), 2));
    e.hub_said_status(A, "away", false).unwrap();
    assert_eq!((e.status(), said(&s, A)), ("busy".into(), some("busy")), "a word it doesn't know changes nothing");
}

#[test]
fn a_first_word_is_taken_when_it_is_a_choice_and_a_hub_that_never_heard_ours_is_told_it() {
    let (e, _, s) = engine_with(&[A, B]);
    e.hub_said_status(A, "busy", false).unwrap();
    assert_eq!(e.status(), "busy", "a new device takes the status chosen elsewhere");
    e.hub_said_status(B, "auto", false).unwrap();
    assert_eq!((e.status(), owed(&s, B)), ("busy".into(), some("busy")), "Automatic from a hub that never heard ours: it is owed ours");
    // a reset answer (a re-created hub database) is a first word too
    e.hub_said_status(A, "auto", true).unwrap();
    assert_eq!((e.status(), owed(&s, A)), ("busy".into(), some("busy")));
}

#[test]
fn our_pick_waits_for_its_own_word() {
    let (e, ev, s) = engine_with(&[A, B]);
    e.hub_said_status(A, "auto", false).unwrap();
    e.set_status("dnd").unwrap();
    // an answer from before the hub was told
    e.hub_said_status(A, "auto", false).unwrap();
    assert_eq!((e.status(), owed(&s, A), said(&s, A)), ("dnd".into(), some("dnd"), some("auto")), "not a change");
    e.hub_said_status(A, "dnd", false).unwrap();
    assert_eq!((owed(&s, A), said(&s, A), owed(&s, B)), (None, some("dnd"), some("dnd")), "its own word ends the wait, on that hub only");
    // then a pick on another device: taken, and owed instead to the hub still owed ours
    e.hub_said_status(A, "online", false).unwrap();
    assert_eq!((e.status(), owed(&s, B)), ("online".into(), some("online")));
    assert_eq!(told_window(&ev), 2);
}

/// A fake hub: /healthz (features sync and status), /api/register and the
/// first /api/sync answered at once (its word on our status: auto); each
/// later sync held until the client hangs up; /api/status answered with
/// `status_code`. Every request's path and body is kept.
struct FakeHub {
    url: String,
    seen: Arc<Mutex<Vec<(String, String)>>>,
    task: tokio::task::JoinHandle<()>,
}
impl Drop for FakeHub {
    fn drop(&mut self) { self.task.abort(); }
}
impl FakeHub {
    fn seen(&self, path: &str) -> Vec<String> {
        self.seen.lock().unwrap().iter().filter(|(p, _)| p == path).map(|(_, b)| b.clone()).collect()
    }
}

async fn fake_hub(status_code: u16) -> FakeHub {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let seen = Arc::new(Mutex::new(Vec::new()));
    let log = seen.clone();
    let task = tokio::spawn(async move {
        let syncs = Arc::new(AtomicUsize::new(0));
        while let Ok((mut stream, _)) = listener.accept().await {
            let (log, syncs) = (log.clone(), syncs.clone());
            tokio::spawn(async move {
                let (mut buf, mut chunk) = (Vec::new(), [0u8; 4096]);
                // the head, then the body by its length
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
                let path = head.split(' ').nth(1).unwrap_or("").split('?').next().unwrap_or("").to_string();
                log.lock().unwrap().push((path.clone(), body));
                let now = unix_ms();
                let (code, json) = match path.as_str() {
                    "/healthz" => (200, format!(r#"{{"ok":true,"name":"status-proof","version":"2.3.0","features":["sync","status"],"now":{now}}}"#)),
                    "/api/register" => (200, r#"{"name":"status-proof"}"#.to_string()),
                    "/api/sync" if syncs.fetch_add(1, Ordering::SeqCst) > 0 => {
                        // parked until the client hangs up
                        let _ = stream.read(&mut chunk).await;
                        return;
                    }
                    "/api/sync" => (200, format!(r#"{{"name":"status-proof","cursor":"1-0-0000000000000000","changes":[],"roster":[],"more":false,"now":{now},"status":"auto"}}"#)),
                    "/api/status" => (status_code, r#"{"detail":"as asked"}"#.to_string()),
                    _ => (404, r#"{"detail":"not found"}"#.to_string()),
                };
                let resp = format!("HTTP/1.1 {code} X\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{json}", json.len());
                let _ = stream.write_all(resp.as_bytes()).await;
            });
        }
    });
    FakeHub { url, seen, task }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn telling_a_hub_ends_what_it_is_owed() {
    for (code, owed_after, said_after) in [(200, None, some("dnd")), (403, None, None), (503, some("dnd"), None)] {
        let hub = fake_hub(code).await;
        let (e, _, s) = engine_with(&[&hub.url]);
        let client = HubClient::new(HubAddress::parse(&hub.url).unwrap());
        e.set_status("dnd").unwrap();
        e.tell_status(&client, &hub.url, &["sync".to_string()]).await.unwrap();
        assert_eq!((hub.seen("/api/status").len(), owed(&s, &hub.url)), (0, None), "a hub without the feature is owed nothing");
        e.set_status("dnd").unwrap();
        e.tell_status(&client, &hub.url, &["sync".to_string(), "status".to_string()]).await.unwrap();
        assert_eq!(hub.seen("/api/status"), vec![r#"{"status":"dnd"}"#.to_string()], "told ({code})");
        assert_eq!((owed(&s, &hub.url), said(&s, &hub.url)), (owed_after, said_after), "after a {code}");
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_pick_wakes_a_parked_sync_to_tell_its_hub_at_once() {
    let hub = fake_hub(200).await;
    let (e, _, s) = engine_with(&[&hub.url]);
    e.spawn_hub(HubAddress::parse(&hub.url).unwrap(), "t0".into(), None);
    let t0 = std::time::Instant::now();
    while hub.seen("/api/sync").len() < 2 {
        assert!(t0.elapsed() < Duration::from_secs(5), "never parked: {:?}", hub.seen.lock().unwrap());
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    tokio::time::sleep(Duration::from_millis(300)).await;
    let t1 = std::time::Instant::now();
    e.set_status("dnd").unwrap();
    while hub.seen("/api/status").is_empty() {
        assert!(t1.elapsed() < Duration::from_secs(3), "not told while its sync was parked: {:?}", hub.seen.lock().unwrap());
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert_eq!(hub.seen("/api/status"), vec![r#"{"status":"dnd"}"#.to_string()]);
    println!("told in {:?}, its sync parked again: {}", t1.elapsed(), hub.seen("/api/sync").len() >= 3);
    let t2 = std::time::Instant::now();
    while owed(&s, &hub.url).is_some() || hub.seen("/api/sync").len() < 3 {
        assert!(t2.elapsed() < Duration::from_secs(3), "{:?} {:?}", owed(&s, &hub.url), hub.seen.lock().unwrap());
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert_eq!(said(&s, &hub.url), some("dnd"), "its word is ours now; the sync goes on");
}
