//! Database > backup, driven from the real menu: the entries follow the saved
//! location, "set up a database backup location" says what a backup is, asks
//! for a directory and what is in it, keeps the answer and offers a backup;
//! "update database backup" asks, backs up with a popup and remembers when;
//! "restore from a database backup" asks and requests the restore. The
//! sentences are those of `ClientGUI._SetupBackupPath`, `_BackupDatabase` and
//! `RestoreDatabase`.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::{Duration, Instant};

use super::database_menu_jobs::{basic, pump, shown};
use hydrus_gui::{Bound, MainWindow, Pages, SearchPage, bind, headless, message_window};
use hydrus_gui_model::database_backup as model;
use hydrus_store::Store;
use hydrus_store::backup::BackupSettings;
use slint::{ComponentHandle as _, Model as _};

struct Rig {
    _dir: tempfile::TempDir,
    store: std::sync::Arc<Store>,
    ui: MainWindow,
    _bound: Bound,
    _windows: headless::Windows,
    /// What the folder picker answers next, and what it was asked.
    next: Rc<RefCell<Vec<PathBuf>>>,
    asked: Rc<RefCell<Vec<String>>>,
}

fn rig() -> Rig {
    let (dir, store) = basic();
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(&ui, Pages::single(SearchPage::new(store.clone())));
    let next: Rc<RefCell<Vec<PathBuf>>> = Rc::default();
    let asked: Rc<RefCell<Vec<String>>> = Rc::default();
    {
        let (next, asked) = (next.clone(), asked.clone());
        hydrus_gui::set_picker(move |_, title| {
            asked.borrow_mut().push(title.to_owned());
            std::mem::take(&mut *next.borrow_mut())
        });
    }
    Rig {
        _dir: dir,
        store,
        ui,
        _bound: bound,
        _windows: windows,
        next,
        asked,
    }
}

impl Rig {
    fn click(&self, label: &str) {
        super::database_menu_jobs::from_menu_path(&self.ui, &["backup"], label);
    }
    /// The backup submenu's labels, as the menu shows them now.
    fn backup_entries(&self) -> Vec<String> {
        let database = self
            .ui
            .get_menu_titles()
            .iter()
            .position(|t| t.label == "database")
            .unwrap();
        self.ui
            .invoke_menu_title_pressed(i32::try_from(database).unwrap(), 20.0, 22.0);
        let pane = self.ui.get_menu_panes().row_data(0).unwrap();
        let index = pane.lines.iter().position(|l| l.label == "backup").unwrap();
        self.ui
            .invoke_menu_line_clicked(0, i32::try_from(index).unwrap(), 0.0, 0.0, 0.0);
        let labels = self
            .ui
            .get_menu_panes()
            .row_data(1)
            .unwrap()
            .lines
            .iter()
            .map(|l| l.label.to_string())
            .collect();
        self.ui.invoke_menu_dismissed();
        labels
    }
    fn settings(&self) -> BackupSettings {
        self.store.read(hydrus_store::settings::get).unwrap()
    }
    /// Dismiss the message with "ok"; its text.
    #[allow(clippy::unused_self)]
    fn ok(&self) -> String {
        let window = message_window().expect("a message is shown");
        assert!(window.window().is_visible());
        assert_eq!(window.get_no_label(), "ok");
        let text = window.get_message().to_string();
        window.invoke_cancelled();
        assert!(!window.window().is_visible());
        text
    }
    /// The question now asked of the main window, answered.
    fn answer(&self, yes: bool) -> String {
        let question = self.ui.get_question().to_string();
        assert!(!question.is_empty(), "a question is asked");
        self.ui.invoke_answer(yes);
        if !yes {
            assert_eq!(self.ui.get_question(), "");
        }
        question
    }
}

fn wait_popup(store: &Store, text: &str) {
    let deadline = Instant::now() + Duration::from_secs(20);
    while !shown(store).iter().any(|(title, t)| {
        title.as_deref() == Some(model::POPUP_TITLE) && t.as_deref() == Some(text)
    }) {
        assert!(Instant::now() < deadline, "the backup never said {text:?}");
        pump(Duration::from_millis(20));
    }
}

