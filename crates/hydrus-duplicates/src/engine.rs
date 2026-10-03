//! Running the rules (the reference's `DuplicatesAutoResolutionManager`
//! work loop and its database side).
//!
//! Rules work in name order (natural sort). Each unpaused rule first
//! searches its unsearched pairs, then tests its matching pairs one at a
//! time until its work or the time budget runs out. A pair that passes is
//! actioned (fully automatic) or queued for approval (semi-automatic, while
//! fewer than its maximum are waiting).

use std::collections::HashSet;
use std::time::{Duration, Instant};

use rusqlite::{Connection, OptionalExtension};

use hydrus_core::HashId;
use hydrus_core::search::context::{FileSearchContext, LocationContext};
use hydrus_core::search::predicate::{Predicate, SystemPredicate};
use hydrus_search::{Clock, FileSort, SortBy, SortOrder};
use hydrus_store::delete_lock::Reinbox;
use hydrus_store::duplicates::auto::{
    self, GroupPair, OperationMode, PairStatus, Rule, RuleAction,
};
use hydrus_store::duplicates::merge::ArchiveSync;
use hydrus_store::duplicates::{
    self, DuplicateMergeSettings, FileFilter, FileScope, MergeOptions, PairDecision,
    PairRelationship, PotentialsSearch,
};
use hydrus_store::{Snapshot, Store, StoreError, settings};

use crate::selector;

pub type Result<T> = std::result::Result<T, StoreError>;

/// How the pair is oriented before testing: the reference shuffles it.
pub trait Orientation {
    /// Whether to put the second file of the pair first.
    fn swap(&mut self) -> bool;
}

/// The reference's behaviour: a fair coin.
#[derive(Debug, Default)]
pub struct Shuffle;

impl Orientation for Shuffle {
    fn swap(&mut self) -> bool {
        rand::random()
    }
}

/// Never swap (for reproducible tests).
#[derive(Debug, Default)]
pub struct NoShuffle;

impl Orientation for NoShuffle {
    fn swap(&mut self) -> bool {
        false
    }
}

/// A rule's scope, from its search's file domain.
pub fn rule_scope(snapshot: &Snapshot, location: &LocationContext) -> Result<FileScope> {
    if location.is_all_known_files() {
        return Ok(FileScope::AllKnownFiles);
    }
    let ids = |keys: &std::collections::BTreeSet<hydrus_core::ServiceKey>| -> Result<Vec<_>> {
        keys.iter()
            .map(|k| snapshot.services.by_key(k).map(|s| s.id))
            .collect()
    };
    Ok(FileScope::Domains {
        current: ids(location.current())?,
        deleted: ids(location.deleted())?,
    })
}

fn matches_everything(search: &FileSearchContext) -> bool {
    search
        .predicates
        .iter()
        .all(|p| matches!(p, Predicate::System(SystemPredicate::Everything)))
}

/// The files a rule's file search finds (`None`: every file).
fn run_file_search(
    conn: &Connection,
    snapshot: &Snapshot,
    search: &FileSearchContext,
    clock: &Clock,
) -> Result<Option<HashSet<HashId>>> {
    if matches_everything(search) {
        return Ok(None);
    }
    let sort = FileSort {
        by: SortBy::ImportTime,
        order: SortOrder::Ascending,
    };
    match hydrus_search::search_files(conn, snapshot, search, sort, clock) {
        Ok(found) => Ok(Some(found.into_iter().collect())),
        Err(hydrus_search::SearchError::Store(e)) => Err(e),
        Err(e) => Err(StoreError::Invalid(format!("the rule's file search: {e}"))),
    }
}

/// A file filter answering from a search's results.
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

