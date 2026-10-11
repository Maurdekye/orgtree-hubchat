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
    let headers = String::from_utf8(buf).unwrap();
    let length = headers
        .lines()
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.eq_ignore_ascii_case("content-length")
                .then(|| value.trim().parse::<usize>().unwrap())
        })
        .unwrap_or(0);
    let mut body = vec![0; length];
    s.read_exact(&mut body).await.unwrap();
    headers
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

struct StageHost {
    dir: PathBuf,
    gate: Option<(
        std::sync::mpsc::Sender<()>,
        Mutex<std::sync::mpsc::Receiver<()>>,
    )>,
}
impl Host for StageHost {
    fn event(&self, _: Event) {}
    fn open_source(&self, source: &str) -> std::io::Result<std::fs::File> {
        if let Some((entered, release)) = &self.gate {
            entered.send(()).unwrap();
            release.lock().unwrap().recv().unwrap();
        }
        std::fs::File::open(source)
    }
    fn download_dir(&self) -> PathBuf {
        self.dir.clone()
    }
    fn staging_dir(&self) -> PathBuf {
        self.dir.join("private-outbox")
    }
}
fn staged_engine(store: Arc<Store>, host: StageHost) -> Arc<Engine> {
    Engine::new(
        store,
        Identity::generate("snapshot-proof").unwrap(),
        Profile {
            kind: "person".into(),
            org_name: String::new(),
            username: "test".into(),
            blurb: String::new(),
        },
        Arc::new(host),
    )
}
fn outgoing(id: &str, sources: &[(&std::path::Path, u64)]) -> NewOutgoing {
    NewOutgoing {
        id: id.into(),
        peer: "peer.abcdef".into(),
        body: "all files or no send".into(),
        kind: None,
        reply_to: None,
        attachments: sources
            .iter()
            .map(|(p, n)| NewAttachment {
                name: "../../not-a-path.bin".into(),
                bytes: *n,
                source: p.to_string_lossy().into_owned(),
            })
            .collect(),
    }
}

