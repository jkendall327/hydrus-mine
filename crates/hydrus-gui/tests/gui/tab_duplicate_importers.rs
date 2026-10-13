//! "duplicate page" on the tab menu for the importer pages, replayed against
//! the reference's (`oracle/fixtures/tab_duplicate_importers.json`, from
//! `oracle/record_tab_duplicate_importers.py`): a url import page holding four
//! URLs and a simple downloader page holding two, each duplicated from its tab's
//! menu; the tabs, each page's file log and pause state compared, then a URL
//! added to the copy and to the original to show the two queues are separate.

use std::sync::Arc;

use serde_json::{Value, json};
use slint::{ComponentHandle as _, Model as _};

use hydrus_core::import_options::ImportOptionsSlice;
use hydrus_core::pages::{DownloaderKind, Page, PageContent, PageKey, Session};
use hydrus_gui::{MainWindow, Pages, bind, headless};
use hydrus_search::FileSearchContext;
use hydrus_store::Store;
use hydrus_store::import::import_legacy;
use hydrus_store::queues::{self, FileSeedMeta, NewFileSeed, QueueKind, SeedType};
use hydrus_store::sessions::{self, LAST_SESSION};

fn seeds(urls: &[String]) -> Vec<NewFileSeed> {
    urls.iter()
        .map(|u| NewFileSeed {
            seed_type: SeedType::Url,
            data: u.clone(),
            data_for_comparison: u.clone(),
            source_time: None,
            referral_url: None,
            meta: FileSeedMeta::default(),
        })
        .collect()
}

fn strings(v: &Value) -> Vec<String> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|u| u.as_str().unwrap().to_owned())
        .collect()
}

/// A queue as the reference shows an importer: its URLs and whether it is paused.
fn queue_state(store: &Store, queue: i64) -> Value {
    store
        .read(move |c| {
            let q = queues::queue(c, queue)?.unwrap();
            let urls: Vec<String> = queues::file_seeds(c, queue)?
                .into_iter()
                .map(|s| s.data)
                .collect();
            Ok(json!({ "urls": urls, "paused": q.files_paused }))
        })
        .unwrap()
}

// leaf: audit-options-tabs-context-action-2201-duplicate-page
#[test]
fn duplicating_an_importer_page_copies_its_queue_apart_from_the_original() {
    let recorded = hydrus_testkit::fixture_json("tab_duplicate_importers.json");
    let legacy = hydrus_testkit::legacy_fixture("basic");
    let native = tempfile::tempdir().unwrap();
    import_legacy(
        legacy.path(),
        &native.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let _windows = headless::init();
    for case in recorded["pages"].as_array().unwrap() {
        let name = case["page"].as_str().unwrap();
        let store: Arc<Store> = Store::open(native.path()).unwrap();
        let (url_list, simple_list) = (
            strings(&recorded["urls"]),
            strings(&recorded["simple_urls"]),
        );
        let (url_queue, simple_queue) = store
            .write(move |ctx| {
                let c = ctx.conn();
                let options = ImportOptionsSlice::default();
                let u = queues::create_queue(c, QueueKind::Urls, "url import", None, &options, 0)?;
                queues::add_file_seeds(c, u, &seeds(&url_list), false, 0)?;
                queues::set_paused(c, u, Some(true), Some(true))?;
                let q = queues::create_simple_downloader(
                    c,
                    None,
                    &options,
                    &queues::SimpleDownloader::default(),
                    0,
                )?;
                queues::add_file_seeds(c, q, &seeds(&simple_list), false, 0)?;
                queues::set_paused(c, q, Some(true), Some(true))?;
                Ok((u, q))
            })
            .unwrap();
        let page = |n: &str, content| Page {
            key: PageKey::random(),
            name: n.into(),
            content,
        };
        let downloader = |n: &str, kind, queue| {
            page(
                n,
                PageContent::Downloader {
                    kind,
                    queues: vec![queue],
                    sort: None,
                    page: None,
                },
            )
        };
        let search = PageContent::Search {
            search: FileSearchContext::default(),
            synchronised: false,
            sort: None,
            lock: None,
            collect: None,
        };
        let session = Session {
            name: LAST_SESSION.into(),
            pages: vec![
                downloader("url import", DownloaderKind::Urls, url_queue),
                downloader("simple downloader", DownloaderKind::Simple, simple_queue),
                page("c", search),
            ],
        };
        store
            .write(move |ctx| sessions::save(ctx.conn(), &session, 0))
            .unwrap();
        let target_tab = i32::from(name != "url import");
        let ui = MainWindow::new().unwrap();
        ui.show().unwrap();
        let bound = bind(&ui, Pages::open(store.clone()).unwrap());
        let tabs = || -> Vec<String> {
            bound
                .pages
                .borrow()
                .tabs()
                .into_iter()
                .next()
                .map(|t| t.names)
                .unwrap_or_default()
        };
        let queues_of = || -> Vec<i64> {
            bound
                .pages
                .borrow()
                .session()
                .pages
                .iter()
                .filter(|p| p.name == name)
                .filter_map(|p| match &p.content {
                    PageContent::Downloader { queues, .. } => queues.first().copied(),
                    _ => None,
                })
                .collect()
        };
        let steps = case["steps"].as_array().unwrap();
        let expect = |step: &Value, ids: &[i64]| {
            let want: Vec<Value> = step["state"]["pages"]
                .as_array()
                .unwrap()
                .iter()
                .map(|p| json!({ "urls": p["urls"], "paused": p["paused"] }))
                .collect();
            let have: Vec<Value> = ids.iter().map(|&q| queue_state(&store, q)).collect();
            assert_eq!(have, want, "{}", step["do"]);
        };
        assert_eq!(tabs(), strings(&steps[0]["state"]["tabs"]));
        expect(&steps[0], &queues_of());
        // the tab's right-click menu: "duplicate page"
        ui.invoke_tab_menu_requested(0, target_tab, 30.0, 55.0);
        let lines = ui.get_menu_panes().row_data(0).unwrap().lines;
        let at = (0..lines.row_count())
            .position(|i| lines.row_data(i).unwrap().label == "duplicate page")
            .unwrap();
        ui.invoke_menu_line_clicked(0, i32::try_from(at).unwrap(), 200.0, 100.0, 10.0);
        assert_eq!(tabs(), strings(&steps[1]["state"]["tabs"]));
        let ids = queues_of();
        assert_eq!(ids.len(), 2);
        assert_ne!(ids[0], ids[1], "the copy has a queue of its own");
        expect(&steps[1], &ids);
        // a URL given to the copy, then to the original
        for (step, which) in [(&steps[2], 1), (&steps[3], 0)] {
            let extra = strings(&json!([recorded["extra"]]));
            let extra = if which == 0 {
                vec![format!("{}/2", extra[0])]
            } else {
                extra
            };
            let target = ids[which];
            store
                .write(move |ctx| {
                    queues::add_file_seeds(ctx.conn(), target, &seeds(&extra), false, 0)
                })
                .unwrap();
            expect(step, &ids);
        }
    }
}