/// The pairs a rule's search finds, by their groups.
fn matching_pairs(
    conn: &Connection,
    snapshot: &Snapshot,
    rule: &Rule,
    scope: &FileScope,
    clock: &Clock,
) -> Result<HashSet<GroupPair>> {
    let search = &rule.search;
    let one = run_file_search(conn, snapshot, &search.search_1, clock)?;
    let two = match search.kind {
        duplicates::PairSearchKind::BothFilesMatchDifferentSearches => {
            run_file_search(conn, snapshot, &search.search_2, clock)?
        }
        _ => None,
    };
    let one_filter = one.as_ref().map(in_set);
    let two_filter = two.as_ref().map(in_set);
    let pairs = duplicates::potential_pairs(
        conn,
        snapshot,
        &PotentialsSearch {
            scope: scope.clone(),
            kind: search.kind,
            pixel_duplicates: search.pixel_duplicates,
            max_hamming_distance: search.max_hamming_distance,
            search_1: one_filter.as_ref().map(|f| f as &FileFilter<'_>),
            search_2: two_filter.as_ref().map(|f| f as &FileFilter<'_>),
        },
    )?;
    Ok(pairs.into_iter().map(|p| p.groups).collect())
}

/// The file standing for a group in `scope`: its king, or else its
/// lowest-id file in scope (`GetBestKingId`; the reference picks at random).
fn best_king(conn: &Connection, group: i64, scope: &FileScope) -> Result<Option<HashId>> {
    let king: Option<HashId> = conn
        .query_row(
            "SELECT king_hash_id FROM dup_groups WHERE group_id = ?",
            [group],
            |r| r.get(0),
        )
        .optional()?;
    let mut members: Vec<HashId> = {
        let mut stmt =
            conn.prepare_cached("SELECT hash_id FROM dup_group_members WHERE group_id = ?")?;
        stmt.query_map([group], |r| r.get(0))?
            .collect::<rusqlite::Result<_>>()?
    };
    members.extend(king);
    let visible = scope.filter(conn, &members)?;
    if let Some(k) = king
        && visible.contains(&k)
    {
        return Ok(Some(k));
    }
    Ok(visible.into_iter().min())
}

/// A rule's untested pairs, ordered by their kings' hashes.
fn untested_in_order(conn: &Connection, rule_id: i64) -> Result<Vec<GroupPair>> {
    let mut stmt = conn.prepare_cached(
        "SELECT q.smaller_group_id, q.larger_group_id, hs.sha256, hl.sha256 FROM dup_auto_pairs AS q
         JOIN dup_groups AS gs ON gs.group_id = q.smaller_group_id
         JOIN dup_groups AS gl ON gl.group_id = q.larger_group_id
         JOIN hashes AS hs ON hs.hash_id = gs.king_hash_id
         JOIN hashes AS hl ON hl.hash_id = gl.king_hash_id
         WHERE q.rule_id = ? AND q.status = ?",
    )?;
    let mut rows: Vec<(GroupPair, [Vec<u8>; 2])> = stmt
        .query_map(
            rusqlite::params![rule_id, PairStatus::MatchesSearchNotTested.code()],
            |r| {
                let (a, b): (Vec<u8>, Vec<u8>) = (r.get(2)?, r.get(3)?);
                let key = if a <= b { [a, b] } else { [b, a] };
                Ok(((r.get(0)?, r.get(1)?), key))
            },
        )?
        .collect::<rusqlite::Result<_>>()?;
    rows.sort_by(|x, y| x.1.cmp(&y.1).then(x.0.cmp(&y.0)));
    Ok(rows.into_iter().map(|(pair, _)| pair).collect())
}

/// Whether a pair is still untested for the rule and still a potential pair.
fn still_untested_potential(conn: &Connection, rule_id: i64, pair: GroupPair) -> Result<bool> {
    Ok(conn.query_row(
        "SELECT EXISTS (SELECT 1 FROM dup_auto_pairs WHERE rule_id = ?1 AND smaller_group_id = ?2
                        AND larger_group_id = ?3 AND status = ?4)
            AND EXISTS (SELECT 1 FROM potential_pairs WHERE smaller_group_id = ?2 AND larger_group_id = ?3)",
        rusqlite::params![rule_id, pair.0, pair.1, PairStatus::MatchesSearchNotTested.code()],
        |r| r.get(0),
    )?)
}

/// Whether a rule has testing to do (`HasResolutionWorkToDo`).
fn has_resolution_work(conn: &Connection, rule_id: i64, rule: &Rule) -> Result<bool> {
    let counts = auto::counts(conn, rule_id)?;
    if rule.mode == OperationMode::SemiAutomatic
        && let Some(max) = rule.max_pending_pairs
        && counts[&PairStatus::ReadyToAction] >= u64::from(max)
    {
        return Ok(false);
    }
    Ok(counts[&PairStatus::MatchesSearchNotTested] > 0)
}

/// The merge options a rule applies: its own, or the client's for the
/// action. As in the reference's auto-resolution, "archive both
/// regardless" only archives both if one was archived.
fn rule_merge_options(rule: &Rule, client: &DuplicateMergeSettings) -> Option<MergeOptions> {
    let relationship = relationship(rule.action);
    let mut options = match &rule.custom_merge {
        Some(o) => o.clone(),
        None => client.for_relationship(relationship)?.clone(),
    };
    if options.archive == ArchiveSync::Always {
        options.archive = ArchiveSync::IfEither;
    }
    Some(options)
}

fn relationship(action: RuleAction) -> PairRelationship {
    match action {
        RuleAction::FalsePositive => PairRelationship::FalsePositive,
        RuleAction::SameQuality => PairRelationship::SameQuality,
        RuleAction::Alternate => PairRelationship::Alternate,
        RuleAction::Better | RuleAction::Worse => PairRelationship::Better,
    }
}

/// What approving `a` and `b` under `rule` would change, as the reference
/// summarises a pending pair (`GetMergeSummaryOnPair`: A and B as listed,
/// even for "B is better", and the rule's deletes).
pub fn planned_changes(
    conn: &Connection,
    snapshot: &Snapshot,
    rule: &Rule,
    a: HashId,
    b: HashId,
) -> Result<Vec<hydrus_store::duplicates::merge::Change>> {
    let client: DuplicateMergeSettings = settings::get(conn)?;
    let options = rule_merge_options(rule, &client);
    let combined_local =
        hydrus_store::content::DomainRoles::new(&snapshot.services)?.combined_local_media;
    hydrus_store::duplicates::merge::plan(
        conn,
        &snapshot.services,
        combined_local,
        a,
        b,
        options.as_ref(),
        [rule.delete_a, rule.delete_b],
        Reinbox::InAutoResolution,
    )
}

/// Apply a rule's action to A and B and log it
/// (`GetDuplicateActionResult` then `SetDuplicatePairStatus`).
fn action_pair(
    store: &Store,
    rule_id: i64,
    rule: &Rule,
    pair: GroupPair,
    a: HashId,
    b: HashId,
) -> Result<()> {
    let reason = format!("duplicates auto-resolution ({})", rule.name);
    let rule = rule.clone();
    store.write_content(move |w| {
        let client: DuplicateMergeSettings = settings::get(w.conn())?;
        let options = rule_merge_options(&rule, &client);
        // "B is better" is "A is better" the other way round
        let (first, second, delete_first, delete_second) = if rule.action == RuleAction::Worse {
            (b, a, rule.delete_b, rule.delete_a)
        } else {
            (a, b, rule.delete_a, rule.delete_b)
        };
        duplicates::apply_decision(
            w,
            &PairDecision {
                relationship: relationship(rule.action),
                a: first,
                b: second,
                merge: options.as_ref(),
                delete_a: delete_first,
                delete_b: delete_second,
                deletion_reason: &reason,
                reinbox: Reinbox::InAutoResolution,
            },
        )?;
        auto::record_actioned(
            w.conn(),
            rule_id,
            pair,
            a,
            b,
            rule.action.duplicate_type(),
            hydrus_core::time::TimestampMs::now().0,
        )
    })
}

/// A rule editor's preview search (`PreviewPanel`): the potential pairs
/// in the rule's domain (within its distance), and those its search
/// matches, as their kings, the pair with the bigger smaller file first
/// (`DUPE_PAIR_SORT_MIN_FILESIZE`, descending), at most `limit` of them.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PreviewSearch {
    pub searched: usize,
    pub matched: Vec<(HashId, HashId)>,
}

