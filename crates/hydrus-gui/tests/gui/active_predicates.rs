//! Populated active values, actual query consumers and retained-owner boundaries.
use hydrus_gui::{MainWindow, Pages, SearchPage, bind, headless};
use hydrus_search::{FileSearchContext, LocationContext};
use hydrus_store::Store;
use slint::{ComponentHandle as _, Model as _};
use std::sync::Arc;
fn setup() -> (tempfile::TempDir, Arc<Store>) {
    let legacy = hydrus_testkit::legacy_fixture("basic");
    let dir = tempfile::tempdir().unwrap();
    hydrus_store::import::import_legacy(
        legacy.path(),
        &dir.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store = Store::open(dir.path()).unwrap();
    (dir, store)
}
fn main(store: &Arc<Store>) -> (MainWindow, hydrus_gui::Bound) {
    let context = FileSearchContext {
        location: LocationContext::single(hydrus_core::ServiceKey::new(b"local files".to_vec())),
        ..Default::default()
    };
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(
        &ui,
        Pages::single(SearchPage::restored(
            store.clone(),
            context,
            true,
            None,
            Vec::new(),
        )),
    );
    (ui, bound)
}
fn add(ui: &MainWindow, text: &str) {
    ui.invoke_search_edited(text.into());
    ui.invoke_search_accepted();
}
fn index(ui: &MainWindow, text: &str) -> i32 {
    i32::try_from(
        ui.get_predicates()
            .iter()
            .position(|row| row.text.as_str() == text)
            .unwrap(),
    )
    .unwrap()
}
fn editor(ui: &MainWindow, bound: &hydrus_gui::Bound) -> hydrus_gui::PredicateEditorWindow {
    ui.invoke_active_predicate_clicked(index(ui, "system:filesize < 7KB"), false, false);
    ui.invoke_active_predicate_activated(false, true);
    bound
        .predicate_editor
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong()
}
#[test]
fn actual_existing_size_cancel_unchanged_and_edit_reach_reference_query_counts() {
    let (_dir, store) = setup();
    let windows = headless::init();
    let defaults = hydrus_store::settings::CustomPredicateDefaults {
        predicates: hydrus_search::parse_api_search(&serde_json::json!(["system:filesize > 99MB"]))
            .unwrap(),
    };
    store
        .write(move |tx| hydrus_store::settings::set(tx.conn(), &defaults))
        .unwrap();
    let (ui, bound) = main(&store);
    add(&ui, "system:inbox");
    add(&ui, "system:filesize < 7KB");
    let main_adapter = windows.get(windows.count() - 1).unwrap();
    let pixels = headless::render(&main_adapter, 1000, 800);
    assert!(pixels.iter().any(|pixel| pixel.r != pixel.g));
    let position = slint::LogicalPosition::new(
        ui.get_active_predicate_list_x() + 6.0,
        ui.get_active_predicate_list_y() + 11.0,
    );
    ui.window()
        .dispatch_event(slint::platform::WindowEvent::PointerPressed {
            position,
            button: slint::platform::PointerEventButton::Left,
        });
    ui.window()
        .dispatch_event(slint::platform::WindowEvent::PointerReleased {
            position,
            button: slint::platform::PointerEventButton::Left,
        });
    assert!(
        ui.get_active_predicate_selected().row_data(0).unwrap(),
        "real first-row pointer selects the active value"
    );
    let recording = hydrus_testkit::fixture_json("active_predicate_edit.json");
    let accepted = recording["edits"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["name"] == "size" && e["accepted"] == true)
        .unwrap();
    assert_eq!(
        bound.current.borrow().borrow().results().len(),
        accepted["before"]["query_count"].as_u64().unwrap() as usize
    );
    let before = bound.current.borrow().borrow().predicates();
    let child = editor(&ui, &bound);
    assert!(child.get_editing_existing());
    assert_eq!(child.get_panels().row_count(), 1);
    assert_eq!(
        child
            .get_panels()
            .row_data(0)
            .unwrap()
            .fields
            .row_data(2)
            .unwrap()
            .value,
        7
    );
    child.invoke_number_edited(0, 2, 11);
    child.invoke_cancel();
    child.invoke_ok(0);
    assert_eq!(bound.current.borrow().borrow().predicates(), before);
    let child = editor(&ui, &bound);
    child.invoke_ok(0);
    assert_eq!(
        bound.current.borrow().borrow().predicates(),
        before,
        "unchanged edit does not toggle the original"
    );
    let child = editor(&ui, &bound);
    child.invoke_chose(0, 1, 4);
    child.invoke_number_edited(0, 2, 11);
    let adapter = windows.get(windows.count() - 1).unwrap();
    let pixels = headless::render(&adapter, 900, 480);
    headless::save_png(
        &hydrus_testkit::artifacts_dir().join("active-predicate-existing-size.png"),
        &pixels,
        900,
        480,
    )
    .unwrap();
    child.invoke_ok(0);
    assert_eq!(
        bound.current.borrow().borrow().predicates(),
        vec!["system:filesize > 11KB", "system:inbox"]
    );
    assert_eq!(
        bound.current.borrow().borrow().results().len(),
        accepted["after"]["query_count"].as_u64().unwrap() as usize
    );
    assert!(bound.predicate_editor.borrow().is_none());
}
#[test]
fn captured_menu_and_populated_child_cannot_edit_hidden_replaced_rebound_or_dropped_owners() {
    let (_dir, store) = setup();
    let _windows = headless::init();
    let (ui, bound) = main(&store);
    add(&ui, "system:inbox");
    add(&ui, "system:filesize < 7KB");
    let before = bound.current.borrow().borrow().predicates();
    ui.invoke_active_predicate_menu_opened(index(&ui, "system:inbox"));
    let invert = ui
        .get_active_predicate_menu()
        .iter()
        .find(|row| row.label.starts_with("invert:"))
        .unwrap()
        .id;
    ui.hide().unwrap();
    ui.invoke_active_predicate_menu_chosen(invert);
    assert_eq!(bound.current.borrow().borrow().predicates(), before);
    ui.show().unwrap();
    ui.invoke_search_or_action(3);
    assert!(ui.get_search_or_open());
    ui.invoke_active_predicate_clicked(index(&ui, "system:filesize < 7KB"), false, false);
    ui.invoke_active_predicate_activated(false, true);
    ui.invoke_active_predicate_menu_chosen(invert);
    assert!(
        bound.predicate_editor.borrow().is_none(),
        "owned OR child blocks a parent active editor"
    );
    assert_eq!(bound.current.borrow().borrow().predicates(), before);
    let or_child = bound.search_or.borrow().as_ref().unwrap().clone_strong();
    or_child.invoke_cancel();
    assert!(!ui.get_search_or_open());
    let child = editor(&ui, &bound);
    child.invoke_number_edited(0, 2, 11);
    child.hide().unwrap();
    child.invoke_ok(0);
    assert_eq!(bound.current.borrow().borrow().predicates(), before);
    child.show().unwrap();
    let original = bound.current.borrow().clone();
    bound.pages.borrow_mut().new_search_page();
    ui.invoke_tab_chosen(0, 1);
    assert!(
        bound.predicate_editor.borrow().is_none(),
        "page transition cancels its existing-value child"
    );
    ui.invoke_tab_chosen(0, 0);
    child.show().unwrap();
    child.invoke_ok(0);
    assert_eq!(original.borrow().predicates(), before);
    ui.invoke_active_predicate_menu_chosen(invert);
    assert_eq!(
        original.borrow().predicates(),
        before,
        "stale menu remains invalid after A→B→A"
    );
    child.hide().unwrap();
    let old = editor(&ui, &bound);
    let successor = bind(&ui, Pages::open(store.clone()).unwrap());
    let new_before = successor.current.borrow().borrow().predicates();
    old.show().unwrap();
    old.invoke_ok(0);
    ui.invoke_active_predicate_menu_chosen(invert);
    assert_eq!(successor.current.borrow().borrow().predicates(), new_before);
    assert_eq!(original.borrow().predicates(), before);
    old.hide().unwrap();
    add(&ui, "system:inbox");
    add(&ui, "system:filesize < 7KB");
    let child = editor(&ui, &successor);
    let current = successor.current.borrow().clone();
    let dropped_before = current.borrow().predicates();
    let weak = ui.as_weak();
    ui.hide().unwrap();
    drop(ui);
    assert!(weak.upgrade().is_none());
    child.invoke_number_edited(0, 2, 11);
    child.invoke_ok(0);
    assert_eq!(current.borrow().predicates(), dropped_before);
    child.invoke_cancel();
}

#[test]
fn dropping_the_last_owner_releases_a_populated_editor_without_cancel() {
    let (_dir, store) = setup();
    let _windows = headless::init();
    let (ui, bound) = main(&store);
    add(&ui, "system:filesize < 7KB");
    let child = editor(&ui, &bound);
    let weak_child = child.as_weak();
    ui.hide().unwrap();
    drop(ui);
    drop(bound);
    assert!(
        weak_child.upgrade().is_some(),
        "retained child is still live"
    );
    drop(child);
    assert!(
        weak_child.upgrade().is_none(),
        "the close callback must not retain its owning slot"
    );
}
