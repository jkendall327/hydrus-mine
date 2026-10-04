//! The import options editor for an importer (the reference's
//! `EditSpecificImportOptionsContainerPanel`): the kinds of options it
//! lists, whose default each falls back to, and the list's labels
//! ("default tags (gallery/post urls)", "> presentation: presenting new
//! files"). Recorded by `oracle/record_import_options_editor.py`.

use hydrus_core::import_options::{CallerType, ImportOptionsManager, ImportOptionsSlice};

/// A kind of import options (`IMPORT_OPTIONS_TYPE_*`), in the reference's
/// canonical order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Kind {
    Prefetch,
    FileFiltering,
    TagFiltering,
    Locations,
    Tags,
    Notes,
    ExternalPrograms,
    Presentation,
}

impl Kind {
    pub fn code(self) -> u8 {
        match self {
            Self::Prefetch => 0,
            Self::FileFiltering => 1,
            Self::TagFiltering => 2,
            Self::Locations => 3,
            Self::Tags => 4,
            Self::Notes => 5,
            Self::Presentation => 6,
            Self::ExternalPrograms => 7,
        }
    }

    pub const ALL: [Kind; 8] = [
        Kind::Prefetch,
        Kind::FileFiltering,
        Kind::TagFiltering,
        Kind::Locations,
        Kind::Tags,
        Kind::Notes,
        Kind::ExternalPrograms,
        Kind::Presentation,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Kind::Prefetch => "prefetch logic",
            Kind::FileFiltering => "file filtering",
            Kind::TagFiltering => "tag filtering",
            Kind::Locations => "locations",
            Kind::Tags => "tags",
            Kind::Notes => "notes",
            Kind::ExternalPrograms => "external programs",
            Kind::Presentation => "presentation",
        }
    }

    /// The text above its page.
    pub fn description(self) -> &'static str {
        match self {
            Kind::Prefetch => {
                "Hydrus tries to save bandwidth. In most cases, it will not redownload a file page (HTML/JSON) or the file itself if it can correctly identify that it already has the file, or, in conjunction with file filtering options, wishes to exclude previously deleted files. Adjusting these settings can and will waste bandwidth and is only appropriate for one-off jobs where some forced recheck is needed."
            }
            Kind::FileFiltering => {
                "Before a file is imported, it can be checked against these rules. If it fails one of these rules, it will get an \"ignored\" status."
            }
            Kind::TagFiltering => {
                "Before a file is imported, its tags can be checked against a tag blacklist and/or whitelist. If a tag hits the blacklist, or no tag hits the whitelist, the import will get an \"ignored\" status. Only the tags that are parsed as part of the download are used in these tests."
            }
            Kind::Locations => {
                "If you have multiple local file services, you can choose to place incoming files in a different location than your default (probably \"my files\"). You can also send them to multiple locations."
            }
            Kind::Tags => {
                "A file may pick up tags through the downloading and parsing process. Here you choose where to send any parsed tags."
            }
            Kind::Notes => {
                "A file may pick up notes through the downloading and parsing process. Here you choose what to do with these notes. Default options are usually fine unless you have particular needs."
            }
            Kind::ExternalPrograms => {
                "Here you can choose to run an external program for every file imported. This is only appropriate if you are maintaining a secondary database of additional metadata beside hydrus. Be careful with this!"
            }
            Kind::Presentation => {
                "When files are imported, the importer will then likely want to show them, whether that is adding thumbnails to a page or publishing files to a popup button. Here you can filter which files are actually presented to UI. Selecting \"only new\" or \"only inbox\" are often useful to remove clutter in background/automated import workflows."
            }
        }
    }

    /// Whether only downloaders use it.
    pub fn downloader_only(self) -> bool {
        matches!(
            self,
            Kind::Prefetch | Kind::Tags | Kind::TagFiltering | Kind::Notes
        )
    }

    /// Whether `slice` sets it.
    pub fn is_set(self, slice: &ImportOptionsSlice) -> bool {
        match self {
            Kind::Prefetch => slice.prefetch.is_some(),
            Kind::FileFiltering => slice.file_filtering.is_some(),
            Kind::TagFiltering => slice.tag_filtering.is_some(),
            Kind::Locations => slice.locations.is_some(),
            Kind::Tags => slice.tags.is_some(),
            Kind::Notes => slice.notes.is_some(),
            Kind::ExternalPrograms => slice.external_programs.is_some(),
            Kind::Presentation => slice.presentation.is_some(),
        }
    }

    /// Take it out of `slice` (back to the default).
    pub fn clear(self, slice: &mut ImportOptionsSlice) {
        match self {
            Kind::Prefetch => slice.prefetch = None,
            Kind::FileFiltering => slice.file_filtering = None,
            Kind::TagFiltering => slice.tag_filtering = None,
            Kind::Locations => slice.locations = None,
            Kind::Tags => slice.tags = None,
            Kind::Notes => slice.notes = None,
            Kind::ExternalPrograms => slice.external_programs = None,
            Kind::Presentation => slice.presentation = None,
        }
    }

    /// Copy it from `from` into `slice`.
    pub fn copy(self, from: &ImportOptionsSlice, slice: &mut ImportOptionsSlice) {
        match self {
            Kind::Prefetch => slice.prefetch.clone_from(&from.prefetch),
            Kind::FileFiltering => slice.file_filtering.clone_from(&from.file_filtering),
            Kind::TagFiltering => slice.tag_filtering.clone_from(&from.tag_filtering),
            Kind::Locations => slice.locations.clone_from(&from.locations),
            Kind::Tags => slice.tags.clone_from(&from.tags),
            Kind::Notes => slice.notes.clone_from(&from.notes),
            Kind::ExternalPrograms => slice.external_programs.clone_from(&from.external_programs),
            Kind::Presentation => slice.presentation.clone_from(&from.presentation),
        }
    }
}

