//! Engine-level transfer protocol recovery; all traffic stays on loopback.
use super::*;
use crate::store::{NewAttachment, NewOutgoing};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

struct HostStub;
impl Host for HostStub {
    fn event(&self, _: Event) {}
    fn open_source(&self, path: &str) -> std::io::Result<std::fs::File> {
        std::fs::File::open(path)
    }
    fn download_dir(&self) -> PathBuf {
        std::env::temp_dir()
    }
}
async fn request(s: &mut TcpStream) -> String {
    let mut buf = Vec::new();
    loop {
        let mut b = [0];
        s.read_exact(&mut b).await.unwrap();
        buf.push(b[0]);
        if buf.ends_with(b"\r\n\r\n") {
            break;
        }
        assert!(buf.len() < 16384)
    }
    String::from_utf8(buf).unwrap()
}
async fn response(s: &mut TcpStream, status: u16, value: serde_json::Value) {
    let body = value.to_string();
    s.write_all(format!("HTTP/1.1 {status} Response\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).as_bytes()).await.unwrap();
}
fn engine(store: Arc<Store>) -> Arc<Engine> {
    Engine::new(
        store,
        Identity::generate("recovery-proof").unwrap(),
        Profile {
            kind: "person".into(),
            org_name: String::new(),
            username: "test".into(),
            blurb: String::new(),
        },
        Arc::new(HostStub),
    )
}

#[tokio::test]
async fn existing_session_survives_outages_and_only_missing_or_gone_is_replaced() {
    for status in [503, 401, 404, 410] {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let client =
            HubClient::new(HubAddress::parse(&listener.local_addr().unwrap().to_string()).unwrap());
        let url = client.address().to_string();
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            assert!(request(&mut socket)
                .await
                .starts_with("GET /api/uploads/old "));
            response(
                &mut socket,
                status,
                serde_json::json!({"detail":"scripted"}),
            )
            .await;
            drop(socket);
            if matches!(status, 404 | 410) {
                let (mut socket, _) = listener.accept().await.unwrap();
                assert!(request(&mut socket).await.starts_with("POST /api/uploads "));
                response(
                    &mut socket,
                    200,
                    serde_json::json!({"id":"new","bytes":3,"offset":0,"complete":false}),
                )
                .await;
                drop(socket);
                let (mut socket, _) = listener.accept().await.unwrap();
                assert!(request(&mut socket)
                    .await
                    .starts_with("GET /api/uploads/new "));
                response(
                    &mut socket,
                    200,
                    serde_json::json!({"id":"new","bytes":3,"offset":3,"complete":true}),
                )
                .await;
            } else {
                assert!(
                    tokio::time::timeout(Duration::from_millis(75), listener.accept())
                        .await
                        .is_err(),
                    "outage must not open replacement session"
                );
            }
        });
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("source");
        tokio::fs::write(&source, b"xyz").await.unwrap();
        let store = Arc::new(Store::open_in_memory().unwrap());
        store
            .queue_outgoing(
                &NewOutgoing {
                    id: "message".into(),
                    peer: "peer".into(),
                    body: "hold this".into(),
                    kind: None,
                    reply_to: None,
                    attachments: vec![NewAttachment {
                        name: "file".into(),
                        bytes: 3,
                        source: source.to_string_lossy().into_owned(),
                    }],
                },
                "now",
            )
            .unwrap();
        let a = store
            .message("message")
            .unwrap()
            .unwrap()
            .attachments
            .remove(0);
        store
            .set_upload(&a.local_id, "uploading", "old", &url)
            .unwrap();
        let a = store
            .message("message")
            .unwrap()
            .unwrap()
            .attachments
            .remove(0);
        let engine = engine(store.clone());
        let result = engine
            .upload_resumable(
                &client,
                &url,
                &a,
                tokio::fs::File::open(source).await.unwrap(),
                Arc::new(|_, _| {}),
                CancelFlag::default(),
            )
            .await;
        let after = store
            .message("message")
            .unwrap()
            .unwrap()
            .attachments
            .remove(0);
        if matches!(status, 404 | 410) {
            assert_eq!(result.unwrap().id, "new");
            assert_eq!(after.hub_id.as_deref(), Some("new"));
        } else {
            assert_eq!(result.unwrap_err().status(), Some(status));
            assert_eq!(after.hub_id.as_deref(), Some("old"));
        }
        server.await.unwrap();
    }
}

#[tokio::test]
async fn missing_source_holds_entire_message_without_network_send() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("proof.db");
    let store = Arc::new(Store::open(&db).unwrap());
    store
        .queue_outgoing(
            &NewOutgoing {
                id: "message".into(),
                peer: "peer".into(),
                body: "must wait".into(),
                kind: None,
                reply_to: None,
                attachments: vec![NewAttachment {
                    name: "missing".into(),
                    bytes: 3,
                    source: "missing".into(),
                }],
            },
            "now",
        )
        .unwrap();
    rusqlite::Connection::open(&db)
        .unwrap()
        .execute("UPDATE attachments SET source=NULL", [])
        .unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = HubAddress::parse(&listener.local_addr().unwrap().to_string()).unwrap();
    let url = address.to_string();
    let e = engine(store.clone());
    e.hubs.lock().unwrap().insert(
        url.clone(),
        HubRuntime {
            client: HubClient::new(address),
            status: HubStatus {
                url,
                name: "mock".into(),
                state: HubState::Connected,
                error: None,
                retry_at_ms: None,
                max_attachment_bytes: 1073741824,
                features: vec!["uploads".into()],
                version: None,
                answered_ms: None,
                waiting_since_ms: None,
                clock_offset_ms: None,
            },
            retry_now: Arc::new(Notify::new()),
            kick: Arc::new(Notify::new()),
            stop: CancelFlag::default(),
        },
    );
    assert!(e.send_one("message").await.unwrap());
    let msg = store.message("message").unwrap().unwrap();
    assert_eq!(msg.state, "failed");
    assert_eq!(msg.attachments.len(), 1);
    assert_eq!(msg.attachments[0].state, "failed");
    assert!(
        tokio::time::timeout(Duration::from_millis(75), listener.accept())
            .await
            .is_err()
    );
}
