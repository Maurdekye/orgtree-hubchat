//! The hub stream (Mail hub 2.5.0, `GET /api/stream`; Orgtree docket
//! mail-hub-websocket-for-new-mail-clients-prefer-i, spec section 9 and 15.2
//! S1 to S8): `HubClient::sync` goes over the stream when the hub lists
//! `stream` and over HTTP otherwise, by the spec's fallback rules, and
//! returns the same `SyncResult` either way.
//!
//! The fake hub speaks HTTP (`/healthz`, `/api/sync`, `/api/register`) and
//! the WebSocket side on one port, and counts what it was asked. The test
//! knob `HUBCHAT_STREAM_TIME_DIV` divides every stream timer (a minute of
//! back-off becomes 60 s / DIV).
//!
//!     cargo test -p hubchat-core --test stream

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use hubchat_core::hub_v2::{Change, SyncResult};
use hubchat_core::{Error, HubAddress, HubClient, Identity};
use serde_json::{json, Value};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio_tungstenite::tungstenite::handshake::server::{Request, Response};
use tokio_tungstenite::tungstenite::protocol::CloseFrame;
use tokio_tungstenite::tungstenite::Message;

/// What one stream connection does (by its number, 1-based; later ones are Normal).
#[derive(Clone, Debug, PartialEq)]
enum Plan {
    Normal,
    /// answer nothing: drop the socket when the first request arrives
    DropOnRequest,
    /// close with a code right after the welcome
    CloseAfterWelcome(u16),
    /// close with a code instead of welcoming
    CloseAtHello(u16),
}

#[derive(Default)]
struct State {
    features: Vec<String>,
    version: String,
    /// the answer to a WebSocket upgrade: 0 = accept it, else this status, empty body
    upgrade_status: u16,
    plans: Vec<Plan>,
    messages: Vec<Value>,
    /// a refused upgrade turns the hub into a 2.4.0 one (no stream, version 2.4.0)
    downgrade_on_refusal: bool,
    /// every request frame seen: (connection, id, cursor)
    frames: Vec<(usize, u64, Option<String>)>,
}

struct Fake {
    url: String,
    state: Arc<Mutex<State>>,
    upgrades: Arc<AtomicUsize>,
    http_syncs: Arc<AtomicUsize>,
    connections: Arc<AtomicUsize>,
    task: tokio::task::JoinHandle<()>,
}

impl Drop for Fake {
    fn drop(&mut self) {
        self.task.abort();
    }
}

fn msg(n: usize) -> Value {
    json!({ "type": "message", "message": {
        "id": format!("m{n}"), "from": "bob.fake", "to": "ann.fake", "body": format!("body {n}"),
        "received_at": "2026-11-11T12:00:00.000Z", "attachments": [] } })
}

/// The sync answer for a cursor `c<k>`: the messages from k on.
fn answer(s: &State, cursor: Option<&str>) -> Value {
    let k = cursor.and_then(|c| c.strip_prefix('c')).and_then(|n| n.parse::<usize>().ok()).unwrap_or(0);
    json!({ "name": "fake", "version": s.version, "cursor": format!("c{}", s.messages.len()),
            "changes": s.messages[k.min(s.messages.len())..], "roster": [], "roster_removed": [], "more": false })
}

async fn fake(features: &[&str]) -> Fake {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let state = Arc::new(Mutex::new(State { features: features.iter().map(|f| f.to_string()).collect(), version: "2.5.0".into(), ..Default::default() }));
    let (upgrades, http_syncs, connections) = (Arc::new(AtomicUsize::new(0)), Arc::new(AtomicUsize::new(0)), Arc::new(AtomicUsize::new(0)));
    let (s2, u2, h2, c2) = (state.clone(), upgrades.clone(), http_syncs.clone(), connections.clone());
    let task = tokio::spawn(async move {
        while let Ok((sock, _)) = listener.accept().await {
            let (s, u, h, c) = (s2.clone(), u2.clone(), h2.clone(), c2.clone());
            tokio::spawn(async move { serve(sock, s, u, h, c).await });
        }
    });
    Fake { url, state, upgrades, http_syncs, connections, task }
}