#[tokio::test(flavor = "current_thread")]
async fn snapshot_finishes_before_admission_without_blocking_runtime_and_survives_restart() {
    use sha2::{Digest, Sha256};
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("store.db");
    let source = dir.path().join("selected.bin");
    let original = vec![47u8; 3 * 1024 * 1024];
    std::fs::write(&source, &original).unwrap();
    let store = Arc::new(Store::open(&db).unwrap());
    let (entered_tx, entered_rx) = std::sync::mpsc::channel();
    let (release_tx, release_rx) = std::sync::mpsc::channel();
    let engine = staged_engine(
        store.clone(),
        StageHost {
            dir: dir.path().into(),
            gate: Some((entered_tx, Mutex::new(release_rx))),
        },
    );
    let engine2 = engine.clone();
    let message = outgoing("snapshot", &[(&source, original.len() as u64)]);
    let send = tokio::spawn(async move { engine2.send(message).await });
    tokio::time::timeout(Duration::from_secs(3), async {
        while entered_rx.try_recv().is_err() {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
    assert!(
        store.message("snapshot").unwrap().is_none(),
        "partially staged message cannot be sent"
    );
    assert!(!send.is_finished());
    release_tx.send(()).unwrap();
    send.await.unwrap().unwrap();
    std::fs::write(&source, vec![99; original.len()]).unwrap();
    std::fs::remove_file(&source).unwrap();
    drop(engine);
    drop(store);
    let reopened = Store::open(&db).unwrap();
    let a = reopened
        .message("snapshot")
        .unwrap()
        .unwrap()
        .attachments
        .remove(0);
    assert_eq!(
        a.source_sha256,
        Some(hex::encode(Sha256::digest(&original)))
    );
    let copy = PathBuf::from(a.staged_path.unwrap());
    assert!(copy.starts_with(dir.path().join("private-outbox")));
    assert_eq!(std::fs::read(copy).unwrap(), original);
    assert_eq!(a.state, "pending");
    assert_eq!(a.source.as_deref(), source.to_str());
}

#[tokio::test]
async fn preparation_failure_and_duplicate_admission_leave_no_unowned_copies() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("file.bin");
    std::fs::write(&source, b"abc").unwrap();
    let store = Arc::new(Store::open(&dir.path().join("store.db")).unwrap());
    let engine = staged_engine(
        store.clone(),
        StageHost {
            dir: dir.path().into(),
            gate: None,
        },
    );
    let missing = dir.path().join("missing");
    assert!(engine
        .send(outgoing("broken", &[(&source, 3), (&missing, 3)]))
        .await
        .is_err());
    assert!(store.message("broken").unwrap().is_none());
    let root = dir.path().join("private-outbox");
    assert_eq!(std::fs::read_dir(&root).unwrap().count(), 0);
    for declared in [2, 4] {
        assert!(engine
            .send(outgoing("wrong-length", &[(&source, declared)]))
            .await
            .is_err());
        assert_eq!(std::fs::read_dir(&root).unwrap().count(), 0);
    }
    engine
        .send(outgoing("kept", &[(&source, 3)]))
        .await
        .unwrap();
    let copy = store.message("kept").unwrap().unwrap().attachments[0]
        .source
        .clone()
        .unwrap();
    assert!(engine
        .send(outgoing("kept", &[(&source, 3)]))
        .await
        .is_err());
    assert_eq!(std::fs::read_dir(&root).unwrap().count(), 1);
    assert_eq!(std::fs::read(copy).unwrap(), b"abc");
}

#[tokio::test]
async fn staged_copies_are_collected_after_success_and_deletion_but_failed_mail_keeps_them() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("photo.jpg");
    std::fs::write(&source, b"abc").unwrap();
    let db = dir.path().join("store.db");
    let store = Arc::new(Store::open(&db).unwrap());
    let engine = staged_engine(
        store.clone(),
        StageHost {
            dir: dir.path().into(),
            gate: None,
        },
    );
    let root = dir.path().join("private-outbox");
    for id in ["sent-copy", "failed-copy", "deleted-copy", "chat-copy"] {
        engine.send(outgoing(id, &[(&source, 3)])).await.unwrap();
        let a = store.message(id).unwrap().unwrap().attachments.remove(0);
        assert_eq!(a.source.as_deref(), source.to_str());
        assert!(a.staged_path.is_some());
    }
    store
        .set_state("failed-copy", "failed", Some("offline"))
        .unwrap();
    store.set_state("sent-copy", "sent", None).unwrap();
    engine.delete_message("deleted-copy").await.unwrap();
    engine.clean_staging().await;
    let sent = store
        .message("sent-copy")
        .unwrap()
        .unwrap()
        .attachments
        .remove(0);
    assert!(sent.staged_path.is_none());
    assert_eq!(sent.source.as_deref(), source.to_str());
    let failed = store
        .message("failed-copy")
        .unwrap()
        .unwrap()
        .attachments
        .remove(0);
    assert_eq!(std::fs::read(failed.staged_path.unwrap()).unwrap(), b"abc");
    assert_eq!(std::fs::read_dir(&root).unwrap().count(), 2);
    engine.delete_chat("peer.abcdef").await.unwrap();
    assert_eq!(std::fs::read_dir(&root).unwrap().count(), 0);
    assert_eq!(
        std::fs::read(&source).unwrap(),
        b"abc",
        "never delete the user's original"
    );
    // A process can stop after a successful queue-state transaction, before unlink.
    engine
        .send(outgoing("restart-clean", &[(&source, 3)]))
        .await
        .unwrap();
    store.set_state("restart-clean", "sent", None).unwrap();
    drop(engine);
    drop(store);
    let store = Store::open(&db).unwrap();
    crate::staging::collect(&root, &store).unwrap();
    assert_eq!(std::fs::read_dir(&root).unwrap().count(), 0);
    // Old orphan copies disappear, fresh preparation and referenced failures stay.
    let old = root.join("11111111111111111111111111111111");
    let fresh = root.join("22222222222222222222222222222222");
    for p in [&old, &fresh] {
        std::fs::create_dir(p).unwrap();
        std::fs::write(p.join("0.bin"), b"abc").unwrap();
    }
    let oldtime = std::time::SystemTime::now() - Duration::from_secs(25 * 60 * 60);
    let mut opts = std::fs::OpenOptions::new();
    opts.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        opts.custom_flags(0x02000000).access_mode(0x100);
    }
    opts.open(&old)
        .unwrap()
        .set_times(std::fs::FileTimes::new().set_modified(oldtime))
        .unwrap();
    crate::staging::sweep_orphans(&root, &store).unwrap();
    assert!(!old.exists());
    assert!(fresh.exists());
}

