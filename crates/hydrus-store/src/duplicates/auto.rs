//! Duplicates auto-resolution rules and their pair queues.
//!
//! A rule searches the potential duplicate pairs in its file domain, tests
//! each matching pair with its comparators to decide which file is A (the
//! better), and then either applies its action to the pair (fully automatic)
//! or queues it for a human to approve or deny (semi-automatic). Running
//! rules is `hydrus-duplicates`' job; this module stores them.
//!
//! Every potential pair in a rule's domain has one status for that rule, as
//! in the reference ([`PairStatus`]). A pair is keyed by its two duplicate
//! groups (the reference's media ids), so it follows the groups as their
//! kings change. Actioned pairs are no longer potential; they are kept as a
//! log by their files.
//!
//! The reference moves pairs between statuses eagerly, whenever potential
//! pairs are added or removed anywhere. Here a rule's queue is reconciled
//! with the potential pairs ([`resync`]) before the rule does any work, which
//! gives the same queue at the moment it matters with far fewer hooks.

use std::collections::{BTreeMap, BTreeSet, HashSet};

use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};

use hydrus_core::search::comparable::Comparable;
use hydrus_core::search::number::NumberTest;
use hydrus_core::search::predicate::Predicate;
use hydrus_core::{DuplicateType, HashId};

use super::{FileScope, MergeOptions};
use crate::error::{Result, StoreError};

/// When auto-resolution works, and how hard (the reference's
/// `duplicates_auto_resolution_*` options).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct AutoResolutionSettings {
    pub during_active: bool,
    pub during_idle: bool,
    /// How long one burst of work may take.
    pub work_time_ms_active: u32,
    pub work_time_ms_idle: u32,
    /// Rest after a burst, as a percentage of the time it took.
    pub rest_percentage_active: u32,
    pub rest_percentage_idle: u32,
}

impl Default for AutoResolutionSettings {
    /// The reference's defaults.
    fn default() -> Self {
        Self {
            during_active: true,
            during_idle: true,
            work_time_ms_active: 100,
            work_time_ms_idle: 1000,
            rest_percentage_active: 900,
            rest_percentage_idle: 100,
        }
    }
}

impl crate::settings::Setting for AutoResolutionSettings {
    const KEY: &'static str = "duplicates_auto_resolution";
}

/// A rule.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Rule {
    pub name: String,
    pub paused: bool,
    pub mode: OperationMode,
    /// A semi-automatic rule stops testing pairs while this many await
    /// approval.
    pub max_pending_pairs: Option<u32>,
    pub search: RuleSearch,
    /// All must pass, with A and B one way round or the other.
    pub comparators: Vec<Comparator>,
    pub action: RuleAction,
    pub delete_a: bool,
    pub delete_b: bool,
    /// `None`: the client's merge options for the action.
    pub custom_merge: Option<MergeOptions>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OperationMode {
    /// Search and test; a human approves or denies each passing pair.
    SemiAutomatic,
    /// Search, test and apply the action.
    FullyAutomatic,
}

/// The potential pairs a rule looks at.
pub type RuleSearch = hydrus_core::duplicates::DuplicatesSearch;

/// What a rule does to a pair that passes, A and B as the comparators
/// decided.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RuleAction {
    /// Not related.
    FalsePositive,
    SameQuality,
    Alternate,
    /// A is better.
    Better,
    /// B is better.
    Worse,
}

impl RuleAction {
    pub const fn duplicate_type(self) -> DuplicateType {
        match self {
            RuleAction::FalsePositive => DuplicateType::FalsePositive,
            RuleAction::SameQuality => DuplicateType::SameQuality,
            RuleAction::Alternate => DuplicateType::Alternate,
            RuleAction::Better => DuplicateType::Better,
            RuleAction::Worse => DuplicateType::Worse,
        }
    }