async fn serve(mut sock: TcpStream, state: Arc<Mutex<State>>, upgrades: Arc<AtomicUsize>, http_syncs: Arc<AtomicUsize>, connections: Arc<AtomicUsize>) {
    // read the request head without consuming it (the WebSocket accept reads it again)
    let mut buf = vec![0u8; 8192];
    let mut n;
    loop {
        n = sock.peek(&mut buf).await.unwrap_or(0);
        if n == 0 {
            return;
        }
        if buf[..n].windows(4).any(|w| w == b"\r\n\r\n") {
            break;
        }
        tokio::time::sleep(Duration::from_millis(2)).await;
    }
    let head = String::from_utf8_lossy(&buf[..n]).to_string();
    let first = head.lines().next().unwrap_or("").to_string();
    if head.to_ascii_lowercase().contains("upgrade: websocket") {
        upgrades.fetch_add(1, Ordering::SeqCst);
        let status = state.lock().unwrap().upgrade_status;
        if status != 0 {
            let mut st = state.lock().unwrap();
            if st.downgrade_on_refusal {
                st.features = vec!["sync".into()];
                st.version = "2.4.0".into();
            }
            drop(st);
            let _ = sock.read(&mut buf).await;
            let _ = sock.write_all(format!("HTTP/1.1 {status} X\r\ncontent-length: 0\r\nconnection: close\r\n\r\n").as_bytes()).await;
            return;
        }
        let conn = connections.fetch_add(1, Ordering::SeqCst) + 1;
        let plan = state.lock().unwrap().plans.get(conn - 1).cloned().unwrap_or(Plan::Normal);
        let cb = |_req: &Request, mut resp: Response| {
            resp.headers_mut().insert("sec-websocket-protocol", "orgtree-hub-stream.1".parse().unwrap());
            Ok(resp)
        };
        let Ok(mut ws) = tokio_tungstenite::accept_hdr_async(sock, cb).await else { return };
        stream_session(&mut ws, state, conn, plan).await;
        return;
    }
    // plain HTTP: consume the head and answer by path
    let mut total = 0;
    while total < n {
        let k = sock.read(&mut buf[..n - total]).await.unwrap_or(0);
        if k == 0 {
            break;
        }
        total += k;
    }
    let (status, body) = if first.contains("/healthz") {
        let st = state.lock().unwrap();
        (200, json!({ "ok": true, "name": "fake", "orgs": 1, "version": st.version, "features": st.features, "max_attachment_bytes": 1000000 }))
    } else if first.contains("/api/register") {
        (200, json!({ "ok": true }))
    } else if first.contains("/api/sync") {
        http_syncs.fetch_add(1, Ordering::SeqCst);
        let mut rest = vec![0u8; 8192];
        let k = tokio::time::timeout(Duration::from_millis(200), sock.read(&mut rest)).await.ok().and_then(|r| r.ok()).unwrap_or(0);
        let text = String::from_utf8_lossy(&rest[..k]).to_string();
        let cursor = text.find('{').and_then(|i| serde_json::from_str::<Value>(&text[i..]).ok()).and_then(|v| v["cursor"].as_str().map(str::to_string));
        let st = state.lock().unwrap();
        (200, answer(&st, cursor.as_deref()))
    } else {
        (404, json!({ "detail": "Not Found" }))
    };
    let body = body.to_string();
    let _ = sock.write_all(format!("HTTP/1.1 {status} X\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}", body.len()).as_bytes()).await;
}

type Ws = tokio_tungstenite::WebSocketStream<TcpStream>;

