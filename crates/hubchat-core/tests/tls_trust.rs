//! Hub connections trust what the device trusts (`hubchat_core::tls`; Orgtree
//! docket hubchat-on-a-lan-only-hub-apps-trust-local-certi): a hub on a LAN
//! has a certificate from its operator's own certificate authority (CA), and
//! the apps accept it once that CA is among the roots the device trusts,
//! beside the built-in public list. All three hub connections share the
//! rule: requests, profile pictures and the WebSocket stream.
//!
//! The fake hub speaks HTTPS and `wss` on loopback with the throwaway
//! certificates of `tests/tls` (their README says what each is). The device's
//! roots are handed in by the test (`tls::set_device_roots`), so no real
//! certificate store is read or changed.
//!
//!     cargo test -p hubchat-core --test tls_trust
//!
//! Two more tests read the REAL store of the machine and are ignored by
//! default. CI runs them on a throwaway runner, the first before `ca.pem` is
//! installed there and the second after (`.github/workflows/device-trust.yml`):
//!
//!     cargo test -p hubchat-core --test tls_trust -- --ignored os_store_without_the_test_ca
//!     cargo test -p hubchat-core --test tls_trust -- --ignored os_store_with_the_test_ca

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use hubchat_core::{tls, Error, HubAddress, HubClient, Identity};
use serde_json::{json, Value};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio_rustls::rustls::pki_types::pem::PemObject;
use tokio_rustls::rustls::pki_types::{CertificateDer, PrivateKeyDer};
use tokio_rustls::rustls::ServerConfig;
use tokio_rustls::TlsAcceptor;
use tokio_tungstenite::tungstenite::handshake::derive_accept_key;
use tokio_tungstenite::tungstenite::protocol::Role;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::WebSocketStream;

/// The device's roots are one setting of the whole process: one test at a time.
static ONE_AT_A_TIME: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests").join("tls").join(name)
}

/// A certificate file as the bytes a device's store would hand over (DER).
fn der(name: &str) -> Vec<u8> {
    CertificateDer::from_pem_file(fixture(name)).expect("a test certificate").to_vec()
}

struct Fake {
    url: String,
    /// TLS handshakes the hub completed
    handshakes: Arc<AtomicUsize>,
    upgrades: Arc<AtomicUsize>,
    http_syncs: Arc<AtomicUsize>,
    task: tokio::task::JoinHandle<()>,
}

impl Drop for Fake {
    fn drop(&mut self) {
        self.task.abort();
    }
}

/// A hub over HTTPS on 127.0.0.1 that presents `cert` (with `key`).
async fn fake(cert: &str, key: &str) -> Fake {
    let chain: Vec<CertificateDer<'static>> = CertificateDer::pem_file_iter(fixture(cert)).unwrap().map(|c| c.unwrap()).collect();
    let key = PrivateKeyDer::from_pem_file(fixture(key)).unwrap();
    let provider = Arc::new(tokio_rustls::rustls::crypto::ring::default_provider());
    let config = ServerConfig::builder_with_provider(provider).with_safe_default_protocol_versions().unwrap().with_no_client_auth().with_single_cert(chain, key).unwrap();
    let acceptor = TlsAcceptor::from(Arc::new(config));
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("https://{}", listener.local_addr().unwrap());
    let (handshakes, upgrades, http_syncs) = (Arc::new(AtomicUsize::new(0)), Arc::new(AtomicUsize::new(0)), Arc::new(AtomicUsize::new(0)));
    let (h2, u2, s2) = (handshakes.clone(), upgrades.clone(), http_syncs.clone());
    let task = tokio::spawn(async move {
        while let Ok((sock, _)) = listener.accept().await {
            let (acceptor, h, u, s) = (acceptor.clone(), h2.clone(), u2.clone(), s2.clone());
            tokio::spawn(async move {
                // a client that refuses the certificate ends the handshake here
                let Ok(tls) = acceptor.accept(sock).await else { return };
                h.fetch_add(1, Ordering::SeqCst);
                serve(tls, u, s).await;
            });
        }
    });
    Fake { url, handshakes, upgrades, http_syncs, task }
}

type Tls = tokio_rustls::server::TlsStream<tokio::net::TcpStream>;

