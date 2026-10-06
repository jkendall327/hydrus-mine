//! Real Options drafts feed the running session monitor's saved idle gates.
use hydrus_gui::{MainWindow, OptionsWindow, Pages, SearchPage, bind, headless};
use hydrus_store::{
    Store,
    settings::{self, GuiIdleSettings, GuiSessionSettings},
};
use slint::{ComponentHandle as _, Model as _};
const LABELS: [&str; 3] = [
    "Permit idle mode if no general browsing activity has occurred in the past: ",
    "Permit idle mode if your mouse cursor has not been moved in the past: ",
    "Permit idle mode if no Client API requests in the past: ",
];
fn open(ui: &MainWindow, bound: &hydrus_gui::Bound) -> OptionsWindow {
    ui.invoke_menu_title_pressed(0, 20.0, 22.0);
    let lines = ui.get_menu_panes().row_data(0).unwrap().lines;
    let at = lines
        .iter()
        .position(|line| line.label == "options\u{2026}")
        .unwrap();
    ui.invoke_menu_line_clicked(0, i32::try_from(at).unwrap(), 0.0, 0.0, 0.0);
    let window = bound.options.borrow().as_ref().unwrap().clone_strong();
    let page = window
        .get_pages()
        .iter()
        .position(|row| row.text == "maintenance and processing")
        .unwrap();
    window.invoke_page_chosen(i32::try_from(page).unwrap());
    window
}
fn row(window: &OptionsWindow, field: usize) -> i32 {
    i32::try_from(
        window
            .get_rows()
            .iter()
            .position(|row| row.label == LABELS[field])
            .unwrap(),
    )
    .unwrap()
}
fn set(window: &OptionsWindow, values: [Option<i32>; 3]) {
    for (field, value) in values.into_iter().enumerate() {
        let row = row(window, field);
        window.invoke_none_toggled(row, value.is_none());
        if let Some(minutes) = value {
            window.invoke_number_edited(row, minutes);
        }
    }
}
#[test]
fn staged_timeouts_change_live_browsing_mouse_api_and_autosave_gates_only_after_apply() {
    let (dirs, store) = crate::subscriptions::store();
    store
        .write(|ctx| {
            settings::set(
                ctx.conn(),
                &GuiSessionSettings {
                    only_during_idle: true,
                    ..GuiSessionSettings::default()
                },
            )
        })
        .unwrap();
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::single(SearchPage::new(store.clone())));
    ui.invoke_search_edited("system:everything".into());
    ui.invoke_search_accepted();
    let before = store.read(settings::get::<GuiIdleSettings>).unwrap();
    let due = bound.session_autosave.next().unwrap();
    bound.session_autosave.user_at(0);
    bound.session_autosave.mouse_at(0);
    bound.session_autosave.api_at(0);
    let window = open(&ui, &bound);
    let drawn = windows.get(windows.count() - 1).unwrap();
    headless::render(&drawn, 1000, 850);
    let api = window
        .get_rows()
        .row_data(usize::try_from(row(&window, 2)).unwrap())
        .unwrap();
    assert!(api.is_none);
    assert_eq!(api.number, 1);
    assert_eq!((api.minimum, api.maximum), (1, 1000));
    assert_eq!(api.unit, "minutes");
    set(&window, [Some(1), None, None]);
    assert_eq!(
        store.read(settings::get::<GuiIdleSettings>).unwrap(),
        before
    );
    window.invoke_cancel();
    assert_eq!(
        store.read(settings::get::<GuiIdleSettings>).unwrap(),
        before
    );
    let window = open(&ui, &bound);
    set(&window, [Some(10), None, None]);
    window.invoke_apply();
    assert!(bound.session_autosave.idle_at(due));
    // The same native activity route receives events from auxiliary windows.
    hydrus_gui::session_autosave::observe_window_event(
        &slint::winit_030::winit::event::WindowEvent::Focused(true),
    );
    assert!(!bound.session_autosave.idle_at(due));
    assert!(!bound.session_autosave.poll_at(due).unwrap());
    assert_eq!(bound.session_autosave.next(), Some(due + 60_000));
    let window = open(&ui, &bound);
    set(&window, [Some(1), None, None]);
    window.invoke_apply();
    bound.session_autosave.user_at(due);
    assert!(!bound.session_autosave.idle_at(due + 60_000));
    assert!(bound.session_autosave.idle_at(due + 60_001));
    let draft = open(&ui, &bound);
    set(&draft, [None, None, None]);
    assert!(!bound.session_autosave.idle_at(due + 1));
    draft.invoke_cancel();
    assert!(!bound.session_autosave.idle_at(due + 1));
    let window = open(&ui, &bound);
    set(&window, [None, None, None]);
    window.invoke_apply();
    assert!(bound.session_autosave.idle_at(due + 1));
    assert!(bound.session_autosave.poll_at(due + 60_000).unwrap());
    assert!(
        store
            .read(|conn| hydrus_store::session_backups::latest(
                conn,
                hydrus_store::sessions::LAST_SESSION
            ))
            .unwrap()
            .is_some()
    );
    let window = open(&ui, &bound);
    set(&window, [None, Some(1), None]);
    window.invoke_apply();
    bound.session_autosave.mouse_at(due);
    assert!(!bound.session_autosave.idle_at(due + 60_000));
    assert!(bound.session_autosave.idle_at(due + 60_001));
    let window = open(&ui, &bound);
    set(&window, [None, None, Some(1)]);
    window.invoke_apply();
    hydrus_store::api_activity::touch(store.dir(), due).unwrap();
    assert!(!bound.session_autosave.idle_at(due + 60_000));
    assert!(bound.session_autosave.idle_at(due + 60_001));
    let draft = open(&ui, &bound);
    set(&draft, [None, None, None]);
    draft.invoke_cancel();
    assert!(!bound.session_autosave.idle_at(due + 1));
    let window = open(&ui, &bound);
    set(&window, [None, None, None]);
    window.invoke_apply();
    assert!(bound.session_autosave.idle_at(due + 1));
    let disk = Store::open(dirs[1].path()).unwrap();
    let saved = disk.read(settings::get::<GuiIdleSettings>).unwrap();
    assert_eq!(
        (saved.user_seconds, saved.mouse_seconds, saved.api_seconds),
        (None, None, None)
    );
    let reopened = open(&ui, &bound);
    for field in 0..3 {
        let value = reopened
            .get_rows()
            .row_data(usize::try_from(row(&reopened, field)).unwrap())
            .unwrap();
        assert!(value.is_none);
        assert_eq!(value.number, 1);
    }
    let pixels = headless::render(&windows.get(windows.count() - 1).unwrap(), 1000, 850);
    headless::save_png(
        &std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("idle-timeout-options.png"),
        &pixels,
        1000,
        850,
    )
    .unwrap();
    reopened.invoke_cancel();
}

