//! The import options editor, opened from an import folder's dialog: the
//! kinds it lists (an import folder's, in simple mode), a kind set to
//! custom options and edited, and the folder's options written with the
//! list. Its lists and summaries are tested against the reference's in
//! hydrus-gui-model's tests.

use slint::{ComponentHandle as _, Model as _};

use hydrus_core::import_options::PresentationStatus;

#[test]
fn shared_overwrite_drafts_close_cleanly_and_clipboard_reaches_real_importer() {
    use hydrus_core::import_options::CallerType;
    use hydrus_downloader_exchange::import_options;
    use hydrus_gui_model::import_options_overwrite::Overwrite;
    use std::cell::RefCell;
    use std::rc::Rc;
    let (_dirs, store) = store();
    let fixture = hydrus_testkit::fixture_json("subscription_import_options.json");
    let existing = import_options::decode_text(&fixture["existing"].to_string()).unwrap();
    let incoming = import_options::decode_text(&fixture["incoming"].to_string()).unwrap();
    let windows = headless::init();
    let slot = Rc::new(RefCell::new(None));
    let result = Rc::new(RefCell::new(None));
    let accepted = {
        let result = result.clone();
        Rc::new(move |value| *result.borrow_mut() = Some(value))
    };
    let chooser = hydrus_gui::import_options_overwrite_window::open(
        &store,
        Overwrite::new(
            CallerType::SpecificImporter,
            true,
            existing.clone(),
            incoming.clone(),
        ),
        &slot,
        accepted,
        Rc::new(|| {}),
    )
    .unwrap();
    *slot.borrow_mut() = Some(chooser.clone_strong());
    chooser.invoke_preset(1);
    let pixels = headless::render(&windows.get(0).unwrap(), 1180, 430);
    headless::save_png(
        &std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("import_options_overwrite.png"),
        &pixels,
        1180,
        430,
    )
    .unwrap();
    chooser.invoke_apply();
    assert!(slot.borrow().is_none());
    assert_eq!(
        import_options::tuple(result.borrow().as_ref().unwrap()).unwrap(),
        fixture["overwrite"][2]["options"]
    );
    result.borrow_mut().take();
    chooser.invoke_apply();
    assert!(result.borrow().is_none());

    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    ui.invoke_new_page();
    ui.invoke_chooser_pressed(4);
    ui.invoke_chooser_pressed(8);
    let queue = bound.current.borrow().borrow().importer().unwrap().queue;
    ui.invoke_importer_import_options();
    let editor = bound
        .folders
        .import_options
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    let text = import_options::encode_text(&incoming).unwrap();
    hydrus_gui::set_paster(move || text.clone());
    editor.invoke_paste_options(2);
    assert!(
        store
            .read(move |conn| hydrus_store::queues::queue(conn, queue))
            .unwrap()
            .unwrap()
            .options
            .is_empty()
    );
    editor.invoke_apply();
    assert_eq!(
        store
            .read(move |conn| hydrus_store::queues::queue(conn, queue))
            .unwrap()
            .unwrap()
            .options,
        incoming
    );
    editor.invoke_apply();
    ui.invoke_importer_import_options();
    let editor = bound
        .folders
        .import_options
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    hydrus_gui::set_paster(|| "[26, 3, []]".into());
    editor.invoke_paste_options(0);
    assert!(
        editor
            .get_clipboard_error()
            .contains("Import Options Container")
    );
    editor.invoke_cancel();
    assert_eq!(
        store
            .read(move |conn| hydrus_store::queues::queue(conn, queue))
            .unwrap()
            .unwrap()
            .options,
        incoming
    );
}
use hydrus_gui::{ImportOptionsWindow, MainWindow, Pages, bind, headless};
use hydrus_store::{import_folders, queues};

use crate::folders::{import_list, open};
use crate::subscriptions::store;

fn labels(editor: &ImportOptionsWindow) -> Vec<String> {
    let labels = editor.get_labels();
    (0..labels.row_count())
        .map(|i| labels.row_data(i).unwrap().to_string())
        .collect()
}

