//! Import options: what an import may bring in, where it goes, and what
//! metadata it keeps.
//!
//! As in the reference (v688), options come in eight kinds, and a *slice*
//! holds any of them. Defaults are kept per kind of importer (global, post
//! URLs, watchers, subscriptions, local imports, the Client API, ...), per
//! URL class, and on each importer; an import layers the slices that apply,
//! most specific first, and takes each kind from the first slice that has
//! it ([`ImportOptionsManager::full`]). Service keys are hex, as elsewhere
//! in settings.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::service::builtin_keys;
use crate::sort::human_sort;
use crate::tag::split_tag;
use crate::tag_filter::{FilterRule, TagFilter};

/// The kinds of importer defaults are kept for (`IMPORT_OPTIONS_CALLER_TYPE_*`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CallerType {
    LocalImport,
    PostUrls,
    Subscription,
    WatcherUrls,
    Global,
    UrlClass,
    SpecificImporter,
    LocalImportFolder,
    ClientApi,
    Favourites,
}

impl CallerType {
    pub fn from_code(code: i64) -> Option<Self> {
        Some(match code {
            0 => Self::LocalImport,
            1 => Self::PostUrls,
            2 => Self::Subscription,
            3 => Self::WatcherUrls,
            4 => Self::Global,
            7 => Self::UrlClass,
            8 => Self::SpecificImporter,
            9 => Self::LocalImportFolder,
            10 => Self::ClientApi,
            11 => Self::Favourites,
            _ => return None,
        })
    }

    /// The reference's name for it.
    pub fn name(self) -> &'static str {
        match self {
            Self::LocalImport => "local hard drive import",
            Self::PostUrls => "gallery/post urls",
            Self::Subscription => "subscription",
            Self::WatcherUrls => "watchable urls",
            Self::Global => "global",
            Self::UrlClass => "url class",
            Self::SpecificImporter => "specific importer",
            Self::LocalImportFolder => "import folder",
            Self::ClientApi => "client api",
            Self::Favourites => "favourites template",
        }
    }
}

/// Whether to look a file up before fetching it (`DO_NOT_CHECK`, `DO_CHECK`,
/// `DO_CHECK_AND_MATCHES_ARE_DISPOSITIVE`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PrefetchCheck {
    DoNotCheck,
    Check,
    /// A match settles it, even against other evidence.
    CheckAndMatchesAreDispositive,
}

impl PrefetchCheck {
    pub fn from_code(code: i64) -> Option<Self> {
        Some(match code {
            0 => Self::DoNotCheck,
            1 => Self::Check,
            2 => Self::CheckAndMatchesAreDispositive,
            _ => return None,
        })
    }
}

/// Whether to skip downloading what the client already knows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PrefetchOptions {
    pub hash_check: PrefetchCheck,
    pub url_check: PrefetchCheck,
    /// Distrust a URL match when the file has other URLs of the same class
    /// on the same site (a site reusing URLs for different files).
    pub url_check_looks_for_neighbour_spam: bool,
    pub fetch_metadata_even_if_url_recognised_and_file_already_in_db: bool,
    pub fetch_metadata_even_if_hash_recognised_and_file_already_in_db: bool,
}

impl Default for PrefetchOptions {
    fn default() -> Self {
        Self {
            hash_check: PrefetchCheck::CheckAndMatchesAreDispositive,
            url_check: PrefetchCheck::Check,
            url_check_looks_for_neighbour_spam: true,
            fetch_metadata_even_if_url_recognised_and_file_already_in_db: false,
            fetch_metadata_even_if_hash_recognised_and_file_already_in_db: false,
        }
    }
}

/// Which files an import accepts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileFilteringOptions {
    pub exclude_deleted: bool,
    pub allow_decompression_bombs: bool,
    /// Allowed file types, as the reference summarises them (general class
    /// codes stand for their members), by `HC` mime code.
    pub filetypes: BTreeSet<u8>,
    pub min_size: Option<u64>,
    pub max_size: Option<u64>,
    pub max_gif_size: Option<u64>,
    pub min_resolution: Option<(u32, u32)>,
    pub max_resolution: Option<(u32, u32)>,
}

