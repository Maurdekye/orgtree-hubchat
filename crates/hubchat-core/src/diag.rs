//! Diagnostics shared by the core and the shell: what the log may say, and
//! the helpers that keep call sites from saying more (docs/logging.md).
//!
//! The core only emits `tracing` events; the shell (src-tauri `trace.rs`)
//! owns the subscriber, the file, its rotation and its retention. Without a
//! subscriber (Android, tests) every event here costs one atomic load.
//!
//! The log is made safe by what is passed to it, not by cleaning it up
//! afterwards: identifiers are pseudonymous tags (`tag`), errors are names
//! from a fixed list (`err_kind`), paths are endpoints (`wire_path`), and
//! operations state a shape the caller chose (`traced_with`). Error text, keys,
//! recovery words, bodies, file names, addresses and hosts are never passed in,
//! in verbose mode either. The layer in `trace.rs` adds a heuristic mask for
//! secret-named fields and long token-like strings; that is a second net with
//! known holes (a short secret in free text is not recognisable), not a reason
//! to pass content.

use std::future::Future;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use sha2::{Digest, Sha256};
use tracing::Level;

use crate::{Error, Result};

/// Verbose logging (Orgtree's `LOG_VERBOSE`): the call and return line of
/// every traced operation, every REQUEST line and every successful RESPONSE
/// line, and the UI's command names. Off, an operation costs one atomic load;
/// warnings, errors, state changes and slow or failed hub answers are written
/// either way. Off by default in a release build, on in a development build;
/// the shell sets it from the Developer tab.
static VERBOSE: AtomicBool = AtomicBool::new(cfg!(debug_assertions));

pub fn verbose() -> bool {
    VERBOSE.load(Ordering::Relaxed)
}

/// Turn verbose logging on or off; answers the previous state.
pub fn set_verbose(on: bool) -> bool {
    VERBOSE.swap(on, Ordering::Relaxed)
}

/// Whether a build is a development build (verbose logging defaults to on).
pub const fn dev_build() -> bool {
    cfg!(debug_assertions)
}

// ----------------------------------------------------------------- tags

static SALT: OnceLock<[u8; 16]> = OnceLock::new();

/// The per-device salt behind `tag` (the shell keeps it in its diagnostics
/// folder so a tag stays the same across starts). The first call wins.
pub fn set_salt(salt: [u8; 16]) {
    let _ = SALT.set(salt);
}

fn salt() -> &'static [u8; 16] {
    SALT.get_or_init(|| rand::random())
}

/// A short pseudonym for an identifier (`peer:3fa1c2d4`, 32 bits): the same
/// value has the same tag on this device, so lines can be matched. It is not
/// anonymisation: two values can share a tag, and someone who holds both the
/// log and the device's salt file can test a guess. Without the salt the value
/// cannot be recomputed from the tag.
pub fn tag(kind: &str, value: &str) -> String {
    let mut h = Sha256::new();
    h.update(salt());
    h.update(kind.as_bytes());
    h.update([0]);
    h.update(value.as_bytes());
    format!("{kind}:{}", hex::encode(&h.finalize()[..4]))
}

// ------------------------------------------------------------ error kinds

/// A name for an error drawn from a fixed list: what every log line says about
/// a failure, in verbose mode too. Nothing of the error's own text is copied
/// (it can carry URLs, hub replies, paths, file names and addresses); the
/// network cause is found by looking for known phrases, never by quoting.
pub fn err_kind(e: &Error) -> String {
    match e {
        Error::Unreachable(m) => format!("unreachable:{}", net_cause(m)),
        Error::Hub { status, .. } => format!("hub-{status}"),
        Error::NotAHub(_) => "not-a-hub".into(),
        Error::Invalid(_) => "invalid".into(),
        Error::Store(_) => "store".into(),
        Error::BodyTooLarge(_) => "body-too-large".into(),
        Error::LocalBusy => "local-busy".into(),
        Error::FilePaused => "file-paused".into(),
        Error::Cancelled => "cancelled".into(),
        Error::SaveDownload(e) => format!("save-download:{}", io_kind(e)),
        Error::Io(e) => io_kind(e),
    }
}

/// The std kind and, when there is one, the OS error number.
fn io_kind(e: &std::io::Error) -> String {
    let kind = format!("io-{:?}", e.kind()).to_ascii_lowercase();
    match e.raw_os_error() {
        Some(n) => format!("{kind}(os {n})"),
        None => kind,
    }
}