#[test]
fn an_import_folders_import_options_are_edited_and_written() {
    let (_dirs, store) = store();
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    let watched = tempfile::tempdir().unwrap();

    open(&ui, "manage import folders\u{2026}");
    let list = import_list(&bound);
    list.invoke_add();
    let edit = bound
        .folders
        .import_edit
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    edit.set_path(watched.path().to_string_lossy().into_owned().into());
    assert_eq!(edit.get_import_options(), "import options (all default)");
    edit.invoke_edit_import_options();
    let editor = bound
        .folders
        .import_options
        .borrow()
        .as_ref()
        .expect("it opens")
        .clone_strong();
    assert_eq!(
        labels(&editor),
        [
            "default file filtering (global)",
            "default locations (global)",
            "default external programs (global)",
            "default presentation (import folder)"
        ]
    );

    // presentation: custom (from the import folder's default, new files),
    // then all files
    editor.invoke_kind_clicked(3);
    assert_eq!(editor.get_kind(), "presentation");
    editor.set_custom_index(1);
    editor.invoke_changed();
    assert_eq!(labels(&editor)[3], "> presentation: presenting new files");
    editor.set_status_index(0);
    editor.invoke_changed();
    assert_eq!(labels(&editor)[3], "> presentation: presenting all files");

    // file filtering: custom, with a maximum size
    editor.invoke_kind_clicked(0);
    editor.set_custom_index(1);
    editor.invoke_changed();
    editor.invoke_size_edited(1, true, 10, 2);
    assert_eq!(
        labels(&editor)[0],
        "> file filtering: allows all filetypes, excludes previously deleted, excludes > 10 MB"
    );
    // the allowed filetypes' tree: every group ticked; video unticked,
    // and the image group opened
    let tree = |editor: &ImportOptionsWindow| -> Vec<(String, bool, bool)> {
        let rows = editor.get_filetype_rows();
        (0..rows.row_count())
            .map(|i| rows.row_data(i).unwrap())
            .filter(|r| r.shown)
            .map(|r| (r.text.to_string(), r.ticked, r.option < 0))
            .collect()
    };
    assert_eq!(tree(&editor).len(), 7);
    assert!(
        tree(&editor)
            .iter()
            .all(|(_, ticked, group)| *ticked && *group)
    );
    editor.invoke_filetype_ticked(2, -1, false);
    editor.invoke_filetype_expanded(0, true);
    let shown = tree(&editor);
    let row = |name: &str| shown.iter().find(|r| r.0 == name).unwrap().clone();
    assert_eq!(row("video"), ("video".to_owned(), false, true));
    assert_eq!(row("animation"), ("animation".to_owned(), true, true));
    // (the image group's filetypes shown, after it)
    assert_eq!(shown[1], ("jpeg".to_owned(), true, false));
    assert_eq!(shown.iter().filter(|(_, _, g)| *g).count(), 7);
    assert!(!labels(&editor)[0].contains("allows all filetypes"));
    let pixels = headless::render(&windows.get(3).unwrap(), 900, 620);
    let shots = std::path::Path::new(env!("CARGO_TARGET_TMPDIR"));
    headless::save_png(&shots.join("import_options.png"), &pixels, 900, 620).unwrap();

    editor.invoke_apply();
    assert!(bound.folders.import_options.borrow().is_none());
    assert_eq!(
        edit.get_import_options(),
        "import options (file filtering, presentation)"
    );
    edit.invoke_apply();
    list.invoke_apply();
    let folder = store
        .read(import_folders::import_folders)
        .unwrap()
        .remove(0);
    let options = store
        .read(move |c| queues::queue(c, folder.id()))
        .unwrap()
        .unwrap()
        .options;
    assert_eq!(
        options.presentation.unwrap().status,
        PresentationStatus::AnyGood
    );
    let filtering = options.file_filtering.unwrap();
    assert_eq!(filtering.max_size, Some(10 * 1024 * 1024));
    // (as specific filetypes, video's not among them)
    assert!(
        filtering
            .filetypes
            .contains(&hydrus_core::mime::Mime::ImageJpeg.code())
    );
    assert!(
        !filtering
            .filetypes
            .contains(&hydrus_core::mime::Mime::VideoMp4.code())
    );
}

