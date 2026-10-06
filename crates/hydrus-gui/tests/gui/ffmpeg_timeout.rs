//! Real saved Options reaching an existing importer's local subprocess transport.
use hydrus_gui::{Bound, MainWindow, OptionsWindow, Pages, bind, headless};
use hydrus_store::{Store, ffmpeg_policy, settings};
use slint::{ComponentHandle as _, Model as _};
const LABEL: &str = "FFMPEG call timeout:";
fn options(ui: &MainWindow, bound: &Bound) -> (OptionsWindow, i32) {
    ui.invoke_menu_title_pressed(0, 20., 22.);
    let at = ui
        .get_menu_panes()
        .row_data(0)
        .unwrap()
        .lines
        .iter()
        .position(|r| r.label == "options…")
        .unwrap();
    ui.invoke_menu_line_clicked(0, at as i32, 0., 0., 0.);
    let w = bound.options.borrow().as_ref().unwrap().clone_strong();
    let page = w
        .get_pages()
        .iter()
        .position(|p| p.text == "media playback")
        .unwrap();
    w.invoke_page_chosen(page as i32);
    let row = w.get_rows().iter().position(|r| r.label == LABEL).unwrap();
    assert_eq!(w.get_rows().row_data(row).unwrap().kind, 2);
    (w, row as i32)
}
#[test]
fn real_options_cancel_hidden_retired_save_reopen_bounds_and_independent_field() {
    let fixture = hydrus_testkit::fixture_json("ffmpeg_timeout.json");
    let (_dirs, store) = super::subscriptions::store();
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(
        &ui,
        Pages::single(super::common::all_local_page(store.clone())),
    );
    let (cancel, row) = options(&ui, &bound);
    assert_eq!(
        store.read(ffmpeg_policy::load).unwrap().seconds,
        fixture["loaded"]
    );
    cancel.invoke_number_edited(row, 7);
    cancel.invoke_cancel();
    let (hidden, row) = options(&ui, &bound);
    cancel.invoke_number_edited(row, 1);
    cancel.invoke_apply();
    assert_eq!(store.read(ffmpeg_policy::load).unwrap().seconds, 15);
    hidden.hide().unwrap();
    hidden.invoke_number_edited(row, 1);
    hidden.invoke_apply();
    hidden.show().unwrap();
    hidden.invoke_apply();
    assert_eq!(
        store.read(ffmpeg_policy::load).unwrap().seconds,
        15,
        "hidden edits must not be staged for later Apply"
    );
    for (case_index, case) in fixture["cases"].as_array().unwrap().iter().enumerate() {
        let (w, row) = options(&ui, &bound);
        let before = store.read(ffmpeg_policy::load).unwrap();
        w.invoke_number_edited(row, case["requested"].as_i64().unwrap() as i32);
        assert_eq!(store.read(ffmpeg_policy::load).unwrap(), before);
        store
            .write(|c| {
                settings::set(
                    c.conn(),
                    &hydrus_store::image_colour::ImageColour {
                        normalise_icc: false,
                    },
                )
            })
            .unwrap();
        w.invoke_apply();
        assert_eq!(
            store.read(ffmpeg_policy::load).unwrap().seconds,
            case["saved"]
        );
        assert!(
            !store
                .read(hydrus_store::image_colour::load)
                .unwrap()
                .normalise_icc
        );
        let (reopened, row) = options(&ui, &bound);
        assert_eq!(
            reopened.get_rows().row_data(row as usize).unwrap().number,
            i32::try_from(case["reopened"].as_i64().unwrap()).unwrap()
        );
        // Capture the system group, including this persisted timeout and the ICC field.
        let pixels = headless::render(&windows.get(windows.count() - 1).unwrap(), 1200, 1800);
        headless::save_png(
            &std::path::Path::new(env!("CARGO_TARGET_TMPDIR"))
                .join(format!("ffmpeg-timeout-options-{case_index}-native.png")),
            &pixels,
            1200,
            1800,
        )
        .unwrap();
        reopened.invoke_cancel();
    }
    assert_eq!(
        Store::open(store.dir())
            .unwrap()
            .read(ffmpeg_policy::load)
            .unwrap()
            .seconds,
        600
    );
}

