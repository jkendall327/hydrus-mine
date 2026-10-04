//! Clipboard switches and routing through real persisted importer pages.
use hydrus_core::{
    pages::{DownloaderKind, PageContent},
    url::UrlClassSettings,
};
use hydrus_gui::{MainWindow, Pages, bind, headless, page_chooser};
use hydrus_legacy::{objects::domain, serialisable::SerialisableObject};
use hydrus_store::{Store, settings};
use slint::Model as _;
use std::{cell::RefCell, rc::Rc, sync::Arc};

fn store() -> ([tempfile::TempDir; 2], Arc<Store>) {
    let legacy = hydrus_testkit::legacy_fixture("basic");
    let native = tempfile::tempdir().unwrap();
    hydrus_store::import::import_legacy(
        legacy.path(),
        &native.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store = Store::open(native.path()).unwrap();
    let f = hydrus_testkit::fixture_json("clipboard_urls.json");
    let classes = UrlClassSettings {
        url_classes: f["classes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| {
                domain::url_class(&SerialisableObject::from_tuple_str(&t.to_string()).unwrap())
                    .unwrap()
            })
            .collect(),
        parser_keys: vec![f["parser_key"].as_str().unwrap().into()],
        parser_links: f["linked"]
            .as_array()
            .unwrap()
            .iter()
            .map(|key| {
                (
                    key.as_str().unwrap().into(),
                    Some(f["parser_key"].as_str().unwrap().into()),
                )
            })
            .collect(),
        ..Default::default()
    };
    store
        .write_and_refresh(move |ctx| settings::set(ctx.conn(), &classes))
        .unwrap();
    ([legacy, native], store)
}

fn queued(store: &Store, queue: i64) -> i64 {
    store
        .read(|c| {
            Ok(c.query_row(
                "SELECT count(*) FROM queue_url_requests WHERE queue_id=?",
                [queue],
                |r| r.get(0),
            )?)
        })
        .unwrap()
}

#[test]
fn changes_route_to_open_importers_without_switching_pages() {
    let (_dirs, store) = store();
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    let text = Rc::new(RefCell::new("https://file.example/1.jpg".to_owned()));
    hydrus_gui::set_paster({
        let text = text.clone();
        move || text.borrow().clone()
    });
    let original = bound.pages.borrow().shown().key;
    let before = bound.pages.borrow().page_count();
    bound.clipboard_monitor.poll();
    assert_eq!(bound.pages.borrow().page_count(), before);
    bound.clipboard_monitor.toggle(false);
    bound.clipboard_monitor.poll();
    assert_eq!(bound.pages.borrow().shown().key, original);
    let (first_key, first_queue) = {
        let pages = bound.pages.borrow();
        let page = pages
            .session()
            .pages
            .iter()
            .find(|p| {
                matches!(
                    p.content,
                    PageContent::Downloader {
                        kind: DownloaderKind::Urls,
                        ..
                    }
                )
            })
            .unwrap();
        let PageContent::Downloader { queues, .. } = &page.content else {
            unreachable!()
        };
        (page.key, queues[0])
    };
    assert_eq!(queued(&store, first_queue), 1);
    bound.clipboard_monitor.poll();
    assert_eq!(queued(&store, first_queue), 1);
    *text.borrow_mut() =
        "https://unknown.example/1\nhttps://unlinked.example/1\nhttps://watch.example/1".into();
    bound.clipboard_monitor.poll();
    assert_eq!(bound.pages.borrow().page_count(), before + 1);
    bound.clipboard_monitor.toggle(true);
    bound.clipboard_monitor.poll();
    assert_eq!(bound.pages.borrow().page_count(), before + 2);
    assert_eq!(bound.pages.borrow().shown().key, original);
    let count: i64 = store
        .read(|c| {
            Ok(c.query_row(
                "SELECT count(*) FROM import_queues WHERE kind='watcher'",
                [],
                |r| r.get(0),
            )?)
        })
        .unwrap();
    assert_eq!(count, 1);
    (bound.open_page)(&page_chooser::NewPage::Urls);
    let (second_key, second_queue) = {
        let pages = bound.pages.borrow();
        let page = pages.shown();
        let PageContent::Downloader { queues, .. } = &page.content else {
            unreachable!()
        };
        (page.key, queues[0])
    };
    assert_ne!(second_key, first_key);
    *text.borrow_mut() = "https://file.example/2.jpg".into();
    bound.clipboard_monitor.poll();
    assert_eq!(queued(&store, second_queue), 1);
    assert_eq!(queued(&store, first_queue), 1);
    bound.pages.borrow_mut().close_shown().unwrap();
    *text.borrow_mut() = "https://file.example/3.jpg".into();
    bound.clipboard_monitor.poll();
    assert_eq!(
        queued(&store, first_queue),
        2,
        "closed importer is not reused"
    );
    let flags: settings::ClipboardUrls = store.read(settings::get).unwrap();
    assert!(flags.watchers && flags.other_recognised);
}

