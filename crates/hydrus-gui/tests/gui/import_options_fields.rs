//! The import options editor's own pages (`ClientGUIImportOptionsPanels`),
//! driven as a user would on an importer page and read back from the
//! importer's saved options: file filtering's checkboxes and bounds, note
//! handling, prefetch logic, and a kind set to custom while the others keep
//! their defaults.
use hydrus_core::import_options::{
    ImportOptionsSlice, ImportedAs, NoteConflict, PrefetchCheck, PresentationInbox,
    PresentationStatus,
};
use hydrus_gui::{ImportOptionsWindow, MainWindow, Pages, bind, headless};
use hydrus_store::queues;
use slint::{ComponentHandle as _, Model as _};

const PREFETCH: i32 = 0;
const FILE_FILTERING: i32 = 1;
const TAGS: i32 = 4;
const NOTES: i32 = 5;
const PRESENTATION: i32 = 7;

struct Setup {
    ui: MainWindow,
    bound: hydrus_gui::Bound,
    store: std::sync::Arc<hydrus_store::Store>,
    queue: i64,
    _dirs: [tempfile::TempDir; 2],
}

fn setup() -> Setup {
    let (dirs, store) = crate::subscriptions::store();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    ui.invoke_new_page();
    ui.invoke_chooser_pressed(4);
    ui.invoke_chooser_pressed(8);
    let queue = bound.current.borrow().borrow().importer().unwrap().queue;
    Setup {
        ui,
        bound,
        store,
        queue,
        _dirs: dirs,
    }
}

impl Setup {
    fn editor(&self) -> ImportOptionsWindow {
        self.ui.invoke_importer_import_options();
        self.bound
            .folders
            .import_options
            .borrow()
            .as_ref()
            .unwrap()
            .clone_strong()
    }

    fn saved(&self) -> ImportOptionsSlice {
        let queue = self.queue;
        self.store
            .read(move |conn| queues::queue(conn, queue))
            .unwrap()
            .unwrap()
            .options
    }
}

fn labels(editor: &ImportOptionsWindow) -> Vec<String> {
    editor.get_labels().iter().map(|l| l.to_string()).collect()
}

fn custom(editor: &ImportOptionsWindow, kind: i32) {
    editor.invoke_kind_clicked(kind);
    editor.set_custom_index(1);
    editor.invoke_changed();
}

// leaf: audit-network-options-deleted-bombs
// leaf: audit-network-options-sizes
// leaf: audit-network-options-specific-default
#[test]
fn file_filtering_checkboxes_and_bounds_reach_the_importers_options() {
    let _windows = headless::init();
    let s = setup();
    let editor = s.editor();
    assert_eq!(labels(&editor).len(), 8, "{:?}", labels(&editor));
    custom(&editor, FILE_FILTERING);
    // The reference's defaults: previously deleted excluded, bombs allowed,
    // and the size and resolution limits unset.
    let defaults = s.saved();
    assert!(defaults.file_filtering.is_none());
    assert!(editor.get_exclude_deleted());
    assert!(editor.get_bombs());
    editor.set_exclude_deleted(false);
    editor.set_bombs(false);
    editor.invoke_changed();
    // minimum filesize 3 KB, maximum off, maximum gif 2 MB
    editor.invoke_size_edited(0, true, 3, 1);
    editor.invoke_size_edited(1, false, 1, 2);
    editor.invoke_size_edited(2, true, 2, 2);
    // minimum resolution 640x480, maximum 4000x3000
    editor.invoke_resolution_edited(0, true, 640, 480);
    editor.invoke_resolution_edited(1, true, 4000, 3000);
    let sizes = editor.get_sizes();
    assert_eq!(
        (0..3)
            .map(|i| sizes.row_data(i).unwrap().on)
            .collect::<Vec<_>>(),
        [true, false, true]
    );
    // nothing reaches the importer until Apply
    assert!(s.saved().file_filtering.is_none());
    editor.invoke_apply();
    let filtering = s.saved().file_filtering.expect("custom file filtering");
    assert!(!filtering.exclude_deleted);
    assert!(!filtering.allow_decompression_bombs);
    assert_eq!(filtering.min_size, Some(3 * 1024));
    assert_eq!(filtering.max_size, None);
    assert_eq!(filtering.max_gif_size, Some(2 * 1024 * 1024));
    assert_eq!(filtering.min_resolution, Some((640, 480)));
    assert_eq!(filtering.max_resolution, Some((4000, 3000)));
    // only the kind set to custom overrides; every other kind keeps its default
    let saved = s.saved();
    assert!(saved.prefetch.is_none() && saved.notes.is_none() && saved.tags.is_none());
    assert!(saved.locations.is_none() && saved.presentation.is_none());
    assert!(saved.tag_filtering.is_none() && saved.external_programs.is_none());
    assert_eq!(labels(&s.editor())[0], "default prefetch logic (global)",);

    // Reopened, the page shows what was saved; turning a limit off saves None.
    let editor = s.editor();
    editor.invoke_kind_clicked(FILE_FILTERING);
    assert!(!editor.get_exclude_deleted());
    assert!(!editor.get_bombs());
    let sizes = editor.get_sizes();
    assert_eq!(
        (
            sizes.row_data(0).unwrap().on,
            sizes.row_data(0).unwrap().value
        ),
        (true, 3)
    );
    editor.invoke_size_edited(0, false, 3, 1);
    editor.invoke_resolution_edited(1, false, 4000, 3000);
    editor.invoke_apply();
    let filtering = s.saved().file_filtering.unwrap();
    assert_eq!(filtering.min_size, None);
    assert_eq!(filtering.max_resolution, None);
    assert_eq!(filtering.min_resolution, Some((640, 480)));

    // Cancel writes nothing.
    let editor = s.editor();
    editor.invoke_kind_clicked(FILE_FILTERING);
    editor.set_exclude_deleted(true);
    editor.invoke_changed();
    editor.invoke_cancel();
    assert!(!s.saved().file_filtering.unwrap().exclude_deleted);
}

