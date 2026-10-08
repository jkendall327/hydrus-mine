//! The Database menu's idle/normal-time check items: "work file jobs during
//! idle time", "... during normal time" (file maintenance) and "work deferred
//! delete jobs during idle time", "... during normal time" (db maintenance).
//! Like the reference's `CheckboxManagerOptions.Invert`, choosing one flips
//! its saved option, the menu shows the new state at once and again after the
//! store is reopened, and the other three are left alone.
//!
//! hydrus-rs has no background file-maintenance loop in the GUI (only the
//! server's) and no deferred-delete queue, so the saved option is all the GUI
//! changes (DIFFERENCES.md).

use super::database_menu_jobs::{basic, from_menu_path};
use hydrus_gui::{MainWindow, Pages, SearchPage, bind, headless};
use hydrus_store::Store;
use hydrus_store::file_maintenance::FileMaintenanceSettings;
use hydrus_store::settings::{self, BackgroundWork};
use slint::{ComponentHandle as _, Model as _};

const LABELS: [(&str, &str); 4] = [
    ("file maintenance", "work file jobs during idle time"),
    ("file maintenance", "work file jobs during normal time"),
    (
        "db maintenance",
        "work deferred delete jobs during idle time",
    ),
    (
        "db maintenance",
        "work deferred delete jobs during normal time",
    ),
];

/// The four flags, in `LABELS`' order.
fn flags(store: &Store) -> [bool; 4] {
    let files: FileMaintenanceSettings = store.read(settings::get).unwrap();
    let work: BackgroundWork = store.read(settings::get).unwrap();
    [
        files.during_idle,
        files.during_active,
        work.deferred_delete_during_idle,
        work.deferred_delete_during_active,
    ]
}

/// The four items' checkmarks as the menu shows them.
fn shown(ui: &MainWindow) -> [bool; 4] {
    let mut out = [false; 4];
    for (i, (submenu, label)) in LABELS.iter().enumerate() {
        let database = ui
            .get_menu_titles()
            .iter()
            .position(|t| t.label == "database")
            .unwrap();
        ui.invoke_menu_title_pressed(i32::try_from(database).unwrap(), 20.0, 22.0);
        let pane = ui.get_menu_panes().row_data(0).unwrap();
        let at = pane.lines.iter().position(|l| l.label == *submenu).unwrap();
        ui.invoke_menu_line_clicked(0, i32::try_from(at).unwrap(), 0.0, 0.0, 0.0);
        let line = ui
            .get_menu_panes()
            .row_data(1)
            .unwrap()
            .lines
            .iter()
            .find(|l| l.label == *label)
            .unwrap();
        assert_eq!(line.kind, 1, "{label} is a check item");
        assert!(line.usable);
        out[i] = line.checked;
        ui.invoke_menu_dismissed();
    }
    out
}

fn toggle(ui: &MainWindow, which: usize) {
    let (submenu, label) = LABELS[which];
    from_menu_path(ui, &[submenu], label);
}

fn flips_only_its_own_option(which: usize) {
    let (dir, store) = basic();
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let _bound = bind(&ui, Pages::single(SearchPage::new(store.clone())));
    // (the reference's defaults: all four on)
    assert_eq!(flags(&store), [true; 4]);
    assert_eq!(shown(&ui), [true; 4]);

    toggle(&ui, which);
    let mut expected = [true; 4];
    expected[which] = false;
    assert_eq!(flags(&store), expected);
    assert_eq!(shown(&ui), expected);
    // kept across a restart
    let reopened = Store::open(dir.path()).unwrap();
    assert_eq!(flags(&reopened), expected);

    toggle(&ui, which);
    assert_eq!(flags(&store), [true; 4]);
    assert_eq!(shown(&ui), [true; 4]);
}

// leaf: audit-media-menu-database-work-file-jobs-during-idle-time
#[test]
fn work_file_jobs_during_idle_time_flips_its_option() {
    flips_only_its_own_option(0);
}

// leaf: audit-media-menu-database-work-file-jobs-during-normal-time
#[test]
fn work_file_jobs_during_normal_time_flips_its_option() {
    flips_only_its_own_option(1);
}

// (not tagged: the native store has no deferred-delete table, so the switch
// changes nothing; see DIFFERENCES.md, Database menu)
#[test]
fn work_deferred_delete_jobs_during_idle_time_flips_its_option() {
    flips_only_its_own_option(2);
}

// (not tagged: the native store has no deferred-delete table, so the switch
// changes nothing; see DIFFERENCES.md, Database menu)
#[test]
fn work_deferred_delete_jobs_during_normal_time_flips_its_option() {
    flips_only_its_own_option(3);
}
