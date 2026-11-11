//! The listener: HTTP/1.1, plain or over the server's own TLS, one small
//! task per connection. Every answer comes from `Site::answer`; this file
//! only moves bytes and writes the request log.
use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use bytes::Bytes;
use http_body_util::Full;
use hyper::body::Incoming;
use hyper::header::{HeaderName, HeaderValue, ACCEPT_ENCODING, CONTENT_LENGTH, IF_NONE_MATCH};
use hyper::service::service_fn;
use hyper::{Request, Response, StatusCode};
use hyper_util::rt::{TokioIo, TokioTimer};
use rustls::pki_types::pem::PemObject;
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::TcpListener;
use tokio::sync::Semaphore;
use tokio_rustls::TlsAcceptor;

use crate::settings::Settings;
use crate::site::{self, Body, Site};

/// More connections than this at once are dropped: the server must not be
/// talked into unbounded memory.
const MAX_CONNECTIONS: usize = 1024;
/// A client gets this long to finish its TLS greeting, and its request's head.
const PATIENCE: Duration = Duration::from_secs(15);

/// The certificate chain and key of HUBCHAT_WEB_TLS_CERT / _KEY.
pub fn load_tls(cert: &std::path::Path, key: &std::path::Path) -> Result<(Vec<CertificateDer<'static>>, PrivateKeyDer<'static>), String> {
    let certs = CertificateDer::pem_file_iter(cert)
        .and_then(|it| it.collect::<Result<Vec<_>, _>>())
        .map_err(|e| format!("HUBCHAT_WEB_TLS_CERT: can't read {}: {e}", cert.display()))?;
    if certs.is_empty() {
        return Err(format!("HUBCHAT_WEB_TLS_CERT: {} holds no certificate (a PEM file is expected).", cert.display()));
    }
    let key = PrivateKeyDer::from_pem_file(key).map_err(|e| format!("HUBCHAT_WEB_TLS_KEY: can't read a private key from {}: {e}", key.display()))?;
    Ok((certs, key))
}

fn acceptor(settings: &Settings) -> Result<Option<TlsAcceptor>, String> {
    let Some((cert, key)) = &settings.tls else { return Ok(None) };
    let (certs, key) = load_tls(cert, key)?;
    let mut config = rustls::ServerConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
        .with_safe_default_protocol_versions()
        .map_err(|e| format!("TLS: {e}"))?
        .with_no_client_auth()
        .with_single_cert(certs, key)
        .map_err(|e| format!("HUBCHAT_WEB_TLS_CERT and HUBCHAT_WEB_TLS_KEY don't make a usable pair: {e}"))?;
    config.alpn_protocols = vec![b"http/1.1".to_vec()];
    Ok(Some(TlsAcceptor::from(Arc::new(config))))
}

/// Days since 1970-01-01 as a calendar date (Howard Hinnant's civil_from_days).
fn civil(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = (if mp < 10 { mp + 3 } else { mp - 9 }) as u32;
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}

/// `2026-11-11T18:05:07Z`
pub fn timestamp(at: SystemTime) -> String {
    let secs = at.duration_since(UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0);
    let (y, mo, d) = civil(secs.div_euclid(86_400));
    let s = secs.rem_euclid(86_400);
    format!("{y:04}-{mo:02}-{d:02}T{:02}:{:02}:{:02}Z", s / 3600, s % 3600 / 60, s % 60)
}

/// One line per request: the time, the path, the status and how long it
/// took. No address, no query, no header: nothing about who asked.
fn log_line(path: &str, status: u16, ms: u128) -> String {
    serde_json::json!({ "t": timestamp(SystemTime::now()), "path": path, "status": status, "ms": ms as u64 }).to_string()
}

