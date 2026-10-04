//! Actual recorded inputs through native page mutation and Undo menu consumers.
use std::collections::HashMap;
use std::sync::Arc;

use hydrus_gui::{MainWindow, Pages, SearchPage, bind, headless};
use hydrus_search::{Predicate, TextContext, parse_api_search, predicate_text};
use hydrus_store::{Store, import::import_legacy};
use serde_json::{Value, json};
use slint::{ComponentHandle as _, Model as _};

fn decode(value: &Value) -> Predicate {
    let object =
        hydrus_legacy::serialisable::SerialisableObject::from_tuple_str(&value.to_string())
            .unwrap();
    hydrus_legacy::objects::predicates::predicate(&object).unwrap()
}
fn labels(predicates: &[Predicate]) -> Value {
    json!(
        predicates
            .iter()
            .map(|p| predicate_text(p, &TextContext::default()))
            .collect::<Vec<_>>()
    )
}
fn line(ui: &MainWindow, label: &str) -> (i32, i32) {
    let panes = ui.get_menu_panes();
    let p = panes.row_count() - 1;
    let rows = panes.row_data(p).unwrap().lines;
    let i = (0..rows.row_count())
        .find(|&i| rows.row_data(i).unwrap().label == label)
        .unwrap();
    (i32::try_from(p).unwrap(), i32::try_from(i).unwrap())
}
fn hover(ui: &MainWindow, label: &str) {
    let (p, i) = line(ui, label);
    ui.invoke_menu_line_hovered(p, i, 150.0, 40.0, 150.0);
}
fn choose(ui: &MainWindow, label: &str) {
    let (p, i) = line(ui, label);
    ui.invoke_menu_line_clicked(p, i, 0.0, 0.0, 0.0);
}
fn searching(ui: &MainWindow) {
    ui.invoke_menu_title_pressed(1, 40.0, 22.0);
    hover(ui, "searching");
}

#[test]
fn recorded_global_history_reaches_visible_locked_restored_and_empty_pages() {
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
    let mut pages = Pages::single(SearchPage::new(store));
    pages.rename_shown("Undo A");
    let bound = bind(&ui, pages);
    bound.current.borrow().borrow_mut().set_synchronised(false);
    let mut searches = HashMap::from([("Undo A".to_owned(), bound.current.borrow().clone())]);
    let recording = hydrus_testkit::fixture_json("search_predicate_undo.json");
    for event in recording["events"].as_array().unwrap() {
        let op = &event["operation"];
        if let Some(input) = op.get("enter") {
            assert!(
                bound
                    .current
                    .borrow()
                    .borrow_mut()
                    .add_predicates(&[decode(input)])
            );
        }
        if let Some(kind) = op.get("undo") {
            searching(&ui);
            hover(&ui, kind.as_str().unwrap());
            choose(&ui, op["predicate"].as_str().unwrap());
        }
        if op.get("new").is_some() {
            let location = bound.current.borrow().borrow().location().clone();
            let initial = parse_api_search(&json!(["undo:restored"])).unwrap();
            bound
                .pages
                .borrow_mut()
                .open_search(location, initial, "Undo B");
            searches.insert("Undo B".into(), bound.pages.borrow_mut().current());
        }
        if op.get("cancel_child").is_some() {
            ui.invoke_search_edited("".into());
            let index = ui
                .get_suggestions()
                .iter()
                .position(|s| s.text == "system:limit")
                .unwrap();
            ui.invoke_suggestion_chosen(i32::try_from(index).unwrap());
            let child = bound
                .predicate_editor
                .borrow()
                .as_ref()
                .unwrap()
                .clone_strong();
            child.invoke_cancel();
            assert!(bound.predicate_editor.borrow().is_none());
        }
        if op.get("close").is_some() {
            ui.invoke_close_page();
        }
        if op.get("restore").is_some() {
            ui.invoke_menu_title_pressed(1, 40.0, 22.0);
            hover(&ui, "closed pages");
            choose(&ui, "Undo B");
        }
        if let Some(accepted) = op.get("clear") {
            searching(&ui);
            choose(&ui, "clear history…");
            assert_eq!(
                ui.get_question(),
                recording["questions"][0].as_str().unwrap()
            );
            ui.invoke_answer(accepted.as_bool().unwrap());
        }
        if op.get("lock").is_some() {
            bound.current.borrow().borrow_mut().lock_search();
            assert!(bound.current.borrow().borrow().lock().is_some());
        }
        if op.get("empty_notebook").is_some() {
            bound
                .pages
                .borrow_mut()
                .new_page(&hydrus_gui::page_chooser::NewPage::Pages)
                .unwrap();
            bound.pages.borrow_mut().close_shown().unwrap();
            assert!(matches!(
                bound.pages.borrow().shown().content,
                hydrus_core::pages::PageContent::Pages(_)
            ));
        }
        // Show the current tree selection after direct page constructors, without
        // refreshing a paused query or inventing a history mutation.
        let (depth, index) = bound.pages.borrow().shown_position();
        ui.invoke_tab_chosen(i32::try_from(depth).unwrap(), i32::try_from(index).unwrap());
        bound.current.borrow().borrow_mut().set_synchronised(false);
        let history = bound.pages.borrow().predicate_history();
        assert_eq!(labels(&history.added), event["added"], "{}", event["label"]);
        assert_eq!(
            labels(&history.removed),
            event["removed"],
            "{}",
            event["label"]
        );
        for (name, page) in &searches {
            let mut predicates = page.borrow().predicates();
            predicates.sort();
            assert_eq!(
                json!(predicates),
                event["searches"][name],
                "{}: {name}",
                event["label"]
            );
        }
    }
    // Search history is transient and owned by this window, never its saved
    // page predicates or a process-global singleton.
    let another = MainWindow::new().unwrap();
    let other = bind(
        &another,
        Pages::single(SearchPage::new(bound.pages.borrow().store().clone())),
    );
    assert!(other.pages.borrow().predicate_history().added.is_empty());
    assert!(!bound.pages.borrow().predicate_history().added.is_empty());
    for text in ["undo:alpha", "undo:beta"] {
        another.invoke_search_edited(text.into());
        another.invoke_search_accepted();
    }
    another.invoke_remove_predicate(0);
    let history = other.pages.borrow().predicate_history();
    assert_eq!(labels(&history.added), recording["events"][3]["added"]);
    assert_eq!(labels(&history.removed), recording["events"][3]["removed"]);
}
