//! Leaving this device (engine `leave_hubs`): scratch stores and fake hubs on
//! 127.0.0.1; never a real hub.
use super::*;

struct Quiet;
impl Host for Quiet {
    fn event(&self, _: Event) {}
    fn open_source(&self, _: &str) -> std::io::Result<std::fs::File> { unreachable!() }
    fn download_dir(&self) -> PathBuf { PathBuf::new() }
}

fn engine_with(hubs: &[(&str, &str)]) -> Arc<Engine> {
    let store = Arc::new(Store::open_in_memory().unwrap());
    let me = Identity::generate("leave-proof").unwrap();
    for (i, (url, name)) in hubs.iter().enumerate() {
        let url = HubAddress::parse(url).unwrap().to_string();
        store.add_hub(&url, &format!("t{i}")).unwrap();
        if !name.is_empty() {
            store.hub_ok(&url, name, None, "t").unwrap();
        }
    }
    Engine::new(store, me, Profile::default(), Arc::new(Quiet))
}

/// What a fake hub was asked: method, path, whether it carried the identity's signature.
type Seen = Arc<Mutex<Vec<(String, String, bool)>>>;

struct FakeHub {
    url: String,
    seen: Seen,
    task: tokio::task::JoinHandle<()>,
}
impl Drop for FakeHub {
    fn drop(&mut self) { self.task.abort(); }
}

/// `answer`: the status for every request; None: take it and never answer.
async fn fake_hub(answer: Option<u16>) -> FakeHub {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let seen: Seen = Arc::new(Mutex::new(Vec::new()));
    let log = seen.clone();
    let task = tokio::spawn(async move {
        while let Ok((mut stream, _)) = listener.accept().await {
            let log = log.clone();
            tokio::spawn(async move {
                let (mut buf, mut chunk) = (Vec::new(), [0u8; 4096]);
                let head = loop {
                    let n = stream.read(&mut chunk).await.unwrap_or(0);
                    if n == 0 { return; }
                    buf.extend_from_slice(&chunk[..n]);
                    if let Some(at) = buf.windows(4).position(|w| w == b"\r\n\r\n") { break String::from_utf8_lossy(&buf[..at]).to_string(); }
                };
                let method = head.split(' ').next().unwrap_or("").to_string();
                let path = head.split(' ').nth(1).unwrap_or("").split('?').next().unwrap_or("").to_string();
                let signed = head.lines().any(|l| l.to_ascii_lowercase().starts_with("x-org-auth:"));
                log.lock().unwrap().push((method, path, signed));
                let Some(code) = answer else {
                    tokio::time::sleep(Duration::from_secs(600)).await;
                    return;
                };
                let json = "{}";
                let resp = format!("HTTP/1.1 {code} X\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{json}", json.len());
                let _ = stream.write_all(resp.as_bytes()).await;
            });
        }
    });
    FakeHub { url, seen, task }
}

/// An address where nothing listens.
async fn closed_port() -> String {
    let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", l.local_addr().unwrap());
    drop(l);
    url
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn every_hub_forgets_this_devices_push_and_the_device_and_the_unreached_are_named() {
    let (hub, old, refusing) = (fake_hub(Some(200)).await, fake_hub(Some(404)).await, fake_hub(Some(500)).await);
    let gone = closed_port().await;
    let e = engine_with(&[(&hub.url, "home"), (&old.url, "old hub"), (&refusing.url, "flaky"), (&gone, "")]);
    let device = e.device().0;
    let missed: Vec<String> = e.leave_hubs(None, Duration::from_secs(20)).await.unwrap().into_iter().map(|u| u.name).collect();
    // a hub that answers is asked both, signed; push first, then the device
    let asked = hub.seen.lock().unwrap().clone();
    assert_eq!(
        asked,
        vec![("DELETE".to_string(), "/api/push".to_string(), true), ("DELETE".to_string(), format!("/api/devices/{device}"), true)]
    );
    // an older hub has neither (404): nothing to do, so it counts as told
    assert_eq!(old.seen.lock().unwrap().len(), 2);
    // a hub that refuses and one that isn't there are named (by name, else by address)
    let mut missed = missed;
    missed.sort();
    let mut want = vec!["flaky".to_string(), HubAddress::parse(&gone).unwrap().to_string()];
    want.sort();
    assert_eq!(missed, want);
    // the engine was never started: nothing above went through a running session
    e.shutdown();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_hub_that_never_answers_is_given_up_on_at_the_budget() {
    let (hub, silent) = (fake_hub(Some(200)).await, fake_hub(None).await);
    let e = engine_with(&[(&hub.url, "home"), (&silent.url, "silent")]);
    let started = std::time::Instant::now();
    let missed = e.leave_hubs(None, Duration::from_millis(500)).await.unwrap();
    assert!(started.elapsed() < Duration::from_secs(5), "took {:?}", started.elapsed());
    assert_eq!(missed, vec![Unreached { url: HubAddress::parse(&silent.url).unwrap().to_string(), name: "silent".into() }]);
    assert_eq!(hub.seen.lock().unwrap().len(), 2, "the answering hub was told in the meantime");
    e.shutdown();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn with_no_hubs_there_is_nobody_to_tell() {
    let e = engine_with(&[]);
    assert!(e.leave_hubs(None, Duration::from_secs(1)).await.unwrap().is_empty());
    e.shutdown();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_retry_asks_only_the_hubs_it_names() {
    let (told, missed) = (fake_hub(Some(200)).await, fake_hub(Some(200)).await);
    let e = engine_with(&[(&told.url, "home"), (&missed.url, "office")]);
    let only = [HubAddress::parse(&missed.url).unwrap().to_string()];
    assert!(e.leave_hubs(Some(&only), Duration::from_secs(20)).await.unwrap().is_empty());
    assert!(told.seen.lock().unwrap().is_empty(), "a hub already told isn't asked again");
    assert_eq!(missed.seen.lock().unwrap().len(), 2);
    // nothing left to retry: nobody is asked
    assert!(e.leave_hubs(Some(&[]), Duration::from_secs(1)).await.unwrap().is_empty());
    assert_eq!(missed.seen.lock().unwrap().len(), 2);
    e.shutdown();
}
