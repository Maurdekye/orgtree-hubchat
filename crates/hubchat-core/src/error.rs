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
    #[error("cancelled")]
    Cancelled,
    #[error("i/o: {0}")]
    Io(#[from] std::io::Error),
}

impl Error {
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