    pub fn from_duplicate_type(t: DuplicateType) -> Option<Self> {
        Some(match t {
            DuplicateType::FalsePositive => RuleAction::FalsePositive,
            DuplicateType::SameQuality => RuleAction::SameQuality,
            DuplicateType::Alternate => RuleAction::Alternate,
            DuplicateType::Better => RuleAction::Better,
            DuplicateType::Worse => RuleAction::Worse,
            _ => return None,
        })
    }
}

/// Which file of the pair a one-file comparator looks at.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LookingAt {
    A,
    B,
    Either,
}

/// A one-file hardcoded test.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OneFileTest {
    JpegIsProgressive,
    JpegIsNotProgressive,
}

/// A two-file hardcoded test.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PairTest {
    FiletypeSame,
    FiletypeDiffers,
    HasExifSame,
    HasIccProfileSame,
    AHasClearlyBetterJpegQuality,
    AHasSameOrBetterMetadataFlags,
    AHasIccProfileIfBDoes,
}

impl PairTest {
    pub const fn from_code(code: i64) -> Option<Self> {
        Some(match code {
            0 => PairTest::FiletypeSame,
            1 => PairTest::FiletypeDiffers,
            2 => PairTest::HasExifSame,
            3 => PairTest::HasIccProfileSame,
            4 => PairTest::AHasClearlyBetterJpegQuality,
            5 => PairTest::AHasSameOrBetterMetadataFlags,
            6 => PairTest::AHasIccProfileIfBDoes,
            _ => return None,
        })
    }
}

/// A test of a pair, A and B one particular way round.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Comparator {
    /// The file(s) match all of `predicates` (tested in memory).
    OneFileMetadata {
        looking_at: LookingAt,
        predicates: Vec<Predicate>,
    },
    OneFileHardcoded {
        looking_at: LookingAt,
        test: OneFileTest,
    },
    /// A's property against B's times `multiplier` plus `delta`, by
    /// `test`'s operator (its value is unused).
    RelativeFileInfo {
        property: Comparable,
        test: NumberTest,
        multiplier: f64,
        delta: i64,
    },
    Pair(PairTest),
    /// A and B are visual duplicates with at least this confidence
    /// (`VISUAL_DUPLICATES_RESULT_*`: 40, 60, 85, 100).
    VisualDuplicates {
        confidence: u8,
    },
    Or(Vec<Comparator>),
    And(Vec<Comparator>),
}

/// A pair's status for a rule (`DUPLICATE_STATUS_*`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PairStatus {
    DoesNotMatchSearch,
    MatchesSearchNotTested,
    FailedTest,
    Actioned,
    NotSearched,
    /// Passed; a semi-automatic rule waits for a human.
    ReadyToAction,
    Denied,
}

impl PairStatus {
    pub const ALL: [PairStatus; 7] = [
        PairStatus::DoesNotMatchSearch,
        PairStatus::MatchesSearchNotTested,
        PairStatus::FailedTest,
        PairStatus::Actioned,
        PairStatus::NotSearched,
        PairStatus::ReadyToAction,
        PairStatus::Denied,
    ];

    pub const fn code(self) -> u8 {
        match self {
            PairStatus::DoesNotMatchSearch => 0,
            PairStatus::MatchesSearchNotTested => 1,
            PairStatus::FailedTest => 2,
            PairStatus::Actioned => 3,
            PairStatus::NotSearched => 4,
            PairStatus::ReadyToAction => 5,
            PairStatus::Denied => 6,
        }
    }

    pub fn from_code(code: i64) -> Option<Self> {
        Self::ALL.into_iter().find(|s| i64::from(s.code()) == code)
    }

    /// The reference's description.
    pub const fn description(self) -> &'static str {
        match self {
            PairStatus::DoesNotMatchSearch => "Did not match search",
            PairStatus::MatchesSearchNotTested => "Matches search, not yet tested",
            PairStatus::FailedTest => "Matches search, failed test",
            PairStatus::Actioned => "Actioned",
            PairStatus::NotSearched => "Not searched",
            PairStatus::ReadyToAction => "Matches search, passed test, ready to action",
            PairStatus::Denied => "User denied",
        }
    }
}

