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
fn most_used_child_stages_each_service_cancels_descendants_and_persists_options() {
    let (_dirs, store) = store();
    let windows = headless::init();
    let f = hydrus_testkit::fixture_json("tag_suggestions.json");
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    open(&ui);
    let options = bound.options.borrow().as_ref().unwrap().clone_strong();
    show_page(&options, "tag suggestions");
    options.invoke_number_edited(row(&options, "Width of suggested tags columns: ").0, 240);
    options.invoke_choice_chosen(row(&options, "Column layout: ").0, 1);
    options.invoke_choice_chosen(row(&options, "Default notebook page: ").0, 3);
    options.invoke_most_used_tags_clicked();
    let slots = &bound.options_suggested_tags_slot;
    let edit = slots.editor.borrow().as_ref().unwrap().clone_strong();
    let image_index = (0..100)
        .take_while(|&n| windows.get(n).is_some())
        .last()
        .unwrap();
    let mine = edit
        .get_services()
        .iter()
        .position(|s| s == "my tags")
        .unwrap();
    edit.invoke_service_chosen(i32::try_from(mine).unwrap());
    edit.invoke_edit_tags();
    let child = slots.tags.borrow().as_ref().unwrap().clone_strong();
    for tag in f["edited"]["tags"].as_array().unwrap() {
        child.invoke_edited(tag.as_str().unwrap().into());
        child.invoke_entered();
    }
    options.invoke_apply();
    assert!(bound.options.borrow().is_some());
    child.invoke_apply();
    assert_eq!(
        serde_json::json!(
            edit.get_tags()
                .iter()
                .map(|tag| tag.to_string())
                .collect::<Vec<_>>()
        ),
        f["edited"]["tags"]
    );
    let pixels = headless::render(&windows.get(image_index).unwrap(), 520, 440);
    headless::save_png(
        &std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("most-used-tags-options.png"),
        &pixels,
        520,
        440,
    )
    .unwrap();
    edit.invoke_edit_tags();
    let retired = slots.tags.borrow().as_ref().unwrap().clone_strong();
    retired.invoke_edited("parity:cancelled".into());
    retired.invoke_entered();
    edit.invoke_cancel();
    retired.invoke_apply();
    assert!(slots.editor.borrow().is_none() && slots.tags.borrow().is_none());
    assert!(
        !store
            .read(hydrus_store::settings::get::<hydrus_store::settings::TagAutocompleteTabs>)
            .unwrap()
            .most_used
            .values()
            .flatten()
            .any(|tag| tag == "parity:cancelled")
    );
    options.invoke_most_used_tags_clicked();
    let edit = slots.editor.borrow().as_ref().unwrap().clone_strong();
    edit.invoke_service_chosen(i32::try_from(mine).unwrap());
    edit.invoke_edit_tags();
    let child = slots.tags.borrow().as_ref().unwrap().clone_strong();
    for tag in f["edited"]["tags"].as_array().unwrap() {
        child.invoke_edited(tag.as_str().unwrap().into());
        child.invoke_entered();
    }
    child.invoke_apply();
    let second = edit
        .get_services()
        .iter()
        .position(|s| s == "second tags")
        .unwrap();
    edit.invoke_service_chosen(i32::try_from(second).unwrap());
    edit.invoke_edit_tags();
    let child = slots.tags.borrow().as_ref().unwrap().clone_strong();
    child.invoke_edited("parity:second".into());
    child.invoke_entered();
    child.invoke_apply();
    edit.invoke_service_chosen(i32::try_from(mine).unwrap());
    assert_eq!(
        serde_json::json!(
            edit.get_tags()
                .iter()
                .map(|tag| tag.to_string())
                .collect::<Vec<_>>()
        ),
        f["retained"]
    );
    edit.invoke_apply();
    let key = store
        .snapshot()
        .services
        .by_name("my tags")
        .unwrap()
        .key
        .to_hex();
    assert_eq!(
        store
            .read(hydrus_store::settings::get::<hydrus_store::settings::TagAutocompleteTabs>)
            .unwrap()
            .most_used
            .get(&key),
        None
    );
    options.invoke_apply();
    let saved = Store::open(store.dir()).unwrap();
    assert_eq!(
        serde_json::json!(
            saved
                .read(hydrus_store::settings::get::<hydrus_store::settings::TagAutocompleteTabs>)
                .unwrap()
                .most_used[&key]
        ),
        f["edited"]["tags"]
    );
    let prefs = saved
        .read(hydrus_store::settings::get::<hydrus_store::settings::TagSuggestionSettings>)
        .unwrap();
    assert_eq!(prefs.width, 240);
    assert!(prefs.columns);
    assert_eq!(prefs.default_page, "recent");
    open(&ui);
    let options = bound.options.borrow().as_ref().unwrap().clone_strong();
    show_page(&options, "tag suggestions");
    assert_eq!(
        row(&options, "Width of suggested tags columns: ").1.number,
        240
    );
    options.invoke_most_used_tags_clicked();
    let edit = slots.editor.borrow().as_ref().unwrap().clone_strong();
    edit.invoke_service_chosen(i32::try_from(mine).unwrap());
    edit.invoke_edit_tags();
    let stale = slots.tags.borrow().as_ref().unwrap().clone_strong();
    options.invoke_cancel();
    stale.invoke_edited("parity:stale".into());
    stale.invoke_entered();
    stale.invoke_apply();
    edit.invoke_apply();
    options.invoke_apply();
    assert!(slots.editor.borrow().is_none() && slots.tags.borrow().is_none());
    assert!(
        !saved
            .read(hydrus_store::settings::get::<hydrus_store::settings::TagAutocompleteTabs>)
            .unwrap()
            .most_used[&key]
            .contains(&"parity:stale".to_owned())
    );
    ui.hide().unwrap();
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

    // its pages, as the reference lists them; it opens on gui
    open(&ui);
    let window = options();
    assert_eq!(
        page_names(&window),
        [
            "audio",
            "command palette",
            "connection",
            "downloading",
            "duplicates",
            "exporting",
            "external programs",
            "file search",
            "file sort/collect",
            "file viewing statistics",
            "files and trash",
            "gui",
            "gui pages",
            "gui sessions",
            "import options",
            "importing",
            "maintenance and processing",
            "media playback",
            "media viewer",
            "media viewer hovers",
            "notes",
            "open externally",
            "popup notifications",
            "ratings",
            "regex favourites",
            "shortcuts",
            "speed and memory",
            "system",
            "tag autocomplete tabs",
            "tag editing",
            "tag presentation",
            "tag sort",
            "tag suggestions",
            "thumbnails",
            "advanced"
        ]
    );
    assert_eq!(page_names(&window)[window.get_page() as usize], "gui");
    show_page(&window, "audio");
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
    let mut remembered_before = before.clone();
    remembered_before.options_preferences.last_panel = "files and trash".into();
    assert_eq!(settings(), remembered_before);

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

/// The thumbnails' size, as the thumbnails page sets it: the grid's cells
/// take the new box at once, the thumbnails shown are at the size the new
/// settings make them (one stored at the old size scaled to it, as the
/// reference's `_GetThumbnailHydrusBitmap` does), and the stored one is
/// made again from its file at that size.
#[test]
#[allow(clippy::float_cmp, clippy::cast_precision_loss)] // (sizes set, not computed)
fn thumbnails_take_the_size_the_options_give_them() {
    let (_dirs, store) = store();
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(
        &ui,
        Pages::single(hydrus_gui::SearchPage::new(store.clone())),
    );
    ui.invoke_search_edited("system:everything".into());
    ui.invoke_search_accepted();
    let before = store.snapshot().thumbnails;
    assert_eq!(ui.get_thumbnail_width(), before.bounding_width as f32 + 2.0);
    // a big picture, its thumbnail as stored (made under the old settings)
    let results = bound.current.borrow().borrow().results().to_vec();
    let size = |id| {
        store
            .read(|conn| hydrus_store::media::resolution(conn, id))
            .unwrap()
            .unwrap()
    };
    let big = results
        .iter()
        .copied()
        .find(|&id| matches!(size(id), (Some(w), Some(h)) if w > 1000 && h > 1000))
        .expect("a big picture");
    let (w, h) = size(big);
    let hash = store
        .read(|conn| hydrus_store::master::hash(conn, big))
        .unwrap()
        .unwrap();
    let path = store.snapshot().storage.thumbnail_path(&hash).unwrap();
    let stored = || {
        let raster = hydrus_media::decode_image(&std::fs::read(&path).unwrap()).unwrap();
        (raster.width(), raster.height())
    };
    assert_eq!(stored(), before.resolution(w, h));
    // (and shown at it)
    let index = results.iter().position(|&id| id == big).unwrap();
    let shown = || {
        let images: Vec<slint::Image> = (0..bound.rows.row_count())
            .flat_map(|r| {
                let row = bound.rows.row_data(r).unwrap().thumbnails;
                (0..row.row_count())
                    .map(|t| row.row_data(t).unwrap().image)
                    .collect::<Vec<_>>()
            })
            .collect();
        let size = images[index].size();
        (size.width, size.height)
    };
    shown();
    bound.rows.wait();
    assert_eq!(shown(), before.resolution(w, h));

    open(&ui);
    let window = bound.options.borrow().as_ref().unwrap().clone_strong();
    show_page(&window, "thumbnails");
    let (i, width) = row(&window, "Thumbnail width: ");
    assert_eq!(
        (width.kind, width.number, width.minimum, width.maximum),
        (2, 150, 20, 2048)
    );
    window.invoke_number_edited(i, 300);
    let (i, height) = row(&window, "Thumbnail height: ");
    assert_eq!((height.kind, height.number), (2, 125));
    window.invoke_number_edited(i, 250);
    window.invoke_apply();
    let after = store.snapshot().thumbnails;
    assert_eq!((after.bounding_width, after.bounding_height), (300, 250));
    assert_eq!(ui.get_thumbnail_width(), 302.0);
    assert_eq!(ui.get_thumbnail_height(), 252.0);
    let expected = after.resolution(w, h);
    assert_ne!(expected, before.resolution(w, h));

    // shown at the new size
    shown();
    bound.rows.wait();
    assert_eq!(shown(), expected);
    // and made again at it
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    while stored() != expected && std::time::Instant::now() < deadline {
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    assert_eq!(stored(), expected);
}

/// The thumbnails' border and margin, as the thumbnails page sets them:
/// the cells take the border at once, and the margins how many fit across
/// (as the reference's, the width over a cell and two margins); the
/// thumbnails, of the same size, aren't decoded again.
#[test]
#[allow(clippy::float_cmp)] // (sizes set, not computed)
fn the_thumbnails_border_and_margin_lay_out_the_grid() {
    let (_dirs, store) = store();
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(
        &ui,
        Pages::single(hydrus_gui::SearchPage::new(store.clone())),
    );
    ui.invoke_search_edited("system:everything".into());
    ui.invoke_search_accepted();
    let main_window = windows.get(0).unwrap();
    headless::render(&main_window, 1100, 700);
    // (150 by 125 and a pixel's border, two pixels' margin: five across)
    assert_eq!(ui.get_thumbnail_width(), 152.0);
    assert_eq!(ui.get_grid_columns(), 5);
    bound.rows.wait();
    let decoded = bound.rows.cached();
    assert!(decoded > 0);

    open(&ui);
    let window = bound.options.borrow().as_ref().unwrap().clone_strong();
    show_page(&window, "thumbnails");
    let (i, border) = row(&window, "Thumbnail border: ");
    assert_eq!(
        (border.kind, border.number, border.minimum, border.maximum),
        (2, 1, 0, 20)
    );
    window.invoke_number_edited(i, 3);
    let (i, margin) = row(&window, "Thumbnail margin: ");
    assert_eq!((margin.kind, margin.number, margin.maximum), (2, 2, 20));
    window.invoke_number_edited(i, 20);
    window.invoke_apply();
    let layout = store
        .read(hydrus_store::settings::get::<hydrus_store::settings::ThumbnailLayout>)
        .unwrap();
    assert_eq!((layout.border, layout.margin), (3, 20));
    assert_eq!(ui.get_thumbnail_width(), 156.0);
    assert_eq!(ui.get_thumbnail_height(), 131.0);
    assert_eq!(ui.get_thumbnail_border(), 3.0);
    assert_eq!(ui.get_thumbnail_margin(), 20.0);
    // (at 1100 wide, four spans of 196; narrower, 700 or so across, three,
    // where the cells alone and a gap of 4 would fit four)
    headless::render(&main_window, 1100, 700);
    assert_eq!(ui.get_grid_columns(), 4);
    headless::render(&main_window, 1000, 700);
    assert_eq!(ui.get_grid_columns(), 3);
    assert_eq!(bound.rows.cached(), decoded, "not decoded again");
}

/// The file sort/collect page: the default and secondary sorts, each a
/// page's sort types and then the type's orders (a type chosen in its own
/// default order, as the reference's control sets it); and with "Update
/// default file sort every time a new sort is manually chosen", a sort
/// chosen on a page becomes the default (only then).
#[test]
fn the_default_sorts_are_chosen_as_a_pages_sort() {
    use hydrus_core::pages::{PageSort, PageSortBy, SortSettings};
    use hydrus_search::SortBy;

    let (_dirs, store) = store();
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(
        &ui,
        Pages::single(hydrus_gui::SearchPage::new(store.clone())),
    );
    let sorts = || {
        store
            .read(hydrus_store::settings::get::<SortSettings>)
            .unwrap()
    };
    let system = |by: SortBy| PageSortBy::System(i64::from(by.code()));
    let strings = |model: slint::ModelRc<slint::SharedString>| -> Vec<String> {
        (0..model.row_count())
            .map(|i| model.row_data(i).unwrap().to_string())
            .collect()
    };
    let before = sorts();
    // (a page's sort chosen while the option is off leaves the default)
    let page_sorts = strings(ui.get_sort_names());
    let width = page_sorts
        .iter()
        .position(|n| n == "dimensions: width")
        .unwrap();
    ui.invoke_sort_chosen(i32::try_from(width).unwrap());
    assert_eq!(sorts(), before);

    open(&ui);
    let window = bound.options.borrow().as_ref().unwrap().clone_strong();
    show_page(&window, "file sort/collect");
    let label = "Default file sort: ";
    let (i, default) = row(&window, label);
    assert_eq!(default.kind, 10);
    let types = strings(default.items.clone());
    assert_eq!(types, page_sorts, "a page's sort types");
    assert_eq!(
        types[usize::try_from(default.index).unwrap()],
        "file: filesize"
    );
    assert_eq!(
        (strings(default.orders.clone()), default.order_index),
        (
            vec!["smallest first".to_owned(), "largest first".to_owned()],
            0
        )
    );
    // a type: its orders shown, in its default order (newest first)
    let import_time = types.iter().position(|t| t == "time: import time").unwrap();
    window.invoke_sort_chosen(i, i32::try_from(import_time).unwrap());
    let (_, default) = row(&window, label);
    assert_eq!(
        (
            usize::try_from(default.index).unwrap(),
            strings(default.orders.clone()),
            default.order_index
        ),
        (
            import_time,
            vec!["oldest first".to_owned(), "newest first".to_owned()],
            1
        )
    );
    // and an order
    window.invoke_order_chosen(i, 0);
    assert_eq!(row(&window, label).1.order_index, 0);
    let (j, _) = row(
        &window,
        "Secondary file sort (when primary gives two equal values): ",
    );
    let filesize = types.iter().position(|t| t == "file: filesize").unwrap();
    window.invoke_sort_chosen(j, i32::try_from(filesize).unwrap());
    let (k, save) = row(
        &window,
        "Update default file sort every time a new sort is manually chosen: ",
    );
    assert!(!save.checked);
    window.invoke_check_toggled(k, true);
    window.invoke_apply();
    let after = sorts();
    assert_eq!(
        after.default_sort,
        PageSort {
            tag_context: hydrus_core::search::context::TagContext::default(),
            by: system(SortBy::ImportTime),
            ascending: true
        }
    );
    assert_eq!(
        after.fallback_sort,
        PageSort {
            tag_context: hydrus_core::search::context::TagContext::default(),
            by: system(SortBy::FileSize),
            ascending: false
        }
    );
    assert!(after.save_page_sort_on_change);
    assert_eq!(
        hydrus_gui::SearchPage::new(store.clone()).sort(),
        &after.default_sort,
        "a new page's"
    );

    // a page's sort chosen is now the default: its type, then its order
    ui.invoke_sort_chosen(i32::try_from(width).unwrap());
    assert_eq!(
        sorts().default_sort,
        PageSort {
            tag_context: hydrus_core::search::context::TagContext::default(),
            by: system(SortBy::Width),
            ascending: true
        }
    );
    ui.invoke_order_chosen(1);
    assert_eq!(
        sorts().default_sort,
        PageSort {
            tag_context: hydrus_core::search::context::TagContext::default(),
            by: system(SortBy::Width),
            ascending: false
        }
    );
}

/// The default collect, as a page's collect control offers it: its label,
/// its choices to check, and what files matching none of it do; applied,
/// a new page collects so.
#[test]
fn the_default_collect_is_chosen_as_a_pages_collect() {
    use hydrus_core::pages::SortSettings;

    let (_dirs, store) = store();
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(
        &ui,
        Pages::single(hydrus_gui::SearchPage::new(store.clone())),
    );
    let strings = |model: slint::ModelRc<slint::SharedString>| -> Vec<String> {
        (0..model.row_count())
            .map(|i| model.row_data(i).unwrap().to_string())
            .collect()
    };
    let checks = |model: slint::ModelRc<bool>| -> Vec<bool> {
        (0..model.row_count())
            .map(|i| model.row_data(i).unwrap())
            .collect()
    };
    open(&ui);
    let window = bound.options.borrow().as_ref().unwrap().clone_strong();
    show_page(&window, "file sort/collect");
    let label = "Default collect: ";
    let (i, collect) = row(&window, label);
    assert_eq!(collect.kind, 11);
    assert_eq!(collect.text, "no collections");
    let names = strings(collect.items.clone());
    let page_choices: Vec<String> = hydrus_gui::collect::choices(&store)
        .into_iter()
        .map(|c| c.name)
        .collect();
    assert_eq!(names, page_choices, "a page's choices");
    assert!(checks(collect.checks.clone()).iter().all(|c| !c));
    assert_eq!(
        (strings(collect.orders.clone()), collect.order_index),
        (
            vec![
                "collect into one group".to_owned(),
                "leave separate".to_owned()
            ],
            0
        )
    );
    // two choices checked: the label says them, in the choices' order
    let creator = names.iter().position(|n| n == "creator").unwrap();
    let series = names.iter().position(|n| n == "series").unwrap();
    window.invoke_collect_toggled(i, i32::try_from(series).unwrap(), true);
    window.invoke_collect_toggled(i, i32::try_from(creator).unwrap(), true);
    let (_, collect) = row(&window, label);
    assert_eq!(collect.text, "collect by creator-series");
    let checked = checks(collect.checks.clone());
    assert!(checked[creator] && checked[series]);
    assert_eq!(checked.iter().filter(|c| **c).count(), 2);
    // and unchecked again
    window.invoke_collect_toggled(i, i32::try_from(series).unwrap(), false);
    assert_eq!(row(&window, label).1.text, "collect by creator");
    window.invoke_unmatched_chosen(i, 1);
    assert_eq!(row(&window, label).1.order_index, 1);
    window.invoke_apply();
    let collect = store
        .read(hydrus_store::settings::get::<SortSettings>)
        .unwrap()
        .default_collect;
    assert_eq!(collect.namespaces, ["creator"]);
    assert!(collect.ratings.is_empty());
    assert!(!collect.collect_unmatched);
    assert_eq!(
        hydrus_gui::SearchPage::new(store.clone()).collect(),
        &collect,
        "a new page's"
    );
}

/// The tag sort page: a tag list's default sort as the reference's tag
/// sort control has it (a type, then its orders, then its grouping, which
/// a subtag sort hasn't); applied, a new page's tags sort so.
#[test]
fn the_default_tag_sorts_are_chosen() {
    use hydrus_core::tag_presentation::TagPresentation;
    use hydrus_core::tag_sort::{TagGroupBy, TagSort, TagSortType};

    let (_dirs, store) = store();
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(
        &ui,
        Pages::single(hydrus_gui::SearchPage::new(store.clone())),
    );
    let strings = |model: slint::ModelRc<slint::SharedString>| -> Vec<String> {
        (0..model.row_count())
            .map(|i| model.row_data(i).unwrap().to_string())
            .collect()
    };
    let shown = |row: &hydrus_gui::OptionRow| {
        let mut out = vec![
            strings(row.items.clone())[usize::try_from(row.index).unwrap()].clone(),
            strings(row.orders.clone())[usize::try_from(row.order_index).unwrap()].clone(),
        ];
        if row.grouped {
            out.push(
                strings(row.groups.clone())[usize::try_from(row.group_index).unwrap()].clone(),
            );
        }
        out
    };
    open(&ui);
    let window = bound.options.borrow().as_ref().unwrap().clone_strong();
    show_page(&window, "tag sort");
    let label = "Default tag sort in search pages: ";
    let (i, sort) = row(&window, label);
    assert_eq!(sort.kind, 12);
    assert_eq!(shown(&sort), ["sort by tag", "a-z", "namespace (user)"]);
    // by count: its own orders, most first
    window.invoke_tag_sort_chosen(i, 0, 2);
    let (_, sort) = row(&window, label);
    assert_eq!(strings(sort.orders.clone()), ["most first", "fewest first"]);
    assert_eq!(
        shown(&sort),
        ["sort by count", "most first", "namespace (user)"]
    );
    // by subtag: a-z, and no grouping to choose
    window.invoke_tag_sort_chosen(i, 0, 1);
    assert_eq!(shown(&row(&window, label).1), ["sort by subtag", "a-z"]);
    // by count again, fewest first, not grouped
    window.invoke_tag_sort_chosen(i, 0, 2);
    window.invoke_tag_sort_chosen(i, 1, 1);
    window.invoke_tag_sort_chosen(i, 2, 0);
    assert_eq!(
        shown(&row(&window, label).1),
        ["sort by count", "fewest first", "no grouping"]
    );
    let (j, _) = row(&window, "Default tag sort in the media viewer: ");
    window.invoke_tag_sort_chosen(j, 1, 1);
    window.invoke_apply();
    let presentation: TagPresentation = store.read(hydrus_store::settings::get).unwrap();
    assert_eq!(
        presentation.search_page_sort,
        TagSort {
            sort_type: TagSortType::Count,
            ascending: true,
            group_by: TagGroupBy::Nothing
        }
    );
    assert_eq!(
        presentation.media_viewer_sort,
        TagSort {
            ascending: false,
            ..TagSort::DEFAULT
        }
    );
    // a new page's tags: fewest first
    let mut page = hydrus_gui::SearchPage::new(store.clone());
    page.add_predicate("system:everything");
    let counts: Vec<u64> = page
        .tag_rows()
        .iter()
        .map(|row| {
            let n = row.rsplit_once(" (").unwrap().1.trim_end_matches(')');
            n.replace(',', "").parse().unwrap()
        })
        .collect();
    assert!(counts.len() > 3, "{counts:?}");
    assert!(counts.windows(2).all(|w| w[0] <= w[1]), "{counts:?}");
}

/// The ratings page's thumbnail options: on "apply", the grid draws its
/// ratings by them at once.
#[test]
#[allow(clippy::float_cmp)] // (sizes set, not computed)
fn the_thumbnail_rating_options_redraw_the_grid() {
    use hydrus_store::services::{self, ServiceKind, ServiceRegistry};
    let (_dirs, store) = store();
    // (the like service shown on every thumbnail, rated or not)
    store
        .write_and_refresh(|ctx| {
            let registry = ServiceRegistry::load(ctx.conn())?;
            let service = registry.by_name("favourites").unwrap();
            let mut kind = service.kind.clone();
            let ServiceKind::RatingLike(like) = &mut kind else {
                panic!("{kind:?}")
            };
            like.display.show_in_thumbnail = true;
            like.display.show_in_thumbnail_even_when_null = true;
            services::update_config(ctx.conn(), service.id, &kind)
        })
        .unwrap();
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(
        &ui,
        Pages::single(hydrus_gui::SearchPage::new(store.clone())),
    );
    ui.invoke_search_edited("system:everything".into());
    ui.invoke_search_accepted();
    // the first thumbnail's like: its size, and the boxes under it
    let like = || {
        let drawn = bound
            .rows
            .row_data(0)
            .unwrap()
            .thumbnails
            .row_data(0)
            .unwrap();
        let like = drawn.ratings.row_data(0).unwrap();
        (like.size, drawn.rating_boxes.row_count())
    };
    assert_eq!(like(), (12.0, 1));

    open(&ui);
    let window = bound.options.borrow().as_ref().unwrap().clone_strong();
    show_page(&window, "ratings");
    let (i, size) = row(
        &window,
        "Thumbnail like/dislike and numerical rating icon size: ",
    );
    assert_eq!((size.kind, size.text.as_str()), (4, "12.0"));
    window.invoke_text_edited(i, "20".into());
    let (i, background) = row(&window, "Give thumbnail ratings a flat background: ");
    assert!(background.checked);
    window.invoke_check_toggled(i, false);
    window.invoke_apply();
    let stored: hydrus_core::thumbnail::ThumbnailRatingSettings =
        store.read(hydrus_store::settings::get).unwrap();
    assert_eq!((stored.icon_size, stored.background), (20.0, false));
    assert_eq!(like(), (20.0, 0));
}

/// The downloading page's "checker options" buttons open the checker
/// options editor on the default checker options: a reasonable default's
/// button shows its times; a time typed below never faster than is moved
/// up to it; the same times ask first on "apply" (no leaves it open); and
/// what it applies is kept for the options' "apply", which stores it.
#[test]
#[allow(clippy::float_cmp)] // (set, not computed)
fn the_checker_options_buttons_edit_the_default_checker_options() {
    use hydrus_core::subscriptions::CheckerDefaults;
    use hydrus_gui::checker_options::{PRESETS, SAME_QUESTION};
    let (_dirs, store) = store();
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(
        &ui,
        Pages::single(hydrus_gui::SearchPage::new(store.clone())),
    );
    open(&ui);
    let window = bound.options.borrow().as_ref().unwrap().clone_strong();
    // (gone to from the search, so highlighted)
    window.invoke_search_edited("watcher checker".into());
    window.invoke_search_chosen(0);
    let (i, button) = row(&window, "Default watcher checker options:");
    assert_eq!((button.kind, button.text.as_str()), (13, "checker options"));
    assert!(button.found);
    window.invoke_checker_clicked(i);
    let editor = bound
        .checker_options
        .borrow()
        .as_ref()
        .expect("the editor opens")
        .clone_strong();
    let values = |fields: slint::ModelRc<hydrus_gui::DurationField>| -> Vec<i32> {
        (0..fields.row_count())
            .map(|f| fields.row_data(f).unwrap().value)
            .collect()
    };
    // (the fixture's: a thread's, 5 minutes to a day, dead below a file in
    // three days)
    assert_eq!(values(editor.get_faster_fields()), [0, 0, 5, 0]);
    assert_eq!(values(editor.get_slower_fields()), [1, 0, 0, 0]);
    assert_eq!(values(editor.get_velocity_fields()), [3, 0, 0]);
    assert_eq!(editor.get_velocity_files().as_str(), "1");
    assert_eq!(editor.get_intended().as_str(), "4.00");
    assert!(!editor.get_flat());
    let presets = editor.get_presets();
    assert_eq!(presets.row_count(), PRESETS.len());
    assert_eq!(presets.row_data(1).unwrap().as_str(), "slow thread");
    assert_eq!(
        editor.get_texts().faster_label.as_str(),
        "never check faster than once per: "
    );
    // slow thread: 4 hours to 7 days, dead below a file in 30 days
    editor.invoke_preset(1);
    assert_eq!(values(editor.get_faster_fields()), [0, 4, 0, 0]);
    assert_eq!(values(editor.get_slower_fields()), [7, 0, 0, 0]);
    assert_eq!(values(editor.get_velocity_fields()), [30, 0, 0]);
    assert_eq!(editor.get_intended().as_str(), "1.00");
    // never slower than typed below never faster than: moved up to it
    editor.invoke_field_edited(1, 0, 0);
    assert_eq!(values(editor.get_slower_fields()), [0, 4, 0, 0]);
    // the same: "apply" asks; no leaves it open
    editor.invoke_ok();
    assert_eq!(editor.get_question().as_str(), SAME_QUESTION);
    editor.invoke_answered(false);
    assert_eq!(editor.get_question().as_str(), "");
    assert!(bound.checker_options.borrow().is_some());
    editor.invoke_field_edited(1, 0, 2);
    // (past its range, it shows where it was held)
    editor.invoke_velocity_files_edited(5000);
    assert_eq!(editor.get_velocity_files().as_str(), "1000");
    editor.invoke_velocity_files_edited(3);
    editor.invoke_intended_edited("2.5".into());
    editor.invoke_ok();
    assert!(bound.checker_options.borrow().is_none(), "closed");
    // (its row shown again, still highlighted)
    assert!(row(&window, "Default watcher checker options:").1.found);
    // kept for "apply"
    assert_eq!(
        store
            .read(hydrus_store::settings::get::<CheckerDefaults>)
            .unwrap()
            .watchers
            .never_faster_than,
        300,
        "not yet stored"
    );
    window.invoke_apply();
    let stored: CheckerDefaults = store.read(hydrus_store::settings::get).unwrap();
    assert_eq!(stored.watchers.intended_files_per_check, 2.5);
    assert_eq!(
        (
            stored.watchers.never_faster_than,
            stored.watchers.never_slower_than
        ),
        (4 * 3600, 2 * 86400 + 4 * 3600)
    );
    assert_eq!(stored.watchers.death_file_velocity, (3, 30 * 86400));
    assert_eq!(
        stored.subscriptions,
        CheckerDefaults::default().subscriptions
    );
}

#[test]
fn options_remember_navigation_and_apply_search_placement() {
    let (_dirs, store) = store();
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    let recorded = hydrus_testkit::fixture_json("options_preferences.json");
    open(&ui);
    let window = bound.options.borrow().as_ref().unwrap().clone_strong();
    assert_eq!(page_names(&window)[window.get_page() as usize], "gui");
    assert_eq!(
        window.get_search_at_top(),
        recorded["cases"][0]["top"].as_bool().unwrap()
    );
    show_page(&window, "connection");
    window.invoke_cancel();
    open(&ui);
    let window = bound.options.borrow().as_ref().unwrap().clone_strong();
    assert_eq!(
        page_names(&window)[window.get_page() as usize],
        "connection"
    );
    show_page(&window, "gui");
    let (remember, _) = row(&window, "Remember last open options panel in this window: ");
    let (placement, _) = row(&window, "Put the options search bar at the: ");
    window.invoke_check_toggled(remember, false);
    window.invoke_choice_chosen(placement, 1);
    show_page(&window, "audio");
    window.invoke_apply();
    open(&ui);
    let window = bound.options.borrow().as_ref().unwrap().clone_strong();
    assert_eq!(page_names(&window)[window.get_page() as usize], "gui");
    assert!(!window.get_search_at_top());
    window.invoke_search_edited("bottom of this window (gui)".into());
    assert_eq!(window.get_matches().row_count(), 1);
    window.invoke_search_chosen(0);
    assert!(row(&window, "Put the options search bar at the: ").1.found);
    show_page(&window, "connection");
    window.invoke_cancel();
    let preferences = store
        .read(hydrus_store::settings::get::<hydrus_store::settings::OptionsPreferences>)
        .unwrap();
    assert!(!preferences.remember_panel);
    assert_eq!(preferences.last_panel, "audio");
}

#[test]
fn gui_identity_and_exit_confirmation_reach_the_main_window() {
    let (_dirs, store) = store();
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    let recorded = hydrus_testkit::fixture_json("gui_settings.json");
    let close = || {
        ui.window()
            .dispatch_event_with_result(slint::platform::WindowEvent::CloseRequested)
            .unwrap()
    };
    for name in recorded["names"].as_array().unwrap() {
        open(&ui);
        let window = bound.options.borrow().as_ref().unwrap().clone_strong();
        show_page(&window, "gui");
        let (row_index, _) = row(&window, "Application display name: ");
        window.invoke_text_edited(row_index, name["typed"].as_str().unwrap().into());
        window.invoke_apply();
        assert_eq!(
            ui.get_window_title(),
            format!(
                "{} {}",
                name["saved"].as_str().unwrap(),
                env!("CARGO_PKG_VERSION")
            )
        );
    }
    // A canceled switch leaves the live exit consumer unchanged.
    open(&ui);
    let window = bound.options.borrow().as_ref().unwrap().clone_strong();
    show_page(&window, "gui");
    let (row_index, _) = row(&window, "Confirm client exit: ");
    window.invoke_check_toggled(row_index, true);
    window.invoke_cancel();
    assert!(
        !store
            .read(hydrus_store::settings::get::<hydrus_store::settings::GuiSettings>)
            .unwrap()
            .confirm_exit
    );
    for case in recorded["exits"].as_array().unwrap() {
        open(&ui);
        let window = bound.options.borrow().as_ref().unwrap().clone_strong();
        show_page(&window, "gui");
        let (row_index, _) = row(&window, "Confirm client exit: ");
        window.invoke_check_toggled(row_index, case["confirm"].as_bool().unwrap());
        window.invoke_apply();
        ui.show().unwrap();
        close();
        let expected_question = case["events"]
            .as_array()
            .unwrap()
            .iter()
            .find_map(|event| event["question"].as_str());
        if let Some(question) = expected_question {
            assert!(ui.window().is_visible());
            assert_eq!(ui.get_question(), question);
            ui.invoke_answer(case["yes"].as_bool().unwrap());
        }
        let exits = case["events"]
            .as_array()
            .unwrap()
            .iter()
            .any(|event| event["exit_requested"] == true);
        assert_eq!(!ui.window().is_visible(), exits);
        assert_eq!(ui.get_question(), "");
    }
}

#[test]
fn applied_session_backup_count_controls_the_next_save() {
    use hydrus_core::pages::Session;
    use hydrus_store::session_backups;
    let (_dirs, store) = store();
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    open(&ui);
    let window = bound.options.borrow().as_ref().unwrap().clone_strong();
    show_page(&window, "gui sessions");
    let (row_index, control) = row(&window, "Number of session backups to keep: ");
    assert_eq!((control.minimum, control.maximum), (1, 32));
    window.invoke_number_edited(row_index, 2);
    window.invoke_apply();
    let fixture = hydrus_testkit::fixture_json("session_backups.json");
    for step in fixture["steps"].as_array().unwrap() {
        let now = step["now"].as_i64().unwrap();
        store
            .write(move |ctx| {
                session_backups::save(
                    ctx.conn(),
                    &Session {
                        name: "options backup test".into(),
                        pages: Vec::new(),
                    },
                    now,
                )
            })
            .unwrap();
        let backups = store.read(session_backups::names).unwrap();
        let times = backups
            .iter()
            .find(|(name, _)| name == "options backup test")
            .map(|(_, times)| times.clone())
            .unwrap_or_default();
        assert_eq!(serde_json::json!(times), step["backups"]);
    }
    open(&ui);
    let window = bound.options.borrow().as_ref().unwrap().clone_strong();
    show_page(&window, "gui sessions");
    assert_eq!(
        row(&window, "Number of session backups to keep: ").1.number,
        2
    );
    window.invoke_cancel();
}

#[test]
fn notebook_focus_option_changes_the_next_tab_close() {
    use hydrus_core::service::builtin_keys;
    use hydrus_gui::page_chooser::NewPage;
    let (_dirs, store) = store();
    let _windows = headless::init();
    let mut pages = Pages::single(hydrus_gui::SearchPage::new(store.clone()));
    let mut left_key = None;
    for name in ["left", "middle", "right"] {
        pages
            .new_page(&NewPage::Search {
                domain: hydrus_core::ServiceKey::new(
                    builtin_keys::HYDRUS_LOCAL_FILE_STORAGE.to_vec(),
                ),
                name: name.into(),
            })
            .unwrap();
        if name == "left" {
            left_key = Some(pages.shown().key);
        }
    }
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, pages);
    open(&ui);
    let window = bound.options.borrow().as_ref().unwrap().clone_strong();
    show_page(&window, "gui pages");
    let (index, _) = row(&window, "When closing the current tab, move focus: ");
    window.invoke_choice_chosen(index, 0);
    let (index, _) = row(
        &window,
        "  Also automatically prompt when sending some pages to one: ",
    );
    window.invoke_check_toggled(index, true);
    window.invoke_apply();
    ui.invoke_tab_chosen(0, 2);
    ui.invoke_close_tab(0, 2);
    assert_eq!(ui.get_tab_rows().row_data(0).unwrap().selected, 1);
    assert_eq!(bound.pages.borrow().shown().key, left_key.unwrap());
    let stored = store
        .read(hydrus_store::settings::get::<hydrus_store::sessions::NotebookSettings>)
        .unwrap();
    assert!(stored.close_focus_left && stored.rename_sent_notebooks);
    open(&ui);
    let window = bound.options.borrow().as_ref().unwrap().clone_strong();
    show_page(&window, "gui pages");
    let (index, _) = row(&window, "When closing the current tab, move focus: ");
    window.invoke_choice_chosen(index, 1);
    window.invoke_cancel();
    assert!(
        store
            .read(hydrus_store::settings::get::<hydrus_store::sessions::NotebookSettings>)
            .unwrap()
            .close_focus_left
    );
}

#[test]
fn regex_favourites_open_from_options_and_keep_parent_transaction() {
    let (_dirs, store) = store();
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    let original = store.read(hydrus_store::regex_favourites::load).unwrap();
    for apply in [false, true] {
        open(&ui);
        let window = bound.options.borrow().as_ref().unwrap().clone_strong();
        show_page(&window, "regex favourites");
        window.invoke_regex_favourites_clicked();
        let child = hydrus_gui::regex_favourites_window::last_opened().unwrap();
        child.invoke_action("add".into());
        child.set_phrase("^example$".into());
        child.set_description("example favourite".into());
        child.invoke_changed();
        assert!(child.get_valid());
        child.invoke_action("save-row".into());
        child.invoke_action("apply".into());
        assert_eq!(
            store.read(hydrus_store::regex_favourites::load).unwrap(),
            original
        );
        if apply {
            window.invoke_apply();
        } else {
            window.invoke_cancel();
        }
        let saved = store.read(hydrus_store::regex_favourites::load).unwrap();
        assert_eq!(
            saved.0.iter().any(|(phrase, _)| phrase == "^example$"),
            apply
        );
    }
}

#[test]
fn sleep_options_apply_to_the_network_consumer_and_cancel_drafts() {
    use hydrus_net::NetOptions;
    use hydrus_store::network::NetworkSettings;
    use hydrus_store::settings::get;
    let (_dirs, store) = store();
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    let fixture = hydrus_testkit::fixture_json("system_sleep_options.json");
    for case in fixture["cases"].as_array().unwrap() {
        open(&ui);
        let window = bound.options.borrow().as_ref().unwrap().clone_strong();
        show_page(&window, "system");
        let (index, _) = row(&window, "Allow wake-from-system-sleep detection:");
        window.invoke_check_toggled(index, case["enabled"].as_bool().unwrap());
        let (index, control) = row(
            &window,
            "After a wake from system sleep, wait this many seconds before allowing new network access:",
        );
        assert_eq!((control.minimum, control.maximum), (0, 60));
        window.invoke_number_edited(index, case["delay"].as_i64().unwrap() as i32);
        window.invoke_apply();
        let settings = store.read(get::<NetworkSettings>).unwrap();
        let consumer = NetOptions::from_settings(&settings);
        assert_eq!(consumer.detect_sleep, case["enabled"].as_bool().unwrap());
        assert_eq!(consumer.wake_delay, case["delay"].as_u64().unwrap());
    }
    let before = store.read(get::<NetworkSettings>).unwrap();
    open(&ui);
    let window = bound.options.borrow().as_ref().unwrap().clone_strong();
    show_page(&window, "system");
    let (index, _) = row(&window, "Allow wake-from-system-sleep detection:");
    window.invoke_check_toggled(index, !before.detect_sleep);
    window.invoke_cancel();
    assert_eq!(store.read(get::<NetworkSettings>).unwrap(), before);
    open(&ui);
    let window = bound.options.borrow().as_ref().unwrap().clone_strong();
    show_page(&window, "system");
    assert_eq!(
        row(&window, "Allow wake-from-system-sleep detection:")
            .1
            .checked,
        before.detect_sleep
    );
    window.invoke_cancel();
}

#[test]
fn hash_prefix_option_reaches_selected_and_focused_clipboard_hashes() {
    use hydrus_gui::thumbnail_menu::{HashKind, hashes};
    use hydrus_store::settings::{FileHandlingSettings, get};
    let (_dirs, store) = store();
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    let fixture = hydrus_testkit::fixture_json("hash_clipboard_options.json");
    for case in fixture["cases"].as_array().unwrap() {
        open(&ui);
        let window = bound.options.borrow().as_ref().unwrap().clone_strong();
        show_page(&window, "files and trash");
        let (index, _) = row(
            &window,
            "When copying file hashes, prefix with booru-friendly hash type: ",
        );
        window.invoke_check_toggled(index, case["prefix"].as_bool().unwrap());
        window.invoke_apply();
        let files = case["hashes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|hash| {
                store
                    .read(|conn| {
                        hydrus_store::master::hash_id(
                            conn,
                            &hash.as_str().unwrap().parse().unwrap(),
                        )
                    })
                    .unwrap()
                    .unwrap()
            })
            .collect::<Vec<_>>();
        let kind = match case["kind"].as_str().unwrap() {
            "sha256" => HashKind::Sha256,
            "md5" => HashKind::Md5,
            "sha1" => HashKind::Sha1,
            "sha512" => HashKind::Sha512,
            "blurhash" => HashKind::Blurhash,
            "pixel_hash" => HashKind::PixelHash,
            other => panic!("unrecorded hash type {other}"),
        };
        let lines = hashes(&store, &files, kind);
        let publications = if lines.is_empty() {
            Vec::new()
        } else {
            vec![lines.join("\n")]
        };
        assert_eq!(serde_json::json!(publications), case["clipboard"]);
    }
    let before = store.read(get::<FileHandlingSettings>).unwrap();
    open(&ui);
    let window = bound.options.borrow().as_ref().unwrap().clone_strong();
    show_page(&window, "files and trash");
    let (index, _) = row(
        &window,
        "When copying file hashes, prefix with booru-friendly hash type: ",
    );
    window.invoke_check_toggled(index, !before.prefix_hash_when_copying);
    window.invoke_cancel();
    assert_eq!(store.read(get::<FileHandlingSettings>).unwrap(), before);
}

