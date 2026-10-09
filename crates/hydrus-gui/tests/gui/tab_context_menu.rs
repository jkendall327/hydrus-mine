//! The tab right-click menu put through the reference's recordings
//! (`oracle/fixtures/tab_actions.json` and `tab_context.json`) as a user
//! would: the menu opened on a tab, its entries (and submenus) clicked, the
//! bulk-close question answered, and the pages read back from the notebook.

use std::sync::Arc;

use slint::Model as _;

use hydrus_core::pages::{Page, PageContent, PageKey, Session};
use hydrus_gui::{MainWindow, Pages, bind, headless};
use hydrus_search::FileSearchContext;
use hydrus_store::Store;
use hydrus_store::import::import_legacy;
use hydrus_store::sessions::{self, LAST_SESSION};

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

fn search() -> PageContent {
    PageContent::Search {
        search: FileSearchContext::default(),
        synchronised: false,
        sort: None,
        lock: None,
        collect: None,
    }
}

fn page(name: &str, content: PageContent) -> Page {
    Page {
        key: PageKey::random(),
        name: name.into(),
        content,
    }
}

fn labels(ui: &MainWindow, pane: usize) -> Vec<String> {
    let lines = ui.get_menu_panes().row_data(pane).unwrap().lines;
    (0..lines.row_count())
        .map(|i| lines.row_data(i).unwrap().label.to_string())
        .collect()
}

/// Click the line `label` of menu pane `pane` (a submenu line opens the next pane).
fn choose(ui: &MainWindow, pane: usize, label: &str) {
    let at = labels(ui, pane)
        .iter()
        .position(|l| l == label)
        .unwrap_or_else(|| panic!("{label:?} in {:?}", labels(ui, pane)));
    ui.invoke_menu_line_clicked(
        i32::try_from(pane).unwrap(),
        i32::try_from(at).unwrap(),
        200.0,
        100.0,
        10.0,
    );
}

fn seed(store: &Arc<Store>, pages: Vec<Page>) {
    let saved = Session {
        name: LAST_SESSION.into(),
        pages,
    };
    store
        .write(move |ctx| sessions::save(ctx.conn(), &saved, 100))
        .unwrap();
}

fn names(pages: &Pages) -> Vec<String> {
    pages
        .session()
        .pages
        .iter()
        .map(|p| p.name.clone())
        .collect()
}

#[test]
fn close_other_left_and_right_from_the_tab_menu_ask_and_close_as_the_reference_did() {
    let _windows = headless::init();
    let (_dirs, store) = store();
    let fixture = hydrus_testkit::fixture_json("tab_actions.json");
    let original = vec![
        page("a", search()),
        page(
            "nested",
            PageContent::Pages(vec![page("child 0", search()), page("child 1", search())]),
        ),
        page("c", search()),
        page("d", search()),
    ];
    for step in fixture["close"].as_array().unwrap() {
        seed(&store, original.clone());
        let ui = MainWindow::new().unwrap();
        let bound = bind(&ui, Pages::open(store.clone()).unwrap());
        bound.pages.borrow_mut().select(0, 2);
        let index = i32::try_from(step["index"].as_u64().unwrap()).unwrap();
        ui.invoke_tab_menu_requested(0, index, 30.0, 55.0);
        let entry = match step["side"].as_str().unwrap() {
            "left" => "pages to the left",
            "right" => "pages to the right",
            _ => "other pages",
        };
        choose(&ui, 0, "close");
        choose(&ui, 1, entry);
        assert_eq!(ui.get_question(), step["asked"][0].as_str().unwrap());
        ui.invoke_answer(step["accepted"].as_bool().unwrap());
        assert_eq!(
            serde_json::json!(names(&bound.pages.borrow())),
            step["pages"],
            "{step}"
        );
        assert_eq!(
            bound.pages.borrow_mut().closed_names().len(),
            step["closed_indices"].as_array().unwrap().len(),
            "{step}"
        );
        // and undone, the pages are back where they were
        while bound.pages.borrow_mut().unclose() {}
        assert_eq!(bound.pages.borrow().session().pages, original);
    }
}

