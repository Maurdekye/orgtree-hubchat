//! Which certificates a hub connection trusts: the built-in list of public
//! certificate authorities, as before, and beside it every root the DEVICE
//! trusts. A hub on a LAN has a certificate from its operator's own
//! authority (CA); once that CA is installed on the device, the apps accept
//! the hub, as a browser does. Always on, no setting (user ruling 2026-11-16,
//! Orgtree docket hubchat-on-a-lan-only-hub-apps-trust-local-certi).
//!
//! Every hub connection uses the one configuration made here: requests
//! (`HubClient`), profile pictures (`hub_pictures.rs`) and the stream
//! (`stream.rs`). The update check and the Android app download do not:
//! they are not hub traffic and keep the public list alone.
//!
//! How a certificate is checked (`Trust`):
//!
//! 1. Against the built-in list alone. A hub with a public certificate is
//!    verified exactly as it always was, and the device is never asked.
//! 2. Only when that list does not vouch for it: against the built-in list
//!    and the device's roots together. The device's roots are read then, and
//!    kept.
//! 3. If the issuer is still unknown, the device is read once more (at most
//!    every few seconds), so a CA installed while Hubchat runs counts at the
//!    next attempt, with no restart.
//! 4. What was read is used for ten minutes, then read again when it is next
//!    needed, so a CA REMOVED from the device stops counting too. (A TLS
//!    session resumed with a hub is not checked again; that is TLS's rule
//!    for every client.)
//!
//! The verifier is rustls's own (webpki) throughout: names, IP addresses,
//! dates and signatures are checked the same way for both sets of roots.
//! The device's roots all count alike; a root the system has since marked as
//! distrusted is not known to be (the built-in list has no such mark either).
//!
//! Where the device's roots come from: on Windows, macOS and Linux the
//! system's store (`rustls-native-certs`); on Android the shell hands over
//! Android's own store (`set_device_roots`, `src-tauri/src/android.rs`).
//! A store that can't be read leaves the built-in list, never less.

use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::client::WebPkiServerVerifier;
use rustls::crypto::CryptoProvider;
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{CertificateError, ClientConfig, DigitallySignedStruct, RootCertStore, SignatureScheme};

/// The device's trusted roots, each as DER bytes; `None` when they can't be read now.
type Source = Arc<dyn Fn() -> Option<Vec<Vec<u8>>> + Send + Sync>;

/// The least time between two looks at the device's roots.
const REREAD: Duration = Duration::from_secs(5);
/// How long the device's roots are used before they are read again.
const KEEP: Duration = Duration::from_secs(10 * 60);

/// A time above, or a test's own in milliseconds (`HUBCHAT_TLS_REREAD_MS`, `HUBCHAT_TLS_KEEP_MS`).
fn time(knob: &str, usual: Duration) -> Duration {
    std::env::var(knob).ok().and_then(|v| v.parse().ok()).map(Duration::from_millis).unwrap_or(usual)
}

struct Device {
    source: Source,
    /// the built-in list and the device's roots together; `None` until the device was read
    both: Option<Arc<WebPkiServerVerifier>>,
    /// the last look at the device, read or not
    looked: Option<Instant>,
    /// when `both` was read
    read: Option<Instant>,
}

impl Device {
    fn unread(source: Source) -> Self {
        Device { source, both: None, looked: None, read: None }
    }
}

/// The check of a hub's certificate (the module's notes say how).
struct Trust {
    provider: Arc<CryptoProvider>,
    /// the built-in public list alone
    public: Arc<WebPkiServerVerifier>,
    reread: Duration,
    keep: Duration,
    device: Mutex<Device>,
}

impl std::fmt::Debug for Trust {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Trust(the built-in public roots and the device's)")
    }
}

fn public_roots() -> RootCertStore {
    let mut roots = RootCertStore::empty();
    roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    roots
}

/// The built-in list and `device` together, and how many of `device` could be used.
fn roots_with(device: &[Vec<u8>]) -> (RootCertStore, usize) {
    let mut roots = public_roots();
    // a certificate that can't be read as a root (an old or odd one in a store) is left out
    let (usable, _left_out) = roots.add_parsable_certificates(device.iter().map(|der| CertificateDer::from(der.as_slice())));
    (roots, usable)
}

