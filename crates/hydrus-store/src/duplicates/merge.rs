//! Merging two files' metadata when they are set as duplicates: the
//! reference's "duplicate metadata merge options". When the duplicate filter
//! says A is better than B, the default is to move B's tags and favourite
//! rating to A, copy its URLs and notes, keep the older modified date, and
//! archive both.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use hydrus_core::notes::{NoteConflict, NoteMerge};
use hydrus_core::service::builtin_keys;
use hydrus_core::tag_filter::TagFilter;
use hydrus_core::{ContentStatus, HashId, ServiceId, ServiceKey, TagId};

use super::write::{PairRelationship, RelationshipWriter};
use crate::content::{ContentWriter, FileTime, MappingAction};
use crate::error::Result;
use crate::media::{self, MediaResult, Rating};
use crate::services::ServiceKind;

/// How metadata flows between the files of a pair (A, B).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MergeAction {
    /// B's metadata is added to A.
    Copy,
    /// B's metadata is added to A and removed from B.
    Move,
    /// Each file gets the other's metadata.
    TwoWay,
}

/// Merges for URLs and modified dates, which can't be moved.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SyncAction {
    Copy,
    TwoWay,
}

/// When to archive the files of a pair.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArchiveSync {
    #[default]
    Never,
    /// If either file is archived, archive the other.
    IfEither,
    /// Archive both.
    Always,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TagMerge {
    pub service: ServiceKey,
    pub action: MergeAction,
    /// Only tags this allows are merged.
    #[serde(default)]
    pub filter: TagFilter,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RatingMerge {
    pub service: ServiceKey,
    pub action: MergeAction,
}

/// What to merge for one kind of relationship. The default merges nothing.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct MergeOptions {
    pub tags: Vec<TagMerge>,
    pub ratings: Vec<RatingMerge>,
    pub notes: Option<MergeAction>,
    /// How incoming notes join a file's existing notes.
    pub note_merge: Option<NoteMerge>,
    pub archive: ArchiveSync,
    pub urls: Option<SyncAction>,
    pub file_modified: Option<SyncAction>,
}

/// Merge options per relationship (setting `duplicate_merge`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DuplicateMergeSettings {
    pub better: MergeOptions,
    pub same_quality: MergeOptions,
    pub alternate: MergeOptions,
}

impl crate::settings::Setting for DuplicateMergeSettings {
    const KEY: &'static str = "duplicate_merge";
}

impl Default for DuplicateMergeSettings {
    /// The reference's defaults.
    fn default() -> Self {
        let key = |k: &[u8]| ServiceKey::new(k.to_vec());
        let options = |action: MergeAction, sync: SyncAction| MergeOptions {
            tags: [builtin_keys::MY_TAGS, builtin_keys::DOWNLOADER_TAGS]
                .into_iter()
                .map(|k| TagMerge {
                    service: key(k),
                    action,
                    filter: TagFilter::default(),
                })
                .collect(),
            ratings: vec![RatingMerge {
                service: key(builtin_keys::FAVOURITES),
                action,
            }],
            notes: Some(match sync {
                SyncAction::Copy => MergeAction::Copy,
                SyncAction::TwoWay => MergeAction::TwoWay,
            }),
            note_merge: Some(NoteMerge {
                extend_existing: true,
                conflict: NoteConflict::Rename,
            }),
            archive: ArchiveSync::Always,
            urls: Some(sync),
            file_modified: Some(sync),
        };
        Self {
            better: options(MergeAction::Move, SyncAction::Copy),
            same_quality: options(MergeAction::TwoWay, SyncAction::TwoWay),
            alternate: MergeOptions::default(),
        }
    }
}

impl DuplicateMergeSettings {
    /// The options for a relationship, if it merges anything.
    pub fn for_relationship(&self, relationship: PairRelationship) -> Option<&MergeOptions> {
        match relationship {
            PairRelationship::Better => Some(&self.better),
            PairRelationship::SameQuality => Some(&self.same_quality),
            PairRelationship::Alternate => Some(&self.alternate),
            PairRelationship::FalsePositive | PairRelationship::Potential => None,
        }
    }
}

/// A decision about a pair of files, as the duplicate filter makes it.
#[derive(Debug, Clone)]
pub struct PairDecision<'o> {
    pub relationship: PairRelationship,
    /// For [`PairRelationship::Better`], the better file.
    pub a: HashId,
    pub b: HashId,
    /// Metadata to merge first, if any.
    pub merge: Option<&'o MergeOptions>,
    pub delete_a: bool,
    pub delete_b: bool,
    /// Recorded as the reason for deleting a file.
    pub deletion_reason: &'o str,
}

