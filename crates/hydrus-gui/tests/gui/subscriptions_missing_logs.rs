//! Subscription queries whose log is missing, and checker options edits, through the
//! real manage subscriptions dialog, against `oracle/fixtures/subscription_missing_logs.json`
//! (`record_subscription_missing_logs.py`) and `subscription_checker_edit.json`
//! (`record_subscription_checker_edit.py`).

use slint::ComponentHandle as _;

use hydrus_gui::{MainWindow, Pages, bind, headless};
use hydrus_gui_model::subscription_exchange::Headers;
use hydrus_store::{settings, subscriptions};
use serde_json::Value;

use crate::subscriptions::{asked, open_dialog, rows, store};

/// The reference's question when manage subscriptions finds missing logs
/// (`ClientGUI._ManageSubscriptions`).
const MISSING_LOGS_MESSAGE: &str = "1 subscription queries had missing database data! This is a serious error!\n\nIf you continue, the client will now create and save empty file/search logs for those queries, essentially resetting them, but if you know you need to exit and fix your database in a different way, cancel out now.\n\nIf you do not know why this happened, you may have had a hard drive fault. Please check the 'Recovery->Help my db is broke' document in the help, and you may want to contact hydrus dev.";

fn header_of(store: &hydrus_store::Store, queue: i64) -> Value {
    store
        .read(settings::get::<Headers>)
        .unwrap()
        .0
        .get(&queue)
        .cloned()
        .expect("a cached header")
}

fn only_queue(store: &hydrus_store::Store) -> i64 {
    let saved = store.read(subscriptions::subscriptions).unwrap();
    assert_eq!(saved.len(), 1);
    let id = saved[0].id;
    let queries = store
        .read(move |conn| subscriptions::queries(conn, id))
        .unwrap();
    assert_eq!(queries.len(), 1);
    queries[0].queue_id
}

fn paste_and_apply(ui: &MainWindow, bound: &hydrus_gui::Bound, text: &str) {
    let dialog = open_dialog(ui, bound);
    let text = text.to_owned();
    hydrus_gui::set_paster(move || text.clone());
    dialog.invoke_exchange_mode(3);
    dialog.invoke_chosen(0);
    dialog.invoke_apply();
}

// leaf: subscriptions-exchange
#[test]
fn an_accepted_import_without_logs_is_asked_about_again_on_the_next_open() {
    let fixture = hydrus_testkit::fixture_json("subscription_missing_logs.json");
    let (_dirs, store) = store();
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    let dialog = open_dialog(&ui, &bound);
    let text = fixture["source"].to_string();
    hydrus_gui::set_paster(move || text.clone());
    dialog.invoke_exchange_mode(3);
    // the reference's import question, then its "objects added" notice
    let question = &fixture["panel_import_questions"][0];
    let (title, message, choices) = asked(&dialog);
    assert_eq!(title, question["title"].as_str().unwrap());
    assert_eq!(message, question["message"].as_str().unwrap());
    assert_eq!(
        choices,
        [
            question["yes_label"].as_str().unwrap(),
            question["no_label"].as_str().unwrap()
        ]
    );
    dialog.invoke_chosen(0);
    assert_eq!(
        asked(&dialog).1,
        fixture["panel_import_notices"][0][1].as_str().unwrap()
    );
    dialog.invoke_chosen(0);
    dialog.invoke_apply();
    // saved as the reference saves it: the header cached, no log of its own
    let queue = only_queue(&store);
    assert!(
        store
            .read(subscriptions::missing_logs)
            .unwrap()
            .contains(&queue)
    );
    let header = header_of(&store, queue);
    let accepted = &fixture["accepted_header"];
    for index in [8, 13, 14, 15] {
        assert_eq!(header[2][index], accepted[2][index], "header field {index}");
    }
    assert_eq!(fixture["value_containers"], 0);

    // backing out of the next open's question closes the dialog and keeps the mark
    let dialog = open_dialog(&ui, &bound);
    let (title, message, choices) = asked(&dialog);
    assert_eq!(title, "Missing Query Logs!");
    assert_eq!(message, MISSING_LOGS_MESSAGE);
    assert_eq!(choices, ["continue", "back out"]);
    dialog.invoke_chosen(1);
    assert!(bound.subscriptions.borrow().is_none());
    assert!(
        store
            .read(subscriptions::missing_logs)
            .unwrap()
            .contains(&queue)
    );

    // continuing writes empty logs and resets the header, as `Reset` does
    let dialog = open_dialog(&ui, &bound);
    assert_eq!(asked(&dialog).0, "Missing Query Logs!");
    dialog.invoke_chosen(0);
    assert!(!dialog.get_asking());
    assert_eq!(rows(&dialog).len(), 1);
    assert!(store.read(subscriptions::missing_logs).unwrap().is_empty());
    let reset = &fixture["reset_header"];
    let header = header_of(&store, queue);
    assert_eq!(header[2][9][2][1], reset[2][9][2][1]);
    assert_eq!(header[2][9][2][2], reset[2][9][2][2]);
    assert_eq!(header[2][15], reset[2][15]);
    let state = store
        .read(move |conn| subscriptions::query(conn, queue))
        .unwrap()
        .unwrap()
        .state;
    assert_eq!(state.last_check_time, reset[2][4]);
    assert_eq!(state.next_check_time, reset[2][5]);
    assert_eq!(state.paused, reset[2][6]);
    assert_eq!(state.dead, reset[2][7] == 1);
    dialog.invoke_cancel();
    // and it is not asked again
    let dialog = open_dialog(&ui, &bound);
    assert!(!dialog.get_asking());
    dialog.invoke_cancel();
}

