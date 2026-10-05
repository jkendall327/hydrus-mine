//! Lightweight oracle helper: compiles only the actual archive I/O source against
//! cached dependencies, avoiding a Cargo/store/GUI rebuild during breadth work.
use std::{error::Error, fmt};
// Only the domain vocabulary needed by archive.rs; no cached workspace behavior.
extern crate self as hydrus_core;
pub mod hash {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum HashKind {
        Sha256,
        Md5,
        Sha1,
        Sha512,
    }
    impl HashKind {
        pub const fn byte_len(self) -> usize {
            match self {
                Self::Sha256 => 32,
                Self::Md5 => 16,
                Self::Sha1 => 20,
                Self::Sha512 => 64,
            }
        }
    }
}
pub use hash::HashKind;
#[derive(Debug)]
pub enum StoreError {
    Sqlite(rusqlite::Error),
    Io(std::io::Error),
    Invalid(String),
}
impl From<rusqlite::Error> for StoreError {
    fn from(e: rusqlite::Error) -> Self {
        Self::Sqlite(e)
    }
}
impl From<std::io::Error> for StoreError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}
impl fmt::Display for StoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl Error for StoreError {}
pub type Result<T> = std::result::Result<T, StoreError>;
pub use tag_migration::Content;
#[path = "../crates/hydrus-store/src/tag_migration/archive.rs"]
mod archive;
mod tag_migration {
    use super::archive;
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum Content {
        Mappings,
        Siblings,
        Parents,
    }
    pub fn run() -> super::Result<()> {
        let args = std::env::args().collect::<Vec<_>>();
        let content = match args[1].as_str() {
            "mappings" => Content::Mappings,
            "siblings" => Content::Siblings,
            "parents" => Content::Parents,
            _ => return Err(super::StoreError::Invalid("content".into())),
        };
        let source = archive::Archive::source(std::path::Path::new(&args[2]), content)?;
        let kind = if let archive::Metadata::Mappings(kind) = source.metadata {
            kind
        } else {
            hydrus_core::HashKind::Sha256
        };
        let mut destination =
            archive::Archive::destination(std::path::Path::new(&args[3]), content, kind)?;
        let mut cursor = (i64::MIN, i64::MIN);
        loop {
            let batch = source.read(&mut cursor, 3)?;
            if batch.is_empty() {
                break;
            }
            destination.write(&batch)?;
        }
        Ok(())
    }
}
fn main() -> Result<()> {
    tag_migration::run()
}
