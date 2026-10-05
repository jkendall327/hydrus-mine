//! The duplicate filter (the reference's `CanvasFilterDuplicates`), as a
//! model: pairs are fetched a batch at a time, the user decides on each in
//! turn, and a batch's decisions are committed together at its end (asking
//! first, unless there are only a few), so that going back can undo any of
//! them until then.
//!
//! What made the reference's filter slow was fetching: it re-reads the
//! whole search space after every commit, then searches it in fragments
//! throttled to half a second each. Here a batch is one read of the cached
//! potential pairs, and a commit is one transaction.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use hydrus_core::HashId;
use hydrus_core::pages::DuplicatesPage;
use hydrus_core::service::builtin_keys;
use hydrus_duplicates::potentials::PotentialsQuery;
use hydrus_duplicates::statements::{self, FastComparison};
use hydrus_search::media::FileFacts;
use hydrus_store::Store;
use hydrus_store::delete_lock::Reinbox;
use hydrus_store::duplicates::auto::{Rule, RuleAction};
use hydrus_store::duplicates::{
    self, ComparisonScores, DuplicateFilterSettings, DuplicateMergeSettings, MergeOptions,
    PairDecision, PairOrder, PairRelationship, PairSelection,
};

/// What the user decided about a pair (the reference's
/// `DuplicatePairDecision*`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    /// Set the relationship between the file shown (A) and the other (B),
    /// deleting either as asked.
    Relationship {
        relationship: PairRelationship,
        delete_a: bool,
        delete_b: bool,
    },
    /// Approved or denied for the auto-resolution rule the pairs came from
    /// (`DuplicatePairDecisionApproveDeny`): the rule's action on the pair
    /// as given, whichever file is shown.
    Review { approved: bool },
    /// Skipped by the user.
    Skip,
    /// Skipped because a file of it was already merged or deleted in this
    /// batch, or can't be shown.
    AutoSkip,
}

impl Decision {
    /// "this is better, delete the other" (the default left click).
    pub const BETTER_DELETE_OTHER: Decision = Decision::Relationship {
        relationship: PairRelationship::Better,
        delete_a: false,
        delete_b: true,
    };
    pub const BETTER_KEEP_BOTH: Decision = Decision::Relationship {
        relationship: PairRelationship::Better,
        delete_a: false,
        delete_b: false,
    };
    pub const SAME_QUALITY: Decision = Decision::Relationship {
        relationship: PairRelationship::SameQuality,
        delete_a: false,
        delete_b: false,
    };
    pub const ALTERNATES: Decision = Decision::Relationship {
        relationship: PairRelationship::Alternate,
        delete_a: false,
        delete_b: false,
    };
    pub const FALSE_POSITIVE: Decision = Decision::Relationship {
        relationship: PairRelationship::FalsePositive,
        delete_a: false,
        delete_b: false,
    };
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Decided {
    /// The file shown when it was decided.
    a: HashId,
    b: HashId,
    decision: Decision,
    /// A custom action's own merge options, in place of the client's.
    merge: Option<MergeOptions>,
}

/// Where the filter is after a step.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    /// A pair is shown.
    Showing,
    /// The batch is done: commit its decisions and continue ([`commit`]),
    /// or go back ([`back`]).
    ///
    /// [`commit`]: DuplicateFilter::commit
    /// [`back`]: DuplicateFilter::back
    Confirm { question: String },
    /// Every pair of the group was skipped (group mode): load a different
    /// group ([`new_group`]), or the same again ([`load_batch`])?
    ///
    /// [`new_group`]: DuplicateFilter::new_group
    /// [`load_batch`]: DuplicateFilter::load_batch
    SkippedGroup,
    /// There are no pairs (left) to filter.
    Finished,
    /// None of a batch's pairs could be shown.
    Undisplayable,
}

/// How a batch ended.
enum End {
    Showing,
    AutoCommit,
    Ask(String),
    AllSkipped,
    Undisplayable,
}

/// The reason recorded for files deleted by a decision.
fn deletion_reason(relationship: PairRelationship, delete_a: bool, delete_b: bool) -> String {
    let mut reason = match relationship {
        PairRelationship::Better => {
            let mut r = "better/worse".to_owned();
            if delete_b {
                r.push_str(", worse file deleted");
            }
            r
        }
        PairRelationship::SameQuality => "same quality".to_owned(),
        PairRelationship::Alternate => "alternates".to_owned(),
        PairRelationship::FalsePositive => "not related/false positive".to_owned(),
        PairRelationship::Potential => "potential duplicates".to_owned(),
    };
    if delete_a && delete_b {
        reason.push_str(", both files deleted");
    }
    format!("Deleted in Duplicate Filter ({reason}).")
}

