//! The Hubchat log (docs/logging.md): Orgtree's engine log (rust-engine
//! `trace.rs`, decisions 34-36), adapted to Hubchat's components. One file per
//! start, every line in the same shape:
//!
//! ```text
//! INFO     [2026-10-12 01:36:53.394512] T42 RQ1a2b3c4d ui EX5e6f7a8b engine.send(attachments=1, bytes=10240)
//! INFO     [2026-10-12 01:36:53.406871] T42 RQ1a2b3c4d ui EX5e6f7a8b engine.send(...) [12.359 ms] -> ok
//! ```
//!
//! `T<id>` is the emitting Tokio task (`T-` outside one), `RQ<id> <client>` the
//! request it works for (`ui`, `engine` or `system`), `EX<id>` the traced
//! operation (see `hubchat_core::diag::traced`). The file is
//! `<app data>/diagnostics/logs/<start time>.log`, rolled over (gzip) daily or
//! at `MAX_FILE_BYTES`, and kept 30 days within a total budget of
//! `MAX_TOTAL_BYTES`. Crash reports are another folder (`crash-reports`) and
//! another owner (crash.rs); this file never writes there.
//!
//! What may be logged is decided at the call sites and by
//! `hubchat_core::diag` (tags instead of addresses, error kinds instead of
//! error text). This layer is a second net with known holes (a short secret in
//! free text is not recognisable), not a reason to pass content: fields named
//! like a secret print as `*****`, long token-like strings print as `<token>`, a
//! line is cut at 8 KB and an event at 40 lines. A dependency's own warning or
//! error (it arrives as a tracing event or through the `log` bridge) keeps only
//! its level and source; its text is free text we did not write and is never kept.
//! When the diagnostics folder cannot be written the log goes to a folder of its
//! own in the temporary folder (same file names, same pruning) and the Developer
//! page says so; with no folder at all nothing is kept and the page says that.

use std::borrow::Cow;
use std::fmt::Write as _;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, SystemTime};

use hubchat_core::diag;
use serde::Serialize;
use tracing::field::{Field, Visit};
use tracing::span::{Attributes, Id};
use tracing::{Event, Level, Subscriber};
use tracing_log::NormalizeEvent;
use tracing_subscriber::layer::{Context, Layer};
use tracing_subscriber::registry::LookupSpan;

/// A line longer than this is cut (Orgtree decision 36).
pub const LINE_CAP: usize = 8 * 1024;
/// Room kept for the line's prefix.
const PREFIX_ROOM: usize = 256;
const BUDGET: usize = LINE_CAP - PREFIX_ROOM;
/// An event of more lines than this is cut: the log stays bounded whatever a
/// caller formats into one message.
pub const MAX_EVENT_LINES: usize = 40;
/// Console mirror (development builds): lines cut to this, the file keeps them whole.
pub const CONSOLE_CAP: usize = 500;
pub const RETENTION_DAYS: u64 = 30;
/// Roll the file over (gzip) past this size, or daily. Orgtree rolls at 256 MiB;
/// a chat client has no use for logs that large.
pub const MAX_FILE_BYTES: u64 = 8 * 1024 * 1024;
/// All of this log's files together stay under this: past it the oldest
/// archives go first, whatever their age (Orgtree bounds by age alone).
pub const MAX_TOTAL_BYTES: u64 = 64 * 1024 * 1024;
/// Events waiting for the writer thread; more are dropped (lossy), never grown.
const QUEUE_LINES: usize = 16_384;

const VERBOSE_FILE: &str = "verbose-logging";
const SALT_FILE: &str = "log-salt";

// ------------------------------------------------------------- verbose

/// `HUBCHAT_LOG_VERBOSE=0|1` fixes verbose logging for this run (the Developer
/// setting is then ignored).
pub fn pinned() -> Option<bool> {
    parse_pin(std::env::var("HUBCHAT_LOG_VERBOSE").ok().as_deref())
}

fn parse_pin(v: Option<&str>) -> Option<bool> {
    match v {
        Some("0") | Some("false") | Some("off") => Some(false),
        Some("1") | Some("true") | Some("on") => Some(true),
        _ => None,
    }
}

/// The stored Developer setting: `<app data>/verbose-logging` holds `on` or
/// `off`. A device-level preference, so it lives outside the identity store.
fn read_setting(app_data: &Path) -> Option<bool> {
    match std::fs::read_to_string(app_data.join(VERBOSE_FILE)).ok()?.trim() {
        "on" => Some(true),
        "off" => Some(false),
        _ => None,
    }
}

fn write_setting(app_data: &Path, on: bool) -> std::io::Result<()> {
    std::fs::create_dir_all(app_data)?;
    let tmp = app_data.join(format!("{VERBOSE_FILE}.tmp"));
    std::fs::write(&tmp, if on { "on" } else { "off" })?;
    std::fs::rename(&tmp, app_data.join(VERBOSE_FILE))
}

/// The environment pin, else the stored setting, else the build's default.
fn effective(app_data: &Path) -> bool {
    pinned().or_else(|| read_setting(app_data)).unwrap_or(diag::dev_build())
}

// ------------------------------------------------------------- text

pub fn kb(n: usize) -> String {
    format!("{:.1} kb", n as f64 / 1024.0)
}

/// The longest prefix of `s` that is at most `n` bytes, on a char boundary.
pub fn cut(s: &str, n: usize) -> &str {
    if s.len() <= n {
        return s;
    }
    let mut i = n;
    while !s.is_char_boundary(i) {
        i -= 1;
    }
    &s[..i]
}

/// Field names whose values never reach the log (Orgtree's list, trimmed to
/// what Hubchat has).
fn masked(name: &str) -> bool {
    const KEYS: &[&str] = &[
        "secret", "password", "token", "apikey", "api_key", "access_token", "refresh_token", "auth", "auth_token",
        "authorization", "cookie", "key", "secret_key", "private_key", "credentials", "words", "recovery", "p256dh",
        "endpoint", "body", "text", "preview", "about", "address", "addr", "url", "error", "err", "detail", "reason", "why",
    ];
    let n = name.to_ascii_lowercase();
    KEYS.contains(&n.as_str())
}

/// Long token-like strings (a key, a bearer value) print as `<token>`.
fn mask_tokens(s: &str) -> Cow<'_, str> {
    let b = s.as_bytes();
    let tok = |c: u8| c.is_ascii_alphanumeric() || matches!(c, b'+' | b'/' | b'_' | b'=' | b'-');
    let mut out: Option<String> = None;
    let (mut i, mut copied) = (0, 0);
    while i < b.len() {
        if !tok(b[i]) {
            i += 1;
            continue;
        }
        let start = i;
        let (mut digit, mut alpha) = (false, false);
        while i < b.len() && tok(b[i]) {
            digit |= b[i].is_ascii_digit();
            alpha |= b[i].is_ascii_alphabetic();
            i += 1;
        }
        if i - start >= 32 && digit && alpha {
            let o = out.get_or_insert_with(|| String::with_capacity(s.len()));
            o.push_str(&s[copied..start]);
            o.push_str("<token>");
            copied = i;
        }
    }
    match out {
        Some(mut o) => {
            o.push_str(&s[copied..]);
            Cow::Owned(o)
        }
        None => Cow::Borrowed(s),
    }
}

