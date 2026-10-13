//! Duplicating a URL importer page from its tab's menu, against
//! `oracle/fixtures/tab_duplicate_importer.json` (recorded by
//! `oracle/record_tab_duplicate_importer.py`): the copy sits beside the
//! original with its name, its queued URLs in order and its paused state,
//! and is its own queue afterwards.

use hydrus_core::import_options::ImportOptionsSlice;
use hydrus_core::pages::{
    DownloaderKind, DownloaderPageState, Page, PageContent, PageKey, Session,
};
use hydrus_gui::{MainWindow, Pages, bind, headless};
use hydrus_store::queues::{self, FileSeedMeta, NewFileSeed, QueueKind, SeedType};
use hydrus_store::sessions::{self, LAST_SESSION};
use slint::Model as _;

use crate::subscriptions::store;

fn urls(store: &hydrus_store::Store, queue: i64) -> Vec<String> {
    store
        .read(|conn| queues::file_seeds(conn, queue))
        .unwrap()
        .into_iter()
        .map(|seed| seed.data)
        .collect()
}

fn new_seeds(urls: &[String]) -> Vec<NewFileSeed> {
    urls.iter()
        .map(|url| NewFileSeed {
            seed_type: SeedType::Url,
            data: url.clone(),
            data_for_comparison: url.clone(),
            source_time: None,
            referral_url: None,
            meta: FileSeedMeta::default(),
        })
        .collect()
}

fn names(ui: &MainWindow) -> Vec<String> {
    ui.get_tab_rows()
        .row_data(0)
        .unwrap()
        .full_names
        .iter()
        .map(|n| n.to_string())
        .collect()
}

// leaf: audit-options-tabs-context-action-2201-duplicate-page
#[test]
fn duplicating_an_importer_page_copies_its_queue_and_keeps_the_two_apart() {
    let recorded = hydrus_testkit::fixture_json("tab_duplicate_importer.json");
    let step = |label: &str| {
        recorded["steps"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["label"] == label)
            .unwrap()
            .clone()
    };
    let strings = |value: &serde_json::Value| -> Vec<String> {
        value
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().to_owned())
            .collect()
    };
    let duplicated = step("duplicated");
    let (_dirs, store) = store();
    let _windows = headless::init();
    // the same pages around an importer holding the same URLs, paused
    let importer_key = PageKey::random();
    let queued = strings(&duplicated["original_urls"]);
    let items = new_seeds(&queued);
    let queue = store
        .write(move |ctx| {
            let queue = queues::create_queue(
                ctx.conn(),
                QueueKind::Urls,
                "my importer",
                Some(&importer_key.0),
                &ImportOptionsSlice::default(),
                1,
            )?;
            queues::set_paused(ctx.conn(), queue, Some(true), Some(true))?;
            queues::add_file_seeds(ctx.conn(), queue, &items, false, 1)?;
            Ok(queue)
        })
        .unwrap();
    let search = |name: &str| Page {
        key: PageKey::random(),
        name: name.into(),
        content: PageContent::Search {
            search: hydrus_search::FileSearchContext::default(),
            synchronised: true,
            sort: None,
            lock: None,
            collect: None,
        },
    };
    let importer = Page {
        key: importer_key,
        name: "my importer".into(),
        content: PageContent::Downloader {
            kind: DownloaderKind::Urls,
            queues: vec![queue],
            sort: None,
            page: Some(Box::new(DownloaderPageState::default())),
        },
    };
    let before = strings(&duplicated["names_before"]);
    assert_eq!(before, ["before", "my importer", "after"]);
    let session = Session {
        name: LAST_SESSION.into(),
        pages: vec![search("before"), importer, search("after")],
    };
    store
        .write(move |ctx| sessions::save(ctx.conn(), &session, 1))
        .unwrap();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    assert_eq!(names(&ui), before);
    let at = duplicated["importer_index"].as_i64().unwrap() as i32;
    ui.invoke_tab_menu_requested(0, at, 30.0, 55.0);
    let lines = ui.get_menu_panes().row_data(0).unwrap().lines;
    let line = (0..lines.row_count())
        .find(|&i| lines.row_data(i).unwrap().label == "duplicate page")
        .unwrap();
    ui.invoke_menu_line_clicked(0, i32::try_from(line).unwrap(), 200.0, 100.0, 10.0);
    // the copy is beside the original, shown, with its name
    assert_eq!(names(&ui), strings(&duplicated["names"]));
    assert_eq!(
        bound.pages.borrow().shown().name,
        "my importer",
        "the copy is shown (recorded tab {})",
        duplicated["selected"]
    );
    bound.pages.borrow_mut().sync(2).unwrap();
    let pages = bound.pages.borrow().session().pages.clone();
    let (
        PageContent::Downloader {
            queues: original, ..
        },
        PageContent::Downloader { queues: copy, .. },
    ) = (&pages[at as usize].content, &pages[at as usize + 1].content)
    else {
        panic!("two importer pages");
    };
    assert_ne!(pages[at as usize].key, pages[at as usize + 1].key);
    assert_eq!((original.len(), copy.len()), (1, 1));
    let (original, copy) = (original[0], copy[0]);
    assert_ne!(original, copy, "its own queue");
    // the same URLs in order, and paused as the original was
    assert_eq!(
        urls(&store, original),
        strings(&duplicated["original_urls"])
    );
    assert_eq!(urls(&store, copy), strings(&duplicated["copy_urls"]));
    let paused = |queue: i64| {
        let q = store
            .read(|conn| queues::queue(conn, queue))
            .unwrap()
            .unwrap();
        q.files_paused
    };
    assert_eq!(
        paused(original),
        duplicated["original_paused"].as_bool().unwrap()
    );
    assert_eq!(paused(copy), duplicated["copy_paused"].as_bool().unwrap());
    // a URL pended to one does not reach the other
    let only = strings(&step("pended to the original")["original_urls"]);
    let extra = new_seeds(&only[only.len() - 1..]);
    store
        .write(move |ctx| queues::add_file_seeds(ctx.conn(), original, &extra, false, 2))
        .unwrap();
    assert_eq!(
        urls(&store, original),
        strings(&step("pended to the original")["original_urls"])
    );
    assert_eq!(
        urls(&store, copy),
        strings(&step("pended to the original")["copy_urls"])
    );
    let theirs = strings(&step("pended to the copy")["copy_urls"]);
    let extra = new_seeds(&theirs[theirs.len() - 1..]);
    store
        .write(move |ctx| queues::add_file_seeds(ctx.conn(), copy, &extra, false, 3))
        .unwrap();
    assert_eq!(
        urls(&store, original),
        strings(&step("pended to the copy")["original_urls"])
    );
    assert_eq!(urls(&store, copy), theirs);
}
