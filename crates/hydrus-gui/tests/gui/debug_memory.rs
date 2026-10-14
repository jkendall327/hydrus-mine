//! Help > debug > memory actions > run fast / slow memory maintenance, clicked
//! in the real menu and compared with the reference's own pulses (the
//! recording `oracle/fixtures/debug_memory_maintenance.json`, made by
//! `oracle/record_debug_memory_maintenance.py` from the real client's menu).
use super::debug_menu_actions::Debug;
use hydrus_core::HashId;
use hydrus_gui::{MainWindow, Pages, bind, headless};
use hydrus_import::{FileImportOptions, FileImporter};
use hydrus_media::MediaTools;
use hydrus_store::image_cache::Policy;
use hydrus_store::settings;
use hydrus_store::settings::ThumbnailCacheSettings;
use serde_json::Value;
use slint::{ComponentHandle as _, Model as _};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

fn recording() -> Value {
    hydrus_testkit::fixture_json("debug_memory_maintenance.json")
}

fn keys_in_order(value: &Value) -> usize {
    value["keys"].as_array().unwrap().len()
}

// leaf: audit-options-help-debug-action-run-fast-memory-maintenance
#[test]
fn run_fast_memory_maintenance_trims_the_image_and_thumbnail_caches_and_slow_does_not() {
    let recording = recording();
    for which in ["fast", "slow"] {
        assert_eq!(
            recording[which]["path"].as_array().unwrap()[..3],
            ["help", "debug", "memory actions"]
        );
    }
    let steps = recording["caches"].as_array().unwrap();
    let step = |name: &str| steps.iter().find(|s| s["name"] == name).unwrap();
    // the reference's caches after a fast pulse: nothing goes while everything
    // is inside the timeout; later, what has idled past it (but not the newest)
    let aged_fast = step("past the timeout, after fast");
    let aged_slow = step("past the timeout, after slow");
    assert!(
        step("after fast")["pubs"]
            .as_array()
            .unwrap()
            .contains(&Value::from("memory_maintenance_pulse"))
    );
    assert!(step("after slow")["pubs"].as_array().unwrap().is_empty());
    assert_eq!(keys_in_order(&aged_slow["image"]), 3, "slow leaves them");
    assert_eq!(
        keys_in_order(&aged_fast["image"]),
        1,
        "only the newest left"
    );
    assert_eq!(aged_fast["image"], aged_fast["thumbnail"]);

    let (dirs, store) = super::subscriptions::store();
    let importer = FileImporter::new(store.clone(), MediaTools::new());
    let files: Vec<HashId> = ["a.png", "b.png"]
        .map(|name| {
            let result = importer
                .import_path(
                    &hydrus_testkit::fixture_path(format!("image_cache/{name}")),
                    &FileImportOptions::default(),
                )
                .unwrap();
            store
                .read(|conn| hydrus_store::master::hash_id(conn, &result.hash.unwrap()))
                .unwrap()
                .unwrap()
        })
        .into();
    store
        .write_and_refresh(|ctx| {
            settings::set(
                ctx.conn(),
                &Policy {
                    bytes: 100,
                    timeout: 300,
                    percentage: 50,
                },
            )?;
            let mut layout = hydrus_store::page_layout::load(ctx.conn())?;
            layout.hide_preview = false;
            layout.vpos = -240;
            settings::set(ctx.conn(), &layout)?;
            let mut stats: settings::FileViewingStatistics = settings::get(ctx.conn())?;
            stats.active = true;
            stats.preview_min_ms = None;
            stats.preview_max_ms = None;
            settings::set(ctx.conn(), &stats)
        })
        .unwrap();
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(
        &ui,
        Pages::single(super::common::all_local_page(store.clone())),
    );
    let clock = Arc::new(AtomicU64::new(0));
    bound.image_cache.set_clock(Arc::new({
        let clock = clock.clone();
        move || Duration::from_millis(clock.load(Ordering::Acquire))
    }));
    bound.rows.set_cache_policy(ThumbnailCacheSettings {
        bytes: 1 << 20,
        timeout: 300,
    });
    let adapter = windows.get(0).unwrap();
    for _ in 0..3 {
        headless::render(&adapter, 1400, 1000);
    }
    ui.invoke_search_edited("system:everything".into());
    ui.invoke_search_accepted();
    // the images are cached one second apart and two seconds, as in the
    // recording, and so are the thumbnails
    bound.rows.set_columns(1);
    bound.rows.wait();
    bound.rows.clear_thumbnail_cache();
    let ids = bound.current.borrow().borrow().results().to_vec();
    for (file, seconds) in [(0, 1), (1, 3)] {
        let i = ids.iter().position(|id| *id == files[file]).unwrap();
        clock.store(seconds * 1000, Ordering::Release);
        bound
            .rows
            .advance_cache_clock(Duration::from_secs(if file == 0 { 1 } else { 2 }));
        bound.rows.row_data(i).unwrap();
        bound.rows.wait();
        ui.invoke_thumbnail_clicked(i32::try_from(i).unwrap(), false, false);
        bound.preview.refresh();
        let started = Instant::now();
        while ui.get_preview_loading() && started.elapsed() < Duration::from_secs(5) {
            bound.preview.refresh();
            std::thread::yield_now();
        }
    }
    assert_eq!(bound.image_cache.keys().len(), 2);
    let has = |rows: &hydrus_gui::ThumbnailRows, file: HashId| rows.cached_keys().contains(&file);
    assert!(has(&bound.rows, files[0]) && has(&bound.rows, files[1]));
    let (older, newest) = (files[0], files[1]);
    let d = Debug::from_parts(dirs, store, ui, bound, windows);
    // inside the timeout: neither action drops anything
    for entry in ["run slow memory maintenance", "run fast memory maintenance"] {
        d.click(&["memory actions", entry]);
        assert_eq!(d.bound.image_cache.keys().len(), 2, "{entry}");
        assert!(
            has(&d.bound.rows, older) && has(&d.bound.rows, newest),
            "{entry}"
        );
    }
    // past the timeout (the oldest by 1.5 s, the newest not yet): slow still
    // leaves them, fast drops the idle one
    clock.store(301_500, Ordering::Release);
    d.bound
        .rows
        .advance_cache_clock(Duration::from_millis(298_500));
    d.click(&["memory actions", "run slow memory maintenance"]);
    assert_eq!(d.bound.image_cache.keys().len(), 2);
    assert!(has(&d.bound.rows, older) && has(&d.bound.rows, newest));
    d.click(&["memory actions", "run fast memory maintenance"]);
    assert_eq!(d.bound.image_cache.keys(), vec![newest]);
    assert!(!has(&d.bound.rows, older), "the idle thumbnail goes");
    assert!(has(&d.bound.rows, newest), "the newest stays");
    assert_eq!(
        d.bound.image_cache.keys().len(),
        keys_in_order(&aged_fast["image"])
    );
}