#[test]
fn a_gallery_pages_import_options_are_edited_for_its_new_searches() {
    let (_dirs, store) = store();
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store).unwrap());
    // download, then gallery
    ui.invoke_new_page();
    ui.invoke_chooser_pressed(4);
    ui.invoke_chooser_pressed(6);
    assert_eq!(
        ui.get_gallery_data().import_options,
        "import options (all default)"
    );
    ui.invoke_page_import_options();
    let editor = bound
        .folders
        .import_options
        .borrow()
        .as_ref()
        .expect("it opens")
        .clone_strong();
    // (every kind, a downloader's; tags default to gallery/post urls')
    let listed = labels(&editor);
    assert_eq!(listed.len(), 8);
    assert_eq!(listed[4], "default tags (gallery/post urls)");
    editor.invoke_kind_clicked(7);
    editor.set_custom_index(1);
    editor.invoke_changed();

    // tags: additional tags for "my tags"
    editor.invoke_kind_clicked(4);
    editor.set_custom_index(1);
    editor.invoke_changed();
    let services = editor.get_tag_services();
    let mine = (0..services.row_count())
        .position(|i| services.row_data(i).unwrap().name == "my tags")
        .unwrap();
    editor.invoke_tag_service_text(i32::try_from(mine).unwrap(), "zebra\napple".into());
    assert!(
        labels(&editor)[4].contains("my tags[adding \"apple, zebra\"]"),
        "{:?}",
        labels(&editor)
    );
    let pixels = headless::render(&windows.get(1).unwrap(), 900, 620);
    let shots = std::path::Path::new(env!("CARGO_TARGET_TMPDIR"));
    headless::save_png(&shots.join("import_options_tags.png"), &pixels, 900, 620).unwrap();
    // notes: none
    editor.invoke_kind_clicked(5);
    editor.set_custom_index(1);
    editor.invoke_changed();
    editor.set_get_notes(false);
    editor.invoke_changed();
    assert_eq!(labels(&editor)[5], "> notes: not adding notes");
    // tag filtering: a blacklist
    editor.invoke_kind_clicked(2);
    editor.set_custom_index(1);
    editor.invoke_changed();
    // (in the tag filter editor, a blacklist alone)
    assert_eq!(editor.get_tag_blacklist(), "no blacklist set");
    editor.invoke_edit_blacklist();
    let filter = hydrus_gui::tag_filter_window::last_opened().expect("the editor opens");
    assert_eq!(filter.get_window_title(), "edit blacklist");
    assert_eq!(filter.get_tabs().row_count(), 1);
    assert!(
        filter
            .get_message()
            .starts_with("If a file about to be downloaded")
    );
    filter.invoke_typed(1, "Goblin".into());
    filter.invoke_typed(1, "orc".into());
    assert_eq!(filter.get_black_rows().row_count(), 2);
    assert_eq!(filter.get_current(), "blacklisting on goblin, orc");
    // (unnamespaced rules catch namespaced tags, in a blacklist)
    filter.set_test_input("character:goblin\nelf".into());
    filter.invoke_test_edited();
    assert_eq!(filter.get_test_result(), "1 pass, 1 blocked!");
    let last = (0..100)
        .take_while(|&n| windows.get(n).is_some())
        .last()
        .unwrap();
    let pixels = headless::render(&windows.get(last).unwrap(), 760, 720);
    headless::save_png(&shots.join("tag_filter.png"), &pixels, 760, 720).unwrap();
    filter.invoke_apply();
    assert!(hydrus_gui::tag_filter_window::last_opened().is_none());
    assert_eq!(editor.get_tag_blacklist(), "blacklisting on goblin, orc");
    assert_eq!(
        labels(&editor)[2],
        "> tag filtering: blacklisting on goblin, orc"
    );
    // a service's "get tags" filter: no namespaced tags
    editor.invoke_kind_clicked(4);
    editor.invoke_edit_get_tags_filter(i32::try_from(mine).unwrap());
    let filter = hydrus_gui::tag_filter_window::last_opened().expect("the editor opens");
    assert_eq!(filter.get_window_title(), "edit tag filter");
    assert_eq!(filter.get_tabs().row_count(), 3);
    assert_eq!(filter.get_tab(), 0);
    filter.invoke_global_clicked(0, 1);
    assert_eq!(
        filter.get_current(),
        "current filter: allowing all unnamespaced tags"
    );
    // a namespace typed in, then removed, asking first
    filter.invoke_typed(0, "creator:*".into());
    assert_eq!(
        filter.get_current(),
        "current filter: allowing all unnamespaced tags and 'creator' tags"
    );
    let rows = filter.get_white_rows();
    let creator = (0..rows.row_count())
        .position(|r| rows.row_data(r).unwrap().cells.row_data(0).unwrap() == "'creator' tags")
        .unwrap();
    filter.invoke_row_clicked(0, i32::try_from(creator).unwrap(), false, false);
    filter.invoke_remove(0);
    assert!(filter.get_asking());
    assert_eq!(filter.get_asking_message(), "Remove all selected?");
    filter.invoke_chosen(0);
    assert!(!filter.get_asking());
    assert_eq!(
        filter.get_current(),
        "current filter: allowing all unnamespaced tags"
    );
    filter.invoke_help();
    assert!(
        filter
            .get_asking_message()
            .starts_with("Here you can set rules")
    );
    filter.invoke_chosen(0);
    filter.invoke_apply();
    assert_eq!(
        editor.get_tag_services().row_data(mine).unwrap().filter,
        "adding: all unnamespaced tags"
    );

    editor.invoke_apply();
    assert_eq!(
        ui.get_gallery_data().import_options,
        "import options (tag filtering, tags, notes, pre\u{2026}"
    );
    let page = bound.current.borrow();
    let page = page.borrow();
    assert!(page.gallery().unwrap().state.options.presentation.is_some());
}

