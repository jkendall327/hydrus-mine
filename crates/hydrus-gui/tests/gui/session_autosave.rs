//! Actual GUI session controls, timer dispatch, input eligibility and archives.
use hydrus_gui::{MainWindow, OptionsWindow, Pages, bind, headless};
use hydrus_store::{
    Store, sessions,
    settings::{self, GuiIdleSettings, GuiSessionSettings},
};
use slint::{ComponentHandle as _, Model as _};
use std::sync::Arc;

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

fn options(ui: &MainWindow, bound: &hydrus_gui::Bound) -> OptionsWindow {
    ui.invoke_menu_title_pressed(0, 20.0, 22.0);
    let lines = ui.get_menu_panes().row_data(0).unwrap().lines;
    let index = (0..lines.row_count())
        .find(|&i| lines.row_data(i).unwrap().label == "options\u{2026}")
        .unwrap();
    ui.invoke_menu_line_clicked(0, i32::try_from(index).unwrap(), 200.0, 100.0, 10.0);
    let options = bound.options.borrow().as_ref().unwrap().clone_strong();
    let pages = options.get_pages();
    let index = (0..pages.row_count())
        .find(|&i| pages.row_data(i).unwrap().text == "gui sessions")
        .unwrap();
    let index = i32::try_from(index).unwrap();
    options.set_page(index);
    options.invoke_page_chosen(index);
    options
}

fn row(window: &OptionsWindow, label: &str) -> i32 {
    let rows = window.get_rows();
    i32::try_from(
        (0..rows.row_count())
            .find(|&i| rows.row_data(i).unwrap().label == label)
            .unwrap(),
    )
    .unwrap()
}

#[test]
fn applied_idle_and_period_controls_drive_real_archives_with_unchanged_suppression() {
    const PERIOD: &str = "If 'last session' above, autosave it how often (minutes)?";
    const IDLE: &str = "If 'last session' above, only autosave during idle time?";
    let _windows = headless::init();
    let (_dirs, store) = store();
    let before: GuiSessionSettings = store.read(settings::get).unwrap();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    ui.invoke_search_edited("system:everything".into());
    ui.invoke_search_accepted();
    assert!(!bound.current.borrow().borrow().files().is_empty());
    let window = options(&ui, &bound);
    let period = row(&window, PERIOD);
    let idle = row(&window, IDLE);
    window.invoke_number_edited(period, 2);
    window.invoke_check_toggled(idle, true);
    window.invoke_cancel();
    assert_eq!(
        store.read(settings::get::<GuiSessionSettings>).unwrap(),
        before
    );
    let window = options(&ui, &bound);
    window.invoke_number_edited(row(&window, PERIOD), 0);
    window.invoke_apply();
    assert_eq!(
        store.read(settings::get::<GuiSessionSettings>).unwrap(),
        before
    );
    let window = options(&ui, &bound);
    let period = row(&window, PERIOD);
    let idle = row(&window, IDLE);
    window.invoke_number_edited(period, 2);
    window.invoke_check_toggled(idle, true);
    window.invoke_apply();
    let saved: GuiSessionSettings = store.read(settings::get).unwrap();
    assert_eq!(saved.autosave_minutes, 2);
    assert!(saved.only_during_idle);
    store
        .write(move |ctx| {
            settings::set(
                ctx.conn(),
                &GuiIdleSettings {
                    enabled: true,
                    user_seconds: Some(10),
                    mouse_seconds: None,
                    api_seconds: None,
                },
            )
        })
        .unwrap();
    let due = bound.session_autosave.next().unwrap();
    bound.session_autosave.user_at(due);
    assert!(!bound.session_autosave.poll_at(due).unwrap());
    assert_eq!(bound.session_autosave.next(), Some(due + 60_000));
    assert!(
        store
            .read(|conn| hydrus_store::session_backups::latest(conn, sessions::LAST_SESSION))
            .unwrap()
            .is_none()
    );
    assert!(bound.session_autosave.poll_at(due + 60_000).unwrap());
    assert_eq!(bound.session_autosave.next(), Some(due + 180_000));
    let snapshots = store.read(hydrus_store::session_backups::names).unwrap();
    bound.current.borrow().borrow_mut().select(0);
    (bound.sync)();
    assert!(!bound.session_autosave.poll_at(due + 180_000).unwrap());
    assert_eq!(
        store.read(hydrus_store::session_backups::names).unwrap(),
        snapshots
    );
    let previous_count = bound.pages.borrow().session().pages.len();
    // Use the ordinary page chooser consumer to change the live session.
    ui.invoke_new_page();
    ui.invoke_chooser_pressed(6);
    ui.invoke_chooser_pressed(8);
    assert!(bound.session_autosave.poll_at(due + 300_000).unwrap());
    let snapshot = store
        .read(|conn| hydrus_store::session_backups::latest(conn, sessions::LAST_SESSION))
        .unwrap()
        .unwrap();
    assert_eq!(snapshot.session.pages, bound.pages.borrow().session().pages);
    assert_eq!(snapshot.session.pages.len(), previous_count + 1);
    let reopened = Pages::open(store.clone()).unwrap();
    assert_eq!(reopened.session().pages, snapshot.session.pages);
    assert_eq!(
        store.read(settings::get::<GuiSessionSettings>).unwrap(),
        saved
    );
}