/// What the editor says it edits.
pub const DESCRIPTION: &str = "You are editing \"specific importer\".\n\nThese import options are attached to this specific importer alone. If you set something here, it will only apply here, and it will definitely apply, overriding any other default.";

/// A page's choice for custom options (the other is
/// [`use_default_label`]).
pub const CUSTOM_CHOICE: &str = "set custom import options";

/// The kinds the editor lists for an importer whose defaults are
/// `caller`'s: in simple mode, a local import's or import folder's only
/// those that aren't the downloaders' alone; and any the importer sets.
pub fn listed_kinds(caller: CallerType, simple: bool, slice: &ImportOptionsSlice) -> Vec<Kind> {
    let local = matches!(
        caller,
        CallerType::LocalImport | CallerType::LocalImportFolder | CallerType::ClientApi
    );
    Kind::ALL
        .into_iter()
        .filter(|k| !(simple && local && k.downloader_only()) || k.is_set(slice))
        .collect()
}

/// The defaults page hides unusual option kinds, while retaining every custom
/// kind already present. Importer buttons keep their existing broader lists.
pub fn default_kinds(caller: CallerType, simple: bool, own: &ImportOptionsSlice) -> Vec<Kind> {
    use Kind as K;
    let mut kinds = if simple {
        match caller {
            CallerType::LocalImport => vec![
                K::FileFiltering,
                K::Locations,
                K::ExternalPrograms,
                K::Presentation,
            ],
            CallerType::LocalImportFolder => {
                vec![K::Locations, K::ExternalPrograms, K::Presentation]
            }
            CallerType::Subscription => vec![K::Locations, K::Presentation],
            CallerType::PostUrls | CallerType::WatcherUrls => vec![
                K::FileFiltering,
                K::TagFiltering,
                K::Locations,
                K::Tags,
                K::Notes,
                K::Presentation,
            ],
            CallerType::UrlClass => vec![
                K::Prefetch,
                K::FileFiltering,
                K::TagFiltering,
                K::Locations,
                K::Tags,
                K::Notes,
            ],
            CallerType::ClientApi => vec![K::FileFiltering, K::Locations],
            _ => K::ALL.to_vec(),
        }
    } else {
        K::ALL.to_vec()
    };
    for kind in K::ALL {
        if kind.is_set(own) && !kinds.contains(&kind) {
            kinds.push(kind);
        }
    }
    kinds
}