// leaf: audit-network-options-note-conflict
#[test]
fn note_conflict_extension_whitelist_and_renames_reach_the_importers_options() {
    let _windows = headless::init();
    let s = setup();
    let editor = s.editor();
    custom(&editor, NOTES);
    editor.set_get_notes(true);
    editor.set_extend_notes(true);
    // conflict choices in the reference's order: replace, ignore, append, rename
    editor.set_conflict_index(2);
    editor.set_note_whitelist("source\nartist notes".into());
    editor.set_note_renames("old name -> new name\nsecond -> 2nd".into());
    editor.set_rename_all_on(true);
    editor.set_rename_all("imported".into());
    editor.invoke_changed();
    assert!(s.saved().notes.is_none());
    editor.invoke_apply();
    let notes = s.saved().notes.expect("custom notes");
    assert!(notes.get_notes);
    assert!(notes.extend_existing_note_if_possible);
    assert_eq!(notes.conflict, NoteConflict::Append);
    assert_eq!(notes.name_whitelist, ["source", "artist notes"]);
    assert_eq!(
        notes.name_overrides,
        [
            ("old name".to_owned(), "new name".to_owned()),
            ("second".to_owned(), "2nd".to_owned())
        ]
    );
    assert_eq!(notes.all_name_override.as_deref(), Some("imported"));

    let editor = s.editor();
    editor.invoke_kind_clicked(NOTES);
    assert_eq!(editor.get_conflict_index(), 2);
    assert!(editor.get_rename_all_on());
    assert_eq!(editor.get_rename_all(), "imported");
    editor.set_rename_all_on(false);
    editor.set_conflict_index(3);
    editor.invoke_changed();
    editor.invoke_apply();
    let notes = s.saved().notes.unwrap();
    assert_eq!(notes.conflict, NoteConflict::Rename);
    assert_eq!(notes.all_name_override, None);
}

// leaf: audit-network-options-prefetch
#[test]
fn prefetch_checks_fetch_flags_and_the_dispositive_interlock_reach_the_importers_options() {
    let _windows = headless::init();
    let s = setup();
    let editor = s.editor();
    custom(&editor, PREFETCH);
    // hash: dispositive; then url: dispositive too, so the hash check gives way
    editor.set_hash_index(2);
    editor.invoke_changed();
    assert_eq!(editor.get_hash_index(), 2);
    editor.set_url_index(2);
    editor.invoke_changed();
    assert_eq!(editor.get_url_index(), 2);
    assert_eq!(editor.get_hash_index(), 1);
    editor.set_fetch_hash(true);
    editor.set_fetch_url(true);
    editor.set_neighbour_spam(false);
    editor.invoke_changed();
    editor.invoke_apply();
    let prefetch = s.saved().prefetch.expect("custom prefetch");
    assert_eq!(prefetch.hash_check, PrefetchCheck::Check);
    assert_eq!(
        prefetch.url_check,
        PrefetchCheck::CheckAndMatchesAreDispositive
    );
    assert!(prefetch.fetch_metadata_even_if_hash_recognised_and_file_already_in_db);
    assert!(prefetch.fetch_metadata_even_if_url_recognised_and_file_already_in_db);
    assert!(!prefetch.url_check_looks_for_neighbour_spam);
}