#[test]
fn actual_key_and_pointer_events_reset_idle_and_other_startup_stops_autosave() {
    let _windows = headless::init();
    let (_dirs, store) = store();
    store
        .write(move |ctx| {
            settings::set(
                ctx.conn(),
                &GuiIdleSettings {
                    enabled: true,
                    user_seconds: Some(1800),
                    mouse_seconds: Some(600),
                    api_seconds: None,
                },
            )
        })
        .unwrap();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    ui.show().unwrap();
    ui.set_search_focus_requests(ui.get_search_focus_requests() + 1);
    let future = hydrus_core::TimestampMs::now().0 + 120_001;
    bound.session_autosave.user_at(0);
    bound.session_autosave.mouse_at(0);
    assert!(bound.session_autosave.idle_at(future));
    ui.window()
        .dispatch_event(slint::platform::WindowEvent::PointerMoved {
            position: slint::LogicalPosition::new(10.0, 10.0),
        });
    assert!(!bound.session_autosave.idle_at(future));
    bound.session_autosave.mouse_at(0);
    assert!(bound.session_autosave.idle_at(future));
    ui.window()
        .dispatch_event(slint::platform::WindowEvent::KeyPressed { text: "a".into() });
    assert!(!bound.session_autosave.idle_at(future));
    let config = GuiSessionSettings {
        startup: Some("other startup".into()),
        ..GuiSessionSettings::default()
    };
    store
        .write(move |ctx| settings::set(ctx.conn(), &config))
        .unwrap();
    assert!(
        !bound
            .session_autosave
            .poll_at(bound.session_autosave.next().unwrap())
            .unwrap()
    );
    assert_eq!(bound.session_autosave.next(), None);
    assert!(
        store
            .read(|conn| hydrus_store::session_backups::latest(conn, sessions::LAST_SESSION))
            .unwrap()
            .is_none()
    );
}

#[test]
fn auxiliary_native_window_events_reset_only_input_idle_and_preserve_autosave_retry() {
    use slint::winit_030::winit::{
        dpi::PhysicalPosition,
        event::{DeviceId, WindowEvent},
    };
    let _windows = headless::init();
    let (_dirs, store) = store();
    store
        .write(move |ctx| {
            settings::set(
                ctx.conn(),
                &GuiIdleSettings {
                    enabled: true,
                    user_seconds: Some(1800),
                    mouse_seconds: Some(600),
                    api_seconds: None,
                },
            )?;
            settings::set(
                ctx.conn(),
                &GuiSessionSettings {
                    only_during_idle: true,
                    ..GuiSessionSettings::default()
                },
            )
        })
        .unwrap();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    let due = bound.session_autosave.next().unwrap();
    bound.session_autosave.user_at(0);
    bound.session_autosave.mouse_at(0);
    assert!(bound.session_autosave.idle_at(due));
    hydrus_gui::session_autosave::observe_window_event(&WindowEvent::RedrawRequested);
    hydrus_gui::session_autosave::observe_window_event(&WindowEvent::Focused(false));
    assert!(bound.session_autosave.idle_at(due));
    // The global handler receives the same event regardless of which editor
    // window owns it. Movement only changes the mouse timer.
    hydrus_gui::session_autosave::observe_window_event(&WindowEvent::CursorMoved {
        device_id: DeviceId::dummy(),
        position: PhysicalPosition::new(20.0, 25.0),
    });
    assert!(!bound.session_autosave.idle_at(due));
    assert!(!bound.session_autosave.poll_at(due).unwrap());
    assert_eq!(bound.session_autosave.next(), Some(due + 60_000));
    bound.session_autosave.mouse_at(0);
    assert!(bound.session_autosave.idle_at(due));
    hydrus_gui::session_autosave::observe_window_event(&WindowEvent::Focused(true));
    assert!(!bound.session_autosave.idle_at(due));
    bound.session_autosave.user_at(0);
    assert!(bound.session_autosave.poll_at(due + 60_000).unwrap());
    assert!(
        store
            .read(|conn| hydrus_store::session_backups::latest(conn, sessions::LAST_SESSION))
            .unwrap()
            .is_some()
    );
}