// leaf: audit-options-tabs-context-action-2141-first-page
// leaf: audit-options-tabs-context-action-2146-page-to-the-left
// leaf: audit-options-tabs-context-action-2151-page-to-the-right
// leaf: audit-options-tabs-context-action-2156-last-page
#[test]
fn select_from_the_tab_menu_goes_where_the_reference_went() {
    let _windows = headless::init();
    let (_dirs, store) = store();
    let fixture = hydrus_testkit::fixture_json("tab_actions.json");
    let original: Vec<_> = (0..4)
        .map(|i| page(&format!("page {i}"), search()))
        .collect();
    // the menu the reference offered with the second-from-right tab selected
    // and clicked: which of the four movements it had
    let menu = fixture["menus"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["selected"] == 2 && m["clicked"] == 2)
        .unwrap();
    let offered: Vec<String> = menu["entries"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["menu"] == "select")
        .unwrap()["entries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e.as_str().unwrap().to_owned())
        .collect();
    let mut driven = 0;
    for step in fixture["navigation"].as_array().unwrap() {
        seed(&store, original.clone());
        let ui = MainWindow::new().unwrap();
        let bound = bind(&ui, Pages::open(store.clone()).unwrap());
        bound.pages.borrow_mut().select(0, 2);
        ui.invoke_tab_menu_requested(0, 2, 30.0, 55.0);
        choose(&ui, 0, "select");
        let ours = labels(&ui, 1);
        assert_eq!(ours, offered, "the select submenu");
        let entry = match step["movement"].as_str().unwrap() {
            "first" => "first page",
            "left" => "page to the left",
            "right" => "page to the right",
            _ => "last page",
        };
        if offered.iter().any(|o| o == entry) {
            choose(&ui, 1, entry);
            assert_eq!(
                bound.pages.borrow().tabs()[0].selected,
                usize::try_from(step["selected"].as_u64().unwrap()).unwrap(),
                "{step}"
            );
            assert_eq!(
                bound.pages.borrow().shown().name,
                format!("page {}", step["selected"]),
                "{step}"
            );
            driven += 1;
        } else {
            // (not offered with this tab selected: the reference's keys did it)
            assert!(!ours.iter().any(|o| o == entry), "{step}");
        }
    }
    assert_eq!(driven, 3, "first, left and right are offered here");
    // "last page" is offered with the first tab selected (the recorded menu
    // for that), and goes to the last one, as the recorded `last` did
    let first_menu = fixture["menus"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["selected"] == 0 && m["clicked"] == 0)
        .unwrap();
    let offered: Vec<String> = first_menu["entries"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["menu"] == "select")
        .unwrap()["entries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e.as_str().unwrap().to_owned())
        .collect();
    seed(&store, original.clone());
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    bound.pages.borrow_mut().select(0, 0);
    ui.invoke_tab_menu_requested(0, 0, 30.0, 55.0);
    choose(&ui, 0, "select");
    assert_eq!(labels(&ui, 1), offered, "the select submenu");
    choose(&ui, 1, "last page");
    let last = fixture["navigation"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["movement"] == "last")
        .unwrap();
    assert_eq!(
        bound.pages.borrow().tabs()[0].selected,
        usize::try_from(last["selected"].as_u64().unwrap()).unwrap()
    );
    assert_eq!(
        bound.pages.borrow().shown().name,
        format!("page {}", last["selected"])
    );
}

// leaf: audit-options-tabs-context-action-2178-to-left-end
// leaf: audit-options-tabs-context-action-2183-left
// leaf: audit-options-tabs-context-action-2188-right
// leaf: audit-options-tabs-context-action-2193-to-right-end
#[test]
fn move_page_from_the_tab_menu_reorders_as_the_reference_did() {
    let _windows = headless::init();
    let (_dirs, store) = store();
    let fixture = hydrus_testkit::fixture_json("tab_context.json");
    let count = fixture["pages"].as_array().unwrap().len();
    let shown = usize::try_from(fixture["selected"].as_u64().unwrap()).unwrap();
    let original: Vec<_> = (0..count)
        .map(|i| page(&format!("p{i}"), search()))
        .collect();
    let mut moved = 0;
    for step in fixture["moves"].as_array().unwrap() {
        seed(&store, original.clone());
        let ui = MainWindow::new().unwrap();
        let bound = bind(&ui, Pages::open(store.clone()).unwrap());
        bound.pages.borrow_mut().select(0, shown);
        let index = usize::try_from(step["index"].as_u64().unwrap()).unwrap();
        ui.invoke_tab_menu_requested(0, i32::try_from(index).unwrap(), 30.0, 55.0);
        let entry = match step["movement"].as_str().unwrap() {
            "first" => "to left end",
            "left" => "left",
            "right" => "right",
            _ => "to right end",
        };
        choose(&ui, 0, "move page");
        let offered = labels(&ui, 1);
        let expected: Vec<String> = step["order"]
            .as_array()
            .unwrap()
            .iter()
            .map(|i| format!("p{}", i.as_u64().unwrap()))
            .collect();
        let unchanged = expected == (0..count).map(|i| format!("p{i}")).collect::<Vec<_>>();
        // (a move that goes nowhere is not offered, as the reference's menu
        // left it out, and changes nothing)
        assert_eq!(offered.iter().any(|o| o == entry), !unchanged, "{step}");
        if !unchanged {
            choose(&ui, 1, entry);
            moved += 1;
        }
        assert_eq!(names(&bound.pages.borrow()), expected, "{step}");
        // the page that was shown still is (the recording's `selected` is the
        // original index of the current page)
        assert_eq!(
            bound.pages.borrow().shown().name,
            format!("p{shown}"),
            "{step}"
        );
    }
    assert_eq!(moved, 12, "all but the four moves that go nowhere");

    // and the entries the reference's menus offered for each tab of four
    // pages, selected and clicked, are the ones this menu offers
    let actions = hydrus_testkit::fixture_json("tab_actions.json");
    for menu in actions["menus"].as_array().unwrap() {
        let (selected, clicked) = (
            usize::try_from(menu["selected"].as_u64().unwrap()).unwrap(),
            usize::try_from(menu["clicked"].as_u64().unwrap()).unwrap(),
        );
        seed(
            &store,
            (0..4)
                .map(|i| page(&format!("page {i}"), search()))
                .collect(),
        );
        let ui = MainWindow::new().unwrap();
        let bound = bind(&ui, Pages::open(store.clone()).unwrap());
        bound.pages.borrow_mut().select(0, selected);
        ui.invoke_tab_menu_requested(0, i32::try_from(clicked).unwrap(), 30.0, 55.0);
        choose(&ui, 0, "move page");
        let recorded: Vec<&str> = menu["entries"]
            .as_array()
            .unwrap()
            .iter()
            .find(|e| e["menu"] == "move page")
            .unwrap()["entries"]
            .as_array()
            .unwrap()
            .iter()
            .map(|e| e.as_str().unwrap())
            .collect();
        assert_eq!(labels(&ui, 1), recorded, "{selected} {clicked}");
    }
}

/// Seed the six pages of `tab_context.json` (their names, the files they
/// show and, for the three importer pages, their progress), select the page
/// the recording left current, choose the sort from the tab menu, and
/// return the keys in the order the notebook ended up in, as indexes of the
/// recorded pages.
fn sorted_from_the_menu(
    store: &Arc<Store>,
    fixture: &serde_json::Value,
    sort: &serde_json::Value,
    files: &[Vec<hydrus_core::HashId>],
    open: bool,
) -> Vec<usize> {
    use hydrus_core::pages::{DownloaderKind, PageContent};
    use hydrus_store::queues::{self, FileSeedMeta, NewFileSeed, QueueKind, SeedStatus, SeedType};

    let rows = fixture["pages"].as_array().unwrap();
    let shown = usize::try_from(fixture["selected"].as_u64().unwrap()).unwrap();
    // the importer pages: (done, of) as recorded
    let progress: Vec<(usize, usize)> = rows
        .iter()
        .map(|r| {
            let p = r["progress"].as_array().unwrap();
            (
                usize::try_from(p[0].as_u64().unwrap()).unwrap(),
                usize::try_from(p[1].as_u64().unwrap()).unwrap(),
            )
        })
        .collect();
    let pages: Vec<Page> = rows
        .iter()
        .zip(&progress)
        .map(|(row, &(_, of))| {
            let name = row["name"].as_str().unwrap();
            if of == 0 {
                page(name, search())
            } else {
                page(
                    name,
                    PageContent::Downloader {
                        kind: DownloaderKind::Urls,
                        queues: vec![],
                        sort: None,
                        page: None,
                    },
                )
            }
        })
        .collect();
    let keys: Vec<PageKey> = pages.iter().map(|p| p.key).collect();
    let kept = files.to_vec();
    let progress_for_write = progress.clone();
    let mut pages = pages;
    let queued: Vec<Option<i64>> = {
        store
            .write(move |ctx| {
                let conn = ctx.conn();
                let mut made = Vec::new();
                for &(done, of) in &progress_for_write {
                    if of == 0 {
                        made.push(None);
                        continue;
                    }
                    let options = hydrus_core::import_options::ImportOptionsSlice::default();
                    let queue =
                        queues::create_queue(conn, QueueKind::Urls, "importer", None, &options, 0)?;
                    let seeds: Vec<NewFileSeed> = (0..of)
                        .map(|n| NewFileSeed {
                            seed_type: SeedType::Url,
                            data: format!("https://site.example/{queue}/{n}"),
                            data_for_comparison: format!("https://site.example/{queue}/{n}"),
                            source_time: None,
                            referral_url: None,
                            meta: FileSeedMeta::default(),
                        })
                        .collect();
                    queues::add_file_seeds(conn, queue, &seeds, false, 0)?;
                    let ids: Vec<i64> = queues::file_seeds(conn, queue)?
                        .iter()
                        .take(done)
                        .map(|s| s.id)
                        .collect();
                    queues::set_file_seed_statuses(conn, &ids, SeedStatus::Error, 0)?;
                    made.push(Some(queue));
                }
                Ok(made)
            })
            .unwrap()
    };
    for (page, queue) in pages.iter_mut().zip(&queued) {
        if let (PageContent::Downloader { queues, .. }, Some(queue)) = (&mut page.content, queue) {
            queues.push(*queue);
        }
    }
    seed(store, pages);
    let saved_keys = keys.clone();
    store
        .write(move |ctx| {
            for (key, files) in saved_keys.iter().zip(&kept) {
                sessions::set_page_files(ctx.conn(), key, files)?;
            }
            Ok(())
        })
        .unwrap();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    if open {
        for key in &keys {
            bound.pages.borrow_mut().page(key).unwrap();
        }
    }
    bound.pages.borrow_mut().select(0, shown);
    ui.invoke_tab_menu_requested(0, 3, 30.0, 55.0);
    choose(&ui, 0, "sort pages");
    let label = match (
        sort["by"].as_str().unwrap(),
        sort["ascending"].as_bool().unwrap(),
    ) {
        ("files", false) => "by most files first",
        ("files", true) => "by fewest files first",
        ("size", false) => "by largest total file size first",
        ("size", true) => "by smallest total file size first",
        ("name", true) => "by name a-z",
        _ => "by name z-a",
    };
    choose(&ui, 1, label);
    let ended: Vec<usize> = bound
        .pages
        .borrow()
        .session()
        .pages
        .iter()
        .map(|p| keys.iter().position(|k| *k == p.key).unwrap())
        .collect();
    // the page that was current still is (the recording's `selected` is its
    // original index)
    assert_eq!(
        bound.pages.borrow().shown().key,
        keys[shown],
        "the current page after {sort}"
    );
    ended
}

fn recorded_order(sort: &serde_json::Value) -> Vec<usize> {
    sort["order"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| usize::try_from(i.as_u64().unwrap()).unwrap())
        .collect()
}

fn recorded_sorts<'a>(
    fixture: &'a serde_json::Value,
    by: &str,
) -> impl Iterator<Item = &'a serde_json::Value> {
    let by = by.to_owned();
    fixture["sorts"]
        .as_array()
        .unwrap()
        .iter()
        .filter(move |s| s["by"] == by.as_str())
}