/// Whose default a kind falls back to, for an importer whose defaults are
/// `caller`'s ("global", "subscription", ...): the most specific defaults
/// that set it.
pub fn source_label(manager: &ImportOptionsManager, caller: CallerType, kind: Kind) -> String {
    for layer in hydrus_core::import_options::preference_stack(caller, &[]) {
        if matches!(layer, CallerType::SpecificImporter | CallerType::UrlClass) {
            continue;
        }
        if manager
            .caller_default(layer)
            .is_some_and(|slice| kind.is_set(slice))
        {
            return layer.name().to_owned();
        }
    }
    "global".to_owned()
}

/// The "use the default" choice, naming whose.
pub fn use_default_label(source: &str) -> String {
    format!("use the default import options ({source})")
}

/// A kind's label in the list: "default <kind> (<whose>)", or "> <kind>:
/// <summary>" for custom options (its summary's first line).
pub fn tab_label(kind: Kind, custom_summary: Option<&str>, source: &str) -> String {
    match custom_summary {
        None => format!("default {} ({source})", kind.name()),
        Some(summary) => {
            let summary = summary.lines().next().unwrap_or_default();
            if summary.is_empty() {
                format!("> {}", kind.name())
            } else {
                format!("> {}: {summary}", kind.name())
            }
        }
    }
}

/// What a kind's options in `slice` (which must set it) say they do, as
/// the editor's list shows them for a downloader (`GetSummary`); `name`
/// names a service by its key (hex).
/// The container summary shown in subscription rows and favourites menus.
pub fn container_summary(slice: &ImportOptionsSlice, name: &dyn Fn(&str) -> String) -> String {
    let kinds = Kind::ALL
        .into_iter()
        .filter(|kind| kind.is_set(slice))
        .collect::<Vec<_>>();
    let names = kinds.iter().map(|kind| kind.name()).collect::<Vec<_>>();
    let summaries = kinds
        .iter()
        .map(|kind| summary(*kind, slice, name))
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>();
    if names.is_empty() {
        String::new()
    } else if names.len() <= 2 && (1..=2).contains(&summaries.len()) {
        format!("{}: {}", names.join(", "), summaries.join(" | "))
    } else {
        names.join(", ")
    }
}