impl Default for FileFilteringOptions {
    fn default() -> Self {
        Self {
            exclude_deleted: true,
            allow_decompression_bombs: true,
            // HC.GENERAL_CLASSES_OF_FILETYPE
            filetypes: crate::mime::Mime::general_classes()
                .iter()
                .map(|m| m.code())
                .collect(),
            min_size: None,
            max_size: None,
            max_gif_size: None,
            min_resolution: None,
            max_resolution: None,
        }
    }
}

/// Tags a download must not (or must) have.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TagFilteringOptions {
    pub blacklist: TagFilter,
    /// If any, a download needs one of these (whole tags or subtags).
    pub whitelist: Vec<String>,
}

impl TagFilteringOptions {
    pub fn allows_everything(&self) -> bool {
        self.blacklist.allows_everything() && self.whitelist.is_empty()
    }

    /// Veto a download by its parsed tags (and those tags' siblings), with
    /// the reference's reason.
    pub fn check_tags_veto(
        &self,
        tags: &BTreeSet<String>,
        sibling_tags: &BTreeSet<String>,
    ) -> Result<(), String> {
        for test in [tags, sibling_tags] {
            let mut bad: Vec<&str> = test
                .iter()
                .filter(|t| !self.blacklist.tag_ok(t, true))
                .map(String::as_str)
                .collect();
            if !bad.is_empty() {
                human_sort(&mut bad);
                return Err(format!("{} is blacklisted!", bad.join(", ")));
            }
        }
        if !self.whitelist.is_empty() {
            let mut all: BTreeSet<&str> = tags
                .iter()
                .chain(sibling_tags)
                .map(String::as_str)
                .collect();
            let subtags: Vec<&str> = all.iter().map(|t| split_tag(t).1).collect();
            all.extend(subtags);
            if !self.whitelist.iter().any(|w| all.contains(w.as_str())) {
                return Err("did not pass the whitelist!".into());
            }
        }
        Ok(())
    }
}

/// Where imported files go.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocationOptions {
    /// Local file domains (hex keys).
    pub destinations: Vec<String>,
    pub automatically_archive: bool,
    pub associate_primary_urls: bool,
    pub associate_source_urls: bool,
    /// Also archive files that were already in the client.
    pub archive_already_in_db: bool,
    /// Also add files that were already in the client to the destinations.
    pub destinations_for_already_in_db: bool,
}

impl Default for LocationOptions {
    fn default() -> Self {
        Self {
            destinations: vec![hex::encode(builtin_keys::MY_FILES)],
            automatically_archive: false,
            associate_primary_urls: true,
            associate_source_urls: true,
            archive_already_in_db: true,
            destinations_for_already_in_db: false,
        }
    }
}

/// Which files a tag rule applies to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportedAs {
    New,
    AlreadyInInbox,
    AlreadyInArchive,
}

/// What to do with a download's tags for one tag service.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServiceTagImportOptions {
    /// Keep the tags the parser found (those the filter allows).
    pub get_tags: bool,
    pub get_tags_filter: TagFilter,
    pub additional_tags: Vec<String>,
    pub to_new_files: bool,
    pub to_already_in_inbox: bool,
    pub to_already_in_archive: bool,
    /// For tags the filter below allows, only add those the service
    /// already has somewhere.
    pub only_add_existing_tags: bool,
    pub only_add_existing_tags_filter: TagFilter,
    pub get_tags_overwrite_deleted: bool,
    pub additional_tags_overwrite_deleted: bool,
}

impl Default for ServiceTagImportOptions {
    fn default() -> Self {
        Self {
            get_tags: false,
            get_tags_filter: TagFilter::new(),
            additional_tags: Vec::new(),
            to_new_files: true,
            to_already_in_inbox: true,
            to_already_in_archive: true,
            only_add_existing_tags: false,
            only_add_existing_tags_filter: TagFilter::new(),
            get_tags_overwrite_deleted: false,
            additional_tags_overwrite_deleted: false,
        }
    }
}

