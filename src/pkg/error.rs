use thiserror::Error;

// Define a custom error type
#[derive(Debug, Error)]
pub enum CdnError {
    #[error("Cache error: {0}")]
    CacheError(String),

    #[error("Load balancer error: {0}")]
    LoadBalancerError(String),

    #[error("Configuration error: {0}")]
    ConfigError(String),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error), // Automatically converts std::io::Error to CdnError
    #[error("invalid header (expected {expected:?}, found {found:?})")]
    InvalidHeader { expected: String, found: String },
}
