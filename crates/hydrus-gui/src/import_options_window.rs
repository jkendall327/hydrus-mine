//! The import options editor's window, bound (`ui/import_options.slint`):
//! hydrus-gui-model's [`Editor`](crate::import_options_editor::Editor)
//! shown as the reference's `EditSpecificImportOptionsContainerPanel`, the
//! list of kinds and the chosen kind's page. Every kind but external
//! programs is edited here (that can be set to custom, from its default,
//! or back). "apply" gives the importer's options to `done`.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;

use slint::{ComponentHandle as _, ModelRc, SharedString, VecModel};

use hydrus_core::ServiceKey;
use hydrus_core::import_options::{
    CallerType, ImportOptionsManager, ImportOptionsSlice, PrefetchCheck, PresentationInbox,
    PresentationStatus, UrlClassKind,
};
use hydrus_core::service::builtin_keys;
use hydrus_store::Store;
use hydrus_store::services::ServiceRegistry;

use crate::domains::{self, Choice, Flags};
use crate::import_options_editor::{
    CUSTOM_CHOICE, DESCRIPTION, EXISTING_TAGS_FILTER_MESSAGE, Editor, Kind, TagFilterTarget,
    inbox_choices, use_default_label,
};

/// What the additional tags dialog says (`_DoAdditionalTags`).
const ADDITIONAL_TAGS_MESSAGE: &str = "Any tags you enter here will be applied to every file that passes through this import context.";
/// What the whitelist dialog says (`_EditWhitelist`).
const WHITELIST_MESSAGE: &str = "If you add tags here, then any file importing with these options must have at least one of these tags from the download source. You can mix it with a blacklist--both will apply in turn.\n\nThis is usually easier and faster to do just by adding tags to the downloader query (e.g. \"artistname desired_tag\"), so reserve this for downloaders that do not work on tags or where you want to whitelist multiple tags.";
use crate::{ImportOptionsWindow, LocationsWindow, ResolutionLimit, SizeLimit, TagServiceRow};

fn strings(items: Vec<String>) -> ModelRc<SharedString> {
    let items: Vec<SharedString> = items.into_iter().map(Into::into).collect();
    ModelRc::new(VecModel::from(items))
}

/// A service's name, by its key (hex).
fn namer(services: &ServiceRegistry) -> impl Fn(&str) -> String + '_ {
    move |key: &str| {
        hex::decode(key)
            .ok()
            .and_then(|k| services.by_key(&ServiceKey::new(k)).ok())
            .map_or_else(|| "unknown service".into(), |s| s.name.clone())
    }
}

/// The two location buttons the editor has: the import destinations and
/// the presentation location.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Selector {
    Destination,
    Presentation,
}

impl Selector {
    fn named(which: &str) -> Option<Self> {
        match which {
            "destination" => Some(Self::Destination),
            "presentation" => Some(Self::Presentation),
            _ => None,
        }
    }

    /// What the button lets its caller choose
    /// (`SetOnlyImportableDomainsAllowed` on the destination button).
    fn flags(self, advanced: bool) -> Flags {
        match self {
            Self::Destination => Flags::importable(advanced),
            Self::Presentation => Flags::unrestricted(advanced),
        }
    }
}

pub(crate) fn advanced_mode(store: &Store) -> bool {
    store
        .read(hydrus_store::settings::get::<hydrus_store::settings::AdvancedMode>)
        .unwrap_or_default()
        .0
}

/// The location `which` button holds now.
fn selected_location(
    editor: &Editor,
    services: &ServiceRegistry,
    which: Selector,
) -> hydrus_search::LocationContext {
    let (current, deleted) = match which {
        Selector::Destination => editor
            .values
            .locations
            .as_ref()
            .map(|o| (o.destinations.clone(), o.deleted_destinations.clone())),
        Selector::Presentation => editor
            .values
            .presentation
            .as_ref()
            .map(|o| (o.location.clone(), o.deleted_location.clone())),
    }
    .unwrap_or_default();
    domains::context_from_hex(services, &current, &deleted)
}

/// Make the `which` button hold `location`.
fn select_location(
    editor: &mut Editor,
    which: Selector,
    location: &hydrus_search::LocationContext,
) {
    let (current, deleted) = domains::hex_of_context(location);
    match which {
        Selector::Destination => {
            if let Some(o) = &mut editor.values.locations {
                o.destinations = current;
                o.deleted_destinations = deleted;
            }
        }
        Selector::Presentation => {
            if let Some(o) = &mut editor.values.presentation {
                o.location = current;
                o.deleted_location = deleted;
            }
        }
    }
}

/// A location button's drop-down, its label and chosen row.
fn location_button(
    editor: &Editor,
    services: &ServiceRegistry,
    which: Selector,
    advanced: bool,
) -> (ModelRc<SharedString>, i32, SharedString) {
    let current = selected_location(editor, services, which);
    let (rows, at) = domains::dropdown(services, which.flags(advanced), &current);
    (
        strings(rows.into_iter().map(|r| r.label).collect()),
        at.and_then(|i| i32::try_from(i).ok()).unwrap_or(-1),
        domains::location_label(services, &current).into(),
    )
}

const UNITS: [u64; 4] = [1, 1024, 1024 * 1024, 1024 * 1024 * 1024];

/// A byte count as a number and the largest unit it is a whole number of.
fn size_fields(bytes: u64) -> (i32, i32) {
    for (i, unit) in UNITS.iter().enumerate().rev() {
        if bytes >= *unit && bytes.is_multiple_of(*unit) {
            return (
                i32::try_from(bytes / unit).unwrap_or(i32::MAX),
                i32::try_from(i).unwrap_or(0),
            );
        }
    }
    (i32::try_from(bytes).unwrap_or(i32::MAX), 0)
}