/// Why a hub could not be reached, as one of a fixed set of words.
fn net_cause(message: &str) -> &'static str {
    let m = message.to_ascii_lowercase();
    // a one-word needle starts a word ("tls" is not inside "bottlsx"); a phrase is found as written
    let split: Vec<&str> = m.split(|c: char| !c.is_ascii_alphanumeric()).filter(|w| !w.is_empty()).collect();
    let has = |needles: &[&str]| {
        needles.iter().any(|n| if n.contains(' ') { m.contains(n) } else { split.iter().any(|w| w.starts_with(n)) })
    };
    if has(&["timed out", "timeout", "deadline"]) {
        "timeout"
    } else if has(&["dns", "lookup", "resolve", "name or service", "no such host"]) {
        "dns"
    } else if has(&["refused"]) {
        "refused"
    } else if has(&["certificate", "tls", "ssl", "handshake"]) {
        "tls"
    } else if has(&["reset", "broken pipe", "aborted", "closed", "eof"]) {
        "reset"
    } else if has(&["unreachable", "no route", "network is", "network error"]) {
        "network"
    } else if has(&["redirect"]) {
        "redirect"
    } else if has(&["decod", "body"]) {
        "body"
    } else {
        "other"
    }
}

// ------------------------------------------------------------ requests

fn id32() -> u32 {
    rand::random()
}

/// A new request: the root of everything done for it (a UI task, a background
/// loop). `client` is `ui`, `engine` or `system`. The shell prints it as
/// `RQxxxxxxxx <client>` on every line logged inside it.
pub fn request(client: &str) -> tracing::Span {
    tracing::span!(target: "call", parent: None, Level::INFO, "request", rq = id32(), client = client)
}

/// `tokio::spawn` for a background loop of the engine: one `engine` request.
pub fn spawn_engine<F>(fut: F) -> tokio::task::JoinHandle<F::Output>
where
    F: Future + Send + 'static,
    F::Output: Send + 'static,
{
    tokio::spawn(tracing::Instrument::instrument(fut, request("engine")))
}

/// A failure worth a warning: `what` and the error's kind, never its text. Without
/// a subscriber (Android, tests) the same line goes to stderr, so that platform
/// still hears of it.
pub fn failure(what: &str, e: &Error) {
    tracing::warn!("{what}: {}", err_kind(e));
    if !tracing::dispatcher::has_been_set() {
        eprintln!("{what}: {}", err_kind(e));
    }
}

// -------------------------------------------------------- hub (wire) lines

/// `/api/messages/abc?x=1` → `/api/messages/…`: the endpoint, never an id or
/// a query (those carry addresses, message ids and offsets).
pub fn wire_path(path: &str) -> String {
    let path = path.split(['?', '#']).next().unwrap_or("");
    let mut parts = path.split('/').filter(|p| !p.is_empty());
    let mut out = String::new();
    for _ in 0..2 {
        match parts.next() {
            Some(p) => {
                out.push('/');
                out.extend(p.chars().take(24));
            }
            None => return if out.is_empty() { "/".into() } else { out },
        }
    }
    if parts.next().is_some() {
        out.push_str("/…");
    }
    out
}

/// A hub answer slower than this is written even when verbose logging is off
/// (long polls excepted: the hub parks them on purpose).
pub const SLOW: Duration = Duration::from_secs(5);

/// One call to a hub, for the REQUEST and RESPONSE lines (Orgtree's `wire`).
pub struct Wire {
    hub: String,
    method: &'static str,
    path: String,
    start: Instant,
    long: bool,
}

impl Wire {
    /// `hub` is the hub's URL (only its tag is written); `path` may carry ids.
    pub fn start(hub: &str, method: &'static str, path: &str) -> Wire {
        let path = wire_path(path);
        let long = path == "/api/poll" || path == "/api/sync";
        let hub = tag("hub", hub);
        if verbose() {
            tracing::info!(target: "wire", "REQUEST  {hub} {method} {path}");
        }
        Wire { hub, method, path, start: Instant::now(), long }
    }

    fn ms(&self) -> f64 {
        self.start.elapsed().as_secs_f64() * 1000.0
    }

    /// The hub answered with `status`.
    pub fn status(self, status: u16) {
        let ms = self.ms();
        let line = format!("RESPONSE {} {} {} {status} [{ms:.3} ms]", self.hub, self.method, self.path);
        if status >= 500 {
            tracing::error!(target: "wire", "{line}");
        } else if status >= 400 {
            tracing::warn!(target: "wire", "{line}");
        } else if verbose() || (!self.long && self.start.elapsed() >= SLOW) {
            tracing::info!(target: "wire", "{line}");
        }
    }

