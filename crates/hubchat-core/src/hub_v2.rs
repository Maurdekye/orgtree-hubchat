//! Mail hub v2.0 additions (hub repo docs/v2-additions.md): sync, history
//! deletion, whole long bodies, the device list and resumable uploads. Each
//! is used only when the hub lists the feature in /healthz `features`.

use std::time::Duration;

use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncReadExt, AsyncSeekExt};

use crate::hub::{check, AttachmentMeta, CancelFlag, Envelope, HubClient, Progress, RosterEntry};
use crate::{Error, Identity, Result};

/// A message as sync reports it: poll's envelope plus the receipts as they
/// stand now, and `body_bytes` when the body was cut for the answer.
#[derive(Debug, Clone, Deserialize)]
pub struct SyncedMessage {
    #[serde(flatten)]
    pub env: Envelope,
    #[serde(default)]
    pub fetched_at: Option<String>,
    #[serde(default)]
    pub delivered_at: Option<String>,
    #[serde(default)]
    pub read_at: Option<String>,
    #[serde(default)]
    pub body_bytes: Option<u64>,
}

#[derive(Debug, Clone)]
pub enum Change {
    Message(Box<SyncedMessage>),
    Deleted(String),
    /// A change type this client doesn't know (ignored, per the contract).
    Other,
}