impl ServiceTagImportOptions {
    /// Whether these rules apply to a file imported this way.
    pub fn applies_to(&self, imported: ImportedAs) -> bool {
        match imported {
            ImportedAs::New => self.to_new_files,
            ImportedAs::AlreadyInInbox => self.to_already_in_inbox,
            ImportedAs::AlreadyInArchive => self.to_already_in_archive,
        }
    }
}

/// Where a download's tags go, per tag service.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TagImportOptions {
    /// `(service key hex, rules)`.
    pub services: Vec<(String, ServiceTagImportOptions)>,
}

impl TagImportOptions {
    pub fn service(&self, key_hex: &str) -> Option<&ServiceTagImportOptions> {
        self.services
            .iter()
            .find(|(k, _)| k == key_hex)
            .map(|(_, o)| o)
    }

    pub fn worth_fetching_tags(&self) -> bool {
        self.services.iter().any(|(_, o)| o.get_tags)
    }

    pub fn has_additional_tags(&self) -> bool {
        self.services
            .iter()
            .any(|(_, o)| !o.additional_tags.is_empty())
    }
}

pub use crate::notes::NoteConflict;

/// What to do with a download's notes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NoteImportOptions {
    pub get_notes: bool,
    pub extend_existing_note_if_possible: bool,
    pub conflict: NoteConflict,
    /// If any, only notes with these names.
    pub name_whitelist: Vec<String>,
    /// Rename every note to this.
    pub all_name_override: Option<String>,
    pub name_overrides: Vec<(String, String)>,
}

impl Default for NoteImportOptions {
    fn default() -> Self {
        Self {
            get_notes: true,
            extend_existing_note_if_possible: true,
            conflict: NoteConflict::Rename,
            name_whitelist: Vec::new(),
            all_name_override: None,
            name_overrides: Vec::new(),
        }
    }
}

impl NoteImportOptions {
    /// The notes to set on a file with `existing` notes for a download's
    /// notes (`GetUpdateeNamesToNotes`): none unless notes are wanted; only
    /// whitelisted names; renamed as set; then merged.
    pub fn updates(
        &self,
        existing: &std::collections::BTreeMap<String, String>,
        incoming: &[(String, String)],
    ) -> std::collections::BTreeMap<String, String> {
        if !self.get_notes {
            return std::collections::BTreeMap::new();
        }
        let mut incoming = incoming.to_vec();
        incoming.sort();
        let renamed = incoming
            .into_iter()
            .filter(|(name, _)| {
                self.name_whitelist.is_empty() || self.name_whitelist.contains(name)
            })
            .map(|(name, note)| {
                let name = match self.name_overrides.iter().find(|(from, _)| *from == name) {
                    Some((_, to)) => to.clone(),
                    None => self.all_name_override.clone().unwrap_or(name),
                };
                (name, note)
            })
            .collect();
        crate::notes::NoteMerge {
            extend_existing: self.extend_existing_note_if_possible,
            conflict: self.conflict,
        }
        .merge_in_order(existing, renamed)
    }
}

/// `PRESENTATION_STATUS_*`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PresentationStatus {
    AnyGood,
    NewOnly,
    None,
}

/// `PRESENTATION_INBOX_*`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PresentationInbox {
    Agnostic,
    RequireInbox,
    AndIncludeAllInbox,
}

/// Which imported files an importer shows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PresentationOptions {
    /// Only files in these domains (hex keys).
    pub location: Vec<String>,
    pub status: PresentationStatus,
    pub inbox: PresentationInbox,
}

impl Default for PresentationOptions {
    fn default() -> Self {
        Self {
            location: vec![hex::encode(builtin_keys::COMBINED_LOCAL_FILE_DOMAINS)],
            status: PresentationStatus::AnyGood,
            inbox: PresentationInbox::Agnostic,
        }
    }
}