#[tokio::test]
async fn failed_staging_unlink_stays_durable_for_retry() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("source");
    std::fs::write(&source, b"abc").unwrap();
    let store = Arc::new(Store::open(&dir.path().join("db")).unwrap());
    let engine = staged_engine(
        store.clone(),
        StageHost {
            dir: dir.path().into(),
            gate: None,
        },
    );
    engine
        .send(outgoing("cleanup-error", &[(&source, 3)]))
        .await
        .unwrap();
    let path = store.message("cleanup-error").unwrap().unwrap().attachments[0]
        .staged_path
        .clone()
        .unwrap();
    let held = PathBuf::from(&path).with_extension("held");
    std::fs::rename(&path, &held).unwrap();
    std::fs::create_dir(&path).unwrap();
    store.set_state("cleanup-error", "sent", None).unwrap();
    engine.clean_staging().await;
    assert!(
        store.message("cleanup-error").unwrap().unwrap().attachments[0]
            .staged_path
            .is_some()
    );
    assert_eq!(store.staging_due(i64::MAX).unwrap(), vec![path.clone()]);
    std::fs::remove_dir(&path).unwrap();
    std::fs::rename(held, &path).unwrap();
    store.staging_retry(&path, 0).unwrap();
    engine.clean_staging().await;
    assert!(!PathBuf::from(&path).exists());
    assert!(store.staging_due(i64::MAX).unwrap().is_empty());
}

#[tokio::test]
async fn preparation_reports_local_bytes_before_admission_and_names_full_disk() {
    struct ProgressHost {
        dir: PathBuf,
        store: Arc<Store>,
        ticks: Mutex<Vec<(u64, u64)>>,
        full: bool,
    }
    impl Host for ProgressHost {
        fn event(&self, ev: Event) {
            if let Event::Preparing {
                message_id,
                done,
                total,
                ..
            } = ev
            {
                assert!(
                    self.store.message(&message_id).unwrap().is_none(),
                    "preparation is not queue admission"
                );
                self.ticks.lock().unwrap().push((done, total));
            }
        }
        fn open_source(&self, source: &str) -> std::io::Result<std::fs::File> {
            if self.full {
                return Err(std::io::Error::from_raw_os_error(if cfg!(windows) {
                    112
                } else {
                    28
                }));
            }
            std::fs::File::open(source)
        }
        fn download_dir(&self) -> PathBuf {
            self.dir.clone()
        }
    }
    let tmp = tempfile::tempdir().unwrap();
    let store = Arc::new(Store::open(&tmp.path().join("store.db")).unwrap());
    let host = Arc::new(ProgressHost {
        dir: tmp.path().join("out"),
        store: store.clone(),
        ticks: Mutex::new(Vec::new()),
        full: false,
    });
    let e = Engine::new(
        store.clone(),
        Identity::generate("preparing").unwrap(),
        Profile::default(),
        host.clone(),
    );
    let mut atts = Vec::new();
    for (i, n) in [3 * 1024 * 1024usize, 1024 * 1024].into_iter().enumerate() {
        let file = tmp.path().join(format!("source-{i}"));
        std::fs::write(&file, vec![7; n]).unwrap();
        atts.push(NewAttachment {
            name: format!("source-{i}"),
            bytes: n as u64,
            source: file.to_string_lossy().into_owned(),
        });
    }
    let message = NewOutgoing {
        id: "preparing-proof".into(),
        peer: "someone.123456".into(),
        body: "files".into(),
        kind: None,
        reply_to: None,
        attachments: atts,
    };
    e.send(message.clone()).await.unwrap();
    let ticks = host.ticks.lock().unwrap().clone();
    assert_eq!(ticks.first(), Some(&(0, 4 * 1024 * 1024)));
    assert_eq!(ticks.last(), Some(&(4 * 1024 * 1024, 4 * 1024 * 1024)));
    assert!(ticks.windows(2).all(|w| w[0].0 <= w[1].0));
    let failed = Engine::new(
        store.clone(),
        Identity::generate("preparing-fail").unwrap(),
        Profile::default(),
        Arc::new(ProgressHost {
            dir: tmp.path().join("full"),
            store: store.clone(),
            ticks: Mutex::new(Vec::new()),
            full: true,
        }),
    );
    let mut message = message;
    message.id = "full-proof".into();
    let error = failed.send(message).await.unwrap_err();
    assert!(error.to_string().contains("not enough space"));
    assert!(store.message("full-proof").unwrap().is_none());
}

