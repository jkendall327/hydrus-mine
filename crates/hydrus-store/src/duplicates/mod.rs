//! File relationships: duplicate groups, alternates, false positives and
//! potential duplicate pairs. This module reads them; [`write`] changes them
//! and [`merge`] merges metadata between files set as duplicates.
//!
//! Model (tables in `schema.rs`): files that are duplicates of each other
//! form a *duplicate group* with one best file, its *king*. Duplicate groups
//! that are related but not duplicates share an *alternates group*. Two
//! alternates groups can be marked *false positives* of each other. The
//! similar-files search proposes *potential pairs* of duplicate groups, each
//! with a perceptual-hash distance. A file in no duplicate group is its own
//! group of one, and its own king.
//!
//! Everything here is seen through a [`FileScope`]: relationships to files
//! outside it are not reported, and a group whose king is outside it is
//! represented by another of its files that is inside.

use std::collections::{BTreeSet, HashMap, HashSet};

use rusqlite::{Connection, OptionalExtension, params};

use hydrus_core::{HashId, ServiceId};

use self::cache::PairRow;
use crate::error::Result;
use crate::master::id_array;
use crate::store::Snapshot;

pub mod auto;
pub mod cache;
pub mod merge;
pub mod write;

pub use hydrus_core::duplicates::{PairOrder, PairSearchKind, PixelDuplicates};
pub use merge::{DuplicateMergeSettings, MergeOptions, PairDecision, apply_decision};
pub use write::{PairRelationship, RelationshipWriter};

/// A duplicate group's id.
type GroupId = u32;

/// Settings of the duplicate filter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DuplicateFilterSettings {
    /// How many pairs to fetch for one batch of filtering.
    pub max_batch_size: u32,
    /// A batch with at most this many decisions, and no pair skipped by
    /// hand, is committed without asking
    /// (`duplicate_filter_auto_commit_batch_size`).
    #[serde(default = "default_auto_commit")]
    pub auto_commit_batch_size: Option<u32>,
    /// Setting files as alternates merges their metadata too (the reference
    /// does so only in advanced mode).
    #[serde(default)]
    pub merge_alternates: bool,
    #[serde(default)]
    pub scores: ComparisonScores,
}

#[allow(clippy::unnecessary_wraps)] // (serde's default for an option)
fn default_auto_commit() -> Option<u32> {
    Some(1)
}

impl Default for DuplicateFilterSettings {
    fn default() -> Self {
        // the reference's defaults (checked against the fixture's options)
        Self {
            max_batch_size: 100,
            auto_commit_batch_size: default_auto_commit(),
            merge_alternates: false,
            scores: ComparisonScores::default(),
        }
    }
}

/// How much each difference counts towards the file shown being the better
/// of a pair (the options' `duplicate_comparison_score_*`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct ComparisonScores {
    pub higher_filesize: i32,
    pub much_higher_filesize: i32,
    pub higher_resolution: i32,
    pub much_higher_resolution: i32,
    pub more_tags: i32,
    pub older: i32,
    pub nicer_ratio: i32,
    pub has_audio: i32,
    pub higher_jpeg_quality: i32,
    pub much_higher_jpeg_quality: i32,
}

impl Default for ComparisonScores {
    fn default() -> Self {
        Self {
            higher_filesize: 10,
            much_higher_filesize: 20,
            higher_resolution: 20,
            much_higher_resolution: 50,
            more_tags: 8,
            older: 4,
            nicer_ratio: 10,
            has_audio: 20,
            higher_jpeg_quality: 10,
            much_higher_jpeg_quality: 20,
        }
    }
}

