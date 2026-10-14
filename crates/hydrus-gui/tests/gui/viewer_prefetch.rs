//! Saved neighbourhood policy reaches actual viewer/archive cache consumers without SetMedia.
use hydrus_core::HashId;
use hydrus_gui::{Bound, MainWindow, OptionsWindow, Pages, bind, headless};
use hydrus_import::{FileImportOptions, FileImporter};
use hydrus_store::{
    Store,
    image_cache::Policy,
    settings,
    viewer_prefetch::{self, Preferences},
};
use slint::{ComponentHandle as _, Model as _};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};
const PREVIOUS: &str = "Num previous to prefetch in Media Viewer:";
const NEXT: &str = "Num next to prefetch in Media Viewer:";
const PERCENT: &str = "Maximum % of cache that will be prefetched per media viewer:";
fn row(window: &OptionsWindow, label: &str) -> i32 {
    i32::try_from(
        window
            .get_rows()
            .iter()
            .position(|row| row.label == label)
            .unwrap(),
    )
    .unwrap()
}
fn options(ui: &MainWindow, bound: &Bound) -> OptionsWindow {
    ui.invoke_menu_title_pressed(0, 20.0, 22.0);
    let at = ui
        .get_menu_panes()
        .row_data(0)
        .unwrap()
        .lines
        .iter()
        .position(|row| row.label == "options…")
        .unwrap();
    ui.invoke_menu_line_clicked(0, i32::try_from(at).unwrap(), 0.0, 0.0, 0.0);
    let child = bound.options.borrow().as_ref().unwrap().clone_strong();
    let at = child
        .get_pages()
        .iter()
        .position(|page| page.text == "speed and memory")
        .unwrap();
    child.invoke_page_chosen(i32::try_from(at).unwrap());
    child
}
fn pump_until(done: impl Fn() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !done() {
        assert!(
            Instant::now() < deadline,
            "owned warm worker did not finish expected cache admission"
        );
        slint::platform::update_timers_and_animations();
        std::thread::yield_now();
    }
}
fn import(store: &Arc<Store>, name: &str) -> HashId {
    let result = FileImporter::new(store.clone(), hydrus_media::MediaTools::new())
        .import_path(
            &hydrus_testkit::fixture_path(format!("image_cache/{name}")),
            &FileImportOptions::default(),
        )
        .unwrap();
    store
        .read(|conn| hydrus_store::master::hash_id(conn, &result.hash.unwrap()))
        .unwrap()
        .unwrap()
}
// leaf: audit-options-speed-and-memory-image-prefetch-num-next-to-prefetch-in-media-viewer
#[test]
fn options_cancel_hidden_modal_save_reopen_and_live_hidden_viewer_warming_leave_current_pixels_unchanged()
 {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let imported = [
        import(&store, "a.png"),
        import(&store, "b.png"),
        import(&store, "c.png"),
    ];
    store
        .write(|ctx| {
            settings::set(
                ctx.conn(),
                &Policy {
                    bytes: 400,
                    timeout: 300,
                    percentage: 50,
                },
            )?;
            settings::set(
                ctx.conn(),
                &Preferences {
                    previous: 0,
                    next: 0,
                    percentage: 50,
                    ..Preferences::default()
                },
            )?;
            let mut layout = hydrus_store::page_layout::load(ctx.conn())?;
            layout.hide_preview = true;
            settings::set(ctx.conn(), &layout)
        })
        .unwrap();
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(
        &ui,
        Pages::single(super::common::all_local_page(store.clone())),
    );
    let adapter = windows.get(0).unwrap();
    headless::render(&adapter, 1400, 1000);
    ui.invoke_search_edited("system:everything".into());
    ui.invoke_search_accepted();
    let files = bound.current.borrow().borrow().results().to_vec();
    assert_eq!(files.len(), 3);
    assert!(imported.iter().all(|file| files.contains(file)));
    ui.invoke_thumbnail_activated(0);
    let viewer = bound.viewer.borrow().as_ref().unwrap().clone_strong();
    assert_eq!(viewer.get_media().size().width, 10);
    assert_eq!(bound.image_cache.keys(), vec![files[0]]);
    let before = viewer.get_media();
    let before_pixels = before.to_rgba8().unwrap().as_bytes().to_vec();
    let cancelled = options(&ui, &bound);
    cancelled.invoke_number_edited(row(&cancelled, NEXT), 1);
    cancelled.invoke_cancel();
    cancelled.invoke_apply();
    assert_eq!(store.read(viewer_prefetch::load).unwrap().next, 0);
    let hidden = options(&ui, &bound);
    hidden.hide().unwrap();
    hidden.invoke_number_edited(row(&hidden, NEXT), 2);
    hidden.invoke_number_edited(row(&hidden, PREVIOUS), 1);
    hidden.invoke_number_edited(row(&hidden, PERCENT), 10);
    hidden.show().unwrap();
    hidden.invoke_apply();
    assert_eq!(
        store.read(viewer_prefetch::load).unwrap(),
        Preferences {
            previous: 0,
            next: 0,
            percentage: 50,
            ..Preferences::default()
        }
    );
    let modal = options(&ui, &bound);
    let suggested = modal
        .get_pages()
        .iter()
        .position(|page| page.text == "tag suggestions")
        .unwrap();
    modal.invoke_page_chosen(i32::try_from(suggested).unwrap());
    modal.invoke_related_weights_clicked();
    let weights = bound
        .options_suggested_tags_slot
        .weights
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    let speed = modal
        .get_pages()
        .iter()
        .position(|page| page.text == "speed and memory")
        .unwrap();
    modal.invoke_page_chosen(i32::try_from(speed).unwrap());
    modal.invoke_number_edited(row(&modal, NEXT), 2);
    modal.invoke_number_edited(row(&modal, PREVIOUS), 2);
    modal.invoke_number_edited(row(&modal, PERCENT), 10);
    weights.invoke_action("cancel".into());
    modal.invoke_apply();
    assert_eq!(store.read(viewer_prefetch::load).unwrap().next, 0);
    let saved = options(&ui, &bound);
    saved.invoke_number_edited(row(&saved, NEXT), 1);
    saved.invoke_apply();
    assert_eq!(store.read(viewer_prefetch::load).unwrap().next, 1);
    viewer.hide().unwrap();
    pump_until(|| bound.image_cache.keys().contains(&files[1]));
    assert!(
        !viewer.window().is_visible(),
        "Qt allows admitted warm work while hidden"
    );
    viewer.show().unwrap();
    assert_eq!(
        viewer.get_media().to_rgba8().unwrap().as_bytes(),
        before_pixels.as_slice(),
        "warm readiness and policy saves never SetMedia"
    );
    let reopened = options(&ui, &bound);
    assert_eq!(
        reopened
            .get_rows()
            .row_data(row(&reopened, NEXT) as usize)
            .unwrap()
            .number,
        1
    );
    reopened.invoke_number_edited(row(&reopened, PREVIOUS), 1);
    reopened.invoke_apply();
    viewer.invoke_next();
    pump_until(|| bound.image_cache.keys().contains(&files[2]));
    assert!(
        files
            .iter()
            .all(|file| bound.image_cache.keys().contains(file))
    );
    viewer.invoke_close_requested();
    assert!(bound.viewer.borrow().is_none());
    viewer.show().unwrap();
    viewer.invoke_next();
    let cache = bound.image_cache.clone();
    let clone = bound.clone();
    drop(bound);
    assert!(!cache.keys().is_empty());
    drop(clone);
    assert!(cache.keys().is_empty());
    viewer.invoke_next();
    assert!(
        cache.keys().is_empty(),
        "retained/re-shown viewer cannot revive final binding cache admission"
    );
    ui.hide().unwrap();
    let ui2 = MainWindow::new().unwrap();
    ui2.show().unwrap();
    let successor = bind(
        &ui2,
        Pages::single(super::common::all_local_page(store.clone())),
    );
    let stale = saved;
    saved_policy_unchanged(&store, &stale);
    let visible = options(&ui2, &successor);
    let native = windows.get(windows.count() - 1).unwrap();
    let pixels = headless::render(&native, 1100, 650);
    headless::save_png(
        &std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("viewer-prefetch-options.png"),
        &pixels,
        1100,
        650,
    )
    .unwrap();
    visible.invoke_cancel();
    successor.preview.close();
}
fn saved_policy_unchanged(store: &Store, stale: &OptionsWindow) {
    let before = store.read(viewer_prefetch::load).unwrap();
    stale.show().unwrap();
    stale.invoke_number_edited(row(stale, NEXT), 50);
    stale.invoke_apply();
    assert_eq!(store.read(viewer_prefetch::load).unwrap(), before);
}
// leaf: audit-options-speed-and-memory-image-prefetch-num-next-to-prefetch-in-media-viewer
#[test]
fn archive_delete_uses_saved_neighbour_counts_and_retains_current_media_after_warm_readiness() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let imported = [
        import(&store, "a.png"),
        import(&store, "b.png"),
        import(&store, "c.png"),
    ];
    store
        .write(|ctx| {
            settings::set(
                ctx.conn(),
                &Policy {
                    bytes: 400,
                    timeout: 300,
                    percentage: 50,
                },
            )?;
            settings::set(
                ctx.conn(),
                &Preferences {
                    previous: 0,
                    next: 1,
                    percentage: 50,
                    ..Preferences::default()
                },
            )?;
            let mut layout = hydrus_store::page_layout::load(ctx.conn())?;
            layout.hide_preview = true;
            settings::set(ctx.conn(), &layout)
        })
        .unwrap();
    let _headless_windows = headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(
        &ui,
        Pages::single(super::common::all_local_page(store.clone())),
    );
    ui.invoke_search_edited("system:everything".into());
    ui.invoke_search_accepted();
    let files = bound.current.borrow().borrow().results().to_vec();
    assert_eq!(files.len(), 3);
    assert!(imported.iter().all(|file| files.contains(file)));
    ui.invoke_archive_delete_filter();
    let archive = bound
        .archive_delete
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    let current = archive.get_media().to_rgba8().unwrap().as_bytes().to_vec();
    assert_eq!(archive.get_media().size().width, 10);
    pump_until(|| bound.image_cache.keys().contains(&files[1]));
    assert_eq!(
        archive.get_media().to_rgba8().unwrap().as_bytes(),
        current.as_slice()
    );
    archive.invoke_close_requested();
    assert!(bound.archive_delete.borrow().is_none());
    let cached = bound.image_cache.keys();
    archive.show().unwrap();
    archive.invoke_skip();
    assert_eq!(bound.image_cache.keys(), cached);
    bound.preview.close();
}
#[test]
fn actual_duplicate_filter_uses_the_same_saved_percentage_for_future_images_without_replacing_current()
 {
    use hydrus_core::{
        duplicates::DuplicatesSearch,
        pages::{DuplicatesPage, Page, PageContent, PageKey, Session},
    };
    use hydrus_search::{FileSearchContext, LocationContext};
    use hydrus_store::{
        duplicates::{PairSearchKind, PixelDuplicates},
        sessions::{self, LAST_SESSION},
    };
    let (_dir, store) = super::duplicate_filter::store_with_pairs();
    let (_, key) = super::duplicate_filter::my_files(&store);
    let search = FileSearchContext {
        location: LocationContext::single(key),
        ..Default::default()
    };
    let session = Session {
        name: LAST_SESSION.into(),
        pages: vec![Page {
            key: PageKey::random(),
            name: "duplicates".into(),
            content: PageContent::Duplicates {
                duplicates: DuplicatesPage::new(DuplicatesSearch {
                    search_1: search.clone(),
                    search_2: search,
                    kind: PairSearchKind::OneFileMatchesOneSearch,
                    pixel_duplicates: PixelDuplicates::Allowed,
                    max_hamming_distance: 4,
                }),
                sort: None,
            },
        }],
    };
    store
        .write(move |ctx| {
            sessions::save(ctx.conn(), &session, 0)?;
            settings::set(
                ctx.conn(),
                &Preferences {
                    percentage: 50,
                    ..Preferences::default()
                },
            )
        })
        .unwrap();
    let _headless_windows = headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    ui.invoke_launch_filter();
    let filter = bound.filter.borrow().as_ref().unwrap().clone_strong();
    let current = filter.get_media().to_rgba8().unwrap().as_bytes().to_vec();
    assert!(!current.is_empty());
    pump_until(|| bound.image_cache.keys().len() > 2);
    assert_eq!(
        filter.get_media().to_rgba8().unwrap().as_bytes(),
        current.as_slice()
    );
    let cache = bound.image_cache.clone();
    drop(bound);
    assert!(cache.keys().is_empty());
    filter.show().unwrap();
    filter.invoke_switch_media();
    assert!(
        cache.keys().is_empty(),
        "retained duplicate owner cannot admit through the retired binding cache"
    );
    filter.invoke_close_requested();
}

