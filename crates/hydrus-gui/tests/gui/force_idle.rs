//! Actual debug menu, live idle consumers and binding-clone retirement.
use hydrus_gui::{MainWindow, Pages, SearchPage, bind, headless};
use hydrus_store::{
    Store,
    maintenance_gates::Worker,
    settings::{self, GuiIdleSettings, GuiSessionSettings},
};
use slint::{ComponentHandle as _, Model as _};

fn choose(ui: &MainWindow, pane: i32, label: &str) {
    let lines = ui.get_menu_panes().row_data(pane as usize).unwrap().lines;
    let index = lines.iter().position(|line| line.label == label).unwrap();
    assert!(lines.row_data(index).unwrap().usable);
    ui.invoke_menu_line_clicked(pane, index as i32, 0., 0., 0.);
}
fn open(ui: &MainWindow) -> usize {
    let help = ui
        .get_menu_titles()
        .iter()
        .position(|title| title.label == "help")
        .unwrap();
    ui.invoke_menu_title_pressed(help as i32, 20., 22.);
    choose(ui, 0, "debug");
    choose(ui, 1, "debug modes");
    let lines = ui.get_menu_panes().row_data(2).unwrap().lines;
    let i = lines
        .iter()
        .position(|line| line.label == "force idle mode")
        .unwrap();
    assert_eq!(lines.row_data(i).unwrap().kind, 1);
    i
}
fn toggle(ui: &MainWindow) {
    open(ui);
    choose(ui, 2, "force idle mode");
}
fn checked(ui: &MainWindow) -> bool {
    let i = open(ui);
    let checked = ui
        .get_menu_panes()
        .row_data(2)
        .unwrap()
        .lines
        .row_data(i)
        .unwrap()
        .checked;
    ui.invoke_menu_dismissed();
    checked
}
#[test]
fn real_menu_checked_states_controller_matrix_activity_and_autosave_consumer() {
    let directory = tempfile::tempdir().unwrap();
    let store = Store::open(directory.path()).unwrap();
    store
        .write(|ctx| {
            settings::set(
                ctx.conn(),
                &GuiSessionSettings {
                    autosave_minutes: 1,
                    only_during_idle: true,
                    ..Default::default()
                },
            )
        })
        .unwrap();
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::single(SearchPage::new(store.clone())));
    ui.show().unwrap();
    let fixture = hydrus_testkit::fixture_json("force_idle_mode.json");
    for event in fixture["toggles"].as_array().unwrap() {
        assert_eq!(checked(&ui), event["checked"].as_bool().unwrap());
        let deadlines = [
            bound.maintenance.deadline(Worker::Trash),
            bound.maintenance.deadline(Worker::Deferred),
        ];
        let autosave = bound.session_autosave.next();
        toggle(&ui);
        assert_eq!(
            bound.force_idle.enabled(),
            event["after"].as_bool().unwrap()
        );
        assert_eq!(checked(&ui), event["reopened"].as_bool().unwrap());
        assert_eq!(
            deadlines,
            [
                bound.maintenance.deadline(Worker::Trash),
                bound.maintenance.deadline(Worker::Deferred)
            ]
        );
        assert_eq!(
            autosave,
            bound.session_autosave.next(),
            "toggle never invents wake/reset for these timers"
        );
    }
    let start = bound.maintenance.started_ms();
    for case in fixture["idle_matrix"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|case| case["shutdown"] == false)
    {
        let enabled = case["enabled"].as_bool().unwrap();
        store
            .write(move |ctx| {
                settings::set(
                    ctx.conn(),
                    &GuiIdleSettings {
                        enabled,
                        user_seconds: Some(60),
                        mouse_seconds: Some(60),
                        api_seconds: Some(60),
                    },
                )
            })
            .unwrap();
        let now = start
            + if case["boot_recent"] == true {
                0
            } else {
                200_000
            };
        bound.session_autosave.user_at(now - 600_000);
        bound.session_autosave.mouse_at(now - 600_000);
        bound.session_autosave.api_at(now - 600_000);
        if bound.force_idle.enabled() != case["forced"].as_bool().unwrap() {
            toggle(&ui);
        }
        assert_eq!(
            bound.session_autosave.idle_at(now),
            case["idle"].as_bool().unwrap(),
            "{case}"
        );
    }
    store
        .write(|ctx| {
            settings::set(
                ctx.conn(),
                &GuiIdleSettings {
                    enabled: true,
                    user_seconds: Some(60),
                    mouse_seconds: Some(60),
                    api_seconds: Some(60),
                },
            )
        })
        .unwrap();
    if !bound.force_idle.enabled() {
        toggle(&ui);
    }
    let now = start + 200_000;
    // Real observations still update their original timestamps while forced.
    bound.session_autosave.user_at(now);
    bound.session_autosave.mouse_at(now);
    bound.session_autosave.api_at(now);
    assert!(bound.session_autosave.idle_at(now));
    toggle(&ui);
    assert!(!bound.session_autosave.idle_at(now));
    let due = bound.session_autosave.next().unwrap();
    bound.session_autosave.user_at(due);
    assert!(!bound.session_autosave.poll_at(due).unwrap());
    let next = bound.session_autosave.next().unwrap();
    toggle(&ui);
    assert!(
        bound.session_autosave.poll_at(next).unwrap(),
        "forced idle reaches the actual archive writer"
    );
    assert!(
        store
            .read(|conn| hydrus_store::session_backups::latest(
                conn,
                hydrus_store::sessions::LAST_SESSION
            ))
            .unwrap()
            .is_some()
    );
    open(&ui);
    let pixels = headless::render(&windows.get(0).unwrap(), 1100, 700);
    headless::save_png(
        &std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("force_idle_mode_menu.png"),
        &pixels,
        1100,
        700,
    )
    .unwrap();
    ui.invoke_menu_dismissed();
}
#[test]
fn forced_idle_admits_real_trash_and_physical_passes_at_unchanged_deadlines() {
    let (_dirs, store, files) = super::normal_time_maintenance::owned();
    store
        .write(|ctx| {
            settings::set(
                ctx.conn(),
                &GuiIdleSettings {
                    enabled: false,
                    ..Default::default()
                },
            )
        })
        .unwrap();
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::single(SearchPage::new(store.clone())));
    ui.show().unwrap();
    super::normal_time_maintenance::queue(&store, files[0].0);
    let trash = files[1].0;
    store
        .write_content(move |w| {
            w.delete_files(w.roles().combined_local_media, &[trash], None)?;
            w.inbox(&[trash])
        })
        .unwrap();
    store
        .write(|ctx| {
            settings::set(
                ctx.conn(),
                &hydrus_store::trash::TrashSettings {
                    max_age_hours: None,
                    max_size_mb: Some(0),
                },
            )
        })
        .unwrap();
    let before = [
        bound.maintenance.deadline(Worker::Trash),
        bound.maintenance.deadline(Worker::Deferred),
    ];
    toggle(&ui);
    assert_eq!(
        before,
        [
            bound.maintenance.deadline(Worker::Trash),
            bound.maintenance.deadline(Worker::Deferred)
        ]
    );
    bound
        .maintenance
        .poll_at(bound.maintenance.started_ms())
        .unwrap();
    assert!(
        !bound.maintenance.running(Worker::Trash) && !bound.maintenance.running(Worker::Deferred)
    );
    assert!(files[0].1.exists() && files[1].1.exists());
    let due = before[0].max(before[1]);
    bound.maintenance.poll_at(due).unwrap();
    super::normal_time_maintenance::wait(|| {
        bound.maintenance.poll_at(due).unwrap();
        let s = bound.maintenance.statistics();
        s.trash_passes == 1 && s.deferred_passes == 1
    });
    let stats = bound.maintenance.statistics();
    assert!(stats.trashed_files_removed >= 1 && stats.physical_files_removed >= 1);
    assert!(!files[0].1.exists());
    let next = bound.maintenance.deadline(Worker::Deferred);
    bound.maintenance.poll_at(next).unwrap();
    super::normal_time_maintenance::wait(|| {
        bound.maintenance.poll_at(next).unwrap();
        !files[1].1.exists()
    });
    assert!(files[2].1.exists());
    toggle(&ui);
    assert!(!bound.session_autosave.idle_at(next));
}
#[test]
fn hidden_prompt_cancel_rebind_close_and_final_binding_clone_retire_override() {
    let directory = tempfile::tempdir().unwrap();
    let store = Store::open(directory.path()).unwrap();
    store
        .write(|ctx| {
            settings::set(
                ctx.conn(),
                &GuiIdleSettings {
                    enabled: true,
                    user_seconds: None,
                    mouse_seconds: None,
                    api_seconds: None,
                },
            )?;
            let mut gui: settings::GuiSettings = settings::get(ctx.conn())?;
            gui.confirm_exit = true;
            settings::set(ctx.conn(), &gui)
        })
        .unwrap();
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::single(SearchPage::new(store.clone())));
    ui.show().unwrap();
    toggle(&ui);
    ui.hide().unwrap();
    assert!(!bound.force_idle.toggle());
    assert!(bound.force_idle.enabled());
    ui.show().unwrap();
    ui.set_tag_menu_question("owned tag-menu prompt".into());
    assert!(!bound.force_idle.toggle());
    assert!(bound.force_idle.enabled());
    ui.set_tag_menu_question("".into());
    ui.window()
        .dispatch_event(slint::platform::WindowEvent::CloseRequested);
    assert!(!ui.get_question().is_empty());
    assert!(!bound.force_idle.toggle());
    assert!(bound.force_idle.enabled());
    ui.invoke_answer(false);
    assert!(bound.force_idle.enabled());
    let old = bound.force_idle.clone();
    let old_monitor = bound.session_autosave.clone();
    let fresh = bind(&ui, Pages::single(SearchPage::new(store.clone())));
    assert!(!old.enabled() && !old.toggle());
    assert!(!old_monitor.idle_at(i64::MAX));
    assert!(!checked(&ui));
    toggle(&ui);
    ui.window()
        .dispatch_event(slint::platform::WindowEvent::CloseRequested);
    ui.invoke_answer(true);
    ui.show().unwrap();
    assert!(!fresh.force_idle.enabled() && !fresh.force_idle.toggle());
    assert!(!fresh.session_autosave.idle_at(i64::MAX));
    let live = bind(&ui, Pages::single(SearchPage::new(store.clone())));
    toggle(&ui);
    let retained = live.force_idle.clone();
    let monitor = live.session_autosave.clone();
    let clone = live.clone();
    drop(live);
    assert!(retained.enabled() && monitor.idle_at(i64::MAX));
    drop(clone);
    assert!(!retained.enabled() && !retained.toggle());
    assert!(
        !monitor.idle_at(i64::MAX),
        "final clone retirement wins even over otherwise eligible idle"
    );
    let successor = bind(&ui, Pages::single(SearchPage::new(store)));
    assert!(!successor.force_idle.enabled());
    toggle(&ui);
    assert!(successor.force_idle.enabled());
    ui.hide().unwrap();
    drop(ui);
    assert!(!successor.force_idle.enabled());
    assert!(!successor.session_autosave.idle_at(i64::MAX));
}