pub fn preview_search(
    store: &Store,
    rule: &Rule,
    limit: Option<usize>,
    clock: &Clock,
) -> Result<PreviewSearch> {
    let snapshot = store.snapshot();
    let scope = rule_scope(&snapshot, &rule.search.search_1.location)?;
    store.read(|conn| {
        let searched = duplicates::potential_pairs(
            conn,
            &snapshot,
            &PotentialsSearch {
                scope: scope.clone(),
                kind: duplicates::PairSearchKind::OneFileMatchesOneSearch,
                pixel_duplicates: rule.search.pixel_duplicates,
                max_hamming_distance: rule.search.max_hamming_distance,
                search_1: None,
                search_2: None,
            },
        )?
        .len();
        let mut matched = Vec::new();
        let mut groups: Vec<GroupPair> = matching_pairs(conn, &snapshot, rule, &scope, clock)?
            .into_iter()
            .collect();
        groups.sort_unstable();
        for (one, two) in groups {
            if let (Some(a), Some(b)) =
                (best_king(conn, one, &scope)?, best_king(conn, two, &scope)?)
            {
                matched.push((a, b));
            }
        }
        let ids: Vec<HashId> = matched.iter().flat_map(|&(a, b)| [a, b]).collect();
        let facts = hydrus_search::media::load_facts(conn, &snapshot, &ids)?;
        let size = |h: HashId| {
            facts
                .get(&h)
                .and_then(|f| f.size)
                .filter(|&s| s > 0)
                .unwrap_or(1)
        };
        matched.sort_by_key(|&(a, b)| {
            let (x, y) = (size(a), size(b));
            std::cmp::Reverse((x.min(y), x.max(y)))
        });
        if let Some(limit) = limit {
            matched.truncate(limit);
        }
        Ok(PreviewSearch { searched, matched })
    })
}