/// One output line's message: tokens masked, then cut to the budget.
fn fit(line: &str) -> String {
    let line = mask_tokens(line);
    if line.len() <= BUDGET {
        return line.into_owned();
    }
    let head = cut(&line, BUDGET - 40);
    format!("{head}…[line cut: {} more]", kb(line.len() - head.len()))
}

fn timestamp() -> String {
    chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.6f").to_string()
}

/// `hubchat_core::engine` → `engine`, `hubchat_lib::trace` → `trace` (as Orgtree's `dotted`)
fn dotted(target: &str) -> String {
    let t = target.strip_prefix("hubchat_core::").or_else(|| target.strip_prefix("hubchat_lib::")).unwrap_or(target);
    t.replace("::", ".")
}

/// What a dependency's warning or error leaves in the log in place of its text.
const WITHHELD: &str = "[dependency message withheld]";

/// Events from Hubchat's own code (module paths of the two crates and the
/// fixed targets the helpers use). Anything else is a dependency, whether it
/// reached the subscriber as a tracing event or through the `log` bridge.
fn ours_target(target: &str) -> bool {
    let root = target.split("::").next().unwrap_or(target);
    matches!(root, "call" | "wire" | "cmd" | "keylog" | "hubchat" | "hubchat_lib" | "hubchat_core")
}

fn level_name(l: &Level) -> &'static str {
    match *l {
        Level::TRACE => "TRACE",
        Level::DEBUG => "DEBUG",
        Level::INFO => "INFO",
        Level::WARN => "WARNING",
        Level::ERROR => "ERROR",
    }
}

// ------------------------------------------------------------ the layer

/// Where finished lines go: the file's writer thread, the console's, or a test's buffer.
pub trait Sink: Send + Sync + 'static {
    fn put(&self, bytes: &[u8]);
}

struct Nb(tracing_appender::non_blocking::NonBlocking);

impl Sink for Nb {
    fn put(&self, bytes: &[u8]) {
        let mut w = self.0.clone();
        let _ = w.write_all(bytes);
    }
}

struct FrameInfo {
    ex: u32,
    path: String,
    caller: Option<(u32, String)>,
}

struct ReqInfo {
    rq: u32,
    client: String,
}

#[derive(Default)]
struct SpanGrab {
    ex: Option<u64>,
    m: Option<String>,
    rq: Option<u64>,
    client: Option<String>,
}

impl Visit for SpanGrab {
    fn record_u64(&mut self, f: &Field, v: u64) {
        match f.name() {
            "ex" => self.ex = Some(v),
            "rq" => self.rq = Some(v),
            _ => {}
        }
    }
    fn record_str(&mut self, f: &Field, v: &str) {
        match f.name() {
            "m" => self.m = Some(v.to_string()),
            "client" => self.client = Some(v.to_string()),
            _ => {}
        }
    }
    fn record_debug(&mut self, f: &Field, v: &dyn std::fmt::Debug) {
        let s = format!("{v:?}").trim_matches('"').to_string();
        match f.name() {
            "m" => self.m = Some(s),
            "client" => self.client = Some(s),
            _ => {}
        }
    }
}

#[derive(Default)]
struct EventGrab {
    message: String,
    phase: Option<String>,
    fields: String,
}

impl EventGrab {
    fn field(&mut self, name: &str, shown: &dyn std::fmt::Display) {
        if name.starts_with("log.") {
            return; // tracing-log's bookkeeping
        }
        if masked(name) {
            let _ = write!(self.fields, " {name}=*****");
        } else {
            let _ = write!(self.fields, " {name}={shown}");
        }
    }
}

struct DebugAsDisplay<'a>(&'a dyn std::fmt::Debug);

impl std::fmt::Display for DebugAsDisplay<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self.0)
    }
}

impl Visit for EventGrab {
    fn record_str(&mut self, f: &Field, v: &str) {
        match f.name() {
            "message" => self.message.push_str(v),
            "phase" => self.phase = Some(v.to_string()),
            name => self.field(name, &v),
        }
    }
    fn record_debug(&mut self, f: &Field, v: &dyn std::fmt::Debug) {
        match f.name() {
            "message" => {
                let _ = write!(self.message, "{v:?}");
            }
            "phase" => self.phase = Some(format!("{v:?}").trim_matches('"').to_string()),
            name => self.field(name, &DebugAsDisplay(v)),
        }
    }
}

pub struct HubchatLayer {
    file: Arc<dyn Sink>,
    /// A development run's mirror on stderr, through its own writer thread.
    console: Option<Arc<dyn Sink>>,
}

impl HubchatLayer {
    pub fn new(file: Arc<dyn Sink>, console: Option<Arc<dyn Sink>>) -> HubchatLayer {
        HubchatLayer { file, console }
    }
}

/// The emitting Tokio task, read at emission (`T-` outside a task).
fn task_prefix(out: &mut String) {
    match tokio::task::try_id() {
        Some(id) => {
            let _ = write!(out, " T{id}");
        }
        None => out.push_str(" T-"),
    }
}

impl<S> Layer<S> for HubchatLayer
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    fn on_new_span(&self, attrs: &Attributes<'_>, id: &Id, ctx: Context<'_, S>) {
        let mut g = SpanGrab::default();
        attrs.record(&mut g);
        let Some(span) = ctx.span(id) else { return };
        if let (Some(ex), Some(m)) = (g.ex, g.m) {
            let caller = span
                .parent()
                .and_then(|p| p.scope().find_map(|s| s.extensions().get::<FrameInfo>().map(|f| (f.ex, f.path.clone()))));
            span.extensions_mut().insert(FrameInfo { ex: ex as u32, path: m, caller });
        }
        if let Some(rq) = g.rq {
            let client = g.client.unwrap_or_else(|| "system".into());
            span.extensions_mut().insert(ReqInfo { rq: rq as u32, client });
        }
    }

    fn on_event(&self, event: &Event<'_>, ctx: Context<'_, S>) {
        let mut rq: Option<(u32, String)> = None;
        let mut frame: Option<(u32, Option<(u32, String)>)> = None;
        if let Some(scope) = ctx.event_scope(event) {
            for s in scope {
                let ext = s.extensions();
                if frame.is_none() {
                    if let Some(f) = ext.get::<FrameInfo>() {
                        frame = Some((f.ex, f.caller.clone()));
                    }
                }
                if rq.is_none() {
                    if let Some(r) = ext.get::<ReqInfo>() {
                        rq = Some((r.rq, r.client.clone()));
                    }
                }
                if frame.is_some() && rq.is_some() {
                    break;
                }
            }
        }
        // an event from the `log` crate (the plugins) reports its own target and level
        let normalized = event.normalized_metadata();
        let meta = normalized.as_ref().unwrap_or_else(|| event.metadata());
        // a dependency's message is free text we did not write (URLs, hosts, paths,
        // header values): only its level and source are kept, and only warnings and errors
        let foreign = !ours_target(meta.target());
        if foreign && *meta.level() > Level::WARN {
            return;
        }
        let mut g = EventGrab::default();
        if !foreign {
            event.record(&mut g);
        }
        let ours = meta.target() == "call";
        let bare = ours || matches!(meta.target(), "wire" | "cmd");
        let mut msg = String::with_capacity(g.message.len() + 64);
        if g.phase.as_deref() == Some("call") {
            if let Some((_, Some((cex, cpath)))) = &frame {
                let _ = write!(msg, "{cpath}@EX{cex:08x} ");
            }
        }
        if !bare {
            msg.push_str(&dotted(meta.target()));
            msg.push_str(": ");
        }
        if foreign {
            msg.push_str(WITHHELD);
        }
        msg.push_str(&g.message);
        msg.push_str(&g.fields);
        let mut prefix = format!("{:<8} [{}]", level_name(meta.level()), timestamp());
        task_prefix(&mut prefix);
        if let Some((rq, client)) = &rq {
            let _ = write!(prefix, " RQ{rq:08x} {client}");
        }
        if let Some((ex, _)) = &frame {
            let _ = write!(prefix, " EX{ex:08x}");
        }
        let mut out = String::with_capacity(prefix.len() + msg.len() + 8);
        let mut lines = 0;
        for line in msg.split('\n') {
            lines += 1;
            if lines > MAX_EVENT_LINES {
                let more = msg.split('\n').count() - MAX_EVENT_LINES;
                let _ = writeln!(out, "{prefix} […{more} more lines omitted]");
                break;
            }
            out.push_str(&prefix);
            out.push(' ');
            out.push_str(&fit(line.trim_end_matches('\r')));
            out.push('\n');
        }
        self.file.put(out.as_bytes());
        if let Some(console) = &self.console {
            if !ours && *meta.level() <= Level::INFO {
                let mut short = String::with_capacity(out.len().min(CONSOLE_CAP * 4));
                for line in out.lines() {
                    short.push_str(cut(line, CONSOLE_CAP));
                    short.push('\n');
                }
                console.put(short.as_bytes());
            }
        }
    }
}

