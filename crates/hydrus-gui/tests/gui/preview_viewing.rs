//! Real focused-file preview, owned timing, Options persistence and retired decodes.
use hydrus_core::{CanvasType, HashId, Sha256};
use hydrus_gui::{MainWindow, OptionsWindow, Pages, bind, headless};
use hydrus_store::{
    Store,
    settings::{self, FileViewingStatistics},
};
use slint::{ComponentHandle as _, Model as _};
use std::{cell::Cell, rc::Rc, sync::Arc};

fn store() -> ([tempfile::TempDir; 2], Arc<Store>) {
    let legacy = hydrus_testkit::legacy_fixture("basic");
    let directory = tempfile::tempdir().unwrap();
    hydrus_store::import::import_legacy(
        legacy.path(),
        &directory.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store = Store::open(directory.path()).unwrap();
    store
        .write(|ctx| {
            ctx.conn()
                .execute("DELETE FROM file_viewing_stats WHERE canvas_type=1", [])?;
            Ok(())
        })
        .unwrap();
    ([legacy, directory], store)
}
fn file(store: &Store, hash: &str) -> HashId {
    store
        .read(|conn| hydrus_store::master::hash_id(conn, &hash.parse::<Sha256>().unwrap()))
        .unwrap()
        .unwrap()
}
fn request(ui: &MainWindow, bound: &hydrus_gui::Bound, file: HashId) {
    let index = bound
        .current
        .borrow()
        .borrow()
        .results()
        .iter()
        .position(|id| *id == file)
        .unwrap();
    ui.invoke_thumbnail_clicked(i32::try_from(index).unwrap(), false, false);
    bound.preview.refresh();
}
fn select(ui: &MainWindow, bound: &hydrus_gui::Bound, file: HashId) {
    request(ui, bound, file);
    if ui.get_preview_loading() {
        wait_image(ui, &bound.preview);
    }
}
fn save_settings(store: &Store, active: bool, minimum: Option<u64>, maximum: Option<u64>) {
    store
        .write_and_refresh(move |ctx| {
            let mut value: FileViewingStatistics = settings::get(ctx.conn())?;
            value.active = active;
            value.preview_min_ms = minimum;
            value.preview_max_ms = maximum;
            settings::set(ctx.conn(), &value)
        })
        .unwrap();
}
fn assert_event(store: &Store, event: &serde_json::Value) {
    let rows = event["rows"].as_array().unwrap();
    let actual = store
        .read(|conn| {
            conn.query_row(
                "SELECT count(*) FROM file_viewing_stats WHERE canvas_type=1",
                [],
                |row| row.get::<_, i64>(0),
            )
            .map_err(Into::into)
        })
        .unwrap();
    assert_eq!(actual, i64::try_from(rows.len()).unwrap(), "{event}");
    for row in rows {
        let id = file(store, row["hash"].as_str().unwrap());
        let stats = store
            .read(|conn| hydrus_store::media::viewing_stats(conn, &[id]))
            .unwrap()
            .into_iter()
            .find(|stats| stats.canvas == CanvasType::Preview)
            .unwrap();
        assert_eq!(
            serde_json::json!([stats.last_viewed.unwrap().0, stats.views, stats.viewtime_ms]),
            row["row"],
            "{event}"
        );
    }
}
// Show alone does not measure a MinimalSoftwareWindow. Settle the real pane
// before selection, so zero-width startup cannot masquerade as a collapsed sash.
fn settle_viewport(
    ui: &MainWindow,
    preview: &hydrus_gui::preview_window::Monitor,
    native: &slint::platform::software_renderer::MinimalSoftwareWindow,
) {
    assert!(ui.window().is_visible());
    for _ in 0..3 {
        headless::render(native, 1000, 900);
    }
    preview.refresh();
    assert!(ui.get_layout_available_width() > 0.0);
    assert!(ui.get_sidebar_actual_width() > 0.0);
    assert!(ui.get_preview_actual_height() > 0.0);
    assert!(!ui.get_preview_splitter_hidden());
}
fn wait_image(ui: &MainWindow, preview: &hydrus_gui::preview_window::Monitor) {
    let started = std::time::Instant::now();
    while ui.get_preview_media().size().width == 0 {
        preview.refresh();
        assert!(started.elapsed() < std::time::Duration::from_secs(5));
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
}

// leaf: audit-options-file-viewing-statistics-min-time-to-view-on-preview-viewer-to-count-as-a-view
#[test]
fn preview_display_replays_qt_page_splitter_active_clear_close_and_successor_ownership() {
    let fixture = hydrus_testkit::fixture_json("preview_viewing_intervals.json");
    let (_directories, store) = store();
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(
        &ui,
        Pages::single(super::common::all_local_page(store.clone())),
    );
    let clock = Rc::new(Cell::new(123_000));
    bound.preview.set_clock(Rc::new({
        let clock = clock.clone();
        move || clock.get()
    }));
    ui.invoke_search_edited("system:everything".into());
    ui.invoke_search_accepted();
    let first = file(&store, fixture["file"].as_str().unwrap());
    let other_hash = fixture["events"]
        .as_array()
        .unwrap()
        .iter()
        .find(|event| event["event"] == "live-settings-change")
        .unwrap()["shown"]
        .as_str()
        .unwrap();
    let other = file(&store, other_hash);
    save_settings(&store, true, None, Some(1000));
    ui.show().unwrap();
    settle_viewport(&ui, &bound.preview, &windows.get(0).unwrap());
    for event in fixture["events"].as_array().unwrap() {
        clock.set(event["at_ms"].as_i64().unwrap());
        match event["event"].as_str().unwrap() {
            "show" | "same-file" => select(&ui, &bound, first),
            "clear" | "clear-again" => {
                ui.invoke_select_none();
                bound.preview.refresh();
            }
            "page-hidden" => {
                clock.set(126_000);
                select(&ui, &bound, first);
                clock.set(128_000);
                ui.hide().unwrap();
                bound.preview.refresh();
            }
            "page-shown" => {
                ui.show().unwrap();
                bound.preview.refresh();
                wait_image(&ui, &bound.preview);
            }
            "splitter-hidden" => {
                ui.set_preview_splitter_hidden(true);
                bound.preview.refresh();
            }
            "blocked-splitter" => select(&ui, &bound, other),
            "splitter-shown-empty" => {
                ui.set_preview_splitter_hidden(false);
                bound.preview.refresh();
            }
            "blocked-invisible" => {
                ui.hide().unwrap();
                select(&ui, &bound, other);
            }
            "inactive-at-finish" => {
                ui.show().unwrap();
                clock.set(138_000);
                select(&ui, &bound, first);
                save_settings(&store, false, None, Some(1000));
                clock.set(140_000);
                ui.invoke_select_none();
                bound.preview.refresh();
            }
            "live-settings-change" => {
                save_settings(&store, true, Some(2000), None);
                clock.set(142_000);
                select(&ui, &bound, first);
                wait_image(&ui, &bound.preview);
                let native = windows.get(0).unwrap();
                let pixels = headless::render(&native, 1000, 900);
                headless::save_png(
                    &std::path::Path::new(env!("CARGO_TARGET_TMPDIR"))
                        .join("page-owned-preview.png"),
                    &pixels,
                    1000,
                    900,
                )
                .unwrap();
                clock.set(145_000);
                select(&ui, &bound, other);
            }
            "clean-before-destroy" | "clean-again" => bound.preview.close(),
            name => panic!("unknown recorded transition {name}"),
        }
        assert_eq!(
            ui.get_preview_has_media(),
            !event["shown"].is_null(),
            "{event}"
        );
        assert_event(&store, event);
    }
    let retired = bound.preview.clone();
    let successor = bind(
        &ui,
        Pages::single(super::common::all_local_page(store.clone())),
    );
    successor.preview.set_clock(Rc::new({
        let clock = clock.clone();
        move || clock.get()
    }));
    ui.invoke_search_edited("system:everything".into());
    ui.invoke_search_accepted();
    clock.set(150_000);
    select(&ui, &successor, first);
    wait_image(&ui, &successor.preview);
    let image = ui.get_preview_media().size();
    retired.refresh();
    retired.close();
    assert_eq!(ui.get_preview_media().size(), image);
    assert!(ui.get_preview_has_media());
    clock.set(153_000);
    successor.preview.close();
    let saved = store
        .read(|conn| hydrus_store::media::viewing_stats(conn, &[first]))
        .unwrap();
    let reopened = Store::open(store.dir()).unwrap();
    assert_eq!(
        reopened
            .read(|conn| hydrus_store::media::viewing_stats(conn, &[first]))
            .unwrap(),
        saved
    );
}

fn options(ui: &MainWindow, bound: &hydrus_gui::Bound) -> OptionsWindow {
    ui.invoke_menu_title_pressed(0, 20.0, 22.0);
    let lines = ui.get_menu_panes().row_data(0).unwrap().lines;
    let index = (0..lines.row_count())
        .position(|i| lines.row_data(i).unwrap().label == "options\u{2026}")
        .unwrap();
    ui.invoke_menu_line_clicked(0, i32::try_from(index).unwrap(), 0.0, 0.0, 0.0);
    let options = bound.options.borrow().as_ref().unwrap().clone_strong();
    let pages = options.get_pages();
    let page = (0..pages.row_count())
        .position(|i| pages.row_data(i).unwrap().text == "file viewing statistics")
        .unwrap();
    options.set_page(i32::try_from(page).unwrap());
    options.invoke_page_chosen(i32::try_from(page).unwrap());
    options
}
fn row(options: &OptionsWindow, label: &str) -> i32 {
    let rows = options.get_rows();
    i32::try_from(
        (0..rows.row_count())
            .position(|i| rows.row_data(i).unwrap().label == label)
            .unwrap(),
    )
    .unwrap()
}
// Capture a freshly opened owner: callback-only edits can change the editor
// without replacing the standard NumberField/CheckBox bindings already drawn.
fn capture_preview_options(
    windows: &headless::Windows,
    options: &OptionsWindow,
    name: &str,
    minimum_none: bool,
    maximum_values: [i32; 4],
) {
    assert!(options.window().is_visible());
    let native = windows.get(windows.count() - 1).unwrap();
    // Let the standard fields settle from their new, saved row bindings.
    for _ in 0..3 {
        headless::render(&native, 1100, 850);
    }
    for (label, none, minimum, phrase, expected) in [
        (
            "Min time to view on preview viewer to count as a view:",
            minimum_none,
            50,
            "count every view",
            vec![("minutes", 0), ("seconds", 5), ("ms", 0)],
        ),
        (
            "Cap any view on the preview viewer to this maximum time:",
            false,
            1000,
            "no limit",
            ["hours", "minutes", "seconds", "ms"]
                .into_iter()
                .zip(maximum_values)
                .collect::<Vec<_>>(),
        ),
    ] {
        let actual = options
            .get_rows()
            .row_data(usize::try_from(row(options, label)).unwrap())
            .unwrap();
        assert_eq!(actual.kind, 24, "{label}");
        assert_eq!(actual.is_none, none, "{label}");
        assert_eq!(actual.minimum, minimum, "{label}");
        assert_eq!(actual.none_phrase, phrase, "{label}");
        assert_eq!(
            actual
                .fields
                .iter()
                .map(|field| (field.label.to_string(), field.value))
                .collect::<Vec<_>>(),
            expected
                .into_iter()
                .map(|(unit, value)| (unit.to_owned(), value))
                .collect::<Vec<_>>(),
            "{label}"
        );
    }
    let pixels = headless::render_snapshot(&native, 1100, 850);
    headless::save_png(
        &std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join(name),
        &pixels,
        1100,
        850,
    )
    .unwrap();
}
// leaf: audit-options-file-viewing-statistics-cap-any-view-on-the-preview-viewer-to-this-maximum-time
// leaf: audit-options-file-viewing-statistics-min-time-to-view-on-preview-viewer-to-count-as-a-view
#[test]
fn saved_preview_options_reach_open_display_duration_cap_cancel_and_confirmed_client_exit() {
    let fixture = hydrus_testkit::fixture_json("preview_viewing_intervals.json");
    let (_directories, store) = store();
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    // Qt's recorder supplies these media directly to its viewing-statistics
    // policy, including a trashed video outside the current local-file search.
    let first = file(&store, fixture["file"].as_str().unwrap());
    let video = file(&store, fixture["video"].as_str().unwrap());
    let bound = bind(
        &ui,
        Pages::single(hydrus_gui::SearchPage::fixed(
            store.clone(),
            "preview interval recording",
            None,
            vec![first, video],
        )),
    );
    let clock = Rc::new(Cell::new(200_000));
    bound.preview.set_clock(Rc::new({
        let clock = clock.clone();
        move || clock.get()
    }));
    ui.show().unwrap();
    settle_viewport(&ui, &bound.preview, &windows.get(0).unwrap());
    select(&ui, &bound, first);
    let edit = options(&ui, &bound);
    capture_preview_options(
        &windows,
        &edit,
        "preview-viewing-options-defaults.png",
        false,
        [0, 1, 0, 0],
    );
    assert_eq!(
        clock.get(),
        200_000,
        "capture does not advance viewing time"
    );
    let minimum = row(
        &edit,
        "Min time to view on preview viewer to count as a view:",
    );
    let maximum = row(
        &edit,
        "Cap any view on the preview viewer to this maximum time:",
    );
    edit.invoke_none_toggled(minimum, true);
    for field in 0..4 {
        edit.invoke_field_edited(maximum, field, 0);
    }
    edit.invoke_apply();
    let saved: FileViewingStatistics = store.read(settings::get).unwrap();
    assert_eq!(
        (saved.preview_min_ms, saved.preview_max_ms),
        (None, Some(1000))
    );
    clock.set(202_000);
    ui.invoke_select_none();
    bound.preview.refresh();
    let read = |id| {
        store
            .read(|conn| hydrus_store::media::viewing_stats(conn, &[id]))
            .unwrap()
            .into_iter()
            .find(|stats| stats.canvas == CanvasType::Preview)
            .unwrap()
    };
    assert_eq!((read(first).views, read(first).viewtime_ms), (1, 1000));
    let edit = options(&ui, &bound);
    capture_preview_options(
        &windows,
        &edit,
        "preview-viewing-options-saved.png",
        true,
        [0, 0, 1, 0],
    );
    assert_eq!(
        clock.get(),
        202_000,
        "capture does not advance viewing time"
    );
    assert_eq!(
        store.read(settings::get::<FileViewingStatistics>).unwrap(),
        saved,
        "opening and painting the saved owner do not write preferences"
    );
    assert_eq!((read(first).views, read(first).viewtime_ms), (1, 1000));
    edit.invoke_none_toggled(
        row(
            &edit,
            "Cap any view on the preview viewer to this maximum time:",
        ),
        true,
    );
    edit.invoke_cancel();
    edit.invoke_apply();
    assert_eq!(
        store.read(settings::get::<FileViewingStatistics>).unwrap(),
        saved,
        "cancelled editor callbacks are retired"
    );
    let duration = store
        .read(|conn| hydrus_store::media::load_basic(conn, &[video]))
        .unwrap()[0]
        .info
        .as_ref()
        .unwrap()
        .duration_ms
        .unwrap();
    clock.set(210_000);
    select(&ui, &bound, video);
    wait_image(&ui, &bound.preview);
    clock.set(310_000);
    ui.invoke_select_none();
    bound.preview.refresh();
    assert_eq!(read(video).viewtime_ms, (duration * 5).clamp(1000, 100_000));
    clock.set(320_000);
    select(&ui, &bound, first);
    store
        .write(|ctx| {
            let mut settings: hydrus_store::settings::GuiSettings = settings::get(ctx.conn())?;
            settings.confirm_exit = true;
            // Isolate confirmed owner retirement from shutdown maintenance.
            let mut shutdown: hydrus_store::settings::ShutdownWork =
                hydrus_store::settings::get(ctx.conn())?;
            shutdown.action = 0;
            hydrus_store::settings::set(ctx.conn(), &shutdown)?;
            settings::set(ctx.conn(), &settings)
        })
        .unwrap();
    clock.set(320_500);
    ui.window()
        .dispatch_event(slint::platform::WindowEvent::CloseRequested);
    assert!(!ui.get_question().is_empty());
    ui.invoke_answer(false);
    assert!(ui.window().is_visible());
    assert_eq!(read(first).views, 1);
    clock.set(322_000);
    ui.window()
        .dispatch_event(slint::platform::WindowEvent::CloseRequested);
    ui.invoke_answer(true);
    assert!(!ui.window().is_visible());
    assert_eq!((read(first).views, read(first).viewtime_ms), (2, 2000));
    bound.preview.refresh();
    select(&ui, &bound, first);
    bound.preview.close();
    assert_eq!(read(first).views, 2);
    let reopened = Store::open(store.dir()).unwrap();
    assert_eq!(
        reopened
            .read(settings::get::<FileViewingStatistics>)
            .unwrap(),
        saved
    );
}

// leaf: audit-options-file-viewing-statistics-min-time-to-view-on-preview-viewer-to-count-as-a-view
#[test]
fn native_preview_minimum_cap_before_minimum_none_limits_and_inactive_finish() {
    let fixture = hydrus_testkit::fixture_json("preview_viewing_intervals.json");
    let (_directories, store) = store();
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(
        &ui,
        Pages::single(super::common::all_local_page(store.clone())),
    );
    let clock = Rc::new(Cell::new(0));
    bound.preview.set_clock(Rc::new({
        let clock = clock.clone();
        move || clock.get()
    }));
    ui.invoke_search_edited("system:everything".into());
    ui.invoke_search_accepted();
    ui.show().unwrap();
    settle_viewport(&ui, &bound.preview, &windows.get(0).unwrap());
    let first = file(&store, fixture["file"].as_str().unwrap());
    let views = || {
        store
            .read(|conn| hydrus_store::media::viewing_stats(conn, &[first]))
            .unwrap()
            .into_iter()
            .find(|stats| stats.canvas == CanvasType::Preview)
    };
    select(&ui, &bound, first);
    clock.set(49);
    select(&ui, &bound, first);
    clock.set(4999);
    ui.invoke_select_none();
    bound.preview.refresh();
    assert!(views().is_none(), "default preview minimum rejects 4999ms");
    clock.set(5000);
    select(&ui, &bound, first);
    clock.set(10_000);
    ui.invoke_select_none();
    bound.preview.refresh();
    assert_eq!(
        (views().unwrap().views, views().unwrap().viewtime_ms),
        (1, 5000)
    );
    save_settings(&store, true, Some(2000), Some(1000));
    clock.set(11_000);
    select(&ui, &bound, first);
    clock.set(15_000);
    ui.invoke_select_none();
    bound.preview.refresh();
    assert_eq!(
        views().unwrap().views,
        1,
        "cap is applied before the minimum"
    );
    save_settings(&store, true, None, None);
    clock.set(20_000);
    select(&ui, &bound, first);
    clock.set(120_000);
    ui.invoke_select_none();
    bound.preview.refresh();
    assert_eq!(
        (views().unwrap().views, views().unwrap().viewtime_ms),
        (2, 105_000)
    );
    clock.set(130_000);
    select(&ui, &bound, first);
    save_settings(&store, false, None, None);
    clock.set(140_000);
    bound.preview.close();
    assert_eq!(views().unwrap().views, 2, "active is consumed at finish");
}

// leaf: audit-options-file-viewing-statistics-min-time-to-view-on-preview-viewer-to-count-as-a-view
#[test]
fn native_page_generation_same_file_restore_and_late_decode_cannot_publish_to_successor() {
    let fixture = hydrus_testkit::fixture_json("preview_viewing_intervals.json");
    let (_directories, store) = store();
    let windows = headless::init();
    save_settings(&store, true, None, None);
    let first = file(&store, fixture["file"].as_str().unwrap());
    let other = file(
        &store,
        fixture["events"]
            .as_array()
            .unwrap()
            .iter()
            .find(|event| event["event"] == "live-settings-change")
            .unwrap()["shown"]
            .as_str()
            .unwrap(),
    );
    let mut pages = Pages::single(super::common::all_local_page(store.clone()));
    pages.new_search_page();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, pages);
    let clock = Rc::new(Cell::new(100));
    bound.preview.set_clock(Rc::new({
        let clock = clock.clone();
        move || clock.get()
    }));
    ui.show().unwrap();
    settle_viewport(&ui, &bound.preview, &windows.get(0).unwrap());
    ui.invoke_tab_chosen(0, 0);
    ui.invoke_search_edited("system:everything".into());
    ui.invoke_search_accepted();
    select(&ui, &bound, first);
    clock.set(200);
    ui.invoke_tab_chosen(0, 1);
    ui.invoke_search_edited("system:everything".into());
    ui.invoke_search_accepted();
    clock.set(300);
    select(&ui, &bound, first);
    clock.set(400);
    ui.invoke_tab_chosen(0, 0);
    wait_image(&ui, &bound.preview);
    clock.set(500);
    ui.invoke_select_none();
    bound.preview.refresh();
    let stats = store
        .read(|conn| hydrus_store::media::viewing_stats(conn, &[first]))
        .unwrap()
        .into_iter()
        .find(|stats| stats.canvas == CanvasType::Preview)
        .unwrap();
    assert_eq!(
        (stats.views, stats.viewtime_ms, stats.last_viewed.unwrap().0),
        (3, 300, 400),
        "a new page owns a distinct interval even for the same file"
    );

    let (entered, entry) = std::sync::mpsc::channel();
    let (release, released) = std::sync::mpsc::channel();
    let released = std::sync::Mutex::new(released);
    bound.preview.set_decoder(Arc::new({
        let store = store.clone();
        move |_, id| {
            let raster = hydrus_gui::MediaViewer::new(store.clone(), vec![id], 0)
                .unwrap()
                .media();
            if id == first {
                entered.send(()).unwrap();
                released
                    .lock()
                    .unwrap()
                    .recv_timeout(std::time::Duration::from_secs(5))
                    .unwrap();
            }
            raster
        }
    }));
    clock.set(600);
    request(&ui, &bound, first);
    entry
        .recv_timeout(std::time::Duration::from_secs(5))
        .unwrap();
    clock.set(700);
    select(&ui, &bound, other);
    wait_image(&ui, &bound.preview);
    let size = ui.get_preview_media().size();
    release.send(()).unwrap();
    for _ in 0..5 {
        bound.preview.refresh();
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    assert_eq!(ui.get_preview_media().size(), size);
    assert!(ui.get_preview_has_media());
    let after_cancelled_load = store
        .read(|conn| hydrus_store::media::viewing_stats(conn, &[first]))
        .unwrap()
        .into_iter()
        .find(|stats| stats.canvas == CanvasType::Preview)
        .unwrap();
    assert_eq!(
        after_cancelled_load, stats,
        "cancelled loading and late pixels add no first-file view"
    );
    clock.set(800);
    let successor = bind(
        &ui,
        Pages::single(super::common::all_local_page(store.clone())),
    );
    successor.preview.set_clock(Rc::new({
        let clock = clock.clone();
        move || clock.get()
    }));
    ui.invoke_search_edited("system:everything".into());
    ui.invoke_search_accepted();
    clock.set(900);
    select(&ui, &successor, other);
    let image = ui.get_preview_media().size();
    bound.preview.refresh();
    bound.preview.close();
    assert_eq!(ui.get_preview_media().size(), image);
    assert!(ui.get_preview_has_media());
    clock.set(1000);
    successor.preview.close();
    let other_stats = store
        .read(|conn| hydrus_store::media::viewing_stats(conn, &[other]))
        .unwrap()
        .into_iter()
        .find(|stats| stats.canvas == CanvasType::Preview)
        .unwrap();
    assert_eq!(
        (
            other_stats.views,
            other_stats.viewtime_ms,
            other_stats.last_viewed.unwrap().0
        ),
        (2, 200, 900),
        "active rebind retires each owner interval exactly once"
    );
}

#[test]
fn preview_workers_bound_slow_decodes_and_coalesce_the_latest_owned_target() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let fixture = hydrus_testkit::fixture_json("preview_viewing_intervals.json");
    let (_directories, store) = store();
    let windows = headless::init();
    save_settings(&store, true, None, None);
    let first = file(&store, fixture["file"].as_str().unwrap());
    let other = file(
        &store,
        fixture["events"]
            .as_array()
            .unwrap()
            .iter()
            .find(|event| event["event"] == "live-settings-change")
            .unwrap()["shown"]
            .as_str()
            .unwrap(),
    );
    let ui = MainWindow::new().unwrap();
    let bound = bind(
        &ui,
        Pages::single(super::common::all_local_page(store.clone())),
    );
    let clock = Rc::new(Cell::new(100));
    bound.preview.set_clock(Rc::new({
        let clock = clock.clone();
        move || clock.get()
    }));
    let active = Arc::new(AtomicUsize::new(0));
    let maximum = Arc::new(AtomicUsize::new(0));
    let old_calls = Arc::new(AtomicUsize::new(0));
    let latest_calls = Arc::new(AtomicUsize::new(0));
    let (entered, entry) = crossbeam_channel::bounded(2);
    let (release, released) = crossbeam_channel::bounded(2);
    bound.preview.set_decoder(Arc::new({
        let active = active.clone();
        let maximum = maximum.clone();
        let calls = old_calls.clone();
        move |_, id| {
            let count = active.fetch_add(1, Ordering::SeqCst) + 1;
            maximum.fetch_max(count, Ordering::SeqCst);
            calls.fetch_add(1, Ordering::SeqCst);
            entered.send(id).unwrap();
            released
                .recv_timeout(std::time::Duration::from_secs(5))
                .unwrap();
            active.fetch_sub(1, Ordering::SeqCst);
            Some(hydrus_media::Raster::new(1, 1, 3, vec![10; 3]).unwrap())
        }
    }));
    ui.invoke_search_edited("system:everything".into());
    ui.invoke_search_accepted();
    ui.show().unwrap();
    settle_viewport(&ui, &bound.preview, &windows.get(0).unwrap());
    request(&ui, &bound, first);
    assert_eq!(
        entry
            .recv_timeout(std::time::Duration::from_secs(5))
            .unwrap(),
        first
    );
    clock.set(200);
    request(&ui, &bound, other);
    assert_eq!(
        entry
            .recv_timeout(std::time::Duration::from_secs(5))
            .unwrap(),
        other
    );
    bound.preview.set_decoder(Arc::new({
        let active = active.clone();
        let maximum = maximum.clone();
        let calls = latest_calls.clone();
        move |_, id| {
            let count = active.fetch_add(1, Ordering::SeqCst) + 1;
            maximum.fetch_max(count, Ordering::SeqCst);
            calls.fetch_add(1, Ordering::SeqCst);
            assert_eq!(
                id, other,
                "only the latest queued target reaches its decoder snapshot"
            );
            active.fetch_sub(1, Ordering::SeqCst);
            Some(hydrus_media::Raster::new(3, 1, 3, vec![80; 9]).unwrap())
        }
    }));
    for index in 0..64 {
        clock.set(500 + index);
        request(&ui, &bound, if index % 2 == 0 { first } else { other });
    }
    assert_eq!(active.load(Ordering::SeqCst), 2);
    assert_eq!(old_calls.load(Ordering::SeqCst), 2);
    assert_eq!(latest_calls.load(Ordering::SeqCst), 0);
    assert!(ui.get_preview_loading());
    assert!(!ui.get_preview_has_media());
    release.send(()).unwrap();
    release.send(()).unwrap();
    wait_image(&ui, &bound.preview);
    assert_eq!(ui.get_preview_media().size().width, 3);
    assert!(
        maximum.load(Ordering::SeqCst) <= 2,
        "actual concurrent decoders stay bounded"
    );
    assert_eq!(old_calls.load(Ordering::SeqCst), 2);
    assert_eq!(latest_calls.load(Ordering::SeqCst), 1);
    clock.set(2000);
    bound.preview.close();
    let stats = store
        .read(|conn| hydrus_store::media::viewing_stats(conn, &[first, other]))
        .unwrap();
    let previews = stats
        .iter()
        .filter(|stats| stats.canvas == CanvasType::Preview)
        .collect::<Vec<_>>();
    assert_eq!(
        previews.len(),
        1,
        "obsolete and queued loading intervals create no views"
    );
    assert_eq!(
        (
            previews[0].views,
            previews[0].viewtime_ms,
            previews[0].last_viewed.unwrap().0
        ),
        (1, 1437, 563)
    );
    request(&ui, &bound, first);
    assert!(!ui.get_preview_loading());
    assert!(!ui.get_preview_has_media());
    assert_eq!(
        latest_calls.load(Ordering::SeqCst),
        1,
        "close permanently retires the pool"
    );
}

// leaf: audit-options-file-viewing-statistics-min-time-to-view-on-preview-viewer-to-count-as-a-view
#[test]
fn rejected_preview_decode_never_counts_a_thumbnail_selection_or_loading_placeholder() {
    let fixture = hydrus_testkit::fixture_json("preview_viewing_intervals.json");
    let (_directories, store) = store();
    let windows = headless::init();
    save_settings(&store, true, None, None);
    let ui = MainWindow::new().unwrap();
    let bound = bind(
        &ui,
        Pages::single(super::common::all_local_page(store.clone())),
    );
    let clock = Rc::new(Cell::new(100));
    bound.preview.set_clock(Rc::new({
        let clock = clock.clone();
        move || clock.get()
    }));
    let decoded = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    bound.preview.set_decoder(Arc::new({
        let decoded = decoded.clone();
        move |_, _| {
            decoded.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            None
        }
    }));
    ui.invoke_search_edited("system:everything".into());
    ui.invoke_search_accepted();
    ui.show().unwrap();
    settle_viewport(&ui, &bound.preview, &windows.get(0).unwrap());
    let first = file(&store, fixture["file"].as_str().unwrap());
    request(&ui, &bound, first);
    let started = std::time::Instant::now();
    while ui.get_preview_loading() {
        bound.preview.refresh();
        assert!(started.elapsed() < std::time::Duration::from_secs(5));
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert_eq!(
        decoded.load(std::sync::atomic::Ordering::SeqCst),
        1,
        "the visible eligible canvas actually attempted its rejected decode"
    );
    assert!(!ui.get_preview_has_media());
    assert_eq!(ui.get_preview_media().size().width, 0);
    clock.set(10_000);
    bound.preview.close();
    assert!(
        store
            .read(|conn| hydrus_store::media::viewing_stats(conn, &[first]))
            .unwrap()
            .iter()
            .all(|stats| stats.canvas != CanvasType::Preview)
    );
}

// leaf: audit-options-file-viewing-statistics-min-time-to-view-on-preview-viewer-to-count-as-a-view
#[test]
fn preview_rejects_actual_reference_nonlocal_invalid_resolution_and_do_not_show_statuses() {
    let fixture = hydrus_testkit::fixture_json("preview_viewing_intervals.json");
    let windows = headless::init();
    for event in fixture["rejections"].as_array().unwrap() {
        assert_eq!(event["shown"], false);
        let (_directories, store) = store();
        save_settings(&store, true, None, None);
        let first = file(&store, fixture["file"].as_str().unwrap());
        match event["case"].as_str().unwrap() {
            "nonlocal" => store
                .write(move |ctx| {
                    ctx.conn()
                        .execute("DELETE FROM file_domain_current WHERE hash_id=?1", [first])?;
                    Ok(())
                })
                .unwrap(),
            "zero-width" => store
                .write(move |ctx| {
                    ctx.conn()
                        .execute("UPDATE files SET width=0 WHERE hash_id=?1", [first])?;
                    Ok(())
                })
                .unwrap(),
            "do-not-show" => {
                let mime = store
                    .read(|conn| hydrus_store::media::load_basic(conn, &[first]))
                    .unwrap()[0]
                    .info
                    .as_ref()
                    .unwrap()
                    .mime;
                store
                    .write(move |ctx| {
                        let mut settings: hydrus_core::media_viewer::MediaViewerSettings =
                            settings::get(ctx.conn())?;
                        let mut view = settings.view(mime);
                        view.preview_show_action = hydrus_core::media_viewer::ShowAction::DoNotShow;
                        settings.media_view.insert(mime.code(), view);
                        settings::set(ctx.conn(), &settings)
                    })
                    .unwrap();
            }
            name => panic!("unknown rejection {name}"),
        }
        let ui = MainWindow::new().unwrap();
        // A fixed page can legitimately retain a remote or invalid result; the
        // preview must reject it even though the actual thumbnail is selectable.
        let bound = bind(
            &ui,
            Pages::single(hydrus_gui::SearchPage::fixed(
                store.clone(),
                "preview status corpus",
                None,
                vec![first],
            )),
        );
        ui.show().unwrap();
        settle_viewport(
            &ui,
            &bound.preview,
            &windows.get(windows.count() - 1).unwrap(),
        );
        request(&ui, &bound, first);
        assert_eq!(bound.current.borrow().borrow().focused(), Some(0));
        assert!(!ui.get_preview_has_media());
        assert!(!ui.get_preview_loading());
        bound.preview.close();
        assert!(
            store
                .read(|conn| hydrus_store::media::viewing_stats(conn, &[first]))
                .unwrap()
                .iter()
                .all(|stats| stats.canvas != CanvasType::Preview),
            "{event}"
        );
    }
}