/// The duplicate filter's state.
pub struct DuplicateFilter {
    store: Arc<Store>,
    query: PotentialsQuery,
    /// Pairs given to filter (A, B), in place of the query's
    /// (`PotentialDuplicatePairFactoryMediaResults`): those not yet given
    /// out as a batch.
    given: Option<Vec<(HashId, HashId)>>,
    /// The rule whose pending pairs these are, approved or denied here
    /// (`PotentialDuplicatePairFactoryAutoResolutionReview`).
    rule: Option<(i64, Rule)>,
    /// Some decision has been committed (`_have_done_work`).
    done_work: bool,
    order: PairOrder,
    ascending: bool,
    group_mode: bool,
    /// Group mode: the kings of the group being filtered, until it has no
    /// pairs left.
    group: Option<HashSet<HashId>>,
    settings: DuplicateFilterSettings,
    batch: Vec<(HashId, HashId)>,
    facts: HashMap<HashId, FileFacts>,
    /// Files of the batch that can be shown.
    displayable: HashSet<HashId>,
    index: usize,
    decisions: Vec<Decided>,
    showing_first: bool,
}

impl std::fmt::Debug for DuplicateFilter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DuplicateFilter")
            .field("batch", &self.batch.len())
            .field("index", &self.index)
            .field("decisions", &self.decisions.len())
            .finish_non_exhaustive()
    }
}

impl DuplicateFilter {
    pub fn new(
        store: Arc<Store>,
        query: PotentialsQuery,
        order: PairOrder,
        ascending: bool,
        group_mode: bool,
    ) -> anyhow::Result<Self> {
        let settings = store.read(hydrus_store::settings::get::<DuplicateFilterSettings>)?;
        Ok(Self {
            store,
            query,
            given: None,
            rule: None,
            done_work: false,
            order,
            ascending,
            group_mode,
            group: None,
            settings,
            batch: Vec::new(),
            facts: HashMap::new(),
            displayable: HashSet::new(),
            index: 0,
            decisions: Vec::new(),
            showing_first: true,
        })
    }

    /// A filter for a duplicates page's search, order and mode.
    pub fn for_page(store: Arc<Store>, page: &DuplicatesPage) -> anyhow::Result<Self> {
        let query = PotentialsQuery::from_search(&store.snapshot(), &page.search)?;
        Self::new(store, query, page.order, page.ascending, page.group_mode)
    }

    /// A filter over these pairs (A, B) alone, as given (a rule preview's,
    /// say), in batches as the filter's settings size them.
    pub fn for_pairs(
        store: Arc<Store>,
        query: PotentialsQuery,
        pairs: Vec<(HashId, HashId)>,
    ) -> anyhow::Result<Self> {
        let mut filter = Self::new(store, query, PairOrder::MinFilesize, true, false)?;
        filter.given = Some(pairs);
        Ok(filter)
    }

    /// A filter over a rule's pending pairs (A, B), approving or denying
    /// them as well as deciding on them.
    pub fn for_review(
        store: Arc<Store>,
        rule_id: i64,
        rule: Rule,
        pairs: Vec<(HashId, HashId)>,
    ) -> anyhow::Result<Self> {
        let query = PotentialsQuery::from_search(&store.snapshot(), &rule.search)?;
        let mut filter = Self::for_pairs(store, query, pairs)?;
        filter.rule = Some((rule_id, rule));
        Ok(filter)
    }

    /// Whether any decision has been committed since opening.
    pub fn done_work(&self) -> bool {
        self.done_work
    }

    /// Whether the pairs are a rule's, to approve or deny.
    pub fn reviewing(&self) -> bool {
        self.rule.is_some()
    }