#[tokio::test]
async fn queued_upload_cancel_survives_reopen_and_removal_is_explicit() {
    let dir=tempfile::tempdir().unwrap();let db=dir.path().join("store.db");
    let store=Arc::new(Store::open(&db).unwrap());let e=engine(store.clone());
    for (id,body) in [("with-body","keep me"),("file-only","")] {
        store.queue_outgoing(&NewOutgoing{id:id.into(),peer:"peer".into(),body:body.into(),kind:None,reply_to:None,attachments:vec![NewAttachment{name:"file".into(),bytes:3,source:"original-must-stay".into()}]},"now").unwrap();
    }
    let id=store.message("with-body").unwrap().unwrap().attachments[0].local_id.clone();
    e.cancel_transfer(&id).unwrap();e.cancel_transfer(&id).unwrap();
    assert!(store.complete_upload(&id,"late-reply","hub").is_err());
    assert!(store.set_upload(&id,"uploading","late-session","hub").is_err());
    store.requeue_send("with-body").unwrap();
    store.defer_send("with-body","hub",0).unwrap();
    e.fail("peer","with-body","late worker error").unwrap();
    assert_eq!(store.message("with-body").unwrap().unwrap().error.as_deref(),Some("Upload cancelled; retry or remove the file"));
    assert_eq!(store.message("with-body").unwrap().unwrap().state,"failed");
    assert!(store.queued().unwrap().iter().all(|m|m.id!="with-body"));
    drop(e);drop(store);
    let store=Arc::new(Store::open(&db).unwrap());let e=engine(store.clone());
    assert_eq!(store.message("with-body").unwrap().unwrap().attachments[0].state,"cancelled");
    e.retry("with-body").unwrap();
    assert_eq!(store.message("with-body").unwrap().unwrap().state,"queued");
    e.cancel_transfer(&id).unwrap();e.remove_attachment("with-body",&id).await.unwrap();
    let m=store.message("with-body").unwrap().unwrap();assert!(m.attachments.is_empty());assert_eq!(m.body,"keep me");assert_eq!(m.state,"queued");
    let id=store.message("file-only").unwrap().unwrap().attachments[0].local_id.clone();
    e.cancel_transfer(&id).unwrap();
    assert!(e.remove_attachment("file-only",&id).await.is_err());
    assert_eq!(store.message("file-only").unwrap().unwrap().attachments.len(),1,"empty-message rejection rolls deletion back");
}