#[test]
fn the_duplicate_filter_prefetches_as_many_pairs_as_the_options_row_says() {
    use hydrus_core::{
        duplicates::DuplicatesSearch,
        pages::{DuplicatesPage, Page, PageContent, PageKey, Session},
    };
    use hydrus_search::{FileSearchContext, LocationContext};
    use hydrus_store::{
        duplicates::{PairSearchKind, PixelDuplicates},
        sessions::{self, LAST_SESSION},
    };
    const PAIRS: &str = "Num pairs to prefetch in Duplicate Filter:";
    let _headless_windows = headless::init();
    let mut seen = Vec::new();
    for pairs in [0_i32, 2, 1] {
        let (_dir, store) = super::duplicate_filter::store_with_pairs();
        let (_, key) = super::duplicate_filter::my_files(&store);
        let search = FileSearchContext {
            location: LocationContext::single(key),
            ..Default::default()
        };
        let session = Session {
            name: LAST_SESSION.into(),
            pages: vec![Page {
                key: PageKey::random(),
                name: "duplicates".into(),
                content: PageContent::Duplicates {
                    duplicates: DuplicatesPage::new(DuplicatesSearch {
                        search_1: search.clone(),
                        search_2: search,
                        kind: PairSearchKind::OneFileMatchesOneSearch,
                        pixel_duplicates: PixelDuplicates::Allowed,
                        max_hamming_distance: 4,
                    }),
                    sort: None,
                },
            }],
        };
        store
            .write(move |ctx| {
                sessions::save(ctx.conn(), &session, 0)?;
                settings::set(
                    ctx.conn(),
                    &Preferences {
                        percentage: 50,
                        ..Preferences::default()
                    },
                )
            })
            .unwrap();
        let ui = MainWindow::new().unwrap();
        ui.show().unwrap();
        let bound = bind(&ui, Pages::open(store.clone()).unwrap());

        // the row, as the options window shows it, edited and applied
        let window = options(&ui, &bound);
        let at = row(&window, PAIRS);
        assert_eq!(window.get_rows().row_data(at as usize).unwrap().number, 3);
        window.invoke_number_edited(at, pairs);
        window.invoke_apply();
        assert_eq!(
            store.read(viewer_prefetch::load).unwrap().duplicate_pairs,
            pairs as u64
        );

        // the filter shows its pair and decodes ahead that many more
        let mut batch = super::duplicate_filter::filter(&store, false);
        batch.load_batch().unwrap();
        let expected: std::collections::HashSet<_> =
            batch.upcoming(pairs as usize).into_iter().collect();
        assert!(
            batch.upcoming(usize::MAX).len() > expected.len(),
            "the store needs more pairs than {pairs} ahead"
        );
        ui.invoke_launch_filter();
        let filter = bound.filter.borrow().as_ref().unwrap().clone_strong();
        assert!(!filter.get_media().to_rgba8().unwrap().as_bytes().is_empty());
        pump_until(|| bound.image_cache.keys().len() >= expected.len());
        // (and no more arrive)
        let until = Instant::now() + Duration::from_millis(500);
        while Instant::now() < until {
            slint::platform::update_timers_and_animations();
            std::thread::sleep(Duration::from_millis(10));
        }
        let cached: std::collections::HashSet<_> = bound.image_cache.keys().into_iter().collect();
        assert_eq!(cached, expected, "{pairs} pairs ahead");
        seen.push(cached.len());
        filter.invoke_close_requested();
    }
    // (the more pairs the row says, the more are decoded ahead)
    assert!(seen[0] < seen[2] && seen[2] < seen[1], "{seen:?}");
}