#[test]
fn tag_dialog_service_dropdown_tracks_remember_checkbox_and_parent_transaction() {
    use hydrus_store::settings::get;
    use hydrus_store::tag_editing::TagEditingSettings;
    let (_dirs, store) = store();
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    let fixture = hydrus_testkit::fixture_json("tag_dialog_defaults.json");
    for control in fixture["controls"].as_array().unwrap() {
        open(&ui);
        let window = bound.options.borrow().as_ref().unwrap().clone_strong();
        show_page(&window, "tag editing");
        let (remember, _) = row(
            &window,
            "Remember last used default tag service in manage tag dialogs: ",
        );
        window.invoke_check_toggled(remember, control["remember"].as_bool().unwrap());
        let (service_row, service) = row(&window, "Default tag service in tag dialogs: ");
        assert_eq!(
            service.enabled,
            control["default_enabled"].as_bool().unwrap()
        );
        let names = (0..service.items.row_count())
            .map(|index| service.items.row_data(index).unwrap().to_string())
            .collect::<Vec<_>>();
        assert_eq!(serde_json::json!(names), control["choices"]);
        if service.enabled {
            let at = names
                .iter()
                .position(|name| name == control["service"].as_str().unwrap())
                .unwrap();
            window.invoke_choice_chosen(service_row, at as i32);
        }
        window.invoke_apply();
        let saved = store.read(get::<TagEditingSettings>).unwrap();
        assert_eq!(
            saved.remember_service,
            control["remember"].as_bool().unwrap()
        );
        assert_eq!(
            store
                .snapshot()
                .services
                .by_key(&saved.default_service)
                .unwrap()
                .name,
            control["service"].as_str().unwrap()
        );
    }
    let before = store.read(get::<TagEditingSettings>).unwrap();
    open(&ui);
    let window = bound.options.borrow().as_ref().unwrap().clone_strong();
    show_page(&window, "tag editing");
    let (remember, _) = row(
        &window,
        "Remember last used default tag service in manage tag dialogs: ",
    );
    window.invoke_check_toggled(remember, !before.remember_service);
    window.invoke_cancel();
    assert_eq!(store.read(get::<TagEditingSettings>).unwrap(), before);
}

#[test]
fn default_search_service_option_changes_new_pages_and_missing_keys_fall_back() {
    use hydrus_core::service::builtin_keys;
    use hydrus_gui::page_chooser::NewPage;
    use hydrus_store::settings::{SearchDefaults, get};
    let (_dirs, store) = store();
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    let fixture = hydrus_testkit::fixture_json("search_default_service.json");
    for (index, event) in fixture["events"].as_array().unwrap().iter().enumerate() {
        if index < fixture["choices"].as_array().unwrap().len() {
            open(&ui);
            let window = bound.options.borrow().as_ref().unwrap().clone_strong();
            show_page(&window, "file search");
            let (service_row, service) = row(&window, "Default tag service in search pages:");
            let names = (0..service.items.row_count())
                .map(|index| service.items.row_data(index).unwrap().to_string())
                .collect::<Vec<_>>();
            let reference = fixture["choices"]
                .as_array()
                .unwrap()
                .iter()
                .map(|choice| choice["name"].as_str().unwrap())
                .collect::<Vec<_>>();
            assert_eq!(names, reference);
            window.invoke_choice_chosen(service_row, index as i32);
            window.invoke_apply();
        } else {
            let key = hydrus_core::ServiceKey::from_hex(event["saved"].as_str().unwrap()).unwrap();
            store
                .write(move |ctx| {
                    let mut defaults = get::<SearchDefaults>(ctx.conn())?;
                    defaults.tag_service = key;
                    hydrus_store::settings::set(ctx.conn(), &defaults)
                })
                .unwrap();
        }
        bound
            .pages
            .borrow_mut()
            .new_page(&NewPage::Search {
                domain: hydrus_core::ServiceKey::new(builtin_keys::MY_FILES.to_vec()),
                name: "option default search".into(),
            })
            .unwrap();
        let pages = bound.pages.borrow();
        let hydrus_core::pages::PageContent::Search { search, .. } = &pages.shown().content else {
            panic!("the new search page must retain its search context");
        };
        assert_eq!(
            search.tags.service.to_hex(),
            event["page_service"].as_str().unwrap()
        );
    }
}

#[test]
fn removed_default_service_falls_back_in_options_and_waits_for_apply() {
    use hydrus_store::settings::{SearchDefaults, get, set};
    let (_dirs, store) = store();
    let _windows = headless::init();
    store
        .write(|ctx| {
            let mut defaults = get::<SearchDefaults>(ctx.conn())?;
            defaults.tag_service = hydrus_core::ServiceKey::new(b"missing service".to_vec());
            set(ctx.conn(), &defaults)
        })
        .unwrap();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    for apply in [false, true] {
        open(&ui);
        let window = bound.options.borrow().as_ref().unwrap().clone_strong();
        show_page(&window, "file search");
        assert_eq!(
            row(&window, "Default tag service in search pages:").1.index,
            0
        );
        if apply {
            window.invoke_apply();
        } else {
            window.invoke_cancel();
        }
        assert_eq!(
            store
                .read(get::<SearchDefaults>)
                .unwrap()
                .tag_service
                .as_bytes()
                == hydrus_core::service::builtin_keys::COMBINED_TAG,
            apply
        );
    }
}

#[test]
fn search_defaults_apply_to_new_pages_and_the_real_autocomplete() {
    use hydrus_core::ServiceKey;
    use hydrus_core::service::builtin_keys;
    use hydrus_gui::page_chooser::NewPage;
    use hydrus_store::settings::{FileSearchSettings, get};
    let (_dirs, store) = store();
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    let fixture = hydrus_testkit::fixture_json("file_search_defaults.json");
    let saved = || store.read(get::<FileSearchSettings>).unwrap();
    assert_eq!(
        saved().search_immediately,
        fixture["initial"]["search_immediately"].as_bool().unwrap()
    );
    assert_eq!(
        saved().show_system_everything,
        fixture["initial"]["show_system_everything"]
            .as_bool()
            .unwrap()
    );
    open(&ui);
    let window = bound.options.borrow().as_ref().unwrap().clone_strong();
    show_page(&window, "file search");
    let (sync_row, _) = row(
        &window,
        "Start new search pages in 'searching immediately':",
    );
    let (everything_row, _) = row(&window, "Show system:everything:");
    window.invoke_check_toggled(sync_row, false);
    window.invoke_check_toggled(everything_row, false);
    window.invoke_cancel();
    assert!(saved().search_immediately && saved().show_system_everything);

    let mut first_page = None;
    for event in fixture["events"].as_array().unwrap() {
        let enabled = event["enabled"].as_bool().unwrap();
        open(&ui);
        let window = bound.options.borrow().as_ref().unwrap().clone_strong();
        show_page(&window, "file search");
        let (sync_row, _) = row(
            &window,
            "Start new search pages in 'searching immediately':",
        );
        let (everything_row, _) = row(&window, "Show system:everything:");
        window.invoke_check_toggled(sync_row, enabled);
        window.invoke_check_toggled(everything_row, enabled);
        window.invoke_apply();
        assert_eq!(saved().search_immediately, enabled);
        assert_eq!(saved().show_system_everything, enabled);
        bound
            .pages
            .borrow_mut()
            .new_page(&NewPage::Search {
                domain: ServiceKey::new(builtin_keys::MY_FILES.to_vec()),
                name: "synthetic search default recording".into(),
            })
            .unwrap();
        let page = bound.pages.borrow_mut().current();
        let first = first_page.get_or_insert_with(|| page.clone());
        assert_eq!(
            page.borrow().synchronised(),
            event["new_page_synchronised"].as_bool().unwrap()
        );
        assert_eq!(
            first.borrow().synchronised(),
            event["first_page_synchronised"].as_bool().unwrap()
        );
        for offered in event["offered"].as_array().unwrap() {
            let mut autocomplete = hydrus_gui::autocomplete::Autocomplete::new(store.clone());
            let location = hydrus_core::search::context::LocationContext::single(
                ServiceKey::from_hex(offered["location"].as_str().unwrap()).unwrap(),
            );
            autocomplete.set_context(
                &location,
                &hydrus_core::search::context::TagContext::default(),
            );
            assert_eq!(
                autocomplete
                    .suggestions()
                    .iter()
                    .any(|item| item.predicate == "system:everything"),
                offered["everything"].as_bool().unwrap()
            );
            assert!(
                autocomplete
                    .suggestions()
                    .iter()
                    .any(|item| item.predicate == "system:limit")
            );
        }
        assert!(page.borrow_mut().add_predicate("system:everything"));
        if !enabled {
            assert!(
                page.borrow().results().is_empty(),
                "paused page must defer its query"
            );
            page.borrow_mut().set_synchronised(true);
        }
        assert!(
            !page.borrow().results().is_empty(),
            "resuming must execute the staged query"
        );
        // Keep the first page paused while the new-page default changes next.
        page.borrow_mut().set_synchronised(enabled);
    }
    open(&ui);
    let reopened = bound.options.borrow().as_ref().unwrap().clone_strong();
    show_page(&reopened, "file search");
    assert!(row(&reopened, "Show system:everything:").1.checked);
    reopened.invoke_cancel();
}

