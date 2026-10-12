//! Byte-level download recovery. The store owns checkpoints and publication;
//! this layer writes only a private partial file and never exposes it early.
use crate::{
    hub::{check, CancelFlag, HubClient, Progress},
    Error, Identity, Result,
};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use std::{
    path::Path,
    sync::Arc,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tokio::io::{AsyncSeekExt, AsyncWriteExt};

const CHECKPOINT_BYTES: u64 = 8 * 1024 * 1024;
const IDLE_MS: u64 = 24 * 60 * 60 * 1000;

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct DownloadCheckpoint {
    pub confirmed: u64,
    pub validator: Option<String>,
    pub updated_ms: u64,
}
pub type SaveCheckpoint = Arc<dyn Fn(&DownloadCheckpoint) -> Result<()> + Send + Sync>;

pub struct DownloadSpec<'a> {
    pub attachment_id: &'a str,
    pub partial: &'a Path,
    pub total: u64,
    pub checkpoint: DownloadCheckpoint,
}
fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
fn header(resp: &reqwest::Response, name: &str) -> Option<String> {
    resp.headers().get(name)?.to_str().ok().map(str::to_owned)
}
fn validator(resp: &reqwest::Response) -> Option<String> {
    header(resp, "etag")
        .filter(|s| s.starts_with('"') && s.ends_with('"'))
        .or_else(|| header(resp, "last-modified"))
}
fn bad_range() -> Error {
    Error::Invalid("hub returned an inconsistent download range".into())
}
fn check_range(value: Option<String>, offset: u64, total: u64) -> Result<()> {
    let value = value.ok_or_else(bad_range)?;
    let (span, size) = value
        .strip_prefix("bytes ")
        .and_then(|v| v.split_once('/'))
        .ok_or_else(bad_range)?;
    let (first, last) = span.split_once('-').ok_or_else(bad_range)?;
    if first.parse::<u64>().ok() != Some(offset)
        || size.parse::<u64>().ok() != Some(total)
        || last.parse::<u64>().ok() != total.checked_sub(1)
    {
        return Err(bad_range());
    }
    Ok(())
}

impl HubClient {
    /// Resume only confirmed bytes, under a matching validator. The callback
    /// persists after flush+sync, and can fail the operation if its store fails.
    /// Cancellation preserves the last durable checkpoint for an explicit retry.
    pub async fn download_resumable(
        &self,
        me: &Identity,
        spec: DownloadSpec<'_>,
        save: SaveCheckpoint,
        progress: Option<Progress>,
        cancel: CancelFlag,
    ) -> Result<DownloadCheckpoint> {
        self.session_work(async {
        cancel
            .run(async {
                let mut state = spec.checkpoint;
                let length = tokio::fs::metadata(spec.partial)
                    .await
                    .map(|m| m.len())
                    .unwrap_or(0);
                if state.confirmed > spec.total
                    || length < state.confirmed
                    || (state.confirmed > 0 && state.validator.is_none())
                    || now_ms().saturating_sub(state.updated_ms) >= IDLE_MS
                {
                    state = DownloadCheckpoint::default();
                    save(&state)?;
                }
                // Revalidate the last byte plus EOF, rather than requesting total-
                // (which is a valid 416 forever after a crash at the final checkpoint).
                let offset = if state.confirmed == spec.total {
                    state.confirmed.saturating_sub(1)
                } else {
                    state.confirmed
                };
                let range = state.confirmed > 0 && spec.total > 0;
                let id: String =
                    url::form_urlencoded::byte_serialize(spec.attachment_id.as_bytes()).collect();
                let mut request = self
                    .http
                    .get(self.address().join(&format!("/api/attachments/{id}")))
                    .header("X-Org-Auth", me.auth_header());
                if range {
                    request = request
                        .header("Range", format!("bytes={offset}-"))
                        .header("If-Range", state.validator.as_deref().unwrap());
                }
                let resp = request.send().await?;
                if resp.status().as_u16() == 416 {
                    save(&DownloadCheckpoint::default())?;
                    return Err(Error::Unreachable(
                        "download range expired; retry from zero".into(),
                    ));
                }
                let resp = check(resp).await?;
                let next_validator = validator(&resp);
                let start = match resp.status().as_u16() {
                    206 if range => {
                        check_range(header(&resp, "content-range"), offset, spec.total)?;
                        if next_validator != state.validator {
                            return Err(bad_range());
                        }
                        offset
                    }
                    200 => 0,
                    _ => return Err(bad_range()),
                };
                if resp
                    .content_length()
                    .is_some_and(|n| n != spec.total - start)
                {
                    return Err(bad_range());
                }
                // Persist the rewind BEFORE discarding bytes, including the last-byte
                // EOF reconciliation. A crash can never leave a checkpoint past disk.
                state.confirmed = start;
                state.validator = next_validator;
                state.updated_ms = now_ms();
                save(&state)?;
                let mut out = tokio::fs::OpenOptions::new()
                    .create(true)
                    .truncate(false)
                    .write(true)
                    .open(spec.partial)
                    .await?;
                out.set_len(start).await?;
                out.seek(std::io::SeekFrom::Start(start)).await?;
                let mut done = start;
                let mut last_save = Instant::now();
                let mut body = resp.bytes_stream();
                while let Some(chunk) = body.next().await {
                    // A truncated HTTP body is transport loss, even when
                    // reqwest labels its body decoder error as is_decode().
                    let chunk = chunk.map_err(|e| Error::Unreachable(e.to_string()))?;
                    if chunk.len() as u64 > spec.total - done {
                        return Err(Error::Invalid("download exceeded its declared size".into()));
                    }
                    out.write_all(&chunk).await?;
                    done += chunk.len() as u64;
                    if let Some(p) = &progress {
                        p(done, spec.total);
                    }
                    if done - state.confirmed >= CHECKPOINT_BYTES
                        || last_save.elapsed() >= Duration::from_secs(1)
                    {
                        out.flush().await?;
                        out.sync_data().await?;
                        state.confirmed = done;
                        state.updated_ms = now_ms();
                        save(&state)?;
                        last_save = Instant::now();
                    }
                }
                if done != spec.total {
                    return Err(Error::Unreachable(
                        "download ended before all bytes arrived".into(),
                    ));
                }
                out.flush().await?;
                out.sync_data().await?;
                state.confirmed = done;
                state.updated_ms = now_ms();
                save(&state)?;
                Ok(state)
            })
            .await
        }).await
    }
}