/// The size limits: their labels and the reference's values for a limit
/// switched on.
const SIZES: [(&str, u64); 3] = [
    ("minimum filesize: ", 5 * 1024),
    ("maximum filesize: ", 100 * 1024 * 1024),
    ("maximum gif filesize: ", 32 * 1024 * 1024),
];

const RESOLUTIONS: [(&str, (u32, u32)); 2] = [
    ("minimum resolution: ", (50, 50)),
    ("maximum resolution: ", (8192, 8192)),
];

fn check_index(check: PrefetchCheck) -> i32 {
    match check {
        PrefetchCheck::DoNotCheck => 0,
        PrefetchCheck::Check => 1,
        PrefetchCheck::CheckAndMatchesAreDispositive => 2,
    }
}

fn check_of(index: i32) -> PrefetchCheck {
    match index {
        0 => PrefetchCheck::DoNotCheck,
        1 => PrefetchCheck::Check,
        _ => PrefetchCheck::CheckAndMatchesAreDispositive,
    }
}

fn status_of(index: i32) -> PresentationStatus {
    match index {
        0 => PresentationStatus::AnyGood,
        1 => PresentationStatus::NewOnly,
        _ => PresentationStatus::None,
    }
}

/// The real tag services (local tag services and repositories): (key
/// hex, name).
fn tag_services(services: &ServiceRegistry) -> Vec<(String, String)> {
    services
        .all()
        .filter(|s| s.service_type().is_real_tag_service())
        .map(|s| (hex::encode(s.key.as_bytes()), s.name.clone()))
        .collect()
}

/// A tag service's options in the editor's tags, made (at their default)
/// if it has none.
fn service_options<'a>(
    tags: &'a mut hydrus_core::import_options::TagImportOptions,
    key: &str,
) -> &'a mut hydrus_core::import_options::ServiceTagImportOptions {
    if let Some(i) = tags.services.iter().position(|(k, _)| k == key) {
        &mut tags.services[i].1
    } else {
        tags.services.push((
            key.to_owned(),
            hydrus_core::import_options::ServiceTagImportOptions::default(),
        ));
        &mut tags.services.last_mut().expect("just pushed").1
    }
}

/// Set a text field only if it changed (setting it would move the cursor
/// of the field typed in).
fn set_text(get: impl Fn() -> SharedString, set: impl Fn(SharedString), text: String) {
    if get().as_str() != text {
        set(text.into());
    }
}

struct State {
    store: Arc<Store>,
    editor: Editor,
    manager: ImportOptionsManager,
    simple: bool,
    defaults: bool,
    url_classes: Vec<(String, UrlClassKind)>,
    services: Arc<hydrus_store::store::Snapshot>,
    /// The allowed filetypes' groups showing their filetypes.
    filetype_expanded: [bool; 7],
    /// A tag filter being edited.
    tag_filter: crate::tag_filter_window::Slot,
    write_tags: crate::write_tag_window::Slot,
    overwrite: crate::import_options_overwrite_window::Slot,
    /// The shared "multiple/deleted locations" list, from a location button.
    locations: Rc<RefCell<Option<LocationsWindow>>>,
    favourites: Option<Rc<crate::import_options_favourites_window::Controller>>,
}

impl State {
    fn replace_options(&mut self, options: &ImportOptionsSlice) {
        self.editor = if self.defaults {
            Editor::new_for_defaults(
                &self.manager,
                self.editor.caller,
                self.simple,
                options,
                &self.url_classes,
            )
        } else {
            Editor::new(&self.manager, self.editor.caller, self.simple, options)
        };
    }
}

