//! Errors reading a reference database.

use std::path::PathBuf;

use crate::serialisable::SerialisableError;

/// Anything that can go wrong reading a reference database.
#[derive(Debug, thiserror::Error)]
pub enum LegacyError {
    /// The directory does not look like a hydrus client database directory.
    #[error("{} is not a hydrus client database directory: {reason}", path.display())]
    NotADatabase { path: PathBuf, reason: String },

    /// The database was written by a different version of the reference client.
    #[error(
        "this database is hydrus version {found}, but only version {supported} can be read; {advice}"
    )]
    UnsupportedVersion {
        found: i64,
        supported: u32,
        advice: &'static str,
    },

    /// A table that the reference schema guarantees is missing.
    #[error("table {0} is missing from the database")]
    MissingTable(String),

    /// A service id that does not exist, or is not the kind of service asked about.
    #[error("service {service_id} {problem}")]
    BadService { service_id: u32, problem: String },

    /// A stored value that does not have the shape the reference writes.
    #[error("unexpected value in {table}: {detail}")]
    BadValue { table: String, detail: String },

    /// A stored serialised object could not be decoded.
    #[error("serialised object in {location}: {source}")]
    Serialisable {
        location: String,
        #[source]
        source: SerialisableError,
    },

    /// The legacy YAML `options` row could not be decoded.
    #[error("legacy options: {0}")]
    Yaml(String),

    #[error("sqlite: {0}")]
    Sqlite(#[from] rusqlite::Error),

    #[error("{}: {source}", path.display())]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

impl LegacyError {
    pub(crate) fn bad_value(table: impl Into<String>, detail: impl Into<String>) -> Self {
        LegacyError::BadValue {
            table: table.into(),
            detail: detail.into(),
        }
    }

    pub(crate) fn serialisable(location: impl Into<String>, source: SerialisableError) -> Self {
        LegacyError::Serialisable {
            location: location.into(),
            source,
        }
    }
}

/// Result alias for this crate.
pub type Result<T, E = LegacyError> = std::result::Result<T, E>;