#[test]
fn default_local_location_child_draft_drives_blank_pages_and_tag_fallback() {
    use hydrus_core::ServiceKey;
    use hydrus_core::search::context::LocationContext;
    use hydrus_core::service::builtin_keys;
    use hydrus_store::settings::{SearchDefaults, get};
    let (_dirs, store) = store();
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    let fixture = hydrus_testkit::fixture_json("default_search_location.json");
    let defaults = || store.read(get::<SearchDefaults>).unwrap();
    let before = defaults();
    open(&ui);
    let window = bound.options.borrow().as_ref().unwrap().clone_strong();
    show_page(&window, "file search");
    let (location_row, location) = row(&window, "Default/Fallback local file search location:");
    assert_eq!(location.text, fixture["initial"]["label"].as_str().unwrap());
    window.invoke_local_location_clicked(location_row);
    let child = hydrus_gui::locations_window::last_opened().unwrap();
    child.invoke_toggled(0, true);
    window.invoke_cancel();
    assert!(!child.window().is_visible());
    child.invoke_apply();
    assert_eq!(defaults(), before, "cancel invalidates child callbacks");

    for event in fixture["events"].as_array().unwrap() {
        open(&ui);
        let window = bound.options.borrow().as_ref().unwrap().clone_strong();
        show_page(&window, "file search");
        let (location_row, _) = row(&window, "Default/Fallback local file search location:");
        window.invoke_local_location_clicked(location_row);
        let child = hydrus_gui::locations_window::last_opened().unwrap();
        let choices = fixture["choices"].as_array().unwrap();
        assert_eq!(child.get_ticks().row_count(), choices.len());
        for (index, choice) in choices.iter().enumerate() {
            assert_eq!(
                child.get_ticks().row_data(index).unwrap().label,
                choice["label"].as_str().unwrap()
            );
            let selected = event["selected"]["current"]
                .as_array()
                .unwrap()
                .iter()
                .any(|key| key == &choice["data"]);
            child.invoke_toggled(index as i32, selected);
        }
        let unchanged = defaults();
        child.invoke_apply();
        assert_eq!(
            defaults(),
            unchanged,
            "child Apply only changes parent draft"
        );
        window.invoke_apply();
        let keys = |location: &LocationContext| {
            location
                .current()
                .iter()
                .map(ServiceKey::to_hex)
                .collect::<Vec<_>>()
        };
        let expected = |field: &str| {
            event[field]["current"]
                .as_array()
                .unwrap()
                .iter()
                .map(|value| value.as_str().unwrap().to_owned())
                .collect::<Vec<_>>()
        };
        assert_eq!(keys(&defaults().local_location), expected("selected"));
        bound.pages.borrow_mut().new_search_page();
        let page = bound.pages.borrow_mut().current();
        assert_eq!(keys(page.borrow().location()), expected("new_page"));
        page.borrow_mut()
            .choose_location(LocationContext::single(ServiceKey::new(
                builtin_keys::COMBINED_FILE.to_vec(),
            )));
        page.borrow_mut()
            .choose_tag_service(ServiceKey::new(builtin_keys::COMBINED_TAG.to_vec()));
        assert_eq!(keys(page.borrow().location()), expected("fallback"));
        let standalone = hydrus_gui::SearchPage::new(store.clone());
        assert_eq!(keys(standalone.location()), expected("resolved"));
    }
    store
        .write(|ctx| {
            let mut value = get::<SearchDefaults>(ctx.conn())?;
            value.local_location =
                LocationContext::single(ServiceKey::new(b"missing synthetic file domain".to_vec()));
            hydrus_store::settings::set(ctx.conn(), &value)
        })
        .unwrap();
    open(&ui);
    let window = bound.options.borrow().as_ref().unwrap().clone_strong();
    show_page(&window, "file search");
    assert_eq!(
        row(&window, "Default/Fallback local file search location:")
            .1
            .text,
        fixture["missing"]["label"].as_str().unwrap()
    );
    window.invoke_apply();
    assert_eq!(
        defaults()
            .local_location
            .current()
            .iter()
            .map(ServiceKey::to_hex)
            .collect::<Vec<_>>(),
        fixture["missing"]["current"]
            .as_array()
            .unwrap()
            .iter()
            .map(|key| key.as_str().unwrap().to_owned())
            .collect::<Vec<_>>()
    );
}

#[test]
fn read_list_sizes_and_float_policy_reach_rendered_new_pages() {
    use hydrus_core::ServiceKey;
    use hydrus_core::service::builtin_keys;
    use hydrus_gui::page_chooser::NewPage;
    let (_dirs, store) = store();
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    let fixture = hydrus_testkit::fixture_json("file_search_presentation.json");
    ui.show().unwrap();
    let main = windows.get(0).unwrap();
    main.dispatch_event(slint::platform::WindowEvent::WindowActiveChanged(true));
    let mut first = None;
    let mut floated_tags_y = None;
    for (index, event) in fixture["events"].as_array().unwrap().iter().enumerate() {
        open(&ui);
        let window = bound.options.borrow().as_ref().unwrap().clone_strong();
        show_page(&window, "file search");
        let (active_row, active) = row(&window, "Active Search Predicates list height:");
        let (suggestion_row, suggestions) = row(&window, "Autocomplete list height:");
        assert_eq!((active.minimum, active.maximum), (1, 128));
        assert_eq!((suggestions.minimum, suggestions.maximum), (1, 128));
        let (float_row, _) = row(
            &window,
            "Autocomplete dropdown floats over file search pages:",
        );
        window.invoke_number_edited(active_row, event["active_rows"].as_i64().unwrap() as i32);
        window.invoke_number_edited(
            suggestion_row,
            event["autocomplete_rows"].as_i64().unwrap() as i32,
        );
        window.invoke_check_toggled(float_row, event["floating"].as_bool().unwrap());
        window.invoke_apply();
        (bound.open_page)(&NewPage::Search {
            domain: ServiceKey::new(builtin_keys::MY_FILES.to_vec()),
            name: "synthetic search presentation recording".into(),
        });
        let page = bound.pages.borrow_mut().current();
        let original = first.get_or_insert_with(|| page.clone());
        let config = original
            .borrow()
            .autocomplete()
            .presentation_settings()
            .clone();
        assert_eq!(
            config.active_predicate_rows,
            event["first_view"]["active"]["rows"].as_u64().unwrap() as u32
        );
        assert_eq!(
            config.autocomplete_rows,
            event["first_view"]["autocomplete"]["rows"]
                .as_u64()
                .unwrap() as u32
        );
        assert_eq!(
            config.float_autocomplete,
            event["first_view"]["floating"].as_bool().unwrap()
        );
        assert_eq!(
            ui.get_active_predicate_rows(),
            event["view"]["active"]["rows"].as_i64().unwrap() as i32
        );
        assert_eq!(
            ui.get_autocomplete_rows(),
            event["view"]["autocomplete"]["rows"].as_i64().unwrap() as i32
        );
        assert_eq!(
            !ui.get_float_autocomplete(),
            event["view"]["embedded_in_layout"].as_bool().unwrap()
        );
        // The reference dropdown contains generated search suggestions; a
        // pristine new input has no results until its first edit/fetch.
        ui.invoke_search_edited("".into());
        assert!(ui.get_suggestions().row_count() > 0);
        ui.set_search_focus_requests(ui.get_search_focus_requests() + 1);
        // Live geometry bindings publish the conditional input's initial
        // layout; the overlay is rendered at its actual bottom edge.
        headless::render(&main, 1100, 1500);
        slint::platform::update_timers_and_animations();
        let pixels = headless::render(&main, 1100, 1500);
        assert!(pixels.iter().any(|pixel| *pixel != 0));
        assert!(ui.get_active_predicate_list_height() >= 28.0);
        assert!(
            ui.get_active_predicate_list_height() <= ui.get_active_predicate_preferred_height()
        );
        if index == 0 {
            assert!(ui.get_search_focused());
            assert!(ui.get_float_autocomplete());
            assert!(!ui.get_search_locked());
            assert!(ui.get_note().is_empty());
            assert!(ui.get_question().is_empty());
            assert_eq!(ui.get_menu_panes().row_count(), 0);
            assert_eq!(ui.get_chooser_labels().row_count(), 0);
            assert!(
                ui.get_autocomplete_anchor_width() > 0.0,
                "initial conditional search input publishes its width"
            );
            assert!(ui.get_autocomplete_anchor_x() >= 0.0);
            assert!(
                ui.get_autocomplete_anchor_y() > ui.get_active_predicate_list_height(),
                "floating results begin below the actual search input"
            );
            assert!(ui.get_autocomplete_overlay_visible());
            floated_tags_y = Some(ui.get_search_tags_y());
        } else if index == 1 {
            assert!(!ui.get_autocomplete_overlay_visible());
            assert!(
                ui.get_search_tags_y() > floated_tags_y.unwrap() + 200.0,
                "embedded results must occupy sidebar layout space; floating results overlay it"
            );
        }
        ui.set_search_focused(false);
        assert!(
            !ui.get_autocomplete_overlay_visible(),
            "floating results hide without input focus"
        );
    }
}

#[test]
fn implicit_limit_options_reach_queries_and_limited_sort_refresh() {
    use hydrus_search::{SortBy, SortOrder};
    use hydrus_store::settings::{FileSearchSettings, get};
    let (_dirs, store) = store();
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    let fixture = hydrus_testkit::fixture_json("file_search_limits.json");
    open(&ui);
    let window = bound.options.borrow().as_ref().unwrap().clone_strong();
    show_page(&window, "file search");
    let (limit_row, limit) = row(&window, "Implicit system:limit for all searches: ");
    assert_eq!((limit.minimum, limit.maximum), (1, 100_000_000));
    assert_eq!(limit.none_phrase, "no limit");
    assert_eq!(limit.is_none, fixture["initial"]["limit"].is_null());
    window.invoke_none_toggled(limit_row, false);
    window.invoke_number_edited(limit_row, 3);
    window.invoke_cancel();
    assert!(
        store
            .read(get::<FileSearchSettings>)
            .unwrap()
            .implicit_limit
            .is_none()
    );
    for case in fixture["refreshes"].as_array().unwrap() {
        let enabled = case["enabled"].as_bool().unwrap();
        open(&ui);
        let window = bound.options.borrow().as_ref().unwrap().clone_strong();
        show_page(&window, "file search");
        let (limit_row, _) = row(&window, "Implicit system:limit for all searches: ");
        let (refresh_row, _) = row(
            &window,
            "If explicit system:limit, then refresh search when file sort changes: ",
        );
        window.invoke_none_toggled(limit_row, false);
        window.invoke_number_edited(limit_row, 3);
        window.invoke_check_toggled(refresh_row, enabled);
        window.invoke_apply();
        let saved = store.read(get::<FileSearchSettings>).unwrap();
        assert_eq!(saved.implicit_limit, Some(3));
        assert_eq!(saved.refresh_limited_sort, enabled);
        let mut page = hydrus_gui::SearchPage::new(store.clone());
        page.set_sort_by(SortBy::Hash);
        page.set_sort_order(SortOrder::Ascending);
        assert!(page.add_predicate("system:everything"));
        assert_eq!(
            page.results().len(),
            3,
            "implicit limit reaches the search engine"
        );
        if case["explicit"].as_bool().unwrap() {
            assert!(page.add_predicate("system:limit is 3"));
        }
        let original = page
            .results()
            .iter()
            .copied()
            .collect::<std::collections::BTreeSet<_>>();
        page.set_synchronised(case["sync"].as_bool().unwrap());
        page.set_sort_by(SortBy::from_code(case["code"].as_i64().unwrap()).unwrap());
        page.set_sort_order(SortOrder::Descending);
        let after = page
            .results()
            .iter()
            .copied()
            .collect::<std::collections::BTreeSet<_>>();
        if case["refresh_count"].as_u64().unwrap() > 0 {
            assert_ne!(
                after, original,
                "rerun must choose the other end of the hash-sorted search"
            );
        } else {
            assert_eq!(
                after, original,
                "guarded sort changes only reorder the current subset"
            );
        }
    }
    let mut page = hydrus_gui::SearchPage::new(store.clone());
    assert!(page.add_predicate("system:everything"));
    assert!(page.add_predicate("system:limit is 8"));
    assert_eq!(
        page.results().len(),
        8,
        "larger explicit limit overrides the implicit limit"
    );
}

#[test]
fn canvas_options_apply_to_the_open_viewer_and_cancel_discards_the_draft() {
    use hydrus_store::media::FileFlags;
    use hydrus_store::settings::{self, ViewerCanvasSettings};
    let fixture = hydrus_testkit::fixture_json("viewer_canvas_options.json");
    let (_dirs, store) = store();
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(
        &ui,
        Pages::single(hydrus_gui::SearchPage::new(store.clone())),
    );
    ui.invoke_search_edited("system:everything".into());
    ui.invoke_search_accepted();
    let files = bound.current.borrow().borrow().results().to_vec();
    let basic = store
        .read(|conn| hydrus_store::media::load_basic(conn, &files))
        .unwrap();
    let alpha = basic
        .iter()
        .position(|file| {
            file.info.as_ref().is_some_and(|info| {
                info.mime == hydrus_core::Mime::ImagePng && info.flags.has(FileFlags::TRANSPARENCY)
            })
        })
        .unwrap();
    let opaque = basic
        .iter()
        .position(|file| {
            file.info
                .as_ref()
                .is_some_and(|info| info.mime == hydrus_core::Mime::ImageJpeg)
        })
        .unwrap();
    ui.invoke_thumbnail_activated(i32::try_from(alpha).unwrap());
    let viewer = bound.viewer.borrow().as_ref().unwrap().clone_strong();
    let drawn = windows.get(windows.count() - 1).unwrap();
    headless::render(&drawn, 800, 600);
    assert_eq!(viewer.get_transparency_mode(), 0);
    open(&ui);
    let options = bound.options.borrow().as_ref().unwrap().clone_strong();
    show_page(&options, "media playback");
    options.invoke_check_toggled(
        row(&options, "Draw image transparency as checkerboard:").0,
        true,
    );
    options.invoke_check_toggled(row(&options, "Re-center media on window resize:").0, false);
    assert_eq!(viewer.get_transparency_mode(), 0, "draft does not repaint");
    options.invoke_cancel();
    assert_eq!(
        store.read(settings::get::<ViewerCanvasSettings>).unwrap(),
        ViewerCanvasSettings::default()
    );
    for event in fixture["backgrounds"].as_array().unwrap() {
        open(&ui);
        let options = bound.options.borrow().as_ref().unwrap().clone_strong();
        show_page(&options, "media playback");
        let checker = event["checker"].as_bool().unwrap();
        let green = event["green"].as_bool().unwrap();
        options.invoke_check_toggled(
            row(&options, "Draw image transparency as checkerboard:").0,
            checker,
        );
        options.invoke_check_toggled(
            row(
                &options,
                "--Instead of checkerboard, use a bright greenscreen:",
            )
            .0,
            green,
        );
        options.invoke_apply();
        assert_eq!(
            viewer.get_transparency_mode(),
            if checker {
                if green { 2 } else { 1 }
            } else {
                0
            }
        );
        // An empty transparent image isolates the background pixels while the
        // real viewer's metadata and Options callback decide its brush mode.
        viewer.set_media(slint::Image::default());
        viewer.set_sharp_shown(false);
        viewer.set_sharp(slint::Image::default());
        viewer.set_media_x(0.0);
        viewer.set_media_y(0.0);
        viewer.set_media_width(800.0);
        viewer.set_media_height(600.0);
        let pixels = headless::render(&drawn, 800, 600);
        for (reference, x, y) in [
            ("0,0", 32_usize, 128_usize),
            ("16,0", 48, 128),
            ("0,16", 32, 144),
            ("16,16", 48, 144),
            ("32,0", 64, 128),
        ] {
            let at = (y * 800 + x) * 4;
            let expected: Vec<u8> = if checker {
                event["pixels"][reference]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|value| u8::try_from(value.as_u64().unwrap()).unwrap())
                    .collect()
            } else {
                vec![32, 32, 32]
            };
            assert_eq!(
                &pixels[at..at + 3],
                expected.as_slice(),
                "{event:?}: {reference}"
            );
        }
    }
    open(&ui);
    let options = bound.options.borrow().as_ref().unwrap().clone_strong();
    show_page(&options, "media playback");
    assert!(
        row(&options, "Draw image transparency as checkerboard:")
            .1
            .checked
    );
    assert!(
        row(
            &options,
            "--Instead of checkerboard, use a bright greenscreen:"
        )
        .1
        .checked
    );
    options.invoke_check_toggled(
        row(&options, "Draw image transparency as checkerboard:").0,
        false,
    );
    options.invoke_cancel();
    assert_eq!(viewer.get_transparency_mode(), 2);
    viewer.invoke_close_requested();
    ui.invoke_thumbnail_activated(i32::try_from(opaque).unwrap());
    let opaque_viewer = bound.viewer.borrow().as_ref().unwrap().clone_strong();
    assert_eq!(
        opaque_viewer.get_transparency_mode(),
        0,
        "opaque metadata uses the ordinary canvas background"
    );
    opaque_viewer.invoke_close_requested();
}

#[test]
#[allow(clippy::float_cmp)] // logical pixel geometry is exact
fn resize_and_seek_options_reach_the_native_viewer_geometry() {
    use hydrus_store::settings::{self, ViewerCanvasSettings};
    use slint::{LogicalPosition, platform::WindowEvent};
    let fixture = hydrus_testkit::fixture_json("viewer_canvas_options.json");
    let (_dirs, store) = store();
    let animation =
        hydrus_import::FileImporter::new(store.clone(), hydrus_media::MediaTools::new())
            .import_path(
                &hydrus_testkit::fixture_path("media/webp_anim.webp"),
                &hydrus_import::FileImportOptions::default(),
            )
            .unwrap()
            .hash
            .unwrap();
    let animation_id = store
        .read(|conn| hydrus_store::master::hash_id(conn, &animation))
        .unwrap()
        .unwrap();
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(
        &ui,
        Pages::single(hydrus_gui::SearchPage::new(store.clone())),
    );
    ui.invoke_search_edited("system:everything".into());
    ui.invoke_search_accepted();
    let files = bound.current.borrow().borrow().results().to_vec();
    let basic = store
        .read(|conn| hydrus_store::media::load_basic(conn, &files))
        .unwrap();
    let jpeg = basic
        .iter()
        .position(|file| {
            file.info
                .as_ref()
                .is_some_and(|info| info.mime == hydrus_core::Mime::ImageJpeg)
        })
        .unwrap();
    ui.invoke_thumbnail_activated(i32::try_from(jpeg).unwrap());
    let viewer = bound.viewer.borrow().as_ref().unwrap().clone_strong();
    let drawn = windows.get(windows.count() - 1).unwrap();
    headless::render(&drawn, 1000, 750);
    viewer.invoke_zoom(0, false, 0.0, 0.0);
    viewer.invoke_zoom(1, false, 0.0, 0.0);
    viewer.invoke_drag(37.0, -19.0);
    let rect = || {
        (
            viewer.get_media_x(),
            viewer.get_media_y(),
            viewer.get_media_width(),
            viewer.get_media_height(),
        )
    };
    let before = rect();
    open(&ui);
    let options = bound.options.borrow().as_ref().unwrap().clone_strong();
    show_page(&options, "media playback");
    options.invoke_check_toggled(row(&options, "Re-center media on window resize:").0, false);
    options.invoke_apply();
    headless::render(&drawn, 800, 600);
    assert_eq!(rect(), before, "live resize policy preserves the zoom/pan");
    open(&ui);
    let options = bound.options.borrow().as_ref().unwrap().clone_strong();
    show_page(&options, "media playback");
    options.invoke_check_toggled(row(&options, "Re-center media on window resize:").0, true);
    options.invoke_apply();
    headless::render(&drawn, 600, 450);
    let info = basic[jpeg].info.as_ref().unwrap();
    let expected = hydrus_gui::zoom::Zoom::new(
        hydrus_core::media_viewer::MediaViewerSettings::default(),
        info.mime,
        Some((info.width.unwrap(), info.height.unwrap())),
        (600, 450),
        1.0,
    )
    .rect();
    assert_eq!(
        rect(),
        (
            expected.0 as f32,
            expected.1 as f32,
            expected.2 as f32,
            expected.3 as f32
        )
    );
    viewer.invoke_close_requested();
    let index = files.iter().position(|file| *file == animation_id).unwrap();
    ui.invoke_thumbnail_activated(i32::try_from(index).unwrap());
    let viewer = bound.viewer.borrow().as_ref().unwrap().clone_strong();
    assert!(
        viewer.get_scanbar_shown(),
        "native animation has a seek bar"
    );
    viewer.invoke_toggle_pause();
    let drawn = windows.get(windows.count() - 1).unwrap();
    headless::render(&drawn, 800, 600);
    for event in fixture["seek"].as_array().unwrap() {
        open(&ui);
        let options = bound.options.borrow().as_ref().unwrap().clone_strong();
        show_page(&options, "media viewer");
        options.invoke_number_edited(
            row(&options, "Seek bar height:").0,
            event["height"].as_i64().unwrap() as i32,
        );
        let hidden = row(&options, "Seek bar height when mouse away:").0;
        options.invoke_none_toggled(hidden, event["hidden_height"].is_null());
        if let Some(height) = event["hidden_height"].as_i64() {
            options.invoke_number_edited(hidden, height as i32);
        }
        options.invoke_number_edited(
            row(&options, "Seek bar nub width:").0,
            event["nub"].as_i64().unwrap() as i32,
        );
        options.invoke_apply();
        let height = event["height"].as_u64().unwrap() as usize;
        let hidden_height = event["hidden_height"].as_u64().unwrap_or(0) as usize;
        let nub = event["nub"].as_u64().unwrap() as usize;
        assert_eq!(viewer.get_seek_height(), height as f32);
        assert_eq!(viewer.get_seek_hidden_height(), hidden_height as f32);
        assert_eq!(viewer.get_seek_nub_width(), nub as f32);
        assert_eq!(
            event["hidden_visible"].as_bool().unwrap(),
            hidden_height > 0
        );
        viewer.set_media(slint::Image::default());
        viewer.set_sharp_shown(false);
        viewer.set_media_x(0.0);
        viewer.set_media_y(0.0);
        viewer.set_media_width(128.0);
        viewer.set_media_height(600.0);
        viewer.set_scanbar_progress(0.5);
        viewer.set_scanbar_text("".into());
        viewer.window().dispatch_event(WindowEvent::PointerMoved {
            position: LogicalPosition::new(120.0, (600 - height) as f32 + 0.5),
        });
        let full = headless::render_snapshot(&drawn, 800, 600);
        let pixel = |pixels: &[u8], x: usize, y: usize| {
            pixels[(y * 800 + x) * 4..(y * 800 + x) * 4 + 3].to_vec()
        };
        assert_eq!(
            pixel(&full, 120, 600 - height),
            vec![96, 96, 96],
            "full seek height {height}"
        );
        assert_ne!(pixel(&full, 120, 599 - height), vec![96, 96, 96]);
        if height > 2 {
            let left = event["nub_x"][2].as_u64().unwrap() as usize;
            assert_eq!(pixel(&full, left, 600 - height + 1), vec![176, 176, 176]);
            assert_eq!(
                pixel(&full, left + nub - 1, 600 - height + 1),
                vec![176, 176, 176]
            );
            assert_ne!(
                pixel(&full, left + nub, 600 - height + 1),
                vec![176, 176, 176]
            );
        }
        viewer.window().dispatch_event(WindowEvent::PointerMoved {
            position: LogicalPosition::new(799.0, 200.0),
        });
        let small = headless::render_snapshot(&drawn, 800, 600);
        assert_eq!(
            pixel(&small, 120, 599 - hidden_height),
            vec![32, 32, 32],
            "outside collapsed bar"
        );
        if hidden_height > 0 {
            assert_eq!(pixel(&small, 120, 600 - hidden_height), vec![96, 96, 96]);
        } else {
            assert_eq!(
                pixel(&small, 120, 599),
                vec![32, 32, 32],
                "None hides the bar completely"
            );
        }
    }
    open(&ui);
    let options = bound.options.borrow().as_ref().unwrap().clone_strong();
    show_page(&options, "media viewer");
    assert_eq!(row(&options, "Seek bar height:").1.number, 37);
    assert_eq!(row(&options, "Seek bar nub width:").1.number, 19);
    options.invoke_number_edited(row(&options, "Seek bar height:").0, 255);
    options.invoke_cancel();
    assert_eq!(viewer.get_seek_height(), 37.0);
    assert_eq!(
        store
            .read(settings::get::<ViewerCanvasSettings>)
            .unwrap()
            .seek_height,
        37
    );
    viewer.invoke_close_requested();
}

