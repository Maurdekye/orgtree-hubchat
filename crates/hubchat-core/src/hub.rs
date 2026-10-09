//! Client for the mail hub protocol as served today (orgtree engine/mailhub,
//! app.py). One `HubClient` per hub. Every call except `healthz` authenticates
//! with `X-Org-Auth: <slug>:<secret>`.

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use tokio::io::AsyncWriteExt;
use url::Url;

use crate::{Error, Identity, Result};

/// The hub's default port when an `http` address names none.
pub const DEFAULT_PORT: u16 = 7370;
/// The ports a bare host is tried on, in order: the main port, a Docker
/// hub's relay-only door (host port), the relay-only door itself.
pub const DISCOVERY_PORTS: [u16; 3] = [DEFAULT_PORT, 7378, 7371];
/// Upload cap assumed for a hub that does not advertise one (today's hub).
pub const LEGACY_MAX_ATTACHMENT_BYTES: u64 = 25 * 1024 * 1024;
/// The hub caps a long poll at 55 s; we ask for that and allow slack on top.
pub const POLL_WAIT_SECS: u64 = 55;

/// A normalised hub base URL: no scheme means http, http without a port means
/// :7370, https is left alone (a tunnelled hub listens on 443).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HubAddress(String);

impl HubAddress {
    pub fn parse(input: &str) -> Result<Self> {
        let mut a = input.trim().trim_end_matches('/').to_owned();
        if a.is_empty() {
            return Err(Error::Invalid("enter a hub address".into()));
        }
        if !a.contains("://") {
            a = format!("http://{a}");
        }
        let mut u =
            Url::parse(&a).map_err(|e| Error::Invalid(format!("not a hub address: {e}")))?;
        match u.scheme() {
            "http" | "https" => {}
            s => return Err(Error::Invalid(format!("unsupported scheme {s}"))),
        }
        if u.host_str().is_none_or(str::is_empty) {
            return Err(Error::Invalid("the address has no host".into()));
        }
        // a typed :80 is kept (the URL itself drops a scheme's default port)
        if u.scheme() == "http" && u.port().is_none() && !has_port(&a) {
            let _ = u.set_port(Some(DEFAULT_PORT));
        }
        Ok(Self(u.as_str().trim_end_matches('/').to_owned()))
    }

    /// Where to look for a hub the user typed (user 23:46Z: no port to
    /// enter). A bare host, without scheme or port, is tried as the main port
    /// 7370, then 7378 (a Docker hub's relay-only door), then 7371 (the door
    /// itself, Orgtree's included), then https (a tunnel), in that order of
    /// preference; anything else is used as typed.
    pub fn candidates(input: &str) -> Result<Vec<Self>> {
        let typed = Self::parse(input)?;
        let t = input.trim().trim_end_matches('/');
        let authority = t.split(['/', '?', '#']).next().unwrap_or("");
        if t.contains("://") || has_port(t) || authority.len() != t.len() {
            return Ok(vec![typed]);
        }
        let mut all: Vec<Self> = DISCOVERY_PORTS
            .iter()
            .map(|p| Self::parse(&format!("http://{authority}:{p}")))
            .collect::<Result<_>>()?;
        all.push(Self::parse(&format!("https://{authority}"))?);
        Ok(all)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub(crate) fn join(&self, path: &str) -> String {
        format!("{}{}", self.0, path)
    }
}

/// The address names a port (`host:8000`, `http://[::1]:7371/x`).
fn has_port(addr: &str) -> bool {
    let rest = addr.split_once("://").map_or(addr, |(_, r)| r);
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    authority
        .rsplit_once(':')
        .is_some_and(|(h, p)| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()) && !h.ends_with(':'))
}

impl std::fmt::Display for HubAddress {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

// ---------------------------------------------------------------- wire types

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Health {
    pub ok: bool,
    pub name: String,
    #[serde(default)]
    pub retention_days: Option<i64>,
    /// Advertised by hubs that support large uploads (mail hub v2.0 and the
    /// raised-limit hub). Absent means the legacy 25 MiB.
    #[serde(default)]
    pub max_attachment_bytes: Option<u64>,
    /// Mail hub v2.0: protocol version and the additions it supports
    /// ("person", "profile", "reply_to", "sync", ...). Absent on older hubs.
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub features: Vec<String>,
    /// How many addresses the hub holds (with the name and version, tells
    /// whether two addresses lead to the same hub).
    #[serde(default)]
    pub orgs: Option<u64>,
}

impl Health {
    pub fn supports(&self, feature: &str) -> bool {
        self.features.iter().any(|f| f == feature)
    }