/// Show the editor whole: the list and the shown kind's page.
fn show(window: &ImportOptionsWindow, state: &State) {
    let formatting = hydrus_gui_model::gui_format::preferences(&state.store);
    let editor = &state.editor;
    let services = &state.services.services;
    let name = namer(services);
    window.set_labels(strings(editor.labels_with_format(&name, &formatting)));
    window.set_shown(i32::try_from(editor.shown).unwrap_or(0));
    let Some(&kind) = editor.kinds.get(editor.shown) else {
        return;
    };
    window.set_kind(kind.name().into());
    window.set_kind_description(kind.description().into());
    window.set_custom_choices(strings(vec![
        use_default_label(editor.source(kind)),
        CUSTOM_CHOICE.to_owned(),
    ]));
    window.set_custom_index(i32::from(editor.is_custom(kind)));
    let values = &editor.values;
    window.set_summary(
        match kind {
            Kind::ExternalPrograms => format!(
                "{} (not editable here yet)",
                crate::import_options_editor::summary_with_format(kind, values, &name, &formatting)
            ),
            _ => String::new(),
        }
        .into(),
    );
    if let Some(o) = &values.presentation {
        window.set_status_choices(strings(
            crate::import_options_editor::STATUS_CHOICES
                .iter()
                .map(|s| (*s).to_owned())
                .collect(),
        ));
        window.set_status_index(match o.status {
            PresentationStatus::AnyGood => 0,
            PresentationStatus::NewOnly => 1,
            PresentationStatus::None => 2,
        });
        window.set_inbox_choices(strings(
            inbox_choices(o.status)
                .into_iter()
                .map(str::to_owned)
                .collect(),
        ));
        window.set_inbox_index(match o.inbox {
            PresentationInbox::Agnostic => 0,
            PresentationInbox::RequireInbox => 1,
            PresentationInbox::AndIncludeAllInbox => 2,
        });
        let (choices, at, label) = location_button(
            editor,
            services,
            Selector::Presentation,
            advanced_mode(&state.store),
        );
        window.set_location_choices(choices);
        window.set_location_index(at);
        window.set_location_label(label);
    }
    if let Some(o) = &values.prefetch {
        window.set_hash_index(check_index(o.hash_check));
        window.set_url_index(check_index(o.url_check));
        window.set_fetch_hash(o.fetch_metadata_even_if_hash_recognised_and_file_already_in_db);
        window.set_fetch_url(o.fetch_metadata_even_if_url_recognised_and_file_already_in_db);
        window.set_neighbour_spam(o.url_check_looks_for_neighbour_spam);
    }
    if let Some(o) = &values.file_filtering {
        let set = hydrus_core::search::filetype::FiletypeSet::new(
            o.filetypes
                .iter()
                .filter_map(|&c| hydrus_core::mime::Mime::from_code(c)),
        );
        window.set_filetypes(hydrus_search::text::filetypes_text(&set).into());
        window.set_filetype_rows(crate::predicate_editor_window::tree_rows(
            &crate::filetype_tree::groups(&o.filetypes, &state.filetype_expanded),
        ));
        window.set_exclude_deleted(o.exclude_deleted);
        window.set_bombs(o.allow_decompression_bombs);
        let sizes: Vec<SizeLimit> = [o.min_size, o.max_size, o.max_gif_size]
            .iter()
            .zip(SIZES)
            .map(|(value, (label, default))| {
                let (value, unit) = size_fields(value.unwrap_or(default));
                SizeLimit {
                    label: label.into(),
                    on: value_set(o, label),
                    value,
                    unit,
                }
            })
            .collect();
        window.set_sizes(ModelRc::new(VecModel::from(sizes)));
        let resolutions: Vec<ResolutionLimit> = [o.min_resolution, o.max_resolution]
            .iter()
            .zip(RESOLUTIONS)
            .map(|(value, (label, default))| {
                let (w, h) = value.unwrap_or(default);
                ResolutionLimit {
                    label: label.into(),
                    on: value.is_some(),
                    width: i32::try_from(w).unwrap_or(i32::MAX),
                    height: i32::try_from(h).unwrap_or(i32::MAX),
                }
            })
            .collect();
        window.set_resolutions(ModelRc::new(VecModel::from(resolutions)));
    }
    if let Some(o) = &values.notes {
        window.set_get_notes(o.get_notes);
        window.set_extend_notes(o.extend_existing_note_if_possible);
        window.set_conflict_index(
            crate::import_options_editor::CONFLICT_CHOICES
                .iter()
                .position(|c| *c == o.conflict)
                .and_then(|i| i32::try_from(i).ok())
                .unwrap_or(3),
        );
        set_text(
            || window.get_note_whitelist(),
            |t| window.set_note_whitelist(t),
            o.name_whitelist.join("\n"),
        );
        set_text(
            || window.get_note_renames(),
            |t| window.set_note_renames(t),
            crate::import_options_editor::renames_text(&o.name_overrides),
        );
        window.set_rename_all_on(o.all_name_override.is_some());
        set_text(
            || window.get_rename_all(),
            |t| window.set_rename_all(t),
            o.all_name_override.clone().unwrap_or_default(),
        );
    }
    if let Some(o) = &values.tag_filtering {
        let (label, _) = crate::tag_filter_editor::button_label(&o.blacklist, true, "", false);
        window.set_tag_blacklist(label.into());
        set_text(
            || window.get_tag_whitelist(),
            |t| window.set_tag_whitelist(t),
            o.whitelist.join("\n"),
        );
    }
    if let Some(o) = &values.tags {
        window.set_gets_no_tags(!o.worth_fetching_tags() && !o.has_additional_tags());
    }
    if let Some(o) = &values.locations {
        let (choices, at, label) = location_button(
            editor,
            services,
            Selector::Destination,
            advanced_mode(&state.store),
        );
        window.set_destination_choices(choices);
        window.set_destination_index(at);
        window.set_destination_label(label);
        window.set_destination_warning(
            selected_location(editor, services, Selector::Destination).is_empty(),
        );
        window.set_already_destinations(o.destinations_for_already_in_db);
        window.set_archive(o.automatically_archive);
        window.set_archive_already(o.archive_already_in_db);
        window.set_primary_urls(o.associate_primary_urls);
        window.set_source_urls(o.associate_source_urls);
    }
}

/// Show the tags page's services (made anew: only when the page is shown
/// or switched, not as their texts are typed).
fn show_tag_services(window: &ImportOptionsWindow, state: &State) {
    let Some(o) = &state.editor.values.tags else {
        return;
    };
    let default = hydrus_core::import_options::ServiceTagImportOptions::default();
    let rows: Vec<TagServiceRow> = tag_services(&state.services.services)
        .into_iter()
        .map(|(key, name)| {
            let s = o.service(&key).unwrap_or(&default);
            TagServiceRow {
                name: name.into(),
                get_tags: s.get_tags,
                filter: crate::tag_filter_editor::button_label(
                    &s.get_tags_filter,
                    false,
                    "adding: ",
                    true,
                )
                .0
                .into(),
                additional: format!(
                    "{} additional tags",
                    hydrus_core::numbers::human_int(s.additional_tags.len() as u64)
                )
                .into(),
                to_new: s.to_new_files,
                to_inbox: s.to_already_in_inbox,
                to_archive: s.to_already_in_archive,
                get_overwrite: s.get_tags_overwrite_deleted,
                additional_overwrite: s.additional_tags_overwrite_deleted,
                only_existing: s.only_add_existing_tags,
            }
        })
        .collect();
    window.set_tag_services(ModelRc::new(VecModel::from(rows)));
}

/// Whether a size limit (by its label) is set.
fn value_set(o: &hydrus_core::import_options::FileFilteringOptions, label: &str) -> bool {
    match label {
        "minimum filesize: " => o.min_size.is_some(),
        "maximum filesize: " => o.max_size.is_some(),
        _ => o.max_gif_size.is_some(),
    }
}