async fn handle(site: Arc<Site>, log: bool, req: Request<Incoming>) -> Result<Response<Full<Bytes>>, Infallible> {
    let started = Instant::now();
    let header = |name| req.headers().get(name).and_then(|v: &HeaderValue| v.to_str().ok());
    let reply = site.answer(&site::Request {
        method: req.method().as_str(),
        path: req.uri().path(),
        accept_encoding: header(ACCEPT_ENCODING),
        if_none_match: header(IF_NONE_MATCH),
    });
    let mut res = Response::new(Full::new(if req.method() == hyper::Method::HEAD || reply.status == 304 {
        Bytes::new()
    } else {
        match reply.body {
            Body::Fixed(b) => Bytes::from_static(b),
            Body::Made(ref b) => Bytes::from(b.clone()),
        }
    }));
    *res.status_mut() = StatusCode::from_u16(reply.status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    for (name, value) in &reply.headers {
        if let Ok(v) = HeaderValue::from_str(value) {
            res.headers_mut().insert(HeaderName::from_static(*name), v);
        }
    }
    if reply.status != 304 {
        // a HEAD says how long the GET's body would be
        res.headers_mut().insert(CONTENT_LENGTH, HeaderValue::from(reply.body.len() as u64));
    }
    if log {
        println!("{}", log_line(req.uri().path(), reply.status, started.elapsed().as_millis()));
    }
    Ok(res)
}

async fn connection<I: AsyncRead + AsyncWrite + Unpin>(io: I, site: Arc<Site>, log: bool) {
    let service = service_fn(move |req| handle(site.clone(), log, req));
    // a client that hangs up mid-request is its own business: nothing to report
    let _ = hyper::server::conn::http1::Builder::new().timer(TokioTimer::new()).header_read_timeout(PATIENCE).serve_connection(TokioIo::new(io), service).await;
}

/// Bind and serve until the process is told to stop. `ready` is called once,
/// with the address the listener got.
pub async fn serve(settings: &Settings, ready: impl FnOnce(SocketAddr)) -> Result<(), String> {
    let tls = acceptor(settings)?;
    let site = Arc::new(Site::new(settings));
    let at = SocketAddr::new(settings.bind, settings.port);
    let listener = TcpListener::bind(at).await.map_err(|e| format!("can't listen on {at}: {e}"))?;
    ready(listener.local_addr().unwrap_or(at));
    let slots = Arc::new(Semaphore::new(MAX_CONNECTIONS));
    let log = settings.log_requests;
    loop {
        let (stream, _) = match listener.accept().await {
            Ok(s) => s,
            // out of file handles, a connection reset before accept: wait a moment, keep serving
            Err(_) => {
                tokio::time::sleep(Duration::from_millis(50)).await;
                continue;
            }
        };
        let Ok(slot) = slots.clone().try_acquire_owned() else { continue };
        let _ = stream.set_nodelay(true);
        let (site, tls) = (site.clone(), tls.clone());
        tokio::spawn(async move {
            let _slot = slot;
            match tls {
                None => connection(stream, site, log).await,
                Some(tls) => {
                    if let Ok(Ok(stream)) = tokio::time::timeout(PATIENCE, tls.accept(stream)).await {
                        connection(stream, site, log).await;
                    }
                }
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_log_time_is_utc_to_the_second() {
        let at = |secs: u64| timestamp(UNIX_EPOCH + Duration::from_secs(secs));
        assert_eq!(at(0), "1970-01-01T00:00:00Z");
        assert_eq!(at(951_782_399), "2000-02-28T23:59:59Z");
        assert_eq!(at(951_782_400), "2000-02-29T00:00:00Z");
        assert_eq!(at(1_794_420_307), "2026-11-11T18:05:07Z");
        assert_eq!(at(4_102_444_799), "2099-12-31T23:59:59Z");
    }

    #[test]
    fn the_log_line_says_nothing_about_who_asked() {
        let line: serde_json::Value = serde_json::from_str(&log_line("/assets/app.js", 200, 3)).unwrap();
        let mut keys: Vec<&str> = line.as_object().unwrap().keys().map(String::as_str).collect();
        keys.sort_unstable();
        assert_eq!(keys, ["ms", "path", "status", "t"]);
        assert_eq!((line["path"].as_str(), line["status"].as_u64(), line["ms"].as_u64()), (Some("/assets/app.js"), Some(200), Some(3)));
    }
}
