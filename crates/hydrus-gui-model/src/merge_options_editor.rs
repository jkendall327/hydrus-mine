//! The duplicate metadata merge options editor (the reference's
//! `EditDuplicateContentMergeOptionsWidget`): for one decision ("this is
//! better", "same quality", "alternates"), which tag services' tags and
//! rating services' ratings move or copy between the files (each tag
//! service through a tag filter), and whether archived status, file
//! modified times, known urls and notes sync. Only "this is better" offers
//! more than copying in both directions; alternates and false positives,
//! unless a custom action, sync no urls, times or notes.

use hydrus_core::ServiceKey;
use hydrus_core::notes::{NoteConflict, NoteMerge};
use hydrus_core::service::ServiceType;
use hydrus_core::tag_filter::TagFilter;
use hydrus_store::duplicates::PairRelationship;
use hydrus_store::duplicates::merge::{
    ArchiveSync, MergeAction, MergeOptions, NoteNames, RatingMerge, SyncAction, TagMerge,
};

pub const TITLE: &str = "edit duplicate merge options";
pub const BOX_TITLE: &str = "duplicate metadata merge options";
pub const NO_MORE_SERVICES: &str =
    "You have no more tag or rating services to add! Try editing the existing ones instead!";
pub const SELECT_SERVICE: &str = "select service";
pub const SELECT_ACTION: &str = "select action";
pub const REMOVE_SELECTED: &str = "Remove all selected?";
pub const FILTER_TITLE: &str = "edit which tags will be merged";
pub const NOTE_SETTINGS_TITLE: &str = "edit note merge options";
pub const RATINGS_NOTE: &str = "If both files have a rating, inc/dec ratings will move/copy via simple addition. Star ratings will overwrite only if the source has a higher rating than the destination.";
pub const ARCHIVE_TOOLTIP: &str = "In the duplicates auto-resolution system, \"always archive both\" (which assumes human eyes) will be treated as \"if one is archived, archive the other\".";
pub const URLS_TOOLTIP: &str = "This will also sync domain modified times for the respective URLs, assuming they are reasonable and older than any existing domain times.";
pub const TAG_COLUMNS: [&str; 3] = ["service name", "action", "tags merged"];
pub const RATING_COLUMNS: [&str; 2] = ["service name", "action"];
pub const ROW_LABELS: [&str; 4] = [
    "sync archived status?: ",
    "sync file modified time?: ",
    "sync known urls?: ",
    "sync notes?: ",
];
pub const ARCHIVE_CHOICES: [&str; 3] = [
    "make no change",
    "if one is archived, archive the other",
    "always archive both",
];
/// The note merge settings' rows.
pub const NOTE_EXTEND_LABEL: &str = "if possible, extend existing notes: ";
pub const NOTE_CONFLICT_LABEL: &str = "if existing note-name conflict, what to do: ";
pub const NOTE_CONFLICTS: [NoteConflict; 4] = [
    NoteConflict::Replace,
    NoteConflict::Ignore,
    NoteConflict::Append,
    NoteConflict::Rename,
];

/// An action as the reference says it (`content_merge_string_lookup`;
/// `None` is "make no change").
pub fn action_label(action: Option<MergeAction>) -> &'static str {
    match action {
        None => "make no change",
        Some(MergeAction::Copy) => "copy from worse to better",
        Some(MergeAction::Move) => "move from worse to better",
        Some(MergeAction::TwoWay) => "copy in both directions",
    }
}

/// An inc/dec rating's action (`content_number_merge_string_lookup`).
pub fn number_action_label(action: MergeAction) -> &'static str {
    match action {
        MergeAction::Copy => "add worse to better",
        MergeAction::Move => "take from worse and add to better",
        MergeAction::TwoWay => "add in both directions",
    }
}

/// A modified time's sync (`content_modified_date_merge_string_lookup`).
pub fn modified_label(action: Option<SyncAction>) -> &'static str {
    match action {
        None => "make no change",
        Some(SyncAction::Copy) => "earlier worse overwrites later better",
        Some(SyncAction::TwoWay) => "both get earliest",
    }
}

fn sync_label(action: Option<SyncAction>) -> &'static str {
    action_label(action.map(|a| match a {
        SyncAction::Copy => MergeAction::Copy,
        SyncAction::TwoWay => MergeAction::TwoWay,
    }))
}

/// A note conflict rule as the note options say it.
pub fn note_conflict_label(conflict: NoteConflict) -> &'static str {
    match conflict {
        NoteConflict::Replace => "replace the existing note",
        NoteConflict::Ignore => "do not add the new note",
        NoteConflict::Append => "append the new note to the end of the existing note",
        NoteConflict::Rename => "add the new note under a new name",
    }
}