impl ComparisonScores {
    /// The options' names for each score, for reading them.
    pub fn by_option_name(&mut self) -> [(&'static str, &mut i32); 10] {
        [
            (
                "duplicate_comparison_score_higher_filesize",
                &mut self.higher_filesize,
            ),
            (
                "duplicate_comparison_score_much_higher_filesize",
                &mut self.much_higher_filesize,
            ),
            (
                "duplicate_comparison_score_higher_resolution",
                &mut self.higher_resolution,
            ),
            (
                "duplicate_comparison_score_much_higher_resolution",
                &mut self.much_higher_resolution,
            ),
            ("duplicate_comparison_score_more_tags", &mut self.more_tags),
            ("duplicate_comparison_score_older", &mut self.older),
            (
                "duplicate_comparison_score_nicer_ratio",
                &mut self.nicer_ratio,
            ),
            ("duplicate_comparison_score_has_audio", &mut self.has_audio),
            (
                "duplicate_comparison_score_higher_jpeg_quality",
                &mut self.higher_jpeg_quality,
            ),
            (
                "duplicate_comparison_score_much_higher_jpeg_quality",
                &mut self.much_higher_jpeg_quality,
            ),
        ]
    }
}

impl crate::settings::Setting for DuplicateFilterSettings {
    const KEY: &'static str = "duplicate_filter";
}

/// The files a relationship query can see.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileScope {
    /// Every file, known or not.
    AllKnownFiles,
    /// Files currently in any of `current`, or deleted from any of `deleted`.
    Domains {
        current: Vec<ServiceId>,
        deleted: Vec<ServiceId>,
    },
}

impl FileScope {
    /// Which of these files are in scope.
    pub fn filter(&self, conn: &Connection, hash_ids: &[HashId]) -> Result<HashSet<HashId>> {
        let (current, deleted) = match self {
            FileScope::AllKnownFiles => return Ok(hash_ids.iter().copied().collect()),
            FileScope::Domains { current, deleted } => (current, deleted),
        };
        if hash_ids.is_empty() {
            return Ok(HashSet::new());
        }
        let mut stmt = conn.prepare_cached(
            "SELECT hash_id FROM file_domain_current WHERE service_id IN rarray(?1) AND hash_id IN rarray(?3)
             UNION
             SELECT hash_id FROM file_domain_deleted WHERE service_id IN rarray(?2) AND hash_id IN rarray(?3)",
        )?;
        let rows = stmt.query_map(
            params![id_array(current), id_array(deleted), id_array(hash_ids)],
            |r| r.get(0),
        )?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    fn contains(&self, conn: &Connection, hash_id: HashId) -> Result<bool> {
        Ok(!self.filter(conn, &[hash_id])?.is_empty())
    }
}

/// A file's relationships, as far as a [`FileScope`] sees them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileRelationships {
    /// The best file of this file's duplicate group (maybe the file itself).
    pub king: HashId,
    pub king_in_scope: bool,
    /// Whether the king is in local file storage (can be fetched).
    pub king_is_local: bool,
    /// One representative of each group this file's group is a potential
    /// duplicate of, sorted by id.
    pub potentials: Vec<HashId>,
    /// One representative of each group this file's alternates group was
    /// marked a false positive of.
    pub false_positives: Vec<HashId>,
    /// One representative of each other group in this file's alternates group.
    pub alternates: Vec<HashId>,
    /// The other files of this file's duplicate group.
    pub duplicates: Vec<HashId>,
}

fn group_of(conn: &Connection, hash_id: HashId) -> Result<Option<GroupId>> {
    Ok(conn
        .prepare_cached("SELECT group_id FROM dup_group_members WHERE hash_id = ?")?
        .query_row([hash_id], |r| r.get(0))
        .optional()?)
}

fn king_of(conn: &Connection, group: GroupId) -> Result<Option<HashId>> {
    Ok(conn
        .prepare_cached("SELECT king_hash_id FROM dup_groups WHERE group_id = ?")?
        .query_row([group], |r| r.get(0))
        .optional()?)
}

