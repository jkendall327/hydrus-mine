//! A search page over the `basic` fixture: driven directly, then shown in a
//! window drawn headless (the screenshot is kept in the target directory).

use std::cell::RefCell;
use std::rc::Rc;

use slint::{ComponentHandle as _, Model as _};

use hydrus_gui::{MainWindow, MediaViewer, SearchPage, bind, headless};
use hydrus_search::{SortBy, SortOrder};
use hydrus_store::Store;
use hydrus_store::import::import_legacy;

#[test]
fn a_search_page_finds_files_and_shows_their_thumbnails() {
    let legacy = hydrus_testkit::legacy_fixture("basic");
    let native = tempfile::tempdir().unwrap();
    import_legacy(
        legacy.path(),
        &native.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store = Store::open(native.path()).unwrap();

    let mut page = SearchPage::new(store.clone());
    assert!(page.results().is_empty(), "nothing until searched");
    // before anything is typed, the system predicates that need nothing more
    let offered: Vec<String> = page
        .autocomplete()
        .suggestions()
        .iter()
        .map(|s| s.predicate.clone())
        .collect();
    assert_eq!(
        offered,
        ["system:everything", "system:inbox", "system:archive"]
    );
    page.enter();
    let everything = page.results().len();
    assert_eq!(page.predicates(), ["system:everything"]);
    assert_eq!(
        page.autocomplete().suggestions()[0].label,
        format!("system:everything ({everything})")
    );
    assert!(everything > 10, "{everything}");
    assert_eq!(page.status(), format!("{everything} files"));

    page.add_predicate("system:nonsense");
    assert!(page.error().is_some());
    assert_eq!(page.predicates(), ["system:everything"]);
    assert_eq!(
        page.results().len(),
        everything,
        "a refused predicate changes nothing"
    );

    page.add_predicate("system:inbox");
    assert!(page.error().is_none());
    let inbox = page.results().len();
    assert!(inbox <= everything);
    page.remove_predicate(1);
    assert_eq!(page.results().len(), everything);

    // autocomplete: tags matching what's typed, with counts; enter adds the
    // highlighted one and empties the box
    page.type_text("samus");
    let suggestions = page.autocomplete().suggestions().to_vec();
    assert!(!suggestions.is_empty());
    for s in &suggestions {
        assert!(s.label.starts_with(&s.predicate), "{s:?}");
        assert!(s.label.ends_with(')'), "{s:?}");
    }
    page.move_highlight(1);
    let highlighted = page.autocomplete().highlighted().unwrap();
    assert_eq!(highlighted, 1.min(suggestions.len() - 1));
    page.enter();
    assert!(page.error().is_none(), "{:?}", page.error());
    assert_eq!(page.predicates()[1], suggestions[highlighted].predicate);
    assert_eq!(page.autocomplete().text(), "");
    assert!(page.results().len() <= everything);
    page.remove_predicate(1);
    // a leading hyphen excludes
    page.type_text("-samus");
    assert!(
        page.autocomplete().suggestions()[0]
            .predicate
            .starts_with('-')
    );
    page.type_text("");

    // sorting: a new type comes with its default order
    let newest_first = page.results().to_vec();
    page.set_sort_order(SortOrder::Ascending);
    let oldest_first: Vec<_> = page.results().to_vec();
    assert_eq!(
        oldest_first.iter().rev().copied().collect::<Vec<_>>(),
        newest_first
    );
    page.set_sort_by(SortBy::FileSize);
    assert_eq!(page.sort().order, SortOrder::Descending, "largest first");
    assert_eq!(page.results().len(), everything);
    page.set_sort_by(SortBy::Width);
    assert_eq!(page.sort().order, SortOrder::Ascending, "slimmest first");
    page.set_sort_by(SortBy::ImportTime);
    assert_eq!(page.results(), newest_first);

    let thumbnails = page
        .results()
        .iter()
        .filter_map(|&id| page.thumbnail(id))
        .count();
    assert!(thumbnails > 0);

    // the media viewer, from the third file: through the files and round
    let mut viewer = MediaViewer::new(store.clone(), page.results().to_vec(), 2).unwrap();
    assert_eq!(viewer.caption(), format!("3/{everything}"));
    assert!(viewer.media().is_some());
    viewer.previous();
    viewer.previous();
    viewer.previous();
    assert_eq!(
        viewer.index(),
        everything - 1,
        "back past the first to the last"
    );
    viewer.next();
    assert_eq!(viewer.index(), 0);
    assert!(MediaViewer::new(store.clone(), Vec::new(), 0).is_none());

    // the window
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let main_window = windows.get(0).unwrap();
    let bound = bind(&ui, Rc::new(RefCell::new(SearchPage::new(store))));
    ui.invoke_search_edited("system:everything".into());
    ui.invoke_search_accepted();
    assert_eq!(ui.get_status(), format!("{everything} files"));
    assert_eq!(ui.get_predicates().row_count(), 1);
    let sort_names = ui.get_sort_names();
    let import_time = (0..sort_names.row_count())
        .find(|&i| sort_names.row_data(i).unwrap() == "time: import time")
        .unwrap();
    assert_eq!(ui.get_sort_index(), i32::try_from(import_time).unwrap());
    assert_eq!(ui.get_order_names().row_data(1).unwrap(), "newest first");
    assert_eq!(ui.get_order_index(), 1);
    ui.invoke_thumbnail_clicked(0);
    // (the screenshot shows the autocomplete too)
    ui.invoke_search_edited("samus".into());
    assert_eq!(ui.get_search_text(), "samus");
    assert!(ui.get_suggestions().row_count() > 0);
    assert_eq!(ui.get_highlighted(), 0);
    ui.show().unwrap();
    let (width, height) = (1100, 700);
    // (the first frame lays the grid out, which says how many thumbnails fit
    // in a row; the second shows them)
    headless::render(&main_window, width, height);
    assert!(ui.get_grid_columns() > 1);
    let pixels = headless::render(&main_window, width, height);
    // only the rows in view were decoded
    let cached = bound.rows.cached();
    assert!(cached > 0 && cached < everything, "{bound:?}");
    let shots = std::path::Path::new(env!("CARGO_TARGET_TMPDIR"));
    headless::save_png(&shots.join("search_page.png"), &pixels, width, height).unwrap();
    // thumbnails were drawn: far more colours than the window's own few
    let colours: std::collections::HashSet<&[u8]> = pixels.chunks(4).collect();
    assert!(colours.len() > 1000, "{} colours", colours.len());

    // double-clicking a thumbnail opens the viewer on its file
    ui.invoke_thumbnail_activated(2);
    let viewer = bound
        .viewer
        .borrow()
        .as_ref()
        .map(slint::ComponentHandle::clone_strong)
        .expect("a viewer opened");
    assert_eq!(windows.count(), 2);
    assert_eq!(viewer.get_caption(), format!("3/{everything}"));
    let pixels = headless::render(&windows.get(1).unwrap(), 800, 600);
    headless::save_png(&shots.join("media_viewer.png"), &pixels, 800, 600).unwrap();
    // the file fills most of the window, over its dark background
    let background = [0x20, 0x20, 0x20, 0xff];
    let covered = pixels.chunks(4).filter(|p| *p != background).count();
    assert!(covered > 800 * 600 / 2, "{covered} pixels");
    viewer.invoke_next();
    assert_eq!(viewer.get_caption(), format!("4/{everything}"));
    viewer.invoke_close();
    assert!(bound.viewer.borrow().is_none());
}
