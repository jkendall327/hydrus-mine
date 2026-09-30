//! A search page over the `basic` fixture: driven directly, then shown in a
//! window drawn headless (the screenshot is kept in the target directory).

use std::cell::RefCell;
use std::rc::Rc;

use slint::{ComponentHandle as _, Model as _};

use hydrus_gui::{MainWindow, SearchPage, bind, headless};
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
    page.add_predicate("system:everything");
    let everything = page.results().len();
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

    let thumbnails = page
        .results()
        .iter()
        .filter_map(|&id| page.thumbnail(id))
        .count();
    assert!(thumbnails > 0);

    // the window
    let window = headless::init();
    let ui = MainWindow::new().unwrap();
    bind(&ui, Rc::new(RefCell::new(SearchPage::new(store))));
    ui.invoke_search_edited("system:everything".into());
    ui.invoke_search_accepted();
    assert_eq!(ui.get_status(), format!("{everything} files"));
    assert_eq!(ui.get_predicates().row_count(), 1);
    ui.invoke_thumbnail_clicked(0);
    // (the screenshot shows the autocomplete too)
    ui.invoke_search_edited("samus".into());
    assert_eq!(ui.get_search_text(), "samus");
    assert!(ui.get_suggestions().row_count() > 0);
    assert_eq!(ui.get_highlighted(), 0);
    ui.show().unwrap();
    let (width, height) = (1100, 700);
    let pixels = headless::render(&window, width, height);

    let screenshot = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("search_page.png");
    let mut encoder = png::Encoder::new(
        std::io::BufWriter::new(std::fs::File::create(&screenshot).unwrap()),
        width,
        height,
    );
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder
        .write_header()
        .unwrap()
        .write_image_data(&pixels)
        .unwrap();

    // thumbnails were drawn: far more colours than the window's own few
    let colours: std::collections::HashSet<&[u8]> = pixels.chunks(4).collect();
    assert!(colours.len() > 1000, "{} colours", colours.len());
}
