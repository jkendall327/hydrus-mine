//! A search page over the `basic` fixture: driven directly, then shown in a
//! window drawn headless (the screenshot is kept in the target directory).

use slint::{ComponentHandle as _, Model as _};

use hydrus_gui::{MainWindow, MediaViewer, Pages, Palette, SearchPage, bind, headless};
use hydrus_search::{SortBy, SortOrder};
use hydrus_store::Store;
use hydrus_store::import::import_legacy;
use slint::language::ColorScheme;

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

    // the tag list: every file's tags with how many have each, sorted as the
    // reference sorts them (by the user's namespaces, then a-z)
    let rows: Vec<String> = page.tag_rows().iter().map(|r| (*r).to_owned()).collect();
    assert!(rows.len() > 10, "{rows:?}");
    assert!(rows.iter().all(|r| r.ends_with(')')), "{rows:?}");
    let series = rows.iter().position(|r| r.starts_with("series:"));
    let character = rows.iter().position(|r| r.starts_with("character:"));
    if let (Some(series), Some(character)) = (series, character) {
        assert!(series < character, "{rows:?}");
    }
    // with a file selected, that file's tags, one each
    page.select(0);
    let selected: Vec<String> = page.tag_rows().iter().map(|r| (*r).to_owned()).collect();
    assert!(!selected.is_empty() && selected.len() < rows.len());
    assert!(selected.iter().all(|r| r.ends_with(" (1)")), "{selected:?}");
    // double-clicking a tag searches for it too
    assert!(page.activate_tag(0));
    assert_eq!(page.predicates().len(), 2);
    assert!(page.results().len() < everything);
    page.remove_predicate(1);
    assert_eq!(page.tag_rows().len(), rows.len());

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
    // listed as the reference writes them
    page.add_predicate("system:width>1920");
    assert_eq!(page.predicates()[1], "system:width > 1,920");
    page.add_predicate("system:width > 1920");
    assert_eq!(
        page.predicates().len(),
        2,
        "the same predicate, typed differently"
    );
    page.remove_predicate(1);

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
    // what plays in mpv, as the reference's defaults have it: video, audio
    // and animations (but not animated WebP), not stills or documents
    let results = page.results().to_vec();
    let infos = store
        .read(|conn| hydrus_store::media::load_basic(conn, &results))
        .unwrap();
    let mut kinds = std::collections::BTreeMap::new();
    for (index, info) in infos.iter().enumerate() {
        let mime = info.info.as_ref().unwrap().mime;
        let viewer = MediaViewer::new(store.clone(), results.clone(), index).unwrap();
        let playable = viewer.playable();
        if let Some(path) = &playable {
            assert!(path.exists(), "{path:?}");
        }
        kinds.insert(mime.human_name(), playable.is_some());
    }
    for (kind, plays) in [
        ("mp3", true),
        ("flac", true),
        ("animated gif", true),
        ("apng", true),
        ("jpeg", false),
        ("static gif", false),
        ("pdf", false),
        ("zip", false),
    ] {
        assert_eq!(kinds.get(kind), Some(&plays), "{kind}: {kinds:?}");
    }

    // the window
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let main_window = windows.get(0).unwrap();
    let bound = bind(&ui, Pages::single(SearchPage::new(store)));
    ui.invoke_search_edited("system:everything".into());
    ui.invoke_search_accepted();
    assert_eq!(ui.get_status(), format!("{everything} files"));
    assert_eq!(ui.get_predicates().row_count(), 1);
    assert!(ui.get_tags().row_count() > 10);
    ui.invoke_thumbnail_clicked(1);
    let selected_tags = ui.get_tags().row_count();
    assert!(selected_tags > 0 && selected_tags < 10);
    // a row is made at once, with blanks until its thumbnails are decoded
    // (off the UI thread)
    let sizes = |rows: &hydrus_gui::ThumbnailRows| -> Vec<u32> {
        let row = rows.row_data(0).unwrap();
        (0..row.thumbnails.row_count())
            .map(|i| row.thumbnails.row_data(i).unwrap().image.size().width)
            .collect()
    };
    assert!(sizes(&bound.rows).iter().all(|&w| w == 0));
    bound.rows.wait();
    assert!(sizes(&bound.rows).iter().any(|&w| w > 0));
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
    // (the second asks for the thumbnails in view, decoded off the UI
    // thread; the third shows them)
    headless::render(&main_window, width, height);
    bound.rows.wait();
    let pixels = headless::render(&main_window, width, height);
    // only the rows in view were decoded
    let cached = bound.rows.cached();
    assert!(cached > 0 && cached < everything, "{bound:?}");
    let shots = std::path::Path::new(env!("CARGO_TARGET_TMPDIR"));
    headless::save_png(&shots.join("search_page.png"), &pixels, width, height).unwrap();
    // thumbnails were drawn: far more colours than the window's own few
    let colours: std::collections::HashSet<&[u8]> = pixels.chunks(4).collect();
    assert!(colours.len() > 1000, "{} colours", colours.len());

    // the same in dark mode: every colour the window draws follows it
    ui.global::<Palette<'_>>()
        .set_color_scheme(ColorScheme::Dark);
    let dark = headless::render(&main_window, width, height);
    headless::save_png(&shots.join("search_page_dark.png"), &dark, width, height).unwrap();
    let light_pixels = |pixels: &[u8]| {
        pixels
            .chunks(4)
            .filter(|p| p[0] > 200 && p[1] > 200 && p[2] > 200)
            .count()
    };
    assert!(
        light_pixels(&dark) < light_pixels(&pixels) / 4,
        "{} light pixels in dark mode, {} in light",
        light_pixels(&dark),
        light_pixels(&pixels)
    );
    ui.global::<Palette<'_>>()
        .set_color_scheme(ColorScheme::Light);

    // a click on a thumbnail selects it; a double-click opens the viewer on
    // its file (the first click redraws the row, which mustn't lose it)
    let grid_left = 300.0;
    let third = slint::LogicalPosition::new(grid_left + 4.0 + 2.0 * 154.0 + 75.0, 4.0 + 75.0);
    let click = |at: slint::LogicalPosition| {
        use slint::platform::{PointerEventButton, WindowEvent};
        let window = ui.window();
        window.dispatch_event(WindowEvent::PointerMoved { position: at });
        window.dispatch_event(WindowEvent::PointerPressed {
            position: at,
            button: PointerEventButton::Left,
        });
        window.dispatch_event(WindowEvent::PointerReleased {
            position: at,
            button: PointerEventButton::Left,
        });
    };
    click(third);
    headless::render(&main_window, width, height);
    assert!(bound.viewer.borrow().is_none(), "one click only selects");
    assert_eq!(windows.count(), 1);
    click(third);
    headless::render(&main_window, width, height);
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
    viewer.invoke_close_requested();
    assert!(bound.viewer.borrow().is_none());
}

