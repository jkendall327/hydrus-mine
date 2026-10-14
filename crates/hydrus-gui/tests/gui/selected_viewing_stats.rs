//! The thumbnail menu's manage > viewing stats > clear, as the reference's
//! `ClientGUIMediaMenus.AddManageFileViewingStatsMenu` and
//! `ClientGUIMediaModalActions.DoClearFileViewingStats`: the reference's
//! yes/no question ("Are you sure?", yes/no) worded for the selection, then a
//! delete of only the selected files' viewing records.

use std::sync::Arc;

use hydrus_core::HashId;
use hydrus_gui::{MainWindow, Pages, SearchPage, bind, headless, thumbnail_maintenance_window};
use hydrus_store::Store;
use hydrus_store::import::import_legacy;
use slint::ComponentHandle as _;
use slint::Model as _;

fn seed_views(store: &Store, files: &[HashId]) {
    let files = files.to_vec();
    store
        .write(move |ctx| {
            for (n, file) in files.iter().enumerate() {
                for canvas in 0..2 {
                    ctx.conn().execute(
                        "INSERT INTO file_viewing_stats(hash_id,canvas_type,last_viewed_ms,views,viewtime_ms) VALUES(?1,?2,?3,?4,?5)",
                        rusqlite::params![file.0, canvas, 1_700_000_000_000_i64 + n as i64, 3, 4000],
                    )?;
                }
            }
            Ok(())
        })
        .unwrap();
}

fn viewed(store: &Store) -> Vec<i64> {
    store
        .read(|conn| {
            let mut stmt =
                conn.prepare("SELECT DISTINCT hash_id FROM file_viewing_stats ORDER BY hash_id")?;
            Ok(stmt
                .query_map([], |r| r.get(0))?
                .collect::<rusqlite::Result<Vec<_>>>()?)
        })
        .unwrap()
}

/// Right-click the selection and choose manage > viewing stats > clear.
fn choose_clear(ui: &MainWindow) {
    ui.invoke_thumbnail_menu_requested(-1);
    let menu = ui.get_thumbnail_menu();
    let rows = menu.manage_viewing.clone();
    let labels: Vec<_> = (0..rows.row_count())
        .map(|i| rows.row_data(i).unwrap())
        .collect();
    assert_eq!(
        labels
            .iter()
            .map(|r| r.label.to_string())
            .collect::<Vec<_>>(),
        ["clear"]
    );
    ui.invoke_menu_chosen(labels[0].id);
}

// leaf: audit-media-context-missing-stats
#[test]
fn clearing_selected_viewing_stats_asks_then_clears_only_the_selection() {
    let legacy = hydrus_testkit::legacy_fixture("basic");
    let native = tempfile::tempdir().unwrap();
    import_legacy(
        legacy.path(),
        &native.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store: Arc<Store> = Store::open(native.path()).unwrap();
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(&ui, Pages::single(SearchPage::new(store.clone())));
    ui.invoke_search_edited("system:everything".into());
    ui.invoke_search_accepted();
    let page = bound.current.borrow().clone();
    let files = page.borrow().results().to_vec();
    assert!(files.len() >= 3, "{}", files.len());
    store
        .write(|ctx| {
            ctx.conn().execute("DELETE FROM file_viewing_stats", [])?;
            Ok(())
        })
        .unwrap();
    seed_views(&store, &files[..3]);
    let all: Vec<i64> = {
        let mut ids: Vec<i64> = files[..3].iter().map(|f| i64::from(f.0)).collect();
        ids.sort_unstable();
        ids
    };

    // two selected: the question counts them; "no" changes nothing
    page.borrow_mut().select_files(&files[..2]);
    choose_clear(&ui);
    // (the reference's question and its writes, for two files and for one:
    // oracle/record_clear_viewing_stats.py -> clear_viewing_stats.json)
    let recorded = hydrus_testkit::fixture_json("clear_viewing_stats.json");
    let step = |files: u64, answer: bool| {
        recorded["steps"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["files"] == files && s["answer"] == answer)
            .unwrap()
            .clone()
    };
    let (two_no, two_yes) = (step(2, false), step(2, true));
    let (one_no, one_yes) = (step(1, false), step(1, true));
    let question = thumbnail_maintenance_window::question().expect("a question");
    let asked = &two_no["questions"][0];
    assert_eq!(
        question.get_window_title(),
        asked["title"].as_str().unwrap()
    );
    assert_eq!(question.get_message(), asked["message"].as_str().unwrap());
    assert_eq!(question.get_yes_label(), asked["yes"].as_str().unwrap());
    assert_eq!(question.get_no_label(), asked["no"].as_str().unwrap());
    assert_eq!(two_no["writes"].as_array().unwrap().len(), 0, "no writes");
    question.invoke_answered(false);
    assert_eq!(viewed(&store), all);

    // closing the question (escape, the title bar) is a "no" too
    choose_clear(&ui);
    thumbnail_maintenance_window::question()
        .unwrap()
        .invoke_cancelled();
    assert_eq!(viewed(&store), all);

    // "yes" deletes both canvases' records of just the selected files
    choose_clear(&ui);
    thumbnail_maintenance_window::question()
        .unwrap()
        .invoke_answered(true);
    assert_eq!(viewed(&store), [i64::from(files[2].0)]);
    // (the reference's yes is one delete write of the selected files' hashes
    // and nothing else: native deleted just the selected files' rows)
    assert_eq!(two_yes["writes"].as_array().unwrap().len(), 1);

    // one selected: "this file"
    page.borrow_mut().select_files(&files[2..3]);
    choose_clear(&ui);
    let question = thumbnail_maintenance_window::question().unwrap();
    assert_eq!(
        question.get_message(),
        one_no["questions"][0]["message"].as_str().unwrap()
    );
    assert_eq!(one_no["writes"].as_array().unwrap().len(), 0);
    question.invoke_answered(true);
    assert!(viewed(&store).is_empty());
    assert_eq!(one_yes["writes"].as_array().unwrap().len(), 1);
}
