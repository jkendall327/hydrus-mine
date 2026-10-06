//! Real splitter pointer gestures, Pages menu consumers and accepted-exit saving.
use hydrus_gui::{MainWindow, OptionsWindow, Pages, bind, headless};
use hydrus_store::{
    page_layout::{self, PageLayout},
    settings,
};
use slint::{ComponentHandle as _, Model as _};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};
const SAVE: &str = "save current page's sidebar/preview size now";
const EXIT: &str = "save current page's sidebar/preview size on client exit";
const RESTORE: &str = "restore all pages' sidebar/preview sizes to saved value";
const TOGGLE: &str = "show/hide sidebar and preview panel";
const HIDE: &str = "Hide the bottom-left preview window: ";
fn dim(actual: f32, expected: f32) {
    assert!(
        (actual - expected).abs() < 0.1,
        "actual {actual}, expected {expected}"
    );
}
fn render(native: &slint::platform::software_renderer::MinimalSoftwareWindow) -> Vec<u8> {
    for _ in 0..3 {
        headless::render(native, 1400, 1000);
    }
    headless::render_snapshot(native, 1400, 1000)
}
fn sidebar(ui: &MainWindow, label: &str) -> bool {
    let index = ui
        .get_menu_titles()
        .iter()
        .position(|t| t.label == "pages")
        .unwrap() as i32;
    ui.invoke_menu_title_pressed(index, 20.0, 22.0);
    let lines = ui.get_menu_panes().row_data(0).unwrap().lines;
    let index = lines.iter().position(|r| r.label == "sidebar").unwrap() as i32;
    ui.invoke_menu_line_hovered(0, index, 300.0, 100.0, 20.0);
    let pane = ui.get_menu_panes().row_count() - 1;
    let lines = ui.get_menu_panes().row_data(pane).unwrap().lines;
    let index = lines.iter().position(|r| r.label == label).unwrap();
    let row = lines.row_data(index).unwrap();
    assert!(row.usable);
    ui.invoke_menu_line_clicked(pane as i32, index as i32, 0.0, 0.0, 0.0);
    row.checked
}
pub(super) fn restore(ui: &MainWindow) {
    sidebar(ui, RESTORE);
}
fn options(ui: &MainWindow, bound: &hydrus_gui::Bound) -> OptionsWindow {
    ui.invoke_menu_title_pressed(0, 20.0, 22.0);
    let lines = ui.get_menu_panes().row_data(0).unwrap().lines;
    let index = lines.iter().position(|r| r.label == "options…").unwrap() as i32;
    ui.invoke_menu_line_clicked(0, index, 0.0, 0.0, 0.0);
    let w = bound.options.borrow().as_ref().unwrap().clone_strong();
    let p = w
        .get_pages()
        .iter()
        .position(|p| p.text == "gui pages")
        .unwrap() as i32;
    w.invoke_page_chosen(p);
    w
}
fn hide(w: &OptionsWindow, value: bool) {
    let i = w.get_rows().iter().position(|r| r.label == HIDE).unwrap() as i32;
    w.invoke_check_toggled(i, value);
}
fn new_page(ui: &MainWindow, bound: &hydrus_gui::Bound) {
    bound.pages.borrow_mut().new_search_page();
    let (d, i) = bound.pages.borrow().shown_position();
    ui.invoke_tab_chosen(d as i32, i as i32);
}
fn gesture(
    native: &slint::platform::software_renderer::MinimalSoftwareWindow,
    x: f32,
    y: f32,
    dx: f32,
    dy: f32,
) {
    use slint::platform::{PointerEventButton as B, WindowEvent as E};
    let start = slint::LogicalPosition::new(x, y);
    let end = slint::LogicalPosition::new(x + dx, y + dy);
    native.dispatch_event(E::PointerMoved { position: start });
    native.dispatch_event(E::PointerPressed {
        position: start,
        button: B::Left,
    });
    native.dispatch_event(E::PointerMoved { position: end });
    native.dispatch_event(E::PointerReleased {
        position: end,
        button: B::Left,
    });
}
#[test]
fn real_drag_menu_saved_defaults_per_page_reopen_restore_options_and_exit_ownership() {
    let (_dirs, store) = crate::subscriptions::store();
    let fixture = hydrus_testkit::fixture_json("sidebar_layout.json");
    assert_eq!(
        store.read(page_layout::load).unwrap(),
        PageLayout::default()
    );
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(
        &ui,
        Pages::single(super::common::all_local_page(store.clone())),
    );
    let native = windows.get(0).unwrap();
    render(&native);
    dim(ui.get_sidebar_actual_width(), 400.0);
    dim(ui.get_preview_actual_height(), 240.0);
    gesture(
        &native,
        ui.get_sidebar_handle_x() + 3.0,
        ui.get_sidebar_handle_y() + 150.0,
        70.0,
        0.0,
    );
    render(&native);
    dim(ui.get_sidebar_actual_width(), 470.0);
    gesture(
        &native,
        ui.get_preview_handle_x() + 100.0,
        ui.get_preview_handle_y() + 3.0,
        0.0,
        -55.0,
    );
    render(&native);
    dim(ui.get_preview_actual_height(), 295.0);
    sidebar(&ui, TOGGLE);
    render(&native);
    sidebar(&ui, SAVE);
    assert_eq!(
        (
            store.read(page_layout::load).unwrap().hpos,
            store.read(page_layout::load).unwrap().vpos
        ),
        (0, -295),
        "hidden inner preview retains the unsaved drag height"
    );
    store
        .write(|ctx| settings::set(ctx.conn(), &PageLayout::default()))
        .unwrap();
    sidebar(&ui, RESTORE);
    ui.invoke_sidebar_resized(
        ui.get_layout_page_key(),
        ui.get_layout_epoch(),
        false,
        470.0,
    );
    ui.invoke_sidebar_resized(ui.get_layout_page_key(), ui.get_layout_epoch(), true, 295.0);
    render(&native);
    let first = ui.get_layout_page_key();
    let epoch = ui.get_layout_epoch();
    new_page(&ui, &bound);
    render(&native);
    dim(ui.get_sidebar_actual_width(), 400.0);
    let second = ui.get_layout_page_key();
    ui.invoke_sidebar_resized(second.clone(), ui.get_layout_epoch(), false, 460.0);
    ui.invoke_sidebar_resized(second.clone(), ui.get_layout_epoch(), true, 280.0);
    ui.invoke_sidebar_resized(first.clone(), epoch, false, 900.0);
    render(&native);
    dim(ui.get_sidebar_actual_width(), 460.0);
    ui.invoke_tab_chosen(0, 0);
    render(&native);
    dim(ui.get_sidebar_actual_width(), 470.0);
    dim(ui.get_preview_actual_height(), 295.0);
    sidebar(&ui, SAVE);
    assert_eq!(
        serde_json::json!([
            store.read(page_layout::load).unwrap().hpos,
            store.read(page_layout::load).unwrap().vpos
        ]),
        fixture["steps"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["action"] == "save current now")
            .unwrap()["saved"]
    );
    new_page(&ui, &bound);
    render(&native);
    dim(ui.get_sidebar_actual_width(), 470.0);
    dim(ui.get_preview_actual_height(), 295.0);
    // Per-live-page geometry survives closing/reopening; defaults are independent.
    ui.invoke_sidebar_resized(
        ui.get_layout_page_key(),
        ui.get_layout_epoch(),
        false,
        475.0,
    );
    ui.invoke_sidebar_resized(ui.get_layout_page_key(), ui.get_layout_epoch(), true, 290.0);
    bound.pages.borrow_mut().close_shown().unwrap();
    ui.invoke_tab_chosen(0, 0);
    assert!(bound.pages.borrow_mut().unclose());
    let (d, i) = bound.pages.borrow().shown_position();
    ui.invoke_tab_chosen(d as i32, i as i32);
    render(&native);
    dim(ui.get_sidebar_actual_width(), 475.0);
    dim(ui.get_preview_actual_height(), 290.0);
    ui.invoke_tab_chosen(0, 0);
    let hidden_page = bound.pages.borrow().shown().key;
    sidebar(&ui, TOGGLE);
    let hidden = render(&native);
    assert!(ui.get_sidebar_hidden());
    dim(ui.get_sidebar_actual_width(), 0.0);
    assert_eq!(bound.pages.borrow().shown().key, hidden_page);
    assert_eq!(ui.get_layout_page_key(), hidden_page.to_hex());
    headless::save_png(
        &std::path::Path::new(env!("CARGO_TARGET_TMPDIR"))
            .join("sidebar-hidden-current-native.png"),
        &hidden,
        1400,
        1000,
    )
    .unwrap();
    sidebar(&ui, SAVE);
    let saved = store.read(page_layout::load).unwrap();
    assert_eq!((saved.hpos, saved.vpos), (0, -295));
    // Restore reaches current and inactive pages, not just a single UI property.
    store
        .write(|ctx| {
            settings::set(
                ctx.conn(),
                &PageLayout {
                    hpos: 420,
                    vpos: -260,
                    ..Default::default()
                },
            )
        })
        .unwrap();
    sidebar(&ui, RESTORE);
    render(&native);
    dim(ui.get_sidebar_actual_width(), 420.0);
    dim(ui.get_preview_actual_height(), 260.0);
    bound
        .pages
        .borrow_mut()
        .show(&hydrus_core::pages::PageKey::from_hex(&second).unwrap());
    let (d, i) = bound.pages.borrow().shown_position();
    ui.invoke_tab_chosen(d as i32, i as i32);
    render(&native);
    dim(ui.get_sidebar_actual_width(), 420.0);
    dim(ui.get_preview_actual_height(), 260.0);
    let w = options(&ui, &bound);
    hide(&w, true);
    w.invoke_cancel();
    assert!(!store.read(page_layout::load).unwrap().hide_preview);
    let w = options(&ui, &bound);
    hide(&w, true);
    w.invoke_apply();
    assert!(store.read(page_layout::load).unwrap().hide_preview);
    render(&native);
    dim(ui.get_preview_actual_height(), 260.0);
    sidebar(&ui, RESTORE);
    render(&native);
    dim(ui.get_preview_actual_height(), 0.0);
    new_page(&ui, &bound);
    let new_page_key = bound.pages.borrow().shown().key;
    let hidden_preview = render(&native);
    dim(ui.get_preview_actual_height(), 0.0);
    assert!(ui.get_preview_splitter_hidden());
    assert_eq!(bound.pages.borrow().shown().key, new_page_key);
    assert_eq!(ui.get_layout_page_key(), new_page_key.to_hex());
    headless::save_png(
        &std::path::Path::new(env!("CARGO_TARGET_TMPDIR"))
            .join("sidebar-preview-hidden-new-native.png"),
        &hidden_preview,
        1400,
        1000,
    )
    .unwrap();
    let w = options(&ui, &bound);
    hide(&w, false);
    w.invoke_apply();
    sidebar(&ui, RESTORE);
    render(&native);
    dim(ui.get_preview_actual_height(), 260.0);
    let screenshot = render(&native);
    headless::save_png(
        &std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("sidebar_layout_native.png"),
        &screenshot,
        1400,
        1000,
    )
    .unwrap();
    assert!(sidebar(&ui, EXIT));
    assert!(!store.read(page_layout::load).unwrap().save_on_exit);
    assert!(!sidebar(&ui, EXIT));
    assert!(store.read(page_layout::load).unwrap().save_on_exit);
    store
        .write(|ctx| {
            let mut s: settings::GuiSettings = settings::get(ctx.conn())?;
            s.confirm_exit = true;
            // Isolate confirmed owner retirement from shutdown maintenance.
            let mut shutdown: hydrus_store::settings::ShutdownWork =
                hydrus_store::settings::get(ctx.conn())?;
            shutdown.action = 0;
            hydrus_store::settings::set(ctx.conn(), &shutdown)?;
            settings::set(ctx.conn(), &s)
        })
        .unwrap();
    let key = ui.get_layout_page_key();
    let epoch = ui.get_layout_epoch();
    ui.invoke_sidebar_resized(key.clone(), epoch, false, 510.0);
    ui.invoke_sidebar_resized(key.clone(), epoch, true, 310.0);
    render(&native);
    ui.window()
        .dispatch_event(slint::platform::WindowEvent::CloseRequested);
    ui.invoke_answer(false);
    assert!(ui.window().is_visible());
    assert_eq!(store.read(page_layout::load).unwrap().hpos, 420);
    ui.hide().unwrap();
    ui.invoke_sidebar_resized(key.clone(), epoch, false, 900.0);
    ui.show().unwrap();
    render(&native);
    dim(ui.get_sidebar_actual_width(), 510.0);
    ui.window()
        .dispatch_event(slint::platform::WindowEvent::CloseRequested);
    ui.invoke_answer(true);
    assert!(!ui.window().is_visible());
    assert_eq!(
        (
            store.read(page_layout::load).unwrap().hpos,
            store.read(page_layout::load).unwrap().vpos
        ),
        (510, -310)
    );
    ui.invoke_sidebar_resized(key, epoch, false, 990.0);
    ui.show().unwrap();
    render(&native);
    dim(ui.get_sidebar_actual_width(), 510.0);
    ui.hide().unwrap();
    let reopened = hydrus_store::Store::open(store.dir()).unwrap();
    assert_eq!(
        reopened.read(page_layout::load).unwrap(),
        store.read(page_layout::load).unwrap()
    );
    let successor = MainWindow::new().unwrap();
    successor.show().unwrap();
    let _successor = bind(&successor, Pages::open(reopened).unwrap());
    let rendered = windows.get(windows.count() - 1).unwrap();
    render(&rendered);
    dim(successor.get_sidebar_actual_width(), 510.0);
    dim(successor.get_preview_actual_height(), 310.0);
}
#[test]
fn live_hide_setting_keeps_accepted_preview_refuses_replacements_and_collapse_retains_selection() {
    let (_dirs, store) = crate::subscriptions::store();
    store
        .write(|ctx| {
            ctx.conn()
                .execute("DELETE FROM file_viewing_stats WHERE canvas_type=1", [])?;
            let mut value: settings::FileViewingStatistics = settings::get(ctx.conn())?;
            value.active = true;
            value.preview_min_ms = None;
            value.preview_max_ms = None;
            settings::set(ctx.conn(), &value)
        })
        .unwrap();
    let f = hydrus_testkit::fixture_json("sidebar_layout.json");
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(
        &ui,
        Pages::single(super::common::all_local_page(store.clone())),
    );
    let native = windows.get(0).unwrap();
    render(&native);
    let clock = Rc::new(Cell::new(1000));
    bound.preview.set_clock(Rc::new({
        let clock = clock.clone();
        move || clock.get()
    }));
    ui.invoke_search_edited("system:everything".into());
    ui.invoke_search_accepted();
    let hash = f["preview_consumer"][0]["shown"]
        .as_str()
        .unwrap()
        .parse::<hydrus_core::Sha256>()
        .unwrap();
    let first = store
        .read(|c| hydrus_store::master::hash_id(c, &hash))
        .unwrap()
        .unwrap();
    let index = bound
        .current
        .borrow()
        .borrow()
        .results()
        .iter()
        .position(|id| *id == first)
        .unwrap();
    ui.invoke_thumbnail_clicked(index as i32, false, false);
    let started = std::time::Instant::now();
    while ui.get_preview_loading() {
        bound.preview.refresh();
        assert!(started.elapsed() < std::time::Duration::from_secs(5));
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert!(ui.get_preview_has_media());
    let size = ui.get_preview_media().size();
    let w = options(&ui, &bound);
    hide(&w, true);
    w.invoke_apply();
    let second = bound
        .current
        .borrow()
        .borrow()
        .results()
        .iter()
        .position(|id| *id != first)
        .unwrap();
    ui.invoke_thumbnail_clicked(second as i32, false, false);
    bound.preview.refresh();
    assert!(ui.get_preview_has_media());
    assert_eq!(ui.get_preview_media().size(), size);
    ui.invoke_thumbnail_clicked(-1, false, false);
    bound.preview.refresh();
    assert!(
        ui.get_preview_has_media(),
        "Qt global hide rejects ClearMedia as well"
    );
    sidebar(&ui, RESTORE);
    bound.preview.refresh();
    assert!(
        ui.get_preview_has_media(),
        "Qt global hide also rejects splitter clear"
    );
    ui.invoke_sidebar_collapsed(ui.get_layout_page_key(), ui.get_layout_epoch(), true);
    bound.preview.refresh();
    assert!(ui.get_preview_has_media());
    clock.set(2000);
    new_page(&ui, &bound);
    bound.preview.refresh();
    assert!(
        !ui.get_preview_has_media(),
        "old page media cannot cross to a new owner"
    );
    ui.invoke_tab_chosen(0, 0);
    bound.preview.refresh();
    assert!(
        ui.get_preview_has_media(),
        "the returned live page retains its own accepted raster under global hide"
    );
    let totals = || {
        let rows = store
            .read(|c| hydrus_store::media::viewing_stats(c, &[first]))
            .unwrap();
        let row = rows
            .into_iter()
            .find(|r| r.canvas == hydrus_core::CanvasType::Preview)
            .unwrap();
        (row.views, row.viewtime_ms)
    };
    assert_eq!(
        store
            .read(|c| Ok(c.query_row(
                "SELECT count(*) FROM file_viewing_stats WHERE canvas_type=1",
                [],
                |r| r.get::<_, i64>(0)
            )?))
            .unwrap(),
        0,
        "global hide rejects PageHidden clear, so the first owned interval continues"
    );

    let w = options(&ui, &bound);
    hide(&w, false);
    w.invoke_apply();
    sidebar(&ui, RESTORE);
    // Actual Qt restore retains accepted media and its original viewing start,
    // even though thumbnail focus was cleared while globally hidden.
    let recorded = &f["preview_restore_probe"];
    let before = &recorded[0];
    let restored = recorded
        .as_array()
        .unwrap()
        .iter()
        .find(|step| step["action"] == "after restore event settle")
        .unwrap();
    assert!(recorded.as_array().unwrap().iter().all(|step| {
        step["page"]["accepted"] == before["page"]["accepted"]
            && step["page"]["start"] == before["page"]["start"]
            && step["interval_cursor"] == before["interval_cursor"]
    }));
    assert_eq!(
        restored["page"]["accepted"],
        serde_json::json!(hash.to_hex())
    );
    assert_eq!(restored["page"]["canvas_visible"], true);
    assert!(restored["page"]["focused"].is_null());
    render(&native);
    bound.preview.refresh();
    assert!(!ui.get_preview_splitter_hidden());
    assert!(bound.current.borrow().borrow().focused().is_none());
    assert!(
        ui.get_preview_has_media(),
        "restore retains the Qt accepted canvas"
    );
    assert_eq!(bound.preview.displayed_file(), Some(first));
    assert_eq!(ui.get_preview_media().size(), size);
    assert_eq!(
        store
            .read(|c| Ok(c.query_row(
                "SELECT count(*) FROM file_viewing_stats WHERE canvas_type=1",
                [],
                |r| r.get::<_, i64>(0)
            )?))
            .unwrap(),
        0,
        "restore does not finish the accepted interval"
    );
    ui.invoke_thumbnail_clicked(index as i32, false, false);
    let started = std::time::Instant::now();
    while ui.get_preview_loading() {
        bound.preview.refresh();
        assert!(started.elapsed() < std::time::Duration::from_secs(5));
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert!(ui.get_preview_has_media());
    let probe = f["preview_restore_selection_probe"].as_array().unwrap();
    let recorded_step = |action: &str| probe.iter().find(|step| step["action"] == action).unwrap();
    let restore = recorded_step("after restore event settle before next selection");
    let selected = recorded_step("after next selection settle");
    let collapsed = recorded_step("after final collapse settle");
    assert_eq!(selected["page"]["accepted"], restore["page"]["accepted"]);
    assert_eq!(selected["page"]["start"], restore["page"]["start"]);
    assert_eq!(selected["interval_cursor"], restore["interval_cursor"]);
    assert!(collapsed["page"]["accepted"].is_null());
    assert!(collapsed["page"]["focused"].is_null());
    assert_eq!(collapsed["page"]["selected"], selected["page"]["selected"]);
    assert_eq!(
        collapsed["interval_cursor"].as_u64().unwrap(),
        selected["interval_cursor"].as_u64().unwrap() + 1
    );
    assert_eq!(bound.preview.displayed_file(), Some(first));
    let selection = bound.current.borrow().borrow().selected_files();
    clock.set(3000);
    ui.invoke_sidebar_collapsed(ui.get_layout_page_key(), ui.get_layout_epoch(), true);
    bound.preview.refresh();
    assert!(!ui.get_preview_has_media());
    assert_eq!(bound.current.borrow().borrow().selected_files(), selection);
    assert!(bound.current.borrow().borrow().focused().is_none());
    assert_eq!(
        totals(),
        (1, 2000),
        "collapse ends the original interval once after its hidden page roundtrip"
    );
    bound.preview.close();
    assert_eq!(totals(), (1, 2000));
    let reopened = hydrus_store::Store::open(store.dir()).unwrap();
    let row = reopened
        .read(|c| hydrus_store::media::viewing_stats(c, &[first]))
        .unwrap()
        .into_iter()
        .find(|r| r.canvas == hydrus_core::CanvasType::Preview)
        .unwrap();
    assert_eq!((row.views, row.viewtime_ms), (1, 2000));

    sidebar(&ui, RESTORE);
    bound.preview.refresh();
    assert!(
        !ui.get_preview_has_media(),
        "reveal does not restore old focused media"
    );
}

#[test]
fn pressed_old_handle_cannot_resize_same_key_successor_binding_or_same_session_reload() {
    use slint::platform::{PointerEventButton as B, WindowEvent as E};

    let (_dirs, store) = crate::subscriptions::store();
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let first = bind(
        &ui,
        Pages::single(super::common::all_local_page(store.clone())),
    );
    let native = windows.get(0).unwrap();
    render(&native);
    first.pages.borrow_mut().save(1).unwrap();
    let key = ui.get_layout_page_key();
    let epoch = ui.get_layout_epoch();
    let start = slint::LogicalPosition::new(
        ui.get_sidebar_handle_x() + 3.0,
        ui.get_sidebar_handle_y() + 150.0,
    );
    native.dispatch_event(E::PointerMoved { position: start });
    native.dispatch_event(E::PointerPressed {
        position: start,
        button: B::Left,
    });
    let successor = bind(&ui, Pages::open(store.clone()).unwrap());
    render(&native);
    assert_eq!(
        ui.get_layout_page_key(),
        key,
        "same persisted page identity challenges owner incarnation"
    );
    assert_ne!(ui.get_layout_epoch(), epoch);
    let moved = slint::LogicalPosition::new(start.x + 200.0, start.y);
    native.dispatch_event(E::PointerMoved { position: moved });
    native.dispatch_event(E::PointerReleased {
        position: moved,
        button: B::Left,
    });
    render(&native);
    dim(ui.get_sidebar_actual_width(), 400.0);
    ui.invoke_sidebar_resized(key.clone(), epoch, false, 950.0);
    render(&native);
    dim(ui.get_sidebar_actual_width(), 400.0);
    let old_owner = successor.current.borrow().clone();
    successor
        .pages
        .borrow_mut()
        .save_session("reload layout", 1)
        .unwrap();
    successor
        .pages
        .borrow_mut()
        .clear_and_load("reload layout")
        .unwrap();
    ui.invoke_tab_chosen(0, 0);
    render(&native);
    assert!(!Rc::ptr_eq(&old_owner, &successor.current.borrow()));
    ui.invoke_sidebar_resized(key, ui.get_layout_epoch() - 1, false, 900.0);
    render(&native);
    dim(ui.get_sidebar_actual_width(), 400.0);
    // Positive legacy and oversized raw positions save the actual hidden inner size.
    for vpos in [600, -10_000] {
        store
            .write(move |ctx| {
                settings::set(
                    ctx.conn(),
                    &PageLayout {
                        vpos,
                        ..Default::default()
                    },
                )
            })
            .unwrap();
        sidebar(&ui, RESTORE);
        render(&native);
        let preview = ui.get_preview_actual_height().round() as i64;
        assert!(preview > 0);
        sidebar(&ui, TOGGLE);
        render(&native);
        sidebar(&ui, SAVE);
        assert_eq!(store.read(page_layout::load).unwrap().vpos, -preview);
    }
    store
        .write(|ctx| settings::set(ctx.conn(), &PageLayout::default()))
        .unwrap();
    sidebar(&ui, RESTORE);
    render(&native);
    // save-on-exit false leaves the durable default untouched after an accepted close.
    assert!(sidebar(&ui, EXIT));
    ui.invoke_sidebar_resized(
        ui.get_layout_page_key(),
        ui.get_layout_epoch(),
        false,
        525.0,
    );
    render(&native);
    store
        .write(|ctx| {
            let mut s: settings::GuiSettings = settings::get(ctx.conn())?;
            s.confirm_exit = false;
            settings::set(ctx.conn(), &s)?;
            // This boundary tests completed exit, independently of due maintenance.
            let mut shutdown: hydrus_store::settings::ShutdownWork =
                hydrus_store::settings::get(ctx.conn())?;
            shutdown.action = 0;
            settings::set(ctx.conn(), &shutdown)
        })
        .unwrap();
    ui.window().dispatch_event(E::CloseRequested);
    assert!(!ui.window().is_visible());
    assert_eq!(store.read(page_layout::load).unwrap().hpos, 400);
}

#[test]
fn rebind_retires_preview_under_predecessor_splitter_before_successor_hides_it() {
    let (_directories, store) = crate::subscriptions::store();
    store
        .write(|ctx| {
            ctx.conn()
                .execute("DELETE FROM file_viewing_stats WHERE canvas_type=1", [])?;
            let mut stats: settings::FileViewingStatistics = settings::get(ctx.conn())?;
            stats.active = true;
            stats.preview_min_ms = None;
            stats.preview_max_ms = None;
            settings::set(ctx.conn(), &stats)?;
            settings::set(ctx.conn(), &PageLayout::default())
        })
        .unwrap();
    let fixture = hydrus_testkit::fixture_json("sidebar_layout.json");
    let hash: hydrus_core::Sha256 = fixture["preview_consumer"][0]["shown"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    let file = store
        .read(|conn| hydrus_store::master::hash_id(conn, &hash))
        .unwrap()
        .unwrap();
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let first = bind(
        &ui,
        Pages::single(super::common::all_local_page(store.clone())),
    );
    ui.invoke_search_edited("system:everything".into());
    ui.invoke_search_accepted();
    render(&windows.get(0).unwrap());
    assert!(!ui.get_preview_splitter_hidden());
    let now = Rc::new(Cell::new(1000));
    let observed_splitters = Rc::new(RefCell::new(Vec::new()));
    first.preview.set_clock(Rc::new({
        let now = now.clone();
        let observed_splitters = observed_splitters.clone();
        let weak = ui.as_weak();
        move || {
            let window = weak.upgrade().unwrap();
            observed_splitters
                .borrow_mut()
                .push(window.get_preview_splitter_hidden());
            now.get()
        }
    }));
    first
        .preview
        .set_decoder(std::sync::Arc::new(move |_, decoded| {
            assert_eq!(
                decoded, file,
                "accepted raster belongs to the exact selected file"
            );
            Some(hydrus_media::Raster::new(1, 1, 3, vec![10, 20, 30]).unwrap())
        }));
    let index = first
        .current
        .borrow()
        .borrow()
        .results()
        .iter()
        .position(|id| *id == file)
        .unwrap();
    ui.invoke_thumbnail_clicked(i32::try_from(index).unwrap(), false, false);
    let started = std::time::Instant::now();
    while !ui.get_preview_has_media() {
        first.preview.refresh();
        assert!(
            started.elapsed() < std::time::Duration::from_secs(5),
            "actual owned decode must publish its accepted frame"
        );
        std::thread::yield_now();
    }
    assert_eq!(
        (
            ui.get_preview_media().size().width,
            ui.get_preview_media().size().height
        ),
        (1, 1),
        "the exact selected file's accepted frame is displayed"
    );
    now.set(2000);
    observed_splitters.borrow_mut().clear();
    store
        .write(|ctx| {
            let mut saved: PageLayout = page_layout::load(ctx.conn())?;
            saved.hpos = 0;
            settings::set(ctx.conn(), &saved)
        })
        .unwrap();
    // Persisting a successor's default has not changed the predecessor's splitter.
    assert!(!ui.get_preview_splitter_hidden());
    let successor = bind(
        &ui,
        Pages::single(super::common::all_local_page(store.clone())),
    );
    assert!(ui.get_preview_splitter_hidden());
    assert_eq!(
        *observed_splitters.borrow(),
        [false],
        "retirement samples predecessor geometry before any successor splitter setter/presentation callback"
    );
    successor.preview.refresh();
    assert!(!ui.get_preview_has_media());
    assert_eq!(
        (
            ui.get_preview_media().size().width,
            ui.get_preview_media().size().height
        ),
        (0, 0),
        "the successor cannot retain its predecessor's frame"
    );
    let stats = store
        .read(|conn| hydrus_store::media::viewing_stats(conn, &[file]))
        .unwrap()
        .into_iter()
        .find(|row| row.canvas == hydrus_core::CanvasType::Preview)
        .unwrap();
    assert_eq!((stats.views, stats.viewtime_ms), (1, 1000));
    first.preview.refresh();
    first.preview.close();
    assert_eq!(
        *observed_splitters.borrow(),
        [false],
        "retired monitor cannot observe or clear successor state"
    );
    let stats = store
        .read(|conn| hydrus_store::media::viewing_stats(conn, &[file]))
        .unwrap()
        .into_iter()
        .find(|row| row.canvas == hydrus_core::CanvasType::Preview)
        .unwrap();
    assert_eq!(
        (stats.views, stats.viewtime_ms),
        (1, 1000),
        "replayed old close cannot duplicate the durable interval"
    );
}