#[test]
fn system_predicate_counts_are_for_the_page_s_file_domain() {
    use hydrus_core::search::context::{LocationContext, TagContext};
    use hydrus_core::service::ServiceType;
    use hydrus_search::FileSearchContext;

    let legacy = hydrus_testkit::legacy_fixture("basic");
    let native = tempfile::tempdir().unwrap();
    import_legacy(
        legacy.path(),
        &native.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store = Store::open(native.path()).unwrap();
    let snapshot = store.snapshot();
    let mut found = Vec::new();
    // ("my files" and "art" hold different files)
    for domain in snapshot.services.of_type(ServiceType::LocalFileDomain) {
        let search = FileSearchContext {
            location: LocationContext::single(domain.key.clone()),
            tags: TagContext::default(),
            predicates: Vec::new(),
        };
        let mut page = SearchPage::restored(store.clone(), search, true, None, Vec::new());
        let labels: Vec<String> = page
            .autocomplete()
            .suggestions()
            .iter()
            .map(|s| s.label.clone())
            .collect();
        let mut count = |predicate: &str| {
            page.add_predicate(predicate);
            let n = page.results().len();
            page.remove_predicate(0);
            n
        };
        let (everything, inbox, archive) = (
            count("system:everything"),
            count("system:inbox"),
            count("system:archive"),
        );
        let with_count = |predicate: &str, n: usize| {
            if n == 0 {
                predicate.to_owned()
            } else {
                format!("{predicate} ({n})")
            }
        };
        assert_eq!(
            labels,
            [
                with_count("system:everything", everything),
                with_count("system:inbox", inbox),
                with_count("system:archive", archive),
            ],
            "{}",
            domain.name
        );
        found.push(everything);
    }
    found.sort_unstable();
    found.dedup();
    assert!(found.len() > 1, "the domains differ: {found:?}");
}

#[test]
fn tags_are_shown_as_the_user_has_them_shown() {
    use hydrus_core::ServiceKey;
    use hydrus_core::service::builtin_keys;
    use hydrus_core::tag_filter::{FilterRule, TagFilter};
    use hydrus_core::tag_presentation::TagPresentation;
    use hydrus_store::tag_display::{TagDisplayFilters, TagView};

    let legacy = hydrus_testkit::legacy_fixture("basic");
    let native = tempfile::tempdir().unwrap();
    import_legacy(
        legacy.path(),
        &native.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store = Store::open(native.path()).unwrap();
    let mut page = SearchPage::new(store.clone());
    page.enter();
    let rows: Vec<String> = page.tag_rows().iter().map(|r| (*r).to_owned()).collect();
    assert!(rows.iter().any(|r| r.starts_with("character:")), "{rows:?}");
    assert!(rows.iter().any(|r| r.starts_with("series:")), "{rows:?}");

    // namespaces hidden, and character tags hidden from the tag list
    let presentation = TagPresentation {
        show_namespaces: false,
        ..TagPresentation::default()
    };
    let mut filters = TagDisplayFilters::default();
    filters.for_view_mut(TagView::SelectionList).insert(
        ServiceKey::new(builtin_keys::COMBINED_TAG.to_vec()).to_hex(),
        TagFilter::new().with_rule("character:", FilterRule::Blacklist),
    );
    store
        .write(move |ctx| {
            hydrus_store::settings::set(ctx.conn(), &presentation)?;
            hydrus_store::settings::set(ctx.conn(), &filters)
        })
        .unwrap();
    let mut page = SearchPage::new(store.clone());
    assert_eq!(
        page.autocomplete().suggestions()[0].label.split(' ').next(),
        Some("everything")
    );
    page.enter();
    assert_eq!(page.predicates(), ["everything"]);
    let hidden: Vec<String> = page.tag_rows().iter().map(|r| (*r).to_owned()).collect();
    // (namespaces stay on number subtags, as hydrus's
    // `show_subtag_number_namespaces` has it)
    assert!(
        hidden
            .iter()
            .all(|r| !r.starts_with("series:") && !r.starts_with("character:")),
        "{hidden:?}"
    );
    assert!(hidden.iter().any(|r| r.starts_with("page:")), "{hidden:?}");
    let series = rows
        .iter()
        .find(|r| r.starts_with("series:"))
        .unwrap()
        .strip_prefix("series:")
        .unwrap();
    assert!(hidden.iter().any(|r| r == series), "{series} in {hidden:?}");
    let characters = rows.iter().filter(|r| r.starts_with("character:")).count();
    assert_eq!(hidden.len(), rows.len() - characters);
    // a tag in the list still searches as itself
    let index = hidden.iter().position(|r| r == series).unwrap();
    assert!(page.activate_tag(index));
    assert_eq!(
        page.predicates()[1],
        series.rsplit_once(" (").unwrap().0,
        "shown without its namespace"
    );
}
