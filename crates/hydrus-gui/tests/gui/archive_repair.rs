//! Native Database dispatch, exact questions, real timestamp consumers and owners.
use hydrus_core::{HashId, Sha256};
use hydrus_gui::{ArchiveRepairWindow, MainWindow, Pages, archive_repair_window, bind, headless};
use hydrus_store::Store;
use slint::{ComponentHandle as _, Model as _};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    time::{Duration, Instant},
};

fn seed(store: &Store) -> Vec<HashId> {
    let recorded = hydrus_testkit::fixture_json("archive_time_repair.json");
    store.write_content(move |writer| {
        let roles=writer.roles().clone();
        let mut ids=Vec::new();
        for i in 0..2 {
            let hash:Sha256=recorded["corpus"]["hashes"][i].as_str().unwrap().parse().unwrap();
            let id=hydrus_store::master::intern_hash(writer.conn(),&hash)?;
            ids.push(id);
            for sid in [roles.local_file_storage,roles.combined_local_media] {
                writer.conn().execute("INSERT INTO file_domain_current(service_id,hash_id,added_ms) VALUES(?1,?2,?3)",rusqlite::params![sid,id,recorded["corpus"]["imports"][i].as_i64().unwrap()])?;
            }
        }
        Ok(ids)
    }).unwrap()
}
fn times(store: &Store, ids: &[HashId]) -> Vec<Option<i64>> {
    store
        .read(|conn| {
            Ok(
                hydrus_store::media::load(conn, &store.snapshot().services, None, ids)?
                    .results
                    .iter()
                    .map(|m| m.archived.map(hydrus_core::TimestampMs::millis))
                    .collect(),
            )
        })
        .unwrap()
}
fn wait_phase(window: &ArchiveRepairWindow, phase: i32) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while window.get_phase() != phase && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
        slint::platform::update_timers_and_animations();
    }
    assert_eq!(window.get_phase(), phase, "{}", window.get_status());
}
#[test]
fn database_repair_has_owned_scan_population_questions_and_updates_real_media_on_reopen() {
    let windows = headless::init();
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let ids = seed(&store);
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    let database = ui
        .get_menu_titles()
        .iter()
        .position(|r| r.label == "database")
        .unwrap();
    ui.invoke_menu_title_pressed(i32::try_from(database).unwrap(), 10.0, 22.0);
    let pane = ui.get_menu_panes().row_data(0).unwrap();
    let maintenance = pane
        .lines
        .iter()
        .position(|r| r.label == "file maintenance")
        .unwrap();
    ui.invoke_menu_line_clicked(0, i32::try_from(maintenance).unwrap(), 0.0, 0.0, 0.0);
    let pane = ui.get_menu_panes().row_data(1).unwrap();
    let repair = pane
        .lines
        .iter()
        .position(|r| r.label.starts_with("fix missing file archived times"))
        .unwrap();
    assert!(pane.lines.row_data(repair).unwrap().usable);
    ui.invoke_menu_line_clicked(1, i32::try_from(repair).unwrap(), 0.0, 0.0, 0.0);
    let first = bound
        .archive_repair
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    assert_eq!(
        first.get_question(),
        hydrus_gui_model::archive_repair::SCAN_QUESTION
    );
    first.invoke_scan_answered(false);
    assert!(bound.archive_repair.borrow().is_none());
    assert_eq!(times(&store, &ids), vec![None, None]);

    let changed = Rc::new(Cell::new(0));
    let slot = Rc::new(RefCell::new(None));
    let open = || {
        archive_repair_window::open(
            &store,
            &slot,
            {
                let changed = changed.clone();
                Rc::new(move || changed.set(changed.get() + 1))
            },
            {
                let weak = ui.as_weak();
                Rc::new(move || weak.upgrade().is_some_and(|w| w.window().is_visible()))
            },
        )
        .unwrap()
    };
    let cancelled = open();
    cancelled.invoke_scan_answered(true);
    cancelled.invoke_cancel_work();
    assert_eq!(cancelled.get_status(), "Cancelled!");
    cancelled.invoke_close_clicked();
    let declined = open();
    declined.invoke_scan_answered(true);
    wait_phase(&declined, 2);
    assert_eq!(
        declined
            .get_choices()
            .iter()
            .map(|s| s.to_string())
            .collect::<Vec<_>>(),
        ["do legacy times", "do import times", "do both"]
    );
    assert!(
        declined
            .get_question()
            .contains("--1 Missing Legacy Times--")
    );
    assert!(
        declined
            .get_question()
            .contains("--1 Missing Import Times--")
    );
    declined.invoke_close_clicked();
    declined.invoke_chosen(2);
    assert_eq!(times(&store, &ids), vec![None, None]);
    let retired = open();
    let accepted_index = windows.count();
    let accepted = open();
    assert_eq!(windows.count(), accepted_index + 1);
    retired.invoke_close_clicked();
    retired.invoke_scan_answered(true);
    assert!(slot.borrow().is_some());
    assert_eq!(accepted.get_phase(), 0);
    accepted.invoke_scan_answered(true);
    wait_phase(&accepted, 2);
    assert!(accepted.window().is_visible());
    assert_eq!(
        accepted
            .get_choices()
            .iter()
            .map(|s| s.to_string())
            .collect::<Vec<_>>(),
        ["do legacy times", "do import times", "do both"]
    );
    assert_eq!(times(&store, &ids), vec![None, None]);
    let pixels = headless::render(&windows.get(accepted_index).unwrap(), 680, 480);
    // The native theme may be monochrome. Require visible paint in the lower
    // choice-control area rather than a pixel with unequal RGB channels.
    assert!(
        pixels
            .chunks_exact(680 * 4)
            .skip(320)
            .take(148)
            .flat_map(|row| row[12 * 4..668 * 4].chunks_exact(4))
            .any(|pixel| pixel != &pixels[..4]),
        "the visible population-choice controls paint against the window background"
    );
    headless::save_png(
        &std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("archive_time_repair.png"),
        &pixels,
        680,
        480,
    )
    .unwrap();
    ui.hide().unwrap();
    accepted.invoke_chosen(2); // immediate owner gate, without waiting for timer
    assert_eq!(times(&store, &ids), vec![None, None]);
    assert!(slot.borrow().is_none());
    ui.show().unwrap();
    accepted.invoke_chosen(2);
    assert_eq!(times(&store, &ids), vec![None, None]);
    let accepted = open();
    accepted.invoke_scan_answered(true);
    wait_phase(&accepted, 2);
    accepted.invoke_chosen(2);
    wait_phase(&accepted, 3);
    assert_eq!(changed.get(), 1);
    assert!(accepted.get_status().starts_with("Done!"));
    let recorded = hydrus_testkit::fixture_json("archive_time_repair.json");
    assert_eq!(
        times(&store, &ids),
        vec![
            recorded["reopened_archived"][0].as_i64(),
            recorded["reopened_archived"][1].as_i64()
        ]
    );
    accepted.invoke_close_clicked();
    let reopened = open();
    reopened.invoke_scan_answered(true);
    wait_phase(&reopened, 3);
    assert_eq!(reopened.get_status(), "No missing archive times found!");
    reopened.invoke_close_clicked();
    assert_eq!(
        store
            .read(
                |conn| Ok(conn.query_row("SELECT count(*) FROM file_inbox", [], |r| r
                    .get::<_, i64>(0))?)
            )
            .unwrap(),
        0
    );
    ui.hide().unwrap();
}