#[test]
fn hover_options_apply_to_actual_mouseover_panels_and_passive_index_text() {
    use hydrus_store::settings::{self, ViewerHoverSettings};
    use slint::{LogicalPosition, platform::WindowEvent};
    let fixture = hydrus_testkit::fixture_json("viewer_hover_options.json");
    let (_dirs, store) = store();
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(
        &ui,
        Pages::single(hydrus_gui::SearchPage::new(store.clone())),
    );
    ui.invoke_search_edited("system:everything".into());
    ui.invoke_search_accepted();
    let files = bound.current.borrow().borrow().results().to_vec();
    let basic = store
        .read(|conn| hydrus_store::media::load_basic(conn, &files))
        .unwrap();
    let index = basic
        .iter()
        .position(|file| {
            file.info
                .as_ref()
                .is_some_and(|info| info.mime == hydrus_core::Mime::ImageJpeg)
        })
        .unwrap();
    let id = files[index];
    store
        .write_content(move |writer| writer.set_note(id, "details", "synthetic viewer hover note"))
        .unwrap();
    let mut tags = hydrus_gui::manage_tags::ManageTags::new(store.clone(), vec![id]).unwrap();
    let mine = tags
        .service_names()
        .iter()
        .position(|name| name == "my tags")
        .unwrap();
    tags.choose_service(mine).unwrap();
    tags.enter("synthetic viewer hover").unwrap();
    tags.apply().unwrap();
    ui.invoke_thumbnail_activated(i32::try_from(index).unwrap());
    let viewer = bound.viewer.borrow().as_ref().unwrap().clone_strong();
    let drawn = windows.get(windows.count() - 1).unwrap();
    headless::render(&drawn, 1000, 750);
    // Finish the initial asynchronous image before isolating background pixels.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    while !viewer.get_sharp_shown() && std::time::Instant::now() < deadline {
        slint::platform::update_timers_and_animations();
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert!(viewer.get_sharp_shown(), "initial JPEG image arrived");
    assert!(viewer.get_tags().row_count() > 0);
    assert!(viewer.get_ratings().row_count() > 0);
    assert!(viewer.get_notes().row_count() > 0);
    let notes_y = (60..740)
        .step_by(10)
        .find(|&y| {
            viewer.window().dispatch_event(WindowEvent::PointerMoved {
                position: LogicalPosition::new(980.0, y as f32),
            });
            viewer.get_notes_showing()
        })
        .expect("notes hover has a reachable region below the ratings");
    let labels = [
        "Pop-in tags (left) hover window on mouseover:",
        "Pop-in ratings and locations (top-right) hover window on mouseover:",
        "Pop-in notes (right) hover window on mouseover:",
        "Draw index text (bottom-right) in the viewer background:",
    ];
    let move_to = |x: f32, y: f32| {
        viewer.window().dispatch_event(WindowEvent::PointerMoved {
            position: LogicalPosition::new(x, y),
        });
    };
    for event in fixture["events"].as_array().unwrap() {
        open(&ui);
        let options = bound.options.borrow().as_ref().unwrap().clone_strong();
        show_page(&options, "media viewer hovers");
        for (i, label) in labels.iter().enumerate() {
            options.invoke_check_toggled(
                row(&options, label).0,
                event["values"][i].as_bool().unwrap(),
            );
        }
        options.invoke_apply();
        let settings = store.read(settings::get::<ViewerHoverSettings>).unwrap();
        assert_eq!(
            serde_json::json!([
                settings.tags,
                settings.ratings,
                settings.notes,
                settings.index_background
            ]),
            event["values"]
        );
        // Reference disabled hover layouts have no hit region; native gates
        // its existing panels at the same mouseover points.
        for (i, (x, y)) in [(40.0, 300.0), (980.0, 5.0), (980.0, notes_y as f32)]
            .into_iter()
            .enumerate()
        {
            move_to(x, y);
            let showing = [
                viewer.get_tags_showing(),
                viewer.get_ratings_showing(),
                viewer.get_notes_showing(),
            ][i];
            assert_eq!(
                showing,
                event["ideals"][i]["size"][0].as_u64().unwrap() > 0,
                "hover {i}: {event:?}"
            );
        }
        // Isolate the passive index region from the real media and hovers.
        // The current caption/zoom are controlled by recorded viewer values.
        move_to(500.0, 400.0);
        viewer.set_caption(event["index"].as_str().unwrap().into());
        viewer.set_zoom_text(
            hydrus_gui::viewer_menu::zoom_percentage(event["zoom"].as_f64().unwrap()).into(),
        );
        viewer.set_media(slint::Image::default());
        viewer.set_sharp_shown(false);
        viewer.set_media_x(350.0);
        viewer.set_media_y(250.0);
        viewer.set_media_width(100.0);
        viewer.set_media_height(100.0);
        let pixels = headless::render(&drawn, 1000, 750);
        let expected = event["draws"]
            .as_array()
            .unwrap()
            .first()
            .and_then(|draw| draw["text"].as_str())
            .unwrap_or("");
        assert_eq!(viewer.get_index_background_text(), expected);
        let ink = (700_usize..747)
            .flat_map(|y| (700_usize..997).map(move |x| (y * 1000 + x) * 4))
            .filter(|&at| pixels[at] > 100 && pixels[at + 1] > 100 && pixels[at + 2] > 100)
            .count();
        assert_eq!(
            ink > 0,
            !expected.is_empty(),
            "passive index is painted only when enabled"
        );
        if !expected.is_empty() {
            // It is canvas background text: opaque media painted over the
            // same region covers it, as Qt child media cover their parent.
            let mut image = slint::SharedPixelBuffer::<slint::Rgba8Pixel>::new(4, 3);
            for pixel in image.make_mut_bytes().chunks_exact_mut(4) {
                pixel.copy_from_slice(&[32, 32, 32, 255]);
            }
            viewer.set_media(slint::Image::from_rgba8(image));
            viewer.set_media_x(0.0);
            viewer.set_media_y(0.0);
            viewer.set_media_width(1000.0);
            viewer.set_media_height(750.0);
            let covered = headless::render(&drawn, 1000, 750);
            let ink = (700_usize..747)
                .flat_map(|y| (700_usize..997).map(move |x| (y * 1000 + x) * 4))
                .filter(|&at| covered[at] > 100 && covered[at + 1] > 100 && covered[at + 2] > 100)
                .count();
            assert_eq!(ink, 0, "media covers passive canvas text");
        }
    }
    open(&ui);
    let options = bound.options.borrow().as_ref().unwrap().clone_strong();
    show_page(&options, "media viewer hovers");
    for label in labels {
        options.invoke_check_toggled(row(&options, label).0, true);
    }
    options.invoke_cancel();
    let settings = store.read(settings::get::<ViewerHoverSettings>).unwrap();
    assert_eq!(
        settings,
        ViewerHoverSettings {
            tags: false,
            ratings: false,
            notes: false,
            index_background: true
        }
    );
    assert!(!viewer.get_hover_tags_enabled());
    assert!(!viewer.get_hover_ratings_enabled());
    assert!(!viewer.get_hover_notes_enabled());
    open(&ui);
    let options = bound.options.borrow().as_ref().unwrap().clone_strong();
    show_page(&options, "media viewer hovers");
    assert!(!row(&options, labels[0]).1.checked);
    assert!(row(&options, labels[3]).1.checked);
    options.invoke_cancel();
    viewer.invoke_close_requested();
}

#[test]
#[allow(clippy::float_cmp)] // exact integer pointer deltas in logical pixels
fn pointer_options_change_real_drag_acceptance_and_cursor_transitions() {
    use hydrus_store::settings::{self, ViewerPointerSettings};
    use slint::{
        LogicalPosition,
        platform::{PointerEventButton, WindowEvent},
    };
    let fixture = hydrus_testkit::fixture_json("viewer_pointer_options.json");
    let (_dirs, store) = store();
    let animation =
        hydrus_import::FileImporter::new(store.clone(), hydrus_media::MediaTools::new())
            .import_path(
                &hydrus_testkit::fixture_path("media/webp_anim.webp"),
                &hydrus_import::FileImportOptions::default(),
            )
            .unwrap()
            .hash
            .unwrap();
    let animation_id = store
        .read(|conn| hydrus_store::master::hash_id(conn, &animation))
        .unwrap()
        .unwrap();
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(
        &ui,
        Pages::single(hydrus_gui::SearchPage::new(store.clone())),
    );
    ui.invoke_search_edited("system:everything".into());
    ui.invoke_search_accepted();
    let files = bound.current.borrow().borrow().results().to_vec();
    let basic = store
        .read(|conn| hydrus_store::media::load_basic(conn, &files))
        .unwrap();
    let jpeg = basic
        .iter()
        .position(|file| {
            file.info
                .as_ref()
                .is_some_and(|info| info.mime == hydrus_core::Mime::ImageJpeg)
        })
        .unwrap();
    let animation_index = files.iter().position(|id| *id == animation_id).unwrap();
    let labels = [
        "Do not allow mouse media drag-panning when the media has duration:",
        "Hide mouse cursor during media viewer drags:",
    ];
    let mut showing_duration = false;
    ui.invoke_thumbnail_activated(i32::try_from(jpeg).unwrap());
    let mut drawn = windows.get(windows.count() - 1).unwrap();
    for (case, event) in fixture["drags"].as_array().unwrap().iter().enumerate() {
        let duration = event["has_duration"].as_bool().unwrap();
        if duration != showing_duration {
            let previous = bound.viewer.borrow().as_ref().unwrap().clone_strong();
            previous.invoke_close_requested();
            ui.invoke_thumbnail_activated(i32::try_from(animation_index).unwrap());
            showing_duration = duration;
            drawn = windows.get(windows.count() - 1).unwrap();
        }
        let viewer = bound.viewer.borrow().as_ref().unwrap().clone_strong();
        headless::render(&drawn, 1000, 750);
        assert_eq!(
            viewer.get_media_has_duration(),
            duration,
            "real file metadata"
        );
        open(&ui);
        let options = bound.options.borrow().as_ref().unwrap().clone_strong();
        show_page(&options, "media viewer");
        options.invoke_check_toggled(
            row(&options, labels[0]).0,
            event["disallow"].as_bool().unwrap(),
        );
        options.invoke_check_toggled(row(&options, labels[1]).0, event["hide"].as_bool().unwrap());
        options.invoke_apply();
        assert_eq!(
            viewer.get_disallow_duration_drag(),
            event["disallow"].as_bool().unwrap()
        );
        assert_eq!(
            viewer.get_hide_during_drag(),
            event["hide"].as_bool().unwrap()
        );
        // Distinct press points avoid synthesising a double click that closes
        // the actual viewer. The media delta is independent of the origin.
        let x: f32 = 400.0 + case as f32 * 35.0;
        let at = |dx: f32, dy: f32| LogicalPosition::new(x + dx, 350.0 + dy);
        let moved = |dx, dy| {
            viewer.window().dispatch_event(WindowEvent::PointerMoved {
                position: at(dx, dy),
            });
        };
        moved(0.0, 0.0);
        let before = (viewer.get_media_x(), viewer.get_media_y());
        viewer.window().dispatch_event(WindowEvent::PointerPressed {
            position: at(0.0, 0.0),
            button: PointerEventButton::Left,
        });
        assert_eq!(
            viewer.get_drag_accepted(),
            event["accepted"].as_bool().unwrap()
        );
        let cursor_hidden =
            |phase: &str| event["cursor"][phase] == fixture["cursor_values"]["blank"];
        assert_eq!(viewer.get_drag_cursor_hidden(), cursor_hidden("pressed"));
        moved(21.0, 7.0);
        assert_eq!(
            (
                viewer.get_media_x() - before.0,
                viewer.get_media_y() - before.1
            ),
            (
                event["delta"][0].as_i64().unwrap() as f32,
                event["delta"][1].as_i64().unwrap() as f32
            )
        );
        assert_eq!(viewer.get_drag_cursor_hidden(), cursor_hidden("moved"));
        viewer
            .window()
            .dispatch_event(WindowEvent::PointerReleased {
                position: at(21.0, 7.0),
                button: PointerEventButton::Left,
            });
        assert!(!viewer.get_drag_accepted());
        assert_eq!(viewer.get_drag_cursor_hidden(), cursor_hidden("released"));
        moved(30.0, 19.0);
        assert_eq!(
            viewer.get_drag_cursor_hidden(),
            cursor_hidden("ordinary_move")
        );
    }
    let viewer = bound.viewer.borrow().as_ref().unwrap().clone_strong();
    let persisted = store.read(settings::get::<ViewerPointerSettings>).unwrap();
    assert!(persisted.disallow_duration_drag && persisted.hide_during_drag);
    let before = viewer.get_media_x();
    viewer.invoke_pan(1, 0);
    assert_ne!(
        viewer.get_media_x(),
        before,
        "keyboard panning remains available for duration media"
    );
    open(&ui);
    let options = bound.options.borrow().as_ref().unwrap().clone_strong();
    show_page(&options, "media viewer");
    assert!(row(&options, labels[0]).1.checked && row(&options, labels[1]).1.checked);
    options.invoke_check_toggled(row(&options, labels[0]).0, false);
    options.invoke_check_toggled(row(&options, labels[1]).0, false);
    options.invoke_cancel();
    assert!(viewer.get_disallow_duration_drag() && viewer.get_hide_during_drag());
    assert_eq!(
        store.read(settings::get::<ViewerPointerSettings>).unwrap(),
        persisted
    );
    viewer.invoke_close_requested();
    ui.invoke_thumbnail_activated(i32::try_from(animation_index).unwrap());
    let reopened = bound.viewer.borrow().as_ref().unwrap().clone_strong();
    assert!(
        reopened.get_media_has_duration()
            && reopened.get_disallow_duration_drag()
            && reopened.get_hide_during_drag()
    );
    reopened.invoke_close_requested();
}

#[test]
fn favourite_tags_child_replays_reference_and_waits_for_parent_apply() {
    use hydrus_store::settings::{self, FavouriteTags};
    let fixture = hydrus_testkit::fixture_json("write_tag_autocomplete.json");
    let recorded = &fixture["favourite_options"];
    let initial: Vec<String> = serde_json::from_value(recorded["initial"].clone()).unwrap();
    let initial_setting = FavouriteTags(initial);
    let (_dirs, store) = store();
    store
        .write(move |ctx| settings::set(ctx.conn(), &initial_setting))
        .unwrap();
    let original = store.read(settings::get::<FavouriteTags>).unwrap();
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    for apply in [false, true] {
        open(&ui);
        let parent = bound.options.borrow().as_ref().unwrap().clone_strong();
        show_page(&parent, "tag autocomplete tabs");
        assert_eq!(row(&parent, "These tags will appear in every tag autocomplete results dropdown, under the 'favourites' tab.").1.kind, 17);
        parent.invoke_favourite_tags_clicked();
        let child = hydrus_gui::write_tag_window::last_opened().unwrap();
        assert_eq!(child.get_tag_label(), "all known tags");
        child.invoke_edited("parity:cancelled".into());
        child.invoke_entered();
        child.invoke_cancel();
        parent.invoke_favourite_tags_clicked();
        let child = hydrus_gui::write_tag_window::last_opened().unwrap();
        assert!(
            !child
                .get_tags()
                .iter()
                .any(|row| row.text == "parity:cancelled")
        );
        for event in recorded["events"].as_array().unwrap() {
            match event["action"].as_str().unwrap() {
                "initial" => {}
                "manual" | "repeat_manual" => {
                    child.invoke_edited("parity:favourite 1".into());
                    child.invoke_entered();
                }
                "paste" | "repeat_paste" => {
                    hydrus_gui::set_paster(|| {
                        "parity:favourite 2\nparity:favourite 3\nparity:favourite pasted".into()
                    });
                    assert!(child.invoke_paste(true));
                }
                "remove" => {
                    let index = child
                        .get_tags()
                        .iter()
                        .position(|row| row.text == "parity:favourite 2")
                        .unwrap();
                    child.invoke_remove(i32::try_from(index).unwrap());
                }
                "apply" => {
                    parent.invoke_apply();
                    assert!(bound.options.borrow().is_some());
                    child.invoke_apply();
                }
                action => panic!("unexpected recorded action {action}"),
            }
            let tags: Vec<String> = child
                .get_tags()
                .iter()
                .map(|row| row.text.to_string())
                .collect();
            assert_eq!(serde_json::json!(tags), event["rows"]);
            assert_eq!(
                store.read(settings::get::<FavouriteTags>).unwrap(),
                original
            );
        }
        if apply {
            parent.invoke_apply();
        } else {
            parent.invoke_cancel();
        }
        let saved = store.read(settings::get::<FavouriteTags>).unwrap();
        let expected = if apply {
            &recorded["cancelled_saved"]
        } else {
            &recorded["initial"]
        };
        assert_eq!(serde_json::json!(saved.0), *expected);
    }
    ui.invoke_search_edited("system:everything".into());
    ui.invoke_search_accepted();
    ui.invoke_select_all();
    ui.invoke_manage_tags_selected();
    let manage = bound.manage_tags.borrow().as_ref().unwrap().clone_strong();
    manage.invoke_tab_chosen(1);
    let offered: std::collections::BTreeSet<String> = manage
        .get_suggestions()
        .iter()
        .map(|row| row.text.to_string())
        .collect();
    let expected: Vec<String> =
        serde_json::from_value(recorded["cancelled_saved"].clone()).unwrap();
    assert_eq!(offered, expected.into_iter().collect());
    manage.invoke_cancel();
    // Closing the parent closes its child and invalidates retained callbacks.
    open(&ui);
    let parent = bound.options.borrow().as_ref().unwrap().clone_strong();
    parent.invoke_favourite_tags_clicked();
    let child = hydrus_gui::write_tag_window::last_opened().unwrap();
    let saved = store.read(settings::get::<FavouriteTags>).unwrap();
    child.invoke_edited("parity:stale callback".into());
    child.invoke_entered();
    parent
        .window()
        .dispatch_event(slint::platform::WindowEvent::CloseRequested);
    assert!(!child.window().is_visible());
    child.invoke_apply();
    parent.invoke_apply();
    assert!(bound.options.borrow().is_none());
    assert_eq!(store.read(settings::get::<FavouriteTags>).unwrap(), saved);
}

#[test]
#[allow(clippy::float_cmp)] // whole logical-pixel bar heights
fn focus_options_reach_native_activity_and_actual_mouseover_gates() {
    use hydrus_store::settings::{self, ViewerFocusSettings};
    use slint::{
        LogicalPosition,
        platform::{PointerEventButton, WindowEvent},
    };
    let fixture = hydrus_testkit::fixture_json("viewer_focus_options.json");
    let (_dirs, store) = store();
    let animation =
        hydrus_import::FileImporter::new(store.clone(), hydrus_media::MediaTools::new())
            .import_path(
                &hydrus_testkit::fixture_path("media/webp_anim.webp"),
                &hydrus_import::FileImportOptions::default(),
            )
            .unwrap()
            .hash
            .unwrap();
    let id = store
        .read(|conn| hydrus_store::master::hash_id(conn, &animation))
        .unwrap()
        .unwrap();
    store
        .write_content(move |writer| writer.set_note(id, "details", "synthetic focus note"))
        .unwrap();
    let mut tags = hydrus_gui::manage_tags::ManageTags::new(store.clone(), vec![id]).unwrap();
    let mine = tags
        .service_names()
        .iter()
        .position(|name| name == "my tags")
        .unwrap();
    tags.choose_service(mine).unwrap();
    tags.enter("synthetic focus tag").unwrap();
    tags.apply().unwrap();
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(
        &ui,
        Pages::single(hydrus_gui::SearchPage::new(store.clone())),
    );
    ui.invoke_search_edited("system:everything".into());
    ui.invoke_search_accepted();
    let files = bound.current.borrow().borrow().results().to_vec();
    let index = files.iter().position(|file| *file == id).unwrap();
    ui.invoke_thumbnail_activated(i32::try_from(index).unwrap());
    let viewer = bound.viewer.borrow().as_ref().unwrap().clone_strong();
    let drawn = windows.get(windows.count() - 1).unwrap();
    headless::render(&drawn, 1000, 750);
    assert!(viewer.get_scanbar_shown());
    assert!(
        viewer.get_tags().row_count() > 0
            && viewer.get_notes().row_count() > 0
            && viewer.get_ratings().row_count() > 0
    );
    let focus = hydrus_gui::viewer_focus::NativeFocus::new(&viewer);
    let identity = slint::winit_030::winit::window::WindowId::from(9_001);
    focus.watch_id(identity);
    let activate = |active| {
        // Dispatch the native backend observer route used by ActivityHandler;
        // headless Slint also receives its corresponding native activation.
        hydrus_gui::session_autosave::observe_native_focus(identity, active);
        if !active {
            hydrus_gui::session_autosave::observe_native_focus(
                slint::winit_030::winit::window::WindowId::from(9_002),
                true,
            );
        }
        viewer
            .window()
            .dispatch_event(WindowEvent::WindowActiveChanged(active));
        headless::render(&drawn, 1000, 750);
        assert_eq!(viewer.get_window_active(), active);
    };
    let move_to = |x, y| {
        viewer.window().dispatch_event(WindowEvent::PointerMoved {
            position: LogicalPosition::new(x, y),
        });
        headless::render(&drawn, 1000, 750);
    };
    activate(true);
    let notes_y = (60..740)
        .step_by(10)
        .find(|&y| {
            move_to(980.0, y as f32);
            viewer.get_notes_showing()
        })
        .unwrap() as f32;
    let labels = [
        (
            "media viewer",
            "Seek bar full-height pop-in requires window focus:",
        ),
        (
            "media viewer hovers",
            "Hover window pop-in requires window focus:",
        ),
    ];
    for event in fixture["events"].as_array().unwrap() {
        open(&ui);
        let options = bound.options.borrow().as_ref().unwrap().clone_strong();
        for ((name, label), field) in labels
            .iter()
            .zip(["seek_requires_focus", "hover_requires_focus"])
        {
            show_page(&options, name);
            options.invoke_check_toggled(row(&options, label).0, event[field].as_bool().unwrap());
        }
        options.invoke_apply();
        activate(event["active"].as_bool().unwrap());
        let bottom = viewer.get_media_y() + viewer.get_media_height() - 2.0;
        move_to(
            viewer.get_media_x() + viewer.get_media_width() / 2.0,
            bottom,
        );
        assert_eq!(
            viewer.get_seek_bar_full(),
            event["seek_full"].as_bool().unwrap()
        );
        assert_eq!(
            viewer.get_seek_bar_height(),
            event["seek_height"].as_i64().unwrap() as f32
        );
        let states = [
            {
                move_to(500.0, 10.0);
                viewer.get_info_showing()
            },
            {
                move_to(20.0, 375.0);
                viewer.get_tags_showing()
            },
            {
                move_to(980.0, 15.0);
                viewer.get_ratings_showing()
            },
            {
                move_to(980.0, notes_y);
                viewer.get_notes_showing()
            },
        ];
        assert_eq!(serde_json::json!(states), event["hover_up"]);
    }
    for trace in fixture["transient"].as_array().unwrap() {
        let hover = trace["hover"].as_str().unwrap();
        let point = match hover {
            "top" => (500.0, 10.0),
            "tags" => (20.0, 375.0),
            "ratings" => (980.0, 15.0),
            "notes" => (980.0, notes_y),
            name => panic!("unrecorded hover {name}"),
        };
        let showing = || match hover {
            "top" => viewer.get_info_showing(),
            "tags" => viewer.get_tags_showing(),
            "ratings" => viewer.get_ratings_showing(),
            "notes" => viewer.get_notes_showing(),
            name => panic!("unrecorded hover {name}"),
        };
        move_to(500.0, 375.0);
        activate(true);
        move_to(point.0, point.1);
        let mut states = vec![showing()];
        hydrus_gui::session_autosave::observe_native_focus(identity, false);
        viewer
            .window()
            .dispatch_event(WindowEvent::WindowActiveChanged(false));
        headless::render(&drawn, 1000, 750);
        assert!(!viewer.get_window_active() && !viewer.get_another_window_active());
        states.push(showing());
        move_to(500.0, 375.0);
        states.push(showing());
        move_to(point.0, point.1);
        states.push(showing());
        activate(true);
        states.push(showing());
        activate(false);
        states.push(showing());
        let expected: Vec<_> = trace["states"]
            .as_array()
            .unwrap()
            .iter()
            .map(|state| state["up"].as_bool().unwrap())
            .collect();
        assert_eq!(
            states, expected,
            "{} active→None retains existing only→other hides",
            trace["hover"]
        );
    }
    // An inactive viewer can still finish a seek: an actual held scrub keeps
    // the bar full, independently of the mouseover focus requirement.
    activate(false);
    let position = LogicalPosition::new(
        viewer.get_media_x() + viewer.get_media_width() / 2.0,
        viewer.get_media_y() + viewer.get_media_height() - 2.0,
    );
    viewer
        .window()
        .dispatch_event(WindowEvent::PointerMoved { position });
    assert!(!viewer.get_seek_bar_full());
    viewer.window().dispatch_event(WindowEvent::PointerPressed {
        position,
        button: PointerEventButton::Left,
    });
    assert!(viewer.get_seek_bar_full());
    viewer
        .window()
        .dispatch_event(WindowEvent::PointerReleased {
            position,
            button: PointerEventButton::Left,
        });
    assert!(!viewer.get_seek_bar_full());
    let persisted = store.read(settings::get::<ViewerFocusSettings>).unwrap();
    assert_eq!(persisted, ViewerFocusSettings::default());
    open(&ui);
    let options = bound.options.borrow().as_ref().unwrap().clone_strong();
    for (name, label) in labels {
        show_page(&options, name);
        assert!(row(&options, label).1.checked);
        options.invoke_check_toggled(row(&options, label).0, false);
    }
    options.invoke_cancel();
    assert!(viewer.get_seek_requires_focus() && viewer.get_hovers_require_focus());
    assert_eq!(
        store.read(settings::get::<ViewerFocusSettings>).unwrap(),
        persisted
    );
    // Focus for other native identities and stale released observers do not
    // alter this viewer. The registry never extends the callback's lifetime.
    hydrus_gui::session_autosave::observe_native_focus(
        slint::winit_030::winit::window::WindowId::from(9_002),
        true,
    );
    assert!(!viewer.get_window_active());
    drop(focus);
    hydrus_gui::session_autosave::observe_native_focus(identity, true);
    assert!(!viewer.get_window_active());
    viewer.invoke_close_requested();
}

const CLOSING_LABELS: [&str; 4] = [
    "When closing the media viewer, re-select original search page: ",
    "When closing the media viewer, tell original search page to select exit media: ",
    "ADVANCED: When closing the media viewer with the above focusing options, activate Main GUI: ",
    "DEBUG: When closing the media viewer at any time, activate Main GUI: ",
];

fn closing_draft(ui: &MainWindow, bound: &hydrus_gui::Bound, values: &[bool]) -> OptionsWindow {
    open(ui);
    let window = bound.options.borrow().as_ref().unwrap().clone_strong();
    show_page(&window, "media viewer");
    for (label, &value) in CLOSING_LABELS.iter().zip(values) {
        window.invoke_check_toggled(row(&window, label).0, value);
    }
    window
}

fn show_original_key(ui: &MainWindow, bound: &hydrus_gui::Bound, key: hydrus_core::pages::PageKey) {
    let index = bound
        .pages
        .borrow()
        .session()
        .pages
        .iter()
        .position(|page| page.key == key)
        .unwrap();
    ui.invoke_tab_chosen(0, i32::try_from(index).unwrap());
    assert_eq!(bound.pages.borrow().shown().key, key);
}

