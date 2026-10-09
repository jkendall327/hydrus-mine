//! A potential-duplicates search as the duplicates page and the Client API
//! describe it, with its file searches as predicates.
//!
//! As in the reference, the file searches are ordinary file searches in the
//! potentials' file domain, each with its own tag service; a pair's kings
//! are checked against their results. Each search runs once, over the whole
//! domain, and the filter answers from the result. (The reference restricts
//! its search to the candidate kings, so `system:limit` there limits among
//! the candidates; here it limits the whole search. Nobody puts a limit in a
//! duplicates search.)

use std::collections::HashSet;

use rusqlite::Connection;

use hydrus_core::HashId;
use hydrus_core::duplicates::DuplicatesSearch;
use hydrus_search::{
    Clock, FileSearchContext, FileSort, Predicate, SearchError, SortBy, SortOrder, SystemPredicate,
    search_files,
};
use hydrus_store::Snapshot;
use hydrus_store::duplicates::cache::PairRow;
use hydrus_store::duplicates::{
    FileFilter, FileScope, PairSearchKind, PixelDuplicates, PotentialsSearch, count_matching,
    pair_space,
};

/// A potential-duplicates search with its file searches already run, to
/// count a space of pairs a block at a time (the reference's fragmentary
/// search).
#[derive(Debug, Clone)]
pub struct PreparedQuery {
    scope: FileScope,
    kind: PairSearchKind,
    pixel_duplicates: PixelDuplicates,
    max_hamming_distance: u32,
    one: Option<HashSet<HashId>>,
    two: Option<HashSet<HashId>>,
}

impl PreparedQuery {
    /// How many of `rows` the search finds.
    pub fn count_matching(
        &self,
        conn: &Connection,
        rows: &[PairRow],
    ) -> hydrus_store::Result<usize> {
        let one = self.one.as_ref().map(in_set);
        let two = self.two.as_ref().map(in_set);
        let search = PotentialsSearch {
            scope: self.scope.clone(),
            kind: self.kind,
            pixel_duplicates: self.pixel_duplicates,
            max_hamming_distance: self.max_hamming_distance,
            search_1: one.as_ref().map(|f| f as &FileFilter<'_>),
            search_2: two.as_ref().map(|f| f as &FileFilter<'_>),
        };
        count_matching(conn, &search, rows)
    }
}

/// A potential-duplicates search.
#[derive(Debug, Clone)]
pub struct PotentialsQuery {
    pub scope: FileScope,
    pub kind: PairSearchKind,
    pub pixel_duplicates: PixelDuplicates,
    pub max_hamming_distance: u32,
    pub search_1: FileSearchContext,
    /// Only used by [`PairSearchKind::BothFilesMatchDifferentSearches`].
    pub search_2: FileSearchContext,
}

/// Whether every file in the domain matches (no predicates, or only
/// `system:everything`), so there is nothing to search.
pub fn matches_everything(search: &FileSearchContext) -> bool {
    search
        .predicates
        .iter()
        .all(|p| matches!(p, Predicate::System(SystemPredicate::Everything)))
}

/// The files that match `search`, or `None` if every file does.
fn run(
    search: &FileSearchContext,
    conn: &Connection,
    snapshot: &Snapshot,
) -> Result<Option<HashSet<HashId>>, SearchError> {
    if matches_everything(search) {
        return Ok(None);
    }
    let sort = FileSort {
        by: SortBy::ImportTime,
        order: SortOrder::Ascending,
    };
    let found = search_files(conn, snapshot, search, sort, &Clock::system())?;
    Ok(Some(found.into_iter().collect()))
}

/// A filter answering from a search's results.
fn in_set(
    set: &HashSet<HashId>,
) -> impl Fn(&Connection, &[HashId]) -> hydrus_store::Result<HashSet<HashId>> + '_ {
    move |_, candidates| {
        Ok(candidates
            .iter()
            .copied()
            .filter(|h| set.contains(h))
            .collect())
    }
}

impl PotentialsQuery {
    /// The query for a search as a duplicates page or rule stores it (its
    /// file domain is the first file search's).
    pub fn from_search(
        snapshot: &Snapshot,
        search: &DuplicatesSearch,
    ) -> hydrus_store::Result<Self> {
        Ok(Self {
            scope: crate::engine::rule_scope(snapshot, &search.search_1.location)?,
            kind: search.kind,
            pixel_duplicates: search.pixel_duplicates,
            max_hamming_distance: search.max_hamming_distance,
            search_1: search.search_1.clone(),
            search_2: search.search_2.clone(),
        })
    }

    /// Every potential pair in the search's file domain.
    pub fn space(
        &self,
        conn: &Connection,
        snapshot: &Snapshot,
    ) -> hydrus_store::Result<Vec<PairRow>> {
        pair_space(conn, snapshot, &self.scope)
    }

    /// Run the file searches once, for [`PreparedQuery::count_matching`]; a
    /// file search that fails is the inner error.
    pub fn prepare(
        &self,
        conn: &Connection,
        snapshot: &Snapshot,
    ) -> hydrus_store::Result<Result<PreparedQuery, SearchError>> {
        let searched = run(&self.search_1, conn, snapshot).and_then(|one| {
            let two = match self.kind {
                PairSearchKind::BothFilesMatchDifferentSearches => {
                    run(&self.search_2, conn, snapshot)?
                }
                _ => None,
            };
            Ok((one, two))
        });
        Ok(searched.map(|(one, two)| PreparedQuery {
            scope: self.scope.clone(),
            kind: self.kind,
            pixel_duplicates: self.pixel_duplicates,
            max_hamming_distance: self.max_hamming_distance,
            one,
            two,
        }))
    }

    /// How many potential pairs it finds; `None` if a file search fails.
    pub fn count(
        &self,
        conn: &Connection,
        snapshot: &Snapshot,
    ) -> hydrus_store::Result<Option<usize>> {
        Ok(self
            .with_search(conn, snapshot, |search| {
                Ok(hydrus_store::duplicates::potential_pairs(conn, snapshot, search)?.len())
            })?
            .ok())
    }

    /// Run the file searches, then `f` on the search they make; a file
    /// search that fails is the inner error.
    pub fn with_search<T>(
        &self,
        conn: &Connection,
        snapshot: &Snapshot,
        f: impl FnOnce(&PotentialsSearch<'_>) -> hydrus_store::Result<T>,
    ) -> hydrus_store::Result<Result<T, SearchError>> {
        let searched = run(&self.search_1, conn, snapshot).and_then(|one| {
            let two = match self.kind {
                PairSearchKind::BothFilesMatchDifferentSearches => {
                    run(&self.search_2, conn, snapshot)?
                }
                _ => None,
            };
            Ok((one, two))
        });
        let (one, two) = match searched {
            Ok(matched) => matched,
            Err(e) => return Ok(Err(e)),
        };
        let one = one.as_ref().map(in_set);
        let two = two.as_ref().map(in_set);
        let search = PotentialsSearch {
            scope: self.scope.clone(),
            kind: self.kind,
            pixel_duplicates: self.pixel_duplicates,
            max_hamming_distance: self.max_hamming_distance,
            search_1: one.as_ref().map(|f| f as &FileFilter<'_>),
            search_2: two.as_ref().map(|f| f as &FileFilter<'_>),
        };
        f(&search).map(Ok)
    }
}