    /// The pairs to filter next, each in the order it is shown.
    fn fetch(&mut self) -> anyhow::Result<Vec<(HashId, HashId)>> {
        if let Some(given) = &mut self.given {
            let max = (self.settings.max_batch_size as usize).max(1);
            let n = given.len().min(max);
            return Ok(given.drain(..n).collect());
        }
        let snapshot = self.store.snapshot();
        let (order, ascending, group_mode) = (self.order, self.ascending, self.group_mode);
        let max = self.settings.max_batch_size as usize;
        let scores = self.settings.scores;
        loop {
            let group = self.group.clone();
            let (pairs, new_group) = self
                .store
                .read(|conn| {
                    self.query.with_search(conn, &snapshot, |search| {
                        let mut new_group = None;
                        let group = match group {
                            Some(group) => Some(group),
                            None if group_mode => {
                                let kings =
                                    duplicates::random_potential_group(conn, &snapshot, search)?;
                                let kings: HashSet<HashId> = kings.into_iter().collect();
                                new_group = Some(kings.clone());
                                Some(kings)
                            }
                            None => None,
                        };
                        let pairs = match &group {
                            Some(group) => duplicates::select_pairs(
                                conn,
                                &snapshot,
                                search,
                                order,
                                ascending,
                                PairSelection::Batch { max: usize::MAX },
                            )?
                            .into_iter()
                            .filter(|(a, b)| group.contains(a) && group.contains(b))
                            .collect(),
                            None => duplicates::select_pairs(
                                conn,
                                &snapshot,
                                search,
                                order,
                                ascending,
                                PairSelection::Batch { max },
                            )?,
                        };
                        let pairs = statements::ab_order(conn, &snapshot, pairs, &scores)?;
                        Ok((pairs, new_group))
                    })
                })?
                .map_err(|e| anyhow::anyhow!("{e}"))?;
            let was_new = new_group.is_some();
            if let Some(group) = new_group {
                self.group = Some(group);
            }
            if pairs.is_empty() && !was_new && self.group.is_some() {
                // the group is done: on to another
                self.group = None;
                continue;
            }
            return Ok(pairs);
        }
    }

    /// Fetch the next batch and show its first pair that can be shown.
    pub fn load_batch(&mut self) -> anyhow::Result<Step> {
        self.batch = self.fetch()?;
        self.decisions.clear();
        self.index = 0;
        self.showing_first = true;
        if self.batch.is_empty() {
            self.facts.clear();
            return Ok(Step::Finished);
        }
        let ids: Vec<HashId> = self
            .batch
            .iter()
            .flat_map(|&(a, b)| [a, b])
            .collect::<HashSet<_>>()
            .into_iter()
            .collect();
        let snapshot = self.store.snapshot();
        let local = snapshot
            .services
            .builtin(builtin_keys::HYDRUS_LOCAL_FILE_STORAGE)?
            .id;
        let (facts, local) = self.store.read(|conn| {
            Ok((
                hydrus_search::media::load_facts(conn, &snapshot, &ids)?,
                hydrus_store::media::current_in(conn, local, &ids)?,
            ))
        })?;
        // `CanDisplayMedia`: stored here, and not a broken resolution
        self.displayable = ids
            .into_iter()
            .filter(|id| {
                local.contains(id)
                    && facts.get(id).is_some_and(|f| {
                        f.width != Some(0)
                            && f.height != Some(0)
                            && !(f
                                .mime
                                .is_some_and(|m| hydrus_media::mimes::IMAGES.contains(&m))
                                && (f.width.is_none() || f.height.is_none()))
                    })
            })
            .collect();
        self.facts = facts;
        self.next()
    }

    /// Carry on from the pair at `index`: show it or the next that can be
    /// shown, or do what the end of the batch calls for.
    fn next(&mut self) -> anyhow::Result<Step> {
        match self.advance() {
            End::Showing => Ok(Step::Showing),
            End::AutoCommit => self.commit(),
            End::Ask(question) => Ok(Step::Confirm { question }),
            End::AllSkipped if self.group_mode => Ok(Step::SkippedGroup),
            // the same pairs again, as the reference does
            End::AllSkipped => self.load_batch(),
            End::Undisplayable => Ok(Step::Undisplayable),
        }
    }

    /// Files merged away or deleted by this batch's decisions so far.
    fn used(&self) -> HashSet<HashId> {
        let mut used = HashSet::new();
        for d in &self.decisions {
            if let Decision::Relationship {
                relationship,
                delete_a,
                delete_b,
            } = d.decision
            {
                if matches!(
                    relationship,
                    PairRelationship::Better | PairRelationship::SameQuality
                ) || delete_b
                {
                    used.insert(d.b);
                }
                if delete_a {
                    used.insert(d.a);
                }
            }
            // (approved or not, as the reference does)
            if let (Decision::Review { .. }, Some((_, rule))) = (d.decision, &self.rule) {
                if matches!(rule.action, RuleAction::Better | RuleAction::SameQuality)
                    || rule.delete_a
                {
                    used.insert(d.a);
                }
                if rule.delete_b {
                    used.insert(d.b);
                }
            }
        }
        used
    }

