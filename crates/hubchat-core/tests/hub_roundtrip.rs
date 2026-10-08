//! End-to-end against a real mail hub (today's Python hub from the orgtree
//! repo) on a scratch port with a scratch database.
//!
//! Set MAILHUB_DIR to the folder holding the `mailhub` package (default:
//! <orgtree>\engine\mailhub). Skipped when it is missing.

use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use hubchat_core::hub::{CancelFlag, Outgoing, Profile};
use hubchat_core::{HubAddress, HubClient, Identity};

struct Hub {
    child: Child,
    port: u16,
    _data: tempfile::TempDir,
}

impl Drop for Hub {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn mailhub_dir() -> Option<PathBuf> {
    let d = PathBuf::from(
        std::env::var("MAILHUB_DIR").unwrap_or_else(|_| r"<orgtree>\engine\mailhub".into()),
    );
    d.join("mailhub").join("app.py").is_file().then_some(d)
}

async fn start_hub() -> Option<Hub> {
    let dir = mailhub_dir()?;
    let port = {
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        l.local_addr().unwrap().port()
    };
    let data = tempfile::tempdir().unwrap();
    let child = Command::new("python")
        .args(["-m", "mailhub.serve"])
        .current_dir(&dir)
        .env("PYTHONPATH", &dir)
        .env("HUB_DATA", data.path())
        .env("HUB_PORT", port.to_string())
        .env("HUB_BIND", "127.0.0.1")
        .env("HUB_NAME", "testhub")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let hub = Hub { child, port, _data: data };
    let client = HubClient::new(HubAddress::parse(&format!("127.0.0.1:{port}")).unwrap());
    let t0 = std::time::Instant::now();
    while t0.elapsed() < Duration::from_secs(30) {
        if client.healthz().await.is_ok() {
            return Some(hub);
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    panic!("test hub did not come up on port {port}");
}

fn profile(name: &str) -> Profile {
    Profile { kind: "chat".into(), org_name: "Hubchat".into(), username: name.into(), blurb: String::new() }
}

#[tokio::test]
async fn register_send_poll_ack_receipts_and_attachments() {
    let Some(hub) = start_hub().await else {
        eprintln!("SKIPPED: no mailhub source (set MAILHUB_DIR)");
        return;
    };
    let c = HubClient::new(HubAddress::parse(&format!("127.0.0.1:{}", hub.port)).unwrap());

    let h = c.healthz().await.unwrap();
    assert_eq!(h.name, "testhub");
    assert_eq!(h.max_attachment_bytes(), 25 * 1024 * 1024, "today's hub advertises no limit");

    let alex = Identity::generate("alex").unwrap();
    let maya = Identity::generate("maya").unwrap();
    let reg = c.register(&alex, &profile("alex")).await.unwrap();
    assert!(reg.roster.iter().any(|r| r.slug == alex.address()));
    c.register(&maya, &profile("maya")).await.unwrap();

    // Same id, different secret -> different tag, so no clash. Same slug with
    // a wrong secret -> 403.
    let thief = Identity::from_parts("alex", &"f".repeat(64)).unwrap();
    assert_ne!(thief.address(), alex.address());

    // Unknown recipient -> 422 (shown as a failed bubble, never retried forever).
    let err = c.send(&alex, &Outgoing::new("nobody.000000", "hi")).await.unwrap_err();
    assert_eq!(err.status(), Some(422));

    // Attachment: stream 3 MB with progress.
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("blob.bin");
    let payload: Vec<u8> = (0..3_000_000u32).map(|i| (i % 251) as u8).collect();
    std::fs::write(&src, &payload).unwrap();
    let seen = Arc::new(AtomicU64::new(0));
    let s2 = seen.clone();
    let att = c
        .upload_file(&alex, &src, "blob.bin", Some(Arc::new(move |d, _t| s2.store(d, Ordering::SeqCst))), CancelFlag::default())
        .await
        .unwrap();
    assert_eq!(att.bytes, payload.len() as u64);
    assert_eq!(seen.load(Ordering::SeqCst), payload.len() as u64);

    // Send with the attachment; a retry with the same id is a duplicate.
    let mut out = Outgoing::new(&maya.address(), "hello maya");
    out.attachments.push(att.id.clone());
    let sent = c.send(&alex, &out).await.unwrap();
    assert!(!sent.duplicate);
    assert!(c.send(&alex, &out).await.unwrap().duplicate);

    // Maya polls, sees it, downloads, acks.
    let p = c.poll(&maya, 5).await.unwrap();
    assert_eq!(p.messages.len(), 1);
    let m = &p.messages[0];
    assert_eq!((m.from.as_str(), m.body.as_str()), (alex.address().as_str(), "hello maya"));
    assert_eq!(m.attachments[0].bytes, payload.len() as u64);
    let dest = dir.path().join("got.bin");
    let n = c.download_file(&maya, &m.attachments[0].id, &dest, None, CancelFlag::default()).await.unwrap();
    assert_eq!(n, payload.len() as u64);
    assert_eq!(std::fs::read(&dest).unwrap(), payload);
    assert_eq!(c.ack(&maya, &[m.id.clone()]).await.unwrap(), 1);
    c.receipts(&maya, &[(m.id.clone(), "delivered", "2026-10-08T00:00:00Z".into())]).await.unwrap();
    c.receipts(&maya, &[(m.id.clone(), "read", "2026-10-08T00:00:01Z".into())]).await.unwrap();

    // Alex's poll carries the receipt ladder (pushed once).
    let p = c.poll(&alex, 5).await.unwrap();
    let r = p.receipts.iter().find(|r| r.id == m.id).expect("receipt");
    assert_eq!(r.state, "read");
    assert!(r.fetched_at.is_some() && r.delivered_at.is_some());
    let maya_row = p.roster.iter().find(|r| r.slug == maya.address()).unwrap();
    assert!(maya_row.online, "maya made an authed call within 90 s");

    // A third party cannot download it.
    let eve = Identity::generate("eve").unwrap();
    c.register(&eve, &profile("eve")).await.unwrap();
    let err = c.download_file(&eve, &att.id, &dir.path().join("x"), None, CancelFlag::default()).await.unwrap_err();
    assert_eq!(err.status(), Some(403));

    // Long poll returns early when a message arrives.
    let c2 = c.clone();
    let maya2 = maya.clone();
    let waiter = tokio::spawn(async move { c2.poll(&maya2, 30).await });
    tokio::time::sleep(Duration::from_millis(500)).await;
    let t0 = std::time::Instant::now();
    c.send(&alex, &Outgoing::new(&maya.address(), "ping")).await.unwrap();
    let p = waiter.await.unwrap().unwrap();
    assert_eq!(p.messages[0].body, "ping");
    assert!(t0.elapsed() < Duration::from_secs(5), "long poll woke promptly");

    c.unregister(&eve).await.unwrap();
}

#[tokio::test]
async fn unreachable_hub_is_reported_as_such() {
    let c = HubClient::new(HubAddress::parse("127.0.0.1:1").unwrap());
    let err = c.healthz().await.unwrap_err();
    assert!(matches!(err, hubchat_core::Error::Unreachable(_)), "{err:?}");
}