/// The cache keys once the viewer's warm worker has gone quiet.
fn settled_keys(bound: &Bound) -> std::collections::BTreeSet<HashId> {
    let deadline = Instant::now() + Duration::from_millis(250);
    while Instant::now() < deadline {
        slint::platform::update_timers_and_animations();
        std::thread::sleep(Duration::from_millis(5));
    }
    bound.image_cache.keys().into_iter().collect()
}

/// Both counts set in the real Options rows, and a real media viewer opened on
/// each file of a three-file page: the files the cache holds afterwards are the
/// viewer's own and the neighbours the reference chose for those counts
/// (`neighbours` in `oracle/fixtures/viewer_prefetch.json`, all 18 recorded
/// cases for three files).
// leaf: audit-options-speed-and-memory-image-prefetch-num-previous-to-prefetch-in-media-viewer
#[test]
fn the_viewer_warms_the_recorded_neighbours_for_the_previous_and_next_counts_in_the_rows() {
    let fixture = hydrus_testkit::fixture_json("viewer_prefetch.json");
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    for name in ["a.png", "c.png", "d.png"] {
        import(&store, name);
    }
    store
        .write(|ctx| {
            settings::set(
                ctx.conn(),
                &Policy {
                    bytes: 400,
                    timeout: 300,
                    percentage: 50,
                },
            )?;
            let mut layout = hydrus_store::page_layout::load(ctx.conn())?;
            layout.hide_preview = true;
            settings::set(ctx.conn(), &layout)
        })
        .unwrap();
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(
        &ui,
        Pages::single(super::common::all_local_page(store.clone())),
    );
    headless::render(&windows.get(0).unwrap(), 1400, 1000);
    ui.invoke_search_edited("system:everything".into());
    ui.invoke_search_accepted();
    let files = bound.current.borrow().borrow().results().to_vec();
    assert_eq!(files.len(), 3);
    let name_of = |name: &str| files[usize::from(name.as_bytes()[0] - b'a')];
    let initial = &fixture["initial"];
    assert_eq!(
        store.read(viewer_prefetch::load).unwrap().previous,
        initial[0].as_u64().unwrap(),
        "the row starts on the reference's default"
    );

    let mut checked = 0;
    for case in fixture["neighbours"].as_array().unwrap() {
        if case["files"].as_array().unwrap().len() != 3 {
            continue;
        }
        let index = case["index"].as_u64().unwrap() as usize;
        let [previous, next] =
            ["previous", "next"].map(|k| i32::try_from(case[k].as_i64().unwrap()).unwrap());
        let window = options(&ui, &bound);
        window.invoke_number_edited(row(&window, PREVIOUS), previous);
        window.invoke_number_edited(row(&window, NEXT), next);
        window.invoke_apply();
        let saved = store.read(viewer_prefetch::load).unwrap();
        assert_eq!(
            (
                i32::try_from(saved.previous).unwrap(),
                i32::try_from(saved.next).unwrap()
            ),
            (previous, next)
        );

        bound.image_cache.clear();
        ui.invoke_thumbnail_activated(i32::try_from(index).unwrap());
        let viewer = bound.viewer.borrow().as_ref().unwrap().clone_strong();
        let mut expected: std::collections::BTreeSet<HashId> = case["neighbours"]
            .as_array()
            .unwrap()
            .iter()
            .map(|n| name_of(n.as_str().unwrap()))
            .collect();
        expected.insert(files[index]);
        pump_until(|| expected.is_subset(&bound.image_cache.keys().into_iter().collect()));
        assert_eq!(settled_keys(&bound), expected, "{case}");
        viewer.invoke_close_requested();
        checked += 1;
    }
    assert_eq!(checked, 18);
    bound.preview.close();
}