/// External programs run per imported file. Not supported: kept as the
/// reference stored them.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExternalProgramsOptions {
    /// The reference's serialised form, if anything is set.
    pub stored: Option<String>,
}

/// Any of the eight kinds of options.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImportOptionsSlice {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prefetch: Option<PrefetchOptions>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file_filtering: Option<FileFilteringOptions>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tag_filtering: Option<TagFilteringOptions>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub locations: Option<LocationOptions>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tags: Option<TagImportOptions>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<NoteImportOptions>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub presentation: Option<PresentationOptions>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub external_programs: Option<ExternalProgramsOptions>,
}

impl ImportOptionsSlice {
    /// Every kind, at its default.
    pub fn full_defaults() -> Self {
        Self {
            prefetch: Some(PrefetchOptions::default()),
            file_filtering: Some(FileFilteringOptions::default()),
            tag_filtering: Some(TagFilteringOptions::default()),
            locations: Some(LocationOptions::default()),
            tags: Some(TagImportOptions::default()),
            notes: Some(NoteImportOptions::default()),
            presentation: Some(PresentationOptions::default()),
            external_programs: Some(ExternalProgramsOptions::default()),
        }
    }

    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    /// Take each kind this slice lacks from `other`.
    pub fn fill_in(&mut self, other: &Self) {
        fn fill<T: Clone>(mine: &mut Option<T>, theirs: Option<&T>) {
            if mine.is_none() {
                *mine = theirs.cloned();
            }
        }
        fill(&mut self.prefetch, other.prefetch.as_ref());
        fill(&mut self.file_filtering, other.file_filtering.as_ref());
        fill(&mut self.tag_filtering, other.tag_filtering.as_ref());
        fill(&mut self.locations, other.locations.as_ref());
        fill(&mut self.tags, other.tags.as_ref());
        fill(&mut self.notes, other.notes.as_ref());
        fill(&mut self.presentation, other.presentation.as_ref());
        fill(
            &mut self.external_programs,
            other.external_programs.as_ref(),
        );
    }
}

/// Every kind of option, as an import uses them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FullImportOptions {
    pub prefetch: PrefetchOptions,
    pub file_filtering: FileFilteringOptions,
    pub tag_filtering: TagFilteringOptions,
    pub locations: LocationOptions,
    pub tags: TagImportOptions,
    pub notes: NoteImportOptions,
    pub presentation: PresentationOptions,
}

/// The client's import option defaults.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImportOptionsManager {
    pub caller_defaults: Vec<(CallerType, ImportOptionsSlice)>,
    /// `(url class key hex, slice)`.
    pub url_class_defaults: Vec<(String, ImportOptionsSlice)>,
    /// Named slices to copy from.
    pub favourites: Vec<(String, ImportOptionsSlice)>,
}