/// A preview's test of a pair (`GetMatchingAB`, not shuffled): A and B, if
/// it passes either way round, and whether it passes both ways round
/// (`MatchingPairMatchesBothWaysAround`).
pub fn preview_test(
    store: &Store,
    rule: &Rule,
    first: HashId,
    second: HashId,
    clock: &Clock,
) -> Result<Option<((HashId, HashId), bool)>> {
    let snapshot = store.snapshot();
    let facts =
        store.read(|conn| hydrus_search::media::load_facts(conn, &snapshot, &[first, second]))?;
    let (Some(f1), Some(f2)) = (facts.get(&first), facts.get(&second)) else {
        return Ok(None);
    };
    let file = |id, facts| selector::File { id, facts };
    let mut content = crate::content::StoreContent::new(store);
    let Some(first_is_a) = selector::matching_ab(
        &rule.comparators,
        file(first, f1),
        file(second, f2),
        false,
        clock,
        &mut content,
    ) else {
        return Ok(None);
    };
    let (a, b, fa, fb) = if first_is_a {
        (first, second, f1, f2)
    } else {
        (second, first, f2, f1)
    };
    // (the other way round: B as A)
    let both = rule
        .comparators
        .iter()
        .all(|c| selector::test(c, file(b, fb), file(a, fa), clock, &mut content));
    Ok(Some(((a, b), both)))
}

/// What one pass over the rules did.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct WorkDone {
    pub searched: usize,
    pub tested: usize,
    pub actioned: usize,
    pub queued: usize,
    /// The time budget ran out with work left.
    pub more_to_do: bool,
}

