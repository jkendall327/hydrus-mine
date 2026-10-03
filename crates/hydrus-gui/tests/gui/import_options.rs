//! The import options editor, opened from an import folder's dialog: the
//! kinds it lists (an import folder's, in simple mode), a kind set to
//! custom options and edited, and the folder's options written with the
//! list. Its lists and summaries are tested against the reference's in
//! hydrus-gui-model's tests.

use slint::{ComponentHandle as _, Model as _};

use hydrus_core::import_options::PresentationStatus;
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
    editor.set_tag_blacklist("goblin\norc".into());
    editor.invoke_changed();
    assert_eq!(
        labels(&editor)[2],
        "> tag filtering: blacklisting on goblin, orc"
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
