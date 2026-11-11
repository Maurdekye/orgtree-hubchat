//! The hub stream (Mail hub 2.5.0, `GET /api/stream`; Orgtree docket
//! mail-hub-websocket-for-new-mail-clients-prefer-i, spec-hub-websocket.md
//! section 9 and 12): `HubClient::sync` is carried over one WebSocket when the
//! hub lists `stream` in /healthz `features`, and over HTTP long polling
//! otherwise. Same request body, same `SyncResult`: only the carrier differs,
//! and a switch happens only between requests, so the cursor never moves.
//!
//! One `Carrier` per `HubClient` (copies of a client share it) owns the state
//! machine of spec 9.1 to 9.4. It holds the open socket between calls, so a
//! cancelled `sync` (the engine's status wake) leaves the socket in place and
//! the next call supersedes the parked request (spec 5.1, C3).

use std::time::{Duration, Instant};

use futures_util::{SinkExt, StreamExt};
use rand::Rng;
use serde::Serialize;
use serde_json::{json, Value};
use tokio::sync::Mutex;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::protocol::WebSocketConfig;
use tokio_tungstenite::tungstenite::Message;

use crate::hub::HubClient;
use crate::hub_v2::SyncResult;
use crate::{Error, Identity, Result};

const PROTO: &str = "orgtree-hub-stream.1";
/// largest answer read: spec 8 says at least 64 MiB
const MAX_ANSWER: usize = 64 * 1024 * 1024;
/// connect + TLS + 101, and the welcome after the hello
const OPEN_TIMEOUT: Duration = Duration::from_secs(10);
/// an answer must come within the request's wait plus this
const ANSWER_SLACK: Duration = Duration::from_secs(15);
/// a stream that ends sooner than this after its welcome, with no answer, is short-lived
const SHORT_SESSION: Duration = Duration::from_secs(30);
const SHORT_SESSIONS_UNUSABLE: u32 = 3;
/// minutes of long polling before the next stream attempt (spec 9.3); the last repeats
const PROBE_MINUTES: [u64; 4] = [1, 5, 15, 60];
/// seconds before a reconnect after a drop (spec 9.4); the last repeats
const RECONNECT_SECS: [f64; 6] = [1.0, 2.0, 4.0, 8.0, 16.0, 30.0];
/// a hub without the feature is looked at again this often
const REPROBE: Duration = Duration::from_secs(30 * 60);

/// How the next request is carried, and why (diagnostics, spec 11.2).
#[derive(Debug, Clone, Default, Serialize, PartialEq)]
pub struct TransportStatus {
    /// "stream" or "long poll"
    pub transport: String,
    /// unix ms of the last change of transport
    pub transport_since_ms: Option<u64>,
    /// why long polling: no_feature, handshake_<code>, unstable, limit, protocol
    pub fallback_reason: Option<String>,
    /// unix ms of the next stream attempt while long polling
    pub next_probe_ms: Option<u64>,
}

type Socket = tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

struct Conn {
    ws: Socket,
    opened: Instant,
    answered: bool,
}

#[derive(Default)]
struct Inner {
    /// the hub's /healthz has been read since the last reason to look again
    probed: bool,
    listed: bool,
    version: Option<String>,
    probed_at: Option<Instant>,
    conn: Option<Conn>,
    next_id: u64,
    /// no stream attempt before this while long polling
    next_attempt: Option<Instant>,
    probe_step: usize,
    reconnect_step: usize,
    short_sessions: u32,
    /// 1009 or 4400: a bug on one side; long poll until the app restarts
    off_for_good: bool,
    /// a 401 on the stream was already handed to the caller once
    unauthorized_once: bool,
    /// the next retry is an ordinary reconnect: wait out the back-off first
    pause_next: bool,
    status: TransportStatus,
}

/// What one trip over the stream came to.
enum Trip {
    Answer(u16, Value),
    /// it ended: ask again (after the wait it set), or the caller sees the error
    Retry,
    Fail(Error),
}

pub(crate) struct Carrier {
    inner: Mutex<Inner>,
    /// test knob `HUBCHAT_STREAM_TIME_DIV`: every back-off is divided by it
    div: u32,
}

impl Default for Carrier {
    fn default() -> Self {
        let div = std::env::var("HUBCHAT_STREAM_TIME_DIV").ok().and_then(|v| v.parse().ok()).filter(|d| *d >= 1).unwrap_or(1);
        Carrier { inner: Mutex::new(Inner::default()), div }
    }
}

