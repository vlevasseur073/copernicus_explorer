/// All the ways operations in this crate can fail.
///
/// `thiserror::Error` automatically implements the `std::error::Error` and
/// `Display` traits for us.  The `#[error("...")]` attribute on each variant
/// defines what gets printed when you display the error.
///
/// The `#[from]` attribute generates a `From<reqwest::Error>` impl so that
/// the `?` operator can convert an HTTP error into our error type
/// automatically.
#[derive(Debug, thiserror::Error)]
pub enum CopernicusError {
    #[error("authentication failed: {0}")]
    AuthenticationFailed(String),

    #[error("HTTP request failed: {0}")]
    HttpError(#[from] reqwest::Error),

    #[error("failed to parse JSON response: {0}")]
    JsonError(#[from] serde_json::Error),

    #[error("search failed: {0}")]
    SearchFailed(String),

    #[error("download failed: {0}")]
    DownloadFailed(String),

    #[error("search returned zero results")]
    NoResults,

    #[error("invalid argument: {0}")]
    InvalidArgument(String),

    #[error("I/O error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("async runtime error: {0}")]
    RuntimeError(String),
}

/// A convenience alias so we don't have to write
/// `Result<T, CopernicusError>` everywhere.
pub type Result<T> = std::result::Result<T, CopernicusError>;