/// "Maximum % of cache that will be prefetched per media viewer:" set in the
/// real Options row, with a real media viewer warming its neighbours: the
/// reference's two budget scenarios (`prefetch` in
/// `oracle/fixtures/viewer_prefetch.json`) with 30-byte images, then the same
/// pass with a larger cache, where the percentage is what changes the outcome.
// leaf: audit-options-speed-and-memory-image-cache-maximum-of-cache-that-will-be-prefetched-per-media-viewer
#[test]
fn the_percentage_row_limits_what_a_viewer_prefetches_as_in_the_recorded_scenarios() {
    let fixture = hydrus_testkit::fixture_json("viewer_prefetch.json");
    let scenario = |name: &str| {
        fixture["prefetch"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["name"] == name)
            .unwrap()
            .clone()
    };
    // (cache bytes, percentage, the files, neighbours asked for, how many of the files are cached)
    let one_miss = scenario("one miss and pending blocks");
    let total_equality = scenario("total equality permits");
    let policy = |s: &serde_json::Value| {
        (
            s["policy"][0].as_u64().unwrap(),
            i32::try_from(s["policy"][2].as_u64().unwrap()).unwrap(),
        )
    };
    let _windows = headless::init();
    let run = |files: &[&str], bytes: u64, percentage: i32, next: i32| {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path()).unwrap();
        let ids: Vec<HashId> = files.iter().map(|name| import(&store, name)).collect();
        store
            .write(move |ctx| {
                settings::set(
                    ctx.conn(),
                    &Policy {
                        bytes,
                        timeout: 300,
                        percentage: 50,
                    },
                )?;
                let mut layout = hydrus_store::page_layout::load(ctx.conn())?;
                layout.hide_preview = true;
                settings::set(ctx.conn(), &layout)
            })
            .unwrap();
        let ui = MainWindow::new().unwrap();
        ui.show().unwrap();
        let bound = bind(
            &ui,
            Pages::single(super::common::all_local_page(store.clone())),
        );
        ui.invoke_search_edited("system:everything".into());
        ui.invoke_search_accepted();
        let results = bound.current.borrow().borrow().results().to_vec();
        let window = options(&ui, &bound);
        window.invoke_number_edited(row(&window, PREVIOUS), 0);
        window.invoke_number_edited(row(&window, NEXT), next);
        window.invoke_number_edited(row(&window, PERCENT), percentage);
        window.invoke_apply();
        assert_eq!(
            i32::try_from(store.read(viewer_prefetch::load).unwrap().percentage).unwrap(),
            percentage
        );
        // (opened on the first file imported, the reference's "a")
        let first = ids[0];
        let at = results.iter().position(|id| *id == first).unwrap();
        ui.invoke_thumbnail_activated(i32::try_from(at).unwrap());
        let viewer = bound.viewer.borrow().as_ref().unwrap().clone_strong();
        pump_until(|| bound.image_cache.keys().contains(&first));
        let kept = settled_keys(&bound);
        let held = bound.image_cache.bytes();
        viewer.invoke_close_requested();
        bound.preview.close();
        // (which of the imported files they are, in the page's order)
        let in_order: Vec<usize> = results
            .iter()
            .enumerate()
            .filter(|(_, id)| kept.contains(id))
            .map(|(i, _)| i)
            .collect();
        assert!(in_order.contains(&at), "the viewer's own file is cached");
        (in_order, at, held)
    };

    // a, b, c at 30 bytes each, 120 bytes and 50%: the first is cached, then
    // one miss more, then the 60-byte budget is spent and the third is not
    let (bytes, percentage) = policy(&one_miss);
    assert_eq!(
        one_miss["events"].as_array().unwrap().last().unwrap()["keys"],
        serde_json::json!(["a", "b"])
    );
    let (kept, at, held) = run(&["a.png", "c.png", "d.png"], bytes, percentage, 2);
    let next = (at + 1) % 3;
    assert_eq!(
        kept,
        [at.min(next), at.max(next)],
        "the viewer's file and the next"
    );
    assert_eq!(held, 60, "two 30-byte images");

    // 100 bytes and 30%: a 30-byte file is cached and the budget of 30 is
    // already met, so the second (27 bytes) is not, though the cache has room
    let (bytes, percentage) = policy(&total_equality);
    assert_eq!(
        total_equality["events"].as_array().unwrap().last().unwrap()["keys"],
        serde_json::json!(["a"])
    );
    let (kept, at, held) = run(&["a.png", "small.png"], bytes, percentage, 1);
    assert_eq!(kept, [at], "only the viewer's own file");
    assert_eq!(held, 30, "one 30-byte image");

    // the same pass with a cache twice the size, the budget now 60: it goes in
    let (kept, _, held) = run(&["a.png", "small.png"], bytes * 2, percentage, 1);
    assert_eq!(kept, [0, 1], "both files");
    assert_eq!(held, 57, "30 and 27 bytes");
}
