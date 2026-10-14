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
    // The reference's caches, scripted with four entries a to d one second
    // apart, sizes and limits chosen so that these real files give the same
    // sizes (`oracle/record_debug_memory_maintenance.py`): the fourth pushes
    // the oldest out, what is over the limit goes at once, a slow pulse
    // changes nothing, and a fast one drops what has idled past the timeout,
    // but not the newest.
    let steps = recording["caches"].as_array().unwrap();
    let step = |name: &str| steps.iter().find(|s| s["name"] == name).unwrap();
    let letters = ["a", "b", "c", "d"];
    let limit = |cache: &str| recording["limits"][cache].as_u64().unwrap();
    // (the files of the recording's a, b, c and d: 20, 14, 10 and 3 pixels wide)
    let names = ["w20.png", "w14.png", "c.png", "w3.png"];
    let (dirs, store) = super::subscriptions::store();
    let importer = FileImporter::new(store.clone(), MediaTools::new());
    let files: Vec<HashId> = names
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
    let image_limit = limit("image");
    store
        .write_and_refresh(move |ctx| {
            settings::set(
                ctx.conn(),
                &Policy {
                    bytes: image_limit,
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
        bytes: limit("thumbnail"),
        timeout: 300,
    });
    let adapter = windows.get(0).unwrap();
    for _ in 0..3 {
        headless::render(&adapter, 1400, 1000);
    }
    ui.invoke_search_edited("system:everything".into());
    ui.invoke_search_accepted();
    bound.rows.set_columns(1);
    bound.rows.wait();
    bound.rows.clear_thumbnail_cache();
    let ids = bound.current.borrow().borrow().results().to_vec();
    // the images and thumbnails are cached one second apart, in the order
    // of the recording
    for (n, file) in files.iter().enumerate() {
        let i = ids.iter().position(|id| id == file).unwrap();
        clock.store(n as u64 * 1000, Ordering::Release);
        if n > 0 {
            bound.rows.advance_cache_clock(Duration::from_secs(1));
        }
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
    let letter_of =
        |file: HashId| letters[files.iter().position(|f| *f == file).unwrap()].to_owned();
    let image_keys = |bound: &hydrus_gui::Bound| -> Vec<String> {
        let mut keys: Vec<String> = bound
            .image_cache
            .keys()
            .into_iter()
            .map(letter_of)
            .collect();
        keys.sort();
        keys
    };
    let thumbnail_keys = |bound: &hydrus_gui::Bound| -> Vec<String> {
        let mut keys: Vec<String> = bound
            .rows
            .cached_keys()
            .into_iter()
            .map(letter_of)
            .collect();
        keys.sort();
        keys
    };
    let recorded_keys = |name: &str, cache: &str| -> Vec<String> {
        let mut keys: Vec<String> = step(name)[cache]["keys"]
            .as_array()
            .unwrap()
            .iter()
            .map(|k| k.as_str().unwrap().to_owned())
            .collect();
        keys.sort();
        keys
    };
    // the sizes the files have here are the recording's
    assert_eq!(
        bound.rows.cached_bytes(),
        step("primed")["thumbnail"]["bytes"].as_u64().unwrap()
    );
    assert_eq!(
        bound.image_cache.bytes(),
        step("primed")["image"]["bytes"].as_u64().unwrap()
    );
    // the fourth pushed the first out as it came in
    assert_eq!(image_keys(&bound), recorded_keys("primed", "image"));
    assert_eq!(thumbnail_keys(&bound), recorded_keys("primed", "thumbnail"));
    let d = Debug::from_parts(dirs, store, ui, bound, windows);
    // inside the timeout, neither action drops anything
    for (entry, after) in [
        ("run slow memory maintenance", "after slow"),
        ("run fast memory maintenance", "after fast"),
    ] {
        d.click(&["memory actions", entry]);
        assert_eq!(
            image_keys(&d.bound),
            recorded_keys(after, "image"),
            "{entry}"
        );
        assert_eq!(
            thumbnail_keys(&d.bound),
            recorded_keys(after, "thumbnail"),
            "{entry}"
        );
    }
    // past the timeout (the reference's 302.5 s): slow still leaves them, fast
    // drops what has idled, keeping the newest
    let late = (step("past the timeout, after slow")["now"]
        .as_f64()
        .unwrap()
        * 1000.0) as u64;
    clock.store(late, Ordering::Release);
    d.bound
        .rows
        .advance_cache_clock(Duration::from_millis(late - 3000));
    d.click(&["memory actions", "run slow memory maintenance"]);
    assert_eq!(
        image_keys(&d.bound),
        recorded_keys("past the timeout, after slow", "image")
    );
    assert_eq!(
        thumbnail_keys(&d.bound),
        recorded_keys("past the timeout, after slow", "thumbnail")
    );
    d.click(&["memory actions", "run fast memory maintenance"]);
    assert_eq!(
        image_keys(&d.bound),
        recorded_keys("past the timeout, after fast", "image")
    );
    assert_eq!(
        thumbnail_keys(&d.bound),
        recorded_keys("past the timeout, after fast", "thumbnail")
    );
}

// leaf: audit-options-help-debug-action-run-slow-memory-maintenance
#[test]
fn run_slow_memory_maintenance_deletes_closed_pages_over_an_hour_old_if_pages_have_been_still() {
    let recording = recording();
    let minutes = |m: u64| Duration::from_secs(m * 60);
    let d = super::debug_menu_actions::start();
    d.bound.pages.borrow_mut().freeze_clock();
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

#[test]
fn closed_pages_over_an_hour_old_are_still_listed_and_reopen_by_their_place() {
    let d = super::debug_menu_actions::start();
    let mut pages = d.bound.pages.borrow_mut();
    pages.freeze_clock();
    pages.forget_closed();
    for _ in 0..2 {
        pages.new_search_page();
        pages.close_shown().unwrap();
    }
    pages.advance_clock(Duration::from_secs(90 * 60));
    // until the slow maintenance deletes them, the menu lists them
    assert_eq!(pages.closed_names().len(), 2);
    assert_eq!(pages.closed_count(), 2);
    assert!(pages.unclose_at(0));
    assert_eq!(pages.closed_names().len(), 1, "only the one reopened went");
    assert!(pages.unclose());
    assert!(pages.closed_names().is_empty());
}