#[test]
fn unchanged_options_acceptance_normalises_raw_seconds_and_changes_the_existing_live_gate() {
    let (_dirs, store) = crate::subscriptions::store();
    store
        .write(|ctx| {
            settings::set(
                ctx.conn(),
                &GuiIdleSettings {
                    enabled: true,
                    user_seconds: Some(119),
                    mouse_seconds: None,
                    api_seconds: None,
                    busy_cpu_percent: 50,
                    busy_cpu_count: None,
                },
            )
        })
        .unwrap();
    let _headless_windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::single(SearchPage::new(store.clone())));
    let activity = bound.session_autosave.next().unwrap();
    bound.session_autosave.user_at(activity);
    assert!(!bound.session_autosave.idle_at(activity + 60_001));
    let cancelled = open(&ui, &bound);
    assert_eq!(
        cancelled
            .get_rows()
            .row_data(usize::try_from(row(&cancelled, 0)).unwrap())
            .unwrap()
            .number,
        1
    );
    cancelled.invoke_cancel();
    assert_eq!(
        store
            .read(settings::get::<GuiIdleSettings>)
            .unwrap()
            .user_seconds,
        Some(119)
    );
    assert!(!bound.session_autosave.idle_at(activity + 60_001));
    let accepted = open(&ui, &bound);
    accepted.invoke_apply();
    assert_eq!(
        store
            .read(settings::get::<GuiIdleSettings>)
            .unwrap()
            .user_seconds,
        Some(60)
    );
    assert!(!bound.session_autosave.idle_at(activity + 60_000));
    assert!(bound.session_autosave.idle_at(activity + 60_001));
    let reopened = open(&ui, &bound);
    assert_eq!(
        reopened
            .get_rows()
            .row_data(usize::try_from(row(&reopened, 0)).unwrap())
            .unwrap()
            .number,
        1
    );
    reopened.invoke_cancel();
}