pub fn summary(kind: Kind, slice: &ImportOptionsSlice, name: &dyn Fn(&str) -> String) -> String {
    use hydrus_core::import_options::PrefetchCheck as C;
    let mut parts: Vec<String> = Vec::new();
    match kind {
        Kind::Prefetch => {
            let Some(o) = &slice.prefetch else {
                return String::new();
            };
            match (o.hash_check, o.url_check) {
                (C::DoNotCheck, C::DoNotCheck) => {
                    parts.push("WARNING: always redownloads files!".into());
                }
                (C::DoNotCheck, _) => {
                    parts.push("WARNING: ignores hashes for file redownload checks!".into());
                }
                (_, C::DoNotCheck) => {
                    parts.push("WARNING: ignores URLs for file redownload checks!".into());
                }
                _ => {}
            }
            let by_hash = o.fetch_metadata_even_if_hash_recognised_and_file_already_in_db;
            let by_url = o.fetch_metadata_even_if_url_recognised_and_file_already_in_db;
            match (by_hash, by_url) {
                (true, true) => parts.push("always redownloads metadata!".into()),
                (true, false) => parts.push("ignores hashes for metadata redownload checks".into()),
                (false, true) => parts.push("ignores URLs for metadata redownload checks".into()),
                (false, false) => {}
            }
            if parts.is_empty() {
                parts.push("looks good".into());
            }
        }
        Kind::FileFiltering => {
            let Some(o) = &slice.file_filtering else {
                return String::new();
            };
            let set = hydrus_core::search::filetype::FiletypeSet::new(
                o.filetypes
                    .iter()
                    .filter_map(|&c| hydrus_core::mime::Mime::from_code(c)),
            );
            parts.push(format!(
                "allows {}",
                hydrus_search::text::filetypes_text(&set)
            ));
            if o.exclude_deleted {
                parts.push("excludes previously deleted".into());
            }
            if !o.allow_decompression_bombs {
                parts.push("excludes decompression bombs".into());
            }
            let bytes = hydrus_core::numbers::human_bytes;
            let int = |n: u32| hydrus_core::numbers::human_int(u64::from(n));
            if let Some(n) = o.min_size.filter(|&n| n > 0) {
                parts.push(format!("excludes < {}", bytes(n)));
            }
            if let Some(n) = o.max_size.filter(|&n| n > 0) {
                parts.push(format!("excludes > {}", bytes(n)));
            }
            if let Some(n) = o.max_gif_size.filter(|&n| n > 0) {
                parts.push(format!("excludes gifs > {}", bytes(n)));
            }
            if let Some((w, h)) = o.min_resolution {
                parts.push(format!("excludes < ( {} x {} )", int(w), int(h)));
            }
            if let Some((w, h)) = o.max_resolution {
                parts.push(format!("excludes > ( {} x {} )", int(w), int(h)));
            }
        }
        Kind::TagFiltering => {
            let Some(o) = &slice.tag_filtering else {
                return String::new();
            };
            if !o.blacklist.allows_everything() {
                parts.push(o.blacklist.to_blacklist_string());
            }
            if !o.whitelist.is_empty() {
                parts.push(format!("tag whitelist: {}", o.whitelist.join(", ")));
            }
            if parts.is_empty() {
                parts.push("allows everything".into());
            }
        }
        Kind::Locations => {
            let Some(o) = &slice.locations else {
                return String::new();
            };
            let mut names: Vec<String> = o.destinations.iter().map(|k| name(k)).collect();
            names.sort();
            parts.push(format!("imports to {}", names.join(", ")));
            if o.automatically_archive {
                parts.push("automatically archives".into());
            }
            if o.destinations_for_already_in_db {
                parts.push("puts files in destinations even if they are already in db".into());
            }
        }
        Kind::Tags => {
            let Some(o) = &slice.tags else {
                return String::new();
            };
            let services: Vec<String> = o
                .services
                .iter()
                .filter_map(|(key, s)| {
                    let mut sub = Vec::new();
                    if s.get_tags {
                        sub.push(s.get_tags_filter.to_filter_string());
                    }
                    if !s.additional_tags.is_empty() {
                        let mut tags = s.additional_tags.clone();
                        tags.sort();
                        sub.push(format!("adding \"{}\"", tags.join(", ")));
                    }
                    (!sub.is_empty()).then(|| format!("{}[{}]", name(key), sub.join(", ")))
                })
                .collect();
            return if services.is_empty() {
                "not adding any tags".into()
            } else {
                services.join(" | ")
            };
        }
        Kind::Notes => {
            let Some(o) = &slice.notes else {
                return String::new();
            };
            if o.get_notes {
                parts.push("adding notes".into());
                if !o.extend_existing_note_if_possible {
                    parts.push("duplicating rather than extending".into());
                }
                parts.push(format!(
                    "with conflict resolution: {}",
                    note_conflict_label(o.conflict)
                ));
                if o.all_name_override.is_some() || !o.name_overrides.is_empty() {
                    parts.push("with renames".into());
                }
            } else {
                parts.push("not adding notes".into());
            }
        }
        Kind::ExternalPrograms => return String::new(),
        Kind::Presentation => {
            return slice
                .presentation
                .as_ref()
                .map(crate::importer_menu::presentation_summary)
                .unwrap_or_default();
        }
    }
    parts.join(", ")
}

/// A note conflict's choice, as the notes page lists them.
pub fn note_conflict_label(conflict: hydrus_core::import_options::NoteConflict) -> &'static str {
    use hydrus_core::import_options::NoteConflict as N;
    match conflict {
        N::Replace => "replace the existing note",
        N::Ignore => "do not add the new note",
        N::Append => "append the new note to the end of the existing note",
        N::Rename => "add the new note under a new name",
    }
}

/// The editor while it is open: every kind's options (the importer's own,
/// or its defaults' to start from), which are custom, and the kind shown.
#[derive(Debug, Clone)]
pub struct Editor {
    pub caller: CallerType,
    pub kinds: Vec<Kind>,
    /// Every kind set: what its page shows.
    pub values: ImportOptionsSlice,
    pub custom: Vec<Kind>,
    /// The kind whose page is shown (an index into `kinds`).
    pub shown: usize,
    sources: Vec<String>,
}

