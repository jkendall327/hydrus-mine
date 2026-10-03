//! The import options editor's window, bound (`ui/import_options.slint`):
//! hydrus-gui-model's [`Editor`](crate::import_options_editor::Editor)
//! shown as the reference's `EditSpecificImportOptionsContainerPanel`, the
//! list of kinds and the chosen kind's page. Every kind but external
//! programs is edited here (that can be set to custom, from its default,
//! or back). "apply" gives the importer's options to `done`.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use slint::{ComponentHandle as _, ModelRc, SharedString, VecModel};

use hydrus_core::ServiceKey;
use hydrus_core::import_options::{
    CallerType, ImportOptionsManager, ImportOptionsSlice, PrefetchCheck, PresentationInbox,
    PresentationStatus,
};
use hydrus_core::service::{ServiceType, builtin_keys};
use hydrus_store::Store;
use hydrus_store::services::ServiceRegistry;

use crate::import_options_editor::{
    CUSTOM_CHOICE, DESCRIPTION, Editor, Kind, inbox_choices, use_default_label,
};
use crate::{ImportOptionsWindow, ResolutionLimit, SizeLimit, TagServiceRow};

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

/// The locations presentation offers: all my files, or all local files
/// (including the trash).
const PRESENTATION_LOCATIONS: [&[u8]; 2] = [
    builtin_keys::COMBINED_LOCAL_FILE_DOMAINS,
    builtin_keys::HYDRUS_LOCAL_FILE_STORAGE,
];

/// The local file domains files can be imported to: (key hex, name).
fn destinations(services: &ServiceRegistry) -> Vec<(String, String)> {
    services
        .all()
        .filter(|s| s.service_type() == ServiceType::LocalFileDomain)
        .map(|s| (hex::encode(s.key.as_bytes()), s.name.clone()))
        .collect()
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
    editor: Editor,
    services: Arc<hydrus_store::store::Snapshot>,
    /// The allowed filetypes' groups showing their filetypes.
    filetype_expanded: [bool; 7],
}

/// Show the editor whole: the list and the shown kind's page.
fn show(window: &ImportOptionsWindow, state: &State) {
    let editor = &state.editor;
    let services = &state.services.services;
    let name = namer(services);
    window.set_labels(strings(editor.labels(&name)));
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
                crate::import_options_editor::summary(kind, values, &name)
            ),
            _ => String::new(),
        }
        .into(),
    );
    if let Some(o) = &values.presentation {
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
        window.set_location_choices(strings(
            PRESENTATION_LOCATIONS
                .iter()
                .map(|k| name(&hex::encode(k)))
                .collect(),
        ));
        let storage = hex::encode(builtin_keys::HYDRUS_LOCAL_FILE_STORAGE);
        window.set_location_index(i32::from(o.location == [storage]));
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
        set_text(
            || window.get_tag_blacklist(),
            |t| window.set_tag_blacklist(t),
            crate::import_options_editor::blacklist_text(&o.blacklist),
        );
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
        let choices = destinations(services);
        window.set_destination_index(
            o.destinations
                .first()
                .and_then(|d| choices.iter().position(|c| &c.0 == d))
                .and_then(|i| i32::try_from(i).ok())
                .unwrap_or(0),
        );
        window.set_destination_choices(strings(choices.into_iter().map(|c| c.1).collect()));
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
                filter: s.get_tags_filter.to_filter_string().into(),
                additional: s.additional_tags.join("\n").into(),
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
    let services = state.services.clone();
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
                let at = usize::try_from(window.get_location_index()).unwrap_or(0);
                o.location = vec![hex::encode(PRESENTATION_LOCATIONS[at.min(1)])];
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
                o.blacklist = crate::import_options_editor::with_blacklist(
                    &o.blacklist,
                    &window.get_tag_blacklist(),
                );
                o.whitelist = crate::import_options_editor::lines(&window.get_tag_whitelist());
            }
        }
        Kind::Locations => {
            let choices = destinations(&services.services);
            if let Some(o) = &mut editor.values.locations {
                if let Some(d) = usize::try_from(window.get_destination_index())
                    .ok()
                    .and_then(|i| choices.get(i))
                {
                    o.destinations = vec![d.0.clone()];
                }
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
    let manager: ImportOptionsManager = store
        .read(hydrus_store::settings::get)
        .map_err(|e| e.to_string())?;
    // (the reference's "import options simple mode", on as a new client
    // has it; hydrus-rs has no such option yet)
    let editor = Editor::new(&manager, caller, true, own);
    let window = ImportOptionsWindow::new().map_err(|e| e.to_string())?;
    window.set_description(DESCRIPTION.into());
    window.set_downloader(!matches!(
        caller,
        CallerType::LocalImport | CallerType::LocalImportFolder | CallerType::ClientApi
    ));
    let state = Rc::new(RefCell::new(State {
        editor,
        services: store.snapshot(),
        filetype_expanded: [false; 7],
    }));
    let close = {
        let weak = window.as_weak();
        let slot = slot.clone();
        move || {
            if let Some(window) = weak.upgrade() {
                let _ = window.hide();
            }
            slot.borrow_mut().take();
        }
    };
    window.on_kind_clicked({
        let weak = window.as_weak();
        let state = state.clone();
        move |i| {
            let Some(window) = weak.upgrade() else {
                return;
            };
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
            read(&window, &mut state.borrow_mut());
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
    // (typed in: the list's labels shown again, not the services)
    window.on_tag_service_text({
        let weak = window.as_weak();
        let state = state.clone();
        move |i, text| {
            let Some(window) = weak.upgrade() else {
                return;
            };
            {
                let mut state = state.borrow_mut();
                let services = tag_services(&state.services.services);
                let Some((key, _)) = usize::try_from(i).ok().and_then(|i| services.get(i)) else {
                    return;
                };
                if let Some(tags) = &mut state.editor.values.tags {
                    service_options(tags, key).additional_tags =
                        crate::import_options_editor::lines(&text);
                }
            }
            show(&window, &state.borrow());
        }
    });
    window.on_size_edited({
        let weak = window.as_weak();
        let state = state.clone();
        move |i, on, value, unit| {
            let Some(window) = weak.upgrade() else {
                return;
            };
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
    window.on_apply({
        let state = state.clone();
        let close = close.clone();
        move || {
            done(state.borrow().editor.value());
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