/// Pages showing as many files as the recording's (any files will do).
fn recorded_file_counts(
    store: &Arc<Store>,
    fixture: &serde_json::Value,
) -> Vec<Vec<hydrus_core::HashId>> {
    let ids: Vec<hydrus_core::HashId> = store
        .read(|conn| {
            conn.prepare("SELECT hash_id FROM files ORDER BY hash_id")?
                .query_map([], |r| r.get(0))?
                .collect::<rusqlite::Result<_>>()
                .map_err(Into::into)
        })
        .unwrap();
    fixture["pages"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| {
            let n = usize::try_from(row["files"].as_u64().unwrap()).unwrap();
            assert!(ids.len() >= n);
            ids[..n].to_vec()
        })
        .collect()
}

// leaf: audit-options-tabs-context-action-2213-by-name-a-z
// leaf: audit-options-tabs-context-action-2214-by-name-z-a
#[test]
fn sort_pages_by_name_from_the_tab_menu_orders_as_the_reference_did() {
    let _windows = headless::init();
    let (_dirs, store) = store();
    let fixture = hydrus_testkit::fixture_json("tab_context.json");
    // (the reference breaks ties of names by the pages' file counts and
    // importer progress, so the pages need them)
    let files = recorded_file_counts(&store, &fixture);
    let mut driven = 0;
    for sort in recorded_sorts(&fixture, "name") {
        let ended = sorted_from_the_menu(&store, &fixture, sort, &files, false);
        assert_eq!(ended, recorded_order(sort), "{sort}");
        driven += 1;
    }
    assert_eq!(driven, 2);
}

