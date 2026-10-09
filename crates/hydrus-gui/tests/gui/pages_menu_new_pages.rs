//! The pages menu's entries that open a page (new page of pages, the
//! download and special entries, the file search domains), chosen from the
//! real menu bar and compared with what the reference's own menu did
//! (`oracle/record_new_page_menu.py`): the page it opened, named and typed
//! as it names and types it, where it went (into the deepest page of
//! pages, else beside the current page), and which tabs are current.

use slint::Model as _;

use hydrus_core::pages::{Page, PageContent, PageKey, Session};
use hydrus_gui::{MainWindow, Pages, bind, headless};
use hydrus_search::FileSearchContext;
use hydrus_store::import::import_legacy;
use hydrus_store::{Store, sessions};

/// A search page of "my files", as the client begins with.
fn search(name: &str) -> Page {
    Page {
        key: PageKey::random(),
        name: name.into(),
        content: PageContent::Search {
            search: FileSearchContext {
                location: hydrus_search::LocationContext::single(hydrus_core::ServiceKey::new(
                    hydrus_core::service::builtin_keys::MY_FILES.to_vec(),
                )),
                ..FileSearchContext::default()
            },
            synchronised: false,
            sort: None,
            lock: None,
            collect: None,
        },
    }
}

fn panes(ui: &MainWindow) -> Vec<Vec<String>> {
    let panes = ui.get_menu_panes();
    (0..panes.row_count())
        .map(|p| {
            let lines = panes.row_data(p).unwrap().lines;
            (0..lines.row_count())
                .map(|i| lines.row_data(i).unwrap().label.to_string())
                .collect()
        })
        .collect()
}

fn line(ui: &MainWindow, label: &str) -> (i32, i32) {
    let panes = panes(ui);
    let p = panes.len() - 1;
    let i = panes[p]
        .iter()
        .position(|l| l == label)
        .unwrap_or_else(|| panic!("{label} in {:?}", panes[p]));
    (i32::try_from(p).unwrap(), i32::try_from(i).unwrap())
}

fn hover(ui: &MainWindow, label: &str) {
    let (p, i) = line(ui, label);
    ui.invoke_menu_line_hovered(
        p,
        i,
        300.0 + 150.0 * p as f32,
        40.0 + 22.0 * i as f32,
        150.0 * p as f32,
    );
}

/// The tree of pages as the recording describes it: name, type, pages (or,
/// of a search page, the file domains it searches).
fn tree(store: &Store, pages: &[Page]) -> serde_json::Value {
    let names = |keys: &std::collections::BTreeSet<hydrus_core::ServiceKey>| -> Vec<String> {
        let snapshot = store.snapshot();
        let mut names: Vec<String> = keys
            .iter()
            .map(|key| snapshot.services.by_key(key).unwrap().name.clone())
            .collect();
        names.sort();
        names
    };
    pages
        .iter()
        .map(|page| match &page.content {
            PageContent::Pages(children) => serde_json::json!({
                "name": page.name, "type": "pages", "pages": tree(store, children),
            }),
            PageContent::Search { search, .. } => serde_json::json!({
                "name": page.name,
                "type": 6,
                "location": {
                    "current": names(search.location.current()),
                    "deleted": names(search.location.deleted()),
                },
            }),
            content => serde_json::json!({ "name": page.name, "type": content.page_type() }),
        })
        .collect()
}