fn menu_repair(ui: &MainWindow, bound: &hydrus_gui::Bound) -> ArchiveRepairWindow {
    let database = ui
        .get_menu_titles()
        .iter()
        .position(|r| r.label == "database")
        .unwrap();
    ui.invoke_menu_title_pressed(i32::try_from(database).unwrap(), 10.0, 22.0);
    let maintenance = ui
        .get_menu_panes()
        .row_data(0)
        .unwrap()
        .lines
        .iter()
        .position(|r| r.label == "file maintenance")
        .unwrap();
    ui.invoke_menu_line_clicked(0, i32::try_from(maintenance).unwrap(), 0.0, 0.0, 0.0);
    let repair = ui
        .get_menu_panes()
        .row_data(1)
        .unwrap()
        .lines
        .iter()
        .position(|r| r.label.starts_with("fix missing file archived times"))
        .unwrap();
    ui.invoke_menu_line_clicked(1, i32::try_from(repair).unwrap(), 0.0, 0.0, 0.0);
    bound
        .archive_repair
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong()
}

type Controls = Rc<RefCell<std::collections::BTreeMap<i32, hydrus_gui::MenuChoiceFrame>>>;
fn observe(window: &ArchiveRepairWindow) -> Controls {
    let controls = Controls::default();
    window.set_measure_controls(true);
    window.on_control_measured({
        let controls = controls.clone();
        move |id, frame| {
            controls.borrow_mut().insert(id, frame);
        }
    });
    controls
}
fn settled(
    native: &slint::platform::software_renderer::MinimalSoftwareWindow,
    width: u32,
    height: u32,
) -> Vec<u8> {
    let started = Instant::now();
    let mut previous = None;
    loop {
        let pixels = headless::render(native, width, height);
        if started.elapsed() >= Duration::from_millis(40) && previous.as_ref() == Some(&pixels) {
            return pixels;
        }
        assert!(
            started.elapsed() < Duration::from_secs(3),
            "archive repair render did not settle"
        );
        previous = Some(pixels);
        std::thread::sleep(Duration::from_millis(5));
    }
}
fn capture(
    native: &slint::platform::software_renderer::MinimalSoftwareWindow,
    window: &ArchiveRepairWindow,
    name: &str,
    width: u32,
    height: u32,
) {
    use slint::platform::WindowAdapter as _;
    assert!(std::ptr::eq(native.window(), window.window()));
    assert!(window.window().is_visible());
    let pixels = settled(native, width, height);
    let viewport = window.get_question_viewport();
    assert!(viewport.w > 0.0 && viewport.h > 0.0);
    assert!(
        (window.get_question_content_width() - viewport.w).abs() < 1.0,
        "wrapped question must use the actual viewport, not its unwrapped preferred width"
    );
    headless::save_png(
        &std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join(name),
        &pixels,
        width,
        height,
    )
    .unwrap();
}
fn click(
    native: &slint::platform::software_renderer::MinimalSoftwareWindow,
    controls: &Controls,
    id: i32,
) {
    use slint::platform::{PointerEventButton, WindowAdapter as _, WindowEvent};
    let frame = controls.borrow().get(&id).unwrap().clone();
    assert!(
        [frame.x, frame.y, frame.w, frame.h]
            .into_iter()
            .all(f32::is_finite)
    );
    assert!(frame.x >= 0.0 && frame.y >= 0.0 && frame.w > 0.0 && frame.h > 0.0);
    let size = native.window().size();
    assert!(
        frame.x + frame.w <= size.width as f32 + 1.0
            && frame.y + frame.h <= size.height as f32 + 1.0
    );
    let position = slint::LogicalPosition::new(frame.x + frame.w / 2.0, frame.y + frame.h / 2.0);
    native.dispatch_event(WindowEvent::PointerMoved { position });
    native.dispatch_event(WindowEvent::PointerPressed {
        position,
        button: PointerEventButton::Left,
    });
    native.dispatch_event(WindowEvent::PointerReleased {
        position,
        button: PointerEventButton::Left,
    });
}

