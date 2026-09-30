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

use crate::error::Result;
use crate::master::id_array;

pub mod auto;
pub mod merge;
pub mod write;

pub use merge::{DuplicateMergeSettings, MergeOptions, PairDecision, apply_decision};
pub use write::{PairRelationship, RelationshipWriter};

/// A duplicate group's id.
type GroupId = u32;

/// Settings of the duplicate filter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DuplicateFilterSettings {
    /// How many pairs to fetch for one batch of filtering.
    pub max_batch_size: u32,
}

impl Default for DuplicateFilterSettings {
    fn default() -> Self {
        // the reference's default (checked against the fixture's options)
        Self {
            max_batch_size: 100,
        }
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

/// How a potential pair must relate to the file searches.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum PairSearchKind {
    /// At least one of the pair matches the first search.
    OneFileMatchesOneSearch,
    /// Both match the first search.
    BothFilesMatchOneSearch,
    /// One matches the first search and the other the second.
    BothFilesMatchDifferentSearches,
}

/// Whether pairs whose files have identical pixels are wanted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum PixelDuplicates {
    /// Only pixel duplicates, whatever their distance.
    Required,
    /// Any pair within the distance.
    Allowed,
    /// Pairs within the distance that are not pixel duplicates.
    Excluded,
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

struct PairRow {
    groups: (i64, i64),
    smaller_king: HashId,
    larger_king: HashId,
    distance: u32,
    pixel_duplicate: bool,
}

/// Every potential pair whose kings are both in scope, with whether the
/// kings are pixel duplicates (same pixel hash and width).
fn pairs_in_scope(conn: &Connection, scope: &FileScope) -> Result<Vec<PairRow>> {
    let mut stmt = conn.prepare_cached(
        "SELECT gs.king_hash_id, gl.king_hash_id, p.distance,
                fs.pixel_hash IS NOT NULL AND fs.pixel_hash = fl.pixel_hash AND fs.width = fl.width,
                p.smaller_group_id, p.larger_group_id
         FROM potential_pairs p
         JOIN dup_groups gs ON gs.group_id = p.smaller_group_id
         JOIN dup_groups gl ON gl.group_id = p.larger_group_id
         LEFT JOIN files fs ON fs.hash_id = gs.king_hash_id
         LEFT JOIN files fl ON fl.hash_id = gl.king_hash_id",
    )?;
    let rows: Vec<PairRow> = stmt
        .query_map([], |r| {
            Ok(PairRow {
                groups: (r.get(4)?, r.get(5)?),
                smaller_king: r.get(0)?,
                larger_king: r.get(1)?,
                distance: r.get(2)?,
                pixel_duplicate: r.get::<_, Option<bool>>(3)?.unwrap_or(false),
            })
        })?
        .collect::<rusqlite::Result<_>>()?;
    if matches!(scope, FileScope::AllKnownFiles) {
        return Ok(rows);
    }
    let kings: Vec<HashId> = rows
        .iter()
        .flat_map(|r| [r.smaller_king, r.larger_king])
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let visible = scope.filter(conn, &kings)?;
    Ok(rows
        .into_iter()
        .filter(|r| visible.contains(&r.smaller_king) && visible.contains(&r.larger_king))
        .collect())
}

/// The potential pairs a search finds.
pub fn potential_pairs(
    conn: &Connection,
    search: &PotentialsSearch<'_>,
) -> Result<Vec<PotentialPair>> {
    let rows: Vec<PairRow> = pairs_in_scope(conn, &search.scope)?
        .into_iter()
        .filter(|r| match search.pixel_duplicates {
            PixelDuplicates::Required => r.pixel_duplicate,
            PixelDuplicates::Allowed => r.distance <= search.max_hamming_distance,
            PixelDuplicates::Excluded => {
                r.distance <= search.max_hamming_distance && !r.pixel_duplicate
            }
        })
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
        .map(|r| PotentialPair {
            smaller_king: r.smaller_king,
            larger_king: r.larger_king,
            distance: r.distance,
            groups: r.groups,
        })
        .collect())
}