fn sync_answer() -> Value {
    json!({ "name": "tls-fake", "version": "2.5.1", "cursor": "c0", "changes": [], "roster": [], "roster_removed": [], "more": false })
}

async fn serve(mut sock: Tls, upgrades: Arc<AtomicUsize>, http_syncs: Arc<AtomicUsize>) {
    // the request head, then as much body as its length says
    let mut seen: Vec<u8> = Vec::new();
    let head_len = loop {
        if let Some(i) = seen.windows(4).position(|w| w == b"\r\n\r\n") {
            break i + 4;
        }
        let mut more = [0u8; 4096];
        match sock.read(&mut more).await {
            Ok(n) if n > 0 => seen.extend_from_slice(&more[..n]),
            _ => return,
        }
    };
    let head = String::from_utf8_lossy(&seen[..head_len]).to_string();
    let lower = head.to_ascii_lowercase();
    let header = |name: &str| head.lines().find_map(|l| l.split_once(':').filter(|(k, _)| k.eq_ignore_ascii_case(name)).map(|(_, v)| v.trim().to_string()));
    let first = head.lines().next().unwrap_or("").to_string();
    if lower.contains("upgrade: websocket") {
        upgrades.fetch_add(1, Ordering::SeqCst);
        let accept = derive_accept_key(header("sec-websocket-key").unwrap_or_default().as_bytes());
        let answer = format!("HTTP/1.1 101 Switching Protocols\r\nupgrade: websocket\r\nconnection: Upgrade\r\nsec-websocket-accept: {accept}\r\nsec-websocket-protocol: orgtree-hub-stream.1\r\n\r\n");
        if sock.write_all(answer.as_bytes()).await.is_err() {
            return;
        }
        let mut ws = WebSocketStream::from_raw_socket(sock, Role::Server, None).await;
        let Some(Ok(Message::Text(_hello))) = ws.next().await else { return };
        let welcome = json!({ "type": "welcome", "v": 1, "hub": "tls-fake", "version": "2.5.1", "slugs": ["ann.fake"], "ping_s": 25, "max_request": 65536 });
        let _ = ws.send(Message::Text(welcome.to_string().into())).await;
        while let Some(Ok(m)) = ws.next().await {
            let Message::Text(t) = m else { continue };
            let v: Value = serde_json::from_str(t.as_str()).unwrap();
            let frame = json!({ "type": "answer", "id": v["id"], "status": 200, "body": sync_answer() });
            let _ = ws.send(Message::Text(frame.to_string().into())).await;
        }
        return;
    }
    let want: usize = header("content-length").and_then(|v| v.parse().ok()).unwrap_or(0);
    while seen.len() < head_len + want {
        let mut more = [0u8; 4096];
        match sock.read(&mut more).await {
            Ok(n) if n > 0 => seen.extend_from_slice(&more[..n]),
            _ => break,
        }
    }
    let (status, body) = if first.contains("/healthz") {
        (200, json!({ "ok": true, "name": "tls-fake", "orgs": 1, "version": "2.5.1", "features": ["sync", "stream"], "max_attachment_bytes": 1000000 }))
    } else if first.contains("/api/profile-picture") {
        (200, json!({ "picture": null }))
    } else if first.contains("/api/sync") {
        http_syncs.fetch_add(1, Ordering::SeqCst);
        (200, sync_answer())
    } else {
        (404, json!({ "detail": "Not Found" }))
    };
    let body = body.to_string();
    let _ = sock.write_all(format!("HTTP/1.1 {status} X\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}", body.len()).as_bytes()).await;
    let _ = sock.shutdown().await;
}

/// Every test starts here: one at a time, and a short wait between two looks at the device's roots.
async fn alone() -> tokio::sync::MutexGuard<'static, ()> {
    let guard = ONE_AT_A_TIME.lock().await;
    std::env::set_var("HUBCHAT_TLS_REREAD_MS", "40");
    // and the device's roots are kept this long before they are read again
    std::env::set_var("HUBCHAT_TLS_KEEP_MS", "1500");
    guard
}

/// The device trusts exactly these certificate files.
fn device_trusts(files: &[&str]) {
    let roots: Vec<Vec<u8>> = files.iter().map(|f| der(f)).collect();
    tls::set_device_roots(move || Some(roots.clone()));
}

