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
        } else {
            Error::Unreachable(e.to_string())
        }
    }
}