fn verifier(roots: RootCertStore, provider: &Arc<CryptoProvider>) -> Arc<WebPkiServerVerifier> {
    WebPkiServerVerifier::builder_with_provider(Arc::new(roots), provider.clone()).build().expect("the built-in list is never empty")
}

/// The system's store on a desktop.
#[cfg(any(target_os = "windows", target_os = "macos", target_os = "linux"))]
fn os_store() -> Option<Vec<Vec<u8>>> {
    let found = rustls_native_certs::load_native_certs();
    if found.certs.is_empty() && !found.errors.is_empty() {
        tracing::warn!(errors = found.errors.len(), "the device's certificate roots can't be read; the built-in list is used alone");
        return None;
    }
    Some(found.certs.into_iter().map(|c| c.to_vec()).collect())
}

/// Elsewhere (Android) the shell hands the store over; until it has, there is none to read.
#[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
fn os_store() -> Option<Vec<Vec<u8>>> {
    None
}

impl Trust {
    fn new() -> Self {
        Self::with(time("HUBCHAT_TLS_REREAD_MS", REREAD), time("HUBCHAT_TLS_KEEP_MS", KEEP))
    }

    fn with(reread: Duration, keep: Duration) -> Self {
        let provider = Arc::new(rustls::crypto::ring::default_provider());
        let public = verifier(public_roots(), &provider);
        Trust { provider, public, reread, keep, device: Mutex::new(Device::unread(Arc::new(os_store))) }
    }

    /// The built-in list and the device's roots together. The device is read when it never was,
    /// when what was read is older than `keep`, or `again` (the issuer is still unknown); never
    /// more often than `reread`. A device that can't be read now leaves what was read before.
    fn both(&self, again: bool) -> Option<Arc<WebPkiServerVerifier>> {
        let mut d = self.device.lock().unwrap_or_else(|e| e.into_inner());
        let old = d.read.is_none_or(|t| t.elapsed() >= self.keep);
        let due = d.looked.is_none_or(|t| t.elapsed() >= self.reread);
        if (old || again) && due {
            let now = Instant::now();
            d.looked = Some(now);
            if let Some(found) = (d.source)() {
                let (roots, usable) = roots_with(&found);
                tracing::info!(found = found.len(), usable, "device certificate roots read");
                d.both = Some(verifier(roots, &self.provider));
                d.read = Some(now);
            }
        }
        d.both.clone()
    }
}

impl ServerCertVerifier for Trust {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        intermediates: &[CertificateDer<'_>],
        server_name: &ServerName<'_>,
        ocsp_response: &[u8],
        now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        let first = self.public.verify_server_cert(end_entity, intermediates, server_name, ocsp_response, now);
        let Err(rustls::Error::InvalidCertificate(_)) = &first else { return first };
        // the built-in list does not vouch for it: the device's roots may
        let Some(both) = self.both(false) else { return first };
        match both.verify_server_cert(end_entity, intermediates, server_name, ocsp_response, now) {
            Err(rustls::Error::InvalidCertificate(CertificateError::UnknownIssuer)) => match self.both(true) {
                // a CA installed since the last look
                Some(fresh) if !Arc::ptr_eq(&fresh, &both) => fresh.verify_server_cert(end_entity, intermediates, server_name, ocsp_response, now),
                _ => Err(rustls::Error::InvalidCertificate(CertificateError::UnknownIssuer)),
            },
            checked => checked,
        }
    }

    // A handshake's signatures don't depend on the roots: rustls's verifier checks them as ever.
    fn verify_tls12_signature(&self, message: &[u8], cert: &CertificateDer<'_>, dss: &DigitallySignedStruct) -> Result<HandshakeSignatureValid, rustls::Error> {
        self.public.verify_tls12_signature(message, cert, dss)
    }

    fn verify_tls13_signature(&self, message: &[u8], cert: &CertificateDer<'_>, dss: &DigitallySignedStruct) -> Result<HandshakeSignatureValid, rustls::Error> {
        self.public.verify_tls13_signature(message, cert, dss)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.public.supported_verify_schemes()
    }
}

struct Shared {
    trust: Arc<Trust>,
    config: Arc<ClientConfig>,
}