impl Editor {
    /// The editor on an importer's own options, whose defaults are
    /// `caller`'s.
    pub fn new(
        manager: &ImportOptionsManager,
        caller: CallerType,
        simple: bool,
        own: &ImportOptionsSlice,
    ) -> Self {
        let full = manager.full(caller, None, &[]);
        let mut values = ImportOptionsSlice {
            prefetch: Some(full.prefetch),
            file_filtering: Some(full.file_filtering),
            tag_filtering: Some(full.tag_filtering),
            locations: Some(full.locations),
            tags: Some(full.tags),
            notes: Some(full.notes),
            presentation: Some(full.presentation),
            external_programs: Some(
                hydrus_core::import_options::ExternalProgramsOptions::default(),
            ),
        };
        let kinds = listed_kinds(caller, simple, own);
        let mut custom = Vec::new();
        for kind in Kind::ALL {
            if kind.is_set(own) {
                kind.copy(own, &mut values);
                custom.push(kind);
            }
        }
        let sources = Kind::ALL
            .iter()
            .map(|k| source_label(manager, caller, *k))
            .collect();
        Self {
            caller,
            kinds,
            values,
            custom,
            shown: 0,
            sources,
        }
    }

    /// Edit a defaults row using only its less-specific parent as fallback.
    /// URL classes use their post/watch context to choose that parent; their
    /// own override is never fed back into the displayed default values.
    pub fn new_for_defaults(
        manager: &ImportOptionsManager,
        caller: CallerType,
        simple: bool,
        own: &ImportOptionsSlice,
        url_classes: &[(String, hydrus_core::import_options::UrlClassKind)],
    ) -> Self {
        let stack = hydrus_core::import_options::preference_stack(caller, url_classes);
        let parent = if matches!(caller, CallerType::Global | CallerType::Favourites) {
            CallerType::Global
        } else {
            stack
                .iter()
                .position(|layer| *layer == caller)
                .and_then(|index| stack.get(index + 1))
                .copied()
                .unwrap_or(CallerType::Global)
        };
        let mut editor = Self::new(manager, parent, false, own);
        editor.caller = caller;
        editor.kinds = default_kinds(caller, simple, own);
        if own.external_programs.is_none() {
            editor.values.external_programs =
                hydrus_core::import_options::preference_stack(parent, &[])
                    .into_iter()
                    .filter_map(|layer| manager.caller_default(layer))
                    .find_map(|slice| slice.external_programs.clone())
                    .or_else(|| {
                        Some(hydrus_core::import_options::ExternalProgramsOptions::default())
                    });
        }
        editor
    }

    pub fn is_custom(&self, kind: Kind) -> bool {
        self.custom.contains(&kind)
    }

    /// Use custom options for a kind (starting from what its page shows),
    /// or the default.
    pub fn set_custom(&mut self, kind: Kind, custom: bool) {
        self.custom.retain(|k| *k != kind);
        if custom {
            self.custom.push(kind);
        }
    }

    /// Whose default a kind uses.
    pub fn source(&self, kind: Kind) -> &str {
        let i = Kind::ALL.iter().position(|k| *k == kind).unwrap_or(0);
        &self.sources[i]
    }

    /// The list's labels.
    pub fn labels(&self, name: &dyn Fn(&str) -> String) -> Vec<String> {
        self.kinds
            .iter()
            .map(|&kind| {
                let custom = self
                    .is_custom(kind)
                    .then(|| summary(kind, &self.values, name));
                tab_label(kind, custom.as_deref(), self.source(kind))
            })
            .collect()
    }

    /// The importer's options, as edited: the custom kinds alone.
    pub fn value(&self) -> ImportOptionsSlice {
        let mut slice = ImportOptionsSlice::default();
        for kind in &self.custom {
            kind.copy(&self.values, &mut slice);
        }
        slice
    }

    /// Set prefetch's hash check; both checks can't be dispositive, so the
    /// URL check gives way (`EditPrefetchImportOptionsPanel`).
    pub fn set_hash_check(&mut self, check: hydrus_core::import_options::PrefetchCheck) {
        use hydrus_core::import_options::PrefetchCheck as C;
        if let Some(o) = &mut self.values.prefetch {
            o.hash_check = check;
            if check == C::CheckAndMatchesAreDispositive && o.url_check == check {
                o.url_check = C::Check;
            }
        }
    }

    /// Set prefetch's URL check; the hash check gives way.
    pub fn set_url_check(&mut self, check: hydrus_core::import_options::PrefetchCheck) {
        use hydrus_core::import_options::PrefetchCheck as C;
        if let Some(o) = &mut self.values.prefetch {
            o.url_check = check;
            if check == C::CheckAndMatchesAreDispositive && o.hash_check == check {
                o.hash_check = C::Check;
            }
        }
    }

