//! Recursive tab refresh and advanced copyable weights against the real popup.
use hydrus_core::{
    HashId, ServiceKey,
    pages::{Page, PageContent, PageKey, Session},
};
use hydrus_gui::{MainWindow, Pages, bind, headless};
use hydrus_search::{FileSearchContext, LocationContext};
use hydrus_store::{Store, sessions, settings};
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
    ([legacy, native], store)
}

fn search(name: &str) -> Page {
    Page {
        key: PageKey::random(),
        name: name.into(),
        content: PageContent::Search {
            search: FileSearchContext {
                location: LocationContext::single(ServiceKey::new(
                    hydrus_core::service::builtin_keys::MY_FILES.to_vec(),
                )),
                predicates: hydrus_search::api::parse_api_search(&serde_json::json!([
                    "system:everything"
                ]))
                .unwrap(),
                ..FileSearchContext::default()
            },
            synchronised: false,
            sort: None,
            lock: None,
            collect: None,
        },
    }
}

fn notebook(name: &str, children: Vec<Page>) -> Page {
    Page {
        key: PageKey::random(),
        name: name.into(),
        content: PageContent::Pages(children),
    }
}

fn labels(ui: &MainWindow) -> Vec<String> {
    let lines = ui.get_menu_panes().row_data(0).unwrap().lines;
    (0..lines.row_count())
        .map(|i| lines.row_data(i).unwrap().label.to_string())
        .collect()
}

fn choose(ui: &MainWindow, label: &str) {
    let index = labels(ui).iter().position(|row| row == label).unwrap();
    ui.invoke_menu_line_clicked(0, i32::try_from(index).unwrap(), 200.0, 100.0, 10.0);
}

#[test]
fn notebook_refresh_resumes_initialized_descendants_and_preserves_background_selection() {
    let _windows = headless::init();
    let (_dirs, store) = store();
    let fixture = hydrus_testkit::fixture_json("tab_refresh.json");
    let first = search("first");
    let second = search("second");
    let skipped = search("uninitialized");
    let outside = search("outside");
    let keys = [first.key, second.key, skipped.key, outside.key];
    let group = notebook(
        "group",
        vec![first, notebook("nested", vec![second]), skipped],
    );
    let empty = notebook("empty", vec![]);
    let session = Session {
        name: sessions::LAST_SESSION.into(),
        pages: vec![group, outside, empty],
    };
    store
        .write(move |ctx| {
            sessions::save(ctx.conn(), &session, 1)?;
            sessions::set_shown(ctx.conn(), sessions::LAST_SESSION, Some(&keys[3]))
        })
        .unwrap();
    // The recorded skipped page had already queried before Qt marked its media
    // panel uninitialized. Preserve that initial media without opening it here.
    let initial = hydrus_testkit::fixture_json("media_collect.json");
    let initial_files: Vec<HashId> = initial["files"]
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
    store
        .write(move |ctx| sessions::set_page_files(ctx.conn(), &keys[2], &initial_files))
        .unwrap();
    let mut pages = Pages::open(store.clone()).unwrap();
    for key in &keys[..2] {
        assert!(!pages.page(key).unwrap().borrow().synchronised());
    }
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, pages);
    ui.invoke_tab_menu_requested(0, 0, 30.0, 55.0);
    choose(&ui, "refresh all this page's pages");
    for (i, key) in keys.iter().enumerate() {
        let page = bound.pages.borrow_mut().page(key).unwrap();
        assert_eq!(
            page.borrow().synchronised(),
            fixture["refresh"]["after_sync"][i].as_bool().unwrap()
        );
        assert_eq!(
            page.borrow().files().len() as u64,
            fixture["refresh"]["counts"][i].as_u64().unwrap(),
            "descendant {i}"
        );
    }
    assert_eq!(
        bound.pages.borrow().shown().name,
        fixture["refresh"]["shown"].as_str().unwrap()
    );
    ui.invoke_tab_menu_requested(0, 2, 30.0, 55.0);
    assert!(!labels(&ui).iter().any(|label| label.starts_with("refresh")));
    ui.invoke_tab_menu_requested(0, 1, 30.0, 55.0);
    choose(&ui, "refresh this page");
    assert!(bound.current.borrow().borrow().synchronised());
    let before = bound.pages.borrow().session().pages.clone();
    bound.pages.borrow_mut().refresh_tab_tree(PageKey::random());
    assert_eq!(bound.pages.borrow().session().pages, before);
    (bound.sync)();
    let mut reopened = Pages::open(store.clone()).unwrap();
    assert_eq!(reopened.shown().key, keys[3]);
    assert!(reopened.page(&keys[0]).unwrap().borrow().synchronised());
    assert!(!reopened.page(&keys[2]).unwrap().borrow().synchronised());
    let locked = reopened.page(&keys[0]).unwrap();
    locked.borrow_mut().lock_search();
    locked.borrow_mut().set_synchronised(false);
    let files = locked.borrow().files();
    reopened.refresh_tab_tree(keys[0]);
    assert!(!locked.borrow().synchronised());
    assert_eq!(locked.borrow().files(), files);
}

