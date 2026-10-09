//! The shared file domain button under its callers' flags, and the import
//! options editor's two uses of it: the import destinations (only the
//! domains files can be imported to) and the presentation location (no
//! restrictions), against the reference's real button and panels
//! (`oracle/record_location_selector_flags.py`), in normal and advanced mode.

use std::sync::Arc;

use hydrus_core::import_options::{ImportOptionsSlice, PresentationInbox, PresentationStatus};
use hydrus_core::service::builtin_keys;
use hydrus_gui::domains::{Flags, Row, location_menu_for, multiple_ticks_for};
use hydrus_gui::{ImportOptionsWindow, MainWindow, Pages, bind, headless, locations_window};
use hydrus_search::LocationContext;
use hydrus_store::{Store, queues, settings};
use serde_json::{Value as Json, json};
use slint::{ComponentHandle as _, Model as _};

fn recorded() -> Json {
    hydrus_testkit::fixture_json("location_selector_flags.json")
}

fn menu_json(menu: &[Option<Row>]) -> Json {
    json!(
        menu.iter()
            .map(|row| match row {
                None => json!("---"),
                Some(row) => json!({ "text": row.label, "checked": row.checked }),
            })
            .collect::<Vec<_>>()
    )
}

fn set_advanced(store: &Store, on: bool) {
    store
        .write(move |ctx| settings::set(ctx.conn(), &settings::AdvancedMode(on)))
        .unwrap();
}

fn texts(model: &slint::ModelRc<slint::SharedString>) -> Vec<String> {
    model.iter().map(|s| s.to_string()).collect()
}

/// A recorded drop-down: the entries, not the separators.
fn entries(recorded: &Json) -> Vec<String> {
    recorded
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|e| e["text"].as_str().map(str::to_owned))
        .collect()
}

// A button's menu and its "multiple/deleted locations" list for every flag
// combination its callers use, through the function the editor's buttons
// draw their menus with.
#[test]
fn the_buttons_menu_and_list_follow_their_callers_flags_as_the_reference_does() {
    let (_dirs, store) = crate::subscriptions::store();
    let snapshot = store.snapshot();
    let services = &snapshot.services;
    let recorded = recorded();
    let my_files = LocationContext::single(hydrus_core::ServiceKey::new(
        builtin_keys::MY_FILES.to_vec(),
    ));
    for (mode, advanced) in [("normal", false), ("advanced", true)] {
        let base = Flags {
            advanced,
            all_known_files_allowed: true,
            only_importable: false,
            only_local: false,
            only_combined_local: false,
            multiple_allowed: true,
            paired_with_tag_domain: false,
        };
        let presets = [
            (
                "search",
                Flags {
                    all_known_files_allowed: advanced,
                    ..base
                },
            ),
            ("search_not_advanced_all_known", base),
            (
                "no_all_known",
                Flags {
                    all_known_files_allowed: false,
                    ..base
                },
            ),
            (
                "importable",
                Flags {
                    only_importable: true,
                    ..base
                },
            ),
            (
                "only_local",
                Flags {
                    only_local: true,
                    ..base
                },
            ),
            (
                "only_combined_local",
                Flags {
                    only_combined_local: true,
                    ..base
                },
            ),
            (
                "no_multiple",
                Flags {
                    multiple_allowed: false,
                    ..base
                },
            ),
        ];
        for (preset, flags) in presets {
            let theirs = &recorded[mode]["matrix"][preset];
            assert_eq!(
                menu_json(&location_menu_for(services, flags, &my_files)),
                theirs["menu"],
                "{mode} {preset}: menu"
            );
            let ticks: Vec<Json> = multiple_ticks_for(services, flags)
                .iter()
                .map(|t| {
                    json!({
                        "text": t.label,
                        "checked": !t.deleted && my_files.current().contains(&t.service),
                    })
                })
                .collect();
            assert_eq!(json!(ticks), theirs["multiple"], "{mode} {preset}: list");
        }
    }
}

struct Setup {
    ui: MainWindow,
    bound: hydrus_gui::Bound,
    store: Arc<Store>,
    queue: i64,
    _dirs: [tempfile::TempDir; 2],
}

