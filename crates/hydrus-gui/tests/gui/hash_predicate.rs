//! Real hash editor drafts, owned warnings and typed query publication.
use hydrus_core::{ServiceKey, service::builtin_keys};
use hydrus_gui::predicate_editors::defaults::CustomDefaults;
use hydrus_gui::{MainWindow, Pages, PredicateEditorWindow, SearchPage, bind, headless};
use hydrus_store::{Store, import::import_legacy, settings};
use slint::{
    ComponentHandle as _, Model as _,
    platform::{Key, PointerEventButton, WindowAdapter as _, WindowEvent},
};
use std::{cell::RefCell, rc::Rc, sync::Arc};

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
        .position(|s| s.text == "system:hash")
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
    let panel = window.get_panels().row_data(0).unwrap();
    assert!(panel.hash_layout);
    panel.fields
}
fn key(window: &slint::Window, text: slint::SharedString) {
    window.dispatch_event(WindowEvent::KeyPressed { text: text.clone() });
    window.dispatch_event(WindowEvent::KeyReleased { text });
}
#[test]
fn all_types_signs_and_empty_hashes_publish_the_recorded_queries() {
    let (guards, store) = store();
    let fixture = hydrus_testkit::fixture_json("hash_predicate.json");
    store
        .write(|writer| {
            settings::set(
                writer.conn(),
                &CustomDefaults {
                    predicates: vec![hydrus_search::Predicate::System(
                        hydrus_search::SystemPredicate::Limit(37),
                    )],
                },
            )
        })
        .unwrap();
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let mut page = SearchPage::new(store.clone());
    page.choose_location(hydrus_search::LocationContext::single(ServiceKey::new(
        builtin_keys::MY_FILES.to_vec(),
    )));
    let bound = bind(&ui, Pages::single(page));
    for case in fixture["queries"].as_array().unwrap() {
        let window = open(&ui, &bound);
        let kind = fixture["types"]
            .as_array()
            .unwrap()
            .iter()
            .position(|t| t == &case["type"])
            .unwrap();
        window.invoke_chose(0, 1, i32::from(!case["inclusive"].as_bool().unwrap()));
        window.invoke_chose(0, 5, i32::try_from(kind).unwrap());
        window.invoke_text_edited(0, 2, case["text"].as_str().unwrap().into());
        assert_eq!(window.get_hash_text(), case["text"].as_str().unwrap());
        assert_eq!(fields(&window).row_data(5).unwrap().kind, 9);
        window.invoke_ok(0);
        assert!(bound.predicate_editor.borrow().is_none());
        let files = bound.current.borrow().borrow().files();
        let mut hashes = store
            .read(move |conn| hydrus_store::media::load_basic(conn, &files))
            .unwrap()
            .into_iter()
            .map(|m| m.hash.to_string())
            .collect::<Vec<_>>();
        hashes.sort();
        assert_eq!(
            serde_json::json!(hashes),
            case["query_hashes"],
            "{}",
            case["type"]
        );
        ui.invoke_remove_predicate(0);
    }
    // Save a typed MD5 default; Cancel does not save a later SHA1 draft.
    let window = open(&ui, &bound);
    window.invoke_chose(0, 1, 1);
    window.invoke_chose(0, 5, 1);
    window.invoke_text_edited(0, 2, fixture["known"]["md5"][0].as_str().unwrap().into());
    window.invoke_defaults_action(0, "set this as new default".into());
    window.invoke_cancel();
    let saved: CustomDefaults = store.read(settings::get).unwrap();
    assert!(saved.predicates.contains(&hydrus_search::Predicate::System(
        hydrus_search::SystemPredicate::Limit(37)
    )));
    assert_eq!(
        Store::open(guards[1].path())
            .unwrap()
            .read(settings::get::<CustomDefaults>)
            .unwrap(),
        saved
    );
    let reopened = open(&ui, &bound);
    assert_eq!(fields(&reopened).row_data(1).unwrap().chosen, 1);
    assert_eq!(fields(&reopened).row_data(5).unwrap().chosen, 1);
    assert_eq!(
        reopened.get_hash_text(),
        fixture["known"]["md5"][0].as_str().unwrap()
    );
    reopened.invoke_chose(0, 5, 2);
    reopened.invoke_text_edited(0, 2, "bad draft".into());
    reopened.invoke_cancel();
    reopened.invoke_ok(0);
    assert_eq!(store.read(settings::get::<CustomDefaults>).unwrap(), saved);
    assert!(bound.current.borrow().borrow().predicates().is_empty());
}
#[test]
fn cleanup_replaces_real_typed_text_and_owns_warning_acknowledgement() {
    let (_guards, store) = store();
    let fixture = hydrus_testkit::fixture_json("hash_predicate.json");
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(&ui, Pages::single(SearchPage::new(store.clone())));
    let window = open(&ui, &bound);
    let geometry = Rc::new(RefCell::new(std::collections::BTreeMap::new()));
    window.on_hash_field_placed({
        let geometry = geometry.clone();
        move |field, x, y, w, h| {
            geometry.borrow_mut().insert(field, (x, y, w, h));
        }
    });
    let native = windows.get(windows.count() - 1).unwrap();
    let pixels = headless::render(&native, 1000, 500);
    headless::save_png(
        &std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("hash-predicate-native.png"),
        &pixels,
        1000,
        500,
    )
    .unwrap();
    assert_eq!(fields(&window).row_data(1).unwrap().kind, 9);
    // The initial sign group has real keyboard focus and clamps at both edges.
    key(window.window(), Key::UpArrow.into());
    assert_eq!(fields(&window).row_data(1).unwrap().chosen, 0);
    key(window.window(), Key::DownArrow.into());
    assert_eq!(fields(&window).row_data(1).unwrap().chosen, 1);
    key(window.window(), Key::DownArrow.into());
    assert_eq!(fields(&window).row_data(1).unwrap().chosen, 1);
    window.invoke_chose(0, 1, 0);
    let (x, y, w, h) = geometry.borrow()[&5];
    assert!(w > 30.0 && h > 80.0);
    let position = slint::LogicalPosition::new(x + 12.0, y + 16.0);
    window.window().dispatch_event(WindowEvent::PointerPressed {
        position,
        button: PointerEventButton::Left,
    });
    window
        .window()
        .dispatch_event(WindowEvent::PointerReleased {
            position,
            button: PointerEventButton::Left,
        });
    for step in fixture["keys"].as_array().unwrap() {
        let text = match step["action"].as_str().unwrap() {
            "up" | "up-edge" => Key::UpArrow.into(),
            "down" | "down-edge" => Key::DownArrow.into(),
            "space" => " ".into(),
            _ => panic!("key"),
        };
        key(window.window(), text);
        let row = fields(&window).row_data(5).unwrap();
        assert_eq!(
            row.options
                .row_data(usize::try_from(row.chosen).unwrap())
                .unwrap(),
            step["type"].as_str().unwrap()
        );
    }
    window.invoke_chose(0, 5, 0);
    // Physical typing breaks a one-way binding; the live two-way draft must clear.
    let (x, y, w, h) = geometry.borrow()[&2];
    assert!(w >= 420.0 && h >= 220.0);
    let position = slint::LogicalPosition::new(x + 30.0, y + 30.0);
    window.window().dispatch_event(WindowEvent::PointerPressed {
        position,
        button: PointerEventButton::Left,
    });
    window
        .window()
        .dispatch_event(WindowEvent::PointerReleased {
            position,
            button: PointerEventButton::Left,
        });
    for text in ["n", "o", "t", " ", "h", "e", "x"] {
        key(window.window(), text.into());
    }
    assert_eq!(window.get_hash_text(), "not hex");
    window.invoke_pressed(0, 4);
    assert_eq!(window.get_question(), "You sure?");
    window.invoke_answer(true);
    assert_eq!(window.get_hash_text(), "");
    assert_eq!(fields(&window).row_data(2).unwrap().text, "");
    // Inspect the actual TextEdit through its next physical edit, rather than
    // accepting cleared model/getter values while an old visible draft survives.
    window.window().dispatch_event(WindowEvent::PointerPressed {
        position,
        button: PointerEventButton::Left,
    });
    window
        .window()
        .dispatch_event(WindowEvent::PointerReleased {
            position,
            button: PointerEventButton::Left,
        });
    key(window.window(), "x".into());
    assert_eq!(window.get_hash_text(), "x");
    window.invoke_pressed(0, 4);
    window.invoke_answer(true);
    assert_eq!(window.get_hash_text(), "");
    for case in fixture["cleanup"].as_array().unwrap() {
        window.invoke_chose(0, 5, 3);
        window.invoke_text_edited(0, 2, case["before"]["text"].as_str().unwrap().into());
        let forced = case["action"] == "forced";
        window.invoke_pressed(0, if forced { 4 } else { 3 });
        if forced {
            assert_eq!(
                window.get_question(),
                case["questions"][0].as_str().unwrap()
            );
            window.invoke_answer(case["yes"].as_bool().unwrap());
        } else if let Some(warning) = case["warnings"].as_array().unwrap().first() {
            assert!(window.get_notice_open());
            assert_eq!(window.get_notice_message(), warning.as_str().unwrap());
            let notice = windows.get(windows.count() - 1).unwrap();
            let before = window.get_hash_text();
            window.invoke_text_edited(0, 2, "blocked".into());
            window.invoke_ok(0);
            assert_eq!(window.get_hash_text(), before);
            assert!(bound.predicate_editor.borrow().is_some());
            notice.window().hide().unwrap();
            key(notice.window(), Key::Return.into());
            assert!(
                window.get_notice_open(),
                "hidden warning cannot acknowledge the parent"
            );
            notice.window().show().unwrap();
            key(notice.window(), Key::Return.into());
            assert!(!window.get_notice_open());
        }
        assert_eq!(
            window.get_hash_text(),
            case["after"]["text"].as_str().unwrap(),
            "{}",
            case["name"]
        );
        assert_eq!(
            fields(&window).row_data(2).unwrap().text,
            case["after"]["text"].as_str().unwrap()
        );
        let types = fields(&window).row_data(5).unwrap();
        assert_eq!(
            types
                .options
                .row_data(usize::try_from(types.chosen).unwrap())
                .unwrap(),
            case["after"]["type"].as_str().unwrap()
        );
    }
    let text = window.get_hash_text();
    let kind = fields(&window).row_data(5).unwrap().chosen;
    window.hide().unwrap();
    window.invoke_chose(0, 5, 0);
    window.invoke_text_edited(0, 2, "hidden draft".into());
    window.invoke_pressed(0, 4);
    window.invoke_ok(0);
    assert_eq!(window.get_hash_text(), text);
    assert_eq!(fields(&window).row_data(5).unwrap().chosen, kind);
    assert!(window.get_question().is_empty());
    assert!(!window.get_notice_open());
    window.show().unwrap();
    window.invoke_text_edited(0, 2, "not hex".into());
    window.invoke_ok(0);
    assert!(window.get_notice_open());
    assert_eq!(
        window.get_notice_message(),
        fixture["acceptance"][1]["warnings"][0].as_str().unwrap()
    );
    let notice = windows.get(windows.count() - 1).unwrap();
    let pixels = headless::render(&notice, 550, 330);
    headless::save_png(
        &std::path::Path::new(env!("CARGO_TARGET_TMPDIR"))
            .join("hash-predicate-warning-native.png"),
        &pixels,
        550,
        330,
    )
    .unwrap();
    let successor = bind(&ui, Pages::single(SearchPage::new(store.clone())));
    assert!(!window.window().is_visible());
    assert!(!notice.window().is_visible());
    window.show().unwrap();
    window.invoke_ok(0);
    assert!(successor.current.borrow().borrow().predicates().is_empty());
    assert!(!window.get_notice_open());
    window.hide().unwrap();
    // Accepted main close permanently cancels the successor's warning and draft.
    let child = open(&ui, &successor);
    child.invoke_text_edited(0, 2, "bad input".into());
    child.invoke_pressed(0, 3);
    assert!(child.get_notice_open());
    let notice = windows.get(windows.count() - 1).unwrap();
    let mut gui: settings::GuiSettings = store.read(settings::get).unwrap();
    gui.confirm_exit = true;
    store
        .write(move |writer| settings::set(writer.conn(), &gui))
        .unwrap();
    ui.window().dispatch_event(WindowEvent::CloseRequested);
    assert!(!ui.get_question().is_empty());
    key(notice.window(), Key::Return.into());
    assert!(child.get_notice_open());
    ui.invoke_answer(false);
    assert!(child.get_notice_open());
    ui.window().dispatch_event(WindowEvent::CloseRequested);
    ui.invoke_answer(true);
    assert!(!child.window().is_visible());
    assert!(!notice.window().is_visible());
    child.show().unwrap();
    child.invoke_ok(0);
    assert!(successor.current.borrow().borrow().predicates().is_empty());
    child.hide().unwrap();
}
