use thiserror::Error;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Error)]
pub enum Error {
    /// The hub could not be reached at all (DNS, refused, timeout, TLS).
    #[error("hub unreachable: {0}")]
    Unreachable(String),
    /// The hub answered with an HTTP error. `detail` is the hub's own message.
    #[error("hub said {status}: {detail}")]
    Hub { status: u16, detail: String },
    /// Something answered, but it does not speak the mail hub protocol.
    #[error("not a mail hub: {0}")]
    NotAHub(String),
    #[error("invalid input: {0}")]
    Invalid(String),
    #[error("local store: {0}")]
    Store(String),
    #[error("message body is larger than {0} bytes")]
    BodyTooLarge(u64),
    /// A publication recovery read was temporarily refused by local storage.
    #[error("Cannot read the saved file yet; waiting to retry")]
    LocalBusy,
    /// Android paused file work while ordinary message checks may still run.
    #[error("File paused; open Hubchat to resume.")]
    FilePaused,
    #[error("cancelled")]
    Cancelled,
    #[error("Could not save the downloaded file. Check free space and file access, then retry.")]
    SaveDownload(#[source] std::io::Error),
    #[error("i/o: {0}")]
    Io(#[from] std::io::Error),
}

impl Error {
    /// Public file-card text must never contain request URLs or server bodies.
    /// Keep the original error for diagnostics; do not persist it as UI text.
    pub(crate) fn file_message(&self) -> String {
        match self {
            Self::Unreachable(_) => "Connection interrupted; waiting to retry".into(),
            Self::Hub { status: 408 | 409 | 429 | 500..=599, .. } =>
                "The hub is busy or unavailable; waiting to retry".into(),
            Self::Hub { status: 404 | 410, .. } => "The file is no longer on the hub".into(),
            Self::Hub { status: 413, .. } => "The file is too large for this hub".into(),
            Self::Hub { status: 401 | 403, .. } => "The hub refused access to this file".into(),
            Self::Hub { .. } => "The hub could not accept the file. Try again.".into(),
            Self::NotAHub(_) => "The hub sent an unexpected response. Try again.".into(),
            _ => self.to_string(),
        }
    }

    pub fn status(&self) -> Option<u16> {
        match self {
            Error::Hub { status, .. } => Some(*status),
            _ => None,
        }
    }
}

impl From<reqwest::Error> for Error {
    fn from(e: reqwest::Error) -> Self {
        if e.is_decode() {
            Error::NotAHub(e.to_string())
        } else if let Some(why) = certificate_refused(&e) {
            Error::Unreachable(why)
        } else {
            Error::Unreachable(e.to_string())
        }
    }
}

/// A hub certificate this device refused, said so that its owner can act on
/// it. reqwest's own text is only "error sending request"; the reason sits
/// further down its chain of causes, in the TLS library's words.
fn certificate_refused(e: &dyn std::error::Error) -> Option<String> {
    let mut cause = e.source();
    while let Some(c) = cause {
        let text = c.to_string();
        if let Some(at) = text.find("invalid peer certificate") {
            let detail = text[at..].trim();
            return Some(if detail.contains("UnknownIssuer") {
                "this device does not trust the hub's certificate. If the hub has a certificate from an authority of its own, install that authority's certificate on this device".into()
            } else if detail.contains("CaUsedAsEndEntity") {
                // what a single `openssl req -x509` makes: the verifier takes no authority's own
                // certificate as a server's, whether or not the device trusts it
                "the hub uses a certificate authority's own certificate as its certificate (often one self-signed certificate for everything). Hubchat needs two: the authority's certificate, installed on this device, and a separate certificate for the hub, issued by that authority".into()
            } else {
                format!("the hub's certificate was refused ({detail})")
            });
        }
        cause = c.source();
    }
    None
}
