//! Native import/export controls use real stores and isolate pending drafts.
use hydrus_gui::{downloader_interchange_window as windows, headless};
use hydrus_gui_model::downloader_interchange::{self as exchange, Definition, Native};
use hydrus_parse::Downloaders;
use hydrus_store::{Store, settings};
use slint::{ComponentHandle as _, Model as _};
use std::{cell::RefCell, rc::Rc, sync::Arc};
fn setup() -> (tempfile::TempDir, Arc<Store>) {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    (dir, store)
}
fn one_page() -> String {
    let fixture = hydrus_testkit::fixture_json("downloader_interchange.json");
    let all = exchange::decode_text(&fixture["reference"].to_string()).unwrap();
    exchange::encode_text(
        &all.into_iter()
            .filter(|d| matches!(&d.native,Native::Page(p)if p.name=="exchange page"))
            .collect::<Vec<_>>(),
    )
    .unwrap()
}
// leaf: exchange-login
#[test]
fn mixed_registered_login_package_reviews_png_selects_dependencies_and_reopens_saved_scripts() {
    let rendered = headless::init();
    let (dir, store) = setup();
    let fixture = hydrus_testkit::fixture_json("mixed_login_packages.json");
    let slots = windows::Slots::default();
    let import = windows::package(&store, &slots, true).unwrap();
    import.set_path(
        hydrus_testkit::fixture_path("mixed_login_packages.png")
            .to_string_lossy()
            .as_ref()
            .into(),
    );
    import.invoke_action("open".into());
    assert!(import.get_error().is_empty(), "{}", import.get_error());
    assert!(import.get_ready());
    assert!(
        import
            .get_review()
            .contains("Login Script: mixed package login")
    );
    assert!(
        store
            .read(hydrus_store::logins::load)
            .unwrap()
            .scripts
            .is_empty()
    );
    import.invoke_action("cancel".into());
    import.invoke_action("accept".into());
    assert!(
        store
            .read(hydrus_store::logins::load)
            .unwrap()
            .scripts
            .is_empty()
    );
    let import = windows::package(&store, &slots, true).unwrap();
    hydrus_gui::set_paster({
        let text = fixture["reference"].to_string();
        move || text.clone()
    });
    import.invoke_action("paste".into());
    import.invoke_action("review".into());
    assert!(import.get_ready());
    import.invoke_action("accept".into());
    let saved = store.read(hydrus_store::logins::load).unwrap();
    assert_eq!(saved.scripts.len(), 1);
    assert_eq!(saved.scripts[0].name, "mixed package login");
    assert_ne!(saved.scripts[0].key, "71".repeat(32));
    assert!(saved.domains.is_empty());
    let scripts = hydrus_gui::login_workflows_window::Slots::default();
    let list = hydrus_gui::login_workflows_window::open_scripts(&store, &scripts).unwrap();
    assert_eq!(list.get_rows().row_count(), 1);
    assert_eq!(
        list.get_rows()
            .row_data(0)
            .unwrap()
            .cells
            .row_data(0)
            .unwrap(),
        "mixed package login"
    );
    list.invoke_action("cancel".into());
    let export = windows::package(&store, &slots, false).unwrap();
    assert!(export.get_text_read_only());
    assert!(export.get_json_enabled());
    let login = export
        .get_package_choices()
        .iter()
        .position(|row| row.label == "Login Script: mixed package login")
        .unwrap();
    let nested = export
        .get_package_choices()
        .iter()
        .position(|row| row.label == "GUG: mixed package nested")
        .unwrap();
    export.invoke_package_chosen(-1, false);
    assert!(export.get_text().is_empty());
    assert!(export.get_export_empty());
    export.invoke_package_chosen(i32::try_from(login).unwrap(), true);
    let login_only = exchange::decode_text(export.get_text().as_str()).unwrap();
    assert_eq!(
        login_only.len(),
        fixture["login_only"].as_array().unwrap().len()
    );
    assert!(matches!(login_only[0].native, Native::Login(_)));
    export.invoke_package_chosen(i32::try_from(nested).unwrap(), true);
    let mixed = exchange::decode_text(export.get_text().as_str()).unwrap();
    assert_eq!(mixed.len(), fixture["mixed"].as_array().unwrap().len());
    let copied = Rc::new(RefCell::new(String::new()));
    hydrus_gui::set_clipper({
        let copied = copied.clone();
        move |clip| {
            if let hydrus_gui::Clip::Text(text) = clip {
                copied.borrow_mut().clone_from(text);
            }
        }
    });
    export.invoke_action("copy".into());
    assert_eq!(exchange::decode_text(&copied.borrow()).unwrap(), mixed);
    let json_path = dir.path().join("mixed.json");
    export.set_path(json_path.to_string_lossy().as_ref().into());
    export.invoke_action("save-json".into());
    assert!(export.get_error().is_empty(), "{}", export.get_error());
    assert_eq!(
        exchange::decode_text(&std::fs::read_to_string(&json_path).unwrap()).unwrap(),
        mixed
    );
    let png_path = dir.path().join("mixed.png");
    export.set_path(png_path.to_string_lossy().as_ref().into());
    export.invoke_action("save".into());
    assert!(export.get_error().is_empty(), "{}", export.get_error());
    assert_eq!(
        exchange::decode_png(&std::fs::read(&png_path).unwrap()).unwrap(),
        mixed
    );
    let pixels = headless::render(&rendered.get(rendered.count() - 1).unwrap(), 880, 750);
    assert!(pixels.chunks_exact(4).any(|pixel| pixel != &pixels[..4]));
    if let Ok(path) = std::env::var("HYDRUS_INTERCHANGE_SCREENSHOTS") {
        headless::save_png(
            &std::path::Path::new(&path).join("mixed-login-export.png"),
            &pixels,
            880,
            750,
        )
        .unwrap();
    }
    export.invoke_action("cancel".into());
    let retained = export.get_text();
    let clipboard = copied.borrow().clone();
    let retired_path = dir.path().join("retired.json");
    export.set_path(retired_path.to_string_lossy().as_ref().into());
    export.invoke_package_chosen(-1, false);
    export.invoke_action("copy".into());
    export.invoke_action("save-json".into());
    assert_eq!(export.get_text(), retained);
    assert_eq!(*copied.borrow(), clipboard);
    assert!(!retired_path.exists());
    let (_other_dir, other) = setup();
    let import = windows::package(&other, &slots, true).unwrap();
    import.set_path(png_path.to_string_lossy().as_ref().into());
    import.invoke_action("open".into());
    import.invoke_action("accept".into());
    let imported = other.read(hydrus_store::logins::load).unwrap();
    assert_eq!(imported.scripts[0].name, saved.scripts[0].name);
    assert_ne!(imported.scripts[0].key, saved.scripts[0].key);
    assert_eq!(
        other
            .read::<Downloaders>(settings::get)
            .unwrap()
            .parsers
            .len(),
        1
    );
}
// leaf: audit-network-exchange-input
// leaf: audit-network-exchange-review
#[test]
fn package_reviews_real_png_cancel_and_invalid_input_before_atomic_apply() {
    let rendered = headless::init();
    let (_dir, store) = setup();
    let slots = windows::Slots::default();
    let w = windows::package(&store, &slots, true).unwrap();
    w.set_text("bad data".into());
    w.invoke_action("review".into());
    assert!(!w.get_error().is_empty());
    assert!(!w.get_ready());
    w.set_path(
        hydrus_testkit::fixture_path("downloader_interchange.png")
            .to_string_lossy()
            .as_ref()
            .into(),
    );
    w.invoke_action("open".into());
    assert!(!w.get_ready());
    assert!(w.get_error().contains("formulas or content"));
    w.set_text(one_page().into());
    w.invoke_action("review".into());
    assert!(w.get_ready());
    assert!(w.get_review().contains("exchange page"));
    assert!(
        store
            .read::<Downloaders>(settings::get)
            .unwrap()
            .parsers
            .is_empty()
    );
    let pixels = headless::render(&rendered.get(0).unwrap(), 880, 610);
    assert!(pixels.chunks_exact(4).any(|pixel| pixel != &pixels[..4]));
    if let Ok(dir) = std::env::var("HYDRUS_INTERCHANGE_SCREENSHOTS") {
        headless::save_png(
            &std::path::Path::new(&dir).join("downloader-import.png"),
            &pixels,
            880,
            610,
        )
        .unwrap();
    }
    w.invoke_action("cancel".into());
    w.invoke_action("accept".into());
    assert!(
        store
            .read::<Downloaders>(settings::get)
            .unwrap()
            .parsers
            .is_empty()
    );
    let w = windows::package(&store, &slots, true).unwrap();
    w.set_text(one_page().into());
    w.invoke_action("review".into());
    w.invoke_action("accept".into());
    let saved = store.read::<Downloaders>(settings::get).unwrap();
    assert_eq!(saved.parsers.len(), 1);
    assert_eq!(saved.parsers[0].name, "exchange page");
    assert!(slots.0.borrow().is_none());
}
#[test]
fn parser_list_import_and_export_buttons_stage_until_owner_apply() {
    let _headless_windows = headless::init();
    let (_dir, store) = setup();
    let slots = hydrus_gui::parser_editors_window::Slots::default();
    let list = hydrus_gui::parser_editors_window::open(&store, &slots, false).unwrap();
    list.invoke_action("import".into());
    let import = slots.exchange.0.borrow().as_ref().unwrap().clone_strong();
    import.set_text(one_page().into());
    import.invoke_action("review".into());
    import.invoke_action("accept".into());
    assert_eq!(list.get_rows().row_count(), 1);
    assert!(
        store
            .read::<Downloaders>(settings::get)
            .unwrap()
            .parsers
            .is_empty()
    );
    list.invoke_row_clicked(0, false, false);
    list.invoke_action("export".into());
    let export = slots.exchange.0.borrow().as_ref().unwrap().clone_strong();
    let definitions = exchange::decode_text(export.get_text().as_str()).unwrap();
    assert_eq!(definitions[0].name(), "exchange page");
    assert!(export.get_text().contains("preserve child"));
    export.invoke_action("cancel".into());
    list.invoke_action("apply".into());
    assert_eq!(
        store
            .read::<Downloaders>(settings::get)
            .unwrap()
            .parsers
            .len(),
        1
    );
}
#[test]
fn class_list_buttons_preserve_export_format_and_owner_cancel() {
    let _headless_windows = headless::init();
    let (_dir, store) = setup();
    let slots = hydrus_gui::downloader_definitions_window::Slots::default();
    let list = hydrus_gui::downloader_definitions_window::open(&store, &slots, true).unwrap();
    let mut class = hydrus_gui_model::downloader_definitions::new_class();
    class.key = vec![1; 32];
    let text = exchange::encode_text(&[Definition::new(Native::Class(Box::new(class)))]).unwrap();
    list.invoke_action("import".into());
    let import = slots
        .class_exchange
        .0
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    import.set_text(text.into());
    import.invoke_action("review".into());
    import.invoke_action("accept".into());
    assert_eq!(list.get_rows().row_count(), 1);
    list.invoke_action("export".into());
    let export = slots
        .class_exchange
        .0
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    assert!(matches!(
        exchange::decode_text(export.get_text().as_str()).unwrap()[0].native,
        Native::Class(_)
    ));
    export.invoke_action("cancel".into());
    list.invoke_action("cancel".into());
    list.invoke_answered(true);
    assert!(
        store
            .snapshot()
            .url_classes
            .settings()
            .url_classes
            .is_empty()
    );
}
#[test]
fn formula_buttons_keep_imported_auxiliary_through_child_apply() {
    let _headless_windows = headless::init();
    let (_dir, store) = setup();
    let slots = hydrus_gui::formula_window::Slots::default();
    let applied = Rc::new(RefCell::new(None));
    let initial = hydrus_gui_model::formula_editors::new_formula(false);
    let w = hydrus_gui::formula_window::open(
        &store,
        &initial,
        hydrus_gui_model::formula_editors::FormulaTestData::default(),
        &slots,
        Rc::new({
            let applied = applied.clone();
            move |formula| *applied.borrow_mut() = Some(formula)
        }),
    )
    .unwrap();
    w.invoke_exchange(true);
    let import = slots.exchange.0.borrow().as_ref().unwrap().clone_strong();
    import.set_text("[27, 8, [[26, 3, []], 1, \"inert preserved attribute\", \"imported\", [84, 1, [26, 3, []]]]]".into());
    import.invoke_action("review".into());
    import.invoke_action("accept".into());
    assert_eq!(w.get_name(), "imported");
    w.set_name("native edited".into());
    w.invoke_changed();
    w.invoke_apply();
    let formula = applied.borrow_mut().take().unwrap();
    let encoded = Definition::new(Native::Formula(formula)).tuple().unwrap();
    assert_eq!(encoded[2][2], "inert preserved attribute");
    assert_eq!(encoded[2][3], "native edited");
}