async fn stream_session(ws: &mut Ws, state: Arc<Mutex<State>>, conn: usize, plan: Plan) {
    // hello
    let Some(Ok(Message::Text(_hello))) = ws.next().await else { return };
    if let Plan::CloseAtHello(code) = plan {
        let _ = ws.send(Message::Close(Some(CloseFrame { code: code.into(), reason: "no".into() }))).await;
        return;
    }
    let version = state.lock().unwrap().version.clone();
    let welcome = json!({ "type": "welcome", "v": 1, "hub": "fake", "version": version, "slugs": ["ann.fake"], "ping_s": 25, "max_request": 65536 });
    let _ = ws.send(Message::Text(welcome.to_string().into())).await;
    if let Plan::CloseAfterWelcome(code) = plan {
        let _ = ws.send(Message::Close(Some(CloseFrame { code: code.into(), reason: "bye".into() }))).await;
        return;
    }
    // the request being waited on: (id, cursor, wait seconds, when it came)
    let mut parked: Option<(u64, Option<String>, f64, std::time::Instant)> = None;
    let mut tick = tokio::time::interval(Duration::from_millis(10));
    loop {
        tokio::select! {
            m = ws.next() => {
                let Some(Ok(m)) = m else { return };
                let Message::Text(t) = m else { continue };
                let v: Value = serde_json::from_str(t.as_str()).unwrap();
                let id = v["id"].as_u64().unwrap();
                let cursor = v["body"]["cursor"].as_str().map(str::to_string);
                let wait = v["body"]["wait"].as_f64().unwrap_or(0.0);
                state.lock().unwrap().frames.push((conn, id, cursor.clone()));
                if plan == Plan::DropOnRequest {
                    return;
                }
                // a newer request replaces the parked one, which is never answered
                parked = Some((id, cursor, wait, std::time::Instant::now()));
            }
            _ = tick.tick() => {}
        }
        if let Some((id, cursor, wait, at)) = parked.clone() {
            let body = {
                let st = state.lock().unwrap();
                answer(&st, cursor.as_deref())
            };
            let news = !body["changes"].as_array().unwrap().is_empty();
            // the test hub parks for at most 3 s whatever `wait` says
            if wait == 0.0 || news || at.elapsed() > Duration::from_secs(3) {
                parked = None;
                let frame = json!({ "type": "answer", "id": id, "status": 200, "body": body });
                let _ = ws.send(Message::Text(frame.to_string().into())).await;
            }
        }
    }
}

fn client(f: &Fake) -> (HubClient, Identity) {
    std::env::set_var("HUBCHAT_STREAM_TIME_DIV", "600"); // a minute = 100 ms, a second = 1.7 ms
    let me = Identity::generate("ann.fake").unwrap();
    (HubClient::new(HubAddress::parse(&f.url).unwrap()), me)
}

async fn sync(c: &HubClient, me: &Identity, cursor: Option<&str>, wait: u64) -> Result<SyncResult, Error> {
    c.sync(me, "dev1", "test", cursor, false, wait, None).await
}

fn ids(r: &SyncResult) -> Vec<String> {
    r.changes.iter().filter_map(|c| match c { Change::Message(m) => Some(m.env.id.clone()), _ => None }).collect()
}

// -------------------------------------------------------------------- S1

#[tokio::test]
async fn s1_a_hub_without_stream_gets_no_upgrade_attempt() {
    let f = fake(&["sync"]).await;
    let (c, me) = client(&f);
    f.state.lock().unwrap().messages.push(msg(1));
    let r = sync(&c, &me, None, 0).await.unwrap();
    assert_eq!(ids(&r), ["m1"]);
    assert_eq!((f.upgrades.load(Ordering::SeqCst), f.http_syncs.load(Ordering::SeqCst)), (0, 1));
}

// -------------------------------------------------------------------- S2