#[test]
fn actual_archive_controls_wrap_scroll_and_apply_each_recorded_population() {
    use slint::platform::WindowEvent;
    let windows = headless::init();
    let fixture = hydrus_testkit::fixture_json("archive_time_repair.json");
    for choice in 0..3 {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path()).unwrap();
        let ids = seed(&store);
        let ui = MainWindow::new().unwrap();
        ui.show().unwrap();
        let bound = bind(&ui, Pages::open(store.clone()).unwrap());
        let declined = menu_repair(&ui, &bound);
        let controls = observe(&declined);
        let native = windows.get(windows.count() - 1).unwrap();
        capture(
            &native,
            &declined,
            &format!("archive-repair-warning-{choice}.png"),
            680,
            480,
        );
        assert_eq!(
            declined.get_question(),
            fixture["events"][0]["asked"][0]["message"]
                .as_str()
                .unwrap()
        );
        click(&native, &controls, 1);
        assert!(bound.archive_repair.borrow().is_none());
        assert_eq!(times(&store, &ids), vec![None, None]);

        let window = menu_repair(&ui, &bound);
        let controls = observe(&window);
        let native = windows.get(windows.count() - 1).unwrap();
        settled(&native, 680, 480);
        click(&native, &controls, 0);
        wait_phase(&window, 2);
        assert_eq!(
            window
                .get_choices()
                .iter()
                .map(|s| s.to_string())
                .collect::<Vec<_>>(),
            ["do legacy times", "do import times", "do both"]
        );
        let plan = store
            .read(|conn| {
                hydrus_store::archive_repair::scan(
                    conn,
                    &hydrus_store::content::DomainRoles::new(&store.snapshot().services)?,
                    &std::sync::atomic::AtomicBool::new(false),
                )
            })
            .unwrap();
        assert_eq!(
            window.get_question(),
            hydrus_gui_model::archive_repair::question(&plan)
        );
        capture(
            &native,
            &window,
            &format!("archive-repair-choices-{choice}.png"),
            680,
            480,
        );
        if choice == 0 {
            capture(
                &native,
                &window,
                "archive-repair-choices-narrow-top.png",
                440,
                480,
            );
            let viewport = window.get_question_viewport();
            assert!(window.get_question_content_height() > viewport.h);
            let before = window.get_question_offset();
            native.dispatch_event(WindowEvent::PointerScrolled {
                position: slint::LogicalPosition::new(
                    viewport.x + viewport.w / 2.0,
                    viewport.y + viewport.h / 2.0,
                ),
                delta_x: 0.0,
                delta_y: -180.0,
            });
            capture(
                &native,
                &window,
                "archive-repair-choices-narrow-scrolled.png",
                440,
                480,
            );
            assert!(window.get_question_offset() < before);
            assert_eq!(times(&store, &ids), vec![None, None]);
            capture(
                &native,
                &window,
                "archive-repair-choices-wide.png",
                840,
                640,
            );
        }
        settled(&native, 680, 480);
        click(&native, &controls, choice + 2);
        wait_phase(&window, 3);
        let expected = vec![
            if choice == 1 {
                None
            } else {
                fixture["reopened_archived"][0].as_i64()
            },
            if choice == 0 {
                None
            } else {
                fixture["reopened_archived"][1].as_i64()
            },
        ];
        assert_eq!(times(&store, &ids), expected);
        assert!(window.get_status().starts_with("Done!"));
        capture(
            &native,
            &window,
            &format!("archive-repair-done-{choice}.png"),
            680,
            480,
        );
        click(&native, &controls, 7);
        assert!(bound.archive_repair.borrow().is_none());
        assert_eq!(times(&store, &ids), expected);
        if choice == 2 {
            let reopened = menu_repair(&ui, &bound);
            let controls = observe(&reopened);
            let native = windows.get(windows.count() - 1).unwrap();
            settled(&native, 680, 480);
            click(&native, &controls, 0);
            wait_phase(&reopened, 3);
            assert_eq!(reopened.get_status(), "No missing archive times found!");
            capture(
                &native,
                &reopened,
                "archive-repair-no-missing.png",
                680,
                480,
            );
            click(&native, &controls, 7);
        }
        ui.hide().unwrap();
    }
}