fn unix_ms() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

/// `http://h/x` -> `ws://h/x/api/stream`; `https` -> `wss`.
pub(crate) fn stream_url(address: &str) -> String {
    let base = address.trim_end_matches('/');
    let swapped = if let Some(rest) = base.strip_prefix("https://") {
        format!("wss://{rest}")
    } else if let Some(rest) = base.strip_prefix("http://") {
        format!("ws://{rest}")
    } else {
        base.to_string()
    };
    format!("{swapped}/api/stream")
}

fn jitter(secs: f64) -> f64 {
    secs * rand::thread_rng().gen_range(0.8..1.2)
}

impl Carrier {
    fn scaled(&self, d: Duration) -> Duration {
        d / self.div
    }

    /// A snapshot for the hub's status.
    pub(crate) async fn status(&self) -> TransportStatus {
        self.inner.lock().await.status.clone()
    }

    /// `HubClient::sync`: the request body and the wait, carried by whichever
    /// carrier the state machine picks. Returns what the HTTP route returns.
    pub(crate) async fn sync(&self, client: &HubClient, me: &Identity, body: Value, wait: u64) -> Result<SyncResult> {
        let mut g = self.inner.lock().await;
        // a switch to long polling keeps looping here until a request is carried
        let mut handshake_403 = false;
        loop {
            if !g.probed || (!g.listed && g.probed_at.is_some_and(|t| t.elapsed() >= REPROBE)) {
                self.probe(client, &mut g).await?;
            }
            if !self.use_stream(&g) {
                let r: SyncResult = client.post_json(me, "/api/sync", &body, Duration::from_secs(wait + 30)).await?;
                // a new version in a long-poll answer: look at /healthz again
                if r.version.is_some() && r.version != g.version {
                    g.probed = false;
                }
                return Ok(r);
            }
            if g.conn.is_none() {
                match self.open(client, me, &mut g).await {
                    Ok(()) => {}
                    Err(Opening::Unauthorized(e)) => return Err(e),
                    Err(Opening::Downgraded) if !handshake_403 => {
                        // a 403 with no body is a hub that no longer has the stream: look again
                        handshake_403 = true;
                        g.probed = false;
                        continue;
                    }
                    Err(Opening::Downgraded) => self.fall_back(&mut g, "handshake_403", PROBE_MINUTES[0]),
                    Err(Opening::Failed(reason, after)) => self.fall_back_for(&mut g, &reason, after),
                }
                continue;
            }
            match self.trip(&mut g, &body, wait).await {
                Trip::Answer(status, answer) => {
                    return if status == 200 {
                        serde_json::from_value(answer).map_err(|e| Error::NotAHub(format!("sync answer: {e}")))
                    } else {
                        Err(Error::Hub { status, detail: detail_of(&answer) })
                    };
                }
                Trip::Retry => {
                    if std::mem::take(&mut g.pause_next) {
                        self.pause(&mut g).await;
                    }
                    continue;
                }
                Trip::Fail(e) => return Err(e),
            }
        }
    }

    fn use_stream(&self, g: &Inner) -> bool {
        g.listed && !g.off_for_good && g.next_attempt.is_none_or(|t| Instant::now() >= t)
    }

    async fn probe(&self, client: &HubClient, g: &mut Inner) -> Result<()> {
        let health = client.healthz().await?;
        g.probed = true;
        g.probed_at = Some(Instant::now());
        g.listed = health.features.iter().any(|f| f == "stream");
        g.version = health.version.clone();
        if g.listed {
            if g.status.transport != "stream" && g.next_attempt.is_none() {
                self.set_transport(g, "stream", None);
            }
        } else {
            g.conn = None;
            self.set_transport(g, "long poll", Some("no_feature".into()));
            g.next_attempt = None;
        }
        Ok(())
    }

    fn set_transport(&self, g: &mut Inner, transport: &str, reason: Option<String>) {
        if g.status.transport != transport || g.status.fallback_reason != reason {
            tracing::info!(transport, reason = reason.as_deref().unwrap_or(""), "hub transport");
            g.status.transport = transport.into();
            g.status.transport_since_ms = Some(unix_ms());
            g.status.fallback_reason = reason;
        }
    }