fn shared() -> &'static Shared {
    static SHARED: OnceLock<Shared> = OnceLock::new();
    SHARED.get_or_init(|| {
        let trust = Arc::new(Trust::new());
        let config = ClientConfig::builder_with_provider(trust.provider.clone())
            .with_safe_default_protocol_versions()
            .expect("rustls's own TLS versions")
            .dangerous() // not a looser check: `Trust` hands every decision to rustls's verifier, with more roots
            .with_custom_certificate_verifier(trust.clone())
            .with_no_client_auth();
        Shared { trust, config: Arc::new(config) }
    })
}

/// The TLS configuration of every hub connection (for the stream's connector).
pub(crate) fn config() -> Arc<ClientConfig> {
    shared().config.clone()
}

/// A request client's builder with that configuration.
pub(crate) fn http() -> reqwest::ClientBuilder {
    let mut config = (*config()).clone();
    // what reqwest sets on a configuration of its own making (this build speaks HTTP/1.1)
    config.alpn_protocols = vec![b"http/1.1".to_vec()];
    reqwest::Client::builder().use_preconfigured_tls(config)
}

/// Where the device's trusted root certificates come from: each as DER
/// bytes, or `None` when they can't be read now (they are asked for again
/// later). The shell calls this once at its start where the core can't read
/// the store itself (Android); tests hand in their own.
pub fn set_device_roots(source: impl Fn() -> Option<Vec<Vec<u8>>> + Send + Sync + 'static) {
    let mut d = shared().trust.device.lock().unwrap_or_else(|e| e.into_inner());
    *d = Device::unread(Arc::new(source));
}

/// Back to the machine's own certificate store (the start's state).
pub fn reset_device_roots() {
    set_device_roots(os_store);
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustls::pki_types::pem::PemObject;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn the_device_roots_are_added_to_the_built_in_list_never_instead_of_it() {
        let public = public_roots().len();
        assert!(public > 100, "the built-in list: {public}");
        assert_eq!(roots_with(&[]).0.len(), public);
        let ca = CertificateDer::from_pem_slice(include_bytes!("../tests/tls/ca.pem")).unwrap().to_vec();
        let (roots, usable) = roots_with(&[ca.clone(), b"not a certificate".to_vec(), Vec::new()]);
        assert_eq!((roots.len(), usable), (public + 1, 1), "what can't be read is left out, the rest counts");
    }

    /// A `Trust` whose device hands over `roots` and counts how often it is asked.
    fn counting(reread: Duration, keep: Duration, roots: Option<Vec<Vec<u8>>>) -> (Trust, Arc<AtomicUsize>) {
        let (trust, reads) = (Trust::with(reread, keep), Arc::new(AtomicUsize::new(0)));
        let counted = reads.clone();
        trust.device.lock().unwrap().source = Arc::new(move || {
            counted.fetch_add(1, Ordering::SeqCst);
            roots.clone()
        });
        (trust, reads)
    }

    #[test]
    fn an_unreadable_device_store_is_asked_again_only_after_the_wait() {
        let (trust, reads) = counting(Duration::from_secs(3600), KEEP, None);
        assert!(trust.both(false).is_none() && trust.both(false).is_none() && trust.both(true).is_none());
        assert_eq!(reads.load(Ordering::SeqCst), 1, "one look, then the wait");
    }

    #[test]
    fn what_was_read_is_kept_for_a_while_then_read_again() {
        let (trust, reads) = counting(Duration::ZERO, Duration::from_millis(150), Some(Vec::new()));
        let first = trust.both(false).expect("read");
        let same = trust.both(false).expect("kept");
        assert!(Arc::ptr_eq(&first, &same) && reads.load(Ordering::SeqCst) == 1, "within the time it is kept: not asked again");
        std::thread::sleep(Duration::from_millis(170));
        let later = trust.both(false).expect("read again");
        assert!(!Arc::ptr_eq(&first, &later) && reads.load(Ordering::SeqCst) == 2, "older than that: read again, so a removed CA goes");
        let asked = trust.both(true).expect("read at once");
        assert!(!Arc::ptr_eq(&later, &asked) && reads.load(Ordering::SeqCst) == 3, "an unknown issuer asks at once");
    }
}