// ------------------------------------------------------------ the files

/// `YYYY-MM-DD_HH-MM-SS.log` (the start time) and the `.log.<32 hex>.gz`
/// archives rollover makes: the files this log owns.
fn owned(name: &str) -> bool {
    let b = name.as_bytes();
    let stamp = b.len() >= 23
        && b[..19].iter().enumerate().all(|(i, c)| match i {
            4 | 7 | 13 | 16 => *c == b'-',
            10 => *c == b'_',
            _ => c.is_ascii_digit(),
        })
        && &b[19..23] == b".log";
    if !stamp {
        return false;
    }
    // bytes, not str slices: a foreign file name in this folder may put a
    // multi-byte character across these offsets
    let rest = &b[23..];
    rest.is_empty() || (rest.len() == 1 + 32 + 3 && rest[0] == b'.' && rest.ends_with(b".gz") && rest[1..33].iter().all(u8::is_ascii_hexdigit))
}

/// Remove owned logs last written before the retention window, then the
/// oldest ones while the folder is over its budget. The active file stays.
pub fn prune(dir: &Path, active: Option<&Path>) {
    prune_with(dir, active, SystemTime::now(), Duration::from_secs(RETENTION_DAYS * 24 * 3600), MAX_TOTAL_BYTES);
}

fn prune_with(dir: &Path, active: Option<&Path>, now: SystemTime, keep_for: Duration, max_total: u64) {
    let cutoff = now.checked_sub(keep_for).unwrap_or(SystemTime::UNIX_EPOCH);
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    let mut kept: Vec<(SystemTime, u64, PathBuf)> = Vec::new();
    let mut total = 0u64;
    for e in entries.flatten() {
        let p = e.path();
        let name = e.file_name().to_string_lossy().to_string();
        if !owned(&name) {
            continue;
        }
        let Ok(md) = e.metadata() else { continue };
        if !md.is_file() {
            continue;
        }
        if Some(p.as_path()) == active {
            total += md.len();
            continue;
        }
        let modified = md.modified().unwrap_or(now);
        if modified < cutoff {
            let _ = std::fs::remove_file(&p);
        } else {
            total += md.len();
            kept.push((modified, md.len(), p));
        }
    }
    kept.sort();
    for (_, len, p) in kept {
        if total <= max_total {
            break;
        }
        if std::fs::remove_file(&p).is_ok() {
            total = total.saturating_sub(len);
        }
    }
}

/// The log file: rolled over daily or at `max` bytes (a gzip archive beside
/// it, then a fresh file under the same name); never by count.
pub struct LogFile {
    path: PathBuf,
    file: std::fs::File,
    size: u64,
    max: u64,
    roll_at: SystemTime,
}

impl LogFile {
    pub fn open(path: PathBuf) -> std::io::Result<LogFile> {
        LogFile::open_with(path, MAX_FILE_BYTES)
    }

    fn open_with(path: PathBuf, max: u64) -> std::io::Result<LogFile> {
        let file = std::fs::OpenOptions::new().create(true).append(true).open(&path)?;
        let size = file.metadata().map(|m| m.len()).unwrap_or(0);
        Ok(LogFile { path, file, size, max, roll_at: SystemTime::now() + Duration::from_secs(24 * 3600) })
    }

    fn roll(&mut self) {
        let _ = self.file.flush();
        let archive = self.path.with_file_name(format!(
            "{}.{:032x}.gz",
            self.path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default(),
            rand::random::<u128>()
        ));
        let tmp = archive.with_extension("gz.tmp");
        let ok = (|| -> std::io::Result<()> {
            let mut src = std::fs::File::open(&self.path)?;
            let dst = std::fs::File::create(&tmp)?;
            let mut enc = flate2::write::GzEncoder::new(dst, flate2::Compression::fast());
            std::io::copy(&mut src, &mut enc)?;
            enc.finish()?;
            std::fs::rename(&tmp, &archive)?;
            Ok(())
        })();
        let _ = std::fs::remove_file(&tmp);
        if ok.is_ok() {
            // a failed archive keeps the original: never destroy the log we meant to keep.
            // The fresh file is opened for append like the first one: `emergency` also
            // appends, and a plain write handle would overwrite its lines.
            let fresh = std::fs::File::create(&self.path)
                .and_then(|_| std::fs::OpenOptions::new().create(true).append(true).open(&self.path));
            if let Ok(f) = fresh {
                self.file = f;
                self.size = 0;
            }
        }
        self.roll_at = SystemTime::now() + Duration::from_secs(24 * 3600);
        if let Some(dir) = self.path.parent() {
            prune(dir, Some(&self.path));
        }
    }
}

impl std::io::Write for LogFile {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        if self.size > 0 && (self.size + buf.len() as u64 > self.max || SystemTime::now() >= self.roll_at) {
            self.roll();
        }
        self.file.write_all(buf)?;
        self.size += buf.len() as u64;
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.file.flush()
    }
}

// ----------------------------------------------------------- start / stop

static STARTED: AtomicBool = AtomicBool::new(false);
static APP_DATA: OnceLock<PathBuf> = OnceLock::new();
/// Where the running log really is (the fallback folder when the usual one
/// could not be written). Unset until a file is open and our subscriber is
/// installed.
static LOG_DIR: OnceLock<PathBuf> = OnceLock::new();
static ACTIVE: OnceLock<PathBuf> = OnceLock::new();
static FALLBACK: OnceLock<bool> = OnceLock::new();
static GUARDS: Mutex<Vec<tracing_appender::non_blocking::WorkerGuard>> = Mutex::new(Vec::new());