// leaf: audit-options-menu-menu-pages-new-page-of-pages
// leaf: audit-options-menu-menu-pages-new-simple-downloader-page
// leaf: audit-options-menu-menu-pages-new-url-download-page
// leaf: audit-options-menu-menu-pages-new-watcher-page
// leaf: audit-options-menu-menu-pages-new-gallery-page
// leaf: audit-options-menu-menu-pages-new-duplicates-processing-page
// leaf: audit-options-menu-menu-pages-file-search-domain
// leaf: audit-options-menu-menu-pages-new-page
#[test]
fn pages_menu_entries_open_the_pages_the_reference_opened_where_it_opened_them() {
    let _windows = headless::init();
    let recorded = hydrus_testkit::fixture_json("new_page_menu.json");
    let legacy = hydrus_testkit::legacy_fixture("basic");
    let native = tempfile::tempdir().unwrap();
    import_legacy(
        legacy.path(),
        &native.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store = Store::open(native.path()).unwrap();
    let mut made = 0;
    for case in recorded["cases"].as_array().unwrap() {
        let inside = case["inside"].as_bool().unwrap();
        let session = Session {
            name: sessions::LAST_SESSION.into(),
            pages: if inside {
                vec![Page {
                    key: PageKey::random(),
                    name: "outer".into(),
                    content: PageContent::Pages(vec![search("inner")]),
                }]
            } else {
                vec![search("start")]
            },
        };
        store
            .write(move |ctx| sessions::save(ctx.conn(), &session, 1))
            .unwrap();
        let ui = MainWindow::new().unwrap();
        let bound = bind(&ui, Pages::open(store.clone()).unwrap());
        assert_eq!(
            tree(&store, &bound.pages.borrow().session().pages),
            case["before"],
            "{case}"
        );

        let entry: Vec<&str> = case["entry"]
            .as_array()
            .unwrap()
            .iter()
            .map(|e| e.as_str().unwrap())
            .collect();
        let titles = ui.get_menu_titles();
        let at = (0..titles.row_count())
            .position(|i| titles.row_data(i).unwrap().label == "pages")
            .unwrap();
        ui.invoke_menu_title_pressed(i32::try_from(at).unwrap(), 80.0, 22.0);
        if entry.len() == 1 {
            // new page…: the chooser, a page of pages picked or cancelled
            let (p, i) = line(&ui, entry[0]);
            ui.invoke_menu_line_clicked(p, i, 0.0, 0.0, 0.0);
            assert!(ui.get_chooser_labels().row_count() > 0, "the chooser");
            if case["chooser"] == "cancelled" {
                ui.invoke_chooser_cancel();
            } else {
                ui.invoke_chooser_pressed(6);
                ui.invoke_chooser_pressed(8);
            }
            assert_eq!(ui.get_chooser_labels().row_count(), 0);
        } else {
            hover(&ui, entry[0]);
            // (the file search entries are the reference's, by domain)
            let recorded_searches: Vec<&str> = recorded["search_entries"]
                .as_array()
                .unwrap()
                .iter()
                .map(|e| e.as_str().unwrap())
                .collect();
            if entry[0] == "file search" {
                let ours = panes(&ui).last().unwrap().clone();
                assert_eq!(ours, recorded_searches, "the file search entries");
            }
            let (p, i) = line(&ui, entry[1]);
            ui.invoke_menu_line_clicked(p, i, 0.0, 0.0, 0.0);
        }

        let pages = bound.pages.borrow();
        assert_eq!(
            tree(&store, &pages.session().pages),
            case["after"],
            "after {case}"
        );
        let current: Vec<u64> = pages.tabs().iter().map(|t| t.selected as u64).collect();
        let theirs: Vec<u64> = case["current"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| c.as_u64().unwrap())
            .collect();
        assert_eq!(current, theirs, "the tabs that are current after {case}");
        made += 1;
    }
    assert_eq!(made, 22);
}

fn chooser_labels(ui: &MainWindow) -> Vec<String> {
    let labels = ui.get_chooser_labels();
    (0..labels.row_count())
        .map(|i| labels.row_data(i).unwrap().to_string())
        .collect()
}

fn choose_from_pages_menu(ui: &MainWindow, label: &str) {
    let titles = ui.get_menu_titles();
    let at = (0..titles.row_count())
        .position(|i| titles.row_data(i).unwrap().label == "pages")
        .unwrap();
    ui.invoke_menu_title_pressed(i32::try_from(at).unwrap(), 80.0, 22.0);
    let (p, i) = line(ui, label);
    ui.invoke_menu_line_clicked(p, i, 0.0, 0.0, 0.0);
}

// leaf: audit-options-menu-menu-pages-new-page
#[test]
fn new_page_from_the_pages_menu_asks_the_chooser_the_reference_asked() {
    use hydrus_core::ServiceKey;
    use hydrus_store::settings::{self, PageChooserSettings};

    let _windows = headless::init();
    let recorded = hydrus_testkit::fixture_json("page_chooser_options.json");
    let legacy = hydrus_testkit::legacy_fixture("basic");
    let native = tempfile::tempdir().unwrap();
    import_legacy(
        legacy.path(),
        &native.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store = Store::open(native.path()).unwrap();
    // (the recording's one local file domain is called "domain 01", and the
    // client has no art domain, or other, to offer)
    let lone = recorded["steps"][0]["labels"].clone();
    assert_eq!(lone[7], "domain 01");
    let my_files = ServiceKey::new(hydrus_core::service::builtin_keys::MY_FILES.to_vec());
    store
        .write_and_refresh(move |ctx| {
            ctx.conn()
                .execute("DELETE FROM services WHERE name = 'art'", [])?;
            ctx.conn().execute(
                "UPDATE services SET name = 'domain 01' WHERE service_key = ?",
                [my_files],
            )?;
            Ok(())
        })
        .unwrap();
    let session = Session {
        name: sessions::LAST_SESSION.into(),
        pages: vec![search("start")],
    };
    store
        .write(move |ctx| sessions::save(ctx.conn(), &session, 1))
        .unwrap();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    let recorded_choices: usize = recorded["steps"]
        .as_array()
        .unwrap()
        .iter()
        .map(|step| step["choices"].as_array().unwrap().len())
        .sum();
    assert_eq!(recorded["steps"].as_array().unwrap().len(), 48);
    let mut seen = 0;
    let mut domains = 1;
    for step in recorded["steps"].as_array().unwrap() {
        // (the domains the recording had: more are added as its steps
        // need them)
        let count = usize::try_from(step["count"].as_u64().unwrap()).unwrap();
        while domains < count {
            let service = recorded["services"][domains].clone();
            let key = ServiceKey::from_hex(service["key"].as_str().unwrap()).unwrap();
            let name = service["name"].as_str().unwrap().to_owned();
            store
                .write_and_refresh(move |ctx| {
                    hydrus_store::services::insert(
                        ctx.conn(),
                        &key,
                        &name,
                        &hydrus_store::services::ServiceKind::LocalFiles,
                    )?;
                    Ok(())
                })
                .unwrap();
            domains += 1;
        }
        let flags: Vec<bool> = step["flags"]
            .as_array()
            .unwrap()
            .iter()
            .map(|f| f.as_bool().unwrap())
            .collect();
        store
            .write(move |ctx| {
                settings::set(
                    ctx.conn(),
                    &PageChooserSettings {
                        show_combined: flags[0],
                        combined_at_top: flags[1],
                        show_storage: flags[2],
                        storage_at_top: flags[3],
                    },
                )
            })
            .unwrap();
        let want: Vec<String> = step["labels"]
            .as_array()
            .unwrap()
            .iter()
            .map(|l| l.as_str().unwrap().to_owned())
            .collect();
        // cancelled: the chooser's choices, and no page
        let before = bound.pages.borrow().session().clone();
        choose_from_pages_menu(&ui, "new page\u{2026}");
        ui.invoke_chooser_pressed(8);
        assert_eq!(chooser_labels(&ui), want, "{}", step["flags"]);
        ui.invoke_chooser_cancel();
        assert_eq!(ui.get_chooser_labels().row_count(), 0);
        assert_eq!(bound.pages.borrow().session(), &before);
        // each choice opens a search of exactly the domain it names
        for choice in step["choices"].as_array().unwrap() {
            choose_from_pages_menu(&ui, "new page\u{2026}");
            ui.invoke_chooser_pressed(8);
            ui.invoke_chooser_pressed(i32::try_from(choice["button"].as_u64().unwrap()).unwrap());
            assert_eq!(ui.get_chooser_labels().row_count(), 0);
            let page = bound.pages.borrow_mut().current();
            let page = page.borrow();
            let current: Vec<String> = page
                .location()
                .current()
                .iter()
                .map(ServiceKey::to_hex)
                .collect();
            let theirs: Vec<String> = choice["current"]
                .as_array()
                .unwrap()
                .iter()
                .map(|k| k.as_str().unwrap().to_owned())
                .collect();
            assert_eq!(current, theirs, "{choice}");
            assert!(page.location().deleted().is_empty());
            seen += 1;
        }
    }
    assert_eq!(seen, recorded_choices);
}

// leaf: audit-options-menu-menu-pages-history-page
// leaf: audit-options-menu-menu-pages-clear-history
#[test]
fn the_history_menu_follows_the_pages_shown_closed_and_cleared_as_the_reference_s_does() {
    use hydrus_store::settings::{self, PageNavigationSettings};

    let _windows = headless::init();
    let recorded = hydrus_testkit::fixture_json("page_history_menu.json");
    let names: Vec<String> = recorded["names"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| n.as_str().unwrap().to_owned())
        .collect();
    let legacy = hydrus_testkit::legacy_fixture("basic");
    let native = tempfile::tempdir().unwrap();
    import_legacy(
        legacy.path(),
        &native.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store = Store::open(native.path()).unwrap();
    let session = Session {
        name: sessions::LAST_SESSION.into(),
        pages: names.iter().map(|n| search(n)).collect(),
    };
    store
        .write(move |ctx| sessions::save(ctx.conn(), &session, 1))
        .unwrap();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    // (the page up from the start has not "just changed")
    bound.pages.borrow_mut().clear_history();
    let tab_of = |name: &str| -> i32 {
        let at = bound.pages.borrow().tabs()[0]
            .names
            .iter()
            .position(|n| n == name)
            .unwrap_or_else(|| panic!("{name} open"));
        i32::try_from(at).unwrap()
    };
    let history_menu = |ui: &MainWindow| -> Vec<String> {
        let titles = ui.get_menu_titles();
        let at = (0..titles.row_count())
            .position(|i| titles.row_data(i).unwrap().label == "pages")
            .unwrap();
        ui.invoke_menu_title_pressed(i32::try_from(at).unwrap(), 80.0, 22.0);
        hover(ui, "history");
        let lines = panes(ui).last().unwrap().clone();
        ui.invoke_menu_dismissed();
        lines
            .into_iter()
            .map(|l| if l.is_empty() { "---".to_owned() } else { l })
            .collect()
    };
    for step in recorded["steps"].as_array().unwrap() {
        let what = step["step"][0].as_str().unwrap();
        match what {
            "show" => ui.invoke_tab_chosen(0, tab_of(step["step"][1].as_str().unwrap())),
            "close" => {
                ui.invoke_tab_chosen(0, tab_of(step["step"][1].as_str().unwrap()));
                ui.invoke_close_page();
            }
            "choose" => {
                let prefix = format!("{}: ", step["step"][1]);
                let titles = ui.get_menu_titles();
                let at = (0..titles.row_count())
                    .position(|i| titles.row_data(i).unwrap().label == "pages")
                    .unwrap();
                ui.invoke_menu_title_pressed(i32::try_from(at).unwrap(), 80.0, 22.0);
                hover(&ui, "history");
                let panes = panes(&ui);
                let entry = panes
                    .last()
                    .unwrap()
                    .iter()
                    .find(|l| l.starts_with(&prefix))
                    .unwrap()
                    .clone();
                let (p, i) = line(&ui, &entry);
                ui.invoke_menu_line_clicked(p, i, 0.0, 0.0, 0.0);
            }
            "max" => {
                let entries = u16::try_from(step["step"][1].as_u64().unwrap()).unwrap();
                store
                    .write(move |ctx| {
                        settings::set(
                            ctx.conn(),
                            &PageNavigationSettings {
                                history_entries: entries,
                                ..settings::get::<PageNavigationSettings>(ctx.conn())?
                            },
                        )
                    })
                    .unwrap();
            }
            _ => {
                let titles = ui.get_menu_titles();
                let at = (0..titles.row_count())
                    .position(|i| titles.row_data(i).unwrap().label == "pages")
                    .unwrap();
                ui.invoke_menu_title_pressed(i32::try_from(at).unwrap(), 80.0, 22.0);
                hover(&ui, "history");
                let (p, i) = line(&ui, "Clear History");
                ui.invoke_menu_line_clicked(p, i, 0.0, 0.0, 0.0);
            }
        }
        let theirs: Vec<String> = step["entries"]
            .as_array()
            .unwrap()
            .iter()
            .map(|e| {
                e.as_str()
                    .map_or_else(|| e["text"].as_str().unwrap().to_owned(), str::to_owned)
            })
            .collect();
        assert_eq!(history_menu(&ui), theirs, "after {}", step["step"]);
        // (and the page that is shown: choosing an entry shows that page, and
        // closing one shows the one the reference moved to)
        assert_eq!(
            bound.pages.borrow().shown().name,
            step["current"].as_str().unwrap(),
            "the page shown after {}",
            step["step"]
        );
    }
}
