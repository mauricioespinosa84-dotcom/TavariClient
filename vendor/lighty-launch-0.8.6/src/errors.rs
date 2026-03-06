use thiserror::Error;

/// Erreurs possibles lors de l'installation
#[derive(Debug, Error)]
pub enum InstallerError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("SHA1 verification failed: {0}")]
    Sha1(#[from] lighty_core::HashError),

    #[error("Query error: {0}")]
    Query(#[from] lighty_loaders::utils::error::QueryError),

    #[error("Loader not supported: {0}")]
    UnsupportedLoader(String),

    #[error("Invalid metadata: expected Version")]
    InvalidMetadata,

    #[error("Missing required field: {0}")]
    MissingField(String),

    #[error("Download failed: {0}")]
    DownloadFailed(String),

    #[error("Zip error: {0}")]
    Zip(#[from] async_zip::error::ZipError),
}

pub type InstallerResult<T> = std::result::Result<T, InstallerError>;