    /// Show the pair at `index` or the next that can be shown, skipping
    /// the others; at the end of the batch, say what next.
    fn advance(&mut self) -> End {
        let used = self.used();
        while let Some(&(a, b)) = self.batch.get(self.index) {
            let good = !used.contains(&a)
                && !used.contains(&b)
                && self.displayable.contains(&a)
                && self.displayable.contains(&b);
            if good {
                self.showing_first = true;
                return End::Showing;
            }
            self.decisions.push(Decided {
                a,
                b,
                decision: Decision::AutoSkip,
                merge: None,
            });
            self.index += 1;
        }
        self.end_of_batch()
    }

    /// How many decisions wait to be committed.
    pub fn pending(&self) -> usize {
        self.committable()
    }

    fn committable(&self) -> usize {
        self.decisions
            .iter()
            .filter(|d| {
                matches!(
                    d.decision,
                    Decision::Relationship { .. } | Decision::Review { .. }
                )
            })
            .count()
    }

    fn end_of_batch(&self) -> End {
        let committable = self.committable();
        if committable > 0 {
            let auto_skips = self
                .decisions
                .iter()
                .filter(|d| d.decision == Decision::AutoSkip)
                .count();
            // (not if any pair was skipped by hand)
            let auto_commit = self.settings.auto_commit_batch_size.is_some_and(|n| {
                committable <= n as usize && committable + auto_skips == self.decisions.len()
            });
            if auto_commit {
                return End::AutoCommit;
            }
            return End::Ask(format!(
                "commit {} decisions and continue?",
                hydrus_core::numbers::human_int(committable as u64)
            ));
        }
        if self
            .decisions
            .iter()
            .all(|d| d.decision == Decision::AutoSkip)
        {
            End::Undisplayable
        } else {
            End::AllSkipped
        }
    }

    /// The pair shown, as (the file shown, the other).
    pub fn current(&self) -> Option<(HashId, HashId)> {
        let &(a, b) = self.batch.get(self.index)?;
        Some(if self.showing_first { (a, b) } else { (b, a) })
    }

    /// Whether the first file of the current frozen pair is shown, independent
    /// of comparison scores and the ordering of other batches.
    pub fn showing_file_a(&self) -> bool {
        self.current().is_some() && self.showing_first
    }

    /// The files of the pair shown and the next `pairs` pairs, for
    /// decoding ahead (the reference's `duplicate_filter_prefetch_num_pairs`).
    pub fn upcoming(&self, pairs: usize) -> Vec<HashId> {
        let end = (self.index + 1 + pairs).min(self.batch.len());
        self.batch
            .get(self.index..end)
            .unwrap_or_default()
            .iter()
            .flat_map(|&(a, b)| [a, b])
            .collect()
    }

    /// The facts about a file of the batch.
    pub fn facts(&self, id: HashId) -> Option<&FileFacts> {
        self.facts.get(&id)
    }

    /// Show the other file of the pair.
    pub fn switch(&mut self) {
        if self.index < self.batch.len() {
            self.showing_first = !self.showing_first;
        }
    }

    /// Decide on the pair shown: the file shown is A.
    pub fn decide(&mut self, decision: Decision) -> anyhow::Result<Step> {
        let Some((a, b)) = self.current() else {
            return Ok(Step::Finished);
        };
        // a review decision is on the pair as given
        let (a, b) = match decision {
            Decision::Review { .. } if self.rule.is_none() => return Ok(Step::Showing),
            Decision::Review { .. } => self.batch[self.index],
            _ => (a, b),
        };
        self.decisions.push(Decided {
            a,
            b,
            decision,
            merge: None,
        });
        self.index += 1;
        self.next()
    }

    /// A custom action on the pair shown (`_DoCustomAction`): the file
    /// shown is A, merged by `merge` if given, else as the client's
    /// options for the relationship say.
    pub fn decide_custom(
        &mut self,
        relationship: PairRelationship,
        delete_a: bool,
        delete_b: bool,
        merge: Option<MergeOptions>,
    ) -> anyhow::Result<Step> {
        let Some((a, b)) = self.current() else {
            return Ok(Step::Finished);
        };
        self.decisions.push(Decided {
            a,
            b,
            decision: Decision::Relationship {
                relationship,
                delete_a,
                delete_b,
            },
            merge,
        });
        self.index += 1;
        self.next()
    }