#[tokio::test]
async fn s2_stream_listed_means_zero_http_syncs() {
    let f = fake(&["sync", "stream"]).await;
    let (c, me) = client(&f);
    let first = sync(&c, &me, None, 0).await.unwrap();
    assert!(ids(&first).is_empty());
    let (c2, me2, state) = (c.clone(), me.clone(), f.state.clone());
    let parked = tokio::spawn(async move { sync(&c2, &me2, Some("c0"), 55).await });
    tokio::time::sleep(Duration::from_millis(150)).await;
    state.lock().unwrap().messages.push(msg(1));
    let r = tokio::time::timeout(Duration::from_secs(5), parked).await.unwrap().unwrap().unwrap();
    assert_eq!(ids(&r), ["m1"]);
    assert_eq!((f.upgrades.load(Ordering::SeqCst), f.http_syncs.load(Ordering::SeqCst)), (1, 0), "one upgrade, zero POST /api/sync");
}

// -------------------------------------------------------------------- S3

#[tokio::test]
async fn s3_a_403_empty_handshake_re_probes_and_a_new_version_brings_the_stream_back() {
    let f = fake(&["sync", "stream"]).await;
    f.state.lock().unwrap().upgrade_status = 403; // the hub was downgraded behind our back
    let (c, me) = client(&f);
    f.state.lock().unwrap().messages.push(msg(1));
    f.state.lock().unwrap().downgrade_on_refusal = true;
    // the first call sees `stream` listed, is refused 403-empty, looks again and finds none
    let r = sync(&c, &me, None, 0).await.unwrap();
    assert_eq!(ids(&r), ["m1"]);
    let after_first = f.upgrades.load(Ordering::SeqCst);
    assert_eq!(after_first, 1, "the one refused attempt");
    let r = sync(&c, &me, Some("c1"), 0).await.unwrap();
    assert!(ids(&r).is_empty());
    assert_eq!(f.upgrades.load(Ordering::SeqCst), after_first, "no further attempt against a hub without the feature");
    // the hub is updated again: a new version in a long-poll answer sends us to probe, then to the stream
    {
        let mut st = f.state.lock().unwrap();
        st.features = vec!["sync".into(), "stream".into()];
        st.version = "2.5.0".into();
        st.upgrade_status = 0;
    }
    let _ = sync(&c, &me, Some("c1"), 0).await.unwrap(); // its answer says 2.5.0: re-probe
    let _ = sync(&c, &me, Some("c1"), 0).await.unwrap();
    assert!(f.connections.load(Ordering::SeqCst) >= 1, "back on the stream");
}

// -------------------------------------------------------------------- S4

#[tokio::test]
async fn s4_a_refused_handshake_with_the_feature_listed_falls_back_and_retries_on_a_schedule() {
    let f = fake(&["sync", "stream"]).await;
    f.state.lock().unwrap().upgrade_status = 426;
    let (c, me) = client(&f);
    let r = sync(&c, &me, None, 0).await.unwrap();
    assert!(ids(&r).is_empty());
    assert_eq!((f.upgrades.load(Ordering::SeqCst), f.http_syncs.load(Ordering::SeqCst)), (1, 1), "tried once, then the long poll");
    let _ = sync(&c, &me, Some("c0"), 0).await.unwrap();
    assert_eq!(f.upgrades.load(Ordering::SeqCst), 1, "not again before the first minute is up");
    tokio::time::sleep(Duration::from_millis(130)).await; // 1 "minute" = 100 ms
    let _ = sync(&c, &me, Some("c0"), 0).await.unwrap();
    assert_eq!(f.upgrades.load(Ordering::SeqCst), 2, "the 1-minute retry");
    f.state.lock().unwrap().upgrade_status = 0;
    tokio::time::sleep(Duration::from_millis(560)).await; // the next step is 5 "minutes"
    let _ = sync(&c, &me, Some("c0"), 0).await.unwrap();
    assert_eq!(f.connections.load(Ordering::SeqCst), 1, "the 5-minute retry found a working stream");
}

// -------------------------------------------------------------------- S5

