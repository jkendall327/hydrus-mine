//! The error type for everything in this crate.

use std::path::PathBuf;

use hydrus_core::Mime;

/// Why a file could not be inspected or rendered.
///
/// The variants mirror the reference implementation's exception classes where
/// callers act on the difference (an import job reports "unsupported" and
/// "damaged" files differently), so they are part of the behaviour, not just
/// messages.
#[derive(Debug, thiserror::Error)]
pub enum MediaError {
    /// Reading the file (or a temp file) failed.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    /// The file is empty (`ZeroSizeFileException`).
    #[error("file is of zero length")]
    ZeroSize,
    /// The file type is not importable (`UnsupportedFileException`).
    #[error("unsupported file type {mime}: {reason}")]
    Unsupported {
        /// What the file was detected as.
        mime: Mime,
        /// Human-readable explanation.
        reason: String,
    },
    /// The file is a known type but its content could not be understood
    /// (`DamagedOrUnusualFileException`).
    #[error("damaged or unusual file: {0}")]
    Damaged(String),
    /// The ffmpeg executable could not be run.
    #[error("could not run ffmpeg at {path}: {source}")]
    FfmpegMissing {
        /// The executable we tried to run.
        path: PathBuf,
        /// The underlying spawn error.
        source: std::io::Error,
    },
    /// ffmpeg produced no output at all (`DataMissing`).
    #[error("ffmpeg returned no output for this file")]
    FfmpegNoOutput,
}

impl MediaError {
    pub(crate) fn damaged(msg: impl Into<String>) -> Self {
        MediaError::Damaged(msg.into())
    }
}

/// Shorthand for results in this crate.
pub type Result<T, E = MediaError> = std::result::Result<T, E>;