// leaf: audit-options-tabs-context-action-2209-by-most-files-first
// leaf: audit-options-tabs-context-action-2210-by-fewest-files-first
#[test]
fn sort_pages_by_files_from_the_tab_menu_orders_as_the_reference_did() {
    let _windows = headless::init();
    let (_dirs, store) = store();
    let fixture = hydrus_testkit::fixture_json("tab_context.json");
    let files = recorded_file_counts(&store, &fixture);
    let mut driven = 0;
    for sort in recorded_sorts(&fixture, "files") {
        let ended = sorted_from_the_menu(&store, &fixture, sort, &files, false);
        assert_eq!(ended, recorded_order(sort), "{sort}");
        driven += 1;
    }
    assert_eq!(driven, 2);
}

// leaf: audit-options-tabs-context-action-2211-by-largest-total-file-size-first
// leaf: audit-options-tabs-context-action-2212-by-smallest-total-file-size-first
#[test]
fn sort_pages_by_size_from_the_tab_menu_orders_as_the_reference_did() {
    let _windows = headless::init();
    let (_dirs, store) = store();
    let fixture = hydrus_testkit::fixture_json("tab_context.json");
    // the recorder stubbed each page's total size (80, 10, 80, 10, 10, 0): the
    // database's largest and smallest files stand in for 80 and 10, so the
    // same pages tie and the same ones lead
    let everything = FileSearchContext {
        predicates: hydrus_search::parse_api_search(&serde_json::json!(["system:everything"]))
            .unwrap(),
        ..FileSearchContext::default()
    };
    let by_size = store
        .read(|conn| {
            Ok(hydrus_search::search_files(
                conn,
                &store.snapshot(),
                &everything,
                hydrus_search::FileSort {
                    by: hydrus_search::SortBy::FileSize,
                    order: hydrus_search::SortOrder::Descending,
                },
                &hydrus_search::Clock::system(),
            )
            .unwrap())
        })
        .unwrap();
    let (large, small) = (by_size[0], *by_size.last().unwrap());
    let sizes = store
        .read(|conn| hydrus_store::media::load_basic(conn, &[large, small]))
        .unwrap();
    assert!(sizes[0].info.as_ref().unwrap().size > sizes[1].info.as_ref().unwrap().size);
    let files: Vec<Vec<hydrus_core::HashId>> = fixture["pages"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| match row["size"].as_u64().unwrap() {
            80 => vec![large],
            10 => vec![small],
            _ => Vec::new(),
        })
        .collect();
    let mut driven = 0;
    for sort in recorded_sorts(&fixture, "size") {
        let ended = sorted_from_the_menu(&store, &fixture, sort, &files, true);
        assert_eq!(ended, recorded_order(sort), "{sort}");
        driven += 1;
    }
    assert_eq!(driven, 2);
}
