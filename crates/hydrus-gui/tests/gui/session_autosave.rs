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
