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

    // its pages, as the reference lists them; it opens on gui
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
            "ratings",
            "regex favourites",
            "system",
            "tag editing",
            "tag presentation",
            "tag sort",
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
        ui.set_search_focus_requests(ui.get_search_focus_requests() + 1);
        // Layout publishes geometry from the conditional sidebar. Flush those
        // change handlers, then render its correctly placed overlay.
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
        let full = headless::render(&drawn, 800, 600);
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
        let small = headless::render(&drawn, 800, 600);
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
        })
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
            bound
                .viewer
                .borrow()
                .as_ref()
                .unwrap()
                .invoke_close_requested();
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
            })
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
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    for apply in [false, true] {
        open(&ui);
        let parent = bound.options.borrow().as_ref().unwrap().clone_strong();
        show_page(&parent, "tag autocomplete tabs");
        assert_eq!(row(&parent, "These tags will appear in every tag autocomplete results dropdown, under the 'favourites' tab.").1.kind, 16);
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
        let pixels = headless::render(&drawn, 1000, 750);
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
        let pixels = headless::render(&drawn, 1000, 750);
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
        window.invoke_number_edited(at, i32::try_from(state["given"].as_i64().unwrap()).unwrap());
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
