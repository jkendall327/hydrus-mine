//! Native display controls, detached drafts, rendering and live consumers.
use hydrus_core::url::{AnyGug, DomainMask, Gug, Gugs, UrlClass, UrlClassSettings};
use hydrus_gui::{
    MainWindow, Pages, SearchPage, bind, downloader_display_window as windows, headless,
};
use hydrus_gui_model::downloader_display::{Draft, ViewerUrls};
use hydrus_parse::Downloaders;
use hydrus_store::{Store, settings};
use slint::{ComponentHandle as _, Model as _};

fn install(store: &Store) {
    store
        .write_and_refresh(|ctx| {
            let d = Downloaders {
                gugs: Gugs {
                    gugs: [("alpha", 17), ("beta", 34)]
                        .into_iter()
                        .map(|(name, key)| {
                            AnyGug::Single(Gug {
                                name: name.into(),
                                key: hex::encode([key; 32]),
                                url_template: "https://example.com/search?q=%tags%".into(),
                                replacement_phrase: "%tags%".into(),
                                separator: "+".into(),
                                initial_search_text: String::new(),
                                example_search_text: String::new(),
                            })
                        })
                        .collect(),
                    keys_to_display: vec![hex::encode([17; 32])],
                },
                ..Downloaders::default()
            };
            settings::set(ctx.conn(), &d)?;
            let c = UrlClassSettings {
                url_classes: [
                    ("example post", 51, vec!["example.com"]),
                    ("multi post", 68, vec!["a.example", "b.example"]),
                ]
                .into_iter()
                .map(|(name, key, domains)| UrlClass {
                    name: name.into(),
                    key: vec![key; 32],
                    domain_mask: DomainMask::new(
                        domains.into_iter().map(str::to_owned).collect(),
                        vec![],
                        false,
                        false,
                    ),
                    ..UrlClass::default()
                })
                .collect(),
                ..UrlClassSettings::default()
            };
            settings::set(ctx.conn(), &c)?;
            settings::set(ctx.conn(), &ViewerUrls::default())
        })
        .unwrap();
}
fn cells(w: &hydrus_gui::DownloaderDisplayWindow) -> Vec<Vec<String>> {
    let rows = w.get_rows();
    (0..rows.row_count())
        .map(|i| {
            let cells = rows.row_data(i).unwrap().cells;
            (0..cells.row_count())
                .map(|j| cells.row_data(j).unwrap().to_string())
                .collect()
        })
        .collect()
}
fn until(mut condition: impl FnMut() -> bool) {
    let start = std::time::Instant::now();
    while !condition() {
        assert!(start.elapsed() < std::time::Duration::from_secs(5));
        slint::platform::update_timers_and_animations();
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}
#[test]
fn questions_cancel_apply_reopen_sort_selection_and_render() {
    let h = headless::init();
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    install(&store);
    let slots = windows::Slots::default();
    let w = windows::open(&store, &slots).unwrap();
    let f = hydrus_testkit::fixture_json("downloader_display.json");
    assert_eq!(
        serde_json::to_value(cells(&w)).unwrap(),
        f["initial"]["gugs"]
    );
    w.invoke_row_clicked(0, false, false);
    w.invoke_row_clicked(1, false, true);
    w.invoke_edit_clicked();
    assert_eq!(
        w.get_question(),
        f["questions"][0]["message"].as_str().unwrap()
    );
    assert_eq!(w.get_question_title(), "Show in the first list?");
    w.invoke_apply_clicked();
    assert!(
        w.window().is_visible(),
        "a pending question must be answered first"
    );
    w.invoke_answer(-1);
    assert_eq!(
        serde_json::to_value(cells(&w)).unwrap(),
        f["steps"][0]["gugs"]
    );
    w.invoke_edit_clicked();
    w.invoke_answer(1);
    assert_eq!(
        serde_json::to_value(cells(&w)).unwrap(),
        f["steps"][1]["gugs"]
    );
    w.invoke_edit_clicked();
    w.invoke_answer(0);
    assert_eq!(
        serde_json::to_value(cells(&w)).unwrap(),
        f["steps"][2]["gugs"]
    );
    w.set_tab(1);
    w.invoke_tab_changed();
    w.invoke_row_clicked(0, false, false);
    w.invoke_row_clicked(1, true, false);
    w.invoke_edit_clicked();
    assert_eq!(
        w.get_question(),
        f["questions"][3]["message"].as_str().unwrap()
    );
    w.invoke_answer(-1);
    assert_eq!(
        serde_json::to_value(cells(&w)).unwrap(),
        f["steps"][3]["classes"]
    );
    w.invoke_edit_clicked();
    w.invoke_answer(0);
    assert_eq!(
        serde_json::to_value(cells(&w)).unwrap(),
        f["steps"][4]["classes"]
    );
    w.set_show_unmatched(false);
    let image = headless::render(&h.get(h.count() - 1).unwrap(), 800, 600);
    headless::save_png(
        &std::path::PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("downloader-display.png"),
        &image,
        800,
        600,
    )
    .unwrap();
    w.invoke_cancel_clicked();
    assert!(Draft::load(&store).unwrap().show_unmatched);
    assert!(
        Draft::load(&store)
            .unwrap()
            .classes
            .iter()
            .all(|r| r.display)
    );
    // An old handle must remain inert even while a new editor is open.
    let reopened = windows::open(&store, &slots).unwrap();
    w.invoke_apply_clicked();
    assert!(reopened.window().is_visible());
    assert!(Draft::load(&store).unwrap().show_unmatched);
    reopened.invoke_row_clicked(0, false, false);
    reopened.invoke_sort(0, false);
    assert_eq!(cells(&reopened)[1][0], "alpha");
    assert!(reopened.get_rows().row_data(1).unwrap().selected);
    reopened.invoke_edit_clicked();
    reopened.invoke_answer(0);
    reopened.set_tab(1);
    reopened.invoke_tab_changed();
    reopened.invoke_row_clicked(0, false, false);
    reopened.invoke_edit_clicked();
    reopened.invoke_answer(0);
    reopened.set_show_unmatched(false);
    reopened.invoke_apply_clicked();
    let applied = windows::open(&store, &slots).unwrap();
    assert_eq!(cells(&applied)[0], vec!["alpha", "no"]);
    applied.set_tab(1);
    applied.invoke_tab_changed();
    assert_eq!(cells(&applied)[0], vec!["example post", "post url", "no"]);
    assert!(!applied.get_show_unmatched());
    applied.set_show_unmatched(true);
    drop(slots);
    applied.invoke_apply_clicked();
    assert!(!Draft::load(&store).unwrap().show_unmatched);
    assert!(!applied.window().is_visible());
}
#[test]
fn saved_choices_reach_existing_gallery_and_open_viewer() {
    let h = headless::init();
    let legacy = hydrus_testkit::legacy_fixture("basic");
    let dir = tempfile::tempdir().unwrap();
    hydrus_store::import::import_legacy(
        legacy.path(),
        &dir.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store = Store::open(dir.path()).unwrap();
    install(&store);
    let mut gallery = SearchPage::gallery_downloader(
        store.clone(),
        hydrus_core::pages::PageKey([91; 32]),
        "display test",
        vec![],
        None,
        None,
        vec![],
    );
    assert_eq!(
        gallery
            .gallery()
            .unwrap()
            .selector_gugs()
            .iter()
            .map(|g| g.1.as_str())
            .collect::<Vec<_>>(),
        ["alpha"]
    );
    let mut draft = Draft::load(&store).unwrap();
    draft.answer(false, &[0], Some(false));
    draft.answer(false, &[1], Some(true));
    draft.save(&store).unwrap();
    gallery.refresh_import();
    assert_eq!(
        gallery
            .gallery()
            .unwrap()
            .selector_gugs()
            .iter()
            .map(|g| g.1.as_str())
            .collect::<Vec<_>>(),
        ["beta"]
    );
    gallery.set_show_other_gugs(true);
    assert_eq!(
        gallery
            .gallery()
            .unwrap()
            .selector_gugs()
            .iter()
            .map(|g| g.1.as_str())
            .collect::<Vec<_>>(),
        ["beta", "alpha"]
    );
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::single(SearchPage::new(store.clone())));
    ui.invoke_search_edited("system:everything".into());
    ui.invoke_search_accepted();
    let file = bound.current.borrow().borrow().results()[0];
    let urls = vec![
        "https://example.com/".into(),
        "https://other.example/item".into(),
    ];
    store
        .write_content(move |w| w.add_urls(&[file], &urls))
        .unwrap();
    ui.invoke_thumbnail_activated(0);
    let viewer = bound
        .viewer
        .borrow()
        .as_ref()
        .map(slint::ComponentHandle::clone_strong)
        .unwrap();
    assert!(viewer.get_url_links().row_count() >= 2);
    assert!(
        (0..viewer.get_url_links().row_count()).any(|i| viewer
            .get_url_links()
            .row_data(i)
            .unwrap()
            .label
            == "example post")
    );
    let drawn = h.get(h.count() - 1).unwrap();
    headless::render(&drawn, 800, 600);
    drawn.dispatch_event(slint::platform::WindowEvent::PointerMoved {
        position: slint::LogicalPosition::new(799.0, 1.0),
    });
    assert!(viewer.get_ratings_showing());
    let pixels = headless::render(&drawn, 800, 600);
    headless::save_png(
        &std::path::PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("downloader-viewer-urls.png"),
        &pixels,
        800,
        600,
    )
    .unwrap();
    let slots = windows::Slots::default();
    let editor = windows::open(&store, &slots).unwrap();
    editor.set_tab(1);
    editor.invoke_tab_changed();
    editor.invoke_row_clicked(0, false, false);
    editor.invoke_row_clicked(1, false, true);
    editor.invoke_edit_clicked();
    editor.invoke_answer(0);
    editor.set_show_unmatched(false);
    editor.invoke_apply_clicked();
    until(|| viewer.get_url_links().row_count() == 0);
    let editor = windows::open(&store, &slots).unwrap();
    editor.set_tab(1);
    editor.invoke_tab_changed();
    editor.invoke_row_clicked(0, false, false);
    editor.invoke_edit_clicked();
    editor.invoke_answer(1);
    editor.invoke_apply_clicked();
    until(|| viewer.get_url_links().row_count() == 1);
    assert_eq!(
        viewer.get_url_links().row_data(0).unwrap().label,
        "example post"
    );
    headless::render(&drawn, 800, 600);
    drawn.dispatch_event(slint::platform::WindowEvent::PointerMoved {
        position: slint::LogicalPosition::new(799.0, 1.0),
    });
    assert!(
        viewer.get_ratings_showing(),
        "URL frame appears at the top right"
    );
    viewer.invoke_close_requested();
}
