//! File search: the predicate model and the search text parsers.
//!
//! # The model
//!
//! A search is a [`FileSearchContext`]: a file domain ([`LocationContext`]),
//! a tag domain ([`TagContext`]) and a list of [`Predicate`]s that files must
//! all satisfy.
//!
//! [`Predicate`] covers tags, excluded tags, namespaces, wildcards and OR
//! groups, and wraps [`SystemPredicate`], which has one variant per kind of
//! `system:` predicate. Every value is typed: numbers are compared with
//! [`NumberTest`]s or narrower operator enums, times are either a
//! [`CalendarDelta`] age or a [`CivilDateTime`], filetypes are a
//! [`FiletypeSet`] of [`hydrus_core::Mime`]s, hashes are typed by kind, and so
//! on. Each predicate only admits the operators the reference accepts for it,
//! so an executor can match exhaustively without "impossible" arms.
//!
//! Values that depend on client state are kept symbolic and resolved when
//! the search runs:
//!
//! - services named in text are [`ServiceRef::Name`]s (lowercased, as the
//!   reference does). The executor resolves a name among the services of the
//!   types the predicate allows (file domains for `file service`, rating
//!   services for ratings, tag services for `has tag`), preferring an exact
//!   match over a case-insensitive one;
//! - URL classes are named by [`UrlRule::UrlClass`];
//! - `system:views` without canvases uses [`ViewCanvases::Default`], the
//!   user's "interesting canvases" option;
//! - rating values keep the form they were typed in ([`RatingTest`]); the
//!   executor interprets stars against the named service's own star count.
//!
//! Relative times stay calendar quantities until execution, so "1 month"
//! means one calendar month back from whenever the search runs.
//!
//! # Parsing
//!
//! [`parse_system_predicate`] parses one `system:` string with the same
//! accepted inputs and results as the reference implementation (checked
//! against its output in `oracle/fixtures/system_predicates.json`), except
//! for the deliberate differences listed in this crate's README.
//! [`parse_api_search`] parses the Client API's JSON `tags` list.
//! [`predicate_text`] writes a predicate as the reference writes it.
//!
//! # Execution
//!
//! [`search_files`] runs a [`FileSearchContext`] against the native store
//! and sorts the results; see the [`exec`] module for how.

pub mod api;
pub use hydrus_core::search::context;
pub mod error;
pub mod exec;
pub mod media;
pub use hydrus_core::search::filetype;
#[cfg(test)]
mod filetype_tests;
pub use hydrus_core::search::number;
pub mod parse;
pub use hydrus_core::search::predicate;
pub mod text;
pub use hydrus_core::search::time;

pub use api::parse_api_search;
pub use context::{FileSearchContext, LocationContext, TagContext};
pub use error::{ApiSearchError, ParseError, ParseErrorKind};
pub use exec::{
    Clock, FileSort, SearchError, SortBy, SortOrder, collect_page_files, search_files, sort_files,
    sort_page_files,
};
pub use filetype::FiletypeSet;
pub use number::{
    Comparison, DEFAULT_APPROX_PERCENT, NumberOp, NumberTest, RatingOp, RatioOp, TagNumberOp,
};
pub use parse::parse_system_predicate;
pub use predicate::{
    FileHashes, FileProperty, NamespaceFilter, NumericProperty, PixelUnit, Predicate, RatingLogic,
    RatingTest, Relationship, ServiceRef, ServiceSelection, SizeUnit, SystemPredicate,
    TagDisplayType, UrlRule, ViewCanvas, ViewCanvases, ViewingStat, Wildcard,
};
pub use text::{NamedService, TextContext, predicate_text};
pub use time::{CalendarDelta, CivilDateTime, RelativeOp, TimeKind, TimeTest};

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
}