#[tokio::test]
async fn cancelling_active_upload_blocks_early_retry_and_never_dispatches_mail() {
    let listener=TcpListener::bind("127.0.0.1:0").await.unwrap();
    let client=HubClient::new(HubAddress::parse(&listener.local_addr().unwrap().to_string()).unwrap());
    let url=client.address().to_string();
    let dir=tempfile::tempdir().unwrap();let file=dir.path().join("source");std::fs::write(&file,b"abc").unwrap();
    let store=Arc::new(Store::open_in_memory().unwrap());let e=engine(store.clone());
    store.queue_outgoing(&NewOutgoing{id:"m".into(),peer:"peer".into(),body:"body".into(),kind:None,reply_to:None,attachments:vec![NewAttachment{name:"file".into(),bytes:3,source:file.to_string_lossy().into()}]},"now").unwrap();
    let id=store.message("m").unwrap().unwrap().attachments[0].local_id.clone();
    let (ready,wait)=tokio::sync::oneshot::channel();
    let server=tokio::spawn(async move {
        let (mut socket,_)=listener.accept().await.unwrap();assert!(request(&mut socket).await.starts_with("POST /api/attachments"));
        ready.send(()).unwrap();
        let mut rest=Vec::new();socket.read_to_end(&mut rest).await.unwrap();
        assert!(tokio::time::timeout(Duration::from_millis(100),listener.accept()).await.is_err(),"cancel must not dispatch mail");
    });
    let controls=async {
        wait.await.unwrap();
        assert!(e.send_one_on("m",(url.clone(),client.clone(),1024)).await.unwrap(),"busy launch must not request a deferral");
        assert_eq!(store.message("m").unwrap().unwrap().state,"sending");
        e.cancel_transfer(&id).unwrap();
        assert!(e.retry("m").is_err());assert!(e.remove_attachment("m",&id).await.is_err());
    };
    let (result,())=tokio::time::timeout(Duration::from_secs(3),async{tokio::join!(e.send_one_on("m",(url.clone(),client.clone(),1024)),controls)}).await.unwrap();
    assert!(result.is_ok());assert_eq!(store.message("m").unwrap().unwrap().state,"failed");
    assert_eq!(store.message("m").unwrap().unwrap().attachments[0].state,"cancelled");
    assert!(e.send_writers.lock().unwrap().is_empty());e.retry("m").unwrap();
    server.await.unwrap();
}

#[tokio::test]
async fn shutdown_of_upload_is_resumable_and_has_no_failure_notice() {
    let listener=TcpListener::bind("127.0.0.1:0").await.unwrap();
    let client=HubClient::new(HubAddress::parse(&listener.local_addr().unwrap().to_string()).unwrap());let url=client.address().to_string();
    let dir=tempfile::tempdir().unwrap();let file=dir.path().join("source");std::fs::write(&file,b"abc").unwrap();
    let store=Arc::new(Store::open_in_memory().unwrap());let e=engine(store.clone());
    store.queue_outgoing(&NewOutgoing{id:"m".into(),peer:"peer".into(),body:"body".into(),kind:None,reply_to:None,attachments:vec![NewAttachment{name:"file".into(),bytes:3,source:file.to_string_lossy().into()}]},"now").unwrap();
    let (ready,wait)=tokio::sync::oneshot::channel();
    let server=tokio::spawn(async move {let(mut socket,_)=listener.accept().await.unwrap();request(&mut socket).await;ready.send(()).unwrap();let mut rest=Vec::new();socket.read_to_end(&mut rest).await.unwrap();});
    let pause=async {wait.await.unwrap();e.shutdown();};
    let (result,())=tokio::time::timeout(Duration::from_secs(3),async{tokio::join!(e.send_one_on("m",(url,client,1024)),pause)}).await.unwrap();
    assert!(!result.unwrap());let m=store.message("m").unwrap().unwrap();assert_eq!(m.state,"queued");assert_eq!(m.attachments[0].state,"pending");
    assert!(store.pending_transfer_failures(u64::MAX/2).unwrap().is_empty());server.await.unwrap();
}