impl Default for ImportOptionsManager {
    /// A new client's defaults (`STATICGetDefaultInitialisedManager`).
    fn default() -> Self {
        let quiet = ImportOptionsSlice {
            presentation: Some(PresentationOptions {
                status: PresentationStatus::NewOnly,
                ..PresentationOptions::default()
            }),
            ..ImportOptionsSlice::default()
        };
        let downloader_tags = TagImportOptions {
            services: vec![(
                hex::encode(builtin_keys::DOWNLOADER_TAGS),
                ServiceTagImportOptions {
                    get_tags: true,
                    ..ServiceTagImportOptions::default()
                },
            )],
        };
        let only = |slice: ImportOptionsSlice| slice;
        let all_local_media = hex::encode(builtin_keys::COMBINED_LOCAL_FILE_DOMAINS);
        Self {
            caller_defaults: vec![
                (CallerType::Global, ImportOptionsSlice::full_defaults()),
                (CallerType::ClientApi, ImportOptionsSlice::default()),
                (CallerType::LocalImport, ImportOptionsSlice::default()),
                (CallerType::LocalImportFolder, quiet.clone()),
                (
                    CallerType::WatcherUrls,
                    only(ImportOptionsSlice {
                        tags: Some(TagImportOptions::default()),
                        ..ImportOptionsSlice::default()
                    }),
                ),
                (
                    CallerType::PostUrls,
                    only(ImportOptionsSlice {
                        tags: Some(downloader_tags),
                        ..ImportOptionsSlice::default()
                    }),
                ),
                (CallerType::Subscription, quiet),
            ],
            url_class_defaults: Vec::new(),
            favourites: vec![
                (
                    "no tags".into(),
                    ImportOptionsSlice {
                        tags: Some(TagImportOptions::default()),
                        ..ImportOptionsSlice::default()
                    },
                ),
                (
                    "show new files".into(),
                    ImportOptionsSlice {
                        presentation: Some(PresentationOptions {
                            location: vec![all_local_media.clone()],
                            status: PresentationStatus::NewOnly,
                            inbox: PresentationInbox::Agnostic,
                        }),
                        ..ImportOptionsSlice::default()
                    },
                ),
                (
                    "show all files".into(),
                    ImportOptionsSlice {
                        presentation: Some(PresentationOptions {
                            location: vec![all_local_media],
                            status: PresentationStatus::AnyGood,
                            inbox: PresentationInbox::Agnostic,
                        }),
                        ..ImportOptionsSlice::default()
                    },
                ),
                (
                    "example blacklist".into(),
                    ImportOptionsSlice {
                        tag_filtering: Some(TagFilteringOptions {
                            blacklist: TagFilter::new()
                                .with_rule("goblin", FilterRule::Blacklist)
                                .with_rule("orc", FilterRule::Blacklist),
                            whitelist: Vec::new(),
                        }),
                        ..ImportOptionsSlice::default()
                    },
                ),
                (
                    "force metadata refetch".into(),
                    ImportOptionsSlice {
                        prefetch: Some(PrefetchOptions {
                            hash_check: PrefetchCheck::CheckAndMatchesAreDispositive,
                            url_check: PrefetchCheck::Check,
                            url_check_looks_for_neighbour_spam: true,
                            fetch_metadata_even_if_url_recognised_and_file_already_in_db: true,
                            fetch_metadata_even_if_hash_recognised_and_file_already_in_db: true,
                        }),
                        ..ImportOptionsSlice::default()
                    },
                ),
            ],
        }
    }
}

/// What a URL class says about a URL, for choosing defaults.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UrlClassKind {
    Watchable,
    Other,
}

impl ImportOptionsManager {
    pub fn caller_default(&self, caller: CallerType) -> Option<&ImportOptionsSlice> {
        self.caller_defaults
            .iter()
            .find(|(c, _)| *c == caller)
            .map(|(_, s)| s)
    }

    pub fn url_class_default(&self, key_hex: &str) -> Option<&ImportOptionsSlice> {
        self.url_class_defaults
            .iter()
            .find(|(k, _)| k == key_hex)
            .map(|(_, s)| s)
    }

    /// The options an import uses: the importer's own slice, then its URL
    /// classes' defaults (the first class that has some), then the defaults
    /// for its kind of importer and the kinds it falls under, then global
    /// (`GenerateFullImportOptionsContainer`). `url_classes` are the URL's
    /// class and the classes of its API URLs, in that order, each with
    /// whether it is watchable (for looking up URL class defaults alone).
    pub fn full(
        &self,
        caller: CallerType,
        specific: Option<&ImportOptionsSlice>,
        url_classes: &[(String, UrlClassKind)],
    ) -> FullImportOptions {
        let mut result = ImportOptionsSlice::default();
        for layer in preference_stack(caller, url_classes) {
            match layer {
                CallerType::SpecificImporter => {
                    if let Some(specific) = specific {
                        result.fill_in(specific);
                    }
                }
                CallerType::UrlClass => {
                    if let Some(slice) = url_classes
                        .iter()
                        .find_map(|(key, _)| self.url_class_default(key))
                    {
                        result.fill_in(slice);
                    }
                }
                other => {
                    if let Some(slice) = self.caller_default(other) {
                        result.fill_in(slice);
                    }
                }
            }
        }
        // a damaged global slice is repaired with defaults, as the reference
        // repairs it
        result.fill_in(&ImportOptionsSlice::full_defaults());
        FullImportOptions {
            prefetch: result.prefetch.unwrap_or_default(),
            file_filtering: result.file_filtering.unwrap_or_default(),
            tag_filtering: result.tag_filtering.unwrap_or_default(),
            locations: result.locations.unwrap_or_default(),
            tags: result.tags.unwrap_or_default(),
            notes: result.notes.unwrap_or_default(),
            presentation: result.presentation.unwrap_or_default(),
        }
    }
}

