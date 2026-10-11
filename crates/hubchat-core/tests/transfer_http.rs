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

use hubchat_core::download::{DownloadCheckpoint, DownloadSpec, SaveCheckpoint};
use std::sync::{Arc, Mutex};
fn checkpoint(n: u64, validator: Option<&str>) -> DownloadCheckpoint {
    DownloadCheckpoint {
        confirmed: n,
        validator: validator.map(str::to_owned),
        updated_ms: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64,
    }
}
fn save_checkpoint(
    path: std::path::PathBuf,
    value: Arc<Mutex<DownloadCheckpoint>>,
) -> SaveCheckpoint {
    Arc::new(move |cp| {
        assert!(std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0) >= cp.confirmed);
        *value.lock().unwrap() = cp.clone();
        Ok(())
    })
}

#[tokio::test]
async fn download_restarts_from_durable_checkpoint_without_unconfirmed_tail() {
    let (listener, hub, me) = client().await;
    let dir = tempfile::tempdir().unwrap();
    let partial = dir.path().join("private.part");
    let state = Arc::new(Mutex::new(DownloadCheckpoint::default()));
    let save = save_checkpoint(partial.clone(), state.clone());
    let total = 9 * 1024 * 1024;
    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        assert!(!request(&mut socket)
            .await
            .to_ascii_lowercase()
            .contains("\r\nrange:"));
        socket.write_all(format!("HTTP/1.1 200 OK\r\nContent-Length: {total}\r\nETag: \"v1\"\r\nConnection: close\r\n\r\n").as_bytes()).await.unwrap();
        socket
            .write_all(&vec![41u8; 8 * 1024 * 1024])
            .await
            .unwrap();
        socket.shutdown().await.unwrap();
        drop(socket);
        let (mut socket, _) = listener.accept().await.unwrap();
        let headers = request(&mut socket).await.to_ascii_lowercase();
        assert!(headers.contains("range: bytes=8388608-"));
        assert!(headers.contains("if-range: \"v1\""));
        socket.write_all(b"HTTP/1.1 206 Partial Content\r\nContent-Length: 1048576\r\nContent-Range: bytes 8388608-9437183/9437184\r\nETag: \"v1\"\r\nConnection: close\r\n\r\n").await.unwrap();
        socket.write_all(&vec![42u8; 1024 * 1024]).await.unwrap();
    });
    assert!(hub
        .download_resumable(
            &me,
            DownloadSpec {
                attachment_id: "file",
                partial: &partial,
                total,
                checkpoint: DownloadCheckpoint::default()
            },
            save.clone(),
            None,
            CancelFlag::default()
        )
        .await
        .is_err());
    let saved = state.lock().unwrap().clone();
    assert_eq!(saved.confirmed, 8 * 1024 * 1024);
    // Bytes beyond the durable checkpoint can come from the interrupted write.
    let mut file = tokio::fs::OpenOptions::new()
        .append(true)
        .open(&partial)
        .await
        .unwrap();
    file.write_all(b"unconfirmed tail").await.unwrap();
    drop(file);
    let done = hub
        .download_resumable(
            &me,
            DownloadSpec {
                attachment_id: "file",
                partial: &partial,
                total,
                checkpoint: saved,
            },
            save,
            None,
            CancelFlag::default(),
        )
        .await
        .unwrap();
    assert_eq!(done.confirmed, total);
    let bytes = std::fs::read(partial).unwrap();
    assert!(bytes[..8 * 1024 * 1024].iter().all(|x| *x == 41));
    assert!(bytes[8 * 1024 * 1024..].iter().all(|x| *x == 42));
    server.await.unwrap();
}

