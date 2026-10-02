//! The options window (file > options) in the main window: its pages,
//! edits that wait for "apply" (and are forgotten on "cancel"), and what
//! applying changes: the store's settings, the tab names, and a popup for
//! a value that couldn't be set.

use std::sync::Arc;

use slint::{ComponentHandle as _, Model as _};

use hydrus_gui::{MainWindow, OptionsWindow, Pages, bind, headless};
use hydrus_store::Store;
use hydrus_store::import::import_legacy;

fn store() -> ([tempfile::TempDir; 2], Arc<Store>) {
    let legacy = hydrus_testkit::legacy_fixture("basic");
    let native = tempfile::tempdir().unwrap();
    import_legacy(
        legacy.path(),
        &native.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store = Store::open(native.path()).unwrap();
    ([legacy, native], store)
}

/// file > options…
fn open(ui: &MainWindow) {
    ui.invoke_menu_title_pressed(0, 20.0, 22.0);
    let lines = ui.get_menu_panes().row_data(0).unwrap().lines;
    let at = (0..lines.row_count())
        .position(|i| lines.row_data(i).unwrap().label == "options\u{2026}")
        .expect("file > options");
    ui.invoke_menu_line_clicked(0, at as i32, 0.0, 0.0, 0.0);
}

fn page_names(options: &OptionsWindow) -> Vec<String> {
    let pages = options.get_pages();
    (0..pages.row_count())
        .map(|i| pages.row_data(i).unwrap().text.to_string())
        .collect()
}

fn show_page(options: &OptionsWindow, name: &str) {
    let i = page_names(options).iter().position(|n| n == name).unwrap() as i32;
    options.set_page(i);
    options.invoke_page_chosen(i);
}

fn row(options: &OptionsWindow, label: &str) -> (i32, hydrus_gui::OptionRow) {
    let rows = options.get_rows();
    (0..rows.row_count())
        .map(|i| (i as i32, rows.row_data(i).unwrap()))
        .find(|(_, r)| r.label == label)
        .unwrap_or_else(|| panic!("{label:?}"))
}

fn tabs(ui: &MainWindow) -> Vec<String> {
    let row = ui.get_tab_rows().row_data(0).unwrap();
    (0..row.names.row_count())
        .map(|i| row.names.row_data(i).unwrap().to_string())
        .collect()
}

#[test]
fn the_options_window_applies_its_changes() {
    let (_dirs, store) = store();
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    let options = || bound.options.borrow().as_ref().unwrap().clone_strong();
    let settings = || store.read(hydrus_gui::options::Settings::load).unwrap();
    let before = settings();

    // its pages, as the reference lists them; it opens on the first
    open(&ui);
    let window = options();
    assert_eq!(
        page_names(&window),
        [
            "audio",
            "connection",
            "downloading",
            "duplicates",
            "exporting",
            "file viewing statistics",
            "files and trash",
            "gui",
            "gui pages",
            "importing",
            "maintenance and processing",
            "media playback",
            "media viewer",
            "media viewer hovers",
            "ratings",
            "tag presentation",
            "thumbnails",
            "advanced"
        ]
    );
    assert_eq!(window.get_page(), 0);
    assert_eq!(row(&window, "Label for files with audio: ").1.kind, 6);

    // the search: suggestions as it is typed; the arrows pick one, enter
    // goes to it (its page shown, its row highlighted), and it is cleared
    assert_eq!(
        window.get_search_placeholder(),
        "Search options... (Experimental!)"
    );
    window.set_search_text("TRASH".into());
    window.invoke_search_edited("TRASH".into());
    let matches: Vec<String> = (0..window.get_matches().row_count())
        .map(|i| window.get_matches().row_data(i).unwrap().to_string())
        .collect();
    assert!(
        matches.contains(&"Maximum size of trash (MB):  (files and trash)".to_owned()),
        "{matches:?}"
    );
    let at = matches
        .iter()
        .position(|m| m == "Maximum size of trash (MB):  (files and trash)")
        .unwrap();
    for _ in 0..=at {
        window.invoke_move_match(1);
    }
    assert_eq!(window.get_match_highlighted(), i32::try_from(at).unwrap());
    window.invoke_search_chosen(window.get_match_highlighted());
    assert_eq!(
        page_names(&window)[usize::try_from(window.get_page()).unwrap()],
        "files and trash"
    );
    let (_, found) = row(&window, "Maximum size of trash (MB): ");
    assert!(found.found);
    assert!(
        !row(
            &window,
            "Number of hours a file will stay in the trash before being deleted: "
        )
        .1
        .found
    );
    assert_eq!(window.get_search_text(), "");
    assert_eq!(window.get_matches().row_count(), 0);
    // (nothing typed: nothing suggested)
    window.invoke_search_edited("".into());
    assert_eq!(window.get_matches().row_count(), 0);

    // edits that are cancelled are forgotten
    show_page(&window, "files and trash");
    let (i, trash) = row(&window, "Maximum size of trash (MB): ");
    assert_eq!((trash.kind, trash.number, trash.is_none), (3, 2048, false));
    assert_eq!(trash.none_phrase, "no size limit");
    window.invoke_none_toggled(i, true);
    window.invoke_cancel();
    assert!(bound.options.borrow().is_none(), "closed");
    assert_eq!(settings(), before);

    // and those applied are kept: in the store, and in the tab names
    open(&ui);
    let window = options();
    show_page(&window, "files and trash");
    let (i, _) = row(&window, "Maximum size of trash (MB): ");
    window.invoke_none_toggled(i, true);
    let (i, recycle) = row(
        &window,
        "When physically deleting files or folders, send them to the OS's recycle bin: ",
    );
    window.invoke_check_toggled(i, !recycle.checked);
    show_page(&window, "gui pages");
    let (i, chars) = row(&window, "Max characters to display in a page name: ");
    assert_eq!(chars.kind, 2);
    window.invoke_number_edited(i, 2);
    let names_before = tabs(&ui);
    // (a value that can't be had is left, and said in a popup)
    show_page(&window, "media viewer");
    let (i, _) = row(&window, "Slideshow durations:");
    window.invoke_text_edited(i, "1.0,soon".into());
    window.invoke_apply();
    assert!(bound.options.borrow().is_none(), "closed");
    let after = settings();
    assert_eq!(after.trash.max_size_mb, None);
    assert_eq!(after.folders.delete_to_recycle_bin, !recycle.checked);
    assert_eq!(after.page_names.max_chars, 2);
    assert_eq!(after.slideshow, before.slideshow);
    assert_eq!(after.trash.max_age_hours, before.trash.max_age_hours);
    assert_ne!(tabs(&ui), names_before, "the tab names are shown again");
    let now = hydrus_core::time::TimestampMs::now().millis() / 1000;
    let popups = store
        .read(|conn| hydrus_store::popups::all(conn, now))
        .unwrap();
    assert_eq!(
        popups
            .iter()
            .map(|p| p.status_text_1.clone().unwrap_or_default())
            .collect::<Vec<_>>(),
        ["Could not parse those slideshow durations, so they were not saved!"]
    );

    // a proxy, a rate and a wait, as their controls set them
    open(&ui);
    let window = options();
    show_page(&window, "connection");
    let (i, http) = row(&window, "http: ");
    assert_eq!(
        (http.kind, http.is_none, http.none_phrase.as_str()),
        (7, true, "none")
    );
    window.invoke_text_edited(i, "http://127.0.0.1:8080".into());
    window.invoke_none_toggled(i, false);
    let (i, errors) = row(
        &window,
        "Halt new jobs as long as this many network infrastructure errors on their domain (0 for never wait): ",
    );
    assert_eq!((errors.kind, errors.per.as_str()), (9, "errors within"));
    let fields: Vec<(i32, String)> = (0..errors.fields.row_count())
        .map(|f| {
            let f = errors.fields.row_data(f).unwrap();
            (f.value, f.label.to_string())
        })
        .collect();
    let before_window = settings().network.domain_error_window;
    assert_eq!(fields.len(), 3, "hours, minutes, seconds: {fields:?}");
    window.invoke_number_edited(i, 9);
    window.invoke_field_edited(i, 1, 20);
    show_page(&window, "downloading");
    let (i, wait) = row(&window, "Delay time on a gallery/watcher network error:");
    assert_eq!(wait.kind, 8);
    window.invoke_field_edited(i, 0, 2);
    show_page(&window, "media playback");
    let (i, zoom) = row(&window, "Media Viewer default zoom:");
    assert_eq!(zoom.kind, 5);
    window.invoke_choice_chosen(i, 2);
    show_page(&window, "duplicates");
    let (i, audio) = row(&window, "Score for file with audio:");
    assert_eq!((audio.kind, audio.minimum, audio.maximum), (2, -100, 100));
    window.invoke_number_edited(i, -30);
    window.invoke_apply();
    assert_eq!(
        settings().media_viewer.default_zoom_type,
        hydrus_core::media_viewer::ZoomType::Canvas,
        "canvas fit"
    );
    let scores = settings().duplicate_filter.scores;
    assert_eq!(scores.has_audio, -30);
    assert_eq!(scores.more_tags, before.duplicate_filter.scores.more_tags);
    let network = settings().network;
    assert_eq!(network.http_proxy.as_deref(), Some("http://127.0.0.1:8080"));
    assert_eq!(network.domain_error_number, 9);
    assert_eq!(network.domain_error_window, 20 * 60 + before_window % 60);
    assert_eq!(
        network.downloader_network_error_delay,
        2 * 86400 + before.network.downloader_network_error_delay % 86400
    );
}
