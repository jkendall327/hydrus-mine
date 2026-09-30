//! Duplicate metadata merge options (`ClientDuplicates.DuplicateContentMergeOptions`,
//! type 43) and the note import options inside them
//! (`NoteImportOptions.NoteImportOptions`, type 153).
//!
//! The client options keep one merge options object per duplicate
//! relationship (better, same quality, alternate) under
//! `duplicate_action_options`.

use hydrus_core::ServiceKey;

use super::tag_filter::TagFilter;
use super::util::{
    DecodeResult, boolean, int, list, malformed, opt_string, service_key, string, tuple,
};
use crate::pyjson::PyJson;
use crate::serialisable::{SerialisableObject, SerialisableType};

const KIND: SerialisableType = SerialisableType::DUPLICATE_CONTENT_MERGE_OPTIONS;
const NOTES_KIND: SerialisableType = SerialisableType::NOTE_IMPORT_OPTIONS;

/// `HC.CONTENT_MERGE_ACTION_*`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MergeAction {
    /// From the worse file to the better (0).
    Copy,
    /// Copy, then remove from the worse file (1).
    Move,
    /// Both ways (2).
    TwoWay,
    /// Nothing (3).
    None,
}

impl MergeAction {
    fn from_code(code: i64) -> DecodeResult<Self> {
        Ok(match code {
            0 => MergeAction::Copy,
            1 => MergeAction::Move,
            2 => MergeAction::TwoWay,
            3 => MergeAction::None,
            other => return Err(malformed(KIND, format!("unknown merge action {other}"))),
        })
    }
}

/// `ClientDuplicates.SYNC_ARCHIVE_*`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArchiveSync {
    None,
    IfOneDoBoth,
    DoBothRegardless,
}

/// How incoming notes join existing ones.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoteImportOptions {
    pub get_notes: bool,
    pub extend_existing_note_if_possible: bool,
    /// `NoteImportOptions.NOTE_IMPORT_CONFLICT_*`: 0 replace, 1 ignore,
    /// 2 append, 3 rename.
    pub conflict_resolution: i64,
    pub name_whitelist: Vec<String>,
    pub all_name_override: Option<String>,
    pub names_to_name_overrides: Vec<(String, String)>,
}

impl NoteImportOptions {
    pub fn from_object(object: &SerialisableObject) -> DecodeResult<Self> {
        object.expect_kind(NOTES_KIND)?;
        object.check_not_future()?;
        let info = object.info();
        let [get, extend, conflict, whitelist, all_override, overrides] =
            tuple::<6>(NOTES_KIND, &info, "note import options")?;
        Ok(NoteImportOptions {
            get_notes: boolean(NOTES_KIND, get, "get notes")?,
            extend_existing_note_if_possible: boolean(NOTES_KIND, extend, "extend")?,
            conflict_resolution: int(NOTES_KIND, conflict, "conflict resolution")?,
            name_whitelist: list(NOTES_KIND, whitelist, "name whitelist")?
                .iter()
                .map(|n| string(NOTES_KIND, n, "name"))
                .collect::<DecodeResult<_>>()?,
            all_name_override: opt_string(NOTES_KIND, all_override, "name override")?,
            names_to_name_overrides: list(NOTES_KIND, overrides, "name overrides")?
                .iter()
                .map(|pair| {
                    let [from, to] = tuple::<2>(NOTES_KIND, pair, "name override")?;
                    Ok((
                        string(NOTES_KIND, from, "name")?,
                        string(NOTES_KIND, to, "name")?,
                    ))
                })
                .collect::<DecodeResult<_>>()?,
        })
    }
}

/// What to merge between two files set as duplicates.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DuplicateMergeOptions {
    pub tag_services: Vec<(ServiceKey, MergeAction, TagFilter)>,
    pub rating_services: Vec<(ServiceKey, MergeAction)>,
    pub sync_notes: MergeAction,
    pub note_import: NoteImportOptions,
    pub sync_archive: ArchiveSync,
    pub sync_urls: MergeAction,
    pub sync_file_modified_date: MergeAction,
}

impl DuplicateMergeOptions {
    /// Decode a merge options object. v688 writes version 8; options are
    /// re-saved with the client options, so older versions don't occur.
    pub fn from_object(object: &SerialisableObject) -> DecodeResult<Self> {
        object.expect_kind(KIND)?;
        object.check_not_future()?;
        if object.version != 8 {
            return Err(crate::serialisable::SerialisableError::UnsupportedVersion {
                kind: KIND,
                version: object.version,
                detail: "merge options are re-saved at version 8 with the client options",
            });
        }
        let info = object.info();
        let [tags, ratings, notes, note_import, archive, urls, modified] =
            tuple::<7>(KIND, &info, "merge options")?;
        let nested = |value: &PyJson, what: &str| -> DecodeResult<SerialisableObject> {
            SerialisableObject::from_tuple(value)
                .map_err(|e| malformed(KIND, format!("{what}: {e}")))
        };
        Ok(DuplicateMergeOptions {
            tag_services: list(KIND, tags, "tag service actions")?
                .iter()
                .map(|row| {
                    let [key, action, filter] = tuple::<3>(KIND, row, "tag service action")?;
                    Ok((
                        service_key(KIND, key, "tag service")?,
                        MergeAction::from_code(int(KIND, action, "merge action")?)?,
                        TagFilter::from_object(&nested(filter, "tag filter")?)?,
                    ))
                })
                .collect::<DecodeResult<_>>()?,
            rating_services: list(KIND, ratings, "rating service actions")?
                .iter()
                .map(|row| {
                    let [key, action] = tuple::<2>(KIND, row, "rating service action")?;
                    Ok((
                        service_key(KIND, key, "rating service")?,
                        MergeAction::from_code(int(KIND, action, "merge action")?)?,
                    ))
                })
                .collect::<DecodeResult<_>>()?,
            sync_notes: MergeAction::from_code(int(KIND, notes, "notes action")?)?,
            note_import: NoteImportOptions::from_object(&nested(note_import, "note options")?)?,
            sync_archive: match int(KIND, archive, "archive action")? {
                0 => ArchiveSync::None,
                1 => ArchiveSync::IfOneDoBoth,
                2 => ArchiveSync::DoBothRegardless,
                other => return Err(malformed(KIND, format!("unknown archive action {other}"))),
            },
            sync_urls: MergeAction::from_code(int(KIND, urls, "urls action")?)?,
            sync_file_modified_date: MergeAction::from_code(int(
                KIND,
                modified,
                "modified date action",
            )?)?,
        })
    }
}
