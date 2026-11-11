//! `hubchat-web healthcheck`: asks the server running in this same container
//! for `/healthz` and says whether it answered (the image's HEALTHCHECK;
//! the image holds nothing else that could ask).
use std::io::{Read, Write};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, TcpStream};
use std::sync::Arc;
use std::time::Duration;

use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::crypto::{verify_tls12_signature, verify_tls13_signature, CryptoProvider};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{DigitallySignedStruct, SignatureScheme};

use crate::settings::Settings;

const PATIENCE: Duration = Duration::from_secs(4);

/// Trusts exactly the certificate the server was given: the check talks to
/// 127.0.0.1, a name no certificate is issued for.
#[derive(Debug)]
struct OwnCertificate {
    cert: CertificateDer<'static>,
    provider: Arc<CryptoProvider>,
}

impl ServerCertVerifier for OwnCertificate {
    fn verify_server_cert(&self, end_entity: &CertificateDer<'_>, _: &[CertificateDer<'_>], _: &ServerName<'_>, _: &[u8], _: UnixTime) -> Result<ServerCertVerified, rustls::Error> {
        if end_entity.as_ref() == self.cert.as_ref() {
            Ok(ServerCertVerified::assertion())
        } else {
            Err(rustls::Error::General("not the certificate this server was given".into()))
        }
    }
    fn verify_tls12_signature(&self, message: &[u8], cert: &CertificateDer<'_>, dss: &DigitallySignedStruct) -> Result<HandshakeSignatureValid, rustls::Error> {
        verify_tls12_signature(message, cert, dss, &self.provider.signature_verification_algorithms)
    }
    fn verify_tls13_signature(&self, message: &[u8], cert: &CertificateDer<'_>, dss: &DigitallySignedStruct) -> Result<HandshakeSignatureValid, rustls::Error> {
        verify_tls13_signature(message, cert, dss, &self.provider.signature_verification_algorithms)
    }
    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.provider.signature_verification_algorithms.supported_schemes()
    }
}

fn ask(mut stream: impl Read + Write) -> Result<String, String> {
    stream.write_all(b"GET /healthz HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n").map_err(|e| format!("can't ask: {e}"))?;
    let mut answer = Vec::new();
    // a TLS peer that closes without saying goodbye still gave its answer
    let _ = stream.take(65_536).read_to_end(&mut answer);
    Ok(String::from_utf8_lossy(&answer).into_owned())
}

/// Ok when the server on this machine answers `/healthz` as Hubchat Web.
pub fn check(settings: &Settings) -> Result<(), String> {
    // a server on every interface is asked on this machine's own
    let ip = match settings.bind {
        IpAddr::V4(a) if a.is_unspecified() => IpAddr::V4(Ipv4Addr::LOCALHOST),
        IpAddr::V6(a) if a.is_unspecified() => IpAddr::V6(Ipv6Addr::LOCALHOST),
        other => other,
    };
    let at = SocketAddr::new(ip, settings.port);
    let stream = TcpStream::connect_timeout(&at, PATIENCE).map_err(|e| format!("no answer on {at}: {e}"))?;
    stream.set_read_timeout(Some(PATIENCE)).and_then(|()| stream.set_write_timeout(Some(PATIENCE))).map_err(|e| e.to_string())?;
    let answer = match &settings.tls {
        None => ask(stream)?,
        Some((cert, key)) => {
            let (certs, _) = crate::serve::load_tls(cert, key)?;
            let provider = Arc::new(rustls::crypto::ring::default_provider());
            let config = rustls::ClientConfig::builder_with_provider(provider.clone())
                .with_safe_default_protocol_versions()
                .map_err(|e| format!("TLS: {e}"))?
                .dangerous()
                .with_custom_certificate_verifier(Arc::new(OwnCertificate { cert: certs[0].clone(), provider }))
                .with_no_client_auth();
            let name = ServerName::try_from("localhost").map_err(|e| e.to_string())?;
            let conn = rustls::ClientConnection::new(Arc::new(config), name).map_err(|e| format!("TLS: {e}"))?;
            ask(rustls::StreamOwned::new(conn, stream))?
        }
    };
    let (head, body) = answer.split_once("\r\n\r\n").unwrap_or((&answer, ""));
    let status = head.lines().next().unwrap_or("");
    if !status.starts_with("HTTP/1.1 200") {
        return Err(format!("{at} answered \"{status}\", not 200"));
    }
    match serde_json::from_str::<serde_json::Value>(body.trim()) {
        Ok(v) if v["ok"] == true && v["app"] == "hubchat-web" => Ok(()),
        _ => Err(format!("{at} answered, but not as Hubchat Web")),
    }
}
