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
            "file sort/collect",
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
            by: system(SortBy::ImportTime),
            ascending: true
        }
    );
    assert_eq!(
        after.fallback_sort,
        PageSort {
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
            by: system(SortBy::Width),
            ascending: true
        }
    );
    ui.invoke_order_chosen(1);
    assert_eq!(
        sorts().default_sort,
        PageSort {
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