/// A pair of duplicate groups (smaller id first), as the potential pairs
/// table keys them.
pub type GroupPair = (i64, i64);

fn rule_json(rule: &Rule) -> String {
    serde_json::to_string(rule).expect("a rule serialises")
}

fn parse_rule(text: &str) -> Result<Rule> {
    serde_json::from_str(text).map_err(|e| StoreError::Corrupt(format!("a stored rule: {e}")))
}

/// Every rule, by id, in id order.
pub fn rules(conn: &Connection) -> Result<Vec<(i64, Rule)>> {
    let mut stmt =
        conn.prepare_cached("SELECT rule_id, rule FROM dup_auto_rules ORDER BY rule_id")?;
    let rows = stmt
        .query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    rows.into_iter()
        .map(|(id, text)| Ok((id, parse_rule(&text)?)))
        .collect()
}

pub fn rule(conn: &Connection, rule_id: i64) -> Result<Option<Rule>> {
    conn.query_row(
        "SELECT rule FROM dup_auto_rules WHERE rule_id = ?",
        [rule_id],
        |r| r.get::<_, String>(0),
    )
    .optional()?
    .map(|t| parse_rule(&t))
    .transpose()
}

/// Add a rule (with a given id when migrating), returning its id. Its queue
/// fills on the next [`resync`]. `None` if the name is taken.
pub fn add_rule(conn: &Connection, rule: &Rule, id: Option<i64>) -> Result<Option<i64>> {
    let taken: bool = conn.query_row(
        "SELECT EXISTS (SELECT 1 FROM dup_auto_rules WHERE name = ?)",
        [&rule.name],
        |r| r.get(0),
    )?;
    if taken {
        return Ok(None);
    }
    conn.execute(
        "INSERT INTO dup_auto_rules (rule_id, name, rule) VALUES (?, ?, ?)",
        params![id, rule.name, rule_json(rule)],
    )?;
    Ok(Some(conn.last_insert_rowid()))
}

/// Replace a rule's settings, keeping (or resetting) its progress as the
/// reference does: a changed pair selector re-tests; a changed search
/// re-searches; a semi-automatic rule made fully automatic re-tests the
/// pairs that were waiting for approval (so they are actioned).
pub fn update_rule(conn: &Connection, rule_id: i64, new: &Rule) -> Result<()> {
    let Some(old) = rule(conn, rule_id)? else {
        return Err(StoreError::Invalid(format!(
            "no auto-resolution rule {rule_id}"
        )));
    };
    conn.execute(
        "UPDATE dup_auto_rules SET name = ?, rule = ? WHERE rule_id = ?",
        params![new.name, rule_json(new), rule_id],
    )?;
    if new.mode == OperationMode::FullyAutomatic && old.mode == OperationMode::SemiAutomatic {
        move_all(
            conn,
            rule_id,
            PairStatus::ReadyToAction,
            PairStatus::MatchesSearchNotTested,
        )?;
    }
    if new.comparators != old.comparators {
        reset_test_progress(conn, rule_id)?;
    }
    if new.search != old.search {
        let mut old_elsewhere = old.search.clone();
        old_elsewhere.search_1.location = new.search.search_1.location.clone();
        old_elsewhere.search_2.location = new.search.search_2.location.clone();
        // a new domain alone keeps the search progress of pairs in both
        if old_elsewhere != new.search {
            reset_search_progress(conn, rule_id)?;
        }
    }
    Ok(())
}

pub fn set_paused(conn: &Connection, rule_id: i64, paused: bool) -> Result<()> {
    if let Some(mut r) = rule(conn, rule_id)? {
        r.paused = paused;
        conn.execute(
            "UPDATE dup_auto_rules SET rule = ? WHERE rule_id = ?",
            params![rule_json(&r), rule_id],
        )?;
    }
    Ok(())
}

