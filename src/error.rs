//! The single error type shared by the library and the binary.

use std::fmt;

/// Result alias used across the crate.
pub type Result<T> = std::result::Result<T, Error>;

/// Everything that can go wrong while talking to the upstream services or
/// while interpreting a command line.
#[derive(Debug)]
pub enum Error {
    /// A network call failed (DNS, TLS, timeout, non-2xx status, read failure).
    Http {
        /// The URL that was requested.
        url: String,
        /// A readable description of the failure.
        message: String,
    },
    /// An upstream payload did not look like the documented format.
    Parse(String),
    /// Standard input/output failed, for example in the MCP loop.
    Io(std::io::Error),
    /// JSON encoding or decoding failed.
    Json(serde_json::Error),
    /// The command line was invalid; the CLI exits with status 2.
    Usage(String),
    /// A `--station` filter matched none of the known stations; the CLI exits
    /// with status 1.
    NoMatch(String),
}

impl Error {
    /// Build a usage error.
    pub fn usage(message: impl Into<String>) -> Self {
        Self::Usage(message.into())
    }

    /// Build a parse error.
    pub fn parse(message: impl Into<String>) -> Self {
        Self::Parse(message.into())
    }

    /// Build a station-lookup error.
    pub fn no_match(message: impl Into<String>) -> Self {
        Self::NoMatch(message.into())
    }

    /// Build an HTTP error for `url` from any displayable cause.
    pub fn http(url: &str, cause: impl fmt::Display) -> Self {
        Self::Http {
            url: url.to_string(),
            message: cause.to_string(),
        }
    }

    /// Whether this error should be reported as a command-line usage problem.
    #[must_use]
    pub fn is_usage(&self) -> bool {
        matches!(self, Self::Usage(_))
    }
}

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Http { url, message } => {
                write!(formatter, "HTTP request to {url} failed: {message}")
            }
            Self::Parse(message) => write!(formatter, "could not parse upstream data: {message}"),
            Self::Io(error) => write!(formatter, "I/O error: {error}"),
            Self::Json(error) => write!(formatter, "JSON error: {error}"),
            Self::Usage(message) => write!(formatter, "{message}"),
            Self::NoMatch(message) => write!(formatter, "{message}"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::Json(error) => Some(error),
            _ => None,
        }
    }
}

impl From<std::io::Error> for Error {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<serde_json::Error> for Error {
    fn from(error: serde_json::Error) -> Self {
        Self::Json(error)
    }
}