/// An importer page (a downloader's options: its caller shows the URL
/// boxes).
fn setup(advanced: bool) -> Setup {
    let (dirs, store) = crate::subscriptions::store();
    set_advanced(&store, advanced);
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

/// Show the page named `kind`, set to custom.
fn custom_page(editor: &ImportOptionsWindow, kind: &str) {
    let count = editor.get_labels().row_count();
    for i in 0..count {
        editor.invoke_kind_clicked(i32::try_from(i).unwrap());
        if editor.get_kind() == kind {
            editor.set_custom_index(1);
            editor.invoke_changed();
            return;
        }
    }
    panic!("no {kind} page");
}

fn pick(editor: &ImportOptionsWindow, which: &str, label: &str) {
    let choices = texts(&if which == "destination" {
        editor.get_destination_choices()
    } else {
        editor.get_location_choices()
    });
    let at = choices
        .iter()
        .position(|c| c == label)
        .unwrap_or_else(|| panic!("no {label} in {choices:?}"));
    editor.invoke_location_picked(which.into(), i32::try_from(at).unwrap());
}

/// What the destination button shows now.
fn destination_state(editor: &ImportOptionsWindow) -> Json {
    json!({
        "label": editor.get_destination_label().to_string(),
        "warning": editor.get_destination_warning(),
    })
}

fn theirs(chosen: &Json) -> Json {
    json!({ "label": chosen["label"], "warning": chosen["warning"] })
}

// The destinations button of the import options editor, as a user drives
// it on an importer page: the importable domains' menu, each chosen in
// turn, several at once through the shared list (and cancelling it), and
// none, which warns; applied to the importer and read back from it.
// leaf: import-locations
#[test]
fn the_destination_button_chooses_one_several_or_no_domains_as_the_reference_does() {
    let _windows = headless::init();
    let recorded = recorded();
    for (mode, advanced) in [("normal", false), ("advanced", true)] {
        let theirs_destination = &recorded[mode]["destination"];
        let s = setup(advanced);
        let editor = s.editor();
        custom_page(&editor, "locations");
        // a downloader's caller shows the URL boxes
        assert_eq!(
            editor.get_downloader(),
            theirs_destination["callers"]["specific importer"]["primary_urls"]
                .as_bool()
                .unwrap()
        );
        // the menu, and the start
        let menu: Vec<String> = theirs_destination["menu"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|e| e["text"].as_str().map(str::to_owned))
            .collect();
        assert_eq!(texts(&editor.get_destination_choices()), menu, "{mode}");
        assert_eq!(
            destination_state(&editor),
            theirs(&theirs_destination["start"]),
            "{mode}"
        );
        // each domain in turn
        for chosen in theirs_destination["chosen"].as_array().unwrap() {
            pick(&editor, "destination", chosen["chosen"].as_str().unwrap());
            assert_eq!(destination_state(&editor), theirs(chosen), "{mode}");
        }
        // several, through the shared list: it lists the importable domains
        pick(&editor, "destination", "multiple/deleted locations");
        let list = locations_window::last_opened().expect("the list opens");
        let ticked = |list: &hydrus_gui::LocationsWindow| -> Vec<(String, bool)> {
            list.get_ticks()
                .iter()
                .map(|t| (t.label.to_string(), t.checked))
                .collect()
        };
        assert_eq!(
            ticked(&list)
                .iter()
                .map(|(l, _)| l.clone())
                .collect::<Vec<_>>(),
            vec!["art", "my files"],
            "{mode}"
        );
        // the parent can't be edited or applied while it is open
        assert!(editor.get_location_child_open());
        editor.invoke_apply();
        assert!(s.bound.folders.import_options.borrow().is_some());
        // cancelling it changes nothing
        list.invoke_toggled(0, true);
        list.invoke_cancel();
        assert!(!editor.get_location_child_open());
        assert_eq!(
            editor.get_destination_label(),
            "my files",
            "{mode}: cancel keeps the last choice"
        );
        pick(&editor, "destination", "multiple/deleted locations");
        let list = locations_window::last_opened().expect("the list opens again");
        list.invoke_toggled(0, true);
        list.invoke_toggled(1, true);
        list.invoke_apply();
        let both = &theirs_destination["multiple"][0];
        assert_eq!(destination_state(&editor), theirs(both), "{mode}");
        // none: the warning
        pick(&editor, "destination", "multiple/deleted locations");
        let list = locations_window::last_opened().unwrap();
        list.invoke_toggled(0, false);
        list.invoke_toggled(1, false);
        list.invoke_apply();
        let none = &theirs_destination["multiple"][2];
        assert_eq!(destination_state(&editor), theirs(none), "{mode}");
        assert_eq!(none["warning"], json!(true));
        // "even for 'already in db' files?" needs auto-archive
        for case in theirs_destination["interlock"].as_array().unwrap() {
            editor.set_archive(case["ticked"].as_bool().unwrap());
            editor.invoke_changed();
            assert_eq!(
                editor.get_archive_already_enabled(),
                case["even_if_already"].as_bool().unwrap(),
                "{mode}: {case}"
            );
        }
        editor.set_archive(false);
        editor.invoke_changed();
        editor.invoke_apply();
        let saved = s.saved().locations.expect("custom locations");
        assert!(saved.destinations.is_empty(), "{mode}");
        // and reopened, the importer still has none
        let editor = s.editor();
        custom_page(&editor, "locations");
        assert_eq!(destination_state(&editor), theirs(none), "{mode}");
        // two domains, applied
        pick(&editor, "destination", "multiple/deleted locations");
        let list = locations_window::last_opened().unwrap();
        list.invoke_toggled(0, true);
        list.invoke_toggled(1, true);
        list.invoke_apply();
        editor.invoke_apply();
        let saved = s.saved().locations.unwrap();
        let mut names = saved.destinations.clone();
        names.sort();
        let mut expected = vec![
            hex::encode(builtin_keys::MY_FILES),
            hex::encode(
                s.store
                    .snapshot()
                    .services
                    .all()
                    .find(|service| service.name == "art")
                    .unwrap()
                    .key
                    .as_bytes(),
            ),
        ];
        expected.sort();
        assert_eq!(names, expected, "{mode}");
    }
}

// The presentation panel's status and inbox choices, which controls
// "do not show anything" greys, and the values chosen reaching the importer.
#[test]
fn presentation_status_and_inbox_choices_follow_the_references_panel() {
    let _windows = headless::init();
    let theirs = &recorded()["presentation_gates"];
    let s = setup(false);
    let editor = s.editor();
    custom_page(&editor, "presentation");
    assert_eq!(
        texts(&editor.get_status_choices()),
        entries_of(&theirs["status_texts"])
    );
    for (status, entry) in theirs["statuses"].as_array().unwrap().iter().enumerate() {
        let editor = s.editor();
        custom_page(&editor, "presentation");
        editor.set_status_index(i32::try_from(status).unwrap());
        editor.invoke_changed();
        assert_eq!(
            texts(&editor.get_inbox_choices()),
            entries_of(&entry["inbox_texts"]),
            "{entry}"
        );
        assert_eq!(
            editor.get_presentation_gates_enabled(),
            entry["inbox_enabled"].as_bool().unwrap()
        );
        assert_eq!(
            editor.get_presentation_gates_enabled(),
            entry["location_enabled"].as_bool().unwrap()
        );
        for (choice, value) in entry["values"].as_array().unwrap().iter().enumerate() {
            let editor = s.editor();
            custom_page(&editor, "presentation");
            editor.set_status_index(i32::try_from(status).unwrap());
            editor.invoke_changed();
            editor.set_inbox_index(i32::try_from(choice).unwrap());
            editor.invoke_changed();
            editor.invoke_apply();
            let saved = s.saved().presentation.expect("custom presentation");
            assert_eq!(
                saved.status as i64,
                value["status"].as_i64().unwrap(),
                "{value}"
            );
            assert_eq!(
                saved.inbox as i64,
                value["inbox_value"].as_i64().unwrap(),
                "{value}"
            );
        }
    }
    // "or in inbox" goes back to "inbox or archive" when the status moves
    // off "new files"
    let editor = s.editor();
    custom_page(&editor, "presentation");
    editor.set_status_index(1);
    editor.invoke_changed();
    editor.set_inbox_index(2);
    editor.invoke_changed();
    let shown = |editor: &ImportOptionsWindow| {
        let at = usize::try_from(editor.get_inbox_index()).unwrap();
        texts(&editor.get_inbox_choices())[at].clone()
    };
    let moved = &theirs["or_in_inbox_then_all_files"];
    assert_eq!(shown(&editor), moved["before"].as_str().unwrap());
    editor.set_status_index(0);
    editor.invoke_changed();
    assert_eq!(shown(&editor), moved["after"].as_str().unwrap());
}

fn entries_of(list: &Json) -> Vec<String> {
    list.as_array()
        .unwrap()
        .iter()
        .map(|t| t.as_str().unwrap().to_owned())
        .collect()
}

// The non-downloader callers (an import folder's) don't show the URL boxes.
// (The reference's recorded callers: local import, import folder and client
// api hide them; the others show them.)
#[test]
fn an_import_folders_destination_hides_the_url_boxes() {
    let _windows = headless::init();
    let recorded = recorded();
    let (_dirs, store) = crate::subscriptions::store();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    let watched = tempfile::tempdir().unwrap();
    crate::folders::open(&ui, "manage import folders\u{2026}");
    let list = crate::folders::import_list(&bound);
    list.invoke_add();
    let edit = bound
        .folders
        .import_edit
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    edit.set_path(watched.path().to_string_lossy().into_owned().into());
    edit.invoke_edit_import_options();
    let editor = bound
        .folders
        .import_options
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    custom_page(&editor, "locations");
    let callers = &recorded["normal"]["destination"]["callers"];
    assert_eq!(
        editor.get_downloader(),
        callers["import folder"]["primary_urls"].as_bool().unwrap()
    );
    assert_eq!(
        editor.get_downloader(),
        callers["import folder"]["source_urls"].as_bool().unwrap()
    );
}

// The presentation location, a button with no restrictions: its menu
// (including all known files, and in advanced mode everything deleted), the
// list with "deleted from" boxes in advanced mode, and what is chosen
// reaching the importer's options.
#[test]
fn the_presentation_location_is_the_unrestricted_button() {
    let _windows = headless::init();
    let recorded = recorded();
    for (mode, advanced) in [("normal", false), ("advanced", true)] {
        let theirs = &recorded[mode]["presentation"];
        let s = setup(advanced);
        let editor = s.editor();
        custom_page(&editor, "presentation");
        assert_eq!(
            texts(&editor.get_location_choices()),
            entries(&theirs["menu"]),
            "{mode}"
        );
        assert_eq!(
            editor.get_location_label(),
            theirs["label"].as_str().unwrap()
        );
        // the list: the same boxes, "deleted from" ones in advanced mode
        pick(&editor, "presentation", "multiple/deleted locations");
        let list = locations_window::last_opened().expect("the list opens");
        let boxes: Vec<(String, bool)> = list
            .get_ticks()
            .iter()
            .map(|t| (t.label.to_string(), t.checked))
            .collect();
        let want: Vec<(String, bool)> = theirs["multiple"]
            .as_array()
            .unwrap()
            .iter()
            .map(|b| {
                (
                    b["text"].as_str().unwrap().to_owned(),
                    b["checked"].as_bool().unwrap(),
                )
            })
            .collect();
        assert_eq!(boxes, want, "{mode}");
        list.invoke_cancel();
        // all known files skips the location filter, a mix of current and
        // deleted is kept whole
        editor.set_status_index(0);
        pick(&editor, "presentation", "all known files");
        assert_eq!(editor.get_location_label(), "all known files with tags");
        if advanced {
            pick(&editor, "presentation", "multiple/deleted locations");
            let list = locations_window::last_opened().unwrap();
            let at = |label: &str| {
                list.get_ticks()
                    .iter()
                    .position(|t| t.label == label)
                    .unwrap_or_else(|| panic!("no {label} box"))
            };
            // (all known files covers the rest, so it goes first)
            list.invoke_toggled(i32::try_from(at("all known files")).unwrap(), false);
            list.invoke_toggled(i32::try_from(at("my files")).unwrap(), true);
            list.invoke_toggled(i32::try_from(at("deleted from art")).unwrap(), true);
            list.invoke_apply();
            assert_eq!(
                editor.get_location_label(),
                "a mix of current and deleted files of art, my files"
            );
        } else {
            pick(&editor, "presentation", "my files");
        }
        editor.invoke_apply();
        let saved = s.saved().presentation.expect("custom presentation");
        assert_eq!(saved.status, PresentationStatus::AnyGood);
        assert_eq!(saved.inbox, PresentationInbox::Agnostic);
        if advanced {
            assert_eq!(saved.deleted_location.len(), 1, "{mode}");
        }
        assert_eq!(saved.location.len(), 1, "{mode}");
    }
}

/// Set a tag service's additional tags the way a user does: its button
/// opens the tag entry dialog, the tags are entered, and it is accepted.
pub(crate) fn set_additional_tags(owner: &ImportOptionsWindow, service: i32, tags: &[&str]) {
    owner.invoke_edit_additional_tags(service);
    let child = hydrus_gui::write_tag_window::last_opened().expect("the tag entry opens");
    for tag in tags {
        child.invoke_edited((*tag).into());
        child.invoke_entered();
    }
    child.invoke_apply();
}

fn presentation(store: &Store, queue: i64) -> hydrus_core::import_options::PresentationOptions {
    store
        .read(move |c| queues::queue(c, queue))
        .unwrap()
        .unwrap()
        .options
        .presentation
        .expect("custom presentation")
}

/// Open the highlighted search's import options editor, as the page shows it.
fn shown_editor(ui: &MainWindow, bound: &hydrus_gui::Bound) -> ImportOptionsWindow {
    // ("show files" shows the files instead of the search; selecting its row
    // and highlighting it shows the search again)
    if !ui.get_gallery_data().highlighted {
        ui.invoke_gallery_highlight();
    }
    ui.invoke_shown_import_options();
    bound
        .folders
        .import_options
        .borrow()
        .as_ref()
        .expect("the editor opens")
        .clone_strong()
}

/// Press "show files" > "default presented files (...)" on the first row.
fn show_default_files(ui: &MainWindow) {
    ui.invoke_importer_list_menu(0, 20.0, 20.0);
    let pane = |ui: &MainWindow, at: usize| -> Vec<String> {
        let lines = ui.get_menu_panes().row_data(at).unwrap().lines;
        lines.iter().map(|l| l.label.to_string()).collect()
    };
    let choose = |ui: &MainWindow, pane_at: usize, starts: &str| {
        let line = pane(ui, pane_at)
            .iter()
            .position(|l| l.starts_with(starts))
            .unwrap_or_else(|| panic!("{starts} in {:?}", pane(ui, pane_at)));
        let (p, l) = (
            i32::try_from(pane_at).unwrap(),
            i32::try_from(line).unwrap(),
        );
        ui.invoke_menu_line_hovered(p, l, 300.0, 100.0, 10.0);
        ui.invoke_menu_line_clicked(p, l, 300.0, 100.0, 10.0);
    };
    choose(ui, 0, "show files");
    choose(ui, 1, "default presented files");
}

// leaf: audit-network-options-present
// leaf: audit-shared-location-deleted
#[test]
fn the_presentation_editors_choices_filter_the_files_show_files_presents() {
    use hydrus_core::import_options::PresentationStatus;
    use hydrus_store::queues::{FileSeedMeta, NewFileSeed, SeedStatus, SeedType};
    let _windows = headless::init();
    let (_dirs, store) = crate::subscriptions::store();
    set_advanced(&store, true);
    crate::importer_list_menu::with_downloader(&store);
    let snapshot = store.snapshot();
    let find = |name: &str| {
        snapshot
            .services
            .all()
            .find(|s| s.name == name)
            .unwrap_or_else(|| panic!("service {name}"))
            .id
    };
    let (mine, art) = (find("my files"), find("art"));
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    ui.invoke_new_page();
    ui.invoke_chooser_pressed(4);
    ui.invoke_chooser_pressed(6);
    ui.invoke_gallery_queries("blue".into());
    let queue = bound.current.borrow().borrow().gallery().unwrap().queries[0].queue;
    // four imported files: the first two are in my files (the second was
    // already there), the third was deleted from art, the last is nowhere
    let hashes: Vec<hydrus_core::Sha256> = (1..=4)
        .map(|n| hydrus_core::Sha256::from_slice(&[0xB0 + n; 32]).unwrap())
        .collect();
    let ids = {
        let hashes = hashes.clone();
        store
            .write(move |ctx| {
                let conn = ctx.conn();
                let ids: Vec<hydrus_core::HashId> = hashes
                    .iter()
                    .map(|h| hydrus_store::master::intern_hash(conn, h))
                    .collect::<Result<_, _>>()?;
                for id in &ids[..2] {
                    conn.execute(
                        "INSERT INTO file_domain_current (service_id, hash_id, added_ms) VALUES (?, ?, 0)",
                        rusqlite::params![mine, id],
                    )?;
                }
                conn.execute(
                    "INSERT INTO file_domain_deleted (service_id, hash_id, deleted_ms, original_added_ms) VALUES (?, ?, 0, 0)",
                    rusqlite::params![art, ids[2]],
                )?;
                let seeds: Vec<NewFileSeed> = (1..=4)
                    .map(|n| {
                        let mut meta = FileSeedMeta::default();
                        meta.set_hash("sha256", hashes[n - 1].to_hex());
                        let url = format!("https://booru.example/post/{n}");
                        NewFileSeed {
                            seed_type: SeedType::Url,
                            data: url.clone(),
                            data_for_comparison: url,
                            source_time: None,
                            referral_url: None,
                            meta,
                        }
                    })
                    .collect();
                queues::add_file_seeds(conn, queue, &seeds, false, 0)?;
                for (seed, n) in queues::file_seeds(conn, queue)?.iter_mut().zip(1..) {
                    seed.status = if n == 2 {
                        SeedStatus::SuccessfulButRedundant
                    } else {
                        SeedStatus::SuccessfulAndNew
                    };
                    queues::update_file_seed(conn, seed)?;
                }
                Ok(ids)
            })
            .unwrap()
    };
    // (the page sorts what it shows)
    let shown = |bound: &hydrus_gui::Bound| {
        let mut files = bound.current.borrow().borrow().files().clone();
        files.sort();
        files
    };
    let status_at = |editor: &ImportOptionsWindow, text: &str| {
        i32::try_from(
            texts(&editor.get_status_choices())
                .iter()
                .position(|t| t == text)
                .unwrap(),
        )
        .unwrap()
    };

    // new files in my files: the redundant one is not new
    let editor = shown_editor(&ui, &bound);
    custom_page(&editor, "presentation");
    editor.set_status_index(status_at(&editor, "new files"));
    editor.invoke_changed();
    pick(&editor, "presentation", "my files");
    editor.invoke_apply();
    let saved = presentation(&store, queue);
    assert_eq!(saved.status, PresentationStatus::NewOnly);
    show_default_files(&ui);
    assert_eq!(shown(&bound), [ids[0]]);

    // all files in my files: both of its files
    let editor = shown_editor(&ui, &bound);
    custom_page(&editor, "presentation");
    editor.set_status_index(status_at(&editor, "all files"));
    editor.invoke_changed();
    editor.invoke_apply();
    show_default_files(&ui);
    assert_eq!(shown(&bound), [ids[0], ids[1]]);

    // my files with files deleted from art: the deleted one joins them
    let editor = shown_editor(&ui, &bound);
    custom_page(&editor, "presentation");
    pick(&editor, "presentation", "multiple/deleted locations");
    let list = locations_window::last_opened().expect("the list opens");
    let tick = |list: &hydrus_gui::LocationsWindow, label: &str, on: bool| {
        let at = list
            .get_ticks()
            .iter()
            .position(|t| t.label == label)
            .unwrap_or_else(|| panic!("no {label} box"));
        list.invoke_toggled(i32::try_from(at).unwrap(), on);
    };
    tick(&list, "deleted from art", true);
    list.invoke_apply();
    editor.invoke_apply();
    let saved = presentation(&store, queue);
    assert_eq!(saved.deleted_location.len(), 1);
    show_default_files(&ui);
    assert_eq!(shown(&bound), [ids[0], ids[1], ids[2]]);

    // only the deleted domain
    let editor = shown_editor(&ui, &bound);
    custom_page(&editor, "presentation");
    pick(&editor, "presentation", "multiple/deleted locations");
    let list = locations_window::last_opened().expect("the list opens");
    tick(&list, "my files", false);
    list.invoke_apply();
    editor.invoke_apply();
    show_default_files(&ui);
    assert_eq!(shown(&bound), [ids[2]]);

    // all known files does not filter by location
    let editor = shown_editor(&ui, &bound);
    custom_page(&editor, "presentation");
    pick(&editor, "presentation", "all known files");
    editor.invoke_apply();
    show_default_files(&ui);
    assert_eq!(shown(&bound), ids);
}
