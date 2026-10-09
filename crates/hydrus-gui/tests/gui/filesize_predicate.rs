//! Native filesize radios publish typed binary-unit searches and own their draft.
use hydrus_gui::predicate_editors::defaults::CustomDefaults;
use hydrus_gui::{MainWindow, Pages, PredicateEditorWindow, SearchPage, bind, headless};
use hydrus_store::{Store, import::import_legacy, settings};
use slint::{
    ComponentHandle as _, Model as _,
    platform::{Key, WindowEvent},
};
use std::sync::Arc;

fn store() -> ([tempfile::TempDir; 2], Arc<Store>) {
    let source = hydrus_testkit::legacy_fixture("basic");
    let destination = tempfile::tempdir().unwrap();
    import_legacy(
        source.path(),
        &destination.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store = Store::open(destination.path()).unwrap();
    ([source, destination], store)
}
fn open(ui: &MainWindow, bound: &hydrus_gui::Bound) -> PredicateEditorWindow {
    ui.invoke_search_edited("".into());
    let index = ui
        .get_suggestions()
        .iter()
        .position(|s| s.text == "system:filesize")
        .unwrap();
    ui.invoke_suggestion_chosen(i32::try_from(index).unwrap());
    bound
        .predicate_editor
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong()
}
fn fields(window: &PredicateEditorWindow) -> slint::ModelRc<hydrus_gui::EditorField> {
    window.get_panels().row_data(0).unwrap().fields
}

// leaf: audit-options-predicate-filesize-size
#[test]
fn real_radio_keys_and_all_units_reach_the_search_consumer() {
    let (guards, store) = store();
    let unrelated = CustomDefaults {
        predicates: vec![hydrus_search::Predicate::System(
            hydrus_search::SystemPredicate::Limit(37),
        )],
    };
    store
        .write(move |writer| settings::set(writer.conn(), &unrelated))
        .unwrap();
    let fixture = hydrus_testkit::fixture_json("filesize_predicate.json");
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    // The recorder queries LOCAL_FILE_SERVICE_KEY, not combined local media.
    let location = hydrus_search::LocationContext::single(hydrus_core::ServiceKey::new(
        hydrus_core::service::builtin_keys::MY_FILES,
    ));
    let mut page = SearchPage::new(store.clone());
    page.choose_location(location.clone());
    assert_eq!(page.location(), &location);
    let bound = bind(&ui, Pages::single(page));
    let window = open(&ui, &bound);
    assert_eq!(fields(&window).row_data(1).unwrap().kind, 8);
    assert_eq!(fields(&window).row_data(3).unwrap().kind, 1);
    let native = windows.get(windows.count() - 1).unwrap();
    let pixels = headless::render(&native, 950, 400);
    headless::save_png(
        &std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("filesize-predicate-native.png"),
        &pixels,
        950,
        400,
    )
    .unwrap();
    for step in fixture["keys"].as_array().unwrap() {
        let text: slint::SharedString = match step["action"].as_str().unwrap() {
            "left-edge" | "left" => Key::LeftArrow.into(),
            "space" => " ".into(),
            "right" | "right-edge" => Key::RightArrow.into(),
            _ => panic!("key"),
        };
        window
            .window()
            .dispatch_event(WindowEvent::KeyPressed { text: text.clone() });
        window
            .window()
            .dispatch_event(WindowEvent::KeyReleased { text });
        let row = fields(&window).row_data(1).unwrap();
        assert_eq!(
            row.options
                .row_data(usize::try_from(row.chosen).unwrap())
                .unwrap(),
            step["value"].as_str().unwrap()
        );
    }
    window.invoke_cancel();
    assert!(bound.current.borrow().borrow().predicates().is_empty());
    for case in fixture["cases"]
        .as_array()
        .unwrap()
        .iter()
        .chain(fixture["boundary_queries"].as_array().unwrap())
    {
        let window = open(&ui, &bound);
        let operator = fixture["operators"]
            .as_array()
            .unwrap()
            .iter()
            .position(|op| op == &case["value"][0])
            .unwrap();
        let unit = fixture["units"]
            .as_array()
            .unwrap()
            .iter()
            .position(|unit| unit[1] == case["value"][2])
            .unwrap();
        window.invoke_chose(0, 1, i32::try_from(operator).unwrap());
        window.invoke_number_edited(
            0,
            2,
            i32::try_from(case["amount"].as_i64().unwrap()).unwrap(),
        );
        window.invoke_chose(0, 3, i32::try_from(unit).unwrap());
        assert_eq!(
            fields(&window).row_data(1).unwrap().chosen,
            i32::try_from(operator).unwrap()
        );
        window.invoke_ok(0);
        assert!(bound.predicate_editor.borrow().is_none());
        let files = bound.current.borrow().borrow().files();
        let mut hashes = store
            .read(|conn| hydrus_store::media::load_basic(conn, &files))
            .unwrap()
            .into_iter()
            .map(|m| m.hash.to_string())
            .collect::<Vec<_>>();
        hashes.sort();
        assert_eq!(
            serde_json::json!(hashes),
            case["hashes"],
            "{}",
            case["value"]
        );
        ui.invoke_remove_predicate(0);
    }
    let window = open(&ui, &bound);
    for case in fixture["bounds"].as_array().unwrap() {
        window.invoke_number_edited(
            0,
            2,
            i32::try_from(case["input"].as_i64().unwrap()).unwrap(),
        );
        assert_eq!(
            fields(&window).row_data(2).unwrap().value,
            i32::try_from(case["result"]["amount"].as_i64().unwrap()).unwrap()
        );
    }
    window.invoke_chose(0, 1, 3);
    window.invoke_number_edited(0, 2, 7);
    window.invoke_chose(0, 3, 4);
    window.invoke_defaults_action(0, "set this as new default".into());
    window.invoke_cancel();
    let saved: CustomDefaults = store.read(settings::get).unwrap();
    assert!(saved.predicates.contains(&hydrus_search::Predicate::System(
        hydrus_search::SystemPredicate::Limit(37)
    )));
    let reopened_store = Store::open(guards[1].path()).unwrap();
    assert_eq!(
        reopened_store
            .read(settings::get::<CustomDefaults>)
            .unwrap(),
        saved
    );
    let reopened = open(&ui, &bound);
    assert_eq!(fields(&reopened).row_data(1).unwrap().chosen, 3);
    assert_eq!(fields(&reopened).row_data(2).unwrap().value, 7);
    assert_eq!(fields(&reopened).row_data(3).unwrap().chosen, 4);
    reopened.invoke_cancel();
    assert_eq!(store.read(settings::get::<CustomDefaults>).unwrap(), saved);
}

#[test]
fn hidden_cancelled_rebound_and_accepted_closed_main_cannot_accept_a_retained_size_draft() {
    let (_guards, store) = store();
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(&ui, Pages::single(SearchPage::new(store.clone())));
    let window = open(&ui, &bound);
    window.hide().unwrap();
    window.invoke_number_edited(0, 2, 99);
    window.invoke_ok(0);
    assert_eq!(fields(&window).row_data(2).unwrap().value, 200);
    window.show().unwrap();
    ui.hide().unwrap();
    super::active_predicates::drain_predicate_timers_for_observer();
    assert!(
        window.window().is_visible(),
        "temporary Main hide must preserve the draft"
    );
    assert!(bound.predicate_editor.borrow().is_some());
    window.invoke_chose(0, 1, 4);
    window.invoke_number_edited(0, 2, 99);
    window.invoke_ok(0);
    assert_eq!(fields(&window).row_data(1).unwrap().chosen, 0);
    assert_eq!(fields(&window).row_data(2).unwrap().value, 200);
    assert!(bound.current.borrow().borrow().predicates().is_empty());
    ui.show().unwrap();
    window.invoke_cancel();
    window.invoke_ok(0);
    assert!(bound.current.borrow().borrow().predicates().is_empty());
    ui.invoke_search_edited("".into());
    let index = ui
        .get_suggestions()
        .iter()
        .position(|s| s.text == "system:filesize")
        .unwrap();
    ui.hide().unwrap();
    ui.invoke_suggestion_chosen(i32::try_from(index).unwrap());
    assert!(bound.predicate_editor.borrow().is_none());
    ui.show().unwrap();
    ui.invoke_search_edited("".into());
    let everything = ui
        .get_suggestions()
        .iter()
        .position(|s| s.text.starts_with("system:everything"))
        .unwrap();
    ui.invoke_suggestion_chosen(i32::try_from(everything).unwrap());
    assert!(
        bound.predicate_editor.borrow().is_none(),
        "a refused hidden activation must not reopen on a later concrete selection"
    );
    assert_eq!(
        bound.current.borrow().borrow().predicates(),
        ["system:everything"]
    );
    ui.invoke_remove_predicate(0);
    let retired = open(&ui, &bound);
    let successor = bind(&ui, Pages::single(SearchPage::new(store.clone())));
    assert!(!retired.window().is_visible());
    retired.show().unwrap();
    retired.invoke_ok(0);
    assert!(bound.current.borrow().borrow().predicates().is_empty());
    assert!(successor.current.borrow().borrow().predicates().is_empty());
    let child = open(&ui, &successor);
    super::active_predicates::drain_predicate_timers_for_observer();
    retired.invoke_cancel();
    retired.invoke_ok(0);
    assert!(
        child.window().is_visible(),
        "a retained rebound owner cannot retire the successor"
    );
    assert!(std::ptr::eq(
        successor
            .predicate_editor
            .borrow()
            .as_ref()
            .unwrap()
            .window(),
        child.window()
    ));
    let mut gui: settings::GuiSettings = store.read(settings::get).unwrap();
    gui.confirm_exit = true;
    store
        .write(move |writer| {
            // Isolate confirmed owner retirement from shutdown maintenance.
            let mut shutdown: hydrus_store::settings::ShutdownWork =
                hydrus_store::settings::get(writer.conn())?;
            shutdown.action = 0;
            hydrus_store::settings::set(writer.conn(), &shutdown)?;
            settings::set(writer.conn(), &gui)
        })
        .unwrap();
    ui.window().dispatch_event(WindowEvent::CloseRequested);
    assert!(!ui.get_question().is_empty());
    super::active_predicates::drain_predicate_timers_for_observer();
    assert!(
        child.window().is_visible(),
        "pending exit must preserve the draft"
    );
    assert!(successor.predicate_editor.borrow().is_some());
    child.invoke_number_edited(0, 2, 99);
    child.invoke_ok(0);
    assert_eq!(fields(&child).row_data(2).unwrap().value, 200);
    assert!(successor.current.borrow().borrow().predicates().is_empty());
    ui.invoke_answer(false);
    super::active_predicates::drain_predicate_timers_for_observer();
    assert!(successor.predicate_editor.borrow().is_some());
    assert!(child.window().is_visible());
    child.invoke_number_edited(0, 2, 201);
    assert_eq!(fields(&child).row_data(2).unwrap().value, 201);
    child.invoke_number_edited(0, 2, 200);
    ui.window().dispatch_event(WindowEvent::CloseRequested);
    ui.invoke_answer(true);
    assert!(!child.window().is_visible());
    ui.show().unwrap();
    child.show().unwrap();
    child.invoke_number_edited(0, 2, 99);
    child.invoke_ok(0);
    assert!(successor.current.borrow().borrow().predicates().is_empty());
}

#[test]
fn destroyed_main_automatically_closes_new_size_while_bound_child_and_collector_survive() {
    let (_guards, store) = store();
    store
        .write(|writer| {
            settings::set(
                writer.conn(),
                &hydrus_store::radio_return::RadioReturn {
                    force_dialog_ok: true,
                },
            )
        })
        .unwrap();
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(&ui, Pages::single(SearchPage::new(store.clone())));
    let current = bound.current.borrow().clone();
    let before = current.borrow().active_predicates().to_vec();
    let saved = super::active_predicates::predicate_settings(&store);
    let child = open(&ui, &bound);
    assert!(!child.get_editing_existing());
    assert!(
        child.invoke_force_radio_ok(0),
        "the live Return route is enabled"
    );
    child.invoke_number_edited(0, 2, 211);
    assert_eq!(fields(&child).row_data(2).unwrap().value, 211);
    let weak_main = ui.as_weak();
    ui.hide().unwrap();
    drop(ui);
    assert!(weak_main.upgrade().is_none());
    super::active_predicates::drain_predicate_timers_until(
        "Main death must close a new size draft",
        || bound.predicate_editor.borrow().is_none() && !child.window().is_visible(),
    );
    assert!(windows.count() >= 2);
    assert_eq!(current.borrow().active_predicates(), before);
    assert_eq!(super::active_predicates::predicate_settings(&store), saved);

    child.show().unwrap();
    child.invoke_number_edited(0, 2, 99);
    child.invoke_defaults_action(0, "set this as new default".into());
    assert_eq!(child.get_radio_default_panel(), 0);
    assert!(
        !child.invoke_force_radio_ok(0),
        "retired Return admission is refused"
    );
    for event in [
        WindowEvent::KeyPressed {
            text: Key::Return.into(),
        },
        WindowEvent::KeyReleased {
            text: Key::Return.into(),
        },
    ] {
        child.window().dispatch_event(event);
    }
    child.invoke_ok(0);
    child.invoke_cancel();
    assert_eq!(fields(&child).row_data(2).unwrap().value, 211);
    assert!(bound.predicate_editor.borrow().is_none());
    assert_eq!(current.borrow().active_predicates(), before);
    assert_eq!(super::active_predicates::predicate_settings(&store), saved);
    child.hide().unwrap();
}

#[test]
fn only_final_bound_drop_retires_size_and_old_timers_cannot_retire_a_live_successor() {
    let (_guards, store) = store();
    store
        .write(|writer| {
            settings::set(
                writer.conn(),
                &hydrus_store::radio_return::RadioReturn {
                    force_dialog_ok: true,
                },
            )
        })
        .unwrap();
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(&ui, Pages::single(SearchPage::new(store.clone())));
    let survivor = bound.clone();
    let old = open(&ui, &bound);
    assert!(old.invoke_force_radio_ok(0));
    old.invoke_number_edited(0, 2, 211);
    let current = bound.current.borrow().clone();
    let slot = bound.predicate_editor.clone();
    let before = current.borrow().active_predicates().to_vec();
    let saved = super::active_predicates::predicate_settings(&store);
    let stale_launch = ui
        .get_suggestions()
        .iter()
        .position(|row| row.text == "system:filesize")
        .unwrap();
    drop(bound);
    super::active_predicates::drain_predicate_timers_for_observer();
    assert!(
        old.window().is_visible(),
        "a nonfinal Bound clone keeps its owner live"
    );
    assert!(std::ptr::eq(
        slot.borrow().as_ref().unwrap().window(),
        old.window()
    ));
    old.invoke_number_edited(0, 2, 212);
    assert_eq!(fields(&old).row_data(2).unwrap().value, 212);

    drop(survivor);
    super::active_predicates::drain_predicate_timers_until(
        "final Bound drop must retire its child",
        || slot.borrow().is_none() && !old.window().is_visible(),
    );
    assert!(
        ui.as_weak().upgrade().is_some(),
        "Main itself remains alive"
    );
    assert!(windows.count() >= 2, "external adapters still survive");
    ui.invoke_suggestion_chosen(i32::try_from(stale_launch).unwrap());
    assert!(
        slot.borrow().is_none(),
        "retired launch callback cannot recreate the slot"
    );
    old.show().unwrap();
    old.invoke_number_edited(0, 2, 99);
    old.invoke_defaults_action(0, "set this as new default".into());
    old.invoke_defaults_action(0, "reset to original default".into());
    assert!(!old.invoke_force_radio_ok(0));
    for event in [
        WindowEvent::KeyPressed {
            text: Key::Return.into(),
        },
        WindowEvent::KeyReleased {
            text: Key::Return.into(),
        },
    ] {
        old.window().dispatch_event(event);
    }
    old.invoke_ok(0);
    assert_eq!(fields(&old).row_data(2).unwrap().value, 212);
    assert_eq!(current.borrow().active_predicates(), before);
    assert_eq!(super::active_predicates::predicate_settings(&store), saved);

    let successor = bind(&ui, Pages::single(SearchPage::new(store.clone())));
    let live = open(&ui, &successor);
    super::active_predicates::drain_predicate_timers_for_observer();
    old.invoke_cancel();
    old.invoke_ok(0);
    assert!(
        live.window().is_visible(),
        "retired owner/timer cannot hide a successor"
    );
    assert!(std::ptr::eq(
        successor
            .predicate_editor
            .borrow()
            .as_ref()
            .unwrap()
            .window(),
        live.window()
    ));
    assert!(slot.borrow().is_none());
    assert_eq!(current.borrow().active_predicates(), before);
    assert_eq!(super::active_predicates::predicate_settings(&store), saved);
    live.invoke_number_edited(0, 2, 123);
    live.invoke_ok(0);
    assert_eq!(
        successor.current.borrow().borrow().predicates(),
        ["system:filesize < 123KB"]
    );
    assert!(successor.predicate_editor.borrow().is_none());
    old.hide().unwrap();
}