#[test]
fn closing_preferences_reach_frozen_page_selection_and_native_main_activation() {
    use hydrus_gui::viewer_closing::set_activation_observer;
    use hydrus_gui_model::viewer_closing::Action;
    use std::{cell::RefCell, rc::Rc};
    let fixture = hydrus_testkit::fixture_json("viewer_closing_options.json");
    let (_dirs, store) = store();
    let ids: Vec<_> = fixture["hashes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|hash| {
            store
                .read(|conn| {
                    hydrus_store::master::hash_id(conn, &hash.as_str().unwrap().parse().unwrap())
                })
                .unwrap()
                .unwrap()
        })
        .collect();
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let location = hydrus_search::LocationContext::single(hydrus_core::ServiceKey::new(
        hydrus_core::service::builtin_keys::HYDRUS_LOCAL_FILE_STORAGE.to_vec(),
    ));
    let mut pages = Pages::single(hydrus_gui::SearchPage::new(store.clone()));
    pages.open_files(location.clone(), ids[..2].to_vec(), None, None);
    let original_key = pages.shown().key;
    let original = pages.current();
    pages.open_files(location, ids[..2].to_vec(), None, None);
    let other_key = pages.shown().key;
    let bound = bind(&ui, pages);
    ui.show().unwrap();
    let drawn = windows.get(0).unwrap();
    headless::render(&drawn, 900, 700);
    let activation = Rc::new(RefCell::new(Vec::new()));
    let weak_main = ui.as_weak();
    set_activation_observer(Some(Rc::new({
        let activation = activation.clone();
        move |main, reason| {
            let expected = weak_main.upgrade().unwrap();
            assert!(
                std::ptr::eq(main.window(), expected.window()),
                "activate the weak main owner"
            );
            activation.borrow_mut().push(match reason {
                Action::ActivateFocusing => "focusing-panel",
                Action::ActivateDebug => "debug-main",
                Action::ReselectPage | Action::SelectExitMedia => panic!("not an activation"),
            });
        }
    })));
    for event in fixture["events"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|event| event["scenario"] != "unowned")
    {
        let values: Vec<_> = event["values"]
            .as_array()
            .unwrap()
            .iter()
            .map(|value| value.as_bool().unwrap())
            .collect();
        closing_draft(&ui, &bound, &values).invoke_apply();
        open(&ui);
        let window = bound.options.borrow().as_ref().unwrap().clone_strong();
        show_page(&window, "media viewer");
        for (label, &expected) in CLOSING_LABELS.iter().zip(&values) {
            assert_eq!(
                row(&window, label).1.checked,
                expected,
                "persisted/reopened {label}"
            );
        }
        window.invoke_cancel();
        show_original_key(&ui, &bound, original_key);
        let missing = event["scenario"] == "missing-exit";
        if missing {
            original.borrow_mut().add_files(&ids[2..]);
        }
        let start = original
            .borrow()
            .results()
            .iter()
            .position(|file| *file == ids[0])
            .unwrap();
        original.borrow_mut().hit(None, false, false);
        original.borrow_mut().select(start);
        if event["scenario"] == "selected-multiple" {
            let exit = original
                .borrow()
                .results()
                .iter()
                .position(|file| *file == ids[1])
                .unwrap();
            original.borrow_mut().hit(Some(exit), true, false);
        }
        let viewer_files = original.borrow().files();
        let start_pos = viewer_files
            .iter()
            .position(|file| *file == ids[0])
            .unwrap();
        let exit = ids[if missing { 2 } else { 1 }];
        let exit_pos = viewer_files.iter().position(|file| *file == exit).unwrap();
        ui.invoke_thumbnail_activated(i32::try_from(start).unwrap());
        let viewer = bound.viewer.borrow().as_ref().unwrap().clone_strong();
        for _ in 0..(exit_pos + viewer_files.len() - start_pos) % viewer_files.len() {
            viewer.invoke_next();
        }
        if missing {
            original.borrow_mut().remove_files(&ids[2..]);
        }
        show_original_key(&ui, &bound, other_key);
        let closed = event["scenario"] == "closed-owner";
        if closed {
            bound
                .pages
                .borrow_mut()
                .close_tab_keys(&[original_key])
                .unwrap();
        }
        activation.borrow_mut().clear();
        let reveal_before = ui.get_viewer_reveal_request();
        viewer.invoke_close_requested();
        assert!(bound.viewer.borrow().is_none());
        let expected_key = if event["after"]["page"] == "original" {
            original_key
        } else {
            other_key
        };
        assert_eq!(bound.pages.borrow().shown().key, expected_key, "{event:?}");
        let hash_of = |id| {
            store
                .read(|conn| hydrus_store::master::hashes(conn, &[id]))
                .unwrap()[&id]
                .to_hex()
        };
        let selected: Vec<_> = original
            .borrow()
            .selected_files()
            .into_iter()
            .map(hash_of)
            .collect();
        assert_eq!(
            serde_json::json!(selected),
            event["after"]["selected"],
            "{event:?}"
        );
        let source = original.borrow();
        let focused = source
            .focused()
            .map(|index| hash_of(source.results()[index]));
        assert_eq!(
            serde_json::json!(focused),
            event["after"]["focused"],
            "{event:?}"
        );
        drop(source);
        assert_eq!(
            serde_json::json!(*activation.borrow()),
            event["after"]["activation"],
            "{event:?}"
        );
        if values[0] && values[1] && !closed && !missing {
            assert_ne!(ui.get_viewer_reveal_request(), reveal_before);
            let index = original
                .borrow()
                .results()
                .iter()
                .position(|file| *file == exit)
                .unwrap();
            assert_eq!(ui.get_viewer_reveal_index(), i32::try_from(index).unwrap());
        }
        if closed {
            assert!(bound.pages.borrow_mut().unclose());
            show_original_key(&ui, &bound, original_key);
            assert_eq!(
                original.borrow().selected_files(),
                [ids[1]],
                "undo preserves the hidden owner's exit selection"
            );
            assert_ne!(
                ui.get_viewer_reveal_request(),
                reveal_before,
                "pending exit scroll follows frozen owner on undo"
            );
        }
    }
    set_activation_observer(None);
    bound.pages.borrow_mut().save(1_700_000_000).unwrap();
    let mut reopened = Pages::open(store).unwrap();
    assert_eq!(
        reopened
            .page(&original_key)
            .unwrap()
            .borrow()
            .selected_files(),
        [ids[1]],
        "exit selection persists in the last session"
    );
}

#[test]
fn cancelled_closing_options_and_stale_viewers_cannot_redirect_the_live_owner() {
    use hydrus_gui::viewer_closing::set_activation_observer;
    use std::{cell::RefCell, rc::Rc};
    let (_dirs, store) = store();
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(
        &ui,
        Pages::single(hydrus_gui::SearchPage::new(store.clone())),
    );
    ui.invoke_search_edited("system:everything".into());
    ui.invoke_search_accepted();
    let files = bound.current.borrow().borrow().results().to_vec();
    assert!(files.len() > 2);
    closing_draft(&ui, &bound, &[true, false, true, true]).invoke_apply();
    ui.invoke_thumbnail_activated(0);
    let stale = bound.viewer.borrow().as_ref().unwrap().clone_strong();
    ui.invoke_thumbnail_activated(1);
    let live = bound.viewer.borrow().as_ref().unwrap().clone_strong();
    let calls = Rc::new(RefCell::new(Vec::new()));
    set_activation_observer(Some(Rc::new({
        let calls = calls.clone();
        move |_, action| calls.borrow_mut().push(action)
    })));
    stale.invoke_close_requested();
    assert!(
        calls.borrow().is_empty(),
        "a stale callback does not activate or close its successor"
    );
    assert!(std::ptr::eq(
        bound.viewer.borrow().as_ref().unwrap().window(),
        live.window()
    ));
    closing_draft(&ui, &bound, &[false, true, false, false]).invoke_cancel();
    live.invoke_close_requested();
    assert_eq!(
        calls.borrow().as_slice(),
        [
            hydrus_gui_model::viewer_closing::Action::ActivateFocusing,
            hydrus_gui_model::viewer_closing::Action::ActivateDebug
        ]
    );
    assert!(bound.viewer.borrow().is_none());
    assert!(
        store
            .read(hydrus_gui::options::Settings::load)
            .unwrap()
            .viewer_closing
            .reselect_page
    );
    // A session replacement destroys the original panel. The viewer's weak
    // owner must not select the replacement page or request focusing activation.
    closing_draft(&ui, &bound, &[true, true, true, true]).invoke_apply();
    bound
        .pages
        .borrow_mut()
        .save_session("closing replacement", 1_700_000_000)
        .unwrap();
    ui.invoke_thumbnail_activated(0);
    let viewer = bound.viewer.borrow().as_ref().unwrap().clone_strong();
    let old_page = std::rc::Rc::downgrade(&bound.current.borrow());
    bound
        .pages
        .borrow_mut()
        .clear_and_load("closing replacement")
        .unwrap();
    ui.invoke_tab_chosen(0, 0);
    assert!(
        old_page.upgrade().is_none(),
        "the original page is destroyed after session replacement"
    );
    let replacement = bound.current.borrow().clone();
    let before = replacement.borrow().selected_files();
    calls.borrow_mut().clear();
    viewer.invoke_close_requested();
    assert_eq!(replacement.borrow().selected_files(), before);
    assert_eq!(
        calls.borrow().as_slice(),
        [hydrus_gui_model::viewer_closing::Action::ActivateDebug]
    );
    set_activation_observer(None);
}

#[test]
#[allow(clippy::float_cmp)] // logical background origin is exact zero or exact layout height
fn passive_background_options_paint_independent_copies_behind_opaque_media() {
    use hydrus_store::settings::{self, ViewerBackgroundSettings};
    let fixture = hydrus_testkit::fixture_json("viewer_background_options.json");
    let (_dirs, store) = store();
    let id = store
        .read(|conn| {
            hydrus_store::master::hash_id(conn, &fixture["hash"].as_str().unwrap().parse().unwrap())
        })
        .unwrap()
        .unwrap();
    for (name, text) in fixture["notes"].as_object().unwrap() {
        let (name, text) = (name.clone(), text.as_str().unwrap().to_owned());
        store
            .write_content(move |writer| writer.set_note(id, &name, &text))
            .unwrap();
    }
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let mut pages = Pages::single(hydrus_gui::SearchPage::new(store.clone()));
    pages.open_files(
        hydrus_search::LocationContext::single(hydrus_core::ServiceKey::new(
            hydrus_core::service::builtin_keys::HYDRUS_LOCAL_FILE_STORAGE.to_vec(),
        )),
        vec![id],
        None,
        None,
    );
    let bound = bind(&ui, pages);
    ui.invoke_thumbnail_activated(0);
    let viewer = bound.viewer.borrow().as_ref().unwrap().clone_strong();
    let drawn = windows.get(windows.count() - 1).unwrap();
    assert!(viewer.get_tags().row_count() > 0);
    assert!(viewer.get_ratings().row_count() > 0);
    assert_eq!(viewer.get_notes().row_count(), 2);
    assert!(!viewer.get_info_line().is_empty());
    assert!(
        viewer.get_location_strings().row_count() > 0,
        "real file domain names join ratings and URLs"
    );
    headless::render(&drawn, 1000, 750); // settle initial canvas resize before isolating its background
    let labels = [
        "Draw tags (left) in the viewer background:",
        "Draw file information (top) in the viewer background:",
        "Draw ratings and locations (top-right) in the viewer background:",
        "Draw notes (right) in the viewer background:",
    ];
    let popup_labels = [
        "Pop-in tags (left) hover window on mouseover:",
        "Pop-in ratings and locations (top-right) hover window on mouseover:",
        "Pop-in notes (right) hover window on mouseover:",
    ];
    let occupancy = |pixels: &[u8]| {
        pixels
            .chunks_exact(4)
            .filter(|pixel| pixel[..3] != [32, 32, 32])
            .count()
    };
    for event in fixture["events"].as_array().unwrap() {
        open(&ui);
        let options = bound.options.borrow().as_ref().unwrap().clone_strong();
        show_page(&options, "media viewer hovers");
        for (label, value) in labels.iter().zip(event["values"].as_array().unwrap()) {
            options.invoke_check_toggled(row(&options, label).0, value.as_bool().unwrap());
        }
        for label in popup_labels {
            options.invoke_check_toggled(
                row(&options, label).0,
                event["hovers_enabled"].as_bool().unwrap(),
            );
        }
        options.invoke_check_toggled(
            row(
                &options,
                "Draw index text (bottom-right) in the viewer background:",
            )
            .0,
            false,
        );
        options.invoke_apply();
        let saved = store
            .read(settings::get::<ViewerBackgroundSettings>)
            .unwrap();
        assert_eq!(
            serde_json::json!([saved.tags, saved.information, saved.ratings, saved.notes]),
            event["values"]
        );
        assert_eq!(
            [
                viewer.get_draw_tags_background(),
                viewer.get_draw_information_background(),
                viewer.get_draw_ratings_background(),
                viewer.get_draw_notes_background()
            ],
            [saved.tags, saved.information, saved.ratings, saved.notes]
        );
        viewer
            .window()
            .dispatch_event(slint::platform::WindowEvent::PointerExited);
        viewer.set_media(slint::Image::default());
        viewer.set_sharp_shown(false);
        viewer.set_sharp(slint::Image::default());
        viewer.set_media_x(0.0);
        viewer.set_media_y(0.0);
        viewer.set_media_width(1000.0);
        viewer.set_media_height(750.0);
        let pixels = headless::render_snapshot(&drawn, 1000, 750);
        assert_eq!(
            occupancy(&pixels) > 0,
            event["visible_pixels"].as_u64().unwrap() > 0,
            "{event:?}"
        );
        assert!(
            !viewer.get_tags_showing()
                && !viewer.get_ratings_showing()
                && !viewer.get_notes_showing()
        );
        assert_eq!(
            viewer.get_background_notes_y(),
            if saved.ratings {
                viewer.get_background_ratings_height()
            } else {
                0.0
            }
        );
        if saved.ratings {
            assert!(viewer.get_background_notes_y() > 0.0);
        }
        let calls = event["calls"].as_array().unwrap();
        if let Some(notes) = calls.iter().find(|call| call["kind"] == "Notes") {
            assert_eq!(notes["input_y"].as_u64().unwrap() > 0, saved.ratings);
        }
        // Opaque media painted after the passive text covers all of it. An
        // empty image above isolated the real metadata/preferences' drawing.
        let mut cover = slint::SharedPixelBuffer::<slint::Rgba8Pixel>::new(4, 3);
        cover.make_mut_slice().fill(slint::Rgba8Pixel {
            r: 32,
            g: 32,
            b: 32,
            a: 255,
        });
        viewer.set_media(slint::Image::from_rgba8(cover));
        let pixels = headless::render_snapshot(&drawn, 1000, 750);
        assert_eq!(
            occupancy(&pixels),
            event["opaque_cover_pixels"].as_u64().unwrap() as usize,
            "media covers passive copies: {event:?}"
        );
    }
    open(&ui);
    let options = bound.options.borrow().as_ref().unwrap().clone_strong();
    show_page(&options, "media viewer hovers");
    for label in labels {
        assert!(row(&options, label).1.checked);
        options.invoke_check_toggled(row(&options, label).0, false);
    }
    options.invoke_cancel();
    assert!(
        viewer.get_draw_tags_background()
            && viewer.get_draw_information_background()
            && viewer.get_draw_ratings_background()
            && viewer.get_draw_notes_background()
    );
    viewer.invoke_close_requested();
    ui.invoke_thumbnail_activated(0);
    let reopened = bound.viewer.borrow().as_ref().unwrap().clone_strong();
    assert!(
        reopened.get_draw_tags_background()
            && reopened.get_draw_information_background()
            && reopened.get_draw_ratings_background()
            && reopened.get_draw_notes_background()
    );
    reopened.invoke_close_requested();
}

#[test]
fn subscription_failure_limit_options_replay_qt_values_apply_cancel_and_reopen() {
    use hydrus_store::network::NetworkSettings;
    let (_dirs, store) = store();
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    let fixture = hydrus_testkit::fixture_json("subscription_failure_limit.json");
    let label = "If a subscription has this many failed file imports, stop and continue later:";
    let saved = || {
        store
            .read(hydrus_store::settings::get::<NetworkSettings>)
            .unwrap()
            .subscription_file_error_cancel_threshold
    };
    for state in fixture["options"]["states"].as_array().unwrap() {
        open(&ui);
        let window = bound.options.borrow().as_ref().unwrap().clone_strong();
        show_page(&window, "downloading");
        let (at, control) = row(&window, label);
        assert_eq!(control.kind, 3);
        assert_eq!(
            serde_json::json!(control.minimum),
            fixture["options"]["minimum"]
        );
        assert_eq!(
            serde_json::json!(control.maximum),
            fixture["options"]["maximum"]
        );
        assert_eq!(
            control.none_phrase.as_str(),
            fixture["options"]["none_phrase"].as_str().unwrap()
        );
        assert_eq!(
            control.unit.as_str(),
            fixture["options"]["unit"].as_str().unwrap()
        );
        assert_eq!(serde_json::json!(saved()), state["saved_before"]);
        let value = state["given"].as_i64();
        window.invoke_none_toggled(at, value.is_none());
        if let Some(value) = value {
            window.invoke_number_edited(at, i32::try_from(value).unwrap());
        }
        show_page(&window, "downloading");
        let (_, control) = row(&window, label);
        assert_eq!(control.is_none, value.is_none());
        if let Some(value) = value {
            assert_eq!(i64::from(control.number), value);
        }
        assert_eq!(
            serde_json::json!(saved()),
            state["saved_before"],
            "edits are staged"
        );
        window.invoke_apply();
        assert_eq!(serde_json::json!(saved()), state["saved_after"]);
    }
    open(&ui);
    let window = bound.options.borrow().as_ref().unwrap().clone_strong();
    show_page(&window, "downloading");
    let (at, control) = row(&window, label);
    assert_eq!(control.number, 1_000_000);
    window.invoke_none_toggled(at, true);
    window.invoke_cancel();
    window.invoke_number_edited(at, 1);
    window.invoke_apply();
    assert_eq!(
        saved(),
        Some(1_000_000),
        "closed owner cannot apply a stale edit"
    );
    open(&ui);
    let window = bound.options.borrow().as_ref().unwrap().clone_strong();
    show_page(&window, "downloading");
    assert!(!row(&window, label).1.is_none);
    let pixels = headless::render(&windows.get(windows.count() - 1).unwrap(), 820, 650);
    headless::save_png(
        &std::path::Path::new(env!("CARGO_TARGET_TMPDIR"))
            .join("options_subscription_failure_limit.png"),
        &pixels,
        820,
        650,
    )
    .unwrap();
    window.invoke_cancel();
}

#[test]
fn cursor_timeout_reaches_native_motion_timer_focus_and_actual_popup_lifecycle() {
    use hydrus_gui::{session_autosave, viewer_cursor};
    use slint::{
        LogicalPosition,
        platform::{PointerEventButton, WindowEvent},
        winit_030::winit::window::WindowId,
    };
    let fixture = hydrus_testkit::fixture_json("viewer_cursor_options.json");
    let (_dirs, store) = store();
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(
        &ui,
        Pages::single(hydrus_gui::SearchPage::new(store.clone())),
    );
    ui.invoke_search_edited("system:everything".into());
    ui.invoke_search_accepted();
    let files = bound.current.borrow().borrow().results().to_vec();
    let reference_id = store
        .read(|conn| {
            hydrus_store::master::hash_id(conn, &fixture["hash"].as_str().unwrap().parse().unwrap())
        })
        .unwrap()
        .unwrap();
    let index = files.iter().position(|file| *file == reference_id).unwrap();
    ui.invoke_thumbnail_activated(i32::try_from(index).unwrap());
    let viewer = bound.viewer.borrow().as_ref().unwrap().clone_strong();
    let drawn = windows.get(windows.count() - 1).unwrap();
    headless::render(&drawn, 1000, 750);
    let cursor = viewer_cursor::last_opened().unwrap();
    let id = WindowId::from(73001);
    let other = WindowId::from(73002);
    cursor.watch_id(id);
    let focus = hydrus_gui::viewer_focus::NativeFocus::new(&viewer);
    focus.watch_id(id);
    let move_to = |x: f32, y: f32| {
        session_autosave::observe_native_pointer(id, (f64::from(x), f64::from(y)));
        viewer.window().dispatch_event(WindowEvent::PointerMoved {
            position: LogicalPosition::new(x, y),
        });
    };
    let label = "Time until mouse cursor autohides on media viewer:";
    for (index, event) in fixture["events"].as_array().unwrap().iter().enumerate() {
        open(&ui);
        let options = bound.options.borrow().as_ref().unwrap().clone_strong();
        show_page(&options, "media viewer");
        let row_index = row(&options, label).0;
        options.invoke_none_toggled(row_index, event["value"].is_null());
        if let Some(value) = event["value"].as_i64() {
            options.invoke_number_edited(row_index, i32::try_from(value).unwrap());
        }
        options.invoke_apply();
        session_autosave::observe_native_focus(id, true);
        move_to(400.0 + index as f32 * 20.0, 500.0);
        assert!(viewer.get_cursor_hide_eligible());
        let origin = cursor.state().touched_ms;
        let delay = event["value"].as_u64().unwrap_or(700);
        cursor.check_at(origin + delay);
        assert!(
            !viewer.get_cursor_idle_hidden(),
            "strict threshold {event:?}"
        );
        cursor.check_at(origin + delay + 1);
        assert_eq!(viewer.get_cursor_idle_hidden(), !event["value"].is_null());
        let expected_interval = event["trace"][2]["interval_ms"].as_u64().unwrap();
        assert_eq!(
            cursor.state().next_check_ms.map(u64::from),
            Some(expected_interval)
        );
        let before_other = cursor.state();
        session_autosave::observe_native_pointer(other, (999.0, 600.0));
        assert_eq!(
            cursor.state(),
            before_other,
            "unrelated native windows cannot reset the viewer clock"
        );
        session_autosave::observe_native_focus(other, true);
        cursor.check_at(origin + delay + 2);
        assert!(!viewer.get_cursor_idle_hidden());
        session_autosave::observe_native_focus(id, true);
        move_to(20.0, 100.0);
        assert!(viewer.get_tags_showing());
        assert!(!viewer.get_cursor_hide_eligible());
        cursor.check_at(origin + delay * 2 + 4);
        assert!(
            !viewer.get_cursor_idle_hidden(),
            "popup controls retain an ordinary pointer"
        );
    }
    open(&ui);
    let options = bound.options.borrow().as_ref().unwrap().clone_strong();
    show_page(&options, "media viewer");
    options.invoke_number_edited(row(&options, label).0, 700);
    options.invoke_apply();
    session_autosave::observe_native_focus(id, true);
    move_to(530.0, 500.0);
    let origin = cursor.state().touched_ms;
    cursor.check_at(origin + 701);
    assert!(viewer.get_cursor_idle_hidden());
    viewer.window().dispatch_event(WindowEvent::PointerPressed {
        position: LogicalPosition::new(530.0, 500.0),
        button: PointerEventButton::Right,
    });
    let inner = slint::private_unstable_api::re_exports::WindowInner::from_pub(viewer.window());
    assert!(
        !inner.active_popups().is_empty(),
        "the actual viewer context menu is open"
    );
    cursor.check_at(origin + 800);
    assert!(
        !viewer.get_cursor_idle_hidden(),
        "the actual popup stack suppresses hiding"
    );
    assert_eq!(cursor.state().touched_ms, origin + 800);
    inner.close_all_popups();
    assert!(inner.active_popups().is_empty());
    viewer
        .window()
        .dispatch_event(WindowEvent::PointerReleased {
            position: LogicalPosition::new(530.0, 500.0),
            button: PointerEventButton::Right,
        });
    cursor.check_at(origin + 1501);
    assert!(viewer.get_cursor_idle_hidden());
    move_to(540.0, 500.0);
    assert!(!viewer.get_cursor_idle_hidden());
    assert_eq!(cursor.state().next_check_ms, Some(100));
    open(&ui);
    let options = bound.options.borrow().as_ref().unwrap().clone_strong();
    show_page(&options, "media viewer");
    options.invoke_check_toggled(
        row(&options, "Hide mouse cursor during media viewer drags:").0,
        true,
    );
    options.invoke_apply();
    viewer.window().dispatch_event(WindowEvent::PointerPressed {
        position: LogicalPosition::new(540.0, 500.0),
        button: PointerEventButton::Left,
    });
    move_to(550.0, 505.0);
    assert!(viewer.get_cursor_idle_hidden());
    assert_eq!(
        cursor.state().next_check_ms,
        None,
        "accepted hidden drag stops the inactivity timer"
    );
    assert!(!cursor.timer_running());
    viewer
        .window()
        .dispatch_event(WindowEvent::PointerReleased {
            position: LogicalPosition::new(550.0, 505.0),
            button: PointerEventButton::Left,
        });
    move_to(560.0, 510.0);
    assert!(!viewer.get_cursor_idle_hidden());
    assert_eq!(cursor.state().next_check_ms, Some(100));
    open(&ui);
    let options = bound.options.borrow().as_ref().unwrap().clone_strong();
    show_page(&options, "media viewer");
    assert_eq!(row(&options, label).1.number, 700);
    options.invoke_none_toggled(row(&options, label).0, true);
    options.invoke_cancel();
    assert_eq!(
        store
            .read(hydrus_store::settings::get::<hydrus_store::settings::ViewerCursorSettings>)
            .unwrap()
            .autohide_ms,
        Some(700)
    );
    // A synchronous platform menu returns with no Slint popup stack. Exercise
    // its real callback path after a long elapsed menu interval: return must
    // establish a fresh wait rather than immediately hide the cursor.
    viewer.invoke_cursor_menu_starting();
    let before_menu = cursor.state().touched_ms;
    cursor.check_at(before_menu + 10_000);
    assert!(viewer.get_cursor_idle_hidden());
    viewer.invoke_cursor_menu_returned();
    assert!(!viewer.get_cursor_idle_hidden());
    let after_menu = cursor.state().touched_ms;
    cursor.check_at(after_menu + 1);
    assert!(!viewer.get_cursor_idle_hidden());
    viewer.invoke_close_requested();
    assert!(
        !cursor.timer_running(),
        "close actually stops the native timer"
    );
    let stopped = cursor.state();
    session_autosave::observe_native_pointer(id, (700.0, 600.0));
    cursor.check_at(u64::MAX);
    assert_eq!(
        cursor.state(),
        stopped,
        "close stops timers and ignores late native input"
    );
}