#[test]
fn a_highlighted_searchs_own_file_limit_and_import_options_are_edited() {
    let (_dirs, store) = store();
    crate::importer_list_menu::with_downloader(&store);
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    ui.invoke_new_page();
    ui.invoke_chooser_pressed(4);
    ui.invoke_chooser_pressed(6);
    ui.invoke_gallery_queries("blue".into());
    let queue = {
        let page = bound.current.borrow();
        let page = page.borrow();
        page.gallery().unwrap().queries[0].queue
    };
    // (a new query is shown, as a new client's options have it)
    assert!(ui.get_gallery_data().highlighted);
    assert_eq!(
        ui.get_gallery_data().shown_import_options,
        "import options (all default)"
    );

    // its file limit
    ui.invoke_gallery_shown_limit(false, 50);
    let extra = store
        .read(move |c| queues::queue(c, queue))
        .unwrap()
        .unwrap()
        .extra;
    assert_eq!(extra["file_limit"], 50);
    assert!(!ui.get_gallery_data().shown_no_limit);
    assert_eq!(ui.get_gallery_data().shown_file_limit, 50);

    // its import options
    ui.invoke_shown_import_options();
    let editor = bound
        .folders
        .import_options
        .borrow()
        .as_ref()
        .expect("it opens")
        .clone_strong();
    editor.invoke_kind_clicked(7);
    editor.set_custom_index(1);
    editor.invoke_changed();
    editor.invoke_apply();
    let options = store
        .read(move |c| queues::queue(c, queue))
        .unwrap()
        .unwrap()
        .options;
    assert!(options.presentation.is_some());
    assert_eq!(
        ui.get_gallery_data().shown_import_options,
        "import options (presentation set)"
    );
}

#[test]
fn a_url_downloaders_import_options_are_edited() {
    let (_dirs, store) = store();
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    // download, then urls
    ui.invoke_new_page();
    ui.invoke_chooser_pressed(4);
    ui.invoke_chooser_pressed(8);
    let queue = bound
        .current
        .borrow()
        .borrow()
        .importer()
        .map(|i| i.queue)
        .unwrap();
    assert_eq!(
        ui.get_import_options_label(),
        "import options (all default)"
    );
    ui.invoke_importer_import_options();
    let editor = bound
        .folders
        .import_options
        .borrow()
        .as_ref()
        .expect("it opens")
        .clone_strong();
    assert_eq!(labels(&editor).len(), 8);
    editor.invoke_kind_clicked(7);
    editor.set_custom_index(1);
    editor.invoke_changed();
    editor.invoke_apply();
    assert_eq!(
        ui.get_import_options_label(),
        "import options (presentation set)"
    );
    let options = store
        .read(move |c| queues::queue(c, queue))
        .unwrap()
        .unwrap()
        .options;
    assert!(options.presentation.is_some());
}

