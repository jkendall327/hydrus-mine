//! Pages > sessions > append backup > {session} > {timestamp}, chosen from
//! the menu bar, replayed against the reference's
//! (`oracle/fixtures/session_backups.json`, from
//! `oracle/record_session_backups.py`): a session saved six times at the
//! recorded times keeping two backups, its backups' entries, and the one
//! recorded appended as a notebook named after the session holding that
//! backup's pages.

use std::sync::Arc;

use hydrus_core::pages::{Page, PageContent, PageKey, Session};
use hydrus_gui::{MainWindow, Pages, bind, headless};
use hydrus_store::Store;
use hydrus_store::import::import_legacy;
use hydrus_store::session_backups::{self, SessionBackupSettings};
use slint::ComponentHandle as _;

use crate::common::main_menu;

// leaf: audit-options-session-backups-timestamp
#[test]
fn a_backup_chosen_from_the_menu_is_appended_as_the_reference_appends_it() {
    let recorded = hydrus_testkit::fixture_json("session_backups.json");
    let legacy = hydrus_testkit::legacy_fixture("basic");
    let native = tempfile::tempdir().unwrap();
    import_legacy(
        legacy.path(),
        &native.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store: Arc<Store> = Store::open(native.path()).unwrap();
    let keep = usize::try_from(recorded["keep"].as_u64().unwrap()).unwrap();
    let steps = recorded["steps"].as_array().unwrap().clone();
    store
        .write(move |ctx| {
            let conn = ctx.conn();
            hydrus_store::settings::set(conn, &SessionBackupSettings { keep })?;
            // each save a top notebook holding a notebook "version {i}"
            for step in &steps {
                let session = Session {
                    name: "backup test".into(),
                    pages: vec![Page {
                        key: PageKey::random(),
                        name: format!("version {}", step["version"]),
                        content: PageContent::Pages(Vec::new()),
                    }],
                };
                session_backups::save(conn, &session, step["now"].as_i64().unwrap())?;
            }
            Ok(())
        })
        .unwrap();
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    let before = bound.pages.borrow().session().pages.len();

    // the session's backups, as the reference lists them (local time)
    let path = ["pages", "sessions", "append backup", "backup test"];
    let tz = jiff::tz::TimeZone::system();
    let label = |ms: i64| hydrus_gui_model::session_saving::backup_timestamp(ms, &tz);
    let backups: Vec<String> = recorded["steps"].as_array().unwrap().last().unwrap()["backups"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| label(t.as_i64().unwrap()))
        .collect();
    let lines: Vec<String> = main_menu::open(&ui, &path)
        .into_iter()
        .map(|l| l.0)
        .collect();
    assert_eq!(lines, backups);

    // the recorded one chosen: appended at the top level
    let timestamp = recorded["timestamp"].as_i64().unwrap();
    main_menu::choose(&ui, &path, &label(timestamp));
    let pages = bound.pages.borrow();
    let session = pages.session();
    assert_eq!(session.pages.len(), before + 1);
    let appended = session.pages.last().unwrap();
    let PageContent::Pages(children) = &appended.content else {
        panic!("a notebook appended");
    };
    let children: Vec<&str> = children.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(
        serde_json::json!({ "name": appended.name, "children": children }),
        recorded["appended"]
    );
}