    /// Go back to the last pair decided by hand (the reference's
    /// `_RewindProcessing`), undoing that decision and the automatic skips
    /// after it. False if there is nothing to go back to.
    pub fn back(&mut self) -> bool {
        if self.index == 0
            || !self
                .decisions
                .iter()
                .any(|d| d.decision != Decision::AutoSkip)
        {
            return false;
        }
        while let Some(d) = self.decisions.pop() {
            self.index -= 1;
            if d.decision != Decision::AutoSkip {
                break;
            }
        }
        self.showing_first = true;
        true
    }

    /// Commit the batch's decisions in one transaction, then load the next
    /// batch.
    pub fn commit(&mut self) -> anyhow::Result<Step> {
        self.commit_pending()?;
        self.load_batch()
    }

    /// Commit the batch's decisions in one transaction (on closing).
    pub fn commit_pending(&mut self) -> anyhow::Result<()> {
        let work: Vec<Decided> = self
            .decisions
            .iter()
            .filter(|d| matches!(d.decision, Decision::Relationship { .. }))
            .cloned()
            .collect();
        let merge_settings: DuplicateMergeSettings =
            self.store.read(hydrus_store::settings::get)?;
        let merge_alternates = self.settings.merge_alternates;
        if self.committable() > 0 {
            self.done_work = true;
        }
        if let Some((rule_id, _)) = &self.rule {
            let reviewed = |approved: bool| -> Vec<(HashId, HashId)> {
                self.decisions
                    .iter()
                    .filter(|d| d.decision == Decision::Review { approved })
                    .map(|d| (d.a, d.b))
                    .collect()
            };
            let (approve, deny) = (reviewed(true), reviewed(false));
            hydrus_duplicates::engine::approve(&self.store, *rule_id, &approve)?;
            hydrus_duplicates::engine::deny(&self.store, *rule_id, &deny)?;
        }
        self.store.write_content(move |w| {
            for d in &work {
                let Decision::Relationship {
                    relationship,
                    delete_a,
                    delete_b,
                } = d.decision
                else {
                    continue;
                };
                let merge = match (&d.merge, relationship) {
                    (Some(custom), _) => Some(custom),
                    (None, PairRelationship::Alternate) if !merge_alternates => None,
                    (None, _) => merge_settings.for_relationship(relationship),
                };
                let reason = deletion_reason(relationship, delete_a, delete_b);
                duplicates::apply_decision(
                    w,
                    &PairDecision {
                        relationship,
                        a: d.a,
                        b: d.b,
                        merge,
                        delete_a,
                        delete_b,
                        deletion_reason: &reason,
                        reinbox: Reinbox::AfterDuplicateFilter,
                    },
                )?;
            }
            Ok(())
        })?;
        self.decisions.clear();
        Ok(())
    }

    /// Group mode: drop the group and load another.
    pub fn new_group(&mut self) -> anyhow::Result<Step> {
        self.group = None;
        self.load_batch()
    }

    /// "File One - 3/100 - 2 decisions", as the reference's top bar says.
    pub fn index_text(&self) -> String {
        if self.index >= self.batch.len() {
            return "-".into();
        }
        let label = if self.showing_first {
            "File One"
        } else {
            "File Two"
        };
        let committable = self.committable();
        let decisions = if committable == 0 {
            "no decisions yet".to_owned()
        } else {
            format!(
                "{} decisions",
                hydrus_core::numbers::human_int(committable as u64)
            )
        };
        format!(
            "{label} - {}/{} - {decisions}",
            hydrus_core::numbers::human_int(self.index as u64 + 1),
            hydrus_core::numbers::human_int(self.batch.len() as u64)
        )
    }

    /// The fast comparison of the file shown against the other, ages
    /// relative to `now` (in seconds).
    pub fn comparison(&self, now: i64) -> Option<FastComparison> {
        let (shown, other) = self.current()?;
        Some(statements::fast(
            self.facts.get(&shown)?,
            self.facts.get(&other)?,
            &self.settings.scores,
            now,
        ))
    }

    pub fn store(&self) -> &Arc<Store> {
        &self.store
    }

    pub fn scores(&self) -> &ComparisonScores {
        &self.settings.scores
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deletion_reasons_read_as_the_reference_writes_them() {
        assert_eq!(
            deletion_reason(PairRelationship::Better, false, true),
            "Deleted in Duplicate Filter (better/worse, worse file deleted)."
        );
        assert_eq!(
            deletion_reason(PairRelationship::SameQuality, true, true),
            "Deleted in Duplicate Filter (same quality, both files deleted)."
        );
    }
}