/// Read the shown page's two-way fields into the editor.
fn read(window: &ImportOptionsWindow, state: &mut State) {
    let editor = &mut state.editor;
    let Some(&kind) = editor.kinds.get(editor.shown) else {
        return;
    };
    editor.set_custom(kind, window.get_custom_index() == 1);
    if !editor.is_custom(kind) {
        return;
    }
    match kind {
        Kind::Presentation => {
            editor.set_presentation_status(status_of(window.get_status_index()));
            if let Some(o) = &mut editor.values.presentation {
                let choices = inbox_choices(o.status).len();
                o.inbox = match window.get_inbox_index() {
                    1 => PresentationInbox::RequireInbox,
                    2 if choices == 3 => PresentationInbox::AndIncludeAllInbox,
                    _ => PresentationInbox::Agnostic,
                };
            }
        }
        Kind::Prefetch => {
            let (hash, url) = (window.get_hash_index(), window.get_url_index());
            let old = editor.values.prefetch.clone().unwrap_or_default();
            // (the one changed wins: the other gives way)
            if check_of(hash) == old.hash_check {
                editor.set_url_check(check_of(url));
            } else {
                editor.set_hash_check(check_of(hash));
            }
            if let Some(o) = &mut editor.values.prefetch {
                o.fetch_metadata_even_if_hash_recognised_and_file_already_in_db =
                    window.get_fetch_hash();
                o.fetch_metadata_even_if_url_recognised_and_file_already_in_db =
                    window.get_fetch_url();
                o.url_check_looks_for_neighbour_spam = window.get_neighbour_spam();
            }
        }
        Kind::FileFiltering => {
            if let Some(o) = &mut editor.values.file_filtering {
                o.exclude_deleted = window.get_exclude_deleted();
                o.allow_decompression_bombs = window.get_bombs();
            }
        }
        Kind::Notes => {
            if let Some(o) = &mut editor.values.notes {
                o.get_notes = window.get_get_notes();
                o.extend_existing_note_if_possible = window.get_extend_notes();
                if let Some(c) = usize::try_from(window.get_conflict_index())
                    .ok()
                    .and_then(|i| crate::import_options_editor::CONFLICT_CHOICES.get(i))
                {
                    o.conflict = *c;
                }
                o.name_whitelist =
                    crate::import_options_editor::lines(&window.get_note_whitelist());
                o.name_overrides =
                    crate::import_options_editor::parse_renames(&window.get_note_renames());
                o.all_name_override = window
                    .get_rename_all_on()
                    .then(|| window.get_rename_all().to_string());
            }
        }
        Kind::TagFiltering => {
            if let Some(o) = &mut editor.values.tag_filtering {
                o.whitelist = crate::import_options_editor::lines(&window.get_tag_whitelist());
            }
        }
        Kind::Locations => {
            if let Some(o) = &mut editor.values.locations {
                o.destinations_for_already_in_db = window.get_already_destinations();
                o.automatically_archive = window.get_archive();
                o.archive_already_in_db = window.get_archive_already();
                o.associate_primary_urls = window.get_primary_urls();
                o.associate_source_urls = window.get_source_urls();
            }
        }
        _ => {}
    }
}

/// Open the editor on an importer's own options (`own`), whose defaults
/// are `caller`'s; "apply" gives the edited options to `done`. It forgets
/// itself from `slot` when closed.
pub(crate) fn open(
    store: &Arc<Store>,
    caller: CallerType,
    own: &ImportOptionsSlice,
    slot: &Rc<RefCell<Option<ImportOptionsWindow>>>,
    done: Rc<dyn Fn(ImportOptionsSlice)>,
) -> Result<ImportOptionsWindow, String> {
    open_inner(
        store,
        Context {
            caller,
            manager: None,
            simple: store
                .read(
                    hydrus_store::settings::get::<hydrus_store::settings::ImportOptionsUiSettings>,
                )
                .map_err(|e| e.to_string())?
                .simple,
            url_classes: Vec::new(),
        },
        own,
        slot,
        None,
        Rc::new(move |_, value| done(value)),
        Rc::new(|| {}),
    )
}

pub(crate) fn open_named(
    store: &Arc<Store>,
    own: &ImportOptionsSlice,
    slot: &Rc<RefCell<Option<ImportOptionsWindow>>>,
    name: &str,
    done: Rc<dyn Fn(String, ImportOptionsSlice)>,
    closed: Rc<dyn Fn()>,
) -> Result<ImportOptionsWindow, String> {
    open_inner(
        store,
        Context {
            caller: CallerType::Favourites,
            manager: None,
            simple: store
                .read(
                    hydrus_store::settings::get::<hydrus_store::settings::ImportOptionsUiSettings>,
                )
                .map_err(|e| e.to_string())?
                .simple,
            url_classes: Vec::new(),
        },
        own,
        slot,
        Some(name),
        done,
        closed,
    )
}

/// Manager and presentation preferences from an owning Options draft.
#[derive(Debug, Clone)]
pub(crate) struct StagedOptions {
    pub manager: Rc<RefCell<ImportOptionsManager>>,
    pub simple: bool,
    pub name: Option<String>,
    pub url_classes: Vec<(String, UrlClassKind)>,
}

struct Context {
    caller: CallerType,
    manager: Option<Rc<RefCell<ImportOptionsManager>>>,
    simple: bool,
    url_classes: Vec<(String, UrlClassKind)>,
}

/// Open a child against the same staged defaults/profiles as its Options owner.
pub(crate) fn open_staged(
    store: &Arc<Store>,
    caller: CallerType,
    own: &ImportOptionsSlice,
    slot: &Rc<RefCell<Option<ImportOptionsWindow>>>,
    configuration: StagedOptions,
    done: Rc<dyn Fn(String, ImportOptionsSlice)>,
    closed: Rc<dyn Fn()>,
) -> Result<ImportOptionsWindow, String> {
    open_inner(
        store,
        Context {
            caller,
            manager: Some(configuration.manager),
            simple: configuration.simple,
            url_classes: configuration.url_classes,
        },
        own,
        slot,
        configuration.name.as_deref(),
        done,
        closed,
    )
}