pub fn delete_rule(conn: &Connection, rule_id: i64) -> Result<()> {
    conn.execute("DELETE FROM dup_auto_rules WHERE rule_id = ?", [rule_id])?;
    conn.execute("DELETE FROM dup_auto_pairs WHERE rule_id = ?", [rule_id])?;
    conn.execute("DELETE FROM dup_auto_actioned WHERE rule_id = ?", [rule_id])?;
    Ok(())
}

/// How many pairs have each status.
pub fn counts(conn: &Connection, rule_id: i64) -> Result<BTreeMap<PairStatus, u64>> {
    let mut out: BTreeMap<PairStatus, u64> = PairStatus::ALL.into_iter().map(|s| (s, 0)).collect();
    let mut stmt = conn.prepare_cached(
        "SELECT status, COUNT(*) FROM dup_auto_pairs WHERE rule_id = ? GROUP BY status",
    )?;
    let mut rows = stmt.query([rule_id])?;
    while let Some(r) = rows.next()? {
        if let Some(status) = PairStatus::from_code(r.get(0)?) {
            out.insert(status, r.get::<_, i64>(1)?.max(0) as u64);
        }
    }
    let actioned: i64 = conn.query_row(
        "SELECT COUNT(*) FROM dup_auto_actioned WHERE rule_id = ?",
        [rule_id],
        |r| r.get(0),
    )?;
    out.insert(PairStatus::Actioned, actioned.max(0) as u64);
    Ok(out)
}

fn move_all(conn: &Connection, rule_id: i64, from: PairStatus, to: PairStatus) -> Result<()> {
    let denied_at = (to == PairStatus::Denied).then(|| hydrus_core::time::TimestampMs::now().0);
    conn.execute(
        "UPDATE dup_auto_pairs SET status = ?, hash_id_a = NULL, hash_id_b = NULL, timestamp_ms = ?
         WHERE rule_id = ? AND status = ?",
        params![to.code(), denied_at, rule_id, from.code()],
    )?;
    Ok(())
}

/// Search every pair again (the reference's "reset search").
pub fn reset_search_progress(conn: &Connection, rule_id: i64) -> Result<()> {
    for status in [
        PairStatus::DoesNotMatchSearch,
        PairStatus::MatchesSearchNotTested,
        PairStatus::FailedTest,
        PairStatus::ReadyToAction,
    ] {
        move_all(conn, rule_id, status, PairStatus::NotSearched)?;
    }
    Ok(())
}

/// Test every matching pair again.
pub fn reset_test_progress(conn: &Connection, rule_id: i64) -> Result<()> {
    for status in [PairStatus::FailedTest, PairStatus::ReadyToAction] {
        move_all(conn, rule_id, status, PairStatus::MatchesSearchNotTested)?;
    }
    Ok(())
}

/// Search denied pairs again.
pub fn reset_denied(conn: &Connection, rule_id: i64) -> Result<()> {
    move_all(conn, rule_id, PairStatus::Denied, PairStatus::NotSearched)
}