// leaf: audit-options-help-debug-action-run-slow-memory-maintenance
#[test]
fn run_slow_memory_maintenance_deletes_closed_pages_over_an_hour_old_if_pages_have_been_still() {
    let recording = recording();
    let minutes = |m: u64| Duration::from_secs(m * 60);
    let d = super::debug_menu_actions::start();
    for scenario in recording["closed_pages"].as_array().unwrap() {
        let name = scenario["name"].as_str().unwrap();
        let ages: Vec<u64> = scenario["closed_minutes_ago"]
            .as_array()
            .unwrap()
            .iter()
            .map(|m| m.as_u64().unwrap())
            .collect();
        // close one page for each age, oldest first, the clock moving on between
        let mut order: Vec<u64> = ages.clone();
        order.sort_unstable_by(|a, b| b.cmp(a));
        {
            let mut pages = d.bound.pages.borrow_mut();
            pages.forget_closed();
            let mut last = order[0];
            for age in &order {
                pages.advance_clock(minutes(last - age));
                last = *age;
                pages.new_search_page();
                pages.close_shown().unwrap();
            }
            pages.advance_clock(minutes(*order.last().unwrap()));
            pages.set_last_page_change_ago(minutes(
                scenario["last_change_minutes_ago"].as_u64().unwrap(),
            ));
        }
        assert_eq!(d.bound.pages.borrow().held_closed_ages().len(), ages.len());
        d.click(&["memory actions", "run slow memory maintenance"]);
        let kept: Vec<u64> = d
            .bound
            .pages
            .borrow()
            .held_closed_ages()
            .iter()
            .map(|age| age.as_secs() / 60)
            .collect();
        let recorded_kept: Vec<u64> = scenario["kept"]
            .as_array()
            .unwrap()
            .iter()
            .map(|n| {
                n.as_str()
                    .unwrap()
                    .trim_start_matches("closed ")
                    .parse()
                    .unwrap()
            })
            .collect();
        let mut expected = recorded_kept.clone();
        expected.sort_unstable_by(|a, b| b.cmp(a));
        assert_eq!(kept, expected, "{name}");
    }
    // the fast one leaves closed pages alone
    {
        let mut pages = d.bound.pages.borrow_mut();
        pages.forget_closed();
        pages.new_search_page();
        pages.close_shown().unwrap();
        pages.advance_clock(minutes(90));
        pages.set_last_page_change_ago(minutes(90));
    }
    d.click(&["memory actions", "run fast memory maintenance"]);
    assert_eq!(d.bound.pages.borrow().held_closed_ages().len(), 1);
    assert_eq!(
        recording["fast_leaves_closed_pages"]["destroyed"],
        serde_json::json!([])
    );
}