fn members_of(conn: &Connection, group: GroupId) -> Result<Vec<HashId>> {
    let mut stmt =
        conn.prepare_cached("SELECT hash_id FROM dup_group_members WHERE group_id = ?")?;
    let rows = stmt.query_map([group], |r| r.get(0))?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

fn alternates_group_of(conn: &Connection, group: GroupId) -> Result<Option<u32>> {
    Ok(conn
        .prepare_cached("SELECT alt_group_id FROM alt_group_members WHERE group_id = ?")?
        .query_row([group], |r| r.get(0))
        .optional()?)
}

fn groups_in_alternates_group(conn: &Connection, alternates_group: u32) -> Result<Vec<GroupId>> {
    let mut stmt =
        conn.prepare_cached("SELECT group_id FROM alt_group_members WHERE alt_group_id = ?")?;
    let rows = stmt.query_map([alternates_group], |r| r.get(0))?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

fn false_positive_alternates_groups(conn: &Connection, alternates_group: u32) -> Result<Vec<u32>> {
    let mut stmt = conn.prepare_cached(
        "SELECT larger_alt_group_id FROM false_positive_pairs WHERE smaller_alt_group_id = ?1
         UNION SELECT smaller_alt_group_id FROM false_positive_pairs WHERE larger_alt_group_id = ?1",
    )?;
    let rows = stmt.query_map([alternates_group], |r| r.get(0))?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

fn potential_partners(conn: &Connection, group: GroupId) -> Result<Vec<GroupId>> {
    let mut stmt = conn.prepare_cached(
        "SELECT larger_group_id FROM potential_pairs WHERE smaller_group_id = ?1
         UNION SELECT smaller_group_id FROM potential_pairs WHERE larger_group_id = ?1",
    )?;
    let rows = stmt.query_map([group], |r| r.get(0))?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

/// The file that stands for a group in a scope: its king if the king is in
/// scope, else the member in scope with the lowest id (the reference picks
/// one at random), else none.
fn representative(conn: &Connection, scope: &FileScope, group: GroupId) -> Result<Option<HashId>> {
    let members = members_of(conn, group)?;
    let visible = scope.filter(conn, &members)?;
    if let Some(king) = king_of(conn, group)?
        && visible.contains(&king)
    {
        return Ok(Some(king));
    }
    Ok(visible.into_iter().min())
}

fn representatives(
    conn: &Connection,
    scope: &FileScope,
    groups: impl IntoIterator<Item = GroupId>,
    exclude: HashId,
) -> Result<Vec<HashId>> {
    let mut out = BTreeSet::new();
    for group in groups {
        if let Some(file) = representative(conn, scope, group)? {
            out.insert(file);
        }
    }
    out.remove(&exclude);
    Ok(out.into_iter().collect())
}

/// A file's relationships. `local_storage` is the "hydrus local file
/// storage" domain, for [`FileRelationships::king_is_local`].
pub fn file_relationships(
    conn: &Connection,
    scope: &FileScope,
    local_storage: ServiceId,
    hash_id: HashId,
) -> Result<FileRelationships> {
    let local = FileScope::Domains {
        current: vec![local_storage],
        deleted: Vec::new(),
    };
    let Some(group) = group_of(conn, hash_id)? else {
        return Ok(FileRelationships {
            king: hash_id,
            king_in_scope: scope.contains(conn, hash_id)?,
            king_is_local: local.contains(conn, hash_id)?,
            potentials: Vec::new(),
            false_positives: Vec::new(),
            alternates: Vec::new(),
            duplicates: Vec::new(),
        });
    };
    let king = king_of(conn, group)?.unwrap_or(hash_id);

    let members = members_of(conn, group)?;
    let mut duplicates: Vec<HashId> = scope
        .filter(conn, &members)?
        .into_iter()
        .filter(|&h| h != hash_id)
        .collect();
    duplicates.sort_unstable();

    let (alternates, false_positives) = match alternates_group_of(conn, group)? {
        None => (Vec::new(), Vec::new()),
        Some(alternates_group) => {
            let others = groups_in_alternates_group(conn, alternates_group)?
                .into_iter()
                .filter(|&g| g != group);
            let alternates = representatives(conn, scope, others, hash_id)?;
            let mut false_positive_groups = Vec::new();
            for other in false_positive_alternates_groups(conn, alternates_group)? {
                if other != alternates_group {
                    false_positive_groups.extend(groups_in_alternates_group(conn, other)?);
                }
            }
            let false_positives = representatives(conn, scope, false_positive_groups, hash_id)?;
            (alternates, false_positives)
        }
    };

    // a potential pair counts only if both kings are in scope
    let own_king_visible = scope.contains(conn, king)?;
    let mut partners = Vec::new();
    for other in potential_partners(conn, group)? {
        if let Some(other_king) = king_of(conn, other)?
            && own_king_visible
            && scope.contains(conn, other_king)?
        {
            partners.push(other);
        }
    }
    let potentials = representatives(conn, scope, partners, hash_id)?;

    Ok(FileRelationships {
        king,
        king_in_scope: scope.contains(conn, king)?,
        king_is_local: local.contains(conn, king)?,
        potentials,
        false_positives,
        alternates,
        duplicates,
    })
}

/// Which files a file search matches, asked in batches. This is where a
/// file search executor plugs in.
pub type FileFilter<'a> = dyn Fn(&Connection, &[HashId]) -> Result<HashSet<HashId>> + 'a;

/// A search over potential duplicate pairs.
pub struct PotentialsSearch<'a> {
    pub scope: FileScope,
    pub kind: PairSearchKind,
    pub pixel_duplicates: PixelDuplicates,
    pub max_hamming_distance: u32,
    /// The first search, or `None` for "every file in scope".
    pub search_1: Option<&'a FileFilter<'a>>,
    /// The second search (only used by
    /// [`PairSearchKind::BothFilesMatchDifferentSearches`]).
    pub search_2: Option<&'a FileFilter<'a>>,
}

impl std::fmt::Debug for PotentialsSearch<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PotentialsSearch")
            .field("scope", &self.scope)
            .field("kind", &self.kind)
            .field("pixel_duplicates", &self.pixel_duplicates)
            .field("max_hamming_distance", &self.max_hamming_distance)
            .field("search_1", &self.search_1.is_some())
            .field("search_2", &self.search_2.is_some())
            .finish()
    }
}

/// A potential duplicate pair, by the kings of its two groups.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PotentialPair {
    pub smaller_king: HashId,
    pub larger_king: HashId,
    pub distance: u32,
    /// The groups (the pair's key).
    pub groups: (i64, i64),
}