fn open_inner(
    store: &Arc<Store>,
    context: Context,
    own: &ImportOptionsSlice,
    slot: &Rc<RefCell<Option<ImportOptionsWindow>>>,
    name: Option<&str>,
    done: Rc<dyn Fn(String, ImportOptionsSlice)>,
    closed: Rc<dyn Fn()>,
) -> Result<ImportOptionsWindow, String> {
    let caller = context.caller;
    let defaults = context.manager.is_some();
    let manager: ImportOptionsManager = context.manager.as_ref().map_or_else(
        || {
            store
                .read(hydrus_store::settings::get)
                .map_err(|e| e.to_string())
        },
        |manager| Ok(manager.borrow().clone()),
    )?;
    let editor = if defaults {
        Editor::new_for_defaults(&manager, caller, context.simple, own, &context.url_classes)
    } else {
        Editor::new(&manager, caller, context.simple, own)
    };
    let active = Rc::new(Cell::new(true));
    let window =
        crate::app_title::new::<crate::ImportOptionsWindow>().map_err(|e| e.to_string())?;
    window.set_favourite_editor(caller == CallerType::Favourites);
    window.set_favourite_name(name.unwrap_or_default().into());
    if caller == CallerType::Favourites {
        window.set_window_title("edit favourite import options".into());
        window.set_description("You are editing \"favourites template\".\n\nThis is a template you can load and paste wherever you need it.".into());
    } else {
        window.set_description(DESCRIPTION.into());
    }
    window.set_downloader(!matches!(
        caller,
        CallerType::LocalImport | CallerType::LocalImportFolder | CallerType::ClientApi
    ));
    let state = Rc::new(RefCell::new(State {
        store: store.clone(),
        editor,
        manager,
        simple: context.simple,
        defaults,
        url_classes: context.url_classes,
        services: store.snapshot(),
        filetype_expanded: [false; 7],
        tag_filter: Rc::default(),
        write_tags: Rc::default(),
        overwrite: Rc::default(),
        locations: Rc::default(),
        favourites: None,
    }));
    let close = {
        let active = active.clone();
        let weak = window.as_weak();
        let slot = slot.clone();
        let state = state.clone();
        move || {
            if !active.replace(false) {
                return;
            }
            let favourites = state.borrow().favourites.clone();
            if let Some(favourites) = favourites {
                favourites.close();
            }
            crate::locations_window::cancel(&state.borrow().locations);
            let overwrite = state.borrow().overwrite.borrow_mut().take();
            if let Some(overwrite) = overwrite {
                overwrite.invoke_cancel();
            }
            let filter = state.borrow().tag_filter.borrow_mut().take();
            if let Some(filter) = filter {
                filter.invoke_cancel();
            }
            let child = state.borrow().write_tags.borrow_mut().take();
            if let Some(child) = child {
                child.invoke_cancel();
            }
            if let Some(window) = weak.upgrade() {
                let _ = window.hide();
            }
            slot.borrow_mut().take();
            closed();
        }
    };
    // The child captures its field/service identity and updates only this owner.
    let edit_filter = {
        let weak = window.as_weak();
        let state = state.clone();
        let store = store.clone();
        let active = active.clone();
        move |target: TagFilterTarget| {
            let Some(window) = weak.upgrade() else {
                return;
            };
            if !active.get()
                || window.get_favourite_child_open()
                || window.get_tag_child_open()
                || window.get_overwrite_open()
            {
                return;
            }
            let (filter, slot) = {
                let state = state.borrow();
                (state.editor.tag_filter(&target), state.tag_filter.clone())
            };
            let Some(filter) = filter else {
                return;
            };
            let advanced = store
                .read(hydrus_store::settings::get::<hydrus_store::settings::AdvancedMode>)
                .unwrap_or_default()
                .0;
            let message = match &target {
                TagFilterTarget::Blacklist => crate::import_options_editor::BLACKLIST_MESSAGE,
                TagFilterTarget::ExistingTags(_) => EXISTING_TAGS_FILTER_MESSAGE,
                TagFilterTarget::ParsedTags(_) if advanced => "",
                TagFilterTarget::ParsedTags(_) => {
                    crate::import_options_editor::GET_TAGS_FILTER_MESSAGE
                }
            };
            let blacklist_only = matches!(target, TagFilterTarget::Blacklist);
            let title = if matches!(target, TagFilterTarget::ExistingTags(_)) {
                "edit already-exist filter"
            } else {
                crate::tag_filter_editor::title(blacklist_only)
            };
            let applied: Rc<dyn Fn(hydrus_core::tag_filter::TagFilter)> = {
                let weak = window.as_weak();
                let state = state.clone();
                let active = active.clone();
                Rc::new(move |filter| {
                    if !active.get() {
                        return;
                    }
                    let Some(window) = weak.upgrade() else {
                        return;
                    };
                    state.borrow_mut().editor.set_tag_filter(&target, filter);
                    show(&window, &state.borrow());
                    show_tag_services(&window, &state.borrow());
                })
            };
            match crate::tag_filter_window::open(
                &store,
                &filter,
                blacklist_only,
                title,
                message,
                &slot,
                applied,
            ) {
                Ok(editor) => {
                    let weak = window.as_weak();
                    editor.on_closed(move || {
                        if let Some(window) = weak.upgrade() {
                            window.set_tag_child_open(false);
                        }
                    });
                    *slot.borrow_mut() = Some(editor);
                    window.set_tag_child_open(true);
                }
                Err(error) => eprintln!("could not open the tag filter editor: {error}"),
            }
        }
    };
    window.on_edit_blacklist({
        let edit_filter = edit_filter.clone();
        move || edit_filter(TagFilterTarget::Blacklist)
    });
    window.on_edit_get_tags_filter({
        let edit_filter = edit_filter.clone();
        let state = state.clone();
        move |index| {
            let key = usize::try_from(index).ok().and_then(|index| {
                tag_services(&state.borrow().services.services)
                    .get(index)
                    .map(|service| service.0.clone())
            });
            if let Some(key) = key {
                edit_filter(TagFilterTarget::ParsedTags(key));
            }
        }
    });
    window.on_edit_existing_tags_filter({
        let state = state.clone();
        move |index| {
            let key = usize::try_from(index).ok().and_then(|index| {
                tag_services(&state.borrow().services.services)
                    .get(index)
                    .map(|service| service.0.clone())
            });
            if let Some(key) = key {
                edit_filter(TagFilterTarget::ExistingTags(key));
            }
        }
    });
    // Detached write-autocomplete lists return only their accepted tags to this draft.
    let edit_tags = {
        let state = state.clone();
        let store = store.clone();
        let weak = window.as_weak();
        move |service: Option<usize>| {
            let Some(window) = weak.upgrade() else {
                return;
            };
            if window.get_favourite_child_open() || window.get_tag_child_open() {
                return;
            }
            if !window.window().is_visible() {
                return;
            }
            let (key, initial, child_slot) = {
                let mut state = state.borrow_mut();
                read(&window, &mut state);
                match service {
                    None => {
                        let Some(options) = &state.editor.values.tag_filtering else {
                            return;
                        };
                        (
                            ServiceKey::new(builtin_keys::COMBINED_TAG.to_vec()),
                            options.whitelist.clone(),
                            state.write_tags.clone(),
                        )
                    }
                    Some(i) => {
                        let services = tag_services(&state.services.services);
                        let Some((hex_key, _)) = services.get(i) else {
                            return;
                        };
                        let Some(options) = &state.editor.values.tags else {
                            return;
                        };
                        let Ok(key) = hex::decode(hex_key) else {
                            return;
                        };
                        (
                            ServiceKey::new(key),
                            options
                                .service(hex_key)
                                .map(|s| s.additional_tags.clone())
                                .unwrap_or_default(),
                            state.write_tags.clone(),
                        )
                    }
                }
            };
            let applied = Rc::new({
                let state = state.clone();
                let weak = window.as_weak();
                let target = service.map(|_| key.to_hex());
                move |tags| {
                    let Some(w) = weak.upgrade() else {
                        return;
                    };
                    if !w.window().is_visible() {
                        return;
                    }
                    let mut state = state.borrow_mut();
                    match &target {
                        None => {
                            if let Some(options) = &mut state.editor.values.tag_filtering {
                                options.whitelist = tags;
                            }
                        }
                        Some(key) => {
                            if let Some(options) = &mut state.editor.values.tags {
                                service_options(options, key).additional_tags = tags;
                            }
                        }
                    }
                    show(&w, &state);
                    show_tag_services(&w, &state);
                }
            });
            let closed = Rc::new({
                let weak = window.as_weak();
                move || {
                    if let Some(w) = weak.upgrade() {
                        w.set_tag_child_open(false);
                    }
                }
            });
            let title = if service.is_some() {
                "edit additional tags"
            } else {
                "edit file whitelist"
            };
            match crate::write_tag_window::open(
                &store,
                key,
                &initial,
                title,
                &child_slot,
                applied,
                closed,
            ) {
                Ok(child) => {
                    child.set_message(
                        if service.is_some() {
                            ADDITIONAL_TAGS_MESSAGE
                        } else {
                            WHITELIST_MESSAGE
                        }
                        .into(),
                    );
                    window.set_tag_child_open(true);
                }
                Err(e) => eprintln!("could not open write tag editor: {e}"),
            }
        }
    };
    window.on_edit_whitelist({
        let edit_tags = edit_tags.clone();
        move || edit_tags(None)
    });
    window.on_edit_additional_tags(move |i| {
        if let Ok(i) = usize::try_from(i) {
            edit_tags(Some(i));
        }
    });
    window.on_kind_clicked({
        let weak = window.as_weak();
        let state = state.clone();
        move |i| {
            let Some(window) = weak.upgrade() else {
                return;
            };
            if window.get_favourite_child_open() || window.get_tag_child_open() {
                return;
            }
            if let Ok(i) = usize::try_from(i) {
                state.borrow_mut().editor.shown = i;
            }
            show(&window, &state.borrow());
            show_tag_services(&window, &state.borrow());
        }
    });
    window.on_filetype_expanded({
        let weak = window.as_weak();
        let state = state.clone();
        move |group, on| {
            let Some(window) = weak.upgrade() else {
                return;
            };
            if window.get_favourite_child_open() || window.get_tag_child_open() {
                return;
            }
            {
                let mut state = state.borrow_mut();
                if let Some(e) = usize::try_from(group)
                    .ok()
                    .and_then(|g| state.filetype_expanded.get_mut(g))
                {
                    *e = on;
                }
            }
            show(&window, &state.borrow());
        }
    });
    window.on_filetype_ticked({
        let weak = window.as_weak();
        let state = state.clone();
        move |group, option, on| {
            let Some(window) = weak.upgrade() else {
                return;
            };
            if window.get_favourite_child_open() || window.get_tag_child_open() {
                return;
            }
            if let Some(o) = &mut state.borrow_mut().editor.values.file_filtering {
                o.filetypes = crate::filetype_tree::tick(
                    &o.filetypes,
                    usize::try_from(group).unwrap_or(usize::MAX),
                    usize::try_from(option).ok(),
                    on,
                );
            }
            show(&window, &state.borrow());
        }
    });
    window.on_changed({
        let weak = window.as_weak();
        let state = state.clone();
        move || {
            let Some(window) = weak.upgrade() else {
                return;
            };
            if window.get_favourite_child_open()
                || window.get_tag_child_open()
                || window.get_location_child_open()
            {
                return;
            }
            read(&window, &mut state.borrow_mut());
            show(&window, &state.borrow());
        }
    });
    window.on_location_picked({
        let weak = window.as_weak();
        let state = state.clone();
        let store = store.clone();
        let active = active.clone();
        move |which, index| {
            let (Some(window), Some(which)) = (weak.upgrade(), Selector::named(&which)) else {
                return;
            };
            if !active.get()
                || window.get_favourite_child_open()
                || window.get_tag_child_open()
                || window.get_overwrite_open()
                || window.get_location_child_open()
            {
                return;
            }
            let services = state.borrow().services.clone();
            let flags = which.flags(advanced_mode(&store));
            let current = selected_location(&state.borrow().editor, &services.services, which);
            let (rows, _) = domains::dropdown(&services.services, flags, &current);
            if let Some(row) = usize::try_from(index).ok().and_then(|i| rows.get(i)) {
                match &row.choice {
                    Choice::Location(location) => {
                        select_location(&mut state.borrow_mut().editor, which, location);
                    }
                    Choice::Multiple => {
                        let slot = state.borrow().locations.clone();
                        let chosen = Rc::new({
                            let weak = weak.clone();
                            let state = state.clone();
                            move |location: hydrus_search::LocationContext| {
                                select_location(&mut state.borrow_mut().editor, which, &location);
                                if let Some(window) = weak.upgrade() {
                                    show(&window, &state.borrow());
                                }
                            }
                        });
                        match crate::locations_window::open_with_flags(
                            &slot,
                            store.clone(),
                            &current,
                            chosen,
                            flags,
                        ) {
                            Ok(()) => {
                                window.set_location_child_open(true);
                                if let Some(child) = slot.borrow().as_ref() {
                                    child.on_closed({
                                        let weak = weak.clone();
                                        let state = state.clone();
                                        move || {
                                            if let Some(window) = weak.upgrade() {
                                                window.set_location_child_open(false);
                                                show(&window, &state.borrow());
                                            }
                                        }
                                    });
                                }
                            }
                            Err(error) => eprintln!("could not open the locations list: {error}"),
                        }
                    }
                    Choice::Tags(_) => {}
                }
            }
            show(&window, &state.borrow());
        }
    });
    window.on_tag_service_toggled({
        let weak = window.as_weak();
        let state = state.clone();
        move |i, field, on| {
            let Some(window) = weak.upgrade() else {
                return;
            };
            if window.get_favourite_child_open() || window.get_tag_child_open() {
                return;
            }
            {
                let mut state = state.borrow_mut();
                let services = tag_services(&state.services.services);
                let Some((key, _)) = usize::try_from(i).ok().and_then(|i| services.get(i)) else {
                    return;
                };
                if let Some(tags) = &mut state.editor.values.tags {
                    let s = service_options(tags, key);
                    match field.as_str() {
                        "get-tags" => s.get_tags = on,
                        "to-new" => s.to_new_files = on,
                        "to-inbox" => s.to_already_in_inbox = on,
                        "to-archive" => s.to_already_in_archive = on,
                        "get-overwrite" => s.get_tags_overwrite_deleted = on,
                        "additional-overwrite" => s.additional_tags_overwrite_deleted = on,
                        _ => s.only_add_existing_tags = on,
                    }
                }
            }
            show(&window, &state.borrow());
            show_tag_services(&window, &state.borrow());
        }
    });
    window.on_size_edited({
        let weak = window.as_weak();
        let state = state.clone();
        move |i, on, value, unit| {
            let Some(window) = weak.upgrade() else {
                return;
            };
            if window.get_favourite_child_open() || window.get_tag_child_open() {
                return;
            }
            {
                let mut state = state.borrow_mut();
                if let Some(o) = &mut state.editor.values.file_filtering {
                    let unit = UNITS[usize::try_from(unit).unwrap_or(0).min(3)];
                    let bytes = on.then(|| u64::try_from(value.max(1)).unwrap_or(1) * unit);
                    match i {
                        0 => o.min_size = bytes,
                        1 => o.max_size = bytes,
                        _ => o.max_gif_size = bytes,
                    }
                }
            }
            show(&window, &state.borrow());
        }
    });
    window.on_resolution_edited({
        let weak = window.as_weak();
        let state = state.clone();
        move |i, on, width, height| {
            let Some(window) = weak.upgrade() else {
                return;
            };
            if window.get_favourite_child_open() || window.get_tag_child_open() {
                return;
            }
            {
                let mut state = state.borrow_mut();
                if let Some(o) = &mut state.editor.values.file_filtering {
                    let size = |n: i32| u32::try_from(n.max(1)).unwrap_or(1);
                    let value = on.then(|| (size(width), size(height)));
                    if i == 0 {
                        o.min_resolution = value;
                    } else {
                        o.max_resolution = value;
                    }
                }
            }
            show(&window, &state.borrow());
        }
    });
    let favourites = crate::import_options_favourites_window::Controller::new_inner(
        store.clone(),
        caller,
        context.manager,
        Rc::new({
            let state = Rc::downgrade(&state);
            let active = active.clone();
            move || {
                if !active.get() {
                    return None;
                }
                state.upgrade().map(|state| state.borrow().editor.value())
            }
        }),
        Rc::new({
            let state = Rc::downgrade(&state);
            let weak = window.as_weak();
            let active = active.clone();
            move |options| {
                if !active.get() {
                    return;
                }
                let Some(state) = state.upgrade() else {
                    return;
                };
                let mut state = state.borrow_mut();
                state.replace_options(&options);
                if let Some(window) = weak.upgrade() {
                    show(&window, &state);
                    show_tag_services(&window, &state);
                }
            }
        }),
        Rc::new({
            let weak = window.as_weak();
            move |error| {
                if let Some(window) = weak.upgrade() {
                    window.set_clipboard_error(error.into());
                }
            }
        }),
        Rc::new({
            let weak = window.as_weak();
            move |busy| {
                if let Some(window) = weak.upgrade() {
                    window.set_favourite_child_open(busy);
                }
            }
        }),
    );
    favourites.set_simple_mode(context.simple);
    state.borrow_mut().favourites = Some(favourites.clone());
    let refresh_favourites = Rc::new({
        let favourites = favourites.clone();
        let weak = window.as_weak();
        move || {
            let Some(window) = weak.upgrade() else {
                return;
            };
            match favourites.rows() {
                Ok(rows) => window.set_favourites(ModelRc::new(VecModel::from(rows))),
                Err(error) => window.set_clipboard_error(error.into()),
            }
        }
    });
    window.on_refresh_favourites({
        let refresh = refresh_favourites.clone();
        move || refresh()
    });
    window.on_favourite({
        let weak = window.as_weak();
        let refresh = refresh_favourites.clone();
        move |action, name| {
            if weak
                .upgrade()
                .is_none_or(|window| window.get_tag_child_open())
            {
                return;
            }
            favourites.choose(action, name.as_str());
            refresh();
        }
    });
    refresh_favourites();
    window.on_copy_options({
        let active = active.clone();
        let state = state.clone();
        let weak = window.as_weak();
        move || {
            if !active.get()
                || state.borrow().tag_filter.borrow().is_some()
                || state.borrow().write_tags.borrow().is_some()
            {
                return;
            }
            let result = hydrus_downloader_exchange::import_options::encode_text(
                &state.borrow().editor.value(),
            );
            match result {
                Ok(text) => crate::to_clipboard(&crate::Clip::Text(text)),
                Err(error) => {
                    if let Some(window) = weak.upgrade() {
                        window.set_clipboard_error(error.to_string().into());
                    }
                }
            }
        }
    });
    window.on_paste_options({
        let active = active.clone(); let state = state.clone(); let weak = window.as_weak(); let store = store.clone();
        move |index| {
            if !active.get() || state.borrow().tag_filter.borrow().is_some() || state.borrow().write_tags.borrow().is_some() || state.borrow().overwrite.borrow().is_some() || state.borrow().favourites.as_ref().is_some_and(|owner| owner.busy()) { return; }
            let Some(window) = weak.upgrade() else { return; };
            let result = crate::from_clipboard().and_then(|text| hydrus_downloader_exchange::import_options::decode_text(&text).map_err(|e| e.to_string()));
            let incoming = match result { Ok(options) => options, Err(error) => { window.set_clipboard_error(format!("Could not understand the clipboard as JSON-serialised Import Options Container.\n\n{error}").into()); return; } };
            if index == 2 && caller == CallerType::Global && Kind::ALL.into_iter().any(|kind| !kind.is_set(&incoming)) {
                window.set_clipboard_error("Hey, you tried to paste a non-full import options container into the \"global\" entry. Did you mean to do a merge-paste instead?".into());
                return;
            }
            window.set_clipboard_error("".into());
            let apply: Rc<dyn Fn(ImportOptionsSlice)> = {
                let active = active.clone(); let state = state.clone(); let weak = weak.clone();
                Rc::new(move |options| {
                    if !active.get() { return; }
                    let mut state = state.borrow_mut();
                    state.replace_options(&options);
                    if let Some(window) = weak.upgrade() { show(&window, &state); show_tag_services(&window, &state); }
                })
            };
            let draft = hydrus_gui_model::import_options_overwrite::Overwrite::new(caller, state.borrow().simple, state.borrow().editor.value(), incoming);
            if index == 3 {
                let slot = state.borrow().overwrite.clone();
                let closed: Rc<dyn Fn()> = { let weak = weak.clone(); Rc::new(move || { if let Some(window) = weak.upgrade() { window.set_overwrite_open(false); } }) };
                match crate::import_options_overwrite_window::open(&store, draft, &slot, apply, closed) {
                    Ok(child) => { *slot.borrow_mut() = Some(child); window.set_overwrite_open(true); }
                    Err(error) => window.set_clipboard_error(error.into()),
                }
            } else {
                let mut draft = draft;
                let preset = match index { 0 => hydrus_gui_model::import_options_overwrite::Preset::Merge, 1 => hydrus_gui_model::import_options_overwrite::Preset::FillIn, 2 => hydrus_gui_model::import_options_overwrite::Preset::Replace, _ => return };
                draft.preset(preset);
                apply(draft.value());
            }
        }
    });
    window.on_apply({
        let weak = window.as_weak();
        let active = active.clone();
        let state = state.clone();
        let close = close.clone();
        move || {
            if !active.get()
                || state.borrow().tag_filter.borrow().is_some()
                || state.borrow().write_tags.borrow().is_some()
                || state.borrow().overwrite.borrow().is_some()
                || state.borrow().locations.borrow().is_some()
                || state
                    .borrow()
                    .favourites
                    .as_ref()
                    .is_some_and(|owner| owner.busy())
            {
                return;
            }
            let name = weak.upgrade().map_or_else(String::new, |window| {
                window.get_favourite_name().to_string()
            });
            let name = if name.is_empty() {
                "favourite".into()
            } else {
                name
            };
            done(name, state.borrow().editor.value());
            close();
        }
    });
    window.on_cancel({
        let close = close.clone();
        move || close()
    });
    window.window().on_close_requested({
        let close = close.clone();
        move || {
            close();
            slint::CloseRequestResponse::HideWindow
        }
    });
    show(&window, &state.borrow());
    show_tag_services(&window, &state.borrow());
    window.show().map_err(|e| e.to_string())?;
    Ok(window)
}