    /// Long poll until the schedule's next step (spec 9.3).
    fn fall_back(&self, g: &mut Inner, reason: &str, minutes: u64) {
        let after = self.scaled(Duration::from_secs(minutes * 60));
        self.fall_back_for(g, reason, after);
        g.probe_step = 1;
    }

    fn fall_back_for(&self, g: &mut Inner, reason: &str, after: Option<Duration>) {
        let wait = match after {
            Some(d) => d,
            None => {
                let m = PROBE_MINUTES[g.probe_step.min(PROBE_MINUTES.len() - 1)];
                g.probe_step = (g.probe_step + 1).min(PROBE_MINUTES.len() - 1);
                self.scaled(Duration::from_secs(m * 60))
            }
        };
        g.conn = None;
        g.next_attempt = Some(Instant::now() + wait);
        g.status.next_probe_ms = Some(unix_ms() + wait.as_millis() as u64);
        self.set_transport(g, "long poll", Some(reason.into()));
    }

    /// Wait out the reconnect back-off (spec 9.4).
    async fn pause(&self, g: &mut Inner) {
        let secs = RECONNECT_SECS[g.reconnect_step.min(RECONNECT_SECS.len() - 1)];
        g.reconnect_step += 1;
        tokio::time::sleep(self.scaled(Duration::from_secs_f64(jitter(secs)))).await;
    }

    /// Connect, hello, welcome (spec 9.2).
    async fn open(&self, client: &HubClient, me: &Identity, g: &mut Inner) -> std::result::Result<(), Opening> {
        let url = stream_url(client.address_str());
        let mut req = url.into_client_request().map_err(|e| Opening::Failed(format!("bad_url: {e}"), None))?;
        req.headers_mut().insert("sec-websocket-protocol", PROTO.parse().expect("header"));
        req.headers_mut().insert("x-org-auth", me.auth_header().parse().map_err(|_| Opening::Failed("bad_auth".into(), None))?);
        let config = WebSocketConfig::default().max_message_size(Some(MAX_ANSWER)).max_frame_size(Some(MAX_ANSWER));
        let connect = tokio_tungstenite::connect_async_with_config(req, Some(config), false);
        let mut ws = match tokio::time::timeout(OPEN_TIMEOUT, connect).await {
            Err(_) => return Err(Opening::Failed("handshake_timeout".into(), None)),
            Ok(Ok((ws, _))) => ws,
            Ok(Err(tokio_tungstenite::tungstenite::Error::Http(resp))) => {
                let code = resp.status().as_u16();
                let empty = resp.body().as_ref().is_none_or(|b| b.is_empty());
                return Err(match code {
                    403 if empty => Opening::Downgraded,
                    401 => self.unauthorized(g),
                    503 => Opening::Failed("handshake_503".into(), Some(self.scaled(Duration::from_secs(30)))),
                    c => Opening::Failed(format!("handshake_{c}"), None),
                });
            }
            Ok(Err(e)) => return Err(Opening::Failed(format!("handshake_failed: {e}"), None)),
        };
        let hello = json!({ "type": "hello", "v": 1, "client": format!("hubchat/{} ({})", env!("CARGO_PKG_VERSION"), std::env::consts::OS) });
        if ws.send(Message::Text(hello.to_string().into())).await.is_err() {
            return Err(Opening::Failed("hello_failed".into(), None));
        }
        let welcome = tokio::time::timeout(OPEN_TIMEOUT, async {
            loop {
                match ws.next().await {
                    Some(Ok(Message::Text(t))) => return Ok(serde_json::from_str::<Value>(t.as_str()).ok()),
                    Some(Ok(Message::Close(f))) => return Err(f.map(|f| u16::from(f.code))),
                    Some(Ok(_)) => continue,
                    _ => return Err(None),
                }
            }
        })
        .await;
        match welcome {
            Ok(Ok(Some(w))) if w["type"] == "welcome" => {}
            Ok(Err(Some(4401))) => return Err(self.unauthorized(g)),
            Ok(Err(Some(4429))) => return Err(Opening::Failed("limit".into(), Some(self.scaled(Duration::from_secs(15 * 60))))),
            Ok(Err(Some(c @ (1009 | 4400)))) => {
                g.off_for_good = true;
                return Err(Opening::Failed(format!("protocol_{c}"), Some(self.scaled(Duration::from_secs(60 * 60)))));
            }
            _ => return Err(Opening::Failed("no_welcome".into(), None)),
        }
        g.conn = Some(Conn { ws, opened: Instant::now(), answered: false });
        g.unauthorized_once = false;
        g.probe_step = 0;
        g.next_attempt = None;
        g.status.next_probe_ms = None;
        self.set_transport(g, "stream", None);
        Ok(())
    }

