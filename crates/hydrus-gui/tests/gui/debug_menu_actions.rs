//! Help > debug's entries that need nothing but the store: each is clicked
//! in the real menu and its effect read back where the reference puts it
//! (`ClientGUI._DebugMake*`, `HydrusData.DebugPrint`,
//! `ClientController.ForceDatabaseCommit`, `HydrusEnvironment.DumpEnv`).
use hydrus_gui::{Bound, MainWindow, Pages, bind, debug_printed, headless, message_window};
use hydrus_gui_model::debug_actions as model;
use hydrus_store::{Store, popups, sessions};
use slint::{ComponentHandle as _, Model as _};
use std::{sync::Arc, time::Duration};

pub(super) struct Debug {
    _dirs: [tempfile::TempDir; 2],
    pub(super) store: Arc<Store>,
    pub(super) ui: MainWindow,
    pub(super) bound: Bound,
    _windows: headless::Windows,
}

pub(super) fn start() -> Debug {
    let (dirs, store) = super::subscriptions::store();
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    Debug {
        _dirs: dirs,
        store,
        ui,
        bound,
        _windows: windows,
    }
}

impl Debug {
    /// A harness over a window the test made itself.
    pub(super) fn from_parts(
        dirs: [tempfile::TempDir; 2],
        store: Arc<Store>,
        ui: MainWindow,
        bound: Bound,
        windows: headless::Windows,
    ) -> Debug {
        Debug {
            _dirs: dirs,
            store,
            ui,
            bound,
            _windows: windows,
        }
    }

    /// Click `help > debug > ...path`.
    pub(super) fn click(&self, path: &[&str]) {
        let help = self
            .ui
            .get_menu_titles()
            .iter()
            .position(|row| row.label == "help")
            .unwrap();
        self.ui.invoke_menu_title_pressed(help as i32, 20.0, 22.0);
        for (pane, label) in ["debug"].iter().chain(path).enumerate() {
            let rows = self.ui.get_menu_panes().row_data(pane).unwrap().lines;
            let row = rows
                .iter()
                .position(|row| row.label == *label)
                .unwrap_or_else(|| panic!("no menu entry {label:?}"));
            assert!(rows.row_data(row).unwrap().usable, "{label:?} is usable");
            self.ui
                .invoke_menu_line_clicked(pane as i32, row as i32, 0.0, 0.0, 0.0);
        }
    }

    pub(super) fn jobs(&self) -> Vec<popups::Job> {
        self.store
            .read(|conn| popups::all(conn, hydrus_core::TimestampMs::now().0 / 1000))
            .unwrap()
    }

    /// Dismiss the message shown; its title and text.
    #[allow(clippy::unused_self)]
    fn ok(&self) -> (String, String) {
        let window = message_window().expect("a message is shown");
        assert!(window.window().is_visible());
        assert_eq!(window.get_no_label(), "ok");
        let said = (
            window.get_window_title().to_string(),
            window.get_message().to_string(),
        );
        window.invoke_cancelled();
        said
    }
}

// leaf: audit-options-help-debug-action-what-is-this
#[test]
fn profiling_what_is_this_shows_the_profile_information() {
    // the reference's message, recorded by oracle/record_debug_message_boxes.py
    // from its real Help menu's QAction
    let recorded = hydrus_testkit::fixture_json("debug_message_boxes.json");
    let shown = recorded["what is this?"].as_array().unwrap();
    assert_eq!(shown.len(), 1);
    let d = start();
    d.click(&["profiling", "what is this?"]);
    let (title, text) = d.ok();
    assert_eq!(title, shown[0]["title"].as_str().unwrap());
    assert_eq!(text, shown[0]["text"].as_str().unwrap());
}

// leaf: audit-options-help-debug-action-make-a-qmessagebox
#[test]
fn make_a_qmessagebox_shows_the_test_warning() {
    let recorded = hydrus_testkit::fixture_json("debug_message_boxes.json");
    let shown = recorded["make a QMessageBox"].as_array().unwrap();
    assert_eq!(shown.len(), 1);
    let d = start();
    d.click(&["gui actions", "make a QMessageBox"]);
    let (title, text) = d.ok();
    assert_eq!(title, shown[0]["title"].as_str().unwrap());
    assert_eq!(text, shown[0]["text"].as_str().unwrap());
}

