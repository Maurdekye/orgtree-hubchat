//! Android push must close parked HTTP requests, including a request stuck
//! probing a hub, and resume the same engine on the next foreground/wake.
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use hubchat_core::engine::{Engine, Event, Host};
use hubchat_core::hub::Profile;
use hubchat_core::store::Store;
use hubchat_core::Identity;
use tokio::io::AsyncReadExt;

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

#[tokio::test]
async fn suspended_engine_closes_requests_and_resumes_without_recreation() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let engine = Engine::new(
        Arc::new(Store::open_in_memory().unwrap()),
        Identity::generate("push-test").unwrap(),
        Profile {
            kind: "person".into(),
            org_name: String::new(),
            username: "push-test".into(),
            blurb: String::new(),
        },
        Arc::new(QuietHost),
    );
    engine.set_suspended(true);
    engine.start().unwrap();
    engine
        .add_hub(&listener.local_addr().unwrap().to_string())
        .unwrap();
    assert!(
        tokio::time::timeout(Duration::from_millis(150), listener.accept())
            .await
            .is_err()
    );

    for _ in 0..2 {
        engine.set_suspended(false);
        let (mut socket, _) = tokio::time::timeout(Duration::from_secs(3), listener.accept())
            .await
            .unwrap()
            .unwrap();
        let mut data = [0; 4096];
        assert!(
            tokio::time::timeout(Duration::from_secs(3), socket.read(&mut data))
                .await
                .unwrap()
                .unwrap()
                > 0
        );
        engine.set_suspended(true);
        assert_eq!(
            tokio::time::timeout(Duration::from_secs(3), socket.read(&mut data))
                .await
                .unwrap()
                .unwrap(),
            0,
            "suspending must close the parked request"
        );
        engine.kick(); // periodic retries must not bypass suspension
        engine.retry_now();
        assert!(
            tokio::time::timeout(Duration::from_millis(150), listener.accept())
                .await
                .is_err()
        );
    }
    engine.shutdown();
    engine.set_suspended(false);
    assert!(
        tokio::time::timeout(Duration::from_millis(150), listener.accept())
            .await
            .is_err()
    );
}
