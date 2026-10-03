//! Running a file search against the native store.
//!
//! [`search_files`] takes a [`FileSearchContext`] (where to look, which tags,
//! which predicates), finds the matching files and sorts them.
//!
//! # How a search runs
//!
//! 1. **Resolve** ([`context`]): the file domain and tag domain are looked up
//!    in the service registry, and every predicate is turned into an
//!    [`plan::Expr`] tree whose leaves ([`leaf::Leaf`]) know exactly which
//!    rows they need: tags become the per-service sets of *stored* tags that
//!    display as them (siblings and parents are applied in memory from the
//!    snapshot's display graphs, never by rewriting mappings), times become
//!    timestamp ranges, services and hashes become ids.
//! 2. **Evaluate** ([`plan`]): files are sets of hash ids held in roaring
//!    bitmaps. An AND runs its most selective positive terms first, each
//!    later term only looking at the files still in play: a leaf either
//!    *scans* its own index (tag -> files) and intersects, or *probes* the
//!    candidates it was given (file -> tags), whichever touches fewer rows.
//!    Negations and "has none" style terms (`system:no tags`) subtract from
//!    the candidates. The file domain itself is only materialised when some
//!    term needs it; a selective first term is instead checked against the
//!    domain file by file.
//! 3. **Sort** ([`sort`]) the result and apply `system:limit`.
//!
//! No step loads a whole mapping table: tag work is driven by the
//! `(tag_id, hash_id)` and `(hash_id, tag_id)` indexes of the per-service
//! mapping tables, so cost follows the size of the answer and of the
//! candidate set rather than the size of the tag repository.
//!
//! # Semantics
//!
//! The behaviour follows the reference implementation's
//! `ClientDBFilesSearch`; deliberate differences are listed in
//! `docs/rust/DIFFERENCES.md`. In short, every predicate is an independent
//! condition and the search is their conjunction (the reference lets a
//! second size or ratio predicate replace the first), and tags are always
//! searched as displayed, in every file domain.

mod collect;
mod context;
mod dupes;
mod leaf;
mod numbers;
mod page_sort;
mod plan;
mod similar;
mod sort;
mod sql;
mod tags;
pub(crate) mod time;

#[cfg(test)]
mod tests;

use rusqlite::Connection;

use hydrus_core::{HashId, ServiceKey};
use hydrus_store::{Snapshot, StoreError};

use crate::context::FileSearchContext;
use crate::predicate::ServiceRef;

pub use sort::{FileSort, SortBy, SortOrder};
pub use time::Clock;