impl<'de> Deserialize<'de> for Change {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        let v = serde_json::Value::deserialize(d)?;
        Ok(match v.get("type").and_then(|t| t.as_str()) {
            Some("message") => match v.get("message").cloned().map(serde_json::from_value) {
                Some(Ok(m)) => Change::Message(Box::new(m)),
                _ => Change::Other,
            },
            Some("deleted") => match v.get("id").and_then(|i| i.as_str()) {
                Some(id) => Change::Deleted(id.to_owned()),
                None => Change::Other,
            },
            _ => Change::Other,
        })
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct SyncResult {
    pub name: String,
    #[serde(default)]
    pub version: Option<String>,
    pub cursor: String,
    #[serde(default)]
    pub changes: Vec<Change>,
    #[serde(default)]
    pub roster: Vec<RosterEntry>,
    #[serde(default)]
    pub roster_removed: Vec<String>,
    /// Every address online now; only present when that set changed.
    #[serde(default)]
    pub online: Option<Vec<String>>,
    #[serde(default)]
    pub more: bool,
    #[serde(default)]
    pub reset: bool,
    /// The address's OTHER devices in use as of this answer (hubs with the
    /// "active" feature): a message it brings needs no notification here.
    #[serde(default)]
    pub active: Vec<String>,
    /// The hub's clock in unix ms when it answered (v2.0.1).
    #[serde(default)]
    pub now: Option<i64>,
    /// "now" when the hub honoured a first sync's `start: "now"` (lazy
    /// history); absent when it synced from the beginning.
    #[serde(default)]
    pub start: Option<String>,
}

/// Where a history page ends: before a time on the hub's clock (unix ms,
/// strictly before it) or before an exact cursor from an earlier page.
#[derive(Debug, Clone, PartialEq)]
pub enum Before {
    Time(i64),
    Cursor(String),
}

/// One chat in the hub's chat list (`/api/conversations`).
#[derive(Debug, Clone, Deserialize)]
pub struct Conversation {
    pub with: String,
    #[serde(default)]
    pub unread: i64,
    #[serde(default)]
    pub last: Option<SyncedMessage>,
}

/// One page of a conversation, newest first.
#[derive(Debug, Clone, Deserialize)]
pub struct HistoryPage {
    #[serde(default)]
    pub messages: Vec<SyncedMessage>,
    /// The exact cursor for the next older page; None at the start.
    #[serde(default)]
    pub before: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DeviceEntry {
    pub device_id: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub created_at: Option<String>,
    #[serde(default)]
    pub last_seen: Option<String>,
    #[serde(default)]
    pub online: bool,
    #[serde(default)]
    pub public_key: Option<String>,
    #[serde(default)]
    pub signed_out_at: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UploadState {
    pub id: String,
    pub bytes: u64,
    pub offset: u64,
    #[serde(default)]
    pub complete: bool,
}

impl HubClient {
    /// Every change for our address since `cursor` (None: from the start,
    /// or with `start_now` from now on, on hubs with lazy_history).
    pub async fn sync(
        &self,
        me: &Identity,
        device_id: &str,
        device_name: &str,
        cursor: Option<&str>,
        start_now: bool,
        wait_secs: u64,
    ) -> Result<SyncResult> {
        let wait = wait_secs.min(crate::hub::POLL_WAIT_SECS);
        let mut body = serde_json::json!({
            "device_id": device_id,
            "device_name": device_name,
            "cursor": cursor,
            "wait": wait,
        });
        // the hub refuses `start` alongside a cursor
        if start_now && cursor.is_none() {
            body["start"] = "now".into();
        }
        self.post_json(me, "/api/sync", &body, Duration::from_secs(wait + 30))
            .await
    }

    /// One page of our conversation with `with`, older than `before`
    /// (hubs with "history"; a time needs "lazy_history").
    pub async fn history(
        &self,
        me: &Identity,
        with: &str,
        before: &Before,
        limit: u32,
    ) -> Result<HistoryPage> {
        let before = match before {
            Before::Time(ms) => ms.to_string(),
            Before::Cursor(c) => c.clone(),
        };
        let resp = self
            .http
            .get(self.address().join(&format!(
                "/api/history?with={}&before={}&limit={}",
                urlencode(with),
                urlencode(&before),
                limit
            )))
            .header("X-Org-Auth", me.auth_header())
            .timeout(Duration::from_secs(60))
            .send()
            .await?;
        Ok(check(resp).await?.json().await?)
    }

    /// Our chats on this hub, newest first (at most 1000), each with its
    /// newest message and how many of its messages we haven't read.
    pub async fn conversations(&self, me: &Identity) -> Result<Vec<Conversation>> {
        #[derive(Deserialize)]
        struct R {
            #[serde(default)]
            conversations: Vec<Conversation>,
        }
        let resp = self
            .http
            .get(self.address().join("/api/conversations"))
            .header("X-Org-Auth", me.auth_header())
            .timeout(Duration::from_secs(60))
            .send()
            .await?;
        Ok(check(resp).await?.json::<R>().await?.conversations)
    }

    /// This device is in use (true; it counts for 90 s, so repeat it about
    /// every minute) or not any more (false). Hubs with the "active" feature.
    pub async fn set_active(&self, me: &Identity, device_id: &str, active: bool) -> Result<()> {
        let body = serde_json::json!({ "device_id": device_id, "active": active });
        let _: serde_json::Value = self
            .post_json(me, "/api/devices/active", &body, Duration::from_secs(20))
            .await?;
        Ok(())
    }

    /// The whole body of a long message (sync cut it at 20,000 characters).
    pub async fn message_body(&self, me: &Identity, id: &str, max_bytes: u64) -> Result<String> {
        let resp = self
            .http
            .get(
                self.address()
                    .join(&format!("/api/messages/{}/body", urlencode(id))),
            )
            .header("X-Org-Auth", me.auth_header())
            .send()
            .await?;
        let resp = check(resp).await?;
        let mut out = Vec::new();
        let mut s = resp.bytes_stream();
        while let Some(c) = s.next().await {
            let c = c?;
            if (out.len() + c.len()) as u64 > max_bytes {
                return Err(Error::Invalid(format!(
                    "message body is larger than {max_bytes} bytes"
                )));
            }
            out.extend_from_slice(&c);
        }
        String::from_utf8(out).map_err(|_| Error::Invalid("message body is not UTF-8".into()))
    }

    /// Delete our copy of one message (the other side keeps theirs).
    pub async fn delete_message(&self, me: &Identity, id: &str) -> Result<u64> {
        self.delete(me, &format!("/api/messages/{}", urlencode(id)))
            .await
    }

    /// Delete our copy of a whole conversation.
    pub async fn delete_conversation(&self, me: &Identity, with: &str) -> Result<u64> {
        self.delete(me, &format!("/api/conversations/{}", urlencode(with)))
            .await
    }

    async fn delete(&self, me: &Identity, path: &str) -> Result<u64> {
        #[derive(Deserialize)]
        struct R {
            #[serde(default)]
            deleted: u64,
        }
        let resp = self
            .http
            .delete(self.address().join(path))
            .header("X-Org-Auth", me.auth_header())
            .timeout(Duration::from_secs(60))
            .send()
            .await?;
        match check(resp).await {
            Ok(r) => Ok(r.json::<R>().await?.deleted),
            Err(e) if e.status() == Some(404) => Ok(0),
            Err(e) => Err(e),
        }
    }

    /// The devices that have synced as this address.
    pub async fn devices(&self, me: &Identity) -> Result<Vec<DeviceEntry>> {
        #[derive(Deserialize)]
        struct R {
            devices: Vec<DeviceEntry>,
        }
        let resp = self
            .http
            .get(self.address().join("/api/devices"))
            .header("X-Org-Auth", me.auth_header())
            .timeout(Duration::from_secs(20))
            .send()
            .await?;
        Ok(check(resp).await?.json::<R>().await?.devices)
    }

    /// Upload a long body as a text part (v2, G6); returns its id for
    /// `Outgoing::body_part`.
    pub async fn upload_body(&self, me: &Identity, body: String) -> Result<String> {
        #[derive(Deserialize)]
        struct R {
            id: String,
        }
        let len = body.len();
        let resp = self
            .http
            .post(self.address().join("/api/attachments?name=body.txt"))
            .header("X-Org-Auth", me.auth_header())
            .header(reqwest::header::CONTENT_LENGTH, len)
            .body(body)
            .send()
            .await?;
        Ok(check(resp).await?.json::<R>().await?.id)
    }

    /// Take a device off the address's device list: the plain form, no key
    /// change (user ruling 2026-10-08 22:23Z; Hubchat sets no identity key,
    /// so the hub asks for no rotation).
    pub async fn sign_out_device(&self, me: &Identity, device_id: &str) -> Result<()> {
        let resp = self
            .http
            .delete(
                self.address()
                    .join(&format!("/api/devices/{}", urlencode(device_id))),
            )
            .header("X-Org-Auth", me.auth_header())
            .timeout(Duration::from_secs(30))
            .send()
            .await?;
        match check(resp).await {
            Ok(_) => Ok(()),
            Err(e) if matches!(e.status(), Some(404) | Some(409)) => Ok(()),
            Err(e) => Err(e),
        }
    }

    // ---------------------------------------------------- resumable uploads

    pub async fn open_upload(&self, me: &Identity, name: &str, bytes: u64) -> Result<UploadState> {
        self.post_json(
            me,
            "/api/uploads",
            &serde_json::json!({ "bytes": bytes, "name": name }),
            Duration::from_secs(30),
        )
        .await
    }

    pub async fn upload_state(&self, me: &Identity, id: &str) -> Result<UploadState> {
        let resp = self
            .http
            .get(
                self.address()
                    .join(&format!("/api/uploads/{}", urlencode(id))),
            )
            .header("X-Org-Auth", me.auth_header())
            .timeout(Duration::from_secs(30))
            .send()
            .await?;
        Ok(check(resp).await?.json().await?)
    }

    /// Send the file from where the upload stands. On a dropped connection
    /// call again: it asks the hub for the offset and continues from there.
    /// A hub still finishing a broken earlier attempt answers 409; that is
    /// waited out (briefly) rather than reported.
    pub async fn resume_upload(
        &self,
        me: &Identity,
        upload_id: &str,
        file: tokio::fs::File,
        progress: Option<Progress>,
        cancel: CancelFlag,
    ) -> Result<UploadState> {
        let std_file = file.into_std().await;
        let mut tries = 0;
        loop {
            let f = tokio::fs::File::from_std(std_file.try_clone()?);
            match self
                .resume_once(me, upload_id, f, progress.clone(), cancel.clone())
                .await
            {
                Err(e) if e.status() == Some(409) && tries < 20 && !cancel.is_cancelled() => {
                    tries += 1;
                    tokio::time::sleep(Duration::from_millis(500)).await;
                }
                r => return r,
            }
        }
    }

    async fn resume_once(
        &self,
        me: &Identity,
        upload_id: &str,
        mut file: tokio::fs::File,
        progress: Option<Progress>,
        cancel: CancelFlag,
    ) -> Result<UploadState> {
        let st = self.upload_state(me, upload_id).await?;
        if st.complete {
            return Ok(st);
        }
        let total = st.bytes;
        file.seek(std::io::SeekFrom::Start(st.offset)).await?;
        let mut done = st.offset;
        let reader = file.take(total - st.offset);
        let stream =
            tokio_util::io::ReaderStream::with_capacity(reader, 256 * 1024).map(move |chunk| {
                if cancel.is_cancelled() {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::Interrupted,
                        "cancelled",
                    ));
                }
                if let Ok(c) = &chunk {
                    done += c.len() as u64;
                    if let Some(p) = &progress {
                        p(done, total);
                    }
                }
                chunk
            });
        let body = reqwest::Body::wrap_stream(stream);
        let resp = self
            .http
            .patch(self.address().join(&format!(
                "/api/uploads/{}?offset={}",
                urlencode(upload_id),
                st.offset
            )))
            .header("X-Org-Auth", me.auth_header())
            .header(reqwest::header::CONTENT_LENGTH, total - st.offset)
            .body(body)
            .send()
            .await?;
        Ok(check(resp).await?.json().await?)
    }
}

/// Upload meta for a finished resumable upload (it is an ordinary attachment).
pub fn upload_meta(st: &UploadState, name: &str) -> AttachmentMeta {
    AttachmentMeta {
        id: st.id.clone(),
        name: name.to_owned(),
        bytes: st.bytes,
    }
}

fn urlencode(s: &str) -> String {
    url::form_urlencoded::byte_serialize(s.as_bytes()).collect()
}