/// Carry out a decision: merge metadata, delete files, then set the
/// relationship.
pub fn apply_decision(w: &mut ContentWriter<'_>, decision: &PairDecision<'_>) -> Result<()> {
    let (a, b) = (decision.a, decision.b);
    let batch = media::load(w.conn(), &w.snapshot().services, None, &[a, b])?;
    let find = |id: HashId| batch.results.iter().find(|r| r.hash_id == id);
    let (Some(ma), Some(mb)) = (find(a), find(b)) else {
        return Err(crate::StoreError::Corrupt(format!(
            "no hash for file id {a} or {b}"
        )));
    };
    if let Some(options) = decision.merge {
        let pair = Pair {
            a: ma,
            b: mb,
            tags: &batch.tags,
        };
        merge(w, options, &pair, [decision.delete_a, decision.delete_b])?;
    }
    let combined_local = w.roles().combined_local_media;
    for (media, delete) in [(ma, decision.delete_a), (mb, decision.delete_b)] {
        if delete && media.is_current_in(combined_local) {
            w.delete_files(
                combined_local,
                &[media.hash_id],
                Some(decision.deletion_reason),
            )?;
        }
    }
    let local_storage = w.roles().local_file_storage;
    RelationshipWriter::new(w.conn(), local_storage).set_pair(decision.relationship, a, b)
}

/// The two files as they were before the merge.
struct Pair<'m> {
    a: &'m MediaResult,
    b: &'m MediaResult,
    tags: &'m std::collections::HashMap<TagId, hydrus_core::Tag>,
}

impl Pair<'_> {
    /// A file's current and pending tags on a service that pass `filter`.
    fn tags_of(
        &self,
        media: &MediaResult,
        service: ServiceId,
        filter: &TagFilter,
    ) -> BTreeSet<TagId> {
        let Some(tags) = media.tags.get(&service) else {
            return BTreeSet::new();
        };
        [ContentStatus::Current, ContentStatus::Pending]
            .iter()
            .filter_map(|status| tags.by_status.get(status))
            .flatten()
            .copied()
            .filter(|id| {
                self.tags
                    .get(id)
                    .is_some_and(|tag| filter.tag_ok(tag.as_ref(), false))
            })
            .collect()
    }
}

/// Only an older date replaces a file's modified date.
fn should_update_modified(existing: Option<i64>, new: Option<i64>) -> bool {
    match (existing, new) {
        (_, None) => false,
        (None, Some(_)) => true,
        (Some(existing), Some(new)) => new < existing,
    }
}