    /// A 401 at the handshake or the hello: the caller sees it once (the engine
    /// registers and asks again); a second one in a row is the long poll's to handle.
    fn unauthorized(&self, g: &mut Inner) -> Opening {
        if g.unauthorized_once {
            return Opening::Failed("handshake_401".into(), None);
        }
        g.unauthorized_once = true;
        Opening::Unauthorized(Error::Hub { status: 401, detail: "no valid org credentials".into() })
    }

    /// One request over the open stream and its answer (spec 5.1, 7.1).
    async fn trip(&self, g: &mut Inner, body: &Value, wait: u64) -> Trip {
        g.next_id += 1;
        let id = g.next_id;
        let frame = json!({ "type": "sync", "id": id, "body": body });
        let deadline = Duration::from_secs(wait) + ANSWER_SLACK;
        let outcome = {
            let conn = g.conn.as_mut().expect("an open stream");
            match conn.ws.send(Message::Text(frame.to_string().into())).await {
                Err(_) => Err(None),
                Ok(()) => tokio::time::timeout(deadline, async {
                    loop {
                        match conn.ws.next().await {
                            Some(Ok(Message::Text(t))) => {
                                let Ok(v) = serde_json::from_str::<Value>(t.as_str()) else { continue };
                                // an answer to a request a newer one replaced: not ours
                                if v["type"] == "answer" && v["id"].as_u64() == Some(id) {
                                    return Ok((v["status"].as_u64().unwrap_or(500) as u16, v["body"].clone()));
                                }
                            }
                            Some(Ok(Message::Close(f))) => return Err(f.map(|f| u16::from(f.code))),
                            Some(Ok(_)) => {}
                            _ => return Err(None),
                        }
                    }
                })
                .await
                .unwrap_or(Err(None)),
            }
        };
        match outcome {
            Ok((status, answer)) => {
                if let Some(c) = g.conn.as_mut() {
                    c.answered = true;
                }
                g.reconnect_step = 0;
                g.short_sessions = 0;
                Trip::Answer(status, answer)
            }
            Err(code) => self.dropped(g, code),
        }
    }

    /// The stream ended (spec 9.3, 9.4): decide what the request does next.
    fn dropped(&self, g: &mut Inner, code: Option<u16>) -> Trip {
        let conn = g.conn.take();
        match code {
            Some(4401) => match self.unauthorized(g) {
                Opening::Unauthorized(e) => return Trip::Fail(e),
                _ => {
                    self.fall_back(g, "handshake_401", PROBE_MINUTES[0]);
                    return Trip::Retry;
                }
            },
            Some(4429) => {
                self.fall_back_for(g, "limit", Some(self.scaled(Duration::from_secs(15 * 60))));
                return Trip::Retry;
            }
            Some(c @ (1009 | 4400)) => {
                g.off_for_good = true;
                self.fall_back_for(g, &format!("protocol_{c}"), Some(self.scaled(Duration::from_secs(60 * 60))));
                return Trip::Retry;
            }
            _ => {}
        }
        if let Some(c) = &conn {
            if !c.answered && c.opened.elapsed() < self.scaled(SHORT_SESSION) {
                g.short_sessions += 1;
                if g.short_sessions >= SHORT_SESSIONS_UNUSABLE {
                    g.short_sessions = 0;
                    self.fall_back_for(g, "unstable", None);
                    return Trip::Retry;
                }
            }
        }
        // 1012: the hub is restarting, so start the back-off higher
        if code == Some(1012) && g.reconnect_step < 1 {
            g.reconnect_step = 1;
        }
        g.pause_next = true;
        Trip::Retry
    }
}

/// Why a stream could not be opened.
enum Opening {
    /// 401: handed to the caller
    Unauthorized(Error),
    /// 403 with an empty body: today's hub refusing every upgrade
    Downgraded,
    /// long poll, and try again after this (None: the schedule's next step)
    Failed(String, Option<Duration>),
}

fn detail_of(body: &Value) -> String {
    match body.get("detail") {
        Some(Value::String(s)) => s.clone(),
        Some(other) => other.to_string(),
        None => body.to_string(),
    }
}