#[tokio::test]
async fn s5_a_stream_dropped_while_parked_is_asked_again_with_the_same_cursor() {
    let f = fake(&["sync", "stream"]).await;
    f.state.lock().unwrap().plans = vec![Plan::DropOnRequest];
    f.state.lock().unwrap().messages.push(msg(1));
    let (c, me) = client(&f);
    let r = tokio::time::timeout(Duration::from_secs(10), sync(&c, &me, Some("c0"), 55)).await.unwrap().unwrap();
    assert_eq!(ids(&r), ["m1"], "delivered once");
    let frames = f.state.lock().unwrap().frames.clone();
    let cursors: Vec<Option<String>> = frames.iter().map(|(_, _, cur)| cur.clone()).collect();
    assert_eq!(cursors, [Some("c0".into()), Some("c0".into())], "the same cursor on the new stream: {frames:?}");
    assert_eq!(f.http_syncs.load(Ordering::SeqCst), 0);
}

// -------------------------------------------------------------------- S6

#[tokio::test]
async fn s6_three_short_lived_streams_make_the_stream_unusable() {
    let f = fake(&["sync", "stream"]).await;
    f.state.lock().unwrap().plans = vec![Plan::CloseAfterWelcome(1012); 3];
    f.state.lock().unwrap().messages.push(msg(1));
    let (c, me) = client(&f);
    let r = tokio::time::timeout(Duration::from_secs(20), sync(&c, &me, Some("c0"), 0)).await.unwrap().unwrap();
    assert_eq!(ids(&r), ["m1"]);
    assert_eq!(f.upgrades.load(Ordering::SeqCst), 3, "three attempts, then the long poll");
    assert_eq!(f.http_syncs.load(Ordering::SeqCst), 1);
}

// -------------------------------------------------------------------- S7

#[tokio::test]
async fn s7_4401_is_a_401_once_and_4429_means_the_long_poll_for_a_while() {
    let f = fake(&["sync", "stream"]).await;
    f.state.lock().unwrap().plans = vec![Plan::CloseAtHello(4401)];
    let (c, me) = client(&f);
    let e = sync(&c, &me, None, 0).await.unwrap_err();
    assert_eq!(e.status(), Some(401), "handled as an HTTP 401 (the engine registers and asks again): {e}");
    let r = sync(&c, &me, None, 0).await.unwrap(); // the second ask: a stream that works
    assert!(ids(&r).is_empty());
    assert_eq!(f.http_syncs.load(Ordering::SeqCst), 0);

    let g = fake(&["sync", "stream"]).await;
    g.state.lock().unwrap().plans = vec![Plan::CloseAfterWelcome(4429)];
    let (c, me) = client(&g);
    let _ = sync(&c, &me, None, 0).await.unwrap();
    assert_eq!(g.http_syncs.load(Ordering::SeqCst), 1, "4429: the long poll");
    let _ = sync(&c, &me, Some("c0"), 0).await.unwrap();
    assert_eq!(g.upgrades.load(Ordering::SeqCst), 1, "no new attempt for 15 'minutes'");
}

// -------------------------------------------------------------------- S8

#[tokio::test]
async fn s8_a_cancelled_sync_keeps_the_socket_and_the_next_one_supersedes_it() {
    let f = fake(&["sync", "stream"]).await;
    let (c, me) = client(&f);
    let _ = sync(&c, &me, None, 0).await.unwrap();
    // the status picker cancels a parked sync (the engine's status_wake)
    let parked = tokio::time::timeout(Duration::from_millis(200), sync(&c, &me, Some("c0"), 55)).await;
    assert!(parked.is_err(), "it was parked");
    f.state.lock().unwrap().messages.push(msg(1));
    let r = tokio::time::timeout(Duration::from_secs(5), sync(&c, &me, Some("c0"), 55)).await.unwrap().unwrap();
    assert_eq!(ids(&r), ["m1"], "one answer processed");
    assert_eq!((f.connections.load(Ordering::SeqCst), f.http_syncs.load(Ordering::SeqCst)), (1, 0), "the socket stayed open");
    let frames = f.state.lock().unwrap().frames.clone();
    assert!(frames.windows(2).all(|w| w[0].1 < w[1].1), "ids rise: {frames:?}");
}
