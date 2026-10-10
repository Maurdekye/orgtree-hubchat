//! Real hub/client wake-fetch proof without a phone. The scratch hub must
//! be built with push-test (never use a live hub); set HUBCHAT_V2_HUB.
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use hubchat_core::engine::{Engine, Event, Host};
use hubchat_core::hub::{Outgoing, Profile};
use hubchat_core::store::Store;
use hubchat_core::{HubAddress, HubClient, Identity};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

struct QuietHost;
impl Host for QuietHost {
    fn event(&self, _: Event) {}
    fn open_source(&self, source: &str) -> std::io::Result<std::fs::File> {
        std::fs::File::open(source)
    }
    fn download_dir(&self) -> PathBuf {
        std::env::temp_dir()
    }
}

fn profile(name: &str) -> Profile {
    Profile {
        kind: "person".into(),
        org_name: name.into(),
        username: name.into(),
        blurb: String::new(),
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn distributor_wakes_a_suspended_client_which_fetches_normally() {
    let Ok(url) = std::env::var("HUBCHAT_V2_HUB") else {
        eprintln!("SKIPPED: set HUBCHAT_V2_HUB to an isolated push-test hub");
        return;
    };
    let phone = Identity::generate("push-phone").unwrap();
    let sender = Identity::generate("push-sender").unwrap();
    let hub = HubClient::new(HubAddress::parse(&url).unwrap());
    assert!(hub.healthz().await.unwrap().supports("unifiedpush"));
    hub.register(&sender, &profile("sender")).await.unwrap();
    let engine = Engine::new(
        Arc::new(Store::open_in_memory().unwrap()),
        phone.clone(),
        profile("phone"),
        Arc::new(QuietHost),
    );
    engine.set_device("push-phone-test", "Push phone test");
    engine.start().unwrap();
    engine.add_hub(&url).unwrap();
    assert!(engine.check_now(Duration::from_secs(30)).await);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!(
        "http://{}/synthetic-capability",
        listener.local_addr().unwrap()
    );
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let distributor = tokio::spawn(async move {
        loop {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut bytes = Vec::new();
            let mut block = [0; 1024];
            let header_end = loop {
                let n = socket.read(&mut block).await.unwrap();
                assert!(n > 0 && bytes.len() < 8192);
                bytes.extend_from_slice(&block[..n]);
                if let Some(end) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
                    break end + 4;
                }
            };
            let headers = String::from_utf8_lossy(&bytes[..header_end]).to_ascii_lowercase();
            let len: usize = headers
                .lines()
                .find_map(|line| line.strip_prefix("content-length:").map(str::trim))
                .unwrap()
                .parse()
                .unwrap();
            while bytes.len() < header_end + len {
                let n = socket.read(&mut block).await.unwrap();
                assert!(n > 0 && len < 8192);
                bytes.extend_from_slice(&block[..n]);
            }
            assert!(headers.contains("content-encoding: aes128gcm"));
            assert!(!headers.contains("x-org-auth"));
            socket
                .write_all(
                    b"HTTP/1.1 201 Created\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                )
                .await
                .unwrap();
            let _ = tx.send(());
        }
    });
    // Public test key from the WebPushBuilder documentation, synthetic auth.
    engine.register_push(&endpoint,
        "BLn9b-VR0ca83knDNZ32dCHGyjJp-1riX9ZTN40MqV8K_LpQmLqxC_DoHvqvFXO_nGdAB4W9dogZb_sM-uV4JbY",
        "CQkJCQkJCQkJCQkJCQkJCQ").await.unwrap();
    tokio::time::timeout(Duration::from_secs(15), rx.recv())
        .await
        .unwrap()
        .unwrap();
    engine.set_suspended(true);
    tokio::time::sleep(Duration::from_millis(250)).await;
    let sent = hub
        .send(
            &sender,
            &Outgoing::new(
                &phone.address(),
                "only fetched over normal authenticated sync",
            ),
        )
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(15), rx.recv())
        .await
        .unwrap()
        .unwrap();
    assert!(
        engine.store().message(&sent.id).unwrap().is_none(),
        "push alone must not deliver message content"
    );
    engine.set_suspended(false);
    assert!(engine.check_now(Duration::from_secs(15)).await);
    assert!(engine.store().message(&sent.id).unwrap().is_some());
    engine.set_suspended(true);
    engine.unregister_push().await.unwrap();
    hub.send(
        &sender,
        &Outgoing::new(&phone.address(), "no wake after unregister"),
    )
    .await
    .unwrap();
    assert!(tokio::time::timeout(Duration::from_secs(2), rx.recv())
        .await
        .is_err());
    engine.shutdown();
    hub.unregister(&phone).await.unwrap();
    hub.unregister(&sender).await.unwrap();
    distributor.abort();
    let _ = distributor.await;
}