fn choose(ui: &MainWindow, target: &str) -> bool {
    let titles = ui.get_menu_titles();
    let network = (0..titles.row_count())
        .find(|&i| titles.row_data(i).unwrap().label == "network")
        .unwrap();
    ui.invoke_menu_title_pressed(network as i32, 80.0, 22.0);
    for label in ["downloaders", "watch clipboard for urls"] {
        let panes = ui.get_menu_panes();
        let p = panes.row_count() - 1;
        let lines = panes.row_data(p).unwrap().lines;
        let i = (0..lines.row_count())
            .find(|&i| lines.row_data(i).unwrap().label == label)
            .unwrap();
        ui.invoke_menu_line_hovered(p as i32, i as i32, 300.0, 40.0, 100.0);
    }
    let panes = ui.get_menu_panes();
    let p = panes.row_count() - 1;
    let lines = panes.row_data(p).unwrap().lines;
    let i = (0..lines.row_count())
        .find(|&i| lines.row_data(i).unwrap().label == target)
        .unwrap();
    let entry = lines.row_data(i).unwrap();
    assert!(entry.usable);
    ui.invoke_menu_line_clicked(p as i32, i as i32, 0.0, 0.0, 0.0);
    entry.checked
}

#[test]
fn independent_menu_switches_persist_on_reopen() {
    let (_dirs, store) = store();
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    assert!(!choose(&ui, "watcher urls"));
    assert!(!choose(&ui, "other recognised urls"));
    assert!(choose(&ui, "watcher urls"));
    drop(bound);
    drop(ui);
    let reopened = MainWindow::new().unwrap();
    let _bound = bind(&reopened, Pages::open(store.clone()).unwrap());
    assert!(!choose(&reopened, "watcher urls"));
    assert!(choose(&reopened, "other recognised urls"));
}

#[test]
fn empty_nested_notebooks_select_their_first_child_and_keep_both_destinations_inside() {
    let (_dirs, store) = store();
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store).unwrap());
    (bound.open_page)(&page_chooser::NewPage::Pages);
    // Ordinary notebook creation supplies a blank search child. Close it
    // explicitly to exercise clipboard routing into an empty notebook.
    bound.pages.borrow_mut().close_shown().unwrap();
    (bound.open_page)(&page_chooser::NewPage::Pages);
    bound.pages.borrow_mut().close_shown().unwrap();
    let notebook = bound.pages.borrow().shown().key;
    assert!(matches!(
        &bound.pages.borrow().shown().content,
        PageContent::Pages(children) if children.is_empty()
    ));
    hydrus_gui::set_paster(|| "https://watch.example/1\nhttps://file.example/1.jpg".into());
    bound.clipboard_monitor.toggle(true);
    bound.clipboard_monitor.toggle(false);
    bound.clipboard_monitor.poll();
    let pages = bound.pages.borrow();
    let root = pages.session().pages.last().unwrap();
    let PageContent::Pages(children) = &root.content else {
        unreachable!()
    };
    let inner = &children[0];
    assert_eq!(inner.key, notebook);
    let PageContent::Pages(importers) = &inner.content else {
        unreachable!()
    };
    assert_eq!(importers.len(), 2);
    assert_eq!(pages.shown().key, importers[0].key);
    assert_eq!(pages.tabs().len(), 3);
    assert_eq!(ui.get_tab_rows().row_count(), 3);
}

#[test]
fn clipboard_failure_is_a_global_message_and_toggling_retries_it() {
    let (_dirs, store) = store();
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    (bound.open_page)(&page_chooser::NewPage::Watcher);
    let reads = Rc::new(std::cell::Cell::new(0));
    hydrus_gui::set_clipboard_reader({
        let reads = reads.clone();
        move || {
            reads.set(reads.get() + 1);
            Err("test access failure".into())
        }
    });
    bound.clipboard_monitor.toggle(true);
    bound.clipboard_monitor.poll();
    bound.clipboard_monitor.poll();
    assert_eq!(reads.get(), 1, "fatal errors suspend reads");
    let now = hydrus_core::time::TimestampMs::now().millis() / 1000;
    let jobs = store
        .read(|conn| hydrus_store::popups::all(conn, now))
        .unwrap();
    assert!(jobs.iter().any(|job| job.status_text_1.as_deref()
        == Some("Could not access the clipboard: test access failure")));
    hydrus_gui::set_clipboard_reader(|| Ok(Some("https://watch.example/1".into())));
    bound.clipboard_monitor.toggle(true);
    bound.clipboard_monitor.toggle(true);
    bound.clipboard_monitor.poll();
    let count: i64 = store
        .read(|c| {
            Ok(c.query_row(
                "SELECT count(*) FROM import_queues WHERE kind='watcher'",
                [],
                |r| r.get(0),
            )?)
        })
        .unwrap();
    assert_eq!(
        count, 1,
        "toggle retries the current clipboard even between ticks"
    );
}
