//! Which files match a file search, for the endpoints that filter by one
//! (potential duplicates).
//!
//! As in the reference, a potential-duplicates search's file searches are
//! ordinary file searches in the potentials' file domain, each with its own
//! tag service; a pair's kings are checked against their results. Each
//! search runs once, over the whole domain, and the filter answers from the
//! result. (The reference restricts its search to the candidate kings, so
//! `system:limit` there limits among the candidates; here it limits the
//! whole search. Nobody puts a limit in a duplicates search.)

use std::collections::HashSet;

use rusqlite::Connection;

use hydrus_core::HashId;
use hydrus_search::{
    Clock, FileSearchContext, FileSort, Predicate, SearchError, SortBy, SortOrder, SystemPredicate,
    search_files,
};
use hydrus_store::Snapshot;

/// A file search a potential-duplicates search filters by.
#[derive(Debug, Clone)]
pub struct PotentialsFileSearch(pub FileSearchContext);

impl PotentialsFileSearch {
    /// Whether every file in the domain matches (no predicates, or only
    /// `system:everything`), so there is nothing to search.
    pub fn matches_everything(&self) -> bool {
        self.0
            .predicates
            .iter()
            .all(|p| matches!(p, Predicate::System(SystemPredicate::Everything)))
    }

    /// The files that match, or `None` if every file does.
    pub fn run(
        &self,
        conn: &Connection,
        snapshot: &Snapshot,
    ) -> Result<Option<HashSet<HashId>>, SearchError> {
        if self.matches_everything() {
            return Ok(None);
        }
        let sort = FileSort {
            by: SortBy::ImportTime,
            order: SortOrder::Ascending,
        };
        let found = search_files(conn, snapshot, &self.0, sort, &Clock::system())?;
        Ok(Some(found.into_iter().collect()))
    }
}