/// The device's salt for identifier tags: kept beside the logs so a tag means
/// the same thing in every file, created at the first start.
fn load_salt(diagnostics: &Path) -> [u8; 16] {
    let path = diagnostics.join(SALT_FILE);
    if let Some(s) = std::fs::read_to_string(&path).ok().and_then(|t| hex::decode(t.trim()).ok()) {
        if let Ok(salt) = <[u8; 16]>::try_from(s.as_slice()) {
            return salt;
        }
    }
    let salt: [u8; 16] = rand::random();
    let _ = std::fs::write(&path, hex::encode(salt));
    salt
}

/// Where logs go when the diagnostics folder cannot be written: a folder of
/// their own in the temporary folder, named and pruned like the usual one
/// (`owned`), so its files age out and `Open logs folder` shows only them.
fn fallback_dir() -> PathBuf {
    std::env::temp_dir().join("hubchat-logs")
}

fn make_dir(dir: &Path, private: bool) -> std::io::Result<()> {
    let mut b = std::fs::DirBuilder::new();
    b.recursive(true);
    #[cfg(unix)]
    if private {
        use std::os::unix::fs::DirBuilderExt;
        b.mode(0o700);
    }
    #[cfg(not(unix))]
    let _ = private;
    b.create(dir)
}

/// An open log file and where it really is.
struct Opened {
    file: LogFile,
    path: PathBuf,
    dir: PathBuf,
    fallback: bool,
}

/// The first folder that takes a new `<stamp>.log`: `primary`, else `fallback`,
/// else none (the app runs without a log). Each tried folder is pruned first;
/// the fallback folder is also pruned when it is not needed, so what an earlier
/// failed start left there ages out.
fn open_log(primary: &Path, fallback: &Path, stamp: &str) -> Option<Opened> {
    for (dir, is_fallback) in [(primary, false), (fallback, true)] {
        if make_dir(dir, is_fallback).is_err() {
            continue;
        }
        let path = dir.join(format!("{stamp}.log"));
        prune(dir, Some(&path));
        if let Ok(file) = LogFile::open(path.clone()) {
            if !is_fallback {
                prune(fallback, None);
            }
            return Some(Opened { file, path, dir: dir.to_path_buf(), fallback: is_fallback });
        }
    }
    None
}

/// What `begin` hands back: a log that is open and installed.
struct Begun {
    path: PathBuf,
    dir: PathBuf,
    fallback: bool,
    guards: Vec<tracing_appender::non_blocking::WorkerGuard>,
}

/// Open the log file (the usual folder, else the fallback) and give the layer
/// that writes it to `install`, which answers whether it took. `None` means no
/// log is running: no folder took a file, or `install` refused (another
/// subscriber owns the process); then nothing is kept and the empty file made
/// here is removed where the system lets it.
fn begin(
    logs_dir: &Path,
    fallback: &Path,
    install: impl FnOnce(HubchatLayer, tracing_subscriber::EnvFilter) -> bool,
) -> Option<Begun> {
    let stamp = chrono::Local::now().format("%Y-%m-%d_%H-%M-%S").to_string();
    let Some(Opened { file, path, dir, fallback: used_fallback }) = open_log(logs_dir, fallback, &stamp) else {
        eprintln!("hubchat: no folder takes a log file; running without one");
        return None;
    };
    let (w, guard) = tracing_appender::non_blocking::NonBlockingBuilder::default()
        .buffered_lines_limit(QUEUE_LINES)
        .lossy(true)
        .thread_name("hubchat-log")
        .finish(file);
    let file_sink: Arc<dyn Sink> = Arc::new(Nb(w));
    let mut guards = vec![guard];
    let console: Option<Arc<dyn Sink>> = if cfg!(debug_assertions) || std::env::var("HUBCHAT_LOG_CONSOLE").is_ok_and(|v| v == "1") {
        let (w, guard) = tracing_appender::non_blocking::NonBlockingBuilder::default()
            .lossy(true)
            .thread_name("hubchat-console")
            .finish(std::io::stderr());
        guards.push(guard);
        Some(Arc::new(Nb(w)))
    } else {
        None
    };
    let filter = tracing_subscriber::EnvFilter::try_from_env("HUBCHAT_LOG").unwrap_or_else(|_| {
        tracing_subscriber::EnvFilter::new("info,tao=warn,wry=warn,tauri=warn,reqwest=warn,hyper=warn,hyper_util=warn,h2=warn,rustls=warn")
    });
    if !install(HubchatLayer::new(file_sink, console), filter) {
        drop(guards);
        if std::fs::metadata(&path).is_ok_and(|m| m.len() == 0) {
            let _ = std::fs::remove_file(&path);
        }
        return None;
    }
    Some(Begun { path, dir, fallback: used_fallback, guards })
}

/// Start the log: `<logs dir>/<start time>.log`, else the same name in the
/// fallback folder. Never fails the app: with no writable folder, or when
/// another subscriber owns the process, nothing is kept, `Open logs folder`
/// says so and the Developer page reports the log as not running. Answers the
/// file's path.
#[cfg_attr(not(desktop), allow(dead_code))]
pub fn init(app_data: &Path, logs_dir: &Path) -> Option<PathBuf> {
    start(app_data, logs_dir, &fallback_dir())
}

fn start(app_data: &Path, logs_dir: &Path, fallback: &Path) -> Option<PathBuf> {
    use tracing_subscriber::prelude::*;
    if STARTED.swap(true, Ordering::SeqCst) {
        return ACTIVE.get().cloned();
    }
    let _ = APP_DATA.set(app_data.to_path_buf());
    if let Some(root) = logs_dir.parent() {
        let _ = std::fs::create_dir_all(root);
        diag::set_salt(load_salt(root));
    }
    let want = effective(app_data);
    let begun = begin(logs_dir, fallback, |layer, filter| tracing_subscriber::registry().with(layer.with_filter(filter)).try_init().is_ok());
    let Some(Begun { path, dir, fallback: used_fallback, guards }) = begun else {
        diag::set_verbose(false);
        return None;
    };
    diag::set_verbose(want);
    // the plugins talk through the `log` crate: take their messages in (info and up).
    // `try_init` may have installed the bridge already; the level is set either way.
    let _ = tracing_log::LogTracer::builder().with_max_level(log::LevelFilter::Info).init();
    log::set_max_level(log::LevelFilter::Info);
    // the panic hook (crash.rs) leaves one pointer line in this file
    #[cfg(desktop)]
    crate::crash::note_panics_in(emergency);
    // Restart Hubchat skips the Exit event: flush the file first
    #[cfg(desktop)]
    crate::tray::before_restart(shutdown);
    // from here the log is running, in the folder it really got
    let _ = LOG_DIR.set(dir.clone());
    let _ = FALLBACK.set(used_fallback);
    let _ = ACTIVE.set(path.clone());
    GUARDS.lock().unwrap_or_else(|p| p.into_inner()).extend(guards);
    tracing::info!(
        pid = std::process::id(),
        version = env!("CARGO_PKG_VERSION"),
        build = if diag::dev_build() { "dev" } else { "release" },
        os = std::env::consts::OS,
        arch = std::env::consts::ARCH,
        file = %path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default(),
        "hubchat starting"
    );
    if used_fallback {
        tracing::warn!("the diagnostics folder could not be written: this log is in the temporary folder");
    }
    tracing::info!("verbose logging {}", if want { "on" } else { "off" });
    // a daily sweep for a process that is never restarted
    let active = path.clone();
    std::thread::Builder::new()
        .name("hubchat-log-retention".into())
        .spawn(move || loop {
            std::thread::sleep(Duration::from_secs(24 * 3600));
            prune(&dir, Some(&active));
        })
        .ok();
    Some(path)
}

