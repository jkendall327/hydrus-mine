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
            "file sort/collect",
            "file viewing statistics",
            "files and trash",
            "gui",
            "gui pages",
            "gui sessions",
            "importing",
            "maintenance and processing",
            "media playback",
            "media viewer",
            "media viewer hovers",
            "ratings",
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
        let (row, _) = row(&window, "Application display name: ");
        window.invoke_text_edited(row, name["typed"].as_str().unwrap().into());
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
    let (row, _) = row(&window, "Confirm client exit: ");
    window.invoke_check_toggled(row, true);
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
        let (row, _) = row(&window, "Confirm client exit: ");
        window.invoke_check_toggled(row, case["confirm"].as_bool().unwrap());
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