/// Every potential pair whose kings are both in scope.
fn pairs_in_scope(
    conn: &Connection,
    snapshot: &Snapshot,
    scope: &FileScope,
) -> Result<Vec<PairRow>> {
    let rows = snapshot.duplicates.rows(conn)?;
    if matches!(scope, FileScope::AllKnownFiles) {
        return Ok(rows.to_vec());
    }
    let kings: Vec<HashId> = rows
        .iter()
        .flat_map(|r| [r.smaller_king, r.larger_king])
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let visible = visible_files(conn, snapshot, scope, &kings)?;
    Ok(rows
        .iter()
        .filter(|r| visible.contains(&r.smaller_king) && visible.contains(&r.larger_king))
        .copied()
        .collect())
}

/// Which of `files` are in `scope`: from the cached domains when they are
/// cached or worth loading, else by looking each file up.
fn visible_files(
    conn: &Connection,
    snapshot: &Snapshot,
    scope: &FileScope,
    files: &[HashId],
) -> Result<HashSet<HashId>> {
    let FileScope::Domains { current, deleted } = scope else {
        return scope.filter(conn, files);
    };
    let domains = snapshot.domains.for_read(conn)?;
    let mut bitmaps = Vec::new();
    for (services, is_deleted) in [(current, false), (deleted, true)] {
        for &service in services {
            let bitmap = if let Some(bitmap) = domains.cached(service, is_deleted) {
                bitmap
            } else {
                let size = domains.size_estimate(conn, service, is_deleted)?;
                if crate::domains::probe_is_cheaper(files.len() as u64, size) {
                    return scope.filter(conn, files);
                }
                domains.files(conn, service, is_deleted)?
            };
            bitmaps.push(bitmap);
        }
    }
    Ok(files
        .iter()
        .copied()
        .filter(|h| bitmaps.iter().any(|b| b.contains(h.get())))
        .collect())
}

