//! Core domain types shared by every hydrus crate.
//!
//! This crate has no I/O. It defines the vocabulary the rest of the system is
//! written in: typed ids, content hashes, file types, services, tags and the
//! enumerations that are part of the persisted format and the Client API.

pub mod bandwidth;
pub mod casefold;
pub mod content;
pub mod duplicates;
pub mod gallery;
pub mod hash;
pub mod ids;
pub mod import_options;
pub mod mime;
pub mod network;
pub mod notes;
pub mod numbers;
pub mod pages;
pub mod pybytes;
pub mod pyhtml;
pub mod pyjson;
pub mod search;
pub mod service;
pub mod sort;
pub mod subscriptions;
pub mod tag;
pub mod tag_filter;
pub mod tag_presentation;
pub mod tag_sort;
pub mod thumbnail;
pub mod time;
pub mod url;
pub mod watchers;

pub use content::{
    CanvasType, ContentStatus, ContentType, ContentUpdateAction, DuplicateType, TimestampType,
};
pub use hash::{HashKind, Md5, PerceptualHash, Sha1, Sha256, Sha512};
pub use ids::{
    HashId, LabelId, NamespaceId, NoteId, PerceptualHashId, ServiceId, SubtagId, TagId, TextId,
    UrlDomainId, UrlId,
};
pub use mime::Mime;
pub use service::{ServiceKey, ServiceType};
pub use tag::Tag;
pub use tag_filter::{FilterRule, TagFilter};
pub use time::TimestampMs;

/// The reference implementation version whose behaviour and on-disk format
/// this implementation tracks.
pub const REFERENCE_VERSION: u32 = 688;

/// The Client API version we report. Clients use this to feature-detect, so it
/// tracks the reference implementation's API surface that we are compatible with.
pub const CLIENT_API_VERSION: u32 = 95;

#[cfg(test)]
pub(crate) mod test_fixtures {
    use std::path::PathBuf;

    /// Load a JSON fixture recorded from the reference implementation.
    pub fn fixture(name: &str) -> serde_json::Value {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../oracle/fixtures")
            .join(name);
        let text = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("reading fixture {}: {e}", path.display()));
        serde_json::from_str(&text).expect("fixture is valid json")
    }

    pub fn constants() -> serde_json::Value {
        fixture("constants.json")
    }

    #[test]
    fn versions_match_reference_implementation() {
        let c = constants();
        assert_eq!(
            c["software_version"].as_u64(),
            Some(u64::from(super::REFERENCE_VERSION))
        );
        assert_eq!(
            c["client_api_version"].as_u64(),
            Some(u64::from(super::CLIENT_API_VERSION))
        );
    }
}
