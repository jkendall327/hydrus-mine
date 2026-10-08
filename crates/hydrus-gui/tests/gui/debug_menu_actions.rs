//! Help > debug's entries that need nothing but the store: each is clicked
//! in the real menu and its effect read back where the reference puts it
//! (`ClientGUI._DebugMake*`, `HydrusData.DebugPrint`,
//! `ClientController.ForceDatabaseCommit`, `HydrusEnvironment.DumpEnv`).
use hydrus_gui::{
    Bound, MainWindow, Pages, bind, debug_printed, exit_requested, headless, message_window,
};
use hydrus_gui_model::debug_actions as model;
use hydrus_store::{Store, popups, sessions};
use slint::{ComponentHandle as _, Model as _};
use std::{sync::Arc, time::Duration};

struct Debug {
    _dirs: [tempfile::TempDir; 2],
    store: Arc<Store>,
    ui: MainWindow,
    bound: Bound,
    _windows: headless::Windows,
}

fn start() -> Debug {
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
    /// Click `help > debug > ...path`.
    fn click(&self, path: &[&str]) {
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

    fn jobs(&self) -> Vec<popups::Job> {
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
    let d = start();
    d.click(&["profiling", "what is this?"]);
    let (title, text) = d.ok();
    assert_eq!(title, "Information");
    assert_eq!(text, model::PROFILE_MESSAGE);
    assert!(text.starts_with("If something is running slow, you can turn on a profile mode"));
}

// leaf: audit-options-help-debug-action-make-a-qmessagebox
#[test]
fn make_a_qmessagebox_shows_the_test_warning() {
    let d = start();
    d.click(&["gui actions", "make a QMessageBox"]);
    let (title, text) = d.ok();
    assert_eq!(title, "Warning");
    assert!(text.starts_with("This is a test message!\n\nI have a second line of information"));
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
        d.jobs().is_empty(),
        "nothing before the five seconds are up"
    );
    let started = std::time::Instant::now();
    while d.jobs().len() < 2 {
        assert!(
            started.elapsed() < Duration::from_secs(15),
            "modal popups arrive"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
    assert!(started.elapsed() >= Duration::from_secs(5));
    let jobs = d.jobs();
    assert!(
        jobs.iter()
            .all(|j| { j.status_title.as_deref() == Some(model::MODAL_TITLE) && !j.pausable })
    );
    assert_eq!(jobs.iter().filter(|j| j.cancellable).count(), 1);
    let text = jobs.iter().find_map(|j| j.status_text_1.clone()).unwrap();
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

// leaf: audit-options-help-debug-action-simulate-program-exit-signal
#[test]
fn simulate_program_exit_signal_stops_the_event_loop() {
    let d = start();
    assert!(!exit_requested());
    d.click(&["data actions", "simulate program exit signal"]);
    assert!(exit_requested());
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
    assert!(bound.rows.cached_bytes() > 0);
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
static ONE_AT_A_TIME: Mutex<()> = Mutex::new(());

/// Send reports to a list, not the popups.
fn capture() -> std::sync::Arc<Mutex<Vec<String>>> {
    let seen = std::sync::Arc::new(Mutex::new(Vec::new()));
    let sink = seen.clone();
    debug_flags::set_sink(Some(Box::new(move |text| {
        sink.lock().unwrap().push(text.to_owned());
    })));
    seen
}

impl Debug {
    /// Whether `help > debug > report modes > label` shows checked.
    fn report_mode_checked(&self, label: &str) -> bool {
        let help = self
            .ui
            .get_menu_titles()
            .iter()
            .position(|row| row.label == "help")
            .unwrap();
        self.ui.invoke_menu_title_pressed(help as i32, 20.0, 22.0);
        for (pane, name) in ["debug", "report modes"].iter().enumerate() {
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

// leaf: audit-options-help-debug-action-idle-report-mode
#[test]
fn idle_report_mode_says_why_the_client_is_not_idle() {
    let _one = ONE_AT_A_TIME
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let d = start();
    let seen = capture();
    let now = hydrus_core::TimestampMs::now().0;
    assert!(!d.report_mode_checked("idle report mode"));
    d.bound.session_autosave.idle_at(now);
    assert!(seen.lock().unwrap().is_empty(), "silent while off");
    d.click(&["report modes", "idle report mode"]);
    assert!(d.report_mode_checked("idle report mode"));
    d.bound.session_autosave.idle_at(now);
    assert_eq!(
        *seen.lock().unwrap(),
        ["IDLE MODE - Blocked: Program has not been on for 120s yet."]
    );
    seen.lock().unwrap().clear();
    d.click(&["debug modes", "force idle mode"]);
    d.bound.session_autosave.idle_at(now);
    assert_eq!(*seen.lock().unwrap(), ["IDLE MODE - Forced via debug menu"]);
    d.click(&["debug modes", "force idle mode"]);
    d.click(&["report modes", "idle report mode"]);
    assert!(!d.report_mode_checked("idle report mode"));
    seen.lock().unwrap().clear();
    d.bound.session_autosave.idle_at(now);
    assert!(seen.lock().unwrap().is_empty());
    debug_flags::set_sink(None);
}

// leaf: audit-options-help-debug-action-shortcut-report-mode
#[test]
fn shortcut_report_mode_says_what_a_shortcut_matched() {
    let _one = ONE_AT_A_TIME
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let d = start();
    let seen = capture();
    d.click(&["report modes", "shortcut report mode"]);
    assert!(d.report_mode_checked("shortcut report mode"));
    assert!(!d.ui.invoke_shortcut_key("j".into(), 0));
    // ctrl+t is "new page" in the main gui set
    assert!(d.ui.invoke_shortcut_key("t".into(), 1));
    let seen_now = seen.lock().unwrap().clone();
    assert_eq!(seen_now.len(), 2, "{seen_now:?}");
    assert_eq!(seen_now[0], "Shortcut \"j\" did not match any command.");
    assert!(
        seen_now[1].starts_with("Shortcut \"ctrl+t\" matched on \"main_gui\" set to "),
        "{}",
        seen_now[1]
    );
    d.click(&["report modes", "shortcut report mode"]);
    debug_flags::set_sink(None);
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
    let seen = capture();
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
    let mut said = seen.lock().unwrap().clone();
    said.sort();
    assert_eq!(
        said,
        [
            "deferred_physical_deletes doing a job.",
            "maintain_trash doing a job."
        ]
    );
    d.click(&["report modes", "daemon report mode"]);
    debug_flags::set_sink(None);
}