/// Why a search could not run.
#[derive(Debug, thiserror::Error)]
pub enum SearchError {
    /// A service in the search context does not exist.
    #[error("there is no service with key {}", .0.to_hex())]
    NoSuchService(ServiceKey),
    /// A service in the search context is the wrong kind of service.
    #[error("the service with key {} is not a {expected}", .key.to_hex())]
    WrongServiceKind {
        key: ServiceKey,
        expected: &'static str,
    },
    /// A predicate names a service that does not exist.
    #[error("could not find the {kinds} service {service}")]
    UnknownService {
        service: String,
        kinds: &'static str,
    },
    /// A predicate names a URL class the client doesn't have.
    #[error("Did not find URL Class called \"{0}\"!")]
    UnknownUrlClass(String),
    /// A URL regex predicate's pattern does not compile.
    #[error("{pattern:?} is not a valid regular expression: {reason}")]
    InvalidRegex { pattern: String, reason: String },
    #[error(transparent)]
    Store(#[from] StoreError),
}

impl From<rusqlite::Error> for SearchError {
    fn from(e: rusqlite::Error) -> Self {
        SearchError::Store(StoreError::Sqlite(e))
    }
}

impl SearchError {
    /// Whether the search itself was at fault (as opposed to the database).
    pub fn is_bad_search(&self) -> bool {
        !matches!(self, SearchError::Store(_))
    }
}

pub(crate) type Result<T, E = SearchError> = std::result::Result<T, E>;

/// Find the files matching `search`, sorted by `sort`, as hash ids.
///
/// `conn` should be a reader connection inside a read transaction (see
/// `hydrus_store::Db::read`), and `snapshot` the store's current in-memory
/// state, so the whole search sees one consistent state. `clock` supplies
/// "now" and the time zone for time predicates.
pub fn search_files(
    conn: &Connection,
    snapshot: &Snapshot,
    search: &FileSearchContext,
    sort: FileSort,
    clock: &Clock,
) -> Result<Vec<HashId>> {
    search_with_strategy(conn, snapshot, search, sort, clock, context::Strategy::Auto)
}

/// Sort `files` by `sort` as a search in `search`'s file and tag domains
/// would (its predicates are not used): for a page whose files were not
/// found by searching, or were found earlier.
pub fn sort_files(
    conn: &Connection,
    snapshot: &Snapshot,
    search: &FileSearchContext,
    files: &[HashId],
    sort: FileSort,
    clock: &Clock,
) -> Result<Vec<HashId>> {
    let env = context::Env::new(conn, snapshot, search, clock, context::Strategy::Auto)?;
    let files: roaring::RoaringBitmap = files.iter().map(|h| h.0).collect();
    Ok(sort::sort(&env, &files, sort)?
        .into_iter()
        .map(HashId)
        .collect())
}

/// Sort a page's `files` (in the order it shows them) as the reference's
/// pages sort (`MediaList.Sort`): by the `fallback` sort first, then by
/// `sort`, each stably; files with no value take the page's defaults (no
/// views as 0, no time as -1, the inbox before the archive). Sorts by
/// system sort types, by namespaces' tags, and by rating.
pub fn sort_page_files(
    conn: &Connection,
    snapshot: &Snapshot,
    search: &FileSearchContext,
    files: &[HashId],
    sort: &hydrus_core::pages::PageSort,
    fallback: Option<&hydrus_core::pages::PageSort>,
    clock: &Clock,
) -> Result<Vec<HashId>> {
    let env = context::Env::new(conn, snapshot, search, clock, context::Strategy::Auto)?;
    let mut order: Vec<u32> = files.iter().map(|h| h.0).collect();
    for sort in fallback.into_iter().chain([sort]) {
        order = page_sort::sort_page(&env, &order, sort)?;
    }
    Ok(order.into_iter().map(HashId).collect())
}

/// Collect a page's `files` (in the order it shows them) by `collect`, as
/// the reference's pages collect (`MediaList.Collect`), and sort the files
/// and collections that make as [`sort_page_files`] does, each collection's
/// files among themselves.
#[allow(clippy::too_many_arguments)]
pub fn collect_page_files(
    conn: &Connection,
    snapshot: &Snapshot,
    search: &FileSearchContext,
    files: &[HashId],
    collect: &hydrus_core::pages::PageCollect,
    sort: &hydrus_core::pages::PageSort,
    fallback: Option<&hydrus_core::pages::PageSort>,
    clock: &Clock,
) -> Result<Vec<hydrus_core::pages::PageMedia>> {
    let env = context::Env::new(conn, snapshot, search, clock, context::Strategy::Auto)?;
    let current: Vec<hydrus_core::ServiceId> = search
        .location
        .current()
        .iter()
        .filter_map(|key| snapshot.services.by_key(key).ok().map(|s| s.id))
        .collect();
    collect::collect_and_sort(&env, &current, files, collect, sort, fallback)
}

fn search_with_strategy(
    conn: &Connection,
    snapshot: &Snapshot,
    search: &FileSearchContext,
    sort: FileSort,
    clock: &Clock,
    strategy: context::Strategy,
) -> Result<Vec<HashId>> {
    let env = context::Env::new(conn, snapshot, search, clock, strategy)?;
    let (expr, limit) = plan::build(&env, &search.predicates)?;
    if limit == Some(0) {
        return Ok(Vec::new());
    }
    let matches = plan::evaluate(&env, &expr)?;
    let mut ordered = sort::sort(&env, &matches, sort)?;
    if let Some(limit) = limit {
        ordered.truncate(usize::try_from(limit).unwrap_or(usize::MAX));
    }
    Ok(ordered.into_iter().map(HashId).collect())
}

/// How a predicate refers to a service, for error messages.
fn describe(service: &ServiceRef) -> String {
    match service {
        ServiceRef::Name(name) => format!("\"{name}\""),
        ServiceRef::Key(key) => key.to_hex(),
    }
}
