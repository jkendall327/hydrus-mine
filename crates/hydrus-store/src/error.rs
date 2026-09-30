//! Store errors.

use hydrus_core::ServiceId;

pub type Result<T, E = StoreError> = std::result::Result<T, E>;

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("database error: {0}")]
    Sqlite(#[from] rusqlite::Error),

    #[error("i/o error: {0}")]
    Io(#[from] std::io::Error),

    #[error(
        "this database was written by a newer version of hydrus-rs (schema {found}; this build supports up to {supported})"
    )]
    SchemaTooNew { found: u32, supported: u32 },

    #[error("no service with id {0:?}")]
    NoSuchService(ServiceId),

    #[error("no service with key {0}")]
    NoSuchServiceKey(String),

    #[error("service \"{name}\" is a {actual}, but this needs {expected}")]
    WrongServiceType {
        name: String,
        actual: &'static str,
        expected: &'static str,
    },

    #[error("stored data is corrupt: {0}")]
    Corrupt(String),

    #[error("invalid config json: {0}")]
    Json(#[from] serde_json::Error),

    #[error("the database writer has shut down")]
    WriterGone,

    #[error("reading the reference database: {0}")]
    Legacy(#[from] hydrus_legacy::LegacyError),

    #[error("{0}")]
    Invalid(String),
}