/// A decision as the reference names it (`duplicate_type_string_lookup`).
pub fn decision_name(relationship: PairRelationship) -> &'static str {
    match relationship {
        PairRelationship::Better => "this is a better duplicate",
        PairRelationship::SameQuality => "same quality",
        PairRelationship::Alternate => "alternates",
        PairRelationship::FalsePositive => "not related/false positive",
        PairRelationship::Potential => "potential duplicates",
    }
}

/// A service the editor can list: its key, name and type, in the client's
/// order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Service {
    pub key: ServiceKey,
    pub name: String,
    pub kind: ServiceType,
}

/// A select dialog's choices, sorted by label as the reference's sorts
/// them, with the one to preselect.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Choices<T> {
    pub title: &'static str,
    pub choices: Vec<(String, T)>,
    pub preselected: Option<String>,
}

impl<T: Clone> Choices<T> {
    fn new(
        title: &'static str,
        mut choices: Vec<(String, T)>,
        preselected: Option<String>,
    ) -> Self {
        choices.sort_by(|a, b| a.0.cmp(&b.0));
        Self {
            title,
            choices,
            preselected,
        }
    }

    /// One choice is taken without asking.
    pub fn only(&self) -> Option<T> {
        match self.choices.as_slice() {
            [(_, only)] => Some(only.clone()),
            _ => None,
        }
    }
}

/// A sync choice as shown: its options, the one chosen, and whether it can
/// be changed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Choice {
    pub options: Vec<&'static str>,
    pub value: usize,
    pub enabled: bool,
}

#[derive(Debug, Clone)]
pub struct MergeOptionsEditor {
    relationship: PairRelationship,
    for_custom_action: bool,
    services: Vec<Service>,
    tags: Vec<TagMerge>,
    ratings: Vec<RatingMerge>,
    archive: ArchiveSync,
    urls: Option<SyncAction>,
    file_modified: Option<SyncAction>,
    notes: Option<MergeAction>,
    note_merge: NoteMerge,
    note_names: NoteNames,
}

/// The note options merging starts from (`NoteImportOptions()`).
const DEFAULT_NOTE_MERGE: NoteMerge = NoteMerge {
    extend_existing: true,
    conflict: NoteConflict::Rename,
};

impl MergeOptionsEditor {
    /// Edit `options` for `relationship`; services the client no longer
    /// has are dropped.
    pub fn new(
        relationship: PairRelationship,
        options: &MergeOptions,
        for_custom_action: bool,
        services: Vec<Service>,
    ) -> Self {
        let exists = |key: &ServiceKey| services.iter().any(|s| &s.key == key);
        let fixed = matches!(
            relationship,
            PairRelationship::Alternate | PairRelationship::FalsePositive
        ) && !for_custom_action;
        Self {
            relationship,
            for_custom_action,
            tags: options
                .tags
                .iter()
                .filter(|t| exists(&t.service))
                .cloned()
                .collect(),
            ratings: options
                .ratings
                .iter()
                .filter(|r| exists(&r.service))
                .cloned()
                .collect(),
            archive: options.archive,
            urls: if fixed { None } else { options.urls },
            file_modified: if fixed { None } else { options.file_modified },
            notes: if fixed { None } else { options.notes },
            note_merge: options.note_merge.unwrap_or(DEFAULT_NOTE_MERGE),
            note_names: options.note_names.clone(),
            services,
        }
    }

    fn better(&self) -> bool {
        self.relationship == PairRelationship::Better
    }

    /// Whether the urls, modified time and notes choices can be changed.
    fn syncs_enabled(&self) -> bool {
        !matches!(
            self.relationship,
            PairRelationship::Alternate | PairRelationship::FalsePositive
        ) || self.for_custom_action
    }

    /// What it says it edits for.
    pub fn note(&self) -> String {
        let mut note = format!("Editing for \"{}\".", decision_name(self.relationship));
        if !self.better() {
            note.push_str("\n\nNote that this has fewer actions than the \"this is better\" decision. You can mostly just copy in both directions.");
        }
        note
    }

    /// Whether "edit action" shows, for either list (only "this is better"
    /// has more than one action).
    pub fn edit_action_shown(&self) -> bool {
        self.better()
    }

    fn service(&self, key: &ServiceKey) -> Option<&Service> {
        self.services.iter().find(|s| &s.key == key)
    }

    fn name(&self, key: &ServiceKey) -> String {
        self.service(key)
            .map_or_else(|| "missing service!".to_owned(), |s| s.name.clone())
    }