#[cfg(unix)]
#[test]
fn already_open_importer_captures_old_deadline_saved_apply_changes_next_call_and_no_store_cycle() {
    use hydrus_import::FileImporter;
    use hydrus_media::{Ffmpeg, MediaTools};
    use std::{
        io::Write as _,
        os::unix::fs::PermissionsExt as _,
        process::Command,
        sync::{Arc, mpsc},
        thread,
        time::{Duration, Instant},
    };
    fn quoted(path: &std::path::Path) -> String {
        format!("'{}'", path.to_string_lossy().replace('\'', "'\\''"))
    }
    let (_dirs, store) = super::subscriptions::store();
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(
        &ui,
        Pages::single(super::common::all_local_page(store.clone())),
    );
    let (w, row) = options(&ui, &bound);
    w.invoke_number_edited(row, 6);
    w.invoke_apply();
    let transport = tempfile::tempdir().unwrap();
    let fifo = transport.path().join("release");
    let marker = transport.path().join("pid");
    let exe = transport.path().join("ffmpeg");
    assert!(
        Command::new("mkfifo")
            .arg(&fifo)
            .status()
            .unwrap()
            .success()
    );
    std::fs::write(&exe,format!("#!/bin/sh\nprintf '%s' \"$$\" > {}\nread -r reply < {}\nprintf 'ffmpeg version owned-local Copyright transport\\n'\n",quoted(&marker),quoted(&fifo))).unwrap();
    std::fs::set_permissions(&exe, std::fs::Permissions::from_mode(0o755)).unwrap();
    let count = Arc::strong_count(&store);
    let importer = FileImporter::new(
        store.clone(),
        MediaTools::with_ffmpeg(Ffmpeg::with_executable(&exe)),
    );
    assert_eq!(
        Arc::strong_count(&store),
        count + 1,
        "only FileImporter owns Store; timeout and ICC providers are weak"
    );
    let tools = importer.tools().clone();
    let worker_tools = tools.clone();
    let (sender, receiver) = mpsc::channel();
    let worker = thread::spawn(move || sender.send(worker_tools.ffmpeg().version()).unwrap());
    let limit = Instant::now() + Duration::from_secs(3);
    while std::fs::read_to_string(&marker).is_err() {
        assert!(Instant::now() < limit);
        thread::sleep(Duration::from_millis(1));
    }
    let (w, row) = options(&ui, &bound);
    w.invoke_number_edited(row, 1);
    w.invoke_apply();
    assert_eq!(store.read(ffmpeg_policy::load).unwrap().seconds, 1);
    assert!(
        matches!(
            receiver.recv_timeout(Duration::from_millis(3200)),
            Err(mpsc::RecvTimeoutError::Timeout)
        ),
        "old six-second process must survive the newly saved three-second effective limit"
    );
    writeln!(
        std::fs::OpenOptions::new().write(true).open(&fifo).unwrap(),
        "complete"
    )
    .unwrap();
    assert_eq!(
        receiver
            .recv_timeout(Duration::from_secs(2))
            .unwrap()
            .unwrap(),
        Some("owned-local".into())
    );
    worker.join().unwrap();
    let before = Instant::now();
    assert!(
        tools
            .ffmpeg()
            .version()
            .unwrap_err()
            .to_string()
            .contains("ffmpeg took too long to respond")
    );
    assert!(before.elapsed() >= Duration::from_secs(3));
    assert!(before.elapsed() < Duration::from_secs(5));
    let pid = std::fs::read_to_string(&marker).unwrap();
    assert!(
        !Command::new("kill")
            .args(["-0", pid.trim()])
            .stderr(std::process::Stdio::null())
            .status()
            .unwrap()
            .success(),
        "timed out child is reaped before returning"
    );
    drop(importer);
    assert_eq!(Arc::strong_count(&store), count);
    let isolated_dir = tempfile::tempdir().unwrap();
    let isolated = Store::open(isolated_dir.path()).unwrap();
    let weak = Arc::downgrade(&isolated);
    let importer = FileImporter::new(isolated.clone(), MediaTools::new());
    let retained = importer.tools().clone();
    drop(importer);
    drop(isolated);
    assert!(
        weak.upgrade().is_none(),
        "retained tools cannot keep a retired Store alive"
    );
    drop(retained);
}