/// Flush what is queued and stop the writer threads (the app is exiting).
pub fn shutdown() {
    tracing::info!("hubchat stopping");
    let guards: Vec<_> = std::mem::take(&mut *GUARDS.lock().unwrap_or_else(|p| p.into_inner()));
    drop(guards); // a guard flushes its queue when dropped
}

/// The text `emergency` appends.
fn emergency_text(line: &str) -> String {
    let flat: String = cut(line, 1024).chars().map(|c| if c == '\n' || c == '\r' { ' ' } else { c }).collect();
    format!("{:<8} [{}] T- {}\n", "ERROR", timestamp(), mask_tokens(&flat))
}

/// Write one line straight to the active log file, without the writer queue:
/// for the panic hook, which runs just before a release build aborts and would
/// lose a queued line. Never panics; does nothing before `init`. The caller
/// passes a pointer (a report's file name, a source line), never a payload.
#[allow(dead_code)] // the desktop panic hook (crash.rs) calls it; Android has none
pub fn emergency(line: &str) {
    let Some(path) = ACTIVE.get() else { return };
    let text = emergency_text(line);
    if let Ok(mut f) = std::fs::OpenOptions::new().append(true).open(path) {
        let _ = f.write_all(text.as_bytes());
        let _ = f.sync_data();
    }
}

// ------------------------------------------------------ the Tauri side

/// Wrap the app's command handler: each command is a `ui` request (sync
/// commands run inside its span), and verbose logging lists the command names
/// in order. Arguments are never logged (they carry messages and addresses).
pub fn commands<R, F>(inner: F) -> impl Fn(tauri::ipc::Invoke<R>) -> bool + Send + Sync + 'static
where
    R: tauri::Runtime,
    F: Fn(tauri::ipc::Invoke<R>) -> bool + Send + Sync + 'static,
{
    move |invoke| {
        let span = diag::request("ui");
        span.in_scope(|| {
            if diag::verbose() {
                tracing::info!(target: "cmd", "COMMAND {}", invoke.message.command());
            }
            inner(invoke)
        })
    }
}

/// What Settings › Developer shows.
#[derive(Serialize)]
pub struct LogInfo {
    /// the log is running (Android and failed starts: false)
    pub active: bool,
    pub verbose: bool,
    /// the build's own default (a development build is verbose)
    pub default_verbose: bool,
    /// `HUBCHAT_LOG_VERBOSE` fixes the switch for this run
    pub pinned: bool,
    /// the folder the running log really is in (empty when none is running)
    pub dir: String,
    /// the usual folder could not be written: the log is in a temporary folder
    pub fallback: bool,
    pub files: usize,
    pub bytes: u64,
    pub max_bytes: u64,
    pub retention_days: u64,
}

fn usage(dir: &Path) -> (usize, u64) {
    let Ok(entries) = std::fs::read_dir(dir) else { return (0, 0) };
    entries
        .flatten()
        .filter(|e| owned(&e.file_name().to_string_lossy()))
        .filter_map(|e| e.metadata().ok())
        .filter(|m| m.is_file())
        .fold((0, 0), |(n, b), m| (n + 1, b + m.len()))
}

fn info() -> LogInfo {
    info_of(LOG_DIR.get().map(PathBuf::as_path), ACTIVE.get().is_some(), FALLBACK.get().copied().unwrap_or(false))
}

/// What the page reports, from where the running log is (`dir` is None when
/// no log is running: then nothing is counted and nothing is opened).
fn info_of(dir: Option<&Path>, active: bool, fallback: bool) -> LogInfo {
    let (files, bytes) = dir.map_or((0, 0), usage);
    LogInfo {
        active,
        verbose: active && diag::verbose(),
        default_verbose: diag::dev_build(),
        pinned: pinned().is_some(),
        dir: dir.map(|d| d.to_string_lossy().into_owned()).unwrap_or_default(),
        fallback: active && fallback,
        files,
        bytes,
        max_bytes: MAX_TOTAL_BYTES,
        retention_days: RETENTION_DAYS,
    }
}

#[tauri::command]
pub fn hc_log_info() -> LogInfo {
    info()
}

/// Settings › Developer › Verbose logging: applied at once and kept for the next start.
#[tauri::command]
pub fn hc_set_verbose_logging(on: bool) -> Result<LogInfo, String> {
    if pinned().is_some() {
        return Err("HUBCHAT_LOG_VERBOSE fixes verbose logging for this run.".into());
    }
    if ACTIVE.get().is_none() {
        return Err("Logging is not running on this device.".into());
    }
    let app_data = APP_DATA.get().ok_or_else(|| "Logging is not running on this device.".to_string())?;
    write_setting(app_data, on).map_err(|e| format!("Could not save the setting: {e}"))?;
    if diag::set_verbose(on) != on {
        tracing::info!("verbose logging {}", if on { "on" } else { "off" });
    }
    Ok(info())
}