/// Every potential pair in a file domain (the reference's
/// `GetPotentialDuplicateIdPairsAndDistances`): the space a search counts
/// through a block at a time.
pub fn pair_space(
    conn: &Connection,
    snapshot: &Snapshot,
    scope: &FileScope,
) -> Result<Vec<PairRow>> {
    pairs_in_scope(conn, snapshot, scope)
}

/// How many of `rows` (pairs of the search's space) the search finds.
pub fn count_matching(
    conn: &Connection,
    search: &PotentialsSearch<'_>,
    rows: &[PairRow],
) -> Result<usize> {
    Ok(matching(conn, search, rows)?.len())
}

/// The potential pairs a search finds.
pub fn potential_pairs(
    conn: &Connection,
    snapshot: &Snapshot,
    search: &PotentialsSearch<'_>,
) -> Result<Vec<PotentialPair>> {
    let in_scope = pairs_in_scope(conn, snapshot, &search.scope)?;
    Ok(matching(conn, search, &in_scope)?
        .iter()
        .map(PotentialPair::from)
        .collect())
}

impl From<&PairRow> for PotentialPair {
    fn from(r: &PairRow) -> Self {
        PotentialPair {
            smaller_king: r.smaller_king,
            larger_king: r.larger_king,
            distance: r.distance,
            groups: r.groups,
        }
    }
}

/// The pairs of `rows` (pairs in the search's scope) the search finds.
fn matching(
    conn: &Connection,
    search: &PotentialsSearch<'_>,
    rows: &[PairRow],
) -> Result<Vec<PairRow>> {
    let rows: Vec<PairRow> = rows
        .iter()
        .filter(|r| match search.pixel_duplicates {
            PixelDuplicates::Required => r.pixel_duplicate,
            PixelDuplicates::Allowed => r.distance <= search.max_hamming_distance,
            PixelDuplicates::Excluded => {
                r.distance <= search.max_hamming_distance && !r.pixel_duplicate
            }
        })
        .copied()
        .collect();
    if rows.is_empty() {
        return Ok(Vec::new());
    }
    let kings: Vec<HashId> = rows
        .iter()
        .flat_map(|r| [r.smaller_king, r.larger_king])
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let matching = |filter: Option<&FileFilter<'_>>| -> Result<Option<HashSet<HashId>>> {
        filter.map(|f| f(conn, &kings)).transpose()
    };
    let one = matching(search.search_1)?;
    let two = match search.kind {
        PairSearchKind::BothFilesMatchDifferentSearches => matching(search.search_2)?,
        _ => None,
    };
    let in_set =
        |set: &Option<HashSet<HashId>>, h: HashId| set.as_ref().is_none_or(|s| s.contains(&h));
    Ok(rows
        .into_iter()
        .filter(|r| {
            let (s, l) = (r.smaller_king, r.larger_king);
            match search.kind {
                PairSearchKind::OneFileMatchesOneSearch => in_set(&one, s) || in_set(&one, l),
                PairSearchKind::BothFilesMatchOneSearch => in_set(&one, s) && in_set(&one, l),
                PairSearchKind::BothFilesMatchDifferentSearches => {
                    (in_set(&one, s) && in_set(&two, l)) || (in_set(&two, s) && in_set(&one, l))
                }
            }
        })
        .collect())
}

/// Which of the matching pairs to return.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PairSelection {
    /// The first `max` pairs in order.
    Batch { max: usize },
    /// Every matching pair among the first pair's whole network of potential
    /// duplicates (the groups reachable through potential pairs).
    Group,
}