#[tokio::test]
async fn download_eof_checkpoint_and_validator_replacement_are_safe() {
    for mode in [
        "eof",
        "changed",
        "bad-range",
        "bad-validator",
        "expired",
        "416",
    ] {
        let (listener, hub, me) = client().await;
        let dir = tempfile::tempdir().unwrap();
        let partial = dir.path().join("private.part");
        std::fs::write(
            &partial,
            if mode == "eof" {
                b"abc".as_slice()
            } else {
                b"ab".as_slice()
            },
        )
        .unwrap();
        let mut initial = checkpoint(if mode == "eof" { 3 } else { 2 }, Some("\"v1\""));
        if mode == "expired" {
            initial.updated_ms = 1;
        }
        let state = Arc::new(Mutex::new(initial.clone()));
        let save = save_checkpoint(partial.clone(), state.clone());
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let headers = request(&mut socket).await.to_ascii_lowercase();
            if mode == "expired" {
                assert!(!headers.contains("\r\nrange:"));
            } else {
                assert!(headers.contains("range: bytes=2-"));
            }
            let answer=match mode {
                "eof" => "HTTP/1.1 206 Partial Content\r\nContent-Length: 1\r\nContent-Range: bytes 2-2/3\r\nETag: \"v1\"\r\n\r\nc",
                "changed" | "expired" => "HTTP/1.1 200 OK\r\nContent-Length: 3\r\nETag: \"v2\"\r\n\r\nxyz",
                "bad-range" => "HTTP/1.1 206 Partial Content\r\nContent-Length: 1\r\nContent-Range: bytes 1-2/3\r\nETag: \"v1\"\r\n\r\nc",
                "416" => "HTTP/1.1 416 Range Not Satisfiable\r\nContent-Length: 0\r\n\r\n",
                _ => "HTTP/1.1 206 Partial Content\r\nContent-Length: 1\r\nContent-Range: bytes 2-2/3\r\nETag: \"v2\"\r\n\r\nc"
            };
            socket.write_all(answer.as_bytes()).await.unwrap();
        });
        let result = hub
            .download_resumable(
                &me,
                DownloadSpec {
                    attachment_id: "file",
                    partial: &partial,
                    total: 3,
                    checkpoint: initial,
                },
                save,
                None,
                CancelFlag::default(),
            )
            .await;
        match mode {
            "eof" => {
                assert_eq!(result.unwrap().confirmed, 3);
                assert_eq!(std::fs::read(&partial).unwrap(), b"abc");
            }
            "changed" | "expired" => {
                assert_eq!(result.unwrap().confirmed, 3);
                assert_eq!(std::fs::read(&partial).unwrap(), b"xyz");
            }
            "416" => {
                assert!(result.is_err());
                assert_eq!(state.lock().unwrap().confirmed, 0);
            }
            _ => {
                assert!(result.is_err());
                assert_eq!(std::fs::read(&partial).unwrap(), b"ab");
            }
        }
        server.await.unwrap();
    }
}

#[tokio::test]
async fn upload_pieces_checkpoint_only_acknowledged_bytes_before_continuing() {
    use std::sync::{Arc, Mutex};
    const PIECE: usize = 8 * 1024 * 1024;
    const TOTAL: usize = PIECE + 1024 * 1024;
    for mode in ["normal", "checkpoint-failed", "invalid-ack"] {
        let (listener, hub, me) = client().await;
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("source");
        tokio::fs::write(&source, vec![42u8; TOTAL]).await.unwrap();
        let saved = Arc::new(Mutex::new(Vec::new()));
        let observed = saved.clone();
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            assert!(request(&mut socket).await.starts_with("GET "));
            answer(
                &mut socket,
                json!({"id":"same","bytes":TOTAL,"offset":0,"complete":false}),
            )
            .await;
            let (mut socket, _) = listener.accept().await.unwrap();
            let headers = request(&mut socket).await;
            assert!(headers.starts_with("PATCH /api/uploads/same?offset=0 "));
            assert!(headers
                .to_lowercase()
                .contains(&format!("content-length: {PIECE}")));
            let mut bytes = vec![0; PIECE];
            socket.read_exact(&mut bytes).await.unwrap();
            assert!(bytes.iter().all(|b| *b == 42));
            assert_eq!(
                *observed.lock().unwrap(),
                vec![0],
                "wire bytes are not confirmation"
            );
            answer(&mut socket,json!({"id":"same","bytes":TOTAL,"offset":if mode=="invalid-ack" {TOTAL}else{PIECE},"complete":mode=="invalid-ack"})).await;
            if mode != "normal" {
                assert!(
                    tokio::time::timeout(Duration::from_millis(150), listener.accept())
                        .await
                        .is_err(),
                    "must not send another piece"
                );
                return;
            }
            let (mut socket, _) = listener.accept().await.unwrap();
            assert_eq!(*observed.lock().unwrap(), vec![0, PIECE as u64]);
            let headers = request(&mut socket).await;
            assert!(headers.starts_with(&format!("PATCH /api/uploads/same?offset={PIECE} ")));
            assert!(headers
                .to_lowercase()
                .contains(&format!("content-length: {}", TOTAL - PIECE)));
            let mut bytes = vec![0; TOTAL - PIECE];
            socket.read_exact(&mut bytes).await.unwrap();
            assert!(bytes.iter().all(|b| *b == 42));
            answer(
                &mut socket,
                json!({"id":"same","bytes":TOTAL,"offset":TOTAL,"complete":true}),
            )
            .await;
        });
        let values = saved.clone();
        let result = hub
            .resume_upload_confirmed(
                &me,
                "same",
                tokio::fs::File::open(source).await.unwrap(),
                None,
                CancelFlag::default(),
                Some(Arc::new(move |st| {
                    if mode == "checkpoint-failed" && st.offset > 0 {
                        return Err(Error::Invalid("checkpoint disk failed".into()));
                    }
                    values.lock().unwrap().push(st.offset);
                    Ok(())
                })),
            )
            .await;
        if mode == "normal" {
            assert!(result.unwrap().complete);
            assert_eq!(*saved.lock().unwrap(), vec![0, PIECE as u64, TOTAL as u64]);
        } else {
            assert!(matches!(result, Err(Error::Invalid(_))));
            assert_eq!(*saved.lock().unwrap(), vec![0]);
        }
        server.await.unwrap();
    }
}
