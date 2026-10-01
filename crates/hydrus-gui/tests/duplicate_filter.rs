//! The duplicate filter's model, on the families of near-duplicate images in
//! `oracle/fixtures/auto_resolution/` after a similar-files search: pairs
//! come a batch at a time, decisions wait for the batch's end (and can be
//! undone until then), files already dealt with are skipped, and a commit
//! writes the relationships and deletes what was asked.

use std::sync::Arc;

use hydrus_core::service::builtin_keys;
use hydrus_core::{HashId, ServiceId};
use hydrus_duplicates::potentials::PotentialsQuery;
use hydrus_gui::duplicate_filter::{Decision, DuplicateFilter, Step};
use hydrus_import::{FileImportOptions, FileImporter};
use hydrus_media::MediaTools;
use hydrus_search::{FileSearchContext, LocationContext};
use hydrus_store::Store;
use hydrus_store::duplicates::{
    DuplicateFilterSettings, FileScope, PairOrder, PairSearchKind, PixelDuplicates,
};
use hydrus_store::similar::{self, SimilarFilesSettings};

fn store_with_pairs() -> (tempfile::TempDir, Arc<Store>) {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let importer = FileImporter::new(Arc::clone(&store), MediaTools::new());
    let folder = hydrus_testkit::fixture_path("auto_resolution");
    let mut names: Vec<_> = std::fs::read_dir(&folder)
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    names.sort();
    for path in names {
        importer
            .import_path(&path, &FileImportOptions::default())
            .unwrap();
    }
    store
        .write(|ctx| {
            hydrus_store::settings::set(
                ctx.conn(),
                &SimilarFilesSettings {
                    search_distance: 4,
                    ..SimilarFilesSettings::default()
                },
            )
        })
        .unwrap();
    while similar::run_search(&store, 100).unwrap() > 0 {}
    (dir, store)
}

fn my_files(store: &Store) -> (ServiceId, hydrus_core::ServiceKey) {
    let snapshot = store.snapshot();
    let service = snapshot.services.builtin(builtin_keys::MY_FILES).unwrap();
    (service.id, service.key.clone())
}

fn filter(store: &Arc<Store>, group_mode: bool) -> DuplicateFilter {
    let (id, key) = my_files(store);
    let search = FileSearchContext {
        location: LocationContext::single(key),
        ..FileSearchContext::default()
    };
    let query = PotentialsQuery {
        scope: FileScope::Domains {
            current: vec![id],
            deleted: Vec::new(),
        },
        kind: PairSearchKind::OneFileMatchesOneSearch,
        pixel_duplicates: PixelDuplicates::Allowed,
        max_hamming_distance: 4,
        search_1: search.clone(),
        search_2: search,
    };
    DuplicateFilter::new(
        Arc::clone(store),
        query,
        PairOrder::MaxFilesize,
        false,
        group_mode,
    )
    .unwrap()
}

fn in_my_files(store: &Store, file: HashId) -> bool {
    let (id, _) = my_files(store);
    store
        .read(|conn| hydrus_store::media::current_in(conn, id, &[file]))
        .unwrap()
        .contains(&file)
}

fn deletion_reason(store: &Store, file: HashId) -> Option<String> {
    let snapshot = store.snapshot();
    store
        .read(|conn| hydrus_store::media::load(conn, &snapshot.services, None, &[file]))
        .unwrap()
        .results[0]
        .deletion_reason
        .clone()
}

/// Skip pairs until the batch ends.
fn skip_to_the_end(filter: &mut DuplicateFilter) -> Step {
    loop {
        let step = filter.decide(Decision::Skip).unwrap();
        if step != Step::Showing {
            return step;
        }
    }
}