    /// The tag services' rows (name, action, the tags merged), sorted, and
    /// each row's service.
    pub fn tag_rows(&self) -> Vec<([String; 3], ServiceKey)> {
        let mut rows: Vec<([String; 3], ServiceKey)> = self
            .tags
            .iter()
            .map(|t| {
                (
                    [
                        self.name(&t.service),
                        action_label(Some(t.action)).to_owned(),
                        t.filter.to_filter_string(),
                    ],
                    t.service.clone(),
                )
            })
            .collect();
        rows.sort_by(|a, b| a.0.cmp(&b.0));
        rows
    }

    fn rating_label(&self, key: &ServiceKey, action: MergeAction) -> &'static str {
        if self
            .service(key)
            .is_some_and(|s| s.kind == ServiceType::LocalRatingIncDec)
        {
            number_action_label(action)
        } else {
            action_label(Some(action))
        }
    }

    /// The rating services' rows (name, action), sorted.
    pub fn rating_rows(&self) -> Vec<([String; 2], ServiceKey)> {
        let mut rows: Vec<([String; 2], ServiceKey)> = self
            .ratings
            .iter()
            .map(|r| {
                (
                    [
                        self.name(&r.service),
                        self.rating_label(&r.service, r.action).to_owned(),
                    ],
                    r.service.clone(),
                )
            })
            .collect();
        rows.sort_by(|a, b| a.0.cmp(&b.0));
        rows
    }

    /// "add" on the tag services: the services not yet listed, or the
    /// reference's warning when there are none.
    pub fn add_tag_services(&self) -> Result<Choices<ServiceKey>, &'static str> {
        let choices: Vec<(String, ServiceKey)> = self
            .services
            .iter()
            .filter(|s| s.kind.is_real_tag_service())
            .filter(|s| !self.tags.iter().any(|t| t.service == s.key))
            .map(|s| (s.name.clone(), s.key.clone()))
            .collect();
        if choices.is_empty() {
            return Err(NO_MORE_SERVICES);
        }
        Ok(Choices::new(SELECT_SERVICE, choices, None))
    }

    /// "add" on the rating services.
    pub fn add_rating_services(&self) -> Result<Choices<ServiceKey>, &'static str> {
        let choices: Vec<(String, ServiceKey)> = self
            .services
            .iter()
            .filter(|s| s.kind.is_rating_service())
            .filter(|s| !self.ratings.iter().any(|r| r.service == s.key))
            .map(|s| (s.name.clone(), s.key.clone()))
            .collect();
        if choices.is_empty() {
            return Err(NO_MORE_SERVICES);
        }
        Ok(Choices::new(SELECT_SERVICE, choices, None))
    }

    /// The actions a tag service can take, asked only for "this is
    /// better" (else it copies in both directions): a tag repository's
    /// tags can't be moved.
    pub fn tag_actions(
        &self,
        service: &ServiceKey,
        current: Option<MergeAction>,
    ) -> Option<Choices<MergeAction>> {
        if !self.better() {
            return None;
        }
        let repository = self
            .service(service)
            .is_some_and(|s| s.kind == ServiceType::TagRepository);
        let actions: &[MergeAction] = if repository {
            &[MergeAction::Copy, MergeAction::TwoWay]
        } else {
            &[MergeAction::Copy, MergeAction::Move, MergeAction::TwoWay]
        };
        Some(Choices::new(
            SELECT_ACTION,
            actions
                .iter()
                .map(|&a| (action_label(Some(a)).to_owned(), a))
                .collect(),
            current.map(|a| action_label(Some(a)).to_owned()),
        ))
    }

    /// The actions a rating service can take, for "this is better".
    pub fn rating_actions(
        &self,
        service: &ServiceKey,
        current: Option<MergeAction>,
    ) -> Option<Choices<MergeAction>> {
        if !self.better() {
            return None;
        }
        let actions = [MergeAction::Copy, MergeAction::Move, MergeAction::TwoWay];
        Some(Choices::new(
            SELECT_ACTION,
            actions
                .iter()
                .map(|&a| (self.rating_label(service, a).to_owned(), a))
                .collect(),
            current.map(|a| self.rating_label(service, a).to_owned()),
        ))
    }

    pub fn add_tag(&mut self, service: ServiceKey, action: Option<MergeAction>, filter: TagFilter) {
        self.tags.retain(|t| t.service != service);
        self.tags.push(TagMerge {
            service,
            action: action.unwrap_or(MergeAction::TwoWay),
            filter,
        });
    }

    pub fn add_rating(&mut self, service: ServiceKey, action: Option<MergeAction>) {
        self.ratings.retain(|r| r.service != service);
        self.ratings.push(RatingMerge {
            service,
            action: action.unwrap_or(MergeAction::TwoWay),
        });
    }

    pub fn tag_action(&self, service: &ServiceKey) -> Option<MergeAction> {
        self.tags
            .iter()
            .find(|t| &t.service == service)
            .map(|t| t.action)
    }

    pub fn tag_filter(&self, service: &ServiceKey) -> Option<TagFilter> {
        self.tags
            .iter()
            .find(|t| &t.service == service)
            .map(|t| t.filter.clone())
    }

    pub fn rating_action(&self, service: &ServiceKey) -> Option<MergeAction> {
        self.ratings
            .iter()
            .find(|r| &r.service == service)
            .map(|r| r.action)
    }

    pub fn set_tag_action(&mut self, service: &ServiceKey, action: MergeAction) {
        if let Some(t) = self.tags.iter_mut().find(|t| &t.service == service) {
            t.action = action;
        }
    }

    pub fn set_tag_filter(&mut self, service: &ServiceKey, filter: TagFilter) {
        if let Some(t) = self.tags.iter_mut().find(|t| &t.service == service) {
            t.filter = filter;
        }
    }

    pub fn set_rating_action(&mut self, service: &ServiceKey, action: MergeAction) {
        if let Some(r) = self.ratings.iter_mut().find(|r| &r.service == service) {
            r.action = action;
        }
    }

    /// "delete", after asking, of the tag services selected.
    pub fn delete_tags(&mut self, services: &[ServiceKey]) {
        self.tags.retain(|t| !services.contains(&t.service));
    }

    pub fn delete_ratings(&mut self, services: &[ServiceKey]) {
        self.ratings.retain(|r| !services.contains(&r.service));
    }

    /// The archived status choice.
    pub fn archive(&self) -> Choice {
        Choice {
            options: ARCHIVE_CHOICES.to_vec(),
            value: match self.archive {
                ArchiveSync::Never => 0,
                ArchiveSync::IfEither => 1,
                ArchiveSync::Always => 2,
            },
            enabled: true,
        }
    }

    pub fn set_archive(&mut self, index: usize) {
        self.archive = match index {
            1 => ArchiveSync::IfEither,
            2 => ArchiveSync::Always,
            _ => ArchiveSync::Never,
        };
    }

    /// The urls and modified time choices' actions, in order.
    fn sync_actions(&self) -> Vec<Option<SyncAction>> {
        if self.better() {
            vec![None, Some(SyncAction::Copy), Some(SyncAction::TwoWay)]
        } else {
            vec![None, Some(SyncAction::TwoWay)]
        }
    }

    fn note_actions(&self) -> Vec<Option<MergeAction>> {
        if self.better() {
            vec![
                None,
                Some(MergeAction::Copy),
                Some(MergeAction::Move),
                Some(MergeAction::TwoWay),
            ]
        } else {
            vec![None, Some(MergeAction::TwoWay)]
        }
    }

    fn choice<T: PartialEq>(
        &self,
        actions: &[T],
        value: &T,
        label: impl Fn(&T) -> &'static str,
    ) -> Choice {
        Choice {
            options: actions.iter().map(&label).collect(),
            value: actions.iter().position(|a| a == value).unwrap_or(0),
            enabled: self.syncs_enabled(),
        }
    }

    pub fn file_modified(&self) -> Choice {
        self.choice(&self.sync_actions(), &self.file_modified, |a| {
            modified_label(*a)
        })
    }

    pub fn urls(&self) -> Choice {
        self.choice(&self.sync_actions(), &self.urls, |a| sync_label(*a))
    }

    pub fn notes(&self) -> Choice {
        self.choice(&self.note_actions(), &self.notes, |a| action_label(*a))
    }

    pub fn set_file_modified(&mut self, index: usize) {
        if let Some(&a) = self.sync_actions().get(index) {
            self.file_modified = a;
        }
    }

    pub fn set_urls(&mut self, index: usize) {
        if let Some(&a) = self.sync_actions().get(index) {
            self.urls = a;
        }
    }

    pub fn set_notes(&mut self, index: usize) {
        if let Some(&a) = self.note_actions().get(index) {
            self.notes = a;
        }
    }

    /// Whether "note merge settings" can be opened: while notes sync.
    pub fn note_settings_enabled(&self) -> bool {
        self.notes.is_some()
    }

    pub fn note_merge(&self) -> NoteMerge {
        self.note_merge
    }

    pub fn set_note_merge(&mut self, merge: NoteMerge) {
        self.note_merge = merge;
    }

    pub fn value(&self) -> MergeOptions {
        MergeOptions {
            tags: self.tags.clone(),
            ratings: self.ratings.clone(),
            notes: self.notes,
            note_merge: Some(self.note_merge),
            note_names: self.note_names.clone(),
            archive: self.archive,
            urls: self.urls,
            file_modified: self.file_modified,
        }
    }
}