#[test]
fn non_page_api_activity_marker_drives_idle_retry_and_expiry_after_reopen() {
    let _windows = headless::init();
    let (_dirs, store) = store();
    store
        .write(move |ctx| {
            settings::set(
                ctx.conn(),
                &GuiIdleSettings {
                    enabled: true,
                    user_seconds: None,
                    mouse_seconds: None,
                    api_seconds: Some(600),
                },
            )?;
            settings::set(
                ctx.conn(),
                &GuiSessionSettings {
                    only_during_idle: true,
                    ..GuiSessionSettings::default()
                },
            )
        })
        .unwrap();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    let due = bound.session_autosave.next().unwrap();
    bound.session_autosave.api_at(0);
    assert!(bound.session_autosave.idle_at(due));
    let request_at = hydrus_core::TimestampMs::now().0;
    hydrus_store::api_activity::touch(store.dir(), request_at).unwrap();
    assert!(!bound.session_autosave.idle_at(due));
    assert!(!bound.session_autosave.poll_at(due).unwrap());
    assert_eq!(bound.session_autosave.next(), Some(due + 60_000));
    assert!(!bound.session_autosave.idle_at(request_at + 600_000));
    assert!(bound.session_autosave.idle_at(request_at + 600_001));
    assert!(
        bound
            .session_autosave
            .poll_at(request_at + 600_001)
            .unwrap()
    );
    let reopened_ui = MainWindow::new().unwrap();
    let reopened = bind(&reopened_ui, Pages::open(store.clone()).unwrap());
    reopened.session_autosave.api_at(0);
    assert!(
        !reopened
            .session_autosave
            .idle_at(reopened.session_autosave.next().unwrap())
    );
    assert_eq!(
        hydrus_store::api_activity::latest(store.dir()).unwrap(),
        Some(request_at)
    );
}

#[test]
fn applied_size_warning_creates_exact_popup_once_and_resets_only_at_new_boot() {
    const WARNING: &str = "Show warning popup if session size exceeds 10,000,000: ";
    let _windows = headless::init();
    let (_dirs, store) = store();
    let fixture = hydrus_testkit::fixture_json("session_warning.json");
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    let before: GuiSessionSettings = store.read(settings::get).unwrap();
    assert_eq!(
        before.warn_large_session,
        fixture["default_enabled"].as_bool().unwrap()
    );
    let window = options(&ui, &bound);
    window.invoke_check_toggled(row(&window, WARNING), false);
    window.invoke_cancel();
    assert_eq!(
        store.read(settings::get::<GuiSessionSettings>).unwrap(),
        before
    );
    let window = options(&ui, &bound);
    window.invoke_check_toggled(row(&window, WARNING), false);
    window.invoke_apply();
    let now = hydrus_core::TimestampMs::now().0;
    assert!(!bound.session_autosave.check_size(10_000_001, now).unwrap());
    assert!(
        !store
            .read(settings::get::<GuiSessionSettings>)
            .unwrap()
            .warn_large_session
    );
    let window = options(&ui, &bound);
    window.invoke_check_toggled(row(&window, WARNING), true);
    window.invoke_apply();
    assert!(!bound.session_autosave.check_size(10_000_000, now).unwrap());
    let message = fixture["steps"][3]["messages"][0].as_str().unwrap();
    assert!(bound.session_autosave.check_size(10_000_001, now).unwrap());
    std::thread::sleep(std::time::Duration::from_millis(300));
    slint::platform::update_timers_and_animations();
    let popups = ui.get_popups();
    let popup = (0..popups.row_count())
        .find(|&index| popups.row_data(index).unwrap().text_1 == message)
        .unwrap();
    ui.invoke_popup_dismiss(i32::try_from(popup).unwrap());
    assert!(
        !bound
            .session_autosave
            .check_size(20_000_000, now + 1_000)
            .unwrap()
    );
    assert!(
        store
            .read(|conn| hydrus_store::popups::all(conn, now / 1_000))
            .unwrap()
            .iter()
            .all(|job| job.status_text_1.as_deref() != Some(message))
    );
    // A new bound client is a new boot; the durable setting survives but the
    // one-boot warning latch does not. No re-enable/reopen repeats within boot.
    let reopened_ui = MainWindow::new().unwrap();
    let reopened = bind(&reopened_ui, Pages::open(store.clone()).unwrap());
    assert!(
        store
            .read(settings::get::<GuiSessionSettings>)
            .unwrap()
            .warn_large_session
    );
    assert!(
        reopened
            .session_autosave
            .check_size(10_000_020, now + 2_000)
            .unwrap()
    );
    let expected = fixture["steps"][5]["messages"][0].as_str().unwrap();
    assert!(
        store
            .read(|conn| hydrus_store::popups::all(conn, now / 1_000 + 2))
            .unwrap()
            .iter()
            .any(|job| job.status_text_1.as_deref() == Some(expected))
    );
}

#[test]
fn native_focus_registry_delivers_to_owned_callback_once_and_releases_dead_viewers() {
    use hydrus_gui::session_autosave::{
        FocusCallback, observe_native_focus, watch_native_focus_id,
    };
    use std::{cell::RefCell, rc::Rc};
    let id = slint::winit_030::winit::window::WindowId::dummy();
    let events = Rc::new(RefCell::new(Vec::new()));
    let callback: FocusCallback = Rc::new({
        let events = events.clone();
        move |focused| events.borrow_mut().push(focused)
    });
    let weak = Rc::downgrade(&callback);
    watch_native_focus_id(id, &callback);
    watch_native_focus_id(id, &callback);
    observe_native_focus(id, true);
    observe_native_focus(id, false);
    assert_eq!(*events.borrow(), [true, false]);
    drop(callback);
    assert!(weak.upgrade().is_none());
    observe_native_focus(id, true);
    assert_eq!(*events.borrow(), [true, false]);
}