/// Work the rules for up to `budget` (`_WorkRules`).
pub fn work_rules(
    store: &Store,
    budget: Duration,
    orientation: &mut dyn Orientation,
    clock: &Clock,
) -> Result<WorkDone> {
    let stop_at = Instant::now() + budget;
    let mut done = WorkDone::default();
    let mut content = crate::content::StoreContent::new(store);
    let mut rules = store.read(auto::rules)?;
    rules.sort_by_cached_key(|(_, r)| hydrus_core::sort::human_sort_key(&r.name));
    for (rule_id, rule) in rules {
        if rule.paused {
            continue;
        }
        let snapshot = store.snapshot();
        let scope = rule_scope(&snapshot, &rule.search.search_1.location)?;
        {
            let scope = scope.clone();
            store.write(move |ctx| auto::resync(ctx.conn(), rule_id, &scope).map(|_| ()))?;
        }

        // search
        let unsearched = store
            .read(|conn| auto::pairs_with_status(conn, rule_id, PairStatus::NotSearched, None))?;
        if !unsearched.is_empty() {
            let matching =
                store.read(|conn| matching_pairs(conn, &snapshot, &rule, &scope, clock))?;
            let (hits, misses): (Vec<GroupPair>, Vec<GroupPair>) =
                unsearched.iter().partition(|p| matching.contains(p));
            done.searched += unsearched.len();
            store.write(move |ctx| {
                auto::set_status(
                    ctx.conn(),
                    rule_id,
                    &hits,
                    PairStatus::MatchesSearchNotTested,
                )?;
                auto::set_status(ctx.conn(), rule_id, &misses, PairStatus::DoesNotMatchSearch)
            })?;
            if Instant::now() >= stop_at {
                done.more_to_do = true;
                return Ok(done);
            }
        }

        // test
        // pairs are tested in order of their kings' hashes (the reference
        // takes them in whatever order its table gives)
        let mut untested = store
            .read(|conn| untested_in_order(conn, rule_id))?
            .into_iter();
        while store.read(|conn| has_resolution_work(conn, rule_id, &rule))? {
            let Some(pair) = untested.next() else {
                break;
            };
            // a pair that stopped being potential (its groups merged by an
            // earlier action) leaves the queue, as it would in the reference
            if !store.read(|conn| still_untested_potential(conn, rule_id, pair))? {
                store.write(move |ctx| auto::remove_pairs(ctx.conn(), rule_id, &[pair]))?;
                continue;
            }
            let kings = store.read(|conn| {
                Ok((
                    best_king(conn, pair.0, &scope)?,
                    best_king(conn, pair.1, &scope)?,
                ))
            })?;
            let (Some(first), Some(second)) = kings else {
                store.write(move |ctx| {
                    auto::set_status(ctx.conn(), rule_id, &[pair], PairStatus::FailedTest)
                })?;
                continue;
            };
            let facts = store
                .read(|conn| hydrus_search::media::load_facts(conn, &snapshot, &[first, second]))?;
            let (Some(f1), Some(f2)) = (facts.get(&first), facts.get(&second)) else {
                store.write(move |ctx| {
                    auto::set_status(ctx.conn(), rule_id, &[pair], PairStatus::FailedTest)
                })?;
                continue;
            };
            done.tested += 1;
            let (one, two) = (
                selector::File {
                    id: first,
                    facts: f1,
                },
                selector::File {
                    id: second,
                    facts: f2,
                },
            );
            match selector::matching_ab(
                &rule.comparators,
                one,
                two,
                orientation.swap(),
                clock,
                &mut content,
            ) {
                None => store.write(move |ctx| {
                    auto::set_status(ctx.conn(), rule_id, &[pair], PairStatus::FailedTest)
                })?,
                Some(first_is_a) => {
                    let (a, b) = if first_is_a {
                        (first, second)
                    } else {
                        (second, first)
                    };
                    match rule.mode {
                        OperationMode::SemiAutomatic => {
                            store.write(move |ctx| {
                                auto::set_ready_to_action(ctx.conn(), rule_id, pair, a, b)
                            })?;
                            done.queued += 1;
                        }
                        OperationMode::FullyAutomatic => {
                            action_pair(store, rule_id, &rule, pair, a, b)?;
                            done.actioned += 1;
                        }
                    }
                }
            }
            if Instant::now() >= stop_at {
                done.more_to_do = true;
                return Ok(done);
            }
        }
        if Instant::now() >= stop_at {
            done.more_to_do = true;
            return Ok(done);
        }
    }
    Ok(done)
}

/// Approve pairs waiting on a semi-automatic rule: its action is applied to
/// each (`ApprovePendingPairs`). Pairs already merged by an earlier approval
/// are skipped.
pub fn approve(store: &Store, rule_id: i64, pairs: &[(HashId, HashId)]) -> Result<usize> {
    let Some(rule) = store.read(|conn| auto::rule(conn, rule_id))? else {
        return Err(StoreError::Invalid(format!(
            "no auto-resolution rule {rule_id}"
        )));
    };
    let mut n = 0;
    for &(a, b) in pairs {
        let groups = store.read(|conn| Ok((group_of(conn, a)?, group_of(conn, b)?)))?;
        let (Some(ga), Some(gb)) = groups else {
            continue;
        };
        if ga == gb {
            continue;
        }
        action_pair(store, rule_id, &rule, (ga.min(gb), ga.max(gb)), a, b)?;
        n += 1;
    }
    Ok(n)
}

/// Deny pairs waiting on a semi-automatic rule: they are not asked about
/// again unless the denials are reset (`DenyPendingPairs`).
pub fn deny(store: &Store, rule_id: i64, pairs: &[(HashId, HashId)]) -> Result<usize> {
    let mut keys = Vec::new();
    for &(a, b) in pairs {
        let groups = store.read(|conn| Ok((group_of(conn, a)?, group_of(conn, b)?)))?;
        if let (Some(ga), Some(gb)) = groups {
            keys.push((ga.min(gb), ga.max(gb)));
        }
    }
    let n = keys.len();
    store.write(move |ctx| auto::set_status(ctx.conn(), rule_id, &keys, PairStatus::Denied))?;
    Ok(n)
}

/// A file's duplicate group, if it has one.
fn group_of(conn: &Connection, hash_id: HashId) -> Result<Option<i64>> {
    Ok(conn
        .query_row(
            "SELECT group_id FROM dup_group_members WHERE hash_id = ?1
             UNION SELECT group_id FROM dup_groups WHERE king_hash_id = ?1",
            [hash_id],
            |r| r.get(0),
        )
        .optional()?)
}