/// The kinds of defaults an importer consults, most specific first
/// (`GetImportOptionsCallerTypesPreferenceOrderFull`, reversed).
pub fn preference_stack(
    caller: CallerType,
    url_classes: &[(String, UrlClassKind)],
) -> Vec<CallerType> {
    let mut stack = vec![CallerType::Global];
    match caller {
        CallerType::Subscription => {
            stack.extend([CallerType::PostUrls, caller, CallerType::UrlClass]);
        }
        CallerType::PostUrls | CallerType::WatcherUrls => {
            stack.extend([caller, CallerType::UrlClass]);
        }
        CallerType::UrlClass => {
            let watchable = url_classes
                .first()
                .is_some_and(|(_, kind)| *kind == UrlClassKind::Watchable);
            stack.push(if watchable {
                CallerType::WatcherUrls
            } else {
                CallerType::PostUrls
            });
            stack.push(CallerType::UrlClass);
        }
        CallerType::LocalImportFolder => {
            stack.extend([CallerType::LocalImport, CallerType::LocalImportFolder]);
        }
        other => stack.push(other),
    }
    if caller != CallerType::ClientApi {
        stack.push(CallerType::SpecificImporter);
    }
    stack.reverse();
    stack
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layers_most_specific_first() {
        let manager = ImportOptionsManager::default();
        let full = manager.full(CallerType::PostUrls, None, &[]);
        assert!(full.tags.worth_fetching_tags());
        assert_eq!(full.presentation.status, PresentationStatus::AnyGood);
        let sub = manager.full(CallerType::Subscription, None, &[]);
        assert_eq!(sub.presentation.status, PresentationStatus::NewOnly);
        assert!(sub.tags.worth_fetching_tags());
        let watcher = manager.full(CallerType::WatcherUrls, None, &[]);
        assert!(!watcher.tags.worth_fetching_tags());
        let specific = ImportOptionsSlice {
            tags: Some(TagImportOptions::default()),
            ..ImportOptionsSlice::default()
        };
        assert!(
            !manager
                .full(CallerType::PostUrls, Some(&specific), &[])
                .tags
                .worth_fetching_tags()
        );
        assert_eq!(
            preference_stack(CallerType::Subscription, &[]),
            vec![
                CallerType::SpecificImporter,
                CallerType::UrlClass,
                CallerType::Subscription,
                CallerType::PostUrls,
                CallerType::Global
            ]
        );
    }

    #[test]
    fn vetoes_like_the_reference() {
        let filtering = TagFilteringOptions {
            blacklist: TagFilter::new().with_rule("goblin", FilterRule::Blacklist),
            whitelist: Vec::new(),
        };
        let tags: BTreeSet<String> = ["creature:goblin".to_owned(), "orc".to_owned()].into();
        assert_eq!(
            filtering.check_tags_veto(&tags, &BTreeSet::new()),
            Err("creature:goblin is blacklisted!".into())
        );
        let whitelist = TagFilteringOptions {
            blacklist: TagFilter::new(),
            whitelist: vec!["orc".into()],
        };
        assert!(whitelist.check_tags_veto(&tags, &BTreeSet::new()).is_ok());
        assert!(
            whitelist
                .check_tags_veto(&BTreeSet::new(), &BTreeSet::new())
                .is_err()
        );
    }
}