/// How pairs are ordered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PairOrder {
    /// By the larger file of the pair, then the smaller.
    MaxFilesize,
    /// By distance, then how different the two files' sizes are.
    Similarity,
    /// By the smaller file of the pair, then the larger.
    MinFilesize,
    Random,
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

/// Size and resolution of a file, for ordering pairs.
#[derive(Debug, Clone, Copy, Default)]
struct FileShape {
    size: u64,
    pixels: u64,
}

fn file_shapes(conn: &Connection, hash_ids: &[HashId]) -> Result<HashMap<HashId, FileShape>> {
    let mut stmt = conn.prepare_cached(
        "SELECT hash_id, size, coalesce(width, 0) * coalesce(height, 0) FROM files WHERE hash_id IN rarray(?)",
    )?;
    let rows = stmt.query_map([id_array(hash_ids)], |r| {
        Ok((
            r.get(0)?,
            FileShape {
                size: r.get::<_, i64>(1)?.max(0) as u64,
                pixels: r.get::<_, i64>(2)?.max(0) as u64,
            },
        ))
    })?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

/// Potential pairs for a duplicate filter: the pairs `search` finds, chosen
/// and ordered as asked, each as `(a, b)` with the likely better file first.
pub fn select_pairs(
    conn: &Connection,
    search: &PotentialsSearch<'_>,
    order: PairOrder,
    ascending: bool,
    selection: PairSelection,
) -> Result<Vec<(HashId, HashId)>> {
    let mut pairs = potential_pairs(conn, search)?;
    let kings: Vec<HashId> = pairs
        .iter()
        .flat_map(|p| [p.smaller_king, p.larger_king])
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let shapes = file_shapes(conn, &kings)?;
    let shape = |h: HashId| shapes.get(&h).copied().unwrap_or_default();
    // a missing or zero size counts as 1, as in the reference
    let size = |h: HashId| shape(h).size.max(1);

    // ties break by ids, so the order is deterministic
    let ids = |p: &PotentialPair| (p.smaller_king, p.larger_king);
    match order {
        PairOrder::Random => {
            use rand::seq::SliceRandom as _;
            pairs.shuffle(&mut rand::rng());
        }
        PairOrder::Similarity => {
            // distance, then the ratio of the sizes, compared exactly
            pairs.sort_by(|x, y| {
                let ratio = |p: &PotentialPair| {
                    let (a, b) = (size(p.smaller_king), size(p.larger_king));
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
                let (a, b) = (size(p.smaller_king), size(p.larger_king));
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

    let chosen: Vec<PotentialPair> = match selection {
        PairSelection::Batch { max } => pairs.into_iter().take(max).collect(),
        PairSelection::Group => match pairs.first() {
            None => Vec::new(),
            Some(first) => {
                let network = potential_network(
                    &pairs_in_scope(conn, &search.scope)?,
                    [first.smaller_king, first.larger_king],
                );
                pairs
                    .into_iter()
                    .filter(|p| {
                        network.contains(&p.smaller_king) && network.contains(&p.larger_king)
                    })
                    .collect()
            }
        },
    };

    // the better-looking file first: more pixels, then a bigger file
    Ok(chosen
        .into_iter()
        .map(|p| {
            let rank = |h: HashId| (shape(h).pixels, shape(h).size, std::cmp::Reverse(h));
            if rank(p.larger_king) > rank(p.smaller_king) {
                (p.larger_king, p.smaller_king)
            } else {
                (p.smaller_king, p.larger_king)
            }
        })
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
    search: &PotentialsSearch<'_>,
) -> Result<Vec<HashId>> {
    use rand::seq::IndexedRandom as _;
    let pairs = potential_pairs(conn, search)?;
    let Some(chosen) = pairs.choose(&mut rand::rng()) else {
        return Ok(Vec::new());
    };
    let network = potential_network(
        &pairs_in_scope(conn, &search.scope)?,
        [chosen.smaller_king, chosen.larger_king],
    );
    let kings: BTreeSet<HashId> = pairs
        .iter()
        .filter(|p| network.contains(&p.smaller_king) && network.contains(&p.larger_king))
        .flat_map(|p| [p.smaller_king, p.larger_king])
        .collect();
    Ok(kings.into_iter().collect())
}