// leaf: audit-network-exchange-export
#[test]
fn export_text_is_selectable_read_only_and_png_accepts_a_bare_relative_path() {
    let _headless_windows = headless::init();
    let slots = windows::Slots::default();
    let definitions = exchange::decode_text(&one_page()).unwrap();
    let w = windows::open(
        &slots,
        false,
        &definitions,
        Rc::new(|_| Ok(String::new())),
        Rc::new(|_| Ok(())),
    )
    .unwrap();
    assert!(w.get_text_read_only());
    assert!(!w.get_ready());
    assert_eq!(
        exchange::decode_text(w.get_text().as_str()).unwrap(),
        definitions
    );
    // Close the handle before replacing its directory entry: Windows denies
    // replacement of an open NamedTempFile. TempPath retains cleanup ownership.
    let file = tempfile::NamedTempFile::new_in(std::env::current_dir().unwrap())
        .unwrap()
        .into_temp_path();
    let relative = file.file_name().unwrap().to_str().unwrap();
    assert!(
        std::path::Path::new(relative)
            .parent()
            .unwrap()
            .as_os_str()
            .is_empty()
    );
    w.set_path(relative.into());
    // The initial empty file and a previously exported PNG are both replaced.
    for _ in 0..2 {
        w.invoke_action("save".into());
        assert!(w.get_error().is_empty(), "{}", w.get_error());
        assert_eq!(w.get_review(), "PNG saved.");
        assert_eq!(
            exchange::decode_png(&std::fs::read(&file).unwrap()).unwrap(),
            definitions
        );
    }
    w.invoke_action("cancel".into());
}
#[test]
fn closing_a_sibling_definition_list_keeps_the_owning_import_open() {
    let _headless_windows = headless::init();
    let (_dir, store) = setup();
    let slots = hydrus_gui::downloader_definitions_window::Slots::default();
    let classes = hydrus_gui::downloader_definitions_window::open(&store, &slots, true).unwrap();
    let gugs = hydrus_gui::downloader_definitions_window::open(&store, &slots, false).unwrap();
    classes.invoke_action("import".into());
    let import = slots
        .class_exchange
        .0
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    assert!(!import.get_text_read_only());
    gugs.invoke_action("cancel".into());
    assert!(slots.gugs.borrow().is_none());
    assert!(slots.class_exchange.has_open());
    let mut class = hydrus_gui_model::downloader_definitions::new_class();
    class.key = vec![1; 32];
    import.set_text(
        exchange::encode_text(&[Definition::new(Native::Class(Box::new(class)))])
            .unwrap()
            .into(),
    );
    import.invoke_action("review".into());
    assert!(import.get_ready());
    import.invoke_action("accept".into());
    assert_eq!(classes.get_rows().row_count(), 1);
    classes.invoke_action("apply".into());
    assert_eq!(store.snapshot().url_classes.settings().url_classes.len(), 1);
}

