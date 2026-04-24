//! Error types for chrona-core.
//!
//! All public functions return `Result<T, Error>`. Errors are categorized into
//! user-recoverable and non-recoverable; see the variant docs.

use std::io;
use thiserror::Error;

/// The primary error type for Chrona.
#[derive(Debug, Error)]
pub enum Error {
    /// I/O or storage-layer failure. Usually not user-recoverable without
    /// restoring from a backup.
    #[error("storage error: {0}")]
    Storage(String),

    /// Database file is not a valid Chrona database, or uses a format version
    /// we cannot read.
    #[error("format error: {0}")]
    Format(String),

    /// Query parse or resolution failure. User-recoverable — fix the query.
    #[error("query error: {0}")]
    Query(String),

    /// Invalid input data: confidence out of range, revision chain loop,
    /// ext_id collision, malformed timestamp, etc.
    #[error("schema error: {0}")]
    Schema(String),

    /// An entity was not found.
    #[error("not found: {0}")]
    NotFound(String),

    /// An invariant of the format or engine was violated. This indicates a bug.
    #[error("internal error: {0}")]
    Internal(String),

    /// Wraps a raw I/O error from the filesystem.
    #[error("io error: {0}")]
    Io(#[from] io::Error),
}

impl Error {
    /// Stable error code suitable for pattern-matching in bindings and tests.
    pub fn code(&self) -> &'static str {
        match self {
            Error::Storage(_) => "E_STORAGE",
            Error::Format(_) => "E_FORMAT",
            Error::Query(_) => "E_QUERY",
            Error::Schema(_) => "E_SCHEMA",
            Error::NotFound(_) => "E_NOT_FOUND",
            Error::Internal(_) => "E_INTERNAL",
            Error::Io(_) => "E_IO",
        }
    }
}

// Bridge redb's error hierarchy into ours.
macro_rules! impl_redb_error {
    ($src:ty) => {
        impl From<$src> for Error {
            fn from(e: $src) -> Self {
                Error::Storage(e.to_string())
            }
        }
    };
}

impl_redb_error!(redb::DatabaseError);
impl_redb_error!(redb::TransactionError);
impl_redb_error!(redb::TableError);
impl_redb_error!(redb::CommitError);
impl_redb_error!(redb::StorageError);

/// Convenience alias used throughout the crate.
pub type Result<T> = std::result::Result<T, Error>;