#[test]
fn advanced_weight_counts_repeated_media_and_both_seed_logs_and_copies_exact_label() {
    use hydrus_core::{
        import_options::ImportOptionsSlice,
        pages::{DownloaderKind, DownloaderPageState},
    };
    use hydrus_store::queues::{
        self, FileSeedMeta, GallerySeedMeta, NewFileSeed, NewGallerySeed, QueueKind, SeedType,
    };
    let _windows = headless::init();
    let (_dirs, store) = store();
    let fixture = hydrus_testkit::fixture_json("tab_refresh.json");
    let hashes: Vec<HashId> = fixture["weights"]["hashes"]
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
    let a = search("files a");
    let b = search("files b");
    let importer_key = PageKey::random();
    let queue = store
        .write(move |ctx| {
            let queue = queues::create_queue(
                ctx.conn(),
                QueueKind::Urls,
                "weighted importer",
                Some(&importer_key.0),
                &ImportOptionsSlice::default(),
                1,
            )?;
            queues::set_paused(ctx.conn(), queue, Some(true), Some(true))?;
            let files: Vec<_> = (0..54)
                .map(|i| NewFileSeed {
                    seed_type: SeedType::Url,
                    data: format!("https://files.example/{i}.jpg"),
                    data_for_comparison: format!("https://files.example/{i}.jpg"),
                    source_time: None,
                    referral_url: None,
                    meta: FileSeedMeta::default(),
                })
                .collect();
            queues::add_file_seeds(ctx.conn(), queue, &files, false, 1)?;
            queues::add_gallery_seeds(
                ctx.conn(),
                queue,
                &[NewGallerySeed {
                    url: "https://gallery.example/page/1".into(),
                    can_generate_more_pages: true,
                    referral_url: None,
                    meta: GallerySeedMeta::default(),
                }],
                None,
                1,
            )?;
            Ok(queue)
        })
        .unwrap();
    let importer = Page {
        key: importer_key,
        name: "weighted importer".into(),
        content: PageContent::Downloader {
            kind: DownloaderKind::Urls,
            queues: vec![queue],
            sort: None,
            page: Some(Box::new(DownloaderPageState::default())),
        },
    };
    let leaf_keys = [a.key, b.key, importer_key];
    let group = notebook("weighted group", vec![a, b, importer]);
    let group_key = group.key;
    let session = Session {
        name: sessions::LAST_SESSION.into(),
        pages: vec![group],
    };
    store
        .write(move |ctx| {
            sessions::save(ctx.conn(), &session, 1)?;
            sessions::set_page_files(ctx.conn(), &leaf_keys[0], &hashes[..2])?;
            sessions::set_page_files(ctx.conn(), &leaf_keys[1], &hashes[1..])
        })
        .unwrap();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    for (i, key) in leaf_keys.iter().enumerate() {
        assert_eq!(
            bound.pages.borrow().page_weight(*key),
            fixture["weights"]["leaf_weights"][i].as_u64()
        );
    }
    assert_eq!(
        bound.pages.borrow().page_weight(group_key),
        fixture["weights"]["weight"].as_u64()
    );
    assert_eq!(bound.pages.borrow().page_weight(PageKey::random()), None);
    let copied = Rc::new(RefCell::new(Vec::<String>::new()));
    hydrus_gui::set_clipper({
        let copied = copied.clone();
        move |clip| {
            if let hydrus_gui::Clip::Text(text) = clip {
                copied.borrow_mut().push(text.clone());
            }
        }
    });
    for step in fixture["weights"]["menus"].as_array().unwrap() {
        let advanced = step["advanced"].as_bool().unwrap();
        store
            .write_and_refresh(move |ctx| {
                settings::set(ctx.conn(), &settings::AdvancedMode(advanced))
            })
            .unwrap();
        ui.invoke_tab_menu_requested(0, 0, 30.0, 55.0);
        assert_eq!(
            labels(&ui)
                .iter()
                .any(|label| label.starts_with("page weight:")),
            advanced
        );
        if advanced {
            let label = step["entries"][0].as_str().unwrap();
            let before = bound.pages.borrow().session().pages.clone();
            choose(&ui, label);
            assert_eq!(
                copied.borrow().last().unwrap(),
                step["clipboard"][0][1].as_str().unwrap()
            );
            assert_eq!(bound.pages.borrow().session().pages, before);
        }
    }
    let reopened = Pages::open(store.clone()).unwrap();
    assert_eq!(
        reopened.page_weight(group_key),
        fixture["weights"]["weight"].as_u64()
    );
    assert!(
        store
            .read(|conn| queues::queue(conn, queue))
            .unwrap()
            .unwrap()
            .files_paused
    );

    // Importer refresh broadcasts the existing sort, retaining selection and
    // the outside shown leaf even after manual thumbnail rearrangement.
    store
        .write(move |ctx| {
            sessions::set_page_files(ctx.conn(), &importer_key, &[HashId(1), HashId(2)])
        })
        .unwrap();
    let mut pages = Pages::open(store.clone()).unwrap();
    let importer = pages.page(&importer_key).unwrap();
    importer
        .borrow_mut()
        .set_sort_by(hydrus_search::SortBy::FileSize);
    let sorted = importer.borrow().files();
    importer.borrow_mut().select(0);
    let selected = importer.borrow().selected_files();
    importer
        .borrow_mut()
        .rearrange(hydrus_gui::thumbnail_menu::Rearrange::End);
    assert_ne!(importer.borrow().files(), sorted);
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, pages);
    let shown = bound.pages.borrow().shown().key;
    ui.invoke_tab_menu_requested(1, 2, 30.0, 55.0);
    choose(&ui, "refresh this page");
    assert_eq!(importer.borrow().files(), sorted);
    assert_eq!(importer.borrow().selected_files(), selected);
    assert_eq!(bound.pages.borrow().shown().key, shown);
    assert!(
        store
            .read(|conn| queues::queue(conn, queue))
            .unwrap()
            .unwrap()
            .files_paused
    );
}