#[test]
fn dropping_the_final_exchange_owner_hides_and_invalidates_a_retained_window_handle() {
    let _headless_windows = headless::init();
    let slots = windows::Slots::default();
    let other_owner = slots.clone();
    let weak_slot = Rc::downgrade(&slots.0);
    let accepted = Rc::new(std::cell::Cell::new(0));
    let closed = Rc::new(std::cell::Cell::new(0));
    let w = windows::open(
        &slots,
        true,
        &[],
        Rc::new(|_| Ok("reviewed".into())),
        Rc::new({
            let accepted = accepted.clone();
            move |_| {
                accepted.set(accepted.get() + 1);
                Ok(())
            }
        }),
    )
    .unwrap();
    w.on_closed({
        let closed = closed.clone();
        move || closed.set(closed.get() + 1)
    });
    w.set_text(one_page().into());
    w.invoke_action("review".into());
    assert!(w.get_ready());
    assert!(w.window().is_visible());
    drop(slots);
    assert!(other_owner.has_open());
    assert!(w.window().is_visible());
    assert_eq!(closed.get(), 0);
    drop(other_owner);
    assert!(
        weak_slot.upgrade().is_none(),
        "window callbacks retained their owner"
    );
    assert!(!w.window().is_visible());
    assert_eq!(closed.get(), 1);
    w.invoke_action("accept".into());
    w.set_text("invalid".into());
    w.invoke_action("review".into());
    w.invoke_action("cancel".into());
    assert_eq!(accepted.get(), 0);
    assert_eq!(closed.get(), 1);
    assert!(w.get_error().is_empty());
}