// leaf: audit-media-database-backup-path
#[test]
fn setting_up_a_backup_location_asks_validates_and_keeps_the_directory() {
    let rig = rig();
    assert_eq!(
        rig.backup_entries(),
        [
            "set up a database backup location\u{2026}",
            "",
            "restore from a database backup\u{2026}"
        ]
    );
    let dest = tempfile::tempdir().unwrap();
    let dest_text = dest.path().to_string_lossy().into_owned();

    // cancelling the directory picker changes nothing
    rig.click("set up a database backup location\u{2026}");
    assert_eq!(rig.ok(), model::intro(None));
    assert_eq!(rig.asked.borrow().as_slice(), [model::PICK_TITLE]);
    assert_eq!(rig.ui.get_question(), "");
    assert_eq!(rig.settings().path, None);

    // the database's own directory is refused
    // (the picker answers when asked, after the message)
    *rig.next.borrow_mut() = vec![rig.store.dir().to_path_buf()];
    rig.click("set up a database backup location\u{2026}");
    rig.ok();
    assert_eq!(
        message_window().unwrap().get_message(),
        model::SAME_AS_DATABASE
    );
    message_window().unwrap().invoke_cancelled();
    assert_eq!(rig.settings().path, None);

    // an empty directory: asked about, refused (nothing kept), then kept
    *rig.next.borrow_mut() = vec![dest.path().to_path_buf()];
    rig.click("set up a database backup location\u{2026}");
    rig.ok();
    assert_eq!(
        rig.answer(false),
        model::chosen_question(&dest_text, model::Chosen::Empty)
    );
    assert_eq!(rig.settings().path, None);
    *rig.next.borrow_mut() = vec![dest.path().to_path_buf()];
    rig.click("set up a database backup location\u{2026}");
    rig.ok();
    rig.answer(true);
    assert_eq!(rig.settings().path.as_deref(), Some(dest_text.as_str()));
    assert_eq!(rig.settings().last_backup, None);
    // ... and is offered a backup now, which can be declined
    assert_eq!(rig.answer(false), model::CREATE_NOW);
    assert!(std::fs::read_dir(dest.path()).unwrap().next().is_none());

    // choosing the same directory again says nothing changed
    *rig.next.borrow_mut() = vec![dest.path().to_path_buf()];
    rig.click("change database backup location\u{2026}");
    assert_eq!(rig.ok(), model::intro(Some(&dest_text)));
    assert_eq!(message_window().unwrap().get_message(), model::UNCHANGED);
    message_window().unwrap().invoke_cancelled();

    // a directory holding something else is described as such; one that does
    // not exist yet, as to be made
    let busy = tempfile::tempdir().unwrap();
    std::fs::write(busy.path().join("notes.txt"), b"x").unwrap();
    *rig.next.borrow_mut() = vec![busy.path().to_path_buf()];
    rig.click("change database backup location\u{2026}");
    rig.ok();
    assert_eq!(
        rig.answer(false),
        model::chosen_question(&busy.path().to_string_lossy(), model::Chosen::HasFiles)
    );
    let missing = dest.path().join("not yet");
    *rig.next.borrow_mut() = vec![missing.clone()];
    rig.click("change database backup location\u{2026}");
    rig.ok();
    assert_eq!(
        rig.answer(false),
        model::chosen_question(&missing.to_string_lossy(), model::Chosen::Missing)
    );
    assert_eq!(rig.settings().path.as_deref(), Some(dest_text.as_str()));
}