// leaf: audit-network-options-present
#[test]
fn presentation_status_inbox_and_location_gates_reach_the_importers_options() {
    let _windows = headless::init();
    let s = setup();
    // ("hydrus local file storage", which includes the trash, is for
    // advanced mode)
    s.store
        .write(|ctx| {
            hydrus_store::settings::set(ctx.conn(), &hydrus_store::settings::AdvancedMode(true))
        })
        .unwrap();
    let editor = s.editor();
    custom(&editor, PRESENTATION);
    // status: all files, new files, do not show anything
    editor.set_status_index(1);
    editor.invoke_changed();
    // "or in inbox" is only offered with "new files"
    assert_eq!(editor.get_inbox_choices().row_count(), 3);
    editor.set_inbox_index(2);
    editor.invoke_changed();
    let storage = editor
        .get_location_choices()
        .iter()
        .position(|c| c == "hydrus local file storage")
        .unwrap();
    editor.invoke_location_picked("presentation".into(), i32::try_from(storage).unwrap());
    editor.invoke_apply();
    let presentation = s.saved().presentation.expect("custom presentation");
    assert_eq!(presentation.status, PresentationStatus::NewOnly);
    assert_eq!(presentation.inbox, PresentationInbox::AndIncludeAllInbox);
    assert_eq!(
        presentation.location,
        [hex::encode(
            hydrus_core::service::builtin_keys::HYDRUS_LOCAL_FILE_STORAGE
        )]
    );

    // Changing the status away from "new files" drops "or in inbox".
    let editor = s.editor();
    editor.invoke_kind_clicked(PRESENTATION);
    editor.set_status_index(0);
    editor.invoke_changed();
    assert_eq!(editor.get_inbox_choices().row_count(), 2);
    editor.set_inbox_index(1);
    editor.invoke_changed();
    editor.invoke_apply();
    let presentation = s.saved().presentation.unwrap();
    assert_eq!(presentation.status, PresentationStatus::AnyGood);
    assert_eq!(presentation.inbox, PresentationInbox::RequireInbox);
}

// leaf: audit-network-options-tag-fetch
#[test]
fn per_service_get_tags_and_new_or_redundant_gates_reach_the_importers_options() {
    let _windows = headless::init();
    let s = setup();
    let editor = s.editor();
    custom(&editor, TAGS);
    let rows = editor.get_tag_services();
    let mine = i32::try_from(
        rows.iter()
            .position(|service| service.name == "my tags")
            .unwrap(),
    )
    .unwrap();
    // my tags starts as a service that gets nothing
    assert!(!rows.row_data(mine as usize).unwrap().get_tags);
    editor.invoke_tag_service_toggled(mine, "get-tags".into(), true);
    editor.invoke_tag_service_toggled(mine, "to-new".into(), true);
    editor.invoke_tag_service_toggled(mine, "to-inbox".into(), false);
    editor.invoke_tag_service_toggled(mine, "to-archive".into(), false);
    editor.invoke_tag_service_toggled(mine, "get-overwrite".into(), true);
    assert!(!editor.get_gets_no_tags());
    editor.invoke_apply();
    let tags = s.saved().tags.expect("custom tags");
    let key = hex::encode(hydrus_core::service::builtin_keys::MY_TAGS);
    let mine = tags.service(&key).unwrap();
    assert!(mine.get_tags && mine.get_tags_overwrite_deleted);
    assert!(mine.applies_to(ImportedAs::New));
    assert!(!mine.applies_to(ImportedAs::AlreadyInInbox));
    assert!(!mine.applies_to(ImportedAs::AlreadyInArchive));
    assert!(tags.worth_fetching_tags());
    // other services keep their own rules: downloader tags still gets all
    let downloader = hex::encode(hydrus_core::service::builtin_keys::DOWNLOADER_TAGS);
    let downloader = tags.service(&downloader).unwrap();
    assert!(downloader.get_tags && downloader.applies_to(ImportedAs::AlreadyInArchive));
}
