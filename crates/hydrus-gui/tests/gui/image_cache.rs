//! Saved policy reaches real preview/viewer/archive decodes, independently of accepted pixels.
use hydrus_core::{CanvasType, HashId};
use hydrus_gui::{Bound, MainWindow, OptionsWindow, Pages, bind, headless};
use hydrus_import::{FileImportOptions, FileImporter};
use hydrus_media::MediaTools;
use hydrus_store::{
    Store,
    image_cache::{self, Policy},
    settings,
};
use slint::{ComponentHandle as _, Model as _};
use std::{
    cell::Cell,
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};
const BYTES: &str = "Memory reserved for image cache:";
const TIMEOUT: &str = "Image cache timeout:";
const PERCENT: &str = "Maximum image size (in % of cache) that can be cached:";
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
    let index = ui
        .get_menu_panes()
        .row_data(0)
        .unwrap()
        .lines
        .iter()
        .position(|row| row.label == "options…")
        .unwrap();
    ui.invoke_menu_line_clicked(0, i32::try_from(index).unwrap(), 0.0, 0.0, 0.0);
    let window = bound.options.borrow().as_ref().unwrap().clone_strong();
    let page = window
        .get_pages()
        .iter()
        .position(|page| page.text == "speed and memory")
        .unwrap();
    window.invoke_page_chosen(i32::try_from(page).unwrap());
    window
}
fn select(ui: &MainWindow, bound: &Bound, file: HashId) -> i32 {
    let at = bound
        .current
        .borrow()
        .borrow()
        .results()
        .iter()
        .position(|id| *id == file)
        .unwrap();
    let at = i32::try_from(at).unwrap();
    ui.invoke_thumbnail_clicked(at, false, false);
    bound.preview.refresh();
    at
}
fn settle(ui: &MainWindow, bound: &Bound) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while ui.get_preview_loading() {
        bound.preview.refresh();
        assert!(
            Instant::now() < deadline,
            "actual preview decode did not publish"
        );
        std::thread::yield_now();
    }
    assert!(ui.get_preview_has_media());
}
// leaf: audit-options-speed-and-memory-image-cache-maximum-image-size-in-of-cache-that-can-be-cached
// leaf: audit-options-speed-and-memory-image-cache-memory-reserved-for-image-cache
#[test]
fn real_saved_policy_preserves_current_media_and_intervals_then_retires_final_bound_owner() {
    let (_dirs, store) = crate::subscriptions::store();
    let importer = FileImporter::new(store.clone(), MediaTools::new());
    let files: Vec<_> = ["a.png", "b.png"]
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
    let cache = bound.image_cache.clone();
    let clone = bound.clone();
    let clock = Arc::new(AtomicU64::new(0));
    cache.set_clock(Arc::new({
        let clock = clock.clone();
        move || Duration::from_millis(clock.load(Ordering::Acquire))
    }));
    let now = Rc::new(Cell::new(1000));
    bound.preview.set_clock(Rc::new({
        let now = now.clone();
        move || now.get()
    }));
    let adapter = windows.get(0).unwrap();
    for _ in 0..3 {
        headless::render(&adapter, 1400, 1000);
    }
    ui.invoke_search_edited("system:everything".into());
    ui.invoke_search_accepted();
    let index = select(&ui, &bound, files[0]);
    settle(&ui, &bound);
    assert_eq!(cache.keys(), vec![files[0]]);
    assert_eq!(cache.bytes(), 30);
    ui.invoke_thumbnail_activated(index);
    let viewer = bound.viewer.borrow().as_ref().unwrap().clone_strong();
    assert_eq!(
        (
            viewer.get_media().size().width,
            viewer.get_media().size().height
        ),
        (10, 1)
    );
    assert_eq!(cache.keys(), vec![files[0]]);
    let staged = options(&ui, &bound);
    staged.invoke_number_edited(row(&staged, BYTES), 1);
    staged.invoke_number_edited(row(&staged, PERCENT), 10);
    staged.invoke_cancel();
    staged.invoke_apply();
    assert_eq!(store.read(image_cache::load).unwrap().bytes, 100);
    let hidden = options(&ui, &bound);
    hidden.hide().unwrap();
    hidden.invoke_number_edited(row(&hidden, BYTES), 0);
    hidden.invoke_number_edited(row(&hidden, PERCENT), 10);
    hidden.invoke_field_edited(row(&hidden, TIMEOUT), 2, 10);
    hidden.show().unwrap();
    hidden.invoke_apply();
    assert_eq!(
        store.read(image_cache::load).unwrap(),
        Policy {
            bytes: 100,
            timeout: 300,
            percentage: 50
        }
    );
    // An existing suggested-tags descendant still owns input even after a
    // retained parent page callback exposes the image-cache rows.
    let child_owned = options(&ui, &bound);
    let suggestions = child_owned
        .get_pages()
        .iter()
        .position(|page| page.text == "tag suggestions")
        .unwrap();
    child_owned.invoke_page_chosen(i32::try_from(suggestions).unwrap());
    child_owned.invoke_related_weights_clicked();
    let weights = bound
        .options_suggested_tags_slot
        .weights
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    let speed = child_owned
        .get_pages()
        .iter()
        .position(|page| page.text == "speed and memory")
        .unwrap();
    child_owned.invoke_page_chosen(i32::try_from(speed).unwrap());
    child_owned.invoke_number_edited(row(&child_owned, BYTES), 0);
    child_owned.invoke_choice_chosen(row(&child_owned, BYTES), 0);
    child_owned.invoke_number_edited(row(&child_owned, PERCENT), 10);
    child_owned.invoke_field_edited(row(&child_owned, TIMEOUT), 2, 10);
    weights.invoke_action("cancel".into());
    assert!(bound.options_suggested_tags_slot.weights.borrow().is_none());
    child_owned.invoke_apply();
    assert_eq!(
        store.read(image_cache::load).unwrap(),
        Policy {
            bytes: 100,
            timeout: 300,
            percentage: 50
        }
    );
    let saved = options(&ui, &bound);
    let percent = row(&saved, PERCENT);
    saved.invoke_number_edited(percent, 10);
    assert!(
        saved
            .get_rows()
            .row_data(percent as usize)
            .unwrap()
            .unit
            .contains("pixels")
    );
    saved.invoke_apply();
    assert_eq!(
        cache.keys(),
        vec![files[0]],
        "changing percentage retains already-admitted renderer"
    );
    ui.invoke_archive_delete_filter();
    let archive = bound
        .archive_delete
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    assert_eq!(archive.get_media().size().width, 10);
    assert_eq!(cache.keys(), vec![files[0]]);
    archive.invoke_forget(); // No decisions; reference Forget is a no-op until finish, keep it owned.
    select(&ui, &bound, files[1]);
    settle(&ui, &bound);
    assert!(
        !cache.keys().contains(&files[1]),
        "excluded RGBA image still reaches actual preview"
    );
    assert_eq!(ui.get_preview_media().size().width, 10);
    let totals = |store: &Store| {
        store
            .read(|conn| {
                Ok(hydrus_store::media::viewing_stats(conn, &files)?
                    .into_iter()
                    .map(|value| {
                        (
                            value.canvas,
                            value.views,
                            value.viewtime_ms,
                            value.last_viewed,
                        )
                    })
                    .collect::<Vec<_>>())
            })
            .unwrap()
    };
    let totals_before = totals(&store);
    let current = bound.preview.displayed_file();
    let pixels = ui
        .get_preview_media()
        .to_rgba8()
        .unwrap()
        .as_bytes()
        .to_vec();
    let shrink = options(&ui, &bound);
    shrink.invoke_number_edited(row(&shrink, BYTES), 0);
    shrink.invoke_apply();
    assert!(cache.keys().is_empty());
    assert_eq!(cache.bytes(), 0);
    assert_eq!(bound.preview.displayed_file(), current);
    assert_eq!(
        ui.get_preview_media().to_rgba8().unwrap().as_bytes(),
        pixels
    );
    assert_eq!(
        viewer.get_media().size().width,
        10,
        "current source remains independently held after cache shrink"
    );
    assert_eq!(archive.get_media().size().width, 10);
    assert_eq!(totals(&store), totals_before);
    now.set(2000);
    drop(bound); // Existing Bound clone still owns this incarnation.
    let refill = options(&ui, &clone);
    refill.invoke_number_edited(row(&refill, BYTES), 100);
    refill.invoke_choice_chosen(row(&refill, BYTES), 0);
    refill.invoke_number_edited(row(&refill, PERCENT), 50);
    refill.invoke_apply();
    select(&ui, &clone, files[0]);
    settle(&ui, &clone);
    assert!(cache.keys().contains(&files[0]));
    let reopened = Store::open(store.dir()).unwrap();
    assert_eq!(reopened.read(image_cache::load).unwrap().bytes, 100);
    let saved_totals = store
        .read(|conn| hydrus_store::media::viewing_stats(conn, &[files[1]]))
        .unwrap();
    let second = saved_totals
        .iter()
        .find(|stats| stats.canvas == CanvasType::Preview)
        .unwrap();
    assert_eq!(
        (second.views, second.viewtime_ms),
        (1, 1000),
        "excluded media retains its real successful-SetMedia interval through policy refresh"
    );
    let window = options(&ui, &clone);
    let old_row = row(&window, BYTES);
    window.invoke_cancel();
    drop(clone);
    cache.refresh();
    assert!(cache.keys().is_empty());
    window.invoke_number_edited(old_row, 0);
    window.invoke_apply();
    // The cache lease is terminal even though retained GUI callbacks keep other
    // presentation owners alive. A rebind supplies a separate cache incarnation.
    let successor = bind(
        &ui,
        Pages::single(super::common::all_local_page(store.clone())),
    );
    cache.refresh();
    assert_eq!(cache.bytes(), 0);
    assert!(cache.keys().is_empty());
    window.invoke_number_edited(old_row, 0);
    window.invoke_apply();
    assert_eq!(store.read(image_cache::load).unwrap().bytes, 100);
    let output = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("image-cache-options.png");
    let visible = options(&ui, &successor);
    let native = windows.get(windows.count() - 1).unwrap();
    let pixels = headless::render(&native, 1100, 650);
    assert!(
        visible
            .get_rows()
            .iter()
            .any(|row| row.label == PERCENT && row.kind == 2)
    );
    headless::save_png(&output, &pixels, 1100, 650).unwrap();
    visible.invoke_cancel();
    successor.preview.close();
    viewer.invoke_close_requested();
}