#[test]
fn cancel_after_commit_reports_success_and_refreshes_while_cancel_before_write_rolls_back() {
    let _windows = headless::init();
    for committed in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path()).unwrap();
        let ids = seed(&store);
        let changed = Rc::new(Cell::new(0));
        let slot = archive_repair_window::Slot::default();
        let window = archive_repair_window::open(
            &store,
            &slot,
            {
                let changed = changed.clone();
                Rc::new(move || changed.set(changed.get() + 1))
            },
            Rc::new(|| true),
        )
        .unwrap();
        window.invoke_scan_answered(true);
        wait_phase(&window, 2);
        let (entered_send, entered_recv) = std::sync::mpsc::channel();
        let (release_send, release_recv) = std::sync::mpsc::channel();
        let blocker = if committed {
            None
        } else {
            let store = store.clone();
            let thread = std::thread::spawn(move || {
                store
                    .write_content(move |_| {
                        entered_send.send(()).unwrap();
                        release_recv.recv_timeout(Duration::from_secs(10)).unwrap();
                        Ok(())
                    })
                    .unwrap();
            });
            entered_recv.recv_timeout(Duration::from_secs(10)).unwrap();
            Some(thread)
        };
        window.invoke_chosen(2);
        if committed {
            let started = Instant::now();
            // Deliberately do not pump Slint: the commit is durable, but its
            // Completed message has not yet been consumed by the UI timer.
            while times(&store, &ids).iter().any(Option::is_none) {
                assert!(started.elapsed() < Duration::from_secs(10));
                std::thread::sleep(Duration::from_millis(5));
            }
        }
        assert_eq!(window.get_phase(), 1);
        assert_eq!(changed.get(), 0);
        window.invoke_cancel_work();
        assert_eq!(
            window.get_phase(),
            1,
            "cancellation must wait for the worker's authoritative result"
        );
        if let Some(blocker) = blocker {
            release_send.send(()).unwrap();
            blocker.join().unwrap();
        }
        wait_phase(&window, 3);
        assert_eq!(changed.get(), i32::from(committed));
        if committed {
            assert!(window.get_status().starts_with("Done!"));
            let fixture = hydrus_testkit::fixture_json("archive_time_repair.json");
            assert_eq!(
                times(&store, &ids),
                vec![
                    fixture["reopened_archived"][0].as_i64(),
                    fixture["reopened_archived"][1].as_i64()
                ]
            );
        } else {
            assert_eq!(window.get_status(), "Cancelled!");
            assert_eq!(times(&store, &ids), vec![None, None]);
        }
        window.invoke_close_clicked();
        assert!(slot.borrow().is_none());
    }
}