    /// Set presentation's status; "or in inbox" goes with "new files"
    /// alone (`_UpdateInboxChoices`).
    pub fn set_presentation_status(
        &mut self,
        status: hydrus_core::import_options::PresentationStatus,
    ) {
        use hydrus_core::import_options::{PresentationInbox as I, PresentationStatus as S};
        if let Some(o) = &mut self.values.presentation {
            o.status = status;
            if status != S::NewOnly && o.inbox == I::AndIncludeAllInbox {
                o.inbox = I::Agnostic;
            }
        }
    }
}

/// Prefetch's check choices, in order.
pub const CHECK_CHOICES: [&str; 3] = [
    "do not check",
    "check",
    "check - and matches are dispositive",
];

/// Presentation's status choices, in order.
pub const STATUS_CHOICES: [&str; 3] = ["all files", "new files", "do not show anything"];

/// Presentation's inbox choices for a status: "or in inbox" only with
/// "new files".
pub fn inbox_choices(status: hydrus_core::import_options::PresentationStatus) -> Vec<&'static str> {
    let mut choices = vec!["inbox or archive", "must be in inbox"];
    if status == hydrus_core::import_options::PresentationStatus::NewOnly {
        choices.push("or in inbox");
    }
    choices
}

/// A text's lines, trimmed, without blank ones.
pub fn lines(text: &str) -> Vec<String> {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(str::to_owned)
        .collect()
}

/// Note renames as text: "parser name -> saved name", a line each.
pub fn renames_text(renames: &[(String, String)]) -> String {
    renames
        .iter()
        .map(|(from, to)| format!("{from} -> {to}"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Note renames from their text; lines without " -> " are dropped.
pub fn parse_renames(text: &str) -> Vec<(String, String)> {
    lines(text)
        .iter()
        .filter_map(|l| {
            let (from, to) = l.split_once("->")?;
            let (from, to) = (from.trim(), to.trim());
            (!from.is_empty() && !to.is_empty()).then(|| (from.to_owned(), to.to_owned()))
        })
        .collect()
}

/// The notes page's conflict choices, in order (`NOTE_IMPORT_CONFLICT_*`).
pub const CONFLICT_CHOICES: [hydrus_core::import_options::NoteConflict; 4] = [
    hydrus_core::import_options::NoteConflict::Replace,
    hydrus_core::import_options::NoteConflict::Ignore,
    hydrus_core::import_options::NoteConflict::Append,
    hydrus_core::import_options::NoteConflict::Rename,
];

/// What the "get tags" filter's editor explains (not in advanced mode).
pub const GET_TAGS_FILTER_MESSAGE: &str = "Here you can filter which tags are applied to the files being imported in this context. This typically means those tags on a booru file page beside the file, but other contexts provide tags from different locations and quality.\n\nThe namespace checkboxes on the left are compiled from what all your current parsers say they can do and are simply for convenience. It is worth doing some smaller tests with a new download source to make sure you know what it can provide and what you actually want.\n\nOnce you are happy, you might want to say 'only \"character:\", \"creator:\" and \"series:\" tags', or 'everything _except_ \"species:\" tags'. This tag filter can get complicated if you want it to--check the help button in the top-right for more information.";

/// What the tag filtering blacklist's editor explains.
pub const BLACKLIST_MESSAGE: &str = "If a file about to be downloaded has a tag on the site that this blacklist blocks, the file will not be downloaded and imported. If you want to stop 'scat' or 'gore', just type them into the list.\n\nThis system tests the all tags that are parsed from the site, not any other tags the files may have in different places. Siblings of all those tags will also be tested. If none of your tag services have excellent siblings, it is worth adding multiple versions of your tag, just to catch different sites terms. Link up 'gore', 'guro', 'violence', etc...\n\nAdditionally, unnamespaced rules will apply to namespaced tags. 'low_resolution' in the blacklist will catch 'meta:low_resolution' as parsed from a site.\n\nIt is worth doing a small test here, just to make sure it is all set up how you want.";

/// The tag filtering blacklist button's tooltip.
pub const BLACKLIST_TOOLTIP: &str =
    "A blacklist will ignore files if they have any of a certain list of tags.";