#[test]
fn decisions_wait_for_the_end_of_the_batch_and_can_be_undone() {
    let (_dir, store) = store_with_pairs();
    let mut filter = filter(&store, false);
    assert_eq!(filter.load_batch().unwrap(), Step::Showing);
    assert!(
        filter.index_text().starts_with("File One - 1/"),
        "{}",
        filter.index_text()
    );
    assert!(filter.index_text().ends_with(" - no decisions yet"));
    let (a, b) = filter.current().unwrap();

    // the file shown is compared with the other
    let comparison = filter.comparison(0).unwrap();
    assert!(comparison.statements.iter().any(|s| s.key == "mime"));
    filter.switch();
    assert_eq!(filter.current(), Some((b, a)));
    assert!(filter.index_text().starts_with("File Two - 1/"));
    filter.switch();

    assert_eq!(
        filter.decide(Decision::BETTER_DELETE_OTHER).unwrap(),
        Step::Showing
    );
    assert!(filter.index_text().ends_with(" - 1 decisions"));
    // no pair with the file to be deleted comes up again
    let next = filter.current().unwrap();
    assert!(next.0 != b && next.1 != b);

    assert!(filter.back());
    assert_eq!(filter.current(), Some((a, b)));
    assert!(filter.index_text().ends_with(" - no decisions yet"));
    assert!(!filter.back(), "nothing before the first pair");

    filter.decide(Decision::BETTER_DELETE_OTHER).unwrap();
    // with pairs skipped by hand, the filter asks
    assert_eq!(
        skip_to_the_end(&mut filter),
        Step::Confirm {
            question: "commit 1 decisions and continue?".into()
        }
    );
    assert!(in_my_files(&store, b), "nothing is written before a commit");

    // going back from the question returns to the last pair
    assert!(filter.back());
    assert_eq!(
        filter.decide(Decision::Skip).unwrap(),
        Step::Confirm {
            question: "commit 1 decisions and continue?".into()
        }
    );

    let step = filter.commit().unwrap();
    assert!(matches!(step, Step::Showing | Step::Finished), "{step:?}");
    assert!(in_my_files(&store, a));
    assert!(!in_my_files(&store, b));
    assert_eq!(
        deletion_reason(&store, b).as_deref(),
        Some("Deleted in Duplicate Filter (better/worse, worse file deleted).")
    );
}

#[test]
fn the_file_shown_is_the_one_decided_on() {
    let (_dir, store) = store_with_pairs();
    let mut filter = filter(&store, false);
    filter.load_batch().unwrap();
    let (first, second) = filter.current().unwrap();
    filter.switch();
    filter.decide(Decision::BETTER_DELETE_OTHER).unwrap();
    assert!(matches!(skip_to_the_end(&mut filter), Step::Confirm { .. }));
    filter.commit().unwrap();
    assert!(in_my_files(&store, second));
    assert!(!in_my_files(&store, first));
}

#[test]
fn small_batches_commit_themselves_until_no_pairs_are_left() {
    let (_dir, store) = store_with_pairs();
    store
        .write(|ctx| {
            hydrus_store::settings::set(
                ctx.conn(),
                &DuplicateFilterSettings {
                    auto_commit_batch_size: Some(10_000),
                    ..DuplicateFilterSettings::default()
                },
            )
        })
        .unwrap();
    let mut filter = filter(&store, false);
    let mut step = filter.load_batch().unwrap();
    let mut decided = 0;
    while step == Step::Showing {
        step = filter.decide(Decision::FALSE_POSITIVE).unwrap();
        decided += 1;
        assert!(decided < 10_000);
    }
    assert_eq!(step, Step::Finished);
    assert!(decided > 0);
}

#[test]
fn group_mode_keeps_to_one_group() {
    let (_dir, store) = store_with_pairs();
    let mut filter = filter(&store, true);
    assert_eq!(filter.load_batch().unwrap(), Step::Showing);
    // every pair skipped by hand: another group?
    assert_eq!(skip_to_the_end(&mut filter), Step::SkippedGroup);
    assert_eq!(filter.new_group().unwrap(), Step::Showing);
}