// leaf: audit-options-help-debug-action-make-some-popups
#[test]
fn make_some_popups_throws_the_reference_s_varied_popups_then_three_delayed_ones() {
    let d = start();
    d.click(&["gui actions", "make some popups"]);
    let at_once = d.jobs();
    // six numbered, the long one, the long title, the file one, two that
    // merge into one, the unicode one, the gauge job and the exception
    let texts: Vec<_> = at_once
        .iter()
        .map(|j| j.status_text_1.clone().unwrap_or_default())
        .collect();
    for i in 1..=6 {
        assert!(texts.contains(&format!("This is a test popup message -- {i}")));
    }
    assert!(
        texts
            .iter()
            .any(|t| t.starts_with("This is a very long message:"))
    );
    assert!(at_once.iter().any(|j| j.had_error
        && j.status_title.as_deref() == Some("DataMissing")
        && j.traceback.is_some()));
    assert!(at_once.iter().any(|j| j.popup_gauge_1 == Some((4, 8))));
    let before = at_once.len();
    // three more arrive half a second apart
    let started = std::time::Instant::now();
    while d.jobs().len() < before + 3 {
        assert!(
            started.elapsed() < Duration::from_secs(10),
            "delayed popups arrive"
        );
        slint::platform::update_timers_and_animations();
        std::thread::sleep(Duration::from_millis(10));
    }
    let texts: Vec<_> = d
        .jobs()
        .iter()
        .filter_map(|j| j.status_text_1.clone())
        .collect();
    for i in 1..4 {
        assert!(texts.contains(&format!("This is a delayed popup message -- {i}")));
    }
}

