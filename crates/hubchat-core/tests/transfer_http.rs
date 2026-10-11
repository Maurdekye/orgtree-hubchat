//! Loopback protocol regressions: no real hub, account or network registration.
use hubchat_core::hub::{CancelFlag, HubAddress, HubClient};
use hubchat_core::{Error, Identity};
use serde_json::json;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

async fn request(socket: &mut TcpStream) -> String {
    let mut bytes = Vec::new();
    loop {
        let mut byte = [0];
        socket.read_exact(&mut byte).await.unwrap();
        bytes.push(byte[0]);
        if bytes.ends_with(b"\r\n\r\n") {
            break;
        }
        assert!(bytes.len() < 16384);
    }
    String::from_utf8(bytes).unwrap()
}
async fn answer(socket: &mut TcpStream, value: serde_json::Value) {
    let body = value.to_string();
    socket.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",body.len(),body).as_bytes()).await.unwrap();
}
async fn client() -> (TcpListener, HubClient, Identity) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let hub =
        HubClient::new(HubAddress::parse(&listener.local_addr().unwrap().to_string()).unwrap());
    (listener, hub, Identity::generate("transfer-proof").unwrap())
}

#[tokio::test]
async fn cancellation_interrupts_stalled_headers_and_response_body() {
    for mode in ["upload", "download", "download-body", "resumable"] {
        let (listener, hub, me) = client().await;
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("source");
        tokio::fs::write(&source, b"xyz").await.unwrap();
        let dest = dir.path().join("dest");
        let flag = CancelFlag::default();
        let stop = flag.clone();
        let (ready, wait) = tokio::sync::oneshot::channel();
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let headers = request(&mut socket).await;
            if mode == "upload" {
                let mut bytes = [0; 3];
                socket.read_exact(&mut bytes).await.unwrap();
            }
            if mode == "download-body" {
                socket
                    .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 1024\r\n\r\nx")
                    .await
                    .unwrap();
            }
            ready.send(headers).unwrap();
            let mut rest = Vec::new();
            socket.read_to_end(&mut rest).await.unwrap();
        });
        let file = tokio::fs::File::open(&source).await.unwrap();
        let work = async {
            match mode {
                "upload" => hub
                    .upload(&me, file, "source", None, flag)
                    .await
                    .map(|_| ()),
                "resumable" => hub
                    .resume_upload(&me, "upload-id", file, None, flag)
                    .await
                    .map(|_| ()),
                _ => hub
                    .download_file(&me, "attachment-id", &dest, None, flag)
                    .await
                    .map(|_| ()),
            }
        };
        let cancellation = async {
            wait.await.unwrap();
            stop.cancel();
        };
        let (result, ()) = tokio::time::timeout(Duration::from_secs(3), async {
            tokio::join!(work, cancellation)
        })
        .await
        .expect(mode);
        assert!(
            matches!(result, Err(Error::Cancelled)),
            "{mode}: {result:?}"
        );
        assert!(!dest.exists(), "partial download must not be published");
        tokio::time::timeout(Duration::from_secs(3), server)
            .await
            .expect("cancel closes socket")
            .unwrap();
    }
}

#[tokio::test]
async fn cancellation_before_start_opens_no_connection() {
    let (listener, hub, me) = client().await;
    let dir = tempfile::tempdir().unwrap();
    let flag = CancelFlag::default();
    flag.cancel();
    assert!(matches!(
        hub.download_file(&me, "file", &dir.path().join("file"), None, flag)
            .await,
        Err(Error::Cancelled)
    ));
    assert!(
        tokio::time::timeout(Duration::from_millis(75), listener.accept())
            .await
            .is_err()
    );
}

#[tokio::test]
async fn resume_rejects_inconsistent_identity_size_offset_and_completion() {
    let states = [
        json!({"id":"wrong","bytes":3,"offset":0,"complete":false}),
        json!({"id":"same","bytes":4,"offset":0,"complete":false}),
        json!({"id":"same","bytes":3,"offset":4,"complete":false}),
        json!({"id":"same","bytes":3,"offset":1,"complete":true}),
    ];
    for state in states {
        let (listener, hub, me) = client().await;
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            request(&mut socket).await;
            answer(&mut socket, state).await;
            assert!(
                tokio::time::timeout(Duration::from_millis(75), listener.accept())
                    .await
                    .is_err(),
                "invalid status must not PATCH"
            );
        });
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("source");
        tokio::fs::write(&path, b"xyz").await.unwrap();
        let result = hub
            .resume_upload(
                &me,
                "same",
                tokio::fs::File::open(path).await.unwrap(),
                None,
                CancelFlag::default(),
            )
            .await;
        assert!(matches!(result, Err(Error::Invalid(_))), "{result:?}");
        server.await.unwrap();
    }
}

#[tokio::test]
async fn lost_completion_and_partial_resume_use_confirmed_server_offset() {
    for complete in [false, true] {
        let (listener, hub, me) = client().await;
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            request(&mut socket).await;
            answer(
                &mut socket,
                json!({"id":"same","bytes":3,"offset":if complete{3}else{2},"complete":complete}),
            )
            .await;
            drop(socket);
            if complete {
                assert!(
                    tokio::time::timeout(Duration::from_millis(75), listener.accept())
                        .await
                        .is_err()
                );
            } else {
                let (mut socket, _) = listener.accept().await.unwrap();
                let head = request(&mut socket).await;
                assert!(head.starts_with("PATCH /api/uploads/same?offset=2 "));
                let mut bytes = [0; 1];
                socket.read_exact(&mut bytes).await.unwrap();
                assert_eq!(bytes, b"z"[..]);
                answer(
                    &mut socket,
                    json!({"id":"same","bytes":3,"offset":3,"complete":true}),
                )
                .await;
            }
        });
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("source");
        tokio::fs::write(&path, b"xyz").await.unwrap();
        let result = hub
            .resume_upload(
                &me,
                "same",
                tokio::fs::File::open(path).await.unwrap(),
                None,
                CancelFlag::default(),
            )
            .await
            .unwrap();
        assert!(result.complete);
        assert_eq!(result.offset, 3);
        server.await.unwrap();
    }
}