    pub fn max_attachment_bytes(&self) -> u64 {
        self.max_attachment_bytes
            .unwrap_or(LEGACY_MAX_ATTACHMENT_BYTES)
    }
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct Profile {
    /// "chat" or "org" today; anything else is stored as "org" by today's hub.
    pub kind: String,
    pub org_name: String,
    pub username: String,
    pub blurb: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct RosterEntry {
    pub slug: String,
    #[serde(default)]
    pub org_name: String,
    #[serde(default)]
    pub username: String,
    #[serde(default)]
    pub blurb: String,
    #[serde(default)]
    pub online: bool,
    #[serde(default)]
    pub last_seen: Option<String>,
    #[serde(default)]
    pub kind: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Registered {
    pub name: String,
    #[serde(default)]
    pub retention_days: Option<i64>,
    #[serde(default)]
    pub roster: Vec<RosterEntry>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct AttachmentMeta {
    pub id: String,
    pub name: String,
    pub bytes: u64,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct Envelope {
    pub id: String,
    pub from: String,
    pub to: String,
    pub body: String,
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub thread_id: Option<String>,
    #[serde(default)]
    pub sent_at: Option<String>,
    pub received_at: String,
    /// v2 (G3): the id of the message this one answers.
    #[serde(default)]
    pub reply_to: Option<String>,
    #[serde(default)]
    pub attachments: Vec<AttachmentMeta>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct Receipt {
    pub id: String,
    /// received | fetched | delivered | read
    pub state: String,
    #[serde(default)]
    pub fetched_at: Option<String>,
    #[serde(default)]
    pub delivered_at: Option<String>,
    #[serde(default)]
    pub read_at: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PollResult {
    pub name: String,
    /// v2: the hub's version (absent on v1 hubs).
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub messages: Vec<Envelope>,
    #[serde(default)]
    pub receipts: Vec<Receipt>,
    #[serde(default)]
    pub roster: Vec<RosterEntry>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct Outgoing {
    /// Client-minted id: makes a retried send idempotent.
    pub id: String,
    pub to: String,
    pub body: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thread_id: Option<String>,
    /// Only sent to hubs that list "reply_to" in /healthz features.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reply_to: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sent_at: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub attachments: Vec<String>,
    /// v2 (G6): the body was uploaded like a file; `body` is then empty.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body_part: Option<String>,
}

impl Outgoing {
    pub fn new(to: &str, body: &str) -> Self {
        Self {
            id: uuid::Uuid::new_v4().simple().to_string(),
            to: to.into(),
            body: body.into(),
            ..Default::default()
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SendResult {
    pub id: String,
    pub received_at: String,
    #[serde(default)]
    pub duplicate: bool,
}

/// Transfer progress: bytes done of total. Called often; keep it cheap.
pub type Progress = Arc<dyn Fn(u64, u64) + Send + Sync>;

/// Set to cancel an in-flight transfer.
#[derive(Clone, Default)]
pub struct CancelFlag(Arc<AtomicBool>);

impl CancelFlag {
    pub fn cancel(&self) {
        self.0.store(true, Ordering::SeqCst)
    }
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}

// -------------------------------------------------------------------- client

#[derive(Clone)]
pub struct HubClient {
    addr: HubAddress,
    pub(crate) http: reqwest::Client,
}

impl HubClient {
    pub fn new(addr: HubAddress) -> Self {
        let http = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .tcp_keepalive(Duration::from_secs(30))
            .build()
            .expect("http client");
        Self { addr, http }
    }

    pub fn address(&self) -> &HubAddress {
        &self.addr
    }

    /// Probe: is something there, and is it a mail hub? Unauthenticated.
    pub async fn healthz(&self) -> Result<Health> {
        let resp = self
            .http
            .get(self.addr.join("/healthz"))
            .timeout(Duration::from_secs(10))
            .send()
            .await?;
        if !resp.status().is_success() {
            return Err(Error::NotAHub(format!(
                "/healthz answered {}",
                resp.status()
            )));
        }
        let text = resp.text().await?;
        let h: Health = serde_json::from_str(&text)
            .map_err(|_| Error::NotAHub("unexpected /healthz reply".into()))?;
        if !h.ok {
            return Err(Error::NotAHub("/healthz is not ok".into()));
        }
        Ok(h)
    }

    /// Register (or refresh) our address. 403 = the id.tag is held by another
    /// secret, which cannot happen for a tag derived from our own secret
    /// unless the hash prefix collides with someone else's id.
    pub async fn register(&self, me: &Identity, profile: &Profile) -> Result<Registered> {
        #[derive(Serialize)]
        struct Req<'a> {
            slug: String,
            kind: &'a str,
            org_name: &'a str,
            username: &'a str,
            blurb: &'a str,
        }
        let req = Req {
            slug: me.address(),
            kind: &profile.kind,
            org_name: &profile.org_name,
            username: &profile.username,
            blurb: &profile.blurb,
        };
        self.post_json(me, "/api/register", &req, Duration::from_secs(20))
            .await
    }

    /// v2 (G2): change our display name / about line. Only on hubs that
    /// list "profile" in their features.
    pub async fn set_profile(&self, me: &Identity, name: &str, about: &str) -> Result<()> {
        let _: serde_json::Value = self
            .post_json(
                me,
                "/api/profile",
                &serde_json::json!({ "name": name, "about": about }),
                Duration::from_secs(20),
            )
            .await?;
        Ok(())
    }

    pub async fn unregister(&self, me: &Identity) -> Result<()> {
        let _: serde_json::Value = self
            .post_json(
                me,
                "/api/unregister",
                &serde_json::json!({}),
                Duration::from_secs(20),
            )
            .await?;
        Ok(())
    }

    /// Long poll: returns as soon as there are messages or receipts for us,
    /// or after `wait_secs` (max 55) with just the roster.
    pub async fn poll(&self, me: &Identity, wait_secs: u64) -> Result<PollResult> {
        let wait = wait_secs.min(POLL_WAIT_SECS);
        let path = format!("/api/poll?wait={wait}");
        self.post_json(
            me,
            &path,
            &serde_json::json!({}),
            Duration::from_secs(wait + 20),
        )
        .await
    }

    /// Take custody: the hub stops offering these ids (state -> fetched).
    pub async fn ack(&self, me: &Identity, ids: &[String]) -> Result<u64> {
        #[derive(Deserialize)]
        struct R {
            acked: u64,
        }
        let r: R = self
            .post_json(
                me,
                "/api/ack",
                &serde_json::json!({ "ids": ids }),
                Duration::from_secs(20),
            )
            .await?;
        Ok(r.acked)
    }

    pub async fn send(&self, me: &Identity, msg: &Outgoing) -> Result<SendResult> {
        #[derive(Serialize)]
        struct Req<'a> {
            from: String,
            #[serde(flatten)]
            msg: &'a Outgoing,
        }
        self.post_json(
            me,
            "/api/send",
            &Req {
                from: me.address(),
                msg,
            },
            Duration::from_secs(60),
        )
        .await
    }

    /// Tell senders we delivered/read their messages. `state` is
    /// "delivered" or "read"; `at` is an ISO timestamp.
    pub async fn receipts(&self, me: &Identity, items: &[(String, &str, String)]) -> Result<u64> {
        #[derive(Deserialize)]
        struct R {
            recorded: u64,
        }
        let list: Vec<_> = items
            .iter()
            .map(|(id, state, at)| serde_json::json!({"id": id, "state": state, "at": at}))
            .collect();
        let r: R = self
            .post_json(
                me,
                "/api/receipts",
                &serde_json::json!({ "receipts": list }),
                Duration::from_secs(20),
            )
            .await?;
        Ok(r.recorded)
    }

    pub async fn roster(&self, me: &Identity) -> Result<Vec<RosterEntry>> {
        #[derive(Deserialize)]
        struct R {
            roster: Vec<RosterEntry>,
        }
        let resp = self
            .http
            .get(self.addr.join("/api/roster"))
            .header("X-Org-Auth", me.auth_header())
            .timeout(Duration::from_secs(20))
            .send()
            .await?;
        Ok(check(resp).await?.json::<R>().await?.roster)
    }

    /// Stream a file to the hub without reading it into memory. Returns the
    /// attachment id to put in `Outgoing::attachments`.
    pub async fn upload_file(
        &self,
        me: &Identity,
        path: &Path,
        name: &str,
        progress: Option<Progress>,
        cancel: CancelFlag,
    ) -> Result<AttachmentMeta> {
        let file = tokio::fs::File::open(path).await?;
        self.upload(me, file, name, progress, cancel).await
    }

    /// Stream an already-open file (on Android: the descriptor behind a
    /// content:// URI) to the hub.
    pub async fn upload(
        &self,
        me: &Identity,
        file: tokio::fs::File,
        name: &str,
        progress: Option<Progress>,
        cancel: CancelFlag,
    ) -> Result<AttachmentMeta> {
        let total = file.metadata().await?.len();
        let mut done = 0u64;
        let stream =
            tokio_util::io::ReaderStream::with_capacity(file, 256 * 1024).map(move |chunk| {
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
        #[derive(Deserialize)]
        struct R {
            id: String,
            bytes: u64,
        }
        let mut url = Url::parse(&self.addr.join("/api/attachments"))
            .map_err(|e| Error::Invalid(e.to_string()))?;
        url.query_pairs_mut().append_pair("name", name);
        let resp = self
            .http
            .post(url)
            .header("X-Org-Auth", me.auth_header())
            .header(reqwest::header::CONTENT_LENGTH, total)
            .body(reqwest::Body::wrap_stream(stream))
            .send()
            .await?;
        let r: R = check(resp).await?.json().await?;
        Ok(AttachmentMeta {
            id: r.id,
            name: name.to_owned(),
            bytes: r.bytes,
        })
    }

    /// Download an attachment to `dest` (written to `dest.part`, renamed when
    /// complete). 410 = the hub no longer has the file.
    pub async fn download_file(
        &self,
        me: &Identity,
        attachment_id: &str,
        dest: &Path,
        progress: Option<Progress>,
        cancel: CancelFlag,
    ) -> Result<u64> {
        let resp = self
            .http
            .get(self.addr.join(&format!("/api/attachments/{attachment_id}")))
            .header("X-Org-Auth", me.auth_header())
            .send()
            .await?;
        let resp = check(resp).await?;
        let total = resp.content_length().unwrap_or(0);
        let part = dest.with_extension(match dest.extension() {
            Some(e) => format!("{}.part", e.to_string_lossy()),
            None => "part".into(),
        });
        let mut out = tokio::fs::File::create(&part).await?;
        let mut done = 0u64;
        let mut body = resp.bytes_stream();
        while let Some(chunk) = body.next().await {
            if cancel.is_cancelled() {
                drop(out);
                let _ = tokio::fs::remove_file(&part).await;
                return Err(Error::Cancelled);
            }
            let chunk = chunk?;
            out.write_all(&chunk).await?;
            done += chunk.len() as u64;
            if let Some(p) = &progress {
                p(done, total.max(done));
            }
        }
        out.flush().await?;
        drop(out);
        tokio::fs::rename(&part, dest).await?;
        Ok(done)
    }

    pub(crate) async fn post_json<B: Serialize + ?Sized, R: serde::de::DeserializeOwned>(
        &self,
        me: &Identity,
        path: &str,
        body: &B,
        timeout: Duration,
    ) -> Result<R> {
        let resp = self
            .http
            .post(self.addr.join(path))
            .header("X-Org-Auth", me.auth_header())
            .json(body)
            .timeout(timeout)
            .send()
            .await?;
        Ok(check(resp).await?.json::<R>().await?)
    }
}

/// Turn an HTTP error status into `Error::Hub` with the hub's `detail`.
pub(crate) async fn check(resp: reqwest::Response) -> Result<reqwest::Response> {
    let status = resp.status();
    if status.is_success() {
        return Ok(resp);
    }
    let text = resp.text().await.unwrap_or_default();
    let detail = serde_json::from_str::<serde_json::Value>(&text)
        .ok()
        .and_then(|v| {
            v.get("detail").map(|d| {
                d.as_str()
                    .map(str::to_owned)
                    .unwrap_or_else(|| d.to_string())
            })
        })
        .unwrap_or(text);
    Err(Error::Hub {
        status: status.as_u16(),
        detail,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalises_hub_addresses() {
        let n = |s: &str| HubAddress::parse(s).unwrap().as_str().to_owned();
        assert_eq!(n("hub.office.lan"), "http://hub.office.lan:7370");
        assert_eq!(n("10.0.4.2:8000/"), "http://10.0.4.2:8000");
        assert_eq!(n("http://x"), "http://x:7370");
        assert_eq!(n("https://hub.example.com"), "https://hub.example.com");
        assert!(HubAddress::parse("  ").is_err());
        assert!(HubAddress::parse("ftp://x").is_err());
    }

    #[test]
    fn a_bare_host_is_looked_for_on_the_known_ports() {
        let c = |s: &str| -> Vec<String> {
            HubAddress::candidates(s).unwrap().iter().map(|a| a.as_str().to_owned()).collect()
        };
        assert_eq!(
            c("star-hub"),
            [
                "http://star-hub:7370",
                "http://star-hub:7378",
                "http://star-hub:7371",
                "https://star-hub"
            ]
        );
        assert_eq!(c(" 100.64.1.2/ ").len(), 4);
        // an explicit port or URL is used as typed
        assert_eq!(c("star-hub:7378"), ["http://star-hub:7378"]);
        assert_eq!(c("star-hub:80"), ["http://star-hub"]);
        assert_eq!(c("http://star-hub"), ["http://star-hub:7370"]);
        assert_eq!(c("https://xyz.trycloudflare.com"), ["https://xyz.trycloudflare.com"]);
        assert_eq!(c("[::1]:7371"), ["http://[::1]:7371"]);
        assert_eq!(c("star-hub/some/path"), ["http://star-hub:7370/some/path"]);
        assert!(HubAddress::candidates(" ").is_err());
    }
}