#[test]
fn subscription_concurrency_options_replay_bounds_parent_apply_cancel_and_reopen() {
    use hydrus_store::network::NetworkSettings;
    let (_dirs, store) = store();
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    let fixture = hydrus_testkit::fixture_json("subscription_concurrency.json");
    let label = "Maximum number of subscriptions that can sync simultaneously:";
    let saved = || {
        store
            .read(hydrus_store::settings::get::<NetworkSettings>)
            .unwrap()
            .max_simultaneous_subscriptions
    };
    for state in fixture["options"]["states"].as_array().unwrap() {
        open(&ui);
        let window = bound.options.borrow().as_ref().unwrap().clone_strong();
        show_page(&window, "downloading");
        let (at, control) = row(&window, label);
        assert_eq!(control.kind, 2);
        assert_eq!(
            serde_json::json!(control.minimum),
            fixture["options"]["minimum"]
        );
        assert_eq!(
            serde_json::json!(control.maximum),
            fixture["options"]["maximum"]
        );
        assert_eq!(serde_json::json!(saved()), state["saved_before"]);
        // SpinBox emits its range-clamped value. The callback bypasses the
        // widget here, so drive its recorded edit through those same bounds.
        let given = i32::try_from(state["given"].as_i64().unwrap()).unwrap();
        window.invoke_number_edited(at, given.clamp(control.minimum, control.maximum));
        show_page(&window, "downloading");
        assert_eq!(
            serde_json::json!(row(&window, label).1.number),
            state["value"]
        );
        assert_eq!(serde_json::json!(saved()), state["saved_before"]);
        window.invoke_apply();
        assert_eq!(serde_json::json!(saved()), state["saved_after"]);
    }
    open(&ui);
    let window = bound.options.borrow().as_ref().unwrap().clone_strong();
    show_page(&window, "downloading");
    let (at, control) = row(&window, label);
    assert_eq!(control.number, 3);
    window.invoke_number_edited(at, 99);
    window.invoke_cancel();
    window.invoke_apply();
    assert_eq!(saved(), 3, "cancelled owner cannot apply stale values");
    open(&ui);
    let window = bound.options.borrow().as_ref().unwrap().clone_strong();
    show_page(&window, "downloading");
    assert_eq!(row(&window, label).1.number, 3);
    let pixels = headless::render(&windows.get(windows.count() - 1).unwrap(), 820, 650);
    headless::save_png(
        &std::path::Path::new(env!("CARGO_TARGET_TMPDIR"))
            .join("options_subscription_concurrency.png"),
        &pixels,
        820,
        650,
    )
    .unwrap();
    window.invoke_cancel();
}

#[test]
#[allow(clippy::float_cmp)] // recorded whole logical pixels and frame proportions
fn hover_zoom_and_loop_options_reach_owned_viewers_and_survive_cancel_and_reopen() {
    use hydrus_store::settings::{self, ViewerPlaybackSettings};
    use std::time::{Duration, Instant};
    fn wait_until(done: impl Fn() -> bool) {
        let started = Instant::now();
        while !done() && started.elapsed() < Duration::from_secs(10) {
            slint::platform::update_timers_and_animations();
            std::thread::sleep(Duration::from_millis(2));
        }
        assert!(done(), "native playback did not reach the expected frame");
    }
    let fixture = hydrus_testkit::fixture_json("viewer_zoom_loop_options.json");
    let (_dirs, store) = store();
    let sources = tempfile::tempdir().unwrap();
    let jpeg_path = sources.path().join("synthetic-large.jpg");
    let raster = hydrus_media::Raster::new(2000, 1600, 3, vec![31; 2000 * 1600 * 3]).unwrap();
    std::fs::write(
        &jpeg_path,
        hydrus_media::encode::encode_render(&raster, hydrus_media::encode::RenderFormat::Jpeg, 90)
            .unwrap(),
    )
    .unwrap();
    let mut animation = hex::decode(fixture["animation_bytes"].as_str().unwrap()).unwrap();
    let offset = animation
        .windows(4)
        .position(|bytes| bytes == b"ANIM")
        .unwrap()
        + 12;
    animation[offset..offset + 2].copy_from_slice(&1_u16.to_le_bytes());
    let animation_path = sources.path().join("one-play.webp");
    std::fs::write(&animation_path, animation).unwrap();
    let importer = hydrus_import::FileImporter::new(store.clone(), hydrus_media::MediaTools::new());
    let file_id = |path: &std::path::Path| {
        let hash = importer
            .import_path(path, &hydrus_import::FileImportOptions::default())
            .unwrap()
            .hash
            .unwrap();
        store
            .read(|conn| hydrus_store::master::hash_id(conn, &hash))
            .unwrap()
            .unwrap()
    };
    let jpeg = file_id(&jpeg_path);
    let animation = file_id(&animation_path);
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(
        &ui,
        Pages::single(hydrus_gui::SearchPage::new(store.clone())),
    );
    ui.invoke_search_edited("system:everything".into());
    ui.invoke_search_accepted();
    let files = bound.current.borrow().borrow().results().to_vec();
    let jpeg_index = i32::try_from(files.iter().position(|id| *id == jpeg).unwrap()).unwrap();
    let animation_index =
        i32::try_from(files.iter().position(|id| *id == animation).unwrap()).unwrap();
    let zoom_label = "Zoom switch button switches between:";
    let loop_label = "Always Loop Animations:";
    for choice in 0..4 {
        open(&ui);
        let options = bound.options.borrow().as_ref().unwrap().clone_strong();
        show_page(&options, "media viewer hovers");
        let (number, control) = row(&options, zoom_label);
        assert_eq!(
            control.items.row_count(),
            fixture["initial"]["choices"].as_array().unwrap().len()
        );
        options.invoke_choice_chosen(number, choice);
        options.invoke_cancel();
        let previous = store
            .read(settings::get::<ViewerPlaybackSettings>)
            .unwrap()
            .zoom_switch;
        assert_eq!(
            previous,
            if choice == 0 {
                0
            } else {
                usize::try_from(choice - 1).unwrap()
            }
        );
        open(&ui);
        let options = bound.options.borrow().as_ref().unwrap().clone_strong();
        show_page(&options, "media viewer hovers");
        options.invoke_choice_chosen(row(&options, zoom_label).0, choice);
        options.invoke_apply();
        assert_eq!(
            store
                .read(settings::get::<ViewerPlaybackSettings>)
                .unwrap()
                .zoom_switch,
            choice as usize
        );
        ui.invoke_thumbnail_activated(jpeg_index);
        let viewer = bound.viewer.borrow().as_ref().unwrap().clone_strong();
        let drawn = windows.get(windows.count() - 1).unwrap();
        headless::render(&drawn, 1000, 600);
        viewer.invoke_zoom(0, false, 0.0, 0.0); // initial canvas fit -> 100%
        viewer.invoke_drag(37.0, 19.0);
        let case = fixture["zoom"]
            .as_array()
            .unwrap()
            .iter()
            .find(|case| {
                case["choice"] == choice
                    && case["centre"] == 2
                    && case["name"] == "synthetic-large.jpg"
            })
            .unwrap();
        for expected in case["states"].as_array().unwrap() {
            viewer.invoke_zoom_switch_requested(123.0, 145.0);
            assert_eq!(
                serde_json::json!([
                    viewer.get_media_x() as i32,
                    viewer.get_media_y() as i32,
                    viewer.get_media_width() as i32,
                    viewer.get_media_height() as i32,
                ]),
                expected["rect"],
                "{case:?}"
            );
        }
        // A cancelled change cannot alter the command captured by this owner.
        open(&ui);
        let options = bound.options.borrow().as_ref().unwrap().clone_strong();
        show_page(&options, "media viewer hovers");
        options.invoke_choice_chosen(row(&options, zoom_label).0, (choice + 1) % 4);
        options.invoke_cancel();
        viewer.invoke_close_requested();
    }
    open(&ui);
    let options = bound.options.borrow().as_ref().unwrap().clone_strong();
    show_page(&options, "media playback");
    options.invoke_check_toggled(row(&options, loop_label).0, false);
    options.invoke_apply();
    ui.invoke_thumbnail_activated(animation_index);
    let viewer = bound.viewer.borrow().as_ref().unwrap().clone_strong();
    let drawn = windows.get(windows.count() - 1).unwrap();
    headless::render(&drawn, 1000, 750);
    let last_frame = fixture["loops"]
        .as_array()
        .unwrap()
        .iter()
        .find(|case| case["count"] == 1 && case["always"] == false)
        .unwrap()["frames"]
        .as_u64()
        .unwrap();
    let prefix = format!("{last_frame}/{last_frame} - ");
    wait_until(|| viewer.get_scanbar_text().starts_with(&prefix));
    let last = viewer.get_scanbar_text();
    // After two whole plays' worth of time, the finite animation remains at
    // its final frame. The actual viewer's scanbar follows its decoded player.
    let total_ms = hydrus_media::animation::Frames::open(
        &animation_path,
        hydrus_core::Mime::AnimationWebp,
        &[],
        None,
    )
    .unwrap()
    .total_ms();
    let started = Instant::now();
    while started.elapsed() < Duration::from_millis(total_ms * 2 + 100) {
        slint::platform::update_timers_and_animations();
        std::thread::sleep(Duration::from_millis(2));
        assert_eq!(viewer.get_scanbar_text(), last);
    }
    assert_eq!(viewer.get_scanbar_progress(), 1.0);
    open(&ui);
    let options = bound.options.borrow().as_ref().unwrap().clone_strong();
    show_page(&options, "media playback");
    assert!(!row(&options, loop_label).1.checked);
    options.invoke_check_toggled(row(&options, loop_label).0, true);
    options.invoke_cancel();
    assert!(
        !store
            .read(settings::get::<ViewerPlaybackSettings>)
            .unwrap()
            .always_loop
    );
    open(&ui);
    let options = bound.options.borrow().as_ref().unwrap().clone_strong();
    show_page(&options, "media playback");
    options.invoke_check_toggled(row(&options, loop_label).0, true);
    options.invoke_apply();
    viewer.invoke_toggle_pause(); // enabling looping does not resume paused media
    wait_until(|| viewer.get_scanbar_progress() == 0.0);
    wait_until(|| viewer.get_scanbar_progress() == 1.0);
    wait_until(|| viewer.get_scanbar_progress() == 0.0);
    viewer.invoke_close_requested();
    // Stale owner callbacks cannot close the freshly opened successor.
    ui.invoke_thumbnail_activated(animation_index);
    let successor = bound.viewer.borrow().as_ref().unwrap().clone_strong();
    viewer.invoke_close_requested();
    assert!(bound.viewer.borrow().is_some());
    successor.invoke_close_requested();
    open(&ui);
    let options = bound.options.borrow().as_ref().unwrap().clone_strong();
    show_page(&options, "media viewer hovers");
    assert_eq!(row(&options, zoom_label).1.index, 3);
    show_page(&options, "media playback");
    assert!(row(&options, loop_label).1.checked);
    options.invoke_cancel();
}

#[test]
fn default_export_directory_browse_apply_cancel_and_manual_open_use_shared_preference() {
    use hydrus_core::HashId;
    use hydrus_gui::{Pick, export_files_window, set_picker};
    use hydrus_gui_model::export_files::{self, Preferences};
    use hydrus_store::settings::{self, ExportSettings};
    use std::{cell::RefCell, rc::Rc};
    let reference = hydrus_testkit::fixture_json("export_default_directory.json");
    let (_dirs, store) = store();
    let rendered = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    let before: ExportSettings = store.read(settings::get).unwrap();
    assert!(before.default_directory.is_none());
    let fallback = export_files::default_directory(&store, &before);
    assert!(!fallback.is_empty());
    let old_destination = store
        .dir()
        .join("previous manual export")
        .to_string_lossy()
        .into_owned();
    store
        .write(move |ctx| {
            settings::set(
                ctx.conn(),
                &Preferences {
                    destination: old_destination,
                    ..Preferences::default()
                },
            )
        })
        .unwrap();
    let fallback_window =
        export_files_window::open(&store, vec![HashId(1)], &bound.export_files, Rc::new(|| {}))
            .unwrap();
    assert_eq!(fallback_window.get_destination(), fallback);
    assert!(std::path::Path::new(&fallback).is_dir());
    fallback_window.invoke_dismissed();
    let selected = store.dir().join("synthetic exports 日本");
    let picker_calls = Rc::new(RefCell::new(Vec::new()));
    set_picker({
        let calls = picker_calls.clone();
        let selected = selected.clone();
        move |kind, caption| {
            calls.borrow_mut().push((kind, caption.to_owned()));
            vec![selected.clone()]
        }
    });
    open(&ui);
    let window = bound.options.borrow().as_ref().unwrap().clone_strong();
    show_page(&window, "exporting");
    let (index, shown) = row(&window, reference["label"].as_str().unwrap());
    assert_eq!(shown.kind, 20);
    assert_eq!(shown.text, "");
    window.invoke_directory_browse(index);
    assert_eq!(
        picker_calls.borrow()[0],
        (
            Pick::Folder,
            reference["browse"][0]["caption"]
                .as_str()
                .unwrap()
                .to_owned()
        )
    );
    assert_eq!(
        row(&window, reference["label"].as_str().unwrap()).1.text,
        selected.to_string_lossy()
    );
    assert_eq!(store.read(settings::get::<ExportSettings>).unwrap(), before);
    set_picker(|_, _| Vec::new());
    window.invoke_directory_browse(index);
    assert_eq!(
        row(&window, reference["label"].as_str().unwrap()).1.text,
        selected.to_string_lossy()
    );
    let pixels = headless::render(&rendered.get(rendered.count() - 1).unwrap(), 950, 660);
    headless::save_png(
        &std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("export-default-directory.png"),
        &pixels,
        950,
        660,
    )
    .unwrap();
    window.invoke_cancel();
    assert_eq!(store.read(settings::get::<ExportSettings>).unwrap(), before);
    let calls_before = picker_calls.borrow().len();
    window.invoke_directory_browse(index);
    window.invoke_apply();
    assert_eq!(picker_calls.borrow().len(), calls_before);
    assert_eq!(store.read(settings::get::<ExportSettings>).unwrap(), before);
    open(&ui);
    let window = bound.options.borrow().as_ref().unwrap().clone_strong();
    show_page(&window, "exporting");
    let (index, _) = row(&window, reference["label"].as_str().unwrap());
    window.invoke_text_edited(index, selected.to_string_lossy().as_ref().into());
    window.invoke_apply();
    let saved: ExportSettings = store.read(settings::get).unwrap();
    assert_eq!(saved.default_directory.as_deref(), selected.to_str());
    assert_eq!(saved.phrase, before.phrase);
    let export =
        export_files_window::open(&store, vec![HashId(1)], &bound.export_files, Rc::new(|| {}))
            .unwrap();
    assert_eq!(
        export.get_destination(),
        selected.to_string_lossy().as_ref()
    );
    assert!(export.get_rows().row_count() > 0);
    assert!(
        export
            .get_rows()
            .row_data(0)
            .unwrap()
            .cells
            .row_data(2)
            .unwrap()
            .starts_with(selected.to_str().unwrap())
    );
    export.set_destination(
        store
            .dir()
            .join("one off destination")
            .to_string_lossy()
            .as_ref()
            .into(),
    );
    export.invoke_dismissed();
    let export =
        export_files_window::open(&store, vec![HashId(1)], &bound.export_files, Rc::new(|| {}))
            .unwrap();
    assert_eq!(
        export.get_destination(),
        selected.to_string_lossy().as_ref()
    );
    export.invoke_dismissed();
    open(&ui);
    let window = bound.options.borrow().as_ref().unwrap().clone_strong();
    show_page(&window, "exporting");
    let (index, shown) = row(&window, reference["label"].as_str().unwrap());
    assert_eq!(shown.text, selected.to_string_lossy().as_ref());
    window.invoke_text_edited(index, "relative exports".into());
    window.invoke_apply();
    let relative = store.dir().join("relative exports");
    assert_eq!(
        store
            .read(settings::get::<ExportSettings>)
            .unwrap()
            .default_directory
            .as_deref(),
        relative.to_str()
    );
    let export =
        export_files_window::open(&store, vec![HashId(1)], &bound.export_files, Rc::new(|| {}))
            .unwrap();
    assert_eq!(
        export.get_destination(),
        relative.to_string_lossy().as_ref()
    );
    export.invoke_dismissed();
    open(&ui);
    let window = bound.options.borrow().as_ref().unwrap().clone_strong();
    show_page(&window, "exporting");
    let (index, shown) = row(&window, reference["label"].as_str().unwrap());
    assert_eq!(shown.text, relative.to_string_lossy().as_ref());
    window.invoke_text_edited(index, " \t ".into());
    window.invoke_apply();
    assert!(
        store
            .read(settings::get::<ExportSettings>)
            .unwrap()
            .default_directory
            .is_none()
    );
    let export =
        export_files_window::open(&store, vec![HashId(1)], &bound.export_files, Rc::new(|| {}))
            .unwrap();
    assert_eq!(export.get_destination(), fallback);
    export.invoke_dismissed();
}