fn client(f: &Fake) -> HubClient {
    HubClient::new(HubAddress::parse(&f.url).unwrap())
}

fn unreachable_text(r: Result<hubchat_core::hub::Health, Error>) -> String {
    match r {
        Err(Error::Unreachable(text)) => text,
        other => panic!("expected an unreachable hub, got {:?}", other.map(|h| h.name)),
    }
}

// ------------------------------------------------------------ the device trusts the CA

#[tokio::test]
async fn a_hub_with_a_private_ca_answers_once_the_device_trusts_that_ca() {
    let _alone = alone().await;
    device_trusts(&["ca.pem"]);
    let f = fake("hub.pem", "hub-key.pem").await;
    assert!(f.url.starts_with("https://127.0.0.1:"), "the certificate is checked against the IP address: {}", f.url);
    let health = client(&f).healthz().await.expect("the hub answers over HTTPS");
    assert_eq!(health.name, "tls-fake");
    assert_eq!(f.handshakes.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn the_stream_rides_the_same_trust() {
    let _alone = alone().await;
    device_trusts(&["ca.pem"]);
    let f = fake("hub.pem", "hub-key.pem").await;
    let (c, me) = (client(&f), Identity::generate("ann.fake").unwrap());
    let r = c.sync(&me, "dev1", "test", None, false, 0, None).await.expect("sync over the stream");
    assert_eq!(r.cursor, "c0");
    assert_eq!(c.transport().await.transport, "stream", "{:?}", c.transport().await);
    assert_eq!((f.upgrades.load(Ordering::SeqCst), f.http_syncs.load(Ordering::SeqCst)), (1, 0), "one wss upgrade, no POST /api/sync");
}

#[tokio::test]
async fn profile_pictures_ride_the_same_trust() {
    let _alone = alone().await;
    device_trusts(&["ca.pem"]);
    let f = fake("hub.pem", "hub-key.pem").await;
    let me = Identity::generate("ann.fake").unwrap();
    let mine = client(&f).picture(&me, &me.address()).await.expect("the picture request is answered over HTTPS");
    assert!(mine.is_none());
}

#[tokio::test]
async fn the_device_roots_are_read_once_for_a_hub_that_works() {
    let _alone = alone().await;
    let reads = Arc::new(AtomicUsize::new(0));
    let (ca, counted) = (der("ca.pem"), reads.clone());
    tls::set_device_roots(move || {
        counted.fetch_add(1, Ordering::SeqCst);
        Some(vec![ca.clone()])
    });
    let f = fake("hub.pem", "hub-key.pem").await;
    for _ in 0..3 {
        client(&f).healthz().await.expect("the hub answers");
        tokio::time::sleep(Duration::from_millis(60)).await; // longer than the wait between two looks
    }
    assert_eq!(f.handshakes.load(Ordering::SeqCst), 3);
    assert_eq!(reads.load(Ordering::SeqCst), 1, "a certificate that is accepted asks the device nothing more");
}

#[tokio::test]
async fn a_ca_installed_while_hubchat_runs_counts_without_a_restart() {
    let _alone = alone().await;
    let installed = Arc::new(AtomicBool::new(false));
    let (ca, now) = (der("ca.pem"), installed.clone());
    // at first the device's store can't even be read, then it holds the CA
    tls::set_device_roots(move || now.load(Ordering::SeqCst).then(|| vec![ca.clone()]));
    let f = fake("hub.pem", "hub-key.pem").await;
    let c = client(&f);
    let before = unreachable_text(c.healthz().await);
    assert!(before.contains("does not trust the hub's certificate"), "{before}");
    installed.store(true, Ordering::SeqCst);
    tokio::time::sleep(Duration::from_millis(80)).await;
    let health = c.healthz().await.expect("the SAME client connects once the CA is installed");
    assert_eq!(health.name, "tls-fake");
}

#[tokio::test]
async fn a_ca_removed_from_the_device_stops_counting() {
    let _alone = alone().await;
    let removed = Arc::new(AtomicBool::new(false));
    let (ca, gone) = (der("ca.pem"), removed.clone());
    tls::set_device_roots(move || Some(if gone.load(Ordering::SeqCst) { Vec::new() } else { vec![ca.clone()] }));
    let f = fake("hub.pem", "hub-key.pem").await;
    client(&f).healthz().await.expect("the hub answers while the CA is on the device");
    removed.store(true, Ordering::SeqCst);
    tokio::time::sleep(Duration::from_millis(1600)).await; // longer than the roots are kept
    // a hub this client holds no TLS session with: its certificate is checked afresh
    let again = fake("hub.pem", "hub-key.pem").await;
    let text = unreachable_text(client(&again).healthz().await);
    assert!(text.contains("does not trust the hub's certificate"), "{text}");
    assert_eq!(again.handshakes.load(Ordering::SeqCst), 0);
}

// ------------------------------------------------------------ what stays refused

#[tokio::test]
async fn without_the_ca_the_hub_is_refused_and_the_reason_is_said() {
    let _alone = alone().await;
    device_trusts(&[]);
    let f = fake("hub.pem", "hub-key.pem").await;
    let text = unreachable_text(client(&f).healthz().await);
    assert!(text.contains("does not trust the hub's certificate"), "{text}");
    assert!(!text.contains("127.0.0.1"), "no address in the reason: {text}");
    assert_eq!(f.handshakes.load(Ordering::SeqCst), 0, "the hub saw no finished handshake");
}

#[tokio::test]
async fn a_certificate_from_another_ca_stays_refused() {
    let _alone = alone().await;
    device_trusts(&["ca.pem"]);
    let f = fake("other-hub.pem", "other-hub-key.pem").await;
    let text = unreachable_text(client(&f).healthz().await);
    assert!(text.contains("does not trust the hub's certificate"), "{text}");
    let me = Identity::generate("ann.fake").unwrap();
    assert!(matches!(client(&f).picture(&me, &me.address()).await, Err(Error::Unreachable(_))));
    assert_eq!(f.handshakes.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn a_trusted_ca_does_not_excuse_a_certificate_for_another_name() {
    let _alone = alone().await;
    device_trusts(&["ca.pem"]);
    let f = fake("wrong-name.pem", "wrong-name-key.pem").await;
    let text = unreachable_text(client(&f).healthz().await);
    assert!(text.contains("the hub's certificate was refused"), "{text}");
    assert_eq!(f.handshakes.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn a_stream_to_an_untrusted_hub_is_not_opened() {
    let _alone = alone().await;
    device_trusts(&[]);
    let f = fake("hub.pem", "hub-key.pem").await;
    let (c, me) = (client(&f), Identity::generate("ann.fake").unwrap());
    assert!(matches!(c.sync(&me, "dev1", "test", None, false, 0, None).await, Err(Error::Unreachable(_))));
    assert_eq!((f.handshakes.load(Ordering::SeqCst), f.upgrades.load(Ordering::SeqCst)), (0, 0));
}

// ------------------------------------------------------------ the machine's real store

/// Before the test CA is in the machine's own store: refused.
#[tokio::test]
#[ignore = "reads the machine's real certificate store; CI runs it before installing tests/tls/ca.pem"]
async fn os_store_without_the_test_ca() {
    let _alone = alone().await;
    tls::reset_device_roots();
    let f = fake("hub.pem", "hub-key.pem").await;
    let text = unreachable_text(client(&f).healthz().await);
    assert!(text.contains("does not trust the hub's certificate"), "{text}");
}

/// After the test CA was installed in the machine's own store: requests and the stream connect.
#[tokio::test]
#[ignore = "reads the machine's real certificate store; CI runs it after installing tests/tls/ca.pem"]
async fn os_store_with_the_test_ca() {
    let _alone = alone().await;
    tls::reset_device_roots();
    let f = fake("hub.pem", "hub-key.pem").await;
    let (c, me) = (client(&f), Identity::generate("ann.fake").unwrap());
    assert_eq!(c.healthz().await.expect("the machine's store holds the test CA").name, "tls-fake");
    c.sync(&me, "dev1", "test", None, false, 0, None).await.expect("sync over the stream");
    assert_eq!(c.transport().await.transport, "stream");
    // a certificate from a CA the machine doesn't know is still refused
    let other = fake("other-hub.pem", "other-hub-key.pem").await;
    assert!(matches!(client(&other).healthz().await, Err(Error::Unreachable(_))));
}