fn exported_header(ui: &MainWindow, bound: &hydrus_gui::Bound) -> Value {
    let dialog = open_dialog(ui, bound);
    dialog.invoke_row_clicked(0, false, false);
    dialog.invoke_exchange();
    let child = bound
        .subscription_exchange
        .0
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    let exported: Value = serde_json::from_str(child.get_text().as_str()).unwrap();
    child.invoke_action("cancel".into());
    dialog.invoke_cancel();
    exported[2][0][3][1][0].clone()
}

fn overwrite_with_slow_thread(dialog: &hydrus_gui::SubscriptionsWindow, bound: &hydrus_gui::Bound) {
    dialog.invoke_row_clicked(0, false, false);
    dialog.invoke_overwrite_checker();
    let editor = bound
        .checker_options
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    editor.invoke_preset(1);
    editor.invoke_ok();
    dialog.invoke_apply();
}

// leaf: subscriptions-exchange
#[test]
fn a_checker_edit_marks_an_unloaded_history_unsynced_in_the_saved_header() {
    let fixture = hydrus_testkit::fixture_json("subscription_checker_edit.json");
    let recorded = &fixture["cases"]["slow_thread"]["unloaded"];
    let (_dirs, store) = store();
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    paste_and_apply(&ui, &bound, &fixture["source"].to_string());
    let dialog = open_dialog(&ui, &bound);
    overwrite_with_slow_thread(&dialog, &bound);
    let header = header_of(&store, only_queue(&store));
    for index in [8, 9, 13, 14, 15, 16] {
        if index == 9 {
            // (the count of files by status, not when it was counted)
            assert_eq!(header[2][9][2][1], recorded[2][9][2][1]);
            continue;
        }
        assert_eq!(header[2][index], recorded[2][index], "header field {index}");
    }
    // the same header reaches an export
    let exported = exported_header(&ui, &bound);
    for index in [8, 13, 14] {
        assert_eq!(
            exported[2][index], recorded[2][index],
            "exported field {index}"
        );
    }
}

// leaf: subscriptions-exchange
#[test]
fn a_checker_edit_recalculates_a_history_the_dialog_holds() {
    let fixture = hydrus_testkit::fixture_json("subscription_checker_edit.json");
    let recorded = &fixture["cases"]["slow_thread"]["loaded"];
    let (_dirs, store) = store();
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    paste_and_apply(&ui, &bound, &fixture["source"].to_string());
    // exporting reads the history into the dialog
    let dialog = open_dialog(&ui, &bound);
    dialog.invoke_row_clicked(0, false, false);
    dialog.invoke_exchange();
    let child = bound
        .subscription_exchange
        .0
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    child.invoke_action("cancel".into());
    overwrite_with_slow_thread(&dialog, &bound);
    let header = header_of(&store, only_queue(&store));
    for index in [8, 13, 14, 16] {
        assert_eq!(header[2][index], recorded[2][index], "header field {index}");
    }
    // and the timing the recalculation found
    let queue = only_queue(&store);
    let state = store
        .read(move |conn| subscriptions::query(conn, queue))
        .unwrap()
        .unwrap()
        .state;
    assert_eq!(state.next_check_time, recorded[2][5]);
    assert_eq!(state.paused, recorded[2][6]);
    assert_eq!(state.dead, recorded[2][7] == 1);
}