/// Settings › Developer › Open logs folder.
#[tauri::command]
pub fn hc_open_logs_folder(app: tauri::AppHandle) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;
    let dir = LOG_DIR.get().ok_or_else(|| "Logging is not running on this device.".to_string())?;
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    app.opener().open_path(dir.to_string_lossy(), None::<&str>).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tracing_subscriber::prelude::*;

    struct Mem(Mutex<Vec<u8>>);

    impl Sink for Mem {
        fn put(&self, bytes: &[u8]) {
            self.0.lock().unwrap().extend_from_slice(bytes);
        }
    }

    /// Run `f` under a subscriber that writes this layer's lines into memory.
    fn lines(f: impl FnOnce()) -> Vec<String> {
        let mem = Arc::new(Mem(Mutex::new(Vec::new())));
        let layer = HubchatLayer::new(mem.clone(), None);
        let sub = tracing_subscriber::registry().with(layer);
        tracing::subscriber::with_default(sub, f);
        let text = String::from_utf8(mem.0.lock().unwrap().clone()).unwrap();
        text.lines().map(str::to_string).collect()
    }

    fn temp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("hubchat-trace-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn a_line_has_orgtrees_shape_and_a_request_and_frame_when_inside_them() {
        let l = lines(|| {
            tracing::info!("plain");
            let rq = diag::request("ui");
            rq.in_scope(|| {
                tracing::warn!(target: "wire", "RESPONSE hub:ab12cd34 POST /api/send 200 [1.000 ms]");
                let ex = tracing::span!(target: "call", Level::INFO, "frame", ex = 0xabcu32, m = "engine.send");
                ex.in_scope(|| tracing::event!(target: "call", Level::INFO, phase = "call", "engine.send(bytes=3)"));
            });
        });
        assert_eq!(l.len(), 3, "{l:?}");
        let shape = |s: &str, level: &str| s.starts_with(&format!("{level:<8} [20")) && s.contains("] T- ");
        assert!(shape(&l[0], "INFO") && l[0].ends_with(" trace.tests: plain"), "{}", l[0]);
        assert!(shape(&l[1], "WARNING"), "{}", l[1]);
        assert!(l[1].contains(" ui ") && l[1].contains(" RQ") && l[1].ends_with(" RESPONSE hub:ab12cd34 POST /api/send 200 [1.000 ms]"), "{}", l[1]);
        assert!(l[2].contains(" EX00000abc ") && l[2].ends_with(" engine.send(bytes=3)"), "{}", l[2]);
    }

    #[test]
    fn a_multiline_message_is_split_into_prefixed_lines_and_bounded() {
        let long = (0..100).map(|i| format!("row {i}")).collect::<Vec<_>>().join("\n");
        let l = lines(|| tracing::info!("{long}"));
        assert_eq!(l.len(), MAX_EVENT_LINES + 1, "{}", l.len());
        assert!(l.iter().all(|x| x.starts_with("INFO     [")), "every line carries the prefix");
        assert!(l.last().unwrap().contains("60 more lines omitted"), "{}", l.last().unwrap());
    }

    #[test]
    fn a_line_over_the_cap_is_cut_and_says_how_much() {
        let l = lines(|| tracing::info!("{}", "word ".repeat(5000)));
        assert_eq!(l.len(), 1);
        assert!(l[0].len() < LINE_CAP, "{}", l[0].len());
        assert!(l[0].contains("…[line cut: "), "{}", &l[0][l[0].len() - 40..]);
    }

    #[test]
    fn secret_named_fields_and_long_tokens_never_print() {
        let l = lines(|| {
            tracing::info!(token = "hunter2", body = "hello there", count = 3, "x");
            tracing::info!("bearer 0123456789abcdef0123456789abcdef0123456789 end");
        });
        assert!(l[0].ends_with(" x token=***** body=***** count=3"), "{}", l[0]);
        assert!(!l[1].contains("0123456789abcdef"), "{}", l[1]);
        assert!(l[1].ends_with("bearer <token> end"), "{}", l[1]);
    }

    #[test]
    fn mask_tokens_leaves_ordinary_text_alone() {
        for s in ["", "plain words", "2026-10-12_01-36-53.log", "hub:0a1b2c3d", "EX0a1b2c3d", "abcdefghijklmnopqrstuvwxyzabcdefghij", "12345678901234567890123456789012345"] {
            assert_eq!(mask_tokens(s), s, "{s}");
        }
        assert_eq!(mask_tokens("a 3f786850e387550fdab836ed7e6dc881de23001b b"), "a <token> b");
        assert_eq!(mask_tokens("åäö 3f786850e387550fdab836ed7e6dc881de23001b ö"), "åäö <token> ö");
    }

    #[test]
    fn the_files_this_log_owns_are_recognised_and_nothing_else() {
        let archive = format!("2026-10-12_01-36-53.log.{}.gz", "0a".repeat(16));
        assert!(owned("2026-10-12_01-36-53.log") && owned(&archive));
        for other in ["identity-log.txt", "2026-10-12_01-36-53.txt", "2026-10-12_01-36-53.log.1", "2026-10-12_01-36-53.log.zz.gz", "log-salt", "é026-10-12_01-36-53.log", "2026-10-12_01-36-5é.log"] {
            assert!(!owned(other), "{other}");
        }
    }

    #[test]
    fn prune_removes_old_files_then_the_oldest_over_budget_and_keeps_the_active_and_foreign_files() {
        let d = temp("prune");
        let now = SystemTime::now();
        let make = |name: &str, age_days: u64, len: usize| {
            let p = d.join(name);
            std::fs::write(&p, vec![b'x'; len]).unwrap();
            let f = std::fs::OpenOptions::new().write(true).open(&p).unwrap();
            f.set_modified(now - Duration::from_secs(age_days * 24 * 3600)).unwrap();
            p
        };
        let old = make("2026-01-01_00-00-00.log", 40, 10);
        let a = make("2026-09-01_00-00-00.log", 20, 400);
        let b = make("2026-09-20_00-00-00.log", 5, 400);
        let active = make("2026-10-12_00-00-00.log", 0, 300);
        let foreign = make("identity-log.txt", 90, 10);
        prune_with(&d, Some(&active), now, Duration::from_secs(30 * 24 * 3600), 1000);
        assert!(!old.exists(), "older than the retention window");
        assert!(!a.exists(), "oldest over the budget (400+400+300 > 1000)");
        assert!(b.exists() && active.exists() && foreign.exists());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn the_file_rolls_over_to_a_readable_archive_and_goes_on_under_its_name() {
        use std::io::Read as _;
        let d = temp("roll");
        let path = d.join("2026-10-12_01-36-53.log");
        let mut f = LogFile::open_with(path.clone(), 100).unwrap();
        let first = "a".repeat(80) + "\n";
        f.write_all(first.as_bytes()).unwrap();
        f.write_all(b"second\n").unwrap(); // 81 + 7 < 100: same file
        f.write_all(("b".repeat(40) + "\n").as_bytes()).unwrap(); // over: rolls first
        f.flush().unwrap();
        let archives: Vec<_> = std::fs::read_dir(&d)
            .unwrap()
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().ends_with(".gz"))
            .collect();
        assert_eq!(archives.len(), 1);
        assert!(owned(&archives[0].file_name().to_string_lossy()));
        let mut text = String::new();
        flate2::read::GzDecoder::new(std::fs::File::open(archives[0].path()).unwrap()).read_to_string(&mut text).unwrap();
        assert_eq!(text, format!("{first}second\n"));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "b".repeat(40) + "\n");
        // another appender (the panic hook's `emergency`) and this writer do not overwrite each other
        let mut other = std::fs::OpenOptions::new().append(true).open(&path).unwrap();
        other.write_all(b"emergency\n").unwrap();
        f.write_all(b"after\n").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "b".repeat(40) + "\nemergency\nafter\n");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn verbose_follows_the_environment_pin_then_the_stored_setting() {
        assert_eq!(parse_pin(Some("0")), Some(false));
        assert_eq!(parse_pin(Some("on")), Some(true));
        assert_eq!(parse_pin(Some("maybe")), None);
        assert_eq!(parse_pin(None), None);
        let d = temp("verbose");
        assert_eq!(read_setting(&d), None);
        write_setting(&d, true).unwrap();
        assert_eq!(read_setting(&d), Some(true));
        write_setting(&d, false).unwrap();
        assert_eq!(read_setting(&d), Some(false));
        std::fs::write(d.join(VERBOSE_FILE), "garbage").unwrap();
        assert_eq!(read_setting(&d), None);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn the_salt_is_created_once_and_read_back() {
        let d = temp("salt");
        let a = load_salt(&d);
        assert_eq!(a, load_salt(&d));
        std::fs::write(d.join(SALT_FILE), "not hex").unwrap();
        assert_ne!(load_salt(&d), [0u8; 16]);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn an_emergency_line_is_one_masked_error_line() {
        let t = emergency_text("panic: report r-1.txt\nthread=main at 3f786850e387550fdab836ed7e6dc881de23001b");
        assert_eq!(t.lines().count(), 1, "{t}");
        assert!(t.starts_with("ERROR    [20") && t.contains("] T- panic: report r-1.txt thread=main at <token>"), "{t}");
    }

    /// Everything a failing hub call or operation could carry: short secrets,
    /// long tokens, multiline text, URL variants with credentials, queries and
    /// fragments, hosts with ports, Windows, UNC and Unix paths, addresses and
    /// file names. None of it may reach a log line, in either mode.
    const POISON: &str = "pw=hunter2 key=ab12\nsecond line: ghp_0123456789abcdef0123456789abcdef0123\n\
        https://user:pass@hub.office.lan:7370/api/poll?token=s3cr3t&to=maya.lin_2.0a1b2c #frag\n\
        http://[::1]:7371/x ftp://h/p @net:maya.lin_2.0a1b2c 100.64.1.2:7370 hub.lan:7370 star-hub \
        C:\\Users\\someone\\AppData\\x.db \\\\server\\share\\f.txt /home/someone/.ssh/id_rsa ~/notes.txt \
        ./relative/secret.txt file:///etc/passwd tailnet-name.ts.net budget-2026.xlsx";

    const LEAKS: &[&str] = &[
        "hunter2", "ghp_", "s3cr3t", "maya", "office", "someone", "ssh", "notes", "secret", "passwd", "budget", "star-hub",
        "tailnet", "lan:7370", "::1]:7371", "user:pass", "second line", "frag", "relative", "server", "x.db",
    ];

    /// Tests that switch verbose logging (a process-wide setting) take turns.
    static VERBOSITY: Mutex<()> = Mutex::new(());

    #[test]
    fn failures_and_hub_calls_reach_the_log_as_kinds_and_tags_only_in_both_modes() {
        use hubchat_core::Error;
        let _turn = VERBOSITY.lock().unwrap_or_else(|p| p.into_inner());
        let was = diag::verbose();
        for verbose in [false, true] {
            diag::set_verbose(verbose);
            let l = lines(|| {
                let hub = "https://user:pass@hub.office.lan:7370/api/poll?token=s3cr3t";
                diag::failure("upload failed", &Error::Hub { status: 403, detail: POISON.into() });
                diag::failure("hub session ended", &Error::Unreachable(POISON.into()));
                diag::Wire::start(hub, "POST", "/api/conversations/maya.lin_2.0a1b2c?x=pw=hunter2")
                    .failed(&Error::Io(std::io::Error::new(std::io::ErrorKind::NotFound, POISON)));
                diag::Wire::start(hub, "POST", "/api/send").answered(200, &Err::<(), _>(Error::NotAHub(POISON.into())));
                diag::Wire::start(hub, "DELETE", "/api/messages/abc?x=s3cr3t").status(403);
                tracing::info!(address = "maya.lin_2.0a1b2c", url = POISON, error = POISON, detail = "pw=hunter2", "field names");
            });
            let all = l.join("\n");
            for leak in LEAKS {
                assert!(!all.contains(leak), "verbose={verbose}: {leak} leaked:\n{all}");
            }
            // one line per event whatever the content: nothing multiline got in
            assert_eq!(l.len(), if verbose { 9 } else { 6 }, "verbose={verbose}:\n{all}");
            assert!(l.iter().all(|x| x.contains("] T- ")), "{all}");
            assert!(all.contains("upload failed: hub-403") && all.contains("hub session ended: unreachable:other"), "{all}");
            assert!(all.contains("POST /api/conversations/… !! io-notfound"), "{all}");
            assert!(all.contains("POST /api/send 200 !! not-a-hub"), "{all}");
            assert!(all.contains("DELETE /api/messages/… 403 ["), "{all}");
            assert!(all.contains("field names address=***** url=***** error=***** detail=*****"), "{all}");
            assert!(all.contains("hub:"), "the hub shows as a tag: {all}");
        }
        diag::set_verbose(was);
    }

    #[test]
    fn init_starts_a_real_log_in_a_scratch_folder_and_the_switch_survives_in_a_file() {
        let _turn = VERBOSITY.lock().unwrap_or_else(|p| p.into_inner());
        let app_data = temp("init");
        let logs = app_data.join("diagnostics").join("logs");
        std::fs::create_dir_all(&logs).unwrap();
        // a foreign file and an old owned file already in the folder
        std::fs::write(logs.join("notes.txt"), "mine").unwrap();
        let old = logs.join("2020-01-01_00-00-00.log");
        std::fs::write(&old, "old").unwrap();
        std::fs::OpenOptions::new().write(true).open(&old).unwrap().set_modified(SystemTime::now() - Duration::from_secs(40 * 24 * 3600)).unwrap();
        let path = start(&app_data, &logs, &app_data.join("tmp-hubchat-logs")).expect("the log starts in a scratch folder");
        tracing::info!("proof line {}", diag::tag("peer", "maya.lin_2.0a1b2c"));
        tracing::warn!(target: "wire", "RESPONSE {} POST /api/send 403 [1.000 ms]", diag::tag("hub", "http://hub.office.lan:7370"));
        let info = hc_log_info();
        assert!(info.active && info.files >= 1 && !info.fallback && Path::new(&info.dir) == logs, "{}", info.dir);
        // the Developer switch writes its file and flips the setting at once
        if pinned().is_none() {
            let info = hc_set_verbose_logging(true).unwrap();
            assert!(info.verbose && diag::verbose());
            assert_eq!(std::fs::read_to_string(app_data.join("verbose-logging")).unwrap(), "on");
            let info = hc_set_verbose_logging(false).unwrap();
            assert!(!info.verbose && !diag::verbose());
            assert_eq!(std::fs::read_to_string(app_data.join("verbose-logging")).unwrap(), "off");
        }
        shutdown(); // flushes the writer thread
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("hubchat starting") && text.contains("proof line peer:") && text.contains("POST /api/send 403"), "{text}");
        assert!(text.lines().all(|l| l.contains("] T")), "every line has the prefix: {text}");
        for leak in ["maya", "office", "diagnostics", "Users"] {
            assert!(!text.contains(leak), "{leak} in {text}");
        }
        assert!(owned(&path.file_name().unwrap().to_string_lossy()));
        assert!(!old.exists(), "the old log was pruned at start");
        assert!(logs.join("notes.txt").exists(), "a foreign file stays");
        assert_eq!(std::fs::read_to_string(app_data.join("diagnostics").join(SALT_FILE)).unwrap().trim().len(), 32);
        let _ = std::fs::remove_dir_all(&app_data);
    }

    /// A "folder" that cannot be made: a plain file sits where its parent should be.
    fn blocked(root: &Path, name: &str) -> PathBuf {
        let f = root.join(name);
        std::fs::write(&f, "not a folder").unwrap();
        f.join("logs")
    }

    fn age(p: &Path, days: u64) {
        std::fs::OpenOptions::new()
            .write(true)
            .open(p)
            .unwrap()
            .set_modified(SystemTime::now() - Duration::from_secs(days * 24 * 3600))
            .unwrap();
    }

    #[test]
    fn a_folder_that_cannot_be_written_falls_back_to_an_owned_folder_that_is_pruned_and_reported() {
        let root = temp("fallback");
        let primary = blocked(&root, "diagnostics");
        let fb = root.join("tmp-hubchat-logs");
        std::fs::create_dir_all(&fb).unwrap();
        // what an earlier failed start left there: an old log, an old archive, a foreign file
        let old_log = fb.join("2020-01-01_00-00-00.log");
        let old_gz = fb.join(format!("2020-01-01_00-00-00.log.{:032x}.gz", 7u128));
        for p in [&old_log, &old_gz] {
            std::fs::write(p, "old").unwrap();
            age(p, 40);
        }
        std::fs::write(fb.join("notes.txt"), "mine").unwrap();
        let o = open_log(&primary, &fb, "2026-10-12_01-02-03").expect("the fallback takes the log");
        assert!(o.fallback && o.dir == fb && o.path == fb.join("2026-10-12_01-02-03.log"), "{:?}", o.path);
        assert!(owned(&o.path.file_name().unwrap().to_string_lossy()), "the fallback file is one this log owns");
        assert!(!old_log.exists() && !old_gz.exists(), "old fallback logs and archives are pruned");
        assert!(fb.join("notes.txt").exists(), "a foreign file stays");
        // the page reports where the log really is, not the folder that was asked for
        let info = info_of(Some(&o.dir), true, o.fallback);
        assert!(info.active && info.fallback && Path::new(&info.dir) == fb && info.files == 1, "{}", info.dir);
        // an archive the fallback file rolls into is owned: it counts, and it ages out
        let mut f = LogFile::open_with(o.path.clone(), 64).unwrap();
        f.write_all(&[b'x'; 60]).unwrap();
        f.write_all(&[b'y'; 60]).unwrap(); // past the limit: rolls
        let archives: Vec<PathBuf> =
            std::fs::read_dir(&fb).unwrap().flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "gz")).collect();
        assert_eq!(archives.len(), 1, "{archives:?}");
        assert!(owned(&archives[0].file_name().unwrap().to_string_lossy()), "{archives:?}");
        assert_eq!(info_of(Some(&fb), true, true).files, 2, "the file and its archive");
        age(&archives[0], 40);
        prune(&fb, Some(&o.path));
        assert!(!archives[0].exists(), "an old archive in the fallback folder is pruned");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn the_usual_folder_wins_when_it_works_and_the_fallback_folder_is_still_swept() {
        let root = temp("usual");
        let (primary, fb) = (root.join("diagnostics").join("logs"), root.join("tmp-hubchat-logs"));
        std::fs::create_dir_all(&fb).unwrap();
        let old = fb.join("2020-01-01_00-00-00.log");
        std::fs::write(&old, "old").unwrap();
        age(&old, 40);
        let o = open_log(&primary, &fb, "2026-10-12_01-02-03").unwrap();
        assert!(!o.fallback && o.dir == primary && o.path.starts_with(&primary));
        assert!(!old.exists(), "what an earlier fallback start left ages out");
        let info = info_of(Some(&o.dir), true, o.fallback);
        assert!(!info.fallback && Path::new(&info.dir) == primary);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn with_no_writable_folder_or_no_subscriber_no_log_is_reported_running() {
        let root = temp("none");
        let (a, b) = (blocked(&root, "a"), blocked(&root, "b"));
        assert!(open_log(&a, &b, "2026-10-12_01-02-03").is_none(), "neither folder takes a file");
        assert!(begin(&a, &b, |_, _| panic!("there is no file, so no layer")).is_none());
        let off = info_of(None, false, true);
        assert!(!off.active && !off.verbose && !off.fallback && off.dir.is_empty() && off.files == 0 && off.bytes == 0);
        // a file opens but the subscriber is refused (another one owns the process)
        let ok = root.join("ok");
        assert!(begin(&ok, &b, |_, _| false).is_none(), "not running when the layer is refused");
        // and when it is taken, the folder that took the file is the one reported
        let taken = begin(&ok, &b, |_, _| true).expect("running when the layer is taken");
        assert!(!taken.fallback && taken.dir == ok && taken.path.starts_with(&ok));
        drop(taken);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn dependency_text_never_reaches_the_log_only_the_level_and_source_of_warnings_and_errors() {
        let l = lines(|| {
            tracing::error!(target: "reqwest::connect", url = POISON, "error sending request for url ({})", POISON);
            tracing::warn!(target: "tauri_plugin_updater::updater", "check failed: {}", POISON);
            tracing::info!(target: "tauri", "info from a dependency: {}", POISON);
            tracing::debug!(target: "hyper::proto", "{}", POISON);
            let bridged = |level, target: &str| {
                tracing_log::format_trace(&log::Record::builder().level(level).target(target).args(format_args!("{POISON}")).build()).unwrap()
            };
            bridged(log::Level::Error, "tao::platform_impl");
            bridged(log::Level::Warn, "wry::webview");
            bridged(log::Level::Info, "tauri_plugin_notification");
            tracing::warn!("our own line is kept");
            tracing::warn!(target: "hubchat_core::engine", "hub {}: refused", diag::tag("hub", "http://hub.office.lan:7370"));
        });
        let all = l.join("\n");
        for leak in LEAKS {
            assert!(!all.contains(leak), "{leak} leaked:\n{all}");
        }
        assert_eq!(l.len(), 6, "info and below from dependencies are dropped:\n{all}");
        assert!(l[0].starts_with("ERROR") && l[0].ends_with("reqwest.connect: [dependency message withheld]"), "{all}");
        assert!(l[1].starts_with("WARNING") && l[1].ends_with("tauri_plugin_updater.updater: [dependency message withheld]"), "{all}");
        assert!(l[2].starts_with("ERROR") && l[2].ends_with("tao.platform_impl: [dependency message withheld]"), "{all}");
        assert!(l[3].starts_with("WARNING") && l[3].ends_with("wry.webview: [dependency message withheld]"), "{all}");
        assert!(l[4].ends_with("our own line is kept") && l[5].contains("engine: hub hub:") && l[5].ends_with(": refused"), "{all}");
    }

    #[test]
    fn the_key_log_reaches_the_log_only_as_fixed_facts_in_both_modes_and_the_file_keeps_its_line() {
        use crate::keylog::{self, Fact};
        let _turn = VERBOSITY.lock().unwrap_or_else(|p| p.into_inner());
        let was = diag::verbose();
        let dir = temp("keylog");
        for verbose in [false, true] {
            diag::set_verbose(verbose);
            let l = lines(|| {
                keylog::note(&dir, &format!("{POISON}; the backup didn't open ({POISON})"), &[Fact::StoreHasNoKey, Fact::BackupUnreadable]);
                keylog::note(&dir, &format!("key found in Credential Manager; {POISON}"), &[Fact::KeyFound, Fact::BackupNotWritten]);
            });
            let all = l.join("\n");
            for leak in LEAKS {
                assert!(!all.contains(leak), "verbose={verbose}: {leak} leaked:\n{all}");
            }
            assert_eq!(l.len(), 2, "one line per event:\n{all}");
            assert!(l[0].ends_with("keylog: identity key: the key store has no usable key; the backup did not open"), "{all}");
            assert!(l[1].ends_with("keylog: identity key: key found in the key store; backup could not be written"), "{all}");
        }
        // the durable file is unchanged: it keeps the whole line, newest last
        let file = std::fs::read_to_string(dir.join("identity-log.txt")).unwrap();
        assert!(file.contains("hunter2") && file.contains("the backup didn't open"), "{file}");
        diag::set_verbose(was);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