/// The potential pairs whose two groups each have a file in `scope`
/// (`FilterMediaIdPairs`).
pub fn potential_pairs_in_scope(
    conn: &Connection,
    scope: &FileScope,
) -> Result<HashSet<GroupPair>> {
    let pairs: Vec<GroupPair> = {
        let mut stmt =
            conn.prepare_cached("SELECT smaller_group_id, larger_group_id FROM potential_pairs")?;
        stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<rusqlite::Result<_>>()?
    };
    if matches!(scope, FileScope::AllKnownFiles) {
        return Ok(pairs.into_iter().collect());
    }
    let groups: BTreeSet<i64> = pairs.iter().flat_map(|&(a, b)| [a, b]).collect();
    let members: Vec<(i64, HashId)> = {
        let mut stmt = conn.prepare_cached(
            "SELECT group_id, hash_id FROM dup_group_members WHERE group_id IN rarray(?)
             UNION SELECT group_id, king_hash_id FROM dup_groups WHERE group_id IN rarray(?)",
        )?;
        let ids = crate::master::int_array(groups.iter().copied());
        stmt.query_map(params![ids.clone(), ids], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<rusqlite::Result<_>>()?
    };
    let hash_ids: Vec<HashId> = members.iter().map(|&(_, h)| h).collect();
    let visible = scope.filter(conn, &hash_ids)?;
    let good: HashSet<i64> = members
        .iter()
        .filter(|(_, h)| visible.contains(h))
        .map(|&(g, _)| g)
        .collect();
    Ok(pairs
        .into_iter()
        .filter(|(a, b)| good.contains(a) && good.contains(b))
        .collect())
}

/// Reconcile a rule's queue with the potential pairs in its domain: pairs
/// that are no longer potential (or have left the domain) are dropped, and
/// new ones are added unsearched. Returns (removed, added).
pub fn resync(conn: &Connection, rule_id: i64, scope: &FileScope) -> Result<(usize, usize)> {
    let valid = potential_pairs_in_scope(conn, scope)?;
    let have: HashSet<GroupPair> = {
        let mut stmt = conn.prepare_cached(
            "SELECT smaller_group_id, larger_group_id FROM dup_auto_pairs WHERE rule_id = ?",
        )?;
        stmt.query_map([rule_id], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<rusqlite::Result<_>>()?
    };
    let mut removed = 0;
    {
        let mut delete = conn.prepare_cached(
            "DELETE FROM dup_auto_pairs WHERE rule_id = ? AND smaller_group_id = ? AND larger_group_id = ?",
        )?;
        for pair in have.difference(&valid) {
            removed += delete.execute(params![rule_id, pair.0, pair.1])?;
        }
    }
    let mut added = 0;
    {
        let mut insert = conn.prepare_cached(
            "INSERT OR IGNORE INTO dup_auto_pairs (rule_id, smaller_group_id, larger_group_id, status) VALUES (?, ?, ?, ?)",
        )?;
        for pair in valid.difference(&have) {
            added += insert.execute(params![
                rule_id,
                pair.0,
                pair.1,
                PairStatus::NotSearched.code()
            ])?;
        }
    }
    Ok((removed, added))
}

/// Up to `limit` unsearched pairs.
pub fn unsearched_pairs(conn: &Connection, rule_id: i64, limit: usize) -> Result<Vec<GroupPair>> {
    pairs_with_status(conn, rule_id, PairStatus::NotSearched, Some(limit))
}

/// Pairs with a status (in key order), up to `limit`.
pub fn pairs_with_status(
    conn: &Connection,
    rule_id: i64,
    status: PairStatus,
    limit: Option<usize>,
) -> Result<Vec<GroupPair>> {
    let mut stmt = conn.prepare_cached(
        "SELECT smaller_group_id, larger_group_id FROM dup_auto_pairs
         WHERE rule_id = ? AND status = ? ORDER BY smaller_group_id, larger_group_id LIMIT ?",
    )?;
    let limit = limit.map_or(-1, |l| i64::try_from(l).unwrap_or(i64::MAX));
    Ok(stmt
        .query_map(params![rule_id, status.code(), limit], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })?
        .collect::<rusqlite::Result<_>>()?)
}

/// Set pairs' status (not [`PairStatus::ReadyToAction`] or
/// [`PairStatus::Actioned`]: see [`set_ready_to_action`] and
/// [`record_actioned`]).
pub fn set_status(
    conn: &Connection,
    rule_id: i64,
    pairs: &[GroupPair],
    status: PairStatus,
) -> Result<()> {
    assert!(
        !matches!(status, PairStatus::ReadyToAction | PairStatus::Actioned),
        "pairs become ready to action or actioned with their files"
    );
    let denied_at = (status == PairStatus::Denied).then(|| hydrus_core::time::TimestampMs::now().0);
    let mut stmt = conn.prepare_cached(
        "INSERT INTO dup_auto_pairs (rule_id, smaller_group_id, larger_group_id, status, timestamp_ms) VALUES (?, ?, ?, ?, ?)
         ON CONFLICT (rule_id, smaller_group_id, larger_group_id) DO UPDATE SET
            status = excluded.status, hash_id_a = NULL, hash_id_b = NULL, timestamp_ms = excluded.timestamp_ms",
    )?;
    for &(a, b) in pairs {
        stmt.execute(params![rule_id, a, b, status.code(), denied_at])?;
    }
    Ok(())
}

/// Drop pairs from a rule's queue (they are no longer potential pairs).
pub fn remove_pairs(conn: &Connection, rule_id: i64, pairs: &[GroupPair]) -> Result<()> {
    let mut stmt = conn.prepare_cached(
        "DELETE FROM dup_auto_pairs WHERE rule_id = ? AND smaller_group_id = ? AND larger_group_id = ?",
    )?;
    for &(a, b) in pairs {
        stmt.execute(params![rule_id, a, b])?;
    }
    Ok(())
}

/// A pair passed a semi-automatic rule's test: it waits, with A and B.
pub fn set_ready_to_action(
    conn: &Connection,
    rule_id: i64,
    pair: GroupPair,
    a: HashId,
    b: HashId,
) -> Result<()> {
    conn.execute(
        "INSERT INTO dup_auto_pairs (rule_id, smaller_group_id, larger_group_id, status, hash_id_a, hash_id_b) VALUES (?, ?, ?, ?, ?, ?)
         ON CONFLICT (rule_id, smaller_group_id, larger_group_id) DO UPDATE SET
            status = excluded.status, hash_id_a = excluded.hash_id_a, hash_id_b = excluded.hash_id_b, timestamp_ms = NULL",
        params![rule_id, pair.0, pair.1, PairStatus::ReadyToAction.code(), a, b],
    )?;
    Ok(())
}

/// The pairs waiting for approval, as (A, B), up to `limit`.
pub fn pending_pairs(
    conn: &Connection,
    rule_id: i64,
    limit: Option<usize>,
) -> Result<Vec<(GroupPair, HashId, HashId)>> {
    let mut stmt = conn.prepare_cached(
        "SELECT smaller_group_id, larger_group_id, hash_id_a, hash_id_b FROM dup_auto_pairs
         WHERE rule_id = ? AND status = ? ORDER BY smaller_group_id, larger_group_id LIMIT ?",
    )?;
    let limit = limit.map_or(-1, |l| i64::try_from(l).unwrap_or(i64::MAX));
    Ok(stmt
        .query_map(
            params![rule_id, PairStatus::ReadyToAction.code(), limit],
            |r| Ok(((r.get(0)?, r.get(1)?), r.get(2)?, r.get(3)?)),
        )?
        .collect::<rusqlite::Result<_>>()?)
}

/// A pair was actioned: it leaves the queue and joins the log.
pub fn record_actioned(
    conn: &Connection,
    rule_id: i64,
    pair: GroupPair,
    a: HashId,
    b: HashId,
    duplicate_type: DuplicateType,
    timestamp_ms: i64,
) -> Result<()> {
    conn.execute(
        "DELETE FROM dup_auto_pairs WHERE rule_id = ? AND smaller_group_id = ? AND larger_group_id = ?",
        params![rule_id, pair.0, pair.1],
    )?;
    conn.execute(
        "INSERT INTO dup_auto_actioned (rule_id, hash_id_a, hash_id_b, duplicate_type, timestamp_ms) VALUES (?, ?, ?, ?, ?)",
        params![rule_id, a, b, duplicate_type.code(), timestamp_ms],
    )?;
    Ok(())
}

/// The actioned log, newest first: (A, B, duplicate type, when).
pub fn actioned(
    conn: &Connection,
    rule_id: i64,
    limit: Option<usize>,
) -> Result<Vec<(HashId, HashId, DuplicateType, i64)>> {
    let mut stmt = conn.prepare_cached(
        "SELECT hash_id_a, hash_id_b, duplicate_type, timestamp_ms FROM dup_auto_actioned
         WHERE rule_id = ? ORDER BY timestamp_ms DESC LIMIT ?",
    )?;
    let limit = limit.map_or(-1, |l| i64::try_from(l).unwrap_or(i64::MAX));
    let rows = stmt
        .query_map(params![rule_id, limit], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get::<_, i64>(2)?, r.get(3)?))
        })?
        .collect::<rusqlite::Result<Vec<(HashId, HashId, i64, i64)>>>()?;
    rows.into_iter()
        .map(|(a, b, t, when)| {
            let t = u8::try_from(t)
                .ok()
                .and_then(DuplicateType::from_code)
                .ok_or_else(|| StoreError::Corrupt(format!("duplicate type {t}")))?;
            Ok((a, b, t, when))
        })
        .collect()
}