/// Potential pairs for a duplicate filter: the pairs `search` finds, chosen
/// and ordered as asked, each as its two kings, the smaller group's first
/// (the duplicate filter then decides which to show first).
pub fn select_pairs(
    conn: &Connection,
    snapshot: &Snapshot,
    search: &PotentialsSearch<'_>,
    order: PairOrder,
    ascending: bool,
    selection: PairSelection,
) -> Result<Vec<(HashId, HashId)>> {
    let in_scope = pairs_in_scope(conn, snapshot, &search.scope)?;
    let mut pairs = matching(conn, search, &in_scope)?;
    // a missing or zero size counts as 1, as in the reference
    let sizes = |p: &PairRow| (p.smaller_size.max(1), p.larger_size.max(1));

    // ties break by the pair's groups (the reference's media ids: its
    // stable sort keeps the order the database gave, by those), so the
    // order is deterministic
    let ids = |p: &PairRow| p.groups;
    match order {
        PairOrder::Random => {
            use rand::seq::SliceRandom as _;
            pairs.shuffle(&mut rand::rng());
        }
        PairOrder::Similarity => {
            // distance, then the ratio of the sizes, compared exactly
            pairs.sort_by(|x, y| {
                let ratio = |p: &PairRow| {
                    let (a, b) = sizes(p);
                    (u128::from(a.max(b)), u128::from(a.min(b)))
                };
                let ((xn, xd), (yn, yd)) = (ratio(x), ratio(y));
                x.distance
                    .cmp(&y.distance)
                    .then((xn * yd).cmp(&(yn * xd)))
                    .then(ids(x).cmp(&ids(y)))
            });
        }
        PairOrder::MaxFilesize | PairOrder::MinFilesize => {
            pairs.sort_by_key(|p| {
                let (a, b) = sizes(p);
                let (big, small) = (a.max(b), a.min(b));
                let key = if order == PairOrder::MaxFilesize {
                    (big, small)
                } else {
                    (small, big)
                };
                (key, ids(p))
            });
        }
    }
    if order != PairOrder::Random && !ascending {
        pairs.reverse();
    }

    let chosen: Vec<PairRow> = match selection {
        PairSelection::Batch { max } => pairs.into_iter().take(max).collect(),
        PairSelection::Group => match pairs.first() {
            None => Vec::new(),
            Some(first) => {
                let network = potential_network(&in_scope, [first.smaller_king, first.larger_king]);
                pairs
                    .into_iter()
                    .filter(|p| {
                        network.contains(&p.smaller_king) && network.contains(&p.larger_king)
                    })
                    .collect()
            }
        },
    };

    Ok(chosen
        .into_iter()
        .map(|p| (p.smaller_king, p.larger_king))
        .collect())
}

/// Every king reachable from `start` through potential pairs.
fn potential_network(rows: &[PairRow], start: [HashId; 2]) -> HashSet<HashId> {
    let mut neighbours: HashMap<HashId, Vec<HashId>> = HashMap::new();
    for r in rows {
        neighbours
            .entry(r.smaller_king)
            .or_default()
            .push(r.larger_king);
        neighbours
            .entry(r.larger_king)
            .or_default()
            .push(r.smaller_king);
    }
    let mut seen: HashSet<HashId> = HashSet::new();
    let mut to_visit: Vec<HashId> = start.to_vec();
    while let Some(king) = to_visit.pop() {
        if seen.insert(king) {
            to_visit.extend(neighbours.get(&king).into_iter().flatten().copied());
        }
    }
    seen
}

/// The kings of a random potential duplicate group, as the Client API's
/// `get_random_potentials` gives them: a random pair the search finds, then
/// every file of a found pair within that pair's whole network of potential
/// duplicates. Empty if the search finds nothing.
pub fn random_potential_group(
    conn: &Connection,
    snapshot: &Snapshot,
    search: &PotentialsSearch<'_>,
) -> Result<Vec<HashId>> {
    use rand::seq::IndexedRandom as _;
    let in_scope = pairs_in_scope(conn, snapshot, &search.scope)?;
    let pairs = matching(conn, search, &in_scope)?;
    let Some(chosen) = pairs.choose(&mut rand::rng()) else {
        return Ok(Vec::new());
    };
    let network = potential_network(&in_scope, [chosen.smaller_king, chosen.larger_king]);
    let kings: BTreeSet<HashId> = pairs
        .iter()
        .filter(|p| network.contains(&p.smaller_king) && network.contains(&p.larger_king))
        .flat_map(|p| [p.smaller_king, p.larger_king])
        .collect();
    Ok(kings.into_iter().collect())
}