// leaf: audit-media-database-backup-update
#[test]
fn updating_the_backup_asks_backs_up_with_a_popup_and_the_menu_remembers() {
    let rig = rig();
    let dest = tempfile::tempdir().unwrap();
    let dest_text = dest.path().to_string_lossy().into_owned();
    rig.store
        .write({
            let path = dest_text.clone();
            move |ctx| {
                hydrus_store::settings::set(
                    ctx.conn(),
                    &BackupSettings {
                        path: Some(path),
                        last_backup: None,
                    },
                )
            }
        })
        .unwrap();
    // a location is set: update and change, then restore
    assert_eq!(
        rig.backup_entries(),
        [
            "update database backup\u{2026}",
            "change database backup location\u{2026}",
            "",
            "restore from a database backup\u{2026}"
        ]
    );

    // declining writes nothing
    rig.click("update database backup\u{2026}");
    assert_eq!(rig.answer(false), model::update_question(&dest_text, false));
    pump(Duration::from_millis(200));
    assert!(std::fs::read_dir(dest.path()).unwrap().next().is_none());
    assert_eq!(rig.settings().last_backup, None);

    // accepting makes it, with the popup, and remembers when
    rig.click("update database backup\u{2026}");
    rig.answer(true);
    wait_popup(&rig.store, model::COMPLETE);
    assert!(
        dest.path()
            .join(hydrus_store::store::DB_FILE_NAME)
            .is_file()
    );
    let deadline = Instant::now() + Duration::from_secs(10);
    while rig.settings().last_backup.is_none() {
        assert!(Instant::now() < deadline);
        pump(Duration::from_millis(20));
    }
    assert_eq!(
        rig.backup_entries()[0],
        "update database backup (did one recently)\u{2026}"
    );

    // the next one updates the existing backup
    rig.click("update database backup (did one recently)\u{2026}");
    assert_eq!(rig.answer(false), model::update_question(&dest_text, true));

    // a location that has gone is made, with a word about it
    let gone = dest.path().join("gone");
    let gone_text = gone.to_string_lossy().into_owned();
    rig.store
        .write(move |ctx| {
            hydrus_store::settings::set(
                ctx.conn(),
                &BackupSettings {
                    path: Some(gone_text),
                    last_backup: None,
                },
            )
        })
        .unwrap();
    rig.click("update database backup\u{2026}");
    assert_eq!(rig.ok(), model::CREATING_PATH);
    assert!(gone.is_dir());
    assert_eq!(
        rig.answer(false),
        model::update_question(&gone.to_string_lossy(), false)
    );

    // media stored in several locations hides the entries, leaving the note
    let elsewhere = tempfile::tempdir().unwrap();
    let path = elsewhere.path().to_path_buf();
    rig.store
        .write_and_refresh(move |ctx| {
            hydrus_store::storage_locations::add_location(ctx.conn(), &path)
        })
        .unwrap();
    assert_eq!(
        rig.backup_entries(),
        ["database is stored in multiple locations"]
    );
    rig.click("database is stored in multiple locations");
    let deadline = Instant::now() + Duration::from_secs(10);
    while !shown(&rig.store)
        .iter()
        .any(|(_, t)| t.as_deref() == Some(model::MULTIPLE_LOCATIONS))
    {
        assert!(Instant::now() < deadline, "the note was never shown");
        pump(Duration::from_millis(20));
    }
}

// leaf: audit-media-menu-database-restore-from-a-database-backup
#[test]
fn restoring_picks_a_backup_asks_and_requests_the_restore() {
    let rig = rig();
    let backup = tempfile::tempdir().unwrap();
    let backup_text = backup.path().to_string_lossy().into_owned();
    // a backup to restore from
    let mut say = |_: String| {};
    hydrus_store::backup::backup(&rig.store, backup.path(), &mut say, &|| false).unwrap();

    // cancelling the picker asks nothing
    rig.click("restore from a database backup\u{2026}");
    assert_eq!(rig.asked.borrow().as_slice(), [model::PICK_TITLE]);
    assert_eq!(rig.ui.get_question(), "");

    // declining leaves no request
    *rig.next.borrow_mut() = vec![backup.path().to_path_buf()];
    rig.click("restore from a database backup\u{2026}");
    assert_eq!(rig.answer(false), model::restore_question(&backup_text));
    assert!(
        hydrus_store::backup::restore_request(rig.store.dir())
            .unwrap()
            .is_none()
    );

    // accepting asks the next start to restore it
    *rig.next.borrow_mut() = vec![backup.path().to_path_buf()];
    rig.click("restore from a database backup\u{2026}");
    rig.answer(true);
    let (from, _media) = hydrus_store::backup::restore_request(rig.store.dir())
        .unwrap()
        .expect("the restore was requested");
    assert_eq!(from, backup.path());
}