fn merge(
    w: &mut ContentWriter<'_>,
    options: &MergeOptions,
    pair: &Pair<'_>,
    [delete_a, delete_b]: [bool; 2],
) -> Result<()> {
    let (a, b) = (pair.a, pair.b);
    let services = w.snapshot().services.clone();

    for merge in &options.tags {
        let Ok(service) = services.by_key(&merge.service) else {
            continue;
        };
        let (add, action) = match service.kind {
            ServiceKind::LocalTags => (MappingAction::Add, merge.action),
            // repository tags can only be proposed, not removed outright
            ServiceKind::TagRepository(_) => (
                MappingAction::Pend,
                if merge.action == MergeAction::Move {
                    MergeAction::Copy
                } else {
                    merge.action
                },
            ),
            _ => continue,
        };
        let first = pair.tags_of(a, service.id, &merge.filter);
        let second = pair.tags_of(b, service.id, &merge.filter);
        for tag in second.difference(&first) {
            w.update_mappings(service.id, &add, *tag, &[a.hash_id])?;
        }
        match action {
            MergeAction::Copy => {}
            MergeAction::TwoWay => {
                for tag in first.difference(&second) {
                    w.update_mappings(service.id, &add, *tag, &[b.hash_id])?;
                }
            }
            MergeAction::Move => {
                for tag in &second {
                    w.update_mappings(service.id, &MappingAction::Delete, *tag, &[b.hash_id])?;
                }
            }
        }
    }

    for merge in &options.ratings {
        let Ok(service) = services.by_key(&merge.service) else {
            continue;
        };
        match service.kind {
            ServiceKind::RatingLike(_) | ServiceKind::RatingNumerical(_) => {
                let value = |m: &MediaResult| match m.ratings.get(&service.id) {
                    Some(Rating::Fraction(f)) => Some(*f),
                    _ => None,
                };
                let (first, second) = (value(a), value(b));
                // a rating spreads only to files rated lower or not at all
                let worth = |source: Option<f64>, dest: Option<f64>| {
                    source.is_some_and(|s| dest.is_none_or(|d| s > d))
                };
                match merge.action {
                    MergeAction::TwoWay => {
                        if worth(first, second) {
                            w.set_rating(service.id, &[b.hash_id], first)?;
                        } else if worth(second, first) {
                            w.set_rating(service.id, &[a.hash_id], second)?;
                        }
                    }
                    MergeAction::Copy => {
                        if worth(second, first) {
                            w.set_rating(service.id, &[a.hash_id], second)?;
                        }
                    }
                    MergeAction::Move => {
                        if second.is_some() {
                            if worth(second, first) {
                                w.set_rating(service.id, &[a.hash_id], second)?;
                            }
                            w.set_rating(service.id, &[b.hash_id], None)?;
                        }
                    }
                }
            }
            ServiceKind::RatingIncDec(_) => {
                let value = |m: &MediaResult| match m.ratings.get(&service.id) {
                    Some(Rating::IncDec(n)) => *n,
                    _ => 0,
                };
                let (first, second) = (value(a), value(b));
                let sum = first + second;
                match merge.action {
                    MergeAction::TwoWay => {
                        if second > 0 {
                            w.set_incdec(service.id, &[a.hash_id], sum)?;
                        }
                        if first > 0 {
                            w.set_incdec(service.id, &[b.hash_id], sum)?;
                        }
                    }
                    MergeAction::Copy => {
                        if second > 0 {
                            w.set_incdec(service.id, &[a.hash_id], sum)?;
                        }
                    }
                    MergeAction::Move => {
                        if second > 0 {
                            w.set_incdec(service.id, &[a.hash_id], sum)?;
                            w.set_incdec(service.id, &[b.hash_id], 0)?;
                        }
                    }
                }
            }
            _ => {}
        }
    }

    if let Some(action) = options.notes {
        let notes_of =
            |m: &MediaResult| -> BTreeMap<String, String> { m.notes.iter().cloned().collect() };
        let (first, second) = (notes_of(a), notes_of(b));
        let merged_into = |existing: &BTreeMap<String, String>,
                           incoming: &BTreeMap<String, String>| {
            let incoming: Vec<(String, String)> = incoming
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect();
            options
                .note_merge
                .map_or_else(BTreeMap::new, |m| m.merge(existing, &incoming))
        };
        let for_a = merged_into(&first, &second);
        let for_b = (action == MergeAction::TwoWay).then(|| merged_into(&second, &first));
        for (name, note) in &for_a {
            w.set_note(a.hash_id, name, note)?;
        }
        if let Some(for_b) = for_b {
            for (name, note) in &for_b {
                w.set_note(b.hash_id, name, note)?;
            }
        }
        if action == MergeAction::Move {
            for name in second.keys() {
                w.delete_note(b.hash_id, name)?;
            }
        }
    }

    let archive_a = match options.archive {
        ArchiveSync::Never => false,
        ArchiveSync::IfEither => a.inbox && !b.inbox && !delete_a,
        ArchiveSync::Always => a.inbox && !delete_a,
    };
    let archive_b = match options.archive {
        ArchiveSync::Never => false,
        ArchiveSync::IfEither => !a.inbox && b.inbox && !delete_b,
        ArchiveSync::Always => b.inbox && !delete_b,
    };
    for (media, archive) in [(a, archive_a), (b, archive_b)] {
        if archive {
            w.archive(&[media.hash_id])?;
        }
    }

    if let Some(action) = options.file_modified {
        let modified = |m: &MediaResult| m.info.as_ref().and_then(|i| i.file_modified).map(|t| t.0);
        let (first, second) = (modified(a), modified(b));
        if should_update_modified(first, second) {
            w.set_file_time(
                &[a.hash_id],
                &FileTime::FileModified,
                second.unwrap_or_default(),
            )?;
        } else if action == SyncAction::TwoWay && should_update_modified(second, first) {
            w.set_file_time(
                &[b.hash_id],
                &FileTime::FileModified,
                first.unwrap_or_default(),
            )?;
        }
    }

    if let Some(action) = options.urls {
        let urls_of = |m: &MediaResult| -> BTreeSet<String> { m.urls.iter().cloned().collect() };
        let (first, second) = (urls_of(a), urls_of(b));
        sync_urls(w, a, b, &first, &second)?;
        if action == SyncAction::TwoWay {
            sync_urls(w, b, a, &second, &first)?;
        }
    }
    Ok(())
}

/// Give `dest` the URLs of `source` it lacks, and `source`'s older
/// modified dates for their domains.
fn sync_urls(
    w: &mut ContentWriter<'_>,
    dest: &MediaResult,
    source: &MediaResult,
    dest_urls: &BTreeSet<String>,
    source_urls: &BTreeSet<String>,
) -> Result<()> {
    let needed: Vec<String> = source_urls.difference(dest_urls).cloned().collect();
    if needed.is_empty() {
        return Ok(());
    }
    w.add_urls(&[dest.hash_id], &needed)?;
    let domains: BTreeSet<String> = source_urls
        .iter()
        .filter_map(|url| hydrus_core::url::url_domain(url).ok())
        .collect();
    let time_of = |m: &MediaResult, domain: &str| {
        m.domain_modified
            .iter()
            .find(|(d, _)| d == domain)
            .map(|(_, t)| t.0)
    };
    for domain in domains {
        let Some(source_time) = time_of(source, &domain) else {
            continue;
        };
        if should_update_modified(time_of(dest, &domain), Some(source_time)) {
            w.set_file_time(
                &[dest.hash_id],
                &FileTime::DomainModified(domain),
                source_time,
            )?;
        }
    }
    Ok(())
}