#[test]
fn favourites_popup_replays_real_dialogs_persists_and_invalidates_children() {
    use hydrus_core::import_options::{CallerType, ImportOptionsManager, ImportOptionsSlice};
    use hydrus_downloader_exchange::import_options;
    use hydrus_gui::import_options_favourites_window::Controller;
    use hydrus_store::settings;
    use serde_json::json;
    use std::cell::RefCell;
    use std::rc::Rc;
    let (_dirs, store) = store();
    let windows = headless::init();
    let fixture = hydrus_testkit::fixture_json("subscription_import_options.json");
    let reference = &fixture["ui_favourites"];
    let existing = import_options::decode_text(&fixture["existing"].to_string()).unwrap();
    let incoming = import_options::decode_text(&fixture["incoming"].to_string()).unwrap();
    let current = Rc::new(RefCell::new(existing.clone()));
    let mut manager: ImportOptionsManager = store.read(settings::get).unwrap();
    let defaults = manager.caller_defaults.clone();
    manager.favourites = reference["initial"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| {
            (
                row["name"].as_str().unwrap().to_owned(),
                import_options::decode_text(&row["options"].to_string()).unwrap(),
            )
        })
        .collect();
    store
        .write(move |tx| settings::set(tx.conn(), &manager))
        .unwrap();
    let errors = Rc::new(RefCell::new(Vec::new()));
    let owner = Controller::new(
        store.clone(),
        CallerType::SpecificImporter,
        Rc::new({
            let current = current.clone();
            move || Some(current.borrow().clone())
        }),
        Rc::new({
            let current = current.clone();
            move |options| *current.borrow_mut() = options
        }),
        Rc::new({
            let errors = errors.clone();
            move |error| errors.borrow_mut().push(error)
        }),
        Rc::new(|_| {}),
    );
    let menu = owner.rows().unwrap();
    assert_eq!(
        json!(
            menu.iter()
                .map(|row| row.label.to_string())
                .collect::<Vec<_>>()
        ),
        json!(
            reference["menus"][0][1]["children"]
                .as_array()
                .unwrap()
                .iter()
                .map(|row| row["label"].as_str().unwrap())
                .collect::<Vec<_>>()
        )
    );
    assert_eq!(
        json!(
            menu.iter()
                .map(|row| row.edit_label.to_string())
                .collect::<Vec<_>>()
        ),
        json!(
            reference["menus"][0][4]["children"].as_array().unwrap()[..menu.len()]
                .iter()
                .map(|row| row["label"].as_str().unwrap())
                .collect::<Vec<_>>()
        )
    );
    let clipboard = import_options::encode_text(&incoming).unwrap();
    hydrus_gui::set_clipboard_reader(move || Ok(Some(clipboard.clone())));
    for step in reference["steps"].as_array().unwrap() {
        match step["action"].as_str().unwrap() {
            "save current" => {
                owner.choose(5, "");
                let prompt = owner.prompt_window().unwrap();
                assert_eq!(
                    prompt.get_message().as_str(),
                    reference["save_question"]["message"].as_str().unwrap()
                );
                assert_eq!(
                    prompt.get_name().as_str(),
                    reference["save_question"]["default"].as_str().unwrap()
                );
                prompt.invoke_accepted("profile 2".into());
                prompt.invoke_accepted("stale".into());
            }
            action @ ("add cancel" | "add accept" | "edit accept") => {
                owner.choose(if action == "edit accept" { 3 } else { 4 }, "profile 10");
                let editor = owner.editing_window().unwrap();
                assert!(editor.get_favourite_editor());
                assert_eq!(editor.get_window_title(), "edit favourite import options");
                editor.set_favourite_name("profile 2".into());
                editor.invoke_paste_options(2);
                if action == "add cancel" {
                    editor.invoke_cancel();
                } else {
                    editor.invoke_apply();
                }
                editor.invoke_apply();
            }
            "delete" => {
                owner.choose(6, "profile 2");
                let prompt = owner.prompt_window().unwrap();
                assert_eq!(
                    prompt.get_message().as_str(),
                    reference["delete_question"].as_str().unwrap()
                );
                assert!(!prompt.get_asking_name());
                if step["accepted"].as_bool().unwrap() {
                    prompt.invoke_accepted("".into());
                } else {
                    prompt.invoke_cancelled();
                }
            }
            action => panic!("unexpected fixture action {action}"),
        }
        assert!(!owner.busy());
        let manager: ImportOptionsManager = store.read(settings::get).unwrap();
        assert_eq!(manager.caller_defaults, defaults);
        let mut entries = manager.favourites;
        entries.sort_by(|a, b| a.0.cmp(&b.0));
        assert_eq!(json!(entries.into_iter().map(|(name, options)| json!({"name":name,"options":import_options::tuple(&options).unwrap()})).collect::<Vec<_>>()), step["rows"]);
    }
    owner.choose(0, "profile 2 (2)");
    assert_eq!(*current.borrow(), incoming);
    let before = current.borrow().clone();
    owner.choose(1, "empty");
    let chooser = owner.overwrite_window().unwrap();
    chooser.invoke_cancel();
    assert_eq!(*current.borrow(), before);
    owner.choose(1, "empty");
    let stale_chooser = owner.overwrite_window().unwrap();
    owner.close();
    stale_chooser.invoke_apply();
    owner.choose(0, "empty");
    assert_eq!(*current.borrow(), before);
    let reopened: ImportOptionsManager = store.read(settings::get).unwrap();
    assert!(
        reopened
            .favourites
            .iter()
            .any(|(name, options)| name == "profile 2 (2)" && options == &incoming)
    );
    assert!(errors.borrow().is_empty(), "{:?}", errors.borrow());
    assert!(windows.get(0).is_some());
    assert_ne!(*current.borrow(), ImportOptionsSlice::default());
}