/// A file's metadata changed (its facts were regenerated): its pairs are
/// searched and tested again by every rule (`ResetFileSearchProgress`).
/// File maintenance that regenerates metadata must call this.
pub fn reset_file_search_progress(conn: &Connection, hash_id: HashId) -> Result<()> {
    let group: Option<i64> = conn
        .query_row(
            "SELECT group_id FROM dup_group_members WHERE hash_id = ?1
             UNION SELECT group_id FROM dup_groups WHERE king_hash_id = ?1",
            [hash_id],
            |r| r.get(0),
        )
        .optional()?;
    let Some(group) = group else {
        return Ok(());
    };
    conn.execute(
        "UPDATE dup_auto_pairs SET status = ?, hash_id_a = NULL, hash_id_b = NULL, timestamp_ms = NULL
         WHERE (smaller_group_id = ?2 OR larger_group_id = ?2) AND status IN (?3, ?4, ?5, ?6)",
        params![
            PairStatus::NotSearched.code(),
            group,
            PairStatus::DoesNotMatchSearch.code(),
            PairStatus::MatchesSearchNotTested.code(),
            PairStatus::FailedTest.code(),
            PairStatus::ReadyToAction.code(),
        ],
    )?;
    Ok(())
}

/// The reference's suggested rules ("add suggested": `GetDefaultRule
/// Suggestions`), in order, read from their stored form
/// (`suggested_rules.json`, written by
/// `oracle/record_auto_resolution_summaries.py`).
pub fn suggested_rules() -> Vec<Rule> {
    let stored: Vec<serde_json::Value> = serde_json::from_str(include_str!("suggested_rules.json"))
        .expect("the suggestions are JSON");
    stored
        .iter()
        .filter_map(|tuple| {
            let object =
                hydrus_legacy::serialisable::SerialisableObject::from_tuple_str(&tuple.to_string())
                    .ok()?;
            let legacy =
                hydrus_legacy::objects::auto_resolution::AutoResolutionRule::from_object(&object)
                    .ok()?;
            crate::import::auto_resolution_rule(&legacy, &|_| None).ok()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_references_suggested_rules_are_all_read() {
        let names: Vec<String> = super::suggested_rules()
            .into_iter()
            .map(|r| r.name)
            .collect();
        assert_eq!(
            names,
            [
                "pixel-perfect jpegs vs pngs",
                "pixel-perfect gifs vs pngs",
                "pixel-perfect jpegs vs pngs - except when png is smaller",
                "pixel-perfect gifs vs pngs - except when png is smaller",
                "pixel-perfect pairs",
                "visually similar pairs",
                "visually similar pairs - only earlier imports",
                "near-perfect jpegs vs pngs"
            ]
        );
    }
}