    /// An answer the caller expected and handles (a delete of what the hub no
    /// longer has): written only while verbose.
    pub fn expected(self, status: u16) {
        if verbose() {
            let ms = self.ms();
            tracing::info!(target: "wire", "RESPONSE {} {} {} {status} [{ms:.3} ms]", self.hub, self.method, self.path);
        }
    }

    /// The call failed before or while reading the answer.
    pub fn failed(self, e: &Error) {
        let ms = self.ms();
        tracing::warn!(target: "wire", "RESPONSE {} {} {} !! {} [{ms:.3} ms]", self.hub, self.method, self.path, err_kind(e));
    }

    /// The hub answered with `status` and `r` is what the caller made of the
    /// answer (a body that does not parse is a failure under a good status).
    pub fn answered<T>(self, status: u16, r: &Result<T>) {
        match r {
            Err(e) if status < 400 => {
                let ms = self.ms();
                tracing::warn!(target: "wire", "RESPONSE {} {} {} {status} !! {} [{ms:.3} ms]", self.hub, self.method, self.path, err_kind(e));
            }
            _ => self.status(status),
        }
    }
}

// ------------------------------------------------------ traced operations

/// Run `fut` as a traced operation (Orgtree's `#[logged]`, without arguments
/// or return values: only the `shape` the caller chooses, such as tags and
/// counts; it is built only while verbose logging is on). Verbose: a call
/// line, then a return line with the elapsed time. A failure is written either
/// way, as a warning with its kind.
pub async fn traced<T, F>(path: &'static str, shape: impl FnOnce() -> String, fut: F) -> Result<T>
where
    F: Future<Output = Result<T>>,
{
    traced_with(path, shape, fut, |_| String::new()).await
}

/// `traced`, with a `summary` of the result for the return line (a count, a
/// state name: never content).
pub async fn traced_with<T, F>(path: &'static str, shape: impl FnOnce() -> String, fut: F, summary: impl FnOnce(&T) -> String) -> Result<T>
where
    F: Future<Output = Result<T>>,
{
    let on = verbose();
    let span = if on {
        tracing::span!(target: "call", Level::INFO, "frame", ex = id32(), m = path)
    } else {
        tracing::Span::none()
    };
    let started = Instant::now();
    if on {
        let _g = span.enter();
        tracing::event!(target: "call", Level::INFO, phase = "call", "{path}({})", shape());
    }
    let mut end = EndGuard { span: span.clone(), path, started, done: false };
    let r = tracing::Instrument::instrument(fut, span.clone()).await;
    end.done = true;
    let ms = started.elapsed().as_secs_f64() * 1000.0;
    let _g = span.enter();
    match &r {
        Ok(v) if on => {
            let s = summary(v);
            let sep = if s.is_empty() { "" } else { " " };
            tracing::event!(target: "call", Level::INFO, phase = "ret", "{path}(...) [{ms:.3} ms] -> ok{sep}{s}")
        }
        Ok(_) => {}
        Err(Error::Cancelled) if on => {
            tracing::event!(target: "call", Level::INFO, phase = "ret", "{path}(...) [{ms:.3} ms] -> cancelled")
        }
        Err(Error::Cancelled) => {}
        Err(e) => {
            tracing::event!(target: "call", Level::WARN, phase = "ret", "{path}(...) [{ms:.3} ms] !! {}", err_kind(e))
        }
    }
    r
}

/// Written when a traced operation's future is dropped before it finished (its
/// task was cancelled): the return line the operation would have written.
struct EndGuard {
    span: tracing::Span,
    path: &'static str,
    started: Instant,
    done: bool,
}

impl Drop for EndGuard {
    fn drop(&mut self) {
        if self.done || !verbose() {
            return;
        }
        let ms = self.started.elapsed().as_secs_f64() * 1000.0;
        let _g = self.span.enter();
        tracing::event!(target: "call", Level::INFO, phase = "ret", "{}(...) [{ms:.3} ms] -> dropped", self.path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tag_is_stable_per_value_and_kind_and_does_not_contain_the_value() {
        let a = tag("peer", "maya.lin_2.0a1b2c");
        assert_eq!(a, tag("peer", "maya.lin_2.0a1b2c"));
        assert_ne!(a, tag("peer", "maya.lin_2.0a1b2d"));
        assert_ne!(a, tag("hub", "maya.lin_2.0a1b2c"));
        assert!(a.starts_with("peer:") && a.len() == "peer:".len() + 8, "{a}");
        assert!(!a.contains("maya"), "{a}");
    }

    #[test]
    fn a_wire_path_never_carries_an_id_or_a_query() {
        assert_eq!(wire_path("/api/poll?wait=25"), "/api/poll");
        assert_eq!(wire_path("/api/messages/abc123/body"), "/api/messages/…");
        assert_eq!(wire_path("/api/conversations/maya.lin_2.0a1b2c"), "/api/conversations/…");
        assert_eq!(wire_path("/api/uploads"), "/api/uploads");
        assert_eq!(wire_path("/api/uploads/u1?offset=9#frag"), "/api/uploads/…");
        assert_eq!(wire_path("/healthz"), "/healthz");
        assert_eq!(wire_path("//api//x//y"), "/api/x/…");
        assert_eq!(wire_path(""), "/");
        assert!(wire_path(&format!("/{}/b", "ä".repeat(60))).chars().count() < 40);
    }

    /// Content that must never come out of `err_kind`: short secrets, long
    /// tokens, multiline text, URLs with credentials and queries, hosts with
    /// ports, Windows and Unix paths, addresses, file names.
    const POISON: &str = "pw=hunter2 key=ab12\nsecond line: ghp_0123456789abcdef0123456789abcdef0123\n\
        https://user:pass@hub.office.lan:7370/api/poll?token=s3cr3t&to=maya.lin_2.0a1b2c #frag\n\
        http://[::1]:7371/x ftp://h/p @net:maya.lin_2.0a1b2c 100.64.1.2:7370 hub.lan:7370 star-hub \
        C:\\Users\\someone\\AppData\\x.db \\\\server\\share\\f.txt /home/someone/.ssh/id_rsa ~/notes.txt \
        ./relative/secret.txt file:///etc/passwd tailnet-name.ts.net budget-2026.xlsx";

    #[test]
    fn an_error_kind_is_a_name_from_a_fixed_list_never_the_errors_own_text() {
        let errors = [
            Error::Unreachable(POISON.into()),
            Error::Hub { status: 403, detail: POISON.into() },
            Error::NotAHub(POISON.into()),
            Error::Invalid(POISON.into()),
            Error::Store(POISON.into()),
            Error::Io(std::io::Error::new(std::io::ErrorKind::NotFound, POISON)),
            Error::SaveDownload(std::io::Error::new(std::io::ErrorKind::PermissionDenied, POISON)),
            Error::BodyTooLarge(9),
            Error::LocalBusy,
            Error::FilePaused,
            Error::Cancelled,
        ];
        for e in errors {
            let k = err_kind(&e);
            assert!(k.len() < 40, "{k}");
            assert!(k.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '-' | ':' | '(' | ')' | ' ')), "{k}");
            for leak in ["hunter2", "ab12", "ghp_", "s3cr3t", "maya", "hub.", "lan", "someone", "ssh", "notes", "secret", "passwd", "budget", "star", "tailnet", "7370", "pass"] {
                assert!(!k.contains(leak), "{leak} in {k} from {e:?}");
            }
        }
        assert_eq!(err_kind(&Error::Hub { status: 403, detail: POISON.into() }), "hub-403");
        assert_eq!(err_kind(&Error::Io(std::io::Error::from(std::io::ErrorKind::NotFound))), "io-notfound");
        assert_eq!(err_kind(&Error::Io(std::io::Error::from_raw_os_error(2))), "io-notfound(os 2)");
    }

    #[test]
    fn the_network_cause_is_found_by_phrase_and_defaults_to_other() {
        let k = |m: &str| err_kind(&Error::Unreachable(m.into()));
        assert_eq!(k("error sending request: operation timed out"), "unreachable:timeout");
        assert_eq!(k("dns error: failed to lookup address information"), "unreachable:dns");
        assert_eq!(k("tcp connect error: Connection refused (os error 10061)"), "unreachable:refused");
        assert_eq!(k("invalid peer certificate: UnknownIssuer"), "unreachable:tls");
        assert_eq!(k("connection reset by peer"), "unreachable:reset");
        assert_eq!(k("something nobody has seen"), "unreachable:other");
        assert_eq!(k(POISON).split(':').count(), 2, "one cause word, nothing else");
    }
}
