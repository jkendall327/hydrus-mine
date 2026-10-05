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

fn decoded(values: &serde_json::Value) -> Vec<hydrus_search::Predicate> {
    values
        .as_array()
        .unwrap()
        .iter()
        .map(|value| {
            let object =
                hydrus_legacy::serialisable::SerialisableObject::from_tuple_str(&value.to_string())
                    .unwrap();
            hydrus_legacy::objects::predicates::predicate(&object).unwrap()
        })
        .collect()
}
fn mixed_editor(
    ui: &MainWindow,
    bound: &hydrus_gui::Bound,
    selected: &[hydrus_search::Predicate],
) -> hydrus_gui::PredicateEditorWindow {
    let current = bound.current.borrow().borrow().active_predicates().to_vec();
    for (i, p) in selected.iter().enumerate() {
        let index =
            i32::try_from(current.iter().position(|candidate| candidate == p).unwrap()).unwrap();
        ui.invoke_active_predicate_clicked(index, i != 0, false);
    }
    ui.invoke_active_predicate_activated(false, true);
    bound
        .predicate_editor
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong()
}
#[test]
fn mixed_apply_is_atomic_and_hidden_cancel_rebind_preserve_all_original_terms() {
    use hydrus_core::search::recent::RecentPredicates;
    use std::collections::HashSet;
    let (_dir, store) = setup();
    let windows = headless::init();
    let recording = hydrus_testkit::fixture_json("active_predicate_mixed.json");
    let case = recording["mixed"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == "mixed" && c["accepted"] == true)
        .unwrap();
    let context = FileSearchContext {
        location: LocationContext::single(hydrus_core::ServiceKey::new(b"local files".to_vec())),
        predicates: decoded(&case["before"]["predicates"]),
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
    let selected = decoded(&case["selected"]);
    let before = bound.current.borrow().borrow().active_predicates().to_vec();
    let set = |values: Vec<hydrus_search::Predicate>| values.into_iter().collect::<HashSet<_>>();
    let child = mixed_editor(&ui, &bound, &selected);
    assert!(child.get_batch_mode());
    assert_eq!(child.get_panels().row_count(), 1);
    assert_eq!(child.get_simple_texts().row_count(), 1);
    assert_eq!(
        child.get_invertible_labels().row_data(0).unwrap(),
        "system:inbox"
    );
    child.invoke_simple_edited(0, "---".into());
    child.invoke_number_edited(0, 2, 11);
    child.invoke_ok(0);
    assert_eq!(bound.current.borrow().borrow().active_predicates(), before);
    assert_eq!(
        child.get_error(),
        "Please enter some tag, namespace, or wildcard text!"
    );
    assert!(bound.predicate_editor.borrow().is_some());
    child.invoke_simple_edited(0, "--series:edited*".into());
    child.invoke_invertible_clicked(0);
    child.invoke_chose(0, 1, 4);
    child.hide().unwrap();
    child.invoke_simple_edited(0, "hidden:wrong".into());
    child.invoke_invertible_clicked(0);
    child.invoke_ok(0);
    assert_eq!(bound.current.borrow().borrow().active_predicates(), before);
    child.show().unwrap();
    assert_eq!(
        child.get_simple_texts().row_data(0).unwrap(),
        "--series:edited*"
    );
    assert_eq!(
        child.get_invertible_labels().row_data(0).unwrap(),
        "system:archive"
    );
    let adapter = windows.get(windows.count() - 1).unwrap();
    let pixels = headless::render(&adapter, 900, 480);
    headless::save_png(
        &hydrus_testkit::artifacts_dir().join("active-predicate-mixed.png"),
        &pixels,
        900,
        480,
    )
    .unwrap();
    assert!(pixels.iter().any(|p| p.r != p.g));
    child.invoke_cancel();
    assert_eq!(bound.current.borrow().borrow().active_predicates(), before);
    let child = mixed_editor(&ui, &bound, &selected);
    child.invoke_simple_edited(0, "--series:edited*".into());
    child.invoke_invertible_clicked(0);
    child.invoke_chose(0, 1, 4);
    child.invoke_number_edited(0, 2, 11);
    child.invoke_ok(0);
    assert_eq!(
        set(bound.current.borrow().borrow().active_predicates().to_vec()),
        set(decoded(&case["after"]["predicates"]))
    );
    assert!(bound.predicate_editor.borrow().is_none());
    let recent: RecentPredicates = store.read(hydrus_store::settings::get).unwrap();
    assert!(recent.non_system[&3].contains(&decoded(&case["value"])[2]));
    let current = bound.current.borrow().clone();
    let edited = current.borrow().active_predicates().to_vec();
    let child = mixed_editor(&ui, &bound, &edited);
    let successor = bind(&ui, Pages::open(store.clone()).unwrap());
    let successor_before = successor
        .current
        .borrow()
        .borrow()
        .active_predicates()
        .to_vec();
    child.show().unwrap();
    child.invoke_simple_edited(0, "stale:wrong".into());
    child.invoke_ok(0);
    assert_eq!(current.borrow().active_predicates(), edited);
    assert_eq!(
        successor.current.borrow().borrow().active_predicates(),
        successor_before
    );
    child.hide().unwrap();
    ui.hide().unwrap();
}

#[test]
fn mixed_same_family_panels_reopen_distinct_supplied_values_and_apply_both() {
    use std::collections::HashSet;
    let (_dir, store) = setup();
    let _windows = headless::init();
    let defaults = hydrus_store::settings::CustomPredicateDefaults {
        predicates: hydrus_search::parse_api_search(&serde_json::json!(["system:filesize > 99MB"]))
            .unwrap(),
    };
    store
        .write(move |tx| hydrus_store::settings::set(tx.conn(), &defaults))
        .unwrap();
    let recording = hydrus_testkit::fixture_json("active_predicate_mixed.json");
    let case = recording["mixed"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == "two_sizes" && c["accepted"] == true)
        .unwrap();
    let context = FileSearchContext {
        location: LocationContext::single(hydrus_core::ServiceKey::new(b"local files".to_vec())),
        predicates: decoded(&case["before"]["predicates"]),
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
    let child = mixed_editor(&ui, &bound, &decoded(&case["selected"]));
    assert_eq!(child.get_panels().row_count(), 2);
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
    assert_eq!(
        child
            .get_panels()
            .row_data(1)
            .unwrap()
            .fields
            .row_data(2)
            .unwrap()
            .value,
        3
    );
    for (i, size) in [11, 17].into_iter().enumerate() {
        let i = i32::try_from(i).unwrap();
        child.invoke_chose(i, 1, 4);
        child.invoke_number_edited(i, 2, size);
    }
    child.invoke_ok(0);
    assert!(bound.predicate_editor.borrow().is_none());
    assert_eq!(
        bound
            .current
            .borrow()
            .borrow()
            .active_predicates()
            .iter()
            .cloned()
            .collect::<HashSet<_>>(),
        decoded(&case["after"]["predicates"])
            .into_iter()
            .collect::<HashSet<_>>()
    );
    ui.hide().unwrap();
}

fn active_or(
    ui: &MainWindow,
    bound: &hydrus_gui::Bound,
    selected: &[hydrus_search::Predicate],
    start: bool,
) -> hydrus_gui::SearchOrWindow {
    let current = bound.current.borrow().borrow().active_predicates().to_vec();
    for (i, p) in selected.iter().enumerate() {
        let row =
            i32::try_from(current.iter().position(|candidate| candidate == p).unwrap()).unwrap();
        ui.invoke_active_predicate_clicked(row, i != 0, false);
    }
    if start {
        let row = i32::try_from(current.iter().position(|p| p == &selected[0]).unwrap()).unwrap();
        ui.invoke_active_predicate_menu_opened(row);
        let command = hydrus_gui_model::active_predicates::Command::StartOr.id();
        assert!(
            ui.get_active_predicate_menu()
                .iter()
                .any(|item| item.id == command)
        );
        ui.invoke_active_predicate_menu_chosen(command);
    } else {
        ui.invoke_active_predicate_activated(false, true);
    }
    assert!(ui.get_search_or_open());
    bound.search_or.borrow().as_ref().unwrap().clone_strong()
}
#[test]
fn populated_or_and_start_or_replay_all_ten_actual_qt_apply_cancel_shapes() {
    use std::collections::HashSet;
    let (_dir, store) = setup();
    let windows = headless::init();
    let recording = hydrus_testkit::fixture_json("active_predicate_or.json");
    assert_eq!(recording["ors"].as_array().unwrap().len(), 10);
    for case in recording["ors"].as_array().unwrap() {
        let ui = MainWindow::new().unwrap();
        ui.show().unwrap();
        let context = FileSearchContext {
            location: LocationContext::single(hydrus_core::ServiceKey::new(
                b"local files".to_vec(),
            )),
            predicates: decoded(&case["before"]["predicates"]),
            ..Default::default()
        };
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
        let selected = decoded(&case["selected"]);
        let child = active_or(
            &ui,
            &bound,
            &selected,
            case["command"] == "start_or_predicate",
        );
        let keep = case["or_keep"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_u64().unwrap() as usize)
            .collect::<Vec<_>>();
        let initial = child.get_predicates().row_count();
        assert_eq!(initial, if case["name"] == "start_single" { 1 } else { 2 });
        for i in (0..initial).rev() {
            if !keep.contains(&i) {
                child.invoke_remove(i32::try_from(i).unwrap());
            }
        }
        if case["name"] == "edit_or" && case["accepted"] == true {
            let adapter = windows.get(windows.count() - 1).unwrap();
            let pixels = headless::render(&adapter, 900, 600);
            headless::save_png(
                &hydrus_testkit::artifacts_dir().join("active-predicate-populated-or.png"),
                &pixels,
                900,
                600,
            )
            .unwrap();
            assert!(pixels.iter().any(|p| p.r != p.g));
        }
        if case["accepted"] == true {
            child.invoke_apply();
        } else {
            child.invoke_cancel();
        }
        assert!(bound.search_or.borrow().is_none());
        assert!(!ui.get_search_or_open());
        assert_eq!(
            bound
                .current
                .borrow()
                .borrow()
                .active_predicates()
                .iter()
                .cloned()
                .collect::<HashSet<_>>(),
            decoded(&case["after"]["predicates"])
                .into_iter()
                .collect::<HashSet<_>>(),
            "{} {}",
            case["name"],
            case["accepted"]
        );
        ui.hide().unwrap();
    }
}
#[test]
fn retained_populated_or_hidden_page_rebind_and_destroyed_main_cannot_publish() {
    let (_dir, store) = setup();
    let _windows = headless::init();
    let recording = hydrus_testkit::fixture_json("active_predicate_or.json");
    let case = &recording["ors"][4];
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let context = FileSearchContext {
        location: LocationContext::single(hydrus_core::ServiceKey::new(b"local files".to_vec())),
        predicates: decoded(&case["before"]["predicates"]),
        ..Default::default()
    };
    let bound = bind(
        &ui,
        Pages::single(SearchPage::restored(
            store.clone(),
            context.clone(),
            true,
            None,
            Vec::new(),
        )),
    );
    let selected = decoded(&case["selected"]);
    let before = bound.current.borrow().borrow().active_predicates().to_vec();
    let child = active_or(&ui, &bound, &selected, false);
    child.hide().unwrap();
    child.invoke_remove(0);
    child.invoke_edited("hidden:wrong".into());
    child.invoke_apply();
    assert!(bound.search_or.borrow().is_some());
    assert_eq!(bound.current.borrow().borrow().active_predicates(), before);
    child.show().unwrap();
    assert_eq!(child.get_predicates().row_count(), 2);
    let original = bound.current.borrow().clone();
    bound.pages.borrow_mut().new_search_page();
    ui.invoke_tab_chosen(0, 1);
    assert!(bound.search_or.borrow().is_none());
    ui.invoke_tab_chosen(0, 0);
    child.show().unwrap();
    child.invoke_remove(0);
    child.invoke_apply();
    assert_eq!(original.borrow().active_predicates(), before);
    child.hide().unwrap();
    let stale = active_or(&ui, &bound, &selected, false);
    let successor = bind(
        &ui,
        Pages::single(SearchPage::restored(
            store.clone(),
            context,
            true,
            None,
            Vec::new(),
        )),
    );
    assert!(!ui.get_search_or_open());
    stale.invoke_apply();
    let live = active_or(&ui, &successor, &selected, false);
    stale.invoke_cancel();
    assert!(
        ui.get_search_or_open(),
        "retired close cannot release the successor OR guard"
    );
    assert!(successor.search_or.borrow().is_some());
    let current = successor.current.borrow().clone();
    let after = current.borrow().active_predicates().to_vec();
    drop(ui);
    live.invoke_remove(0);
    live.invoke_apply();
    assert_eq!(current.borrow().active_predicates(), after);
    assert!(successor.search_or.borrow().is_none());
}

#[test]
fn retained_or_system_child_does_not_keep_the_or_or_main_component_alive() {
    let (_dir, store) = setup();
    let _windows = headless::init();
    let recording = hydrus_testkit::fixture_json("active_predicate_or.json");
    let case = &recording["ors"][4];
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let context = FileSearchContext {
        location: LocationContext::single(hydrus_core::ServiceKey::new(b"local files".to_vec())),
        predicates: decoded(&case["before"]["predicates"]),
        ..Default::default()
    };
    let bound = bind(
        &ui,
        Pages::single(SearchPage::restored(store, context, true, None, Vec::new())),
    );
    let child = active_or(&ui, &bound, &decoded(&case["selected"]), false);
    child.invoke_edited("system:filesize".into());
    child.invoke_enter(false);
    let system = bound
        .search_or
        .system
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    let weak_or = child.as_weak();
    let weak_main = ui.as_weak();
    drop(child);
    drop(bound);
    drop(ui);
    assert!(weak_main.upgrade().is_none());
    assert!(
        weak_or.upgrade().is_none(),
        "the owned system child has only weak callbacks to its OR owner"
    );
    system.invoke_number_edited(0, 2, 99);
    system.invoke_ok(0);
    system.invoke_cancel();
}

#[test]
fn hidden_or_parent_reconciles_actual_system_and_nested_child_cancellation() {
    let (_dir, store) = setup();
    let _windows = headless::init();
    let recording = hydrus_testkit::fixture_json("active_predicate_or.json");
    let case = &recording["ors"][4];
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let context = FileSearchContext {
        location: LocationContext::single(hydrus_core::ServiceKey::new(b"local files".to_vec())),
        predicates: decoded(&case["before"]["predicates"]),
        ..Default::default()
    };
    let bound = bind(
        &ui,
        Pages::single(SearchPage::restored(store, context, true, None, Vec::new())),
    );
    let child = active_or(&ui, &bound, &decoded(&case["selected"]), false);
    child.invoke_edited("system:filesize".into());
    child.invoke_enter(false);
    assert!(child.get_blocked());
    let system = bound
        .search_or
        .system
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    child.hide().unwrap();
    system.invoke_cancel();
    assert!(bound.search_or.system.borrow().is_none());
    child.show().unwrap();
    assert!(
        !child.get_blocked(),
        "system Cancel reconciles the still-owned hidden parent"
    );
    child.invoke_or_action(3);
    let nested_slot = bound.search_or.child().unwrap();
    let nested = nested_slot.borrow().as_ref().unwrap().clone_strong();
    assert!(child.get_blocked());
    child.hide().unwrap();
    nested.invoke_cancel();
    assert!(nested_slot.borrow().is_none());
    child.show().unwrap();
    assert!(
        !child.get_blocked(),
        "nested OR Cancel reconciles the still-owned hidden parent"
    );
    let before = bound.current.borrow().borrow().active_predicates().to_vec();
    child.invoke_edited("system:filesize".into());
    child.invoke_enter(false);
    let system = bound
        .search_or
        .system
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    assert!(child.get_blocked());
    ui.hide().unwrap();
    system.invoke_cancel();
    ui.show().unwrap();
    assert!(bound.search_or.system.borrow().is_none());
    assert!(
        !child.get_blocked(),
        "cleanup reconciles its live local parent while the external Main owner is temporarily hidden"
    );
    child.invoke_or_action(3);
    let nested_slot = bound.search_or.child().unwrap();
    let nested = nested_slot.borrow().as_ref().unwrap().clone_strong();
    assert!(child.get_blocked());
    ui.hide().unwrap();
    nested.invoke_cancel();
    ui.show().unwrap();
    assert!(nested_slot.borrow().is_none());
    assert!(
        !child.get_blocked(),
        "nested cleanup also reconciles before the external owner-watch timer runs"
    );
    assert_eq!(bound.current.borrow().borrow().active_predicates(), before);
    child.invoke_apply();
    assert!(bound.search_or.borrow().is_none());
    assert!(!ui.get_search_or_open());
    ui.hide().unwrap();
}
