//! Shared test helper: run today's Python mail hub on a scratch port.
#![allow(dead_code)]

use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::Duration;

use hubchat_core::{HubAddress, HubClient};

pub struct Hub {
    child: Child,
    pub port: u16,
    _data: tempfile::TempDir,
}

impl Drop for Hub {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn mailhub_dir() -> Option<PathBuf> {
    let d = PathBuf::from(std::env::var("MAILHUB_DIR").ok()?);
    d.join("mailhub").join("app.py").is_file().then_some(d)
}

pub async fn start_hub() -> Option<Hub> {
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
    let hub = Hub {
        child,
        port,
        _data: data,
    };
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