#[test]
fn command_palette_options_stage_queue_changes_and_persist_only_on_apply() {
    use hydrus_store::command_palette::{CommandPaletteSettings, Provider};
    let (_dirs, store) = store();
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    let saved = || {
        store
            .read(hydrus_store::settings::get::<CommandPaletteSettings>)
            .unwrap()
    };
    let oracle = hydrus_testkit::fixture_json("command_palette.json");
    let before = saved();
    open(&ui);
    let options = bound.options.borrow().as_ref().unwrap().clone_strong();
    show_page(&options, "command palette");
    let provider_names = |window: &OptionsWindow| {
        let rows = window.get_provider_rows();
        (0..rows.row_count())
            .map(|i| {
                rows.row_data(i)
                    .unwrap()
                    .cells
                    .row_data(0)
                    .unwrap()
                    .to_string()
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(
        provider_names(&options),
        before
            .provider_order
            .iter()
            .map(|p| p.name().to_owned())
            .collect::<Vec<_>>()
    );
    let (threshold, control) = row(
        &options,
        "Start searching when this many characters have been typed:",
    );
    assert_eq!([control.minimum, control.maximum], [1, 64]);
    options.invoke_number_edited(threshold, 4);
    for label in [
        "Initially show all page results:",
        "Initially show page history results:",
        "Initially show favourite search results:",
        "Include \"page of pages\" page results:",
        "Open favourite searches in a new page:",
        "ADVANCED: Search main menubar:",
        "ADVANCED: Search media menu:",
    ] {
        let (index, control) = row(&options, label);
        options.invoke_check_toggled(index, !control.checked);
    }
    for label in [
        "Max page results to show:",
        "Max page history to show:",
        "Max favourite searches to show:",
    ] {
        let (index, control) = row(&options, label);
        assert_eq!([control.minimum, control.maximum], [1, 1_000_000]);
        options.invoke_none_toggled(index, false);
        options.invoke_number_edited(index, 2);
    }
    options.invoke_provider_clicked(0, false, false);
    options.invoke_provider_action("down".into());
    options.invoke_provider_action("delete".into());
    assert_eq!(
        options.get_provider_message().as_str(),
        oracle["questions"][0].as_str().unwrap()
    );
    options.invoke_provider_answer(false);
    assert_eq!(provider_names(&options)[1], "calculator");
    options.invoke_provider_action("delete".into());
    options.invoke_provider_answer(true);
    assert!(!provider_names(&options).contains(&"calculator".to_owned()));
    options.invoke_provider_action("add".into());
    assert_eq!(options.get_provider_message(), "Select a provider to add:");
    assert_eq!(
        options.get_provider_missing().row_data(0).unwrap(),
        "calculator"
    );
    options.invoke_provider_action("cancel".into());
    assert_eq!(options.get_provider_mode(), 0);
    assert_eq!(
        saved(),
        before,
        "all controls and queue edits belong to the parent draft"
    );
    options.invoke_provider_action("add".into());
    options.invoke_provider_chosen(0);
    assert_eq!(provider_names(&options).last().unwrap(), "calculator");
    options.invoke_apply();
    assert!(bound.options.borrow().is_none());
    let after = saved();
    assert_eq!(after.threshold, 4);
    assert_eq!(after.page_limit, Some(2));
    assert_eq!(after.history_limit, Some(2));
    assert_eq!(after.favourite_limit, Some(2));
    assert!(
        !after.initially_show_pages
            && !after.initially_show_history
            && after.initially_show_favourites
    );
    assert!(after.show_notebooks && after.show_main_menu && after.show_media_menu);
    assert!(!after.favourites_new_page);
    assert_eq!(
        after.provider_order,
        [
            Provider::MainMenu,
            Provider::MediaMenu,
            Provider::History,
            Provider::Pages,
            Provider::Favourites,
            Provider::Calculator
        ]
    );
    open(&ui);
    let reopened = bound.options.borrow().as_ref().unwrap().clone_strong();
    show_page(&reopened, "command palette");
    assert_eq!(
        row(
            &reopened,
            "Start searching when this many characters have been typed:"
        )
        .1
        .number,
        4
    );
    assert_eq!(provider_names(&reopened), provider_names(&options));
    reopened.invoke_provider_clicked(0, false, false);
    reopened.invoke_provider_action("delete".into());
    reopened.invoke_provider_answer(true);
    reopened.invoke_cancel();
    assert_eq!(saved(), after);
    reopened.invoke_provider_action("add".into());
    reopened.invoke_provider_chosen(0);
    reopened.invoke_provider_answer(true);
    assert_eq!(
        saved(),
        after,
        "callbacks from a cancelled owner cannot save preferences"
    );
    open(&ui);
    let successor = bound.options.borrow().as_ref().unwrap().clone_strong();
    show_page(&successor, "command palette");
    assert_eq!(
        provider_names(&successor),
        after
            .provider_order
            .iter()
            .map(|p| p.name().to_owned())
            .collect::<Vec<_>>()
    );
    successor.invoke_cancel();
}

#[test]
fn viewing_menu_preferences_apply_to_real_menu_lines_and_cancel_preserves_them() {
    use hydrus_core::CanvasType;
    use hydrus_gui::thumbnail_menu::{Entry, info_menu};
    use hydrus_store::settings::{FileViewingStatistics, ViewingStatsMenuDisplay};
    fn recorded(entry: &Entry) -> serde_json::Value {
        match entry {
            Entry::Menu(label, children) => {
                serde_json::json!({"label":label,"children":children.iter().map(recorded).collect::<Vec<_>>()})
            }
            Entry::Label(label) => serde_json::json!({"label":label}),
            _ => panic!("viewing statistics are passive labels"),
        }
    }
    let (_dirs, store) = store();
    let oracle = hydrus_testkit::fixture_json("viewing_statistics_options.json");
    let hash = oracle["file"].as_str().unwrap().parse().unwrap();
    let file = store
        .read(|c| hydrus_store::master::hash_id(c, &hash))
        .unwrap()
        .unwrap();
    let now = oracle["now"].as_i64().unwrap() * 1000;
    store
        .write_content(move |w| {
            for (canvas, ago, views, ms) in [
                (CanvasType::MediaViewer, 10, 2, 12_000),
                (CanvasType::Preview, 20, 3, 9000),
                (CanvasType::ClientApi, 30, 4, 28_000),
            ] {
                w.set_views(file, canvas, Some(now - ago * 1000), views, ms)?;
            }
            Ok(())
        })
        .unwrap();
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    for event in oracle["menu_events"].as_array().unwrap() {
        let before = store
            .read(hydrus_store::settings::get::<FileViewingStatistics>)
            .unwrap();
        open(&ui);
        let options = bound.options.borrow().as_ref().unwrap().clone_strong();
        show_page(&options, "file viewing statistics");
        let (menu, control) = row(&options, "Show viewing stats on media right-click menus?:");
        assert_eq!(control.kind, 5);
        options.invoke_choice_chosen(menu, i32::from(event["style"] == 3));
        let (ticks, control) = row(&options, "Which views to show?:");
        assert_eq!(control.kind, 23);
        assert_eq!(control.items.row_count(), 3);
        for (index, code) in [0, 1, 4].iter().enumerate() {
            options.invoke_canvas_toggled(
                ticks,
                i32::try_from(index).unwrap(),
                event["canvases"]
                    .as_array()
                    .unwrap()
                    .contains(&serde_json::json!(code)),
            );
        }
        assert_eq!(
            store
                .read(hydrus_store::settings::get::<FileViewingStatistics>)
                .unwrap(),
            before,
            "changes remain staged until parent Apply"
        );
        options.invoke_apply();
        assert!(bound.options.borrow().is_none());
        let saved = store
            .read(hydrus_store::settings::get::<FileViewingStatistics>)
            .unwrap();
        assert_eq!(
            saved.menu_display,
            if event["style"] == 3 {
                ViewingStatsMenuDisplay::Stacked
            } else {
                ViewingStatsMenuDisplay::Combined
            }
        );
        let entry = info_menu(
            &store,
            Some(file),
            (&[file], hydrus_gui::status::Items::files(1)),
            &hydrus_core::media_viewer::InfoLineSettings::default(),
            now,
        )
        .unwrap();
        let Entry::Menu(_, lines) = entry else {
            panic!("media info submenu")
        };
        let view_lines: Vec<_> = lines.iter().filter(|line| matches!(line,Entry::Label(label)|Entry::Menu(label,_) if label.starts_with("viewed ") || label.starts_with("no view record"))).map(recorded).collect();
        assert_eq!(serde_json::json!(view_lines), event["menu"], "{event}");
        open(&ui);
        let reopened = bound.options.borrow().as_ref().unwrap().clone_strong();
        show_page(&reopened, "file viewing statistics");
        assert_eq!(
            row(&reopened, "Show viewing stats on media right-click menus?:")
                .1
                .index,
            i32::from(saved.menu_display == ViewingStatsMenuDisplay::Stacked)
        );
        reopened.invoke_canvas_toggled(
            ticks,
            0,
            !saved
                .interesting_canvases
                .contains(&CanvasType::MediaViewer),
        );
        reopened.invoke_cancel();
        reopened.invoke_canvas_toggled(ticks, 1, true);
        assert_eq!(
            store
                .read(hydrus_store::settings::get::<FileViewingStatistics>)
                .unwrap(),
            saved
        );
    }
    ui.invoke_search_edited("system:everything".into());
    ui.invoke_search_accepted();
    let current = bound.current.borrow().clone();
    let other = current
        .borrow()
        .results()
        .iter()
        .copied()
        .find(|id| *id != file)
        .expect("second imported file");
    let sorted_files = current.borrow().results().to_vec();
    store
        .write_content(move |w| {
            // The basic fixture contains other media-view counts (up to six).
            // Isolate this pair's ranking while keeping the real page result
            // set, persisted options and sort consumer under test.
            for id in sorted_files {
                for canvas in [CanvasType::MediaViewer, CanvasType::Preview] {
                    w.set_views(id, canvas, Some(now), 0, 0)?;
                }
            }
            w.set_views(file, CanvasType::MediaViewer, Some(now), 2, 12_000)?;
            w.set_views(file, CanvasType::Preview, Some(now), 3, 9000)?;
            w.set_views(other, CanvasType::MediaViewer, Some(now), 1, 1000)?;
            w.set_views(other, CanvasType::Preview, Some(now), 10, 1000)
        })
        .unwrap();
    for (wanted, index) in [(file, 0), (other, 1)] {
        open(&ui);
        let options = bound.options.borrow().as_ref().unwrap().clone_strong();
        show_page(&options, "file viewing statistics");
        let ticks = row(&options, "Which views to show?:").0;
        for canvas in 0..3 {
            options.invoke_canvas_toggled(ticks, canvas, canvas == index);
        }
        options.invoke_apply();
        current
            .borrow_mut()
            .set_sort_by(hydrus_search::exec::SortBy::MediaViews);
        current
            .borrow_mut()
            .set_sort_order(hydrus_search::exec::SortOrder::Descending);
        assert_eq!(
            current.borrow().results().first(),
            Some(&wanted),
            "new canvas ticks reach the actual page views sort"
        );
    }
}

#[test]
fn viewing_timing_options_reach_real_viewer_and_archive_filter_lifetimes() {
    use hydrus_core::{CanvasType, HashId};
    use hydrus_store::settings::{self, FileViewingStatistics};
    let (_dirs, store) = store();
    let oracle = hydrus_testkit::fixture_json("viewing_statistics_options.json");
    let file = store
        .read(|conn| {
            hydrus_store::master::hash_id(conn, &oracle["file"].as_str().unwrap().parse().unwrap())
        })
        .unwrap()
        .unwrap();
    store
        .write_content(move |w| w.set_views(file, CanvasType::MediaViewer, None, 0, 0))
        .unwrap();
    let stats = |file: HashId| {
        store
            .read(|conn| hydrus_store::media::viewing_stats(conn, &[file]))
            .unwrap()
            .into_iter()
            .find(|s| s.canvas == CanvasType::MediaViewer)
            .unwrap()
    };
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(
        &ui,
        Pages::single(super::common::all_local_page(store.clone())),
    );
    ui.invoke_search_edited("system:everything".into());
    ui.invoke_search_accepted();
    let index = i32::try_from(
        bound
            .current
            .borrow()
            .borrow()
            .results()
            .iter()
            .position(|id| *id == file)
            .unwrap(),
    )
    .unwrap();
    open(&ui);
    let options = bound.options.borrow().as_ref().unwrap().clone_strong();
    show_page(&options, "file viewing statistics");
    let minimum = row(
        &options,
        "Min time to view on media viewer to count as a view:",
    )
    .0;
    let maximum = row(
        &options,
        "Cap any view on the media viewer to this maximum time:",
    )
    .0;
    let archive = row(
        &options,
        "Enable file viewing statistics tracking in the archive/delete filter?:",
    )
    .0;
    let duplicates = row(
        &options,
        "Enable file viewing statistics tracking in the duplicate filter?:",
    )
    .0;
    assert_eq!(
        row(
            &options,
            "Min time to view on media viewer to count as a view:"
        )
        .1
        .kind,
        24
    );
    options.invoke_none_toggled(minimum, true);
    // Hours/minutes/seconds/milliseconds: an entered 0 is clamped to the 1s cap.
    for field in 0..4 {
        options.invoke_field_edited(maximum, field, 0);
    }
    options.invoke_check_toggled(archive, false);
    options.invoke_check_toggled(duplicates, true);
    let pixels = headless::render(&windows.get(windows.count() - 1).unwrap(), 900, 640);
    headless::save_png(
        &std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("options_viewing_statistics.png"),
        &pixels,
        900,
        640,
    )
    .unwrap();
    options.invoke_apply();
    let saved = store.read(settings::get::<FileViewingStatistics>).unwrap();
    assert_eq!(saved.media_min_ms, None);
    assert_eq!(saved.media_max_ms, Some(1000));
    assert!(!saved.archive_delete);
    assert!(saved.duplicates);
    ui.invoke_thumbnail_activated(index);
    let viewer = bound.viewer.borrow().as_ref().unwrap().clone_strong();
    std::thread::sleep(std::time::Duration::from_millis(1050));
    // Presentation refresh is the same file and must retain this one interval.
    viewer.invoke_presentation_settings_changed();
    viewer.invoke_close_requested();
    let recorded = stats(file);
    assert_eq!((recorded.views, recorded.viewtime_ms), (1, 1000));
    viewer.invoke_close_requested();
    viewer.invoke_next();
    assert_eq!(
        stats(file),
        recorded,
        "a closed owner cannot add another interval"
    );
    open(&ui);
    let reopened = bound.options.borrow().as_ref().unwrap().clone_strong();
    show_page(&reopened, "file viewing statistics");
    assert!(
        row(
            &reopened,
            "Min time to view on media viewer to count as a view:"
        )
        .1
        .is_none
    );
    assert_eq!(
        row(
            &reopened,
            "Cap any view on the media viewer to this maximum time:"
        )
        .1
        .fields
        .row_data(2)
        .unwrap()
        .value,
        1
    );
    reopened.invoke_none_toggled(minimum, false);
    reopened.invoke_check_toggled(archive, true);
    reopened.invoke_cancel();
    reopened.invoke_apply();
    assert_eq!(
        store.read(settings::get::<FileViewingStatistics>).unwrap(),
        saved,
        "Cancel and stale Apply preserve saved timing"
    );
    ui.invoke_thumbnail_clicked(index, false, false);
    ui.invoke_archive_delete_filter();
    let filter = bound
        .archive_delete
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    filter.invoke_close_requested();
    assert_eq!(
        stats(file),
        recorded,
        "disabled archive filter adds no viewing record"
    );
    open(&ui);
    let options = bound.options.borrow().as_ref().unwrap().clone_strong();
    show_page(&options, "file viewing statistics");
    options.invoke_check_toggled(archive, true);
    options.invoke_none_toggled(maximum, true);
    options.invoke_apply();
    ui.invoke_archive_delete_filter();
    let filter = bound
        .archive_delete
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    filter.invoke_keep();
    assert!(!filter.get_question().is_empty());
    filter.invoke_forget();
    assert!(bound.archive_delete.borrow().is_none());
    assert_eq!(
        stats(file).views,
        2,
        "forgetting archive decisions still records actual viewing"
    );
    filter.invoke_commit();
    filter.invoke_keep();
    assert_eq!(
        stats(file).views,
        2,
        "closed filter callbacks cannot write or restart tracking"
    );
    // Two owner windows may overlap. Closing an old one flushes only its own
    // interval and cannot discard or navigate the replacement owner.
    ui.invoke_thumbnail_activated(index);
    let stale = bound.viewer.borrow().as_ref().unwrap().clone_strong();
    ui.invoke_thumbnail_activated(index);
    let live = bound.viewer.borrow().as_ref().unwrap().clone_strong();
    stale.invoke_close_requested();
    let once = stats(file).views;
    stale.invoke_next();
    stale.invoke_close_requested();
    assert_eq!(stats(file).views, once);
    assert!(std::ptr::eq(
        bound.viewer.borrow().as_ref().unwrap().window(),
        live.window()
    ));
    live.invoke_close_requested();
    assert_eq!(stats(file).views, once + 1);
}

#[test]
fn files_trash_confirmations_are_staged_reopened_and_consumed() {
    use hydrus_gui::media_actions;
    use hydrus_store::settings::DeletionPreferences;
    let (_dirs, store) = store();
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(
        &ui,
        Pages::single(super::common::all_local_page(store.clone())),
    );
    let preferences = || {
        store
            .read(hydrus_store::settings::get::<DeletionPreferences>)
            .unwrap()
    };
    let change = |value| {
        open(&ui);
        let window = bound.options.borrow().as_ref().unwrap().clone_strong();
        show_page(&window, "files and trash");
        for label in [
            "Confirm sending files to trash: ",
            "Confirm sending more than one file to archive or inbox: ",
        ] {
            let (index, _) = row(&window, label);
            window.invoke_check_toggled(index, value);
        }
        window
    };
    let before = preferences();
    let window = change(false);
    assert_eq!(preferences(), before);
    window.invoke_cancel();
    assert_eq!(preferences(), before);
    let window = change(false);
    window.invoke_apply();
    assert!(!preferences().confirm_archive);
    assert!(!preferences().confirm_trash);
    open(&ui);
    let reopened = bound.options.borrow().as_ref().unwrap().clone_strong();
    show_page(&reopened, "files and trash");
    assert!(!row(&reopened, "Confirm sending files to trash: ").1.checked);
    assert!(
        !row(
            &reopened,
            "Confirm sending more than one file to archive or inbox: "
        )
        .1
        .checked
    );
    reopened.invoke_cancel();

    ui.invoke_search_edited("system:everything".into());
    ui.invoke_search_accepted();
    let page = bound.current.borrow().clone();
    let results = page.borrow().results().to_vec();
    let (inbox, _) = media_actions::by_inbox(&store, &results);
    let files = &inbox[..2];
    page.borrow_mut().select_files(files);
    ui.invoke_archive_selected();
    assert_eq!(ui.get_question(), "");
    assert!(media_actions::by_inbox(&store, files).0.is_empty());
    ui.invoke_inbox_selected();
    assert_eq!(media_actions::by_inbox(&store, files).0, files);
    change(true).invoke_apply();
    ui.invoke_archive_selected();
    assert_eq!(ui.get_question(), "Archive 2 files?");
    ui.invoke_answer(false);
    assert_eq!(media_actions::by_inbox(&store, files).0, files);
    ui.invoke_archive_selected();
    ui.invoke_answer(true);
    assert!(media_actions::by_inbox(&store, files).0.is_empty());

    change(false).invoke_apply();
    let single_domain = results
        .iter()
        .copied()
        .find(|file| {
            let snapshot = store.snapshot();
            let roles = hydrus_store::content::DomainRoles::new(&snapshot.services).unwrap();
            let batch = store
                .read(|c| hydrus_store::media::load(c, &snapshot.services, None, &[*file]))
                .unwrap();
            batch.results[0]
                .current
                .iter()
                .filter(|current| roles.local.contains(&current.service))
                .count()
                == 1
        })
        .unwrap();
    let index = page
        .borrow()
        .results()
        .iter()
        .position(|file| *file == single_domain)
        .unwrap();
    ui.invoke_thumbnail_activated(i32::try_from(index).unwrap());
    let viewer = bound.viewer.borrow().as_ref().unwrap().clone_strong();
    let before = page.borrow().results().len();
    viewer.invoke_delete();
    assert_eq!(viewer.get_question(), "");
    assert_eq!(page.borrow().results().len(), before - 1);
    assert!(!page.borrow().results().contains(&single_domain));
    viewer.invoke_close_requested();
    assert!(windows.get(0).is_some());
}

#[test]
fn advanced_deletion_queue_stages_custom_reason_cancel_and_real_consumer() {
    use hydrus_store::settings::DeletionPreferences;
    let (_dirs, store) = store();
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(
        &ui,
        Pages::single(super::common::all_local_page(store.clone())),
    );
    let preferences = || {
        store
            .read(hydrus_store::settings::get::<DeletionPreferences>)
            .unwrap()
    };
    open(&ui);
    let options = bound.options.borrow().as_ref().unwrap().clone_strong();
    show_page(&options, "files and trash");
    assert!(!row(&options, "Remember the last reason: ").1.enabled);
    let (advanced, _) = row(&options, "Use the advanced file deletion dialog: ");
    options.invoke_check_toggled(advanced, true);
    assert!(row(&options, "Remember the last reason: ").1.enabled);
    options.invoke_reason_action("add".into());
    let child = bound
        .options_reason_child
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    assert_eq!(child.get_message(), "Enter the reason");
    assert_eq!(child.get_text(), "I do not like the file.");
    child.invoke_name_entered("synthetic staged reason 日本".into());
    assert!(
        !preferences()
            .reasons
            .contains(&"synthetic staged reason 日本".into())
    );
    options.invoke_reason_clicked(0, false, false);
    options.invoke_reason_action("edit".into());
    let stale = bound
        .options_reason_child
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    options.invoke_apply();
    assert!(
        bound.options.borrow().is_some(),
        "child blocks parent Apply"
    );
    options.invoke_cancel();
    assert!(bound.options_reason_child.borrow().is_none());
    assert!(!stale.window().is_visible());
    stale.invoke_name_entered("stale replacement".into());
    options.invoke_apply();
    assert!(!preferences().advanced);
    assert!(
        !preferences()
            .reasons
            .contains(&"synthetic staged reason 日本".into())
    );

    open(&ui);
    let options = bound.options.borrow().as_ref().unwrap().clone_strong();
    show_page(&options, "files and trash");
    let (advanced, _) = row(&options, "Use the advanced file deletion dialog: ");
    options.invoke_check_toggled(advanced, true);
    options.invoke_reason_action("add".into());
    let child = bound
        .options_reason_child
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    child.invoke_name_entered("synthetic applied reason 日本".into());
    assert!(bound.options_reason_child.borrow().is_none());
    let (remember, _) = row(&options, "Remember the last action: ");
    options.invoke_check_toggled(remember, true);
    options.invoke_reason_clicked(0, false, false);
    let original_rows = options.get_reason_rows();
    let original = (0..original_rows.row_count())
        .map(|i| {
            original_rows
                .row_data(i)
                .unwrap()
                .cells
                .row_data(0)
                .unwrap()
        })
        .collect::<Vec<_>>();
    options.invoke_reason_action("down".into());
    assert_eq!(
        options
            .get_reason_rows()
            .row_data(1)
            .unwrap()
            .cells
            .row_data(0)
            .unwrap(),
        original[0]
    );
    options.invoke_reason_action("up".into());
    options.invoke_reason_action("delete".into());
    let question = bound
        .options_reason_child
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    assert_eq!(question.get_message(), "Remove 1 selected?");
    question.invoke_answered(false);
    assert_eq!(options.get_reason_rows().row_count(), original.len());
    options.invoke_apply();
    assert!(preferences().advanced);
    assert!(preferences().remember_action);
    assert_eq!(
        preferences().reasons.last().map(String::as_str),
        Some("synthetic applied reason 日本")
    );
    open(&ui);
    let reopened = bound.options.borrow().as_ref().unwrap().clone_strong();
    show_page(&reopened, "files and trash");
    assert_eq!(
        reopened
            .get_reason_rows()
            .row_data(reopened.get_reason_rows().row_count() - 1)
            .unwrap()
            .cells
            .row_data(0)
            .unwrap(),
        "synthetic applied reason 日本"
    );
    reopened.invoke_cancel();
    ui.invoke_search_edited("system:everything".into());
    ui.invoke_search_accepted();
    let recording = hydrus_testkit::fixture_json("files_trash.json");
    let hash: hydrus_core::Sha256 = recording["advanced"][0]["hash"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    let file = store
        .read(|c| hydrus_store::master::hash_id(c, &hash))
        .unwrap()
        .unwrap();
    let page = bound.current.borrow().clone();
    page.borrow_mut().select_files(&[file]);
    let original_reason = store
        .read(|c| hydrus_store::media::load(c, &store.snapshot().services, None, &[file]))
        .unwrap()
        .results[0]
        .deletion_reason
        .clone()
        .unwrap();
    ui.invoke_delete_selected();
    let deletion = bound.delete_files.borrow().as_ref().unwrap().clone_strong();
    let reasons = deletion.get_reasons();
    let authored = (0..reasons.row_count())
        .find(|&i| reasons.row_data(i).unwrap() == "synthetic applied reason 日本")
        .unwrap();
    let custom = (0..reasons.row_count())
        .find(|&i| reasons.row_data(i).unwrap() == "custom")
        .unwrap();
    assert!(authored < custom);
    // The reference appends an unlisted existing reason after its custom row.
    assert_eq!(
        reasons.row_data(reasons.row_count() - 1).unwrap().as_str(),
        format!("keep existing reason: {original_reason}")
    );
    assert_eq!(
        deletion.get_selected_reason(),
        i32::try_from(reasons.row_count() - 1).unwrap()
    );
    let before = preferences();
    deletion.invoke_cancel();
    deletion.invoke_accept_deletion();
    assert_eq!(preferences(), before);
    assert!(page.borrow().results().contains(&file));
    ui.invoke_delete_selected();
    let deletion = bound.delete_files.borrow().as_ref().unwrap().clone_strong();
    let index = (0..deletion.get_reasons().row_count())
        .find(|&i| deletion.get_reasons().row_data(i).unwrap() == "synthetic applied reason 日本")
        .unwrap();
    deletion.invoke_reason_selected(i32::try_from(index).unwrap());
    deletion.invoke_accept_deletion();
    assert!(!page.borrow().results().contains(&file));
    let batch = store
        .read(|c| hydrus_store::media::load(c, &store.snapshot().services, None, &[file]))
        .unwrap();
    assert_eq!(
        batch.results[0].deletion_reason.as_deref(),
        Some("synthetic applied reason 日本")
    );
    assert_eq!(
        preferences().last_reason.as_deref(),
        Some("synthetic applied reason 日本")
    );
    assert!(preferences().last_action.is_some());
}

fn banner_questions(window: &hydrus_gui::TagBannerWindow, answers: [&str; 3]) {
    for (message, answer) in ["Edit namespace.", "Edit prefix.", "Edit separator."]
        .into_iter()
        .zip(answers)
    {
        assert!(window.get_asking());
        assert_eq!(window.get_message(), message);
        window.set_prompt_text(answer.into());
        window.invoke_prompt_accepted();
    }
    assert!(!window.get_asking());
}

fn replay_banner_events(window: &hydrus_gui::TagBannerWindow, events: &[serde_json::Value]) {
    for event in events {
        match event["action"].as_str().unwrap() {
            "appearance/examples" => {
                for (channel, value) in [12, 34, 56, 78].into_iter().enumerate() {
                    window.invoke_colour_edited(false, channel as i32, value);
                }
                for (channel, value) in [210, 180, 140, 120].into_iter().enumerate() {
                    window.invoke_colour_edited(true, channel as i32, value);
                }
                window.set_separator(" / ".into());
                window.set_examples(" CREATOR:Alpha \ncreator:alpha\ntitle:Beta\npage:10\npage:2\npage:3\npage:alpha\n blue_eyes \n".into());
                window.invoke_changed();
            }
            "hide" | "show" => {
                window.set_showing(event["action"] == "show");
                window.invoke_changed();
            }
            "add namespace" => {
                window.invoke_action("add".into());
                banner_questions(window, ["page", "p=", ".."]);
                let last = window.get_rows().row_count() - 1;
                window.invoke_row_clicked(last as i32, false, false);
            }
            "move up" => window.invoke_action("up".into()),
            "move down" => window.invoke_action("down".into()),
            "edit namespace" => {
                window.invoke_action("edit".into());
                banner_questions(window, ["", "plain=", " + "]);
            }
            "cancel namespace" => {
                window.invoke_action("edit".into());
                window.set_prompt_text("discard this namespace".into());
                window.invoke_prompt_accepted();
                window.invoke_prompt_cancelled();
            }
            "cancel delete" | "delete" => {
                window.invoke_action("delete".into());
                assert_eq!(window.get_message(), "Remove 1 selected?");
                window.invoke_answer(event["action"] == "delete");
            }
            other => panic!("{other}"),
        }
        assert_eq!(
            window.get_preview(),
            event["preview"].as_str().unwrap(),
            "{event}"
        );
        let value: hydrus_core::tag_summary::TagSummaryGenerator =
            serde_json::from_value(event["value"].clone()).unwrap();
        assert_eq!(window.get_showing(), value.show);
        assert_eq!(window.get_separator(), value.separator);
        let rows = window.get_rows();
        assert_eq!(rows.row_count(), value.namespace_info.len());
        for (i, info) in value.namespace_info.into_iter().enumerate() {
            let expected = hydrus_gui_model::tag_banner::Row {
                id: 0,
                info,
                selected: false,
            }
            .label();
            assert_eq!(
                rows.row_data(i).unwrap().cells.row_data(0).unwrap(),
                expected
            );
        }
        for (channels, expected) in [
            (window.get_background_channels(), value.background),
            (window.get_text_channels(), value.text),
        ] {
            assert_eq!(
                (0..channels.row_count())
                    .map(|i| channels.row_data(i).unwrap())
                    .collect::<Vec<_>>(),
                expected.map(i32::from)
            );
        }
    }
}

#[test]
fn banner_options_match_qt_drafts_and_refresh_cached_thumbnails_and_open_viewer() {
    use hydrus_core::{ContentStatus, ServiceKey, Tag, tag_summary::TagSummaries};
    use hydrus_store::{content::MappingAction, settings};
    let (_dirs, store) = store();
    let fixture = hydrus_testkit::fixture_json("tag_banner_editors.json");
    let file = store
        .read(|conn| {
            hydrus_store::master::hash_id(
                conn,
                &fixture["consumers"]["file"]
                    .as_str()
                    .unwrap()
                    .parse()
                    .unwrap(),
            )
        })
        .unwrap()
        .unwrap();
    let snapshot = store.snapshot();
    let existing = store
        .read(|conn| hydrus_store::media::load(conn, &snapshot.services, None, &[file]))
        .unwrap()
        .results
        .remove(0)
        .tags;
    let service = snapshot
        .services
        .by_key(&ServiceKey::new(
            hydrus_core::service::builtin_keys::MY_TAGS.to_vec(),
        ))
        .unwrap()
        .id;
    let tags: Vec<Tag> = fixture["consumers"]["tags"]
        .as_array()
        .unwrap()
        .iter()
        .map(|tag| Tag::new(tag.as_str().unwrap()).unwrap())
        .collect();
    store
        .write_content(move |writer| {
            for (service, tags) in existing {
                for (status, ids) in tags.by_status {
                    let action = match status {
                        ContentStatus::Current => MappingAction::Delete,
                        ContentStatus::Pending => MappingAction::RescindPend,
                        _ => continue,
                    };
                    for tag in ids {
                        writer.update_mappings(service, &action, tag, &[file])?;
                    }
                }
            }
            for tag in tags {
                let id = hydrus_store::master::intern_tag(writer.conn(), &tag)?;
                let action = if tag.as_str() == "title:pending" {
                    MappingAction::Pend
                } else {
                    MappingAction::Add
                };
                writer.update_mappings(service, &action, id, &[file])?;
            }
            Ok(())
        })
        .unwrap();
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(
        &ui,
        Pages::single(super::common::all_local_page(store.clone())),
    );
    ui.invoke_search_edited("system:everything".into());
    ui.invoke_search_accepted();
    let index = bound
        .current
        .borrow()
        .borrow()
        .results()
        .iter()
        .position(|id| *id == file)
        .unwrap();
    let thumbnail = || {
        (0..bound.rows.row_count())
            .find_map(|row| {
                let data = bound.rows.row_data(row).unwrap();
                let first = usize::try_from(data.first).unwrap();
                (index >= first && index < first + data.thumbnails.row_count())
                    .then(|| data.thumbnails.row_data(index - first).unwrap())
            })
            .unwrap()
    };
    let cached_before = thumbnail();
    ui.invoke_thumbnail_activated(index as i32);
    let viewer = bound.viewer.borrow().as_ref().unwrap().clone_strong();
    let title_before = viewer.get_tag_banner();
    let viewer_adapter = windows.get(windows.count() - 1).unwrap();
    let before: TagSummaries = store.read(settings::get).unwrap();
    open(&ui);
    let mut options = bound.options.borrow().as_ref().unwrap().clone_strong();
    show_page(&options, "tag presentation");
    let events = fixture["events"].as_array().unwrap();
    let mut old_children = Vec::new();
    for (index, route) in fixture["routes"].as_array().unwrap().iter().enumerate() {
        let label = match route["key"].as_str().unwrap() {
            "thumbnail_top" => "On thumbnail top:",
            "thumbnail_bottom_right" => "On thumbnail bottom-right:",
            "media_viewer_top" => "On media viewer top:",
            other => panic!("{other}"),
        };
        let (at, button) = row(&options, label);
        assert_eq!(button.kind, 27);
        options.invoke_banner_clicked(at);
        let child = bound
            .options_banner_child
            .borrow()
            .as_ref()
            .unwrap()
            .clone_strong();
        replay_banner_events(&child, &events[index * 10..(index + 1) * 10]);
        options.invoke_apply();
        assert!(options.window().is_visible(), "parent waits for its child");
        assert_eq!(store.read(settings::get::<TagSummaries>).unwrap(), before);
        if index == 0 {
            child.invoke_action("cancel".into());
            assert_eq!(row(&options, label).1.text, button.text);
        } else {
            child.invoke_action("apply".into());
            assert_eq!(
                row(&options, label).1.text,
                route["label"].as_str().unwrap()
            );
            if index == 1 {
                // Accepted child changes are still discarded by parent Cancel.
                options.invoke_cancel();
                assert_eq!(store.read(settings::get::<TagSummaries>).unwrap(), before);
                open(&ui);
                options = bound.options.borrow().as_ref().unwrap().clone_strong();
                show_page(&options, "tag presentation");
                options.invoke_banner_clicked(row(&options, label).0);
                let again = bound
                    .options_banner_child
                    .borrow()
                    .as_ref()
                    .unwrap()
                    .clone_strong();
                replay_banner_events(&again, &events[10..20]);
                again.invoke_action("apply".into());
                old_children.push(again);
            }
        }
        assert!(!child.window().is_visible());
        assert!(bound.options_banner_child.borrow().is_none());
        old_children.push(child);
    }
    assert_eq!(
        thumbnail().top,
        cached_before.top,
        "staging retains cached banner"
    );
    assert_eq!(
        viewer.get_tag_banner(),
        title_before,
        "staging retains live viewer title"
    );
    options.invoke_apply();
    let saved: TagSummaries = serde_json::from_value(fixture["saved"].clone()).unwrap();
    assert_eq!(store.read(settings::get::<TagSummaries>).unwrap(), saved);
    let refreshed = thumbnail();
    assert_eq!(
        refreshed.top,
        fixture["consumers"]["thumbnail_top"].as_str().unwrap()
    );
    assert_eq!(
        refreshed.bottom,
        fixture["consumers"]["thumbnail_bottom_right"]
            .as_str()
            .unwrap()
    );
    assert_ne!(refreshed.top, cached_before.top);
    assert_eq!(
        viewer.get_tag_banner(),
        fixture["consumers"]["viewer_title"].as_str().unwrap()
    );
    assert_ne!(viewer.get_tag_banner(), title_before);
    assert_eq!(
        ui.get_banner_top_background(),
        slint::Color::from_argb_u8(78, 12, 34, 56)
    );
    assert_eq!(
        ui.get_banner_bottom_text(),
        slint::Color::from_argb_u8(120, 210, 180, 140)
    );
    let viewer_pixels = headless::render(&viewer_adapter, 900, 640);
    headless::save_png(
        &std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("tag_banner_viewer.png"),
        &viewer_pixels,
        900,
        640,
    )
    .unwrap();
    // Passive information belongs behind opaque media; the top hover raises it above.
    let focus = hydrus_gui::viewer_focus::NativeFocus::new(&viewer);
    let identity = slint::winit_030::winit::window::WindowId::from(98_001);
    focus.watch_id(identity);
    hydrus_gui::session_autosave::observe_native_focus(identity, true);
    viewer
        .window()
        .dispatch_event(slint::platform::WindowEvent::PointerMoved {
            position: slint::LogicalPosition::new(450.0, 10.0),
        });
    assert!(
        viewer.get_info_showing(),
        "saved banner's top hover is raised"
    );
    assert_eq!(
        viewer.get_tag_banner(),
        fixture["consumers"]["viewer_title"].as_str().unwrap()
    );
    // Keep asynchronous media/timers still so only the raised overlay changes paint.
    let raised_pixels = headless::render_snapshot(&viewer_adapter, 900, 640);
    assert_ne!(
        &raised_pixels[..900 * 90 * 4],
        &viewer_pixels[..900 * 90 * 4],
        "the raised title and information paint over the real opaque media"
    );
    headless::save_png(
        &std::path::Path::new(env!("CARGO_TARGET_TMPDIR"))
            .join("tag_banner_viewer_raised_hover.png"),
        &raised_pixels,
        900,
        640,
    )
    .unwrap();
    let main_pixels = headless::render(&windows.get(0).unwrap(), 900, 640);
    headless::save_png(
        &std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("tag_banner_thumbnails.png"),
        &main_pixels,
        900,
        640,
    )
    .unwrap();
    open(&ui);
    let reopened = bound.options.borrow().as_ref().unwrap().clone_strong();
    show_page(&reopened, "tag presentation");
    reopened.invoke_banner_clicked(row(&reopened, "On media viewer top:").0);
    let reopened_child = bound
        .options_banner_child
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    assert_eq!(
        reopened_child.get_preview(),
        events[39]["preview"].as_str().unwrap()
    );
    let adapter = windows.get(windows.count() - 1).unwrap();
    let pixels = headless::render(&adapter, 780, 670);
    headless::save_png(
        &std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("tag_banner_editor.png"),
        &pixels,
        780,
        670,
    )
    .unwrap();
    for child in old_children {
        child.set_showing(false);
        child.invoke_changed();
        child.invoke_action("apply".into());
        child.invoke_row_clicked(0, false, false);
        child.invoke_action("delete".into());
        child.invoke_answer(true);
    }
    assert!(
        reopened_child.window().is_visible(),
        "stale children cannot clear a fresh owner slot"
    );
    assert!(std::ptr::eq(
        bound
            .options_banner_child
            .borrow()
            .as_ref()
            .unwrap()
            .window(),
        reopened_child.window()
    ));
    reopened_child.set_showing(false);
    reopened_child.invoke_changed();
    assert_eq!(reopened_child.get_preview(), "not showing");
    reopened.invoke_cancel();
    assert!(!reopened_child.window().is_visible());
    assert!(bound.options_banner_child.borrow().is_none());
    reopened_child.invoke_action("apply".into());
    assert_eq!(store.read(settings::get::<TagSummaries>).unwrap(), saved);
    assert_eq!(
        viewer.get_tag_banner(),
        fixture["consumers"]["viewer_title"].as_str().unwrap()
    );
    // Enabled state reaches both already-open consumers on the next parent Apply.
    open(&ui);
    let options = bound.options.borrow().as_ref().unwrap().clone_strong();
    show_page(&options, "tag presentation");
    for label in ["On thumbnail top:", "On media viewer top:"] {
        options.invoke_banner_clicked(row(&options, label).0);
        let child = bound
            .options_banner_child
            .borrow()
            .as_ref()
            .unwrap()
            .clone_strong();
        child.set_showing(false);
        child.invoke_changed();
        child.invoke_action("apply".into());
    }
    options.invoke_apply();
    assert!(thumbnail().top.is_empty());
    assert!(viewer.get_tag_banner().is_empty());
}

#[test]
fn eye_menu_collapse_options_stage_reopen_and_rebuild_the_existing_browser_viewer() {
    use hydrus_store::settings::{
        self, ViewerBackgroundSettings, ViewerCanvasSettings, ViewerEyeMenuSettings,
        ViewerHoverSettings,
    };
    use serde_json::json;

    fn flags(menu: &hydrus_gui::ViewerEyeMenu) -> serde_json::Value {
        json!([
            menu.collapse_window,
            menu.collapse_hovers,
            menu.collapse_rendering
        ])
    }
    fn labels(rows: &slint::ModelRc<hydrus_gui::MenuRow>) -> Vec<String> {
        rows.iter().map(|row| row.label.to_string()).collect()
    }

    let fixture = hydrus_testkit::fixture_json("viewer_eye_menu.json");
    let (_dirs, store) = store();
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(
        &ui,
        Pages::single(hydrus_gui::SearchPage::new(store.clone())),
    );
    ui.invoke_search_edited("system:everything".into());
    ui.invoke_search_accepted();
    let files = bound.current.borrow().borrow().results().to_vec();
    let index = files
        .iter()
        .position(|id| {
            store
                .read(|conn| {
                    let (flags, duration): (u32, Option<i64>) = conn.query_row(
                        "SELECT flags, duration_ms FROM files WHERE hash_id = ?",
                        [id],
                        |row| Ok((row.get(0)?, row.get(1)?)),
                    )?;
                    Ok(duration.is_none()
                        && hydrus_store::media::FileFlags(flags)
                            .has(hydrus_store::media::FileFlags::TRANSPARENCY))
                })
                .unwrap()
        })
        .expect("basic fixture has actual transparent still media");
    ui.invoke_thumbnail_activated(i32::try_from(index).unwrap());
    let viewer = bound.viewer.borrow().as_ref().unwrap().clone_strong();
    let drawn = windows.get(windows.count() - 1).unwrap();
    viewer.invoke_eye_menu_requested();
    assert_eq!(flags(&viewer.get_eye_menu()), fixture["initial"]);
    for event in fixture["events"].as_array().unwrap() {
        let before: ViewerEyeMenuSettings = store.read(settings::get).unwrap();
        open(&ui);
        let options = bound.options.borrow().as_ref().unwrap().clone_strong();
        show_page(&options, "media viewer hovers");
        for (label, value) in fixture["labels"]
            .as_array()
            .unwrap()
            .iter()
            .zip(event["values"].as_array().unwrap())
        {
            options.invoke_check_toggled(
                row(&options, label.as_str().unwrap()).0,
                value.as_bool().unwrap(),
            );
        }
        // A detached edit cannot affect the already-open viewer or the store.
        viewer.invoke_eye_menu_requested();
        assert_eq!(
            flags(&viewer.get_eye_menu()),
            json!([
                before.collapse_window,
                before.collapse_hovers,
                before.collapse_rendering
            ])
        );
        assert_eq!(
            store.read(settings::get::<ViewerEyeMenuSettings>).unwrap(),
            before
        );
        options.invoke_cancel();
        options.invoke_apply(); // Retired owner must not save its staged values.
        assert_eq!(
            store.read(settings::get::<ViewerEyeMenuSettings>).unwrap(),
            before
        );
        open(&ui);
        let options = bound.options.borrow().as_ref().unwrap().clone_strong();
        show_page(&options, "media viewer hovers");
        for (label, value) in fixture["labels"]
            .as_array()
            .unwrap()
            .iter()
            .zip(event["values"].as_array().unwrap())
        {
            options.invoke_check_toggled(
                row(&options, label.as_str().unwrap()).0,
                value.as_bool().unwrap(),
            );
        }
        options.invoke_apply();
        viewer.invoke_eye_menu_requested();
        let menu = viewer.get_eye_menu();
        assert_eq!(flags(&menu), event["stored"]);
        // All supported rows keep their original group and ordering in every
        // topology. The real Slint menu switches these same rows between root
        // and submenus; it does not substitute a separate menu for the viewer.
        for (name, groups) in [
            ("window", menu.window),
            ("hovers", menu.hovers),
            ("rendering", menu.rendering),
        ] {
            let native = labels(&groups.g1)
                .into_iter()
                .chain(labels(&groups.g2))
                .collect::<Vec<_>>();
            let reference = event["menu"].as_array().unwrap();
            let entries = reference
                .iter()
                .find(|entry| entry["menu"] == name)
                .map_or(reference, |entry| entry["entries"].as_array().unwrap());
            let expected: Vec<_> = entries
                .iter()
                .filter_map(|entry| entry["check"].as_str())
                .filter(|label| native.iter().any(|native| native == label))
                .collect();
            assert_eq!(native, expected, "{name} {event}");
        }
        open(&ui);
        let reopened = bound.options.borrow().as_ref().unwrap().clone_strong();
        show_page(&reopened, "media viewer hovers");
        for (label, value) in fixture["labels"]
            .as_array()
            .unwrap()
            .iter()
            .zip(event["reopened"].as_array().unwrap())
        {
            assert_eq!(
                row(&reopened, label.as_str().unwrap()).1.checked,
                value.as_bool().unwrap()
            );
        }
        reopened.invoke_cancel();
    }
    // Both nested and flat routes retain real native/setting consumers.
    for action in fixture["window_actions"].as_array().unwrap() {
        let id = match action["label"].as_str().unwrap() {
            "always on top" => 0,
            "always on top (while playing)" => 1,
            "remove titlebar/frame" => 2,
            label => panic!("Unknown recorded window action {label}"),
        };
        viewer.invoke_eye_menu_chosen(id);
        assert_eq!(
            json!([
                viewer.get_viewer_window_top(),
                viewer.get_viewer_top_while_playing(),
                viewer.get_viewer_window_frameless()
            ]),
            action["state"]
        );
    }
    viewer.invoke_eye_menu_chosen(6);
    assert!(!viewer.get_draw_tags_background());
    assert!(
        !store
            .read(settings::get::<ViewerBackgroundSettings>)
            .unwrap()
            .tags
    );
    viewer.invoke_eye_menu_chosen(12);
    assert!(!viewer.get_hover_tags_enabled());
    assert!(
        !store
            .read(settings::get::<ViewerHoverSettings>)
            .unwrap()
            .tags
    );
    viewer.invoke_eye_menu_chosen(15);
    assert_eq!(viewer.get_transparency_mode(), 1);
    viewer.invoke_eye_menu_chosen(16);
    assert_eq!(viewer.get_transparency_mode(), 2);
    let canvas: ViewerCanvasSettings = store.read(settings::get).unwrap();
    assert!(canvas.transparency_checkerboard && canvas.transparency_greenscreen);
    // Changing a new-viewer default while Options is open must survive applying
    // a separate collapse preference from that detached snapshot.
    open(&ui);
    let options = bound.options.borrow().as_ref().unwrap().clone_strong();
    show_page(&options, "media viewer hovers");
    options.invoke_check_toggled(
        row(&options, fixture["labels"][0].as_str().unwrap()).0,
        false,
    );
    viewer.invoke_eye_menu_chosen(5);
    options.invoke_apply();
    assert!(
        store
            .read(settings::get::<ViewerEyeMenuSettings>)
            .unwrap()
            .start_frameless
    );
    assert!(viewer.get_viewer_window_frameless());
    viewer
        .window()
        .dispatch_event(slint::platform::WindowEvent::PointerMoved {
            position: slint::LogicalPosition::new(500.0, 8.0),
        });
    let pixels = headless::render(&drawn, 1000, 750);
    headless::save_png(
        &std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("viewer-eye-menu.png"),
        &pixels,
        1000,
        750,
    )
    .unwrap();
    let before: ViewerEyeMenuSettings = store.read(settings::get).unwrap();
    viewer.invoke_close_requested();
    viewer.invoke_eye_menu_chosen(5);
    viewer.invoke_eye_menu_requested();
    assert_eq!(
        store.read(settings::get::<ViewerEyeMenuSettings>).unwrap(),
        before
    );
    ui.invoke_thumbnail_activated(0);
    let reopened = bound.viewer.borrow().as_ref().unwrap().clone_strong();
    assert!(reopened.get_viewer_window_frameless());
    reopened.invoke_eye_menu_requested();
    assert!(!reopened.get_eye_menu().collapse_window);
    reopened.invoke_close_requested();
}

#[test]
fn eye_menu_mixed_root_boundaries_match_the_recorded_menu_and_real_declaration_order() {
    let fixture = hydrus_testkit::fixture_json("viewer_eye_menu.json");
    let source = include_str!("../../ui/viewer_eye_menu.slint");
    for (previous, group, first_label, previous_flag, group_flag) in [
        (
            "window",
            "hovers",
            "draw tags (left) in the background",
            0,
            1,
        ),
        (
            "hovers",
            "rendering",
            "apply image ICC Profile colour adjustments",
            1,
            2,
        ),
    ] {
        // Inspect the actual consumer declarations: the earlier implementation
        // put this separator after the submenu, despite correct row models.
        // This guards that concrete source defect without presenting a separate
        // test-only layout as the materialized native menu.
        let boundary = format!(
            "if !root.menu.collapse-{previous} || !root.menu.collapse-{group}: MenuSeparator"
        );
        let before = source.find(&boundary).unwrap();
        let submenu = source
            .find(&format!("if root.menu.collapse-{group}: Menu"))
            .unwrap();
        let previous_flat = source
            .find(&format!(
                "for row in root.menu.collapse-{previous} ? [] : root.menu.{previous}.g2"
            ))
            .unwrap();
        assert!(previous_flat < before && before < submenu);
        for event in fixture["events"].as_array().unwrap() {
            let entries = event["menu"].as_array().unwrap();
            let at = entries
                .iter()
                .position(|entry| entry["menu"] == group || entry["check"] == first_label)
                .unwrap();
            let separated = entries[at - 1] == "---";
            assert_eq!(
                separated,
                !event["values"][previous_flag].as_bool().unwrap()
                    || !event["values"][group_flag].as_bool().unwrap(),
                "{event}"
            );
        }
    }
}

#[test]
fn related_weight_drafts_cancel_reopen_and_re_rank_an_already_open_service_panel() {
    use hydrus_store::related_tags::{Settings as Related, Weights};
    let (_dirs, store) = store();
    let f = hydrus_testkit::fixture_json("related_tag_weights.json");
    let files: Vec<hydrus_core::HashId> = store
        .read(|conn| {
            Ok(conn
                .prepare("SELECT hash_id FROM files ORDER BY hash_id LIMIT 6")?
                .query_map([], |r| r.get(0))?
                .collect::<rusqlite::Result<_>>()?)
        })
        .unwrap();
    assert_eq!(files.len(), 6);
    for (tag, indices) in [
        ("source:seed", vec![0, 1, 2, 3]),
        ("context:seed", vec![2, 3, 4, 5]),
        ("alpha:first", vec![0, 1, 2]),
        ("beta:second", vec![3, 4, 5]),
        ("alpha:third", vec![4, 5]),
        ("zero:hidden", vec![0, 1]),
    ] {
        let mut model = hydrus_gui::manage_tags::ManageTags::new(
            store.clone(),
            indices.into_iter().map(|i| files[i]).collect(),
        )
        .unwrap();
        let i = model
            .service_names()
            .iter()
            .position(|n| n == "second tags")
            .unwrap();
        model.choose_service(i).unwrap();
        model.add_side_suggestions(&[tag.into()]);
        model.apply().unwrap();
    }
    let weights = Weights {
        search: serde_json::from_value(f["ranking"][0]["search_weights"].clone()).unwrap(),
        result: serde_json::from_value(f["ranking"][0]["result_weights"].clone()).unwrap(),
    };
    let initial = weights.clone();
    store
        .write(move |ctx| {
            hydrus_store::settings::set(
                ctx.conn(),
                &Related {
                    weights,
                    ..Related::default()
                },
            )?;
            hydrus_store::settings::set(
                ctx.conn(),
                &hydrus_store::settings::TagSuggestionSettings {
                    default_page: "related".into(),
                    recent_limit: None,
                    ..hydrus_store::settings::TagSuggestionSettings::default()
                },
            )
        })
        .unwrap();
    let windows = headless::init();
    let mut page = hydrus_gui::SearchPage::new(store.clone());
    // The Qt recorder passes all six media explicitly. The basic fixture's
    // second file belongs only to "art", so the default "my files" search
    // cannot supply the same six-target selection.
    page.choose_location(hydrus_core::search::context::LocationContext::default());
    page.enter();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::single(page));
    ui.show().unwrap();
    bound.current.borrow().borrow_mut().select_files(&files);
    assert_eq!(
        bound
            .current
            .borrow()
            .borrow()
            .selected_files()
            .into_iter()
            .collect::<std::collections::BTreeSet<_>>(),
        files
            .iter()
            .copied()
            .collect::<std::collections::BTreeSet<_>>(),
        "the real manage-tags owner must capture all six recorded media"
    );
    ui.invoke_manage_tags_selected();
    let manage = bound.manage_tags.borrow().as_ref().unwrap().clone_strong();
    let second = i32::try_from(
        manage
            .get_service_names()
            .iter()
            .position(|n| n == "second tags")
            .unwrap(),
    )
    .unwrap();
    assert_eq!(manage.get_window_title(), "manage tags for 6 files");
    manage.invoke_service_chosen(second);
    manage.set_related_display(false);
    manage.invoke_related_search();
    let mine = i32::try_from(
        manage
            .get_service_names()
            .iter()
            .position(|n| n == "my tags")
            .unwrap(),
    )
    .unwrap();
    for _ in 0..3 {
        manage.invoke_service_chosen(mine);
        manage.invoke_service_chosen(second);
    }
    let related_rows = || {
        manage
            .get_related_tag_rows()
            .iter()
            .map(|row| row.cells.row_data(0).unwrap().to_string())
            .collect::<Vec<_>>()
    };
    let wait_for = |first: &str| {
        for _ in 0..600 {
            slint::platform::update_timers_and_animations();
            if related_rows().first().is_some_and(|row| row == first) {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        panic!(
            "related query did not settle: {:?} / {}",
            related_rows(),
            manage.get_related_status()
        );
    };
    wait_for("alpha:first (1,154)");
    assert_eq!(
        related_rows(),
        [
            "alpha:first (1,154)",
            "beta:second (1,154)",
            "alpha:third (707)"
        ]
    );
    assert_eq!(manage.get_suggested_page(), 2);
    assert_eq!(
        serde_json::json!(
            related_rows()
                .into_iter()
                .map(|row| row.split(" (").next().unwrap().to_owned())
                .collect::<Vec<_>>()
        ),
        f["consumer"]["initial"]
    );
    let related_window = windows.get(windows.count() - 1).unwrap();
    let pixels = headless::render(&related_window, 1100, 700);
    headless::save_png(
        &std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("related_tag_suggestions.png"),
        &pixels,
        1100,
        700,
    )
    .unwrap();
    open(&ui);
    let options = bound.options.borrow().as_ref().unwrap().clone_strong();
    show_page(&options, "tag suggestions");
    options.invoke_related_weights_clicked();
    let child = bound
        .options_suggested_tags_slot
        .weights
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    let count = child.get_rows().row_count();
    let selection_fixture = &f["selection_buttons"];
    for (index, case) in selection_fixture.as_array().unwrap().iter().enumerate() {
        if index > 0 {
            let label = if index == 1 {
                "'source' tags"
            } else {
                "'context' tags"
            };
            let row = child
                .get_rows()
                .iter()
                .position(|row| row.cells.row_data(0).unwrap() == label)
                .unwrap();
            child.invoke_clicked(i32::try_from(row).unwrap(), index == 2, false);
        }
        assert_eq!(
            child.get_can_edit(),
            case["buttons"]["edit"].as_bool().unwrap()
        );
        assert_eq!(
            child.get_can_delete(),
            case["buttons"]["delete"].as_bool().unwrap()
        );
    }

    child.invoke_action("add".into());
    child.set_namespace(":".into());
    child.invoke_action("accept-question".into());
    assert_eq!(
        child.get_error(),
        hydrus_gui_model::related_weights::RESERVED
    );
    assert_eq!(child.get_rows().row_count(), count);
    child.invoke_action("add".into());
    child.set_namespace("probe".into());
    child.invoke_action("accept-question".into());
    child.set_weight(0);
    child.invoke_action("cancel-question".into());
    assert_eq!(child.get_rows().row_count(), count);
    let protected = child
        .get_rows()
        .iter()
        .position(|row| row.cells.row_data(0).unwrap() == "unnamespaced tags")
        .unwrap();
    child.invoke_clicked(i32::try_from(protected).unwrap(), false, false);
    assert!(!child.get_can_delete());
    child.invoke_action("delete".into());
    assert_eq!(child.get_rows().row_count(), count);
    child.invoke_action("add".into());
    child.set_namespace("probe".into());
    child.invoke_action("accept-question".into());
    assert_eq!(child.get_question(), "set weight");
    assert_eq!(child.get_weight(), 100);
    child.set_weight(10_000);
    child.invoke_action("accept-question".into());
    options.invoke_apply();
    assert!(
        bound.options.borrow().is_some(),
        "owner Apply is blocked by its child"
    );
    let child_window = windows.get(windows.count() - 1).unwrap();
    let pixels = headless::render(&child_window, 650, 520);
    headless::save_png(
        &std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("related_tag_weights.png"),
        &pixels,
        650,
        520,
    )
    .unwrap();
    child.invoke_action("apply".into());
    options.invoke_cancel();
    assert_eq!(
        store
            .read(hydrus_store::settings::get::<Related>)
            .unwrap()
            .weights,
        initial
    );
    // A retained component handle cannot publish after its owner/slot retired.
    child.invoke_action("add".into());
    child.invoke_action("apply".into());
    assert!(bound.options_suggested_tags_slot.weights.borrow().is_none());
    open(&ui);
    let options = bound.options.borrow().as_ref().unwrap().clone_strong();
    show_page(&options, "tag suggestions");
    options.invoke_related_weights_clicked();
    let child = bound
        .options_suggested_tags_slot
        .weights
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    child.invoke_choose(true);
    let edit = |slice: &str, weight| {
        let index = child
            .get_rows()
            .iter()
            .position(|row| row.cells.row_data(0).unwrap() == slice)
            .unwrap();
        child.invoke_clicked(i32::try_from(index).unwrap(), false, false);
        child.invoke_action("edit".into());
        assert_eq!(child.get_question(), "edit weight");
        child.set_weight(weight);
        child.invoke_action("accept-question".into());
    };
    edit("'alpha' tags", 50);
    edit("'beta' tags", 400);
    child.invoke_action("apply".into());
    assert_eq!(
        store
            .read(hydrus_store::settings::get::<Related>)
            .unwrap()
            .weights,
        initial,
        "child acceptance only stages"
    );
    options.invoke_apply();
    wait_for("beta:second (4,616)");
    assert_eq!(
        related_rows(),
        [
            "beta:second (4,616)",
            "alpha:first (577)",
            "alpha:third (353)"
        ]
    );
    let persisted: Related = hydrus_store::Store::open(store.dir())
        .unwrap()
        .read(hydrus_store::settings::get)
        .unwrap();
    let expected: Vec<(String, u16)> =
        serde_json::from_value(f["ranking"][2]["result_weights"].clone()).unwrap();
    assert_eq!(
        persisted
            .weights
            .result
            .into_iter()
            .collect::<std::collections::BTreeMap<_, _>>(),
        expected
            .into_iter()
            .collect::<std::collections::BTreeMap<_, _>>()
    );
    manage.invoke_side_clicked(2, 0, false, false);
    manage.invoke_side_activated(2, 0);
    let committed = || {
        store.read(|conn| {let id=store.snapshot().services.by_name("second tags").unwrap().id;let table=hydrus_store::schema::MappingTables::new(id).current;Ok(conn.query_row(&format!("SELECT COUNT(*) FROM {table} WHERE tag_id=(SELECT tag_id FROM tags WHERE namespace_id=(SELECT namespace_id FROM namespaces WHERE namespace='beta') AND subtag_id=(SELECT subtag_id FROM subtags WHERE subtag='second'))"),[],|r|r.get::<_,i64>(0))?)}).unwrap()
    };
    assert_eq!(committed(), 3, "activation stays staged");
    manage.invoke_apply();
    assert_eq!(committed(), i64::try_from(files.len()).unwrap());
    manage.invoke_related_search();
    assert_eq!(committed(), 6, "retired request cannot add again");
    open(&ui);
    let options = bound.options.borrow().as_ref().unwrap().clone_strong();
    show_page(&options, "tag suggestions");
    options.invoke_related_weights_clicked();
    let child = bound
        .options_suggested_tags_slot
        .weights
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    assert_eq!(child.get_rows().row_count(), initial.search.len());
    child.invoke_choose(true);
    assert!(
        child
            .get_rows()
            .iter()
            .any(|row| row.cells.row_data(0).unwrap() == "'beta' tags"
                && row.cells.row_data(1).unwrap() == "400%")
    );
    options.invoke_cancel();
    assert!(bound.options_suggested_tags_slot.weights.borrow().is_none());
}