// Value/persistence regressions above remain intact. This one measures the
// native layout failure they cannot see: wrapped helpers must own space before
// the next actual control/heading, even at the preferred Options opening size.
#[test]
fn speed_memory_helpers_own_wrapped_space_at_preferred_and_capture_sizes() {
    use hydrus_store::viewer_prefetch;
    use std::{cell::RefCell, collections::HashMap};

    const PREFETCH_PERCENT: &str = "Maximum % of cache that will be prefetched per media viewer:";
    const PREVIOUS: &str = "Num previous to prefetch in Media Viewer:";
    const NEXT: &str = "Num next to prefetch in Media Viewer:";
    const PAIRS: &str = "Num pairs to prefetch in Duplicate Filter:";
    let (_dirs, store) = crate::subscriptions::store();
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(
        &ui,
        Pages::single(super::common::all_local_page(store.clone())),
    );
    let counts = viewer_prefetch::Preferences::default();
    let mut warning_heights = HashMap::new();
    // Both a long over-capacity warning and the short success caption; 1GB
    // also produces the long percentage-estimate numbers in the shared captures.
    for (case, bytes) in [("long", 100), ("short", 1024 * 1024 * 1024)] {
        let cache = Policy {
            bytes,
            timeout: 300,
            percentage: 50,
        };
        store
            .write(move |ctx| {
                settings::set(ctx.conn(), &cache)?;
                settings::set(ctx.conn(), &counts)
            })
            .unwrap();
        for (width, height) in [(900, 640), (1100, 650), (1100, 850), (1100, 900)] {
            let window = options(&ui, &bound);
            let rows_before: Vec<_> = window
                .get_rows()
                .iter()
                .map(|r| (r.kind, r.label, r.number, r.unit, r.index))
                .collect();
            assert_eq!(
                window.get_prefetch_warning().as_str(),
                hydrus_gui_model::viewer_prefetch::warning(cache.bytes, counts)
            );
            let stored_percent = row(&window, PERCENT);
            let prefetch_percent = row(&window, PREFETCH_PERCENT);
            let previous = row(&window, PREVIOUS);
            let next = row(&window, NEXT);
            let pairs = row(&window, PAIRS);
            let prefetch_heading = row(&window, "image prefetch");
            let tile_heading = row(&window, "image tile cache");
            let required = [
                (stored_percent, "number"),
                (stored_percent, "estimate"),
                (prefetch_percent, "number"),
                (prefetch_percent, "estimate"),
                (previous, "number"),
                (next, "number"),
                (pairs, "number"),
                (pairs, "warning"),
                (prefetch_heading, "heading"),
                (tile_heading, "heading"),
            ];
            let frames = Rc::new(RefCell::new(HashMap::<
                (i32, String),
                hydrus_gui::MenuChoiceFrame,
            >::new()));
            let native = windows.get(windows.count() - 1).unwrap();
            // Settle the requested viewport before enabling the initial-frame
            // observer; callbacks must not publish the window's opening size.
            headless::render(&native, width, height);
            window.on_speed_memory_geometry({
                let frames = frames.clone();
                move |index, role, frame| {
                    frames.borrow_mut().insert((index, role.to_string()), frame);
                }
            });
            window.set_measure_speed_memory_layout(true);
            let started = Instant::now();
            let pixels = loop {
                let pixels = headless::render(&native, width, height);
                if required
                    .iter()
                    .all(|(index, role)| frames.borrow().contains_key(&(*index, (*role).into())))
                {
                    break pixels;
                }
                assert!(
                    started.elapsed() < Duration::from_secs(2),
                    "native helper/control/heading frames must publish at {width}x{height}"
                );
                std::thread::yield_now();
            };
            let measured = frames.borrow();
            let frame = |index, role: &str| measured.get(&(index, role.into())).unwrap();
            for &(index, role) in &required {
                let f = frame(index, role);
                assert!([f.x, f.y, f.w, f.h].into_iter().all(f32::is_finite));
                assert!(f.w > 0.0 && f.h > 0.0, "{role} needs actual allocated area");
                assert!(
                    f.x >= 0.0 && f.x + f.w <= width as f32,
                    "{role} stays within actual viewport width"
                );
            }
            let separated = |upper: &hydrus_gui::MenuChoiceFrame,
                             lower: &hydrus_gui::MenuChoiceFrame| {
                assert!(
                    upper.y + upper.h < lower.y,
                    "caption/control rectangles must leave positive vertical space: {upper:?} then {lower:?} at {width}x{height}"
                );
            };
            separated(
                frame(stored_percent, "number"),
                frame(stored_percent, "estimate"),
            );
            separated(
                frame(stored_percent, "estimate"),
                frame(prefetch_percent, "number"),
            );
            separated(
                frame(prefetch_percent, "number"),
                frame(prefetch_percent, "estimate"),
            );
            separated(
                frame(prefetch_percent, "estimate"),
                frame(prefetch_heading, "heading"),
            );
            separated(
                frame(prefetch_heading, "heading"),
                frame(previous, "number"),
            );
            separated(frame(previous, "number"), frame(next, "number"));
            separated(frame(next, "number"), frame(pairs, "number"));
            separated(frame(pairs, "number"), frame(pairs, "warning"));
            separated(frame(pairs, "warning"), frame(tile_heading, "heading"));
            warning_heights.insert((case, width), frame(pairs, "warning").h);
            drop(measured);
            assert_eq!(
                window
                    .get_rows()
                    .iter()
                    .map(|r| (r.kind, r.label, r.number, r.unit, r.index))
                    .collect::<Vec<_>>(),
                rows_before,
                "measurement/resize cannot alter row identities, callbacks' indices or displayed values/text"
            );
            headless::save_png(
                &std::path::Path::new(env!("CARGO_TARGET_TMPDIR"))
                    .join(format!("speed-memory-helpers-{case}-{width}x{height}.png")),
                &pixels,
                width,
                height,
            )
            .unwrap();
            window.invoke_cancel();
            assert_eq!(store.read(image_cache::load).unwrap(), cache);
            assert_eq!(store.read(viewer_prefetch::load).unwrap(), counts);
        }
    }
    assert!(
        warning_heights[&("long", 900)] > warning_heights[&("short", 900)],
        "preferred-width capture must actually exercise a wrapped warning"
    );
}