#[test]
fn the_filter_opens_from_a_duplicates_page_and_compares_the_pair() {
    use hydrus_core::duplicates::DuplicatesSearch;
    use hydrus_core::pages::{DuplicatesPage, Page, PageContent, PageKey, Session};
    use hydrus_gui::{MainWindow, Pages, bind, headless};
    use hydrus_store::sessions::{self, LAST_SESSION};
    use slint::Model as _;

    let windows = headless::init();
    let (_dir, store) = store_with_pairs();
    let (_, key) = my_files(&store);
    let search = FileSearchContext {
        location: LocationContext::single(key),
        ..FileSearchContext::default()
    };
    let duplicates = DuplicatesPage::new(DuplicatesSearch {
        search_1: search.clone(),
        search_2: search,
        kind: PairSearchKind::OneFileMatchesOneSearch,
        pixel_duplicates: PixelDuplicates::Allowed,
        max_hamming_distance: 4,
    });
    let session = Session {
        name: LAST_SESSION.into(),
        pages: vec![Page {
            key: PageKey::random(),
            name: "duplicates".into(),
            content: PageContent::Duplicates {
                duplicates,
                sort: None,
            },
        }],
    };
    store
        .write(move |ctx| sessions::save(ctx.conn(), &session, 0))
        .unwrap();

    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(Arc::clone(&store)).unwrap());
    assert!(ui.get_can_filter());
    assert!(
        ui.get_note().contains("potential pairs to filter"),
        "{}",
        ui.get_note()
    );
    ui.invoke_launch_filter();
    let filter = bound
        .filter
        .borrow()
        .as_ref()
        .map(slint::ComponentHandle::clone_strong)
        .expect("the filter opened");
    assert_eq!(filter.get_question(), "");
    assert!(filter.get_index_text().starts_with("File One - 1/"));
    assert!(filter.get_statements().row_count() > 0);

    // the slow statements come from their thread
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
    while filter.get_score_text().ends_with('\u{2026}') && std::time::Instant::now() < deadline {
        slint::platform::update_timers_and_animations();
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    let score = filter.get_score_text();
    assert!(!score.ends_with('\u{2026}'), "{score}");
    assert!(
        score.starts_with("score: ") || score == "no score difference",
        "{score}"
    );
    let window = windows.get(1).expect("the filter's window");
    let pixels = headless::render(&window, 1200, 800);
    let shots = std::path::Path::new(env!("CARGO_TARGET_TMPDIR"));
    headless::save_png(&shots.join("duplicate_filter.png"), &pixels, 1200, 800).unwrap();

    filter.invoke_switch_media();
    assert!(filter.get_index_text().starts_with("File Two - 1/"));
    filter.invoke_decide("better-delete".into());
    assert!(filter.get_index_text().ends_with(" - 1 decisions"));

    // closing with a decision pending asks first
    filter.invoke_close_requested();
    assert_eq!(filter.get_question(), "commit 1 decisions?");
    filter.invoke_answer(2);
    assert_eq!(filter.get_question(), "", "back to filtering");
    filter.invoke_close_requested();
    filter.invoke_answer(1);
    assert!(bound.filter.borrow().is_none(), "closed, forgetting");

    // a new duplicates page from the page chooser: special, then duplicates
    ui.invoke_new_page();
    let main_window = windows.get(0).expect("the main window");
    let pixels = headless::render(&main_window, 1000, 700);
    headless::save_png(&shots.join("page_chooser.png"), &pixels, 1000, 700).unwrap();
    ui.invoke_chooser_pressed(6);
    assert_eq!(
        ui.get_chooser_labels().row_data(3).unwrap(),
        "duplicates processing"
    );
    ui.invoke_chooser_pressed(4);
    assert_eq!(bound.pages.borrow().shown().name, "duplicates");
    assert!(ui.get_can_filter());
    assert!(ui.get_note().contains("potential pairs to filter"));
    // downloader pages can't be made here yet
    ui.invoke_new_page();
    ui.invoke_chooser_pressed(4);
    ui.invoke_chooser_pressed(8);
    assert!(ui.get_error().contains("downloader pages"));
    assert_eq!(ui.get_chooser_labels().row_count(), 0);
}

#[test]
fn duplicates_pages_kept_by_an_earlier_import_still_open() {
    use hydrus_core::duplicates::PairOrder;
    let fixture = hydrus_testkit::fixture_json("gui_sessions.json");
    for case in fixture["duplicates_pages"].as_array().unwrap() {
        // the page's data, as `PageContent::Other` keeps it: the page and
        // its files
        let stored = serde_json::json!([case["stored"], []]);
        let page = hydrus_store::import::stored_duplicates_page(&stored).expect("read");
        let variables = &case["facts"]["variables"];
        let search = &variables["potential_duplicates_search_context"][2];
        assert_eq!(
            page.group_mode,
            variables["filter_group_mode"].as_bool().unwrap()
        );
        assert_eq!(
            page.ascending,
            variables["duplicate_pair_sort_asc"].as_bool().unwrap()
        );
        assert_eq!(
            Some(page.order),
            PairOrder::from_code(variables["duplicate_pair_sort_type"].as_i64().unwrap())
        );
        assert_eq!(
            i64::from(page.search.max_hamming_distance),
            search[4].as_i64().unwrap()
        );
    }
}