// leaf: audit-options-help-debug-action-make-a-modal-popup-in-five-seconds
// leaf: audit-options-help-debug-action-make-a-non-cancellable-modal-popup-in-five-seconds
#[test]
fn modal_popups_arrive_after_five_seconds_counting_down_and_only_one_can_be_cancelled() {
    let d = start();
    d.click(&["gui actions", "make a modal popup in five seconds"]);
    d.click(&[
        "gui actions",
        "make a non-cancellable modal popup in five seconds",
    ]);
    assert!(
        d.jobs().is_empty() && d.bound.popup_modal.dialog().is_none(),
        "nothing before the five seconds are up"
    );
    let started = std::time::Instant::now();
    // the cancellable one is in a dialog of its own; the other, done from
    // the start as the reference's is, goes to the popups
    while d.bound.popup_modal.dialog().is_none() || d.jobs().is_empty() {
        assert!(
            started.elapsed() < Duration::from_secs(15),
            "modal popups arrive"
        );
        slint::platform::update_timers_and_animations();
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(started.elapsed() >= Duration::from_secs(5));
    let released = d.jobs();
    assert_eq!(released.len(), 1);
    assert_eq!(
        released[0].status_title.as_deref(),
        Some(model::MODAL_TITLE)
    );
    assert!(!released[0].cancellable && !released[0].pausable);
    let dialog = d.bound.popup_modal.dialog().unwrap();
    assert_eq!(dialog.get_window_title(), model::MODAL_TITLE);
    assert!(!dialog.get_hide_close_button(), "it can be cancelled");
    let held = d
        .store
        .read(|c| popups::held(c, hydrus_core::TimestampMs::now().0 / 1000))
        .unwrap();
    assert_eq!(held.len(), 1);
    assert!(held[0].cancellable && !held[0].pausable);
    let text = held[0].status_text_1.clone().unwrap();
    assert!(text.starts_with("Will auto-dismiss in "), "{text}");
}

// leaf: audit-options-help-debug-action-reset-multi-column-list-settings-to-default
#[test]
fn reset_multi_column_list_settings_asks_the_reference_s_question() {
    let d = start();
    d.click(&["gui actions", "reset multi-column list settings to default"]);
    assert_eq!(d.ui.get_question(), model::RESET_COLUMNS_QUESTION);
    d.ui.invoke_answer(false);
    assert_eq!(d.ui.get_question(), "");
}

// leaf: audit-options-help-debug-action-save-last-session-gui-session
#[test]
fn save_last_session_writes_the_current_pages_now() {
    let d = start();
    d.store
        .write(|ctx| sessions::delete(ctx.conn(), sessions::LAST_SESSION))
        .unwrap();
    assert!(
        d.store
            .read(|c| sessions::load(c, sessions::LAST_SESSION))
            .unwrap()
            .is_none()
    );
    d.click(&["gui actions", "save 'last session' gui session"]);
    let saved = d
        .store
        .read(|c| sessions::load(c, sessions::LAST_SESSION))
        .unwrap()
        .expect("the last session is saved at once");
    assert!(!saved.pages.is_empty());
}

// leaf: audit-options-help-debug-action-flush-log
#[test]
fn flush_log_prints_flushing_log() {
    let d = start();
    d.click(&["data actions", "flush log"]);
    assert!(debug_printed().contains(&"Flushing log".to_owned()));
}

// leaf: audit-options-help-debug-action-force-database-commit
#[test]
fn force_database_commit_puts_everything_in_the_database_file() {
    let d = start();
    d.store
        .write(|ctx| {
            ctx.conn()
                .execute_batch("CREATE TABLE IF NOT EXISTS commit_probe (x); INSERT INTO commit_probe VALUES (42);")
                .map_err(Into::into)
        })
        .unwrap();
    d.click(&["data actions", "force database commit"]);
    // a copy of the main file alone, without its write-ahead log, has the row
    let copy = tempfile::tempdir().unwrap();
    let main = d.store.dir().join(hydrus_store::store::DB_FILE_NAME);
    let target = copy.path().join("copy.db");
    std::fs::copy(&main, &target).unwrap();
    let conn = rusqlite::Connection::open(&target).unwrap();
    let x: i64 = conn
        .query_row("SELECT x FROM commit_probe", [], |r| r.get(0))
        .unwrap();
    assert_eq!(x, 42);
}

// leaf: audit-options-help-debug-action-show-env
#[test]
fn show_env_prints_and_pops_up_the_environment() {
    let d = start();
    d.click(&["data actions", "show env"]);
    let jobs = d.jobs();
    let text = jobs
        .iter()
        .filter_map(|j| j.status_text_1.clone())
        .find(|t| t.starts_with("Full environment:\n"))
        .expect("the environment popup");
    assert!(text.starts_with("Full environment:\n"));
    let (name, value) = std::env::vars()
        .find(|(k, v)| !k.contains("PATH") && !k.contains("DIRS") && !v.contains('\n'))
        .expect("some plain variable");
    assert!(text.contains(&format!("\n{name}: {value}")));
    assert!(text.contains("\nPATH:\n    "), "PATH is one entry per line");
    assert!(debug_printed().contains(&text));
}

// leaf: audit-options-help-debug-action-clear-all-rendering-caches
#[test]
fn clear_all_rendering_caches_empties_the_thumbnail_and_image_caches() {
    let (dirs, store) = super::subscriptions::store();
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(
        &ui,
        Pages::single(super::common::all_local_page(store.clone())),
    );
    ui.show().unwrap();
    ui.invoke_search_edited("system:everything".into());
    ui.invoke_search_accepted();
    for i in 0..bound.rows.row_count().min(5) {
        bound.rows.row_data(i).unwrap();
    }
    bound.rows.wait();
    // open an image (its decoded pixels go in the image cache), then close
    // the viewer: both caches are full before the action
    let image_files = (0..bound.rows.row_count().min(5)).find_map(|i| {
        ui.invoke_thumbnail_activated(i32::try_from(i).unwrap());
        let viewer = bound
            .viewer
            .borrow()
            .as_ref()
            .map(slint::ComponentHandle::clone_strong)?;
        let started = std::time::Instant::now();
        while bound.image_cache.keys().is_empty() && started.elapsed().as_secs() < 5 {
            slint::platform::update_timers_and_animations();
            std::thread::sleep(Duration::from_millis(10));
        }
        let warmed = !bound.image_cache.keys().is_empty();
        viewer.invoke_close_requested();
        warmed.then_some(i)
    });
    assert!(
        image_files.is_some(),
        "an image is opened into the image cache"
    );
    assert!(bound.rows.cached_bytes() > 0);
    assert!(!bound.image_cache.keys().is_empty());
    let d = Debug {
        _dirs: dirs,
        store,
        ui,
        bound,
        _windows: windows,
    };
    d.click(&["memory actions", "clear all rendering caches"]);
    assert_eq!(d.bound.rows.cached_bytes(), 0);
    assert!(d.bound.image_cache.keys().is_empty());
    assert_eq!(d.bound.image_cache.bytes(), 0);
}

// leaf: audit-options-help-debug-action-review-current-network-jobs
#[test]
fn review_current_network_jobs_opens_the_jobs_review() {
    let d = start();
    assert!(hydrus_gui::network_data_window::last_jobs().is_none());
    d.click(&["network actions", "review current network jobs"]);
    assert!(hydrus_gui::network_data_window::last_jobs().is_some());
}

// --- the report modes ---

use hydrus_core::debug_flags;
use std::sync::Mutex;

/// The flags and the sink are process-wide: one of these tests at a time.
pub(crate) static ONE_AT_A_TIME: Mutex<()> = Mutex::new(());

/// Send reports to a list, not the popups.
pub(super) fn capture() -> std::sync::Arc<Mutex<Vec<String>>> {
    let seen = std::sync::Arc::new(Mutex::new(Vec::new()));
    let sink = seen.clone();
    debug_flags::set_sink(Some(Box::new(move |text| {
        sink.lock().unwrap().push(text.to_owned());
    })));
    seen
}

/// Send the reports made on this thread to a list, not the popups. For a
/// report that is made on the calling thread: the flags and the sink are
/// process-wide, so other tests' timers can report too (on their own
/// threads), and those are left out.
pub(super) fn capture_here() -> std::sync::Arc<Mutex<Vec<String>>> {
    let seen = std::sync::Arc::new(Mutex::new(Vec::new()));
    let sink = seen.clone();
    let mine = std::thread::current().id();
    debug_flags::set_sink(Some(Box::new(move |text| {
        if std::thread::current().id() == mine {
            sink.lock().unwrap().push(text.to_owned());
        }
    })));
    seen
}

impl Debug {
    /// Whether `help > debug > report modes > label` shows checked.
    pub(super) fn report_mode_checked(&self, label: &str) -> bool {
        self.checked_in("report modes", label)
    }

    /// Whether `help > debug > submenu > label` shows checked.
    pub(super) fn checked_in(&self, submenu: &str, label: &str) -> bool {
        let help = self
            .ui
            .get_menu_titles()
            .iter()
            .position(|row| row.label == "help")
            .unwrap();
        self.ui.invoke_menu_title_pressed(help as i32, 20.0, 22.0);
        for (pane, name) in ["debug", submenu].iter().enumerate() {
            let rows = self.ui.get_menu_panes().row_data(pane).unwrap().lines;
            let row = rows.iter().position(|row| row.label == *name).unwrap();
            self.ui
                .invoke_menu_line_clicked(pane as i32, row as i32, 0.0, 0.0, 0.0);
        }
        let rows = self.ui.get_menu_panes().row_data(2).unwrap().lines;
        let row = rows.iter().position(|row| row.label == label).unwrap();
        let line = rows.row_data(row).unwrap();
        assert_eq!(line.kind, 1, "a check item");
        self.ui.invoke_menu_dismissed();
        line.checked
    }
}

/// Puts the network and idle report flags and the sink back when dropped.
pub(super) struct Reset;
impl Drop for Reset {
    fn drop(&mut self) {
        debug_flags::Flag::NetworkReport.set(false);
        debug_flags::Flag::NetworkReportSilent.set(false);
        debug_flags::Flag::IdleReport.set(false);
        debug_flags::set_sink(None);
        debug_flags::set_console(None);
    }
}

// leaf: audit-options-help-debug-action-network-report-mode-silent
#[test]
fn network_report_mode_entries_share_the_mode_as_the_reference_does() {
    let _one = ONE_AT_A_TIME
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    // the flags are process-wide: put them back even if an assertion fails
    let _reset = Reset;
    let d = start();
    let seen = capture();
    let console = std::sync::Arc::new(Mutex::new(Vec::<String>::new()));
    let printed = console.clone();
    debug_flags::set_console(Some(Box::new(move |text| {
        printed.lock().unwrap().push(text.to_owned());
    })));
    let plain = "network report mode";
    let silent = "network report mode (silent)";
    let state = |d: &Debug| (d.report_mode_checked(plain), d.report_mode_checked(silent));
    let report = || debug_flags::report_network(|| "a network job".to_owned());
    assert_eq!(state(&d), (false, false));

    // the silent entry turns the mode on and sets silent: nothing reaches
    // the popup sink, the console gets it
    d.click(&["report modes", silent]);
    assert_eq!(state(&d), (true, true));
    report();
    assert!(seen.lock().unwrap().is_empty(), "silent keeps popups out");
    assert_eq!(*console.lock().unwrap(), ["a network job"]);
    console.lock().unwrap().clear();

    // the plain entry flips the mode (off) and clears silent
    d.click(&["report modes", plain]);
    assert_eq!(state(&d), (false, false));
    report();
    assert!(seen.lock().unwrap().is_empty(), "the mode is off");

    // and on again, loud
    d.click(&["report modes", plain]);
    assert_eq!(state(&d), (true, false));
    report();
    assert_eq!(*seen.lock().unwrap(), ["a network job"]);
    assert!(
        console.lock().unwrap().is_empty(),
        "loud keeps to the popups"
    );

    // silent after plain flips the mode back off, as the reference's does,
    // yet leaves silent set
    d.click(&["report modes", silent]);
    assert_eq!(state(&d), (false, true));
    seen.lock().unwrap().clear();
    report();
    assert!(seen.lock().unwrap().is_empty());

    // the plain entry turns the mode on (clearing silent), and again off
    d.click(&["report modes", plain]);
    assert_eq!(state(&d), (true, false));
    d.click(&["report modes", plain]);
    assert_eq!(state(&d), (false, false));
}

// leaf: audit-options-help-debug-action-daemon-report-mode
#[test]
fn daemon_report_mode_says_when_a_maintenance_daemon_does_a_job() {
    use hydrus_gui::SearchPage;
    use hydrus_store::maintenance_gates::Worker;
    use hydrus_store::settings::{self, GuiIdleSettings};

    let _one = ONE_AT_A_TIME
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let seen = capture_here();
    let (dirs, store, files) = super::normal_time_maintenance::owned();
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(
        &ui,
        Pages::single(SearchPage::fixed(
            store.clone(),
            "maintenance fixture",
            None,
            files.iter().map(|(id, _)| *id).collect(),
        )),
    );
    ui.show().unwrap();
    super::normal_time_maintenance::queue(&store, files[0].0);
    let first = bound.maintenance.started_ms() + 30_000;
    bound.session_autosave.user_at(first);
    bound.maintenance.poll_at(first).unwrap();
    store
        .write(|ctx| {
            settings::set(
                ctx.conn(),
                &GuiIdleSettings {
                    enabled: true,
                    user_seconds: None,
                    mouse_seconds: None,
                    api_seconds: None,
                    busy_cpu_percent: 50,
                    busy_cpu_count: None,
                },
            )
        })
        .unwrap();
    let d = Debug {
        _dirs: dirs,
        store,
        ui,
        bound,
        _windows: windows,
    };
    assert!(!d.report_mode_checked("daemon report mode"));
    d.click(&["report modes", "daemon report mode"]);
    assert!(d.report_mode_checked("daemon report mode"));
    let at = d
        .bound
        .maintenance
        .deadline(Worker::Trash)
        .max(d.bound.maintenance.deadline(Worker::Deferred));
    d.bound.maintenance.poll_at(at).unwrap();
    super::normal_time_maintenance::wait(|| {
        d.bound.maintenance.poll_at(at).unwrap();
        let stats = d.bound.maintenance.statistics();
        stats.trash_passes == 1 && stats.deferred_passes == 1
    });
    // the reference's line for a daemon firing (`DAEMON._DoPreCall`), which it
    // names for its pool thread: `oracle/record_debug_report_modes.py`
    let recording = hydrus_testkit::fixture_json("debug_report_modes.json");
    let recorded = recording["daemon"]["on"][0]["text"].as_str().unwrap();
    assert_eq!(recording["daemon"]["off"], serde_json::json!([]));
    let line = |name: &str| recorded.replace("CallToThread", name);
    let mut said = seen.lock().unwrap().clone();
    said.sort();
    assert_eq!(
        said,
        [line("deferred_physical_deletes"), line("maintain_trash")]
    );
    // the reference reports every firing, before the job looks at idle: a
    // pass the gates turn away (normal-time maintenance off, the user at the
    // keyboard) is reported too, and does no work
    let done = d.bound.maintenance.statistics();
    d.store
        .write(|ctx| {
            settings::set(
                ctx.conn(),
                &hydrus_store::maintenance_gates::Preferences {
                    trash_normal: false,
                    deferred_normal: false,
                },
            )?;
            settings::set(
                ctx.conn(),
                &GuiIdleSettings {
                    enabled: true,
                    user_seconds: Some(3600),
                    mouse_seconds: None,
                    api_seconds: None,
                    busy_cpu_percent: 50,
                    busy_cpu_count: None,
                },
            )
        })
        .unwrap();
    let later = d
        .bound
        .maintenance
        .deadline(Worker::Trash)
        .max(d.bound.maintenance.deadline(Worker::Deferred));
    d.bound.session_autosave.user_at(later);
    seen.lock().unwrap().clear();
    d.bound.maintenance.poll_at(later).unwrap();
    let mut gated = seen.lock().unwrap().clone();
    gated.sort();
    assert_eq!(gated, said);
    let after = d.bound.maintenance.statistics();
    assert_eq!(
        (after.trash_passes, after.deferred_passes),
        (done.trash_passes, done.deferred_passes)
    );
    d.click(&["report modes", "daemon report mode"]);
    seen.lock().unwrap().clear();
    d.bound
        .maintenance
        .poll_at(later + 2 * 24 * 3600 * 1000)
        .unwrap();
    assert!(seen.lock().unwrap().is_empty(), "off: nothing");
    debug_flags::set_sink(None);
}

// (not a leaf tag: the reference's item is a toggle that is off at boot; the owner
// chose always-on crash logging, 2026-10-14, a deliberate divergence in DIFFERENCES.md)
// The panic hook is process-wide, so another test's panic while this one runs
// can land in this log too; the assertions only look for this test's own text.
#[test]
fn crash_logging_is_always_on_and_the_entry_says_where_the_log_is() {
    let _one = ONE_AT_A_TIME
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let d = start();
    hydrus_core::crash_log::install(d.store.dir(), "hydrus-gui-test");
    // (on from the start, unlike the reference, where it is off at boot)
    assert!(d.checked_in("debug modes", "use faulthandler to log crashes"));
    assert!(hydrus_gui::crash_logging());
    let _ = std::thread::Builder::new()
        .name("a-worker".to_owned())
        .spawn(|| panic!("a test panic with no toggle"))
        .unwrap()
        .join();
    let log = d.store.dir().join(hydrus_core::crash_log::LOG_NAME);
    let text = std::fs::read_to_string(&log).unwrap();
    assert!(text.contains("a test panic with no toggle"), "{text}");
    assert!(text.contains("thread: a-worker"), "{text}");
    // clicking it doesn't turn logging off: it says where the log is
    d.click(&["debug modes", "use faulthandler to log crashes"]);
    assert!(d.checked_in("debug modes", "use faulthandler to log crashes"));
    let said = d
        .jobs()
        .iter()
        .filter_map(|j| j.status_text_1.clone())
        .find(|t| t.starts_with("Crashes are always logged here."))
        .expect("the popup");
    assert!(said.contains(&log.display().to_string()), "{said}");
    hydrus_core::crash_log::uninstall();
}

// leaf: audit-options-help-debug-action-scan-file-storage-folders
#[test]
fn scan_file_storage_folders_asks_for_a_directory_and_says_what_it_found() {
    let d = start();
    let dir = tempfile::tempdir().unwrap();
    let granularity = d.store.snapshot().storage.granularity();
    assert!(granularity == 2 || granularity == 3);
    for prefix in ["fa0", "fa1", "fa2"] {
        let mut path = dir.path().join(prefix);
        if granularity == 3 {
            path = path.join(format!("{prefix}0"));
        }
        std::fs::create_dir_all(path).unwrap();
    }
    let asked = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    hydrus_gui::set_picker({
        let asked = asked.clone();
        let path = dir.path().to_owned();
        move |kind, title| {
            asked.borrow_mut().push((kind, title.to_owned()));
            vec![path.clone()]
        }
    });
    d.click(&["data actions", "scan file storage folders"]);
    assert_eq!(
        *asked.borrow(),
        [(hydrus_gui::Pick::Folder, "Select directory".to_owned())]
    );
    let text = d
        .jobs()
        .iter()
        .filter_map(|j| j.status_text_1.clone())
        .find(|t| t.starts_with("It found "))
        .expect("the result popup");
    assert!(
        text.starts_with("It found 3 subfolders and took "),
        "{text}"
    );
    // cancelling the picker does nothing more
    hydrus_gui::set_picker(|_, _| Vec::new());
    let before = d.jobs().len();
    d.click(&["data actions", "scan file storage folders"]);
    assert_eq!(d.jobs().len(), before);
}
