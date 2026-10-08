//! SPIKE commands for the transfer test page. A picked file (on Android a
//! content:// URI resolved to a file descriptor by the fs plugin) is streamed
//! to the hub, sent to our own address, downloaded back to the cache dir and
//! compared. Progress goes to the page as `xfer` events. Phase 2 replaces this.

use std::str::FromStr;
use std::sync::Arc;
use std::time::Instant;

use hubchat_core::hub::{CancelFlag, Outgoing};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, Runtime};
use tauri_plugin_fs::{FilePath, FsExt, OpenOptions};

use crate::connection::{core, log};

#[derive(Serialize)]
pub struct Info {
    address: String,
    hub: String,
    max_attachment_bytes: Option<u64>,
    error: Option<String>,
}

#[derive(Serialize, Clone)]
struct Xfer {
    phase: &'static str,
    done: u64,
    total: u64,
}

#[derive(Serialize)]
pub struct RoundTrip {
    name: String,
    bytes: u64,
    upload_ms: u128,
    download_ms: u128,
    upload_mb_s: f64,
    download_mb_s: f64,
    identical: bool,
    peak_rss_kb: Option<u64>,
    saved_to: String,
}

#[tauri::command]
pub async fn spike_info() -> Info {
    let Some(c) = core() else {
        return Info {
            address: String::new(),
            hub: String::new(),
            max_attachment_bytes: None,
            error: Some("core not started".into()),
        };
    };
    let (max, error) = match c.client.healthz().await {
        Ok(h) => (Some(h.max_attachment_bytes()), None),
        Err(e) => (None, Some(e.to_string())),
    };
    Info {
        address: c.me.address(),
        hub: c.client.address().to_string(),
        max_attachment_bytes: max,
        error,
    }
}

/// Peak resident memory of this process (Linux/Android only).
fn peak_rss_kb() -> Option<u64> {
    let s = std::fs::read_to_string("/proc/self/status").ok()?;
    let line = s.lines().find(|l| l.starts_with("VmHWM:"))?;
    line.split_whitespace().nth(1)?.parse().ok()
}

fn mb_s(bytes: u64, ms: u128) -> f64 {
    if ms == 0 {
        return 0.0;
    }
    (bytes as f64 / 1_048_576.0) / (ms as f64 / 1000.0)
}

fn progress<R: Runtime>(app: &AppHandle<R>, phase: &'static str) -> hubchat_core::hub::Progress {
    let app = app.clone();
    let last = std::sync::Mutex::new(Instant::now());
    Arc::new(move |done, total| {
        let mut l = last.lock().unwrap();
        if done == total || l.elapsed().as_millis() >= 200 {
            *l = Instant::now();
            let _ = app.emit("xfer", Xfer { phase, done, total });
        }
    })
}

#[tauri::command]
pub async fn spike_roundtrip<R: Runtime>(
    app: AppHandle<R>,
    path: String,
    name: String,
) -> Result<RoundTrip, String> {
    let c = core().ok_or("core not started")?;
    let fp = FilePath::from_str(&path).map_err(|e| e.to_string())?;
    let file = app
        .fs()
        .open(fp, OpenOptions::new().read(true).clone())
        .map_err(|e| format!("open {path}: {e}"))?;
    let file = tokio::fs::File::from_std(file);
    log(&c.dir, &format!("roundtrip start {name} rss_peak={:?}", peak_rss_kb()));

    let t0 = Instant::now();
    let att = c
        .client
        .upload(&c.me, file, &name, Some(progress(&app, "upload")), CancelFlag::default())
        .await
        .map_err(|e| e.to_string())?;
    let upload_ms = t0.elapsed().as_millis();

    let mut out = Outgoing::new(&c.me.address(), &format!("spike round trip: {name}"));
    out.attachments.push(att.id.clone());
    c.client.send(&c.me, &out).await.map_err(|e| e.to_string())?;

    let cache = app.path().app_cache_dir().map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&cache).map_err(|e| e.to_string())?;
    let dest = cache.join(format!("rt-{}", att.id));
    let t1 = Instant::now();
    let got = c
        .client
        .download_file(&c.me, &att.id, &dest, Some(progress(&app, "download")), CancelFlag::default())
        .await
        .map_err(|e| e.to_string())?;
    let download_ms = t1.elapsed().as_millis();

    // Compare by re-reading both (streamed, small buffers).
    let identical = got == att.bytes && same_bytes(&app, &path, &dest).unwrap_or(false);
    let r = RoundTrip {
        name,
        bytes: att.bytes,
        upload_ms,
        download_ms,
        upload_mb_s: mb_s(att.bytes, upload_ms),
        download_mb_s: mb_s(got, download_ms),
        identical,
        peak_rss_kb: peak_rss_kb(),
        saved_to: dest.to_string_lossy().into_owned(),
    };
    log(
        &c.dir,
        &format!(
            "roundtrip {} bytes up {} ms ({:.1} MB/s) down {} ms ({:.1} MB/s) identical={} rss_peak={:?}",
            r.bytes, r.upload_ms, r.upload_mb_s, r.download_ms, r.download_mb_s, r.identical, r.peak_rss_kb
        ),
    );
    Ok(r)
}

fn same_bytes<R: Runtime>(app: &AppHandle<R>, src: &str, dest: &std::path::Path) -> std::io::Result<bool> {
    use std::io::Read;
    let mut a = app
        .fs()
        .open(FilePath::from_str(src).map_err(std::io::Error::other)?, OpenOptions::new().read(true).clone())?;
    let mut b = std::fs::File::open(dest)?;
    let (mut x, mut y) = (vec![0u8; 1 << 20], vec![0u8; 1 << 20]);
    loop {
        let n = read_full(&mut a, &mut x)?;
        let m = read_full(&mut b, &mut y)?;
        if n != m || x[..n] != y[..m] {
            return Ok(false);
        }
        if n == 0 {
            return Ok(true);
        }
    }
    fn read_full(r: &mut impl Read, buf: &mut [u8]) -> std::io::Result<usize> {
        let mut n = 0;
        while n < buf.len() {
            match r.read(&mut buf[n..])? {
                0 => break,
                k => n += k,
            }
        }
        Ok(n)
    }
}

/// Copy a downloaded file to a user-chosen destination (on Android a
/// content:// URI from the save dialog).
#[tauri::command]
pub async fn spike_save<R: Runtime>(app: AppHandle<R>, src: String, dest: String) -> Result<u64, String> {
    let fp = FilePath::from_str(&dest).map_err(|e| e.to_string())?;
    let mut out = app
        .fs()
        .open(fp, OpenOptions::new().write(true).create(true).truncate(true).clone())
        .map_err(|e| format!("open {dest}: {e}"))?;
    let mut inp = std::fs::File::open(&src).map_err(|e| e.to_string())?;
    let n = std::io::copy(&mut inp, &mut out).map_err(|e| e.to_string())?;
    std::fs::remove_file(&src).ok();
    Ok(n)
}
