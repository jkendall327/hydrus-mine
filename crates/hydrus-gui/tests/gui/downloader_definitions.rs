//! Definition list and child editor callbacks on a real native store, with
//! rendering and persistence checks independent of the Python client.

use hydrus_core::url::{AnyGug, GugOptions, UrlClassSettings};
use hydrus_gui::downloader_definitions_window::{self as windows, Slots};
use hydrus_gui::{DownloaderDefinitionEditWindow, headless};
use hydrus_legacy::objects::domain;
use hydrus_legacy::serialisable::SerialisableObject;
use hydrus_parse::Downloaders;
use hydrus_store::{Store, settings};
use slint::{ComponentHandle as _, Model as _};

fn child(
    slot: &std::rc::Rc<std::cell::RefCell<Option<DownloaderDefinitionEditWindow>>>,
) -> DownloaderDefinitionEditWindow {
    slot.borrow().as_ref().unwrap().clone_strong()
}

fn screenshot(
    windows: &headless::Windows,
    which: usize,
    name: &str,
    editor: &DownloaderDefinitionEditWindow,
) {
    // Invoking edit callbacks bypasses the widgets' own typed-in values.
    // Reload fields before rendering so the image shows the edited draft.
    editor.invoke_tab_chosen(editor.get_tab());
    let window = windows.get(which).unwrap();
    headless::render(&window, 950, 740);
    assert!(
        editor.get_footer_y() >= 0.0 && editor.get_footer_y() + editor.get_footer_height() <= 740.0,
        "footer {} + {} is outside the 740px render; layout height {}",
        editor.get_footer_y(),
        editor.get_footer_height(),
        editor.get_layout_height()
    );
    let pixels = headless::render(&window, 950, 740);
    assert!(
        (698..740).any(|row| (800..950).any(|column| {
            let offset = (row * 950 + column) * 4;
            pixels[offset..offset + 4] != pixels[..4]
        })),
        "footer buttons must be painted inside the 740px render"
    );
    assert!(pixels.chunks_exact(4).any(|pixel| pixel != &pixels[..4]));
    if let Ok(directory) = std::env::var("HYDRUS_DEFINITION_SCREENSHOTS") {
        headless::save_png(
            &std::path::Path::new(&directory).join(name),
            &pixels,
            950,
            740,
        )
        .unwrap();
    }
}

// leaf: audit-network-definitions-url-check
#[test]
fn class_rules_apply_snapshot_roundtrip_and_cancel_safety() {
    let fixture = hydrus_testkit::fixture_json("downloader_definitions.json");
    let object = SerialisableObject::from_tuple_str(&fixture["class"].to_string()).unwrap();
    let class = domain::url_class(&object).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let original = UrlClassSettings {
        url_classes: vec![class.clone()],
        ..Default::default()
    };
    let settings = original.clone();
    store
        .write_and_refresh(move |ctx| settings::set(ctx.conn(), &settings))
        .unwrap();
    let rendered = headless::init();
    let slots = Slots::default();
    let list = windows::open(&store, &slots, true).unwrap();
    list.invoke_row_clicked(0, false, false);
    list.invoke_action("edit".into());
    let edit = child(&slots.class_edit);
    list.invoke_action("apply".into());
    assert!(
        slots.classes.borrow().is_some(),
        "the parent cannot Apply an unfinished child draft"
    );
    assert!(edit.get_preview().contains("Example matches ok!"));
    edit.invoke_text_edited(0, "renamed gallery".into());
    edit.invoke_text_edited(40, "https://wrong.example/search".into());
    edit.invoke_action("apply".into());
    assert_eq!(
        edit.get_error(),
        "Please enter an example url that matches the given rules!"
    );
    assert!(slots.class_edit.borrow().is_some());
    edit.invoke_text_edited(
        40,
        "http://www.example.com/search?page=2&token=secret&extra=gone#fragment".into(),
    );
    assert_eq!(edit.get_error(), "");
    edit.invoke_rule_selected(0);
    edit.invoke_rule_action("edit".into());
    let path = child(&slots.rule);
    assert!(edit.get_child_open());
    path.invoke_toggled(2, true);
    path.invoke_text_edited(1, "wrong".into());
    path.invoke_action("apply".into());
    assert_eq!(
        path.get_error(),
        "That default value does not match the rule!"
    );
    path.invoke_action("cancel".into());
    assert!(!edit.get_child_open());
    edit.invoke_rule_tab_chosen(1);
    edit.invoke_rule_selected(0);
    edit.invoke_rule_action("edit".into());
    let rule = child(&slots.rule);
    rule.invoke_text_edited(0, "".into());
    rule.invoke_action("apply".into());
    assert_eq!(rule.get_error(), "Sorry, you have to set a key/name!");
    rule.invoke_text_edited(0, "token".into());
    rule.invoke_action("apply".into());
    assert_eq!(
        rule.get_error(),
        "Sorry, your key/name already exists, pick something else!"
    );
    rule.invoke_text_edited(0, "page".into());
    rule.invoke_toggled(2, true);
    rule.invoke_text_edited(1, "5".into());
    rule.invoke_action("apply".into());
    assert!(slots.rule.borrow().is_none());
    assert!(!edit.get_child_open());
    edit.invoke_choice_edited(2, 0);
    edit.invoke_tab_chosen(1);
    edit.invoke_toggled(11, false);
    edit.invoke_text_edited(22, "4".into());
    assert!(
        edit.get_preview()
            .contains("next gallery page: http://example.com/search?page=6")
    );
    screenshot(&rendered, 1, "url-class-options.png", &edit);
    edit.invoke_action("apply".into());
    assert!(slots.class_edit.borrow().is_none());
    assert_eq!(
        store.read::<UrlClassSettings>(settings::get).unwrap(),
        original,
        "inner Apply is staged"
    );
    list.invoke_action("apply".into());
    assert!(slots.classes.borrow().is_none());
    let saved: UrlClassSettings = store.read(settings::get).unwrap();
    assert_eq!(saved.url_classes[0].name, "renamed gallery");
    assert_eq!(saved.url_classes[0].key, class.key);
    assert_eq!(saved.url_classes[0].parameters[0].default, Some("5".into()));
    assert_eq!(
        store
            .snapshot()
            .url_classes
            .class_for(&class.example_url)
            .unwrap()
            .name,
        "renamed gallery"
    );
    assert_eq!(
        store
            .snapshot()
            .url_classes
            .normalise(&class.example_url, true)
            .unwrap(),
        "http://example.com/search?page=2&token=secret"
    );
    let list = windows::open(&store, &slots, true).unwrap();
    list.invoke_test_edited("https://example.com/search?page=3".into());
    assert_eq!(list.get_test_result(), "Matches \"renamed gallery\"");
    list.invoke_action("duplicate".into());
    let edit = child(&slots.class_edit);
    edit.invoke_tab_chosen(0);
    screenshot(
        &rendered,
        rendered.count() - 1,
        "url-class-matching.png",
        &edit,
    );
    edit.invoke_rule_selected(0);
    edit.invoke_rule_action("delete".into());
    edit.invoke_rule_action("edit".into());
    assert!(
        slots.rule.borrow().is_none(),
        "deleted rule selection cannot open a stale editor"
    );
    edit.invoke_action("apply".into());
    assert_eq!(list.get_rows().row_count(), 2);
    list.invoke_action("cancel".into());
    assert_eq!(
        list.get_question(),
        "You have made changes. Sure you are ok to cancel?"
    );
    list.invoke_answered(false);
    assert!(slots.classes.borrow().is_some());
    list.invoke_action("cancel".into());
    list.invoke_answered(true);
    assert_eq!(
        store.read::<UrlClassSettings>(settings::get).unwrap(),
        saved
    );
    drop(store);
    let reopened = Store::open(dir.path()).unwrap();
    assert_eq!(
        reopened.read::<UrlClassSettings>(settings::get).unwrap(),
        saved
    );
}

// leaf: gug-members
// leaf: audit-network-gugs-test
// leaf: audit-network-gugs-delete-dependencies
#[test]
fn single_nested_generators_roundtrip_delete_and_cancel() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let rendered = headless::init();
    let slots = Slots::default();
    let list = windows::open(&store, &slots, false).unwrap();
    list.invoke_action("add".into());
    let edit = child(&slots.gug_edit);
    edit.invoke_text_edited(0, "tag search".into());
    edit.invoke_text_edited(2, "".into());
    edit.invoke_action("apply".into());
    assert_eq!(
        edit.get_error(),
        "Please ensure your generator can make an example url!"
    );
    edit.invoke_text_edited(2, "%tags%".into());
    assert_eq!(edit.get_error(), "");
    edit.invoke_text_edited(5, "blue_eyes 6+girls".into());
    assert!(edit.get_preview().contains("q=blue_eyes+6%2Bgirls"));
    screenshot(&rendered, 1, "gallery-url-generator.png", &edit);
    edit.invoke_action("apply".into());
    assert_eq!(list.get_rows().row_count(), 1);
    list.invoke_action("duplicate".into());
    child(&slots.gug_edit).invoke_action("apply".into());
    assert_eq!(list.get_rows().row_count(), 2);
    list.invoke_tab_chosen(1);
    list.invoke_action("add".into());
    let nested = child(&slots.gug_edit);
    nested.invoke_text_edited(0, "combined search".into());
    nested.invoke_member_toggled(0);
    nested.invoke_member_toggled(1);
    assert_eq!(nested.get_members().row_count(), 2);
    screenshot(
        &rendered,
        rendered.count() - 1,
        "nested-gallery-url-generator.png",
        &nested,
    );
    nested.invoke_action("apply".into());
    assert_eq!(
        store
            .read::<Downloaders>(settings::get)
            .unwrap()
            .gugs
            .gugs
            .len(),
        0
    );
    list.invoke_action("apply".into());
    let saved: Downloaders = store.read(settings::get).unwrap();
    assert_eq!(saved.gugs.gugs.len(), 3);
    assert_ne!(saved.gugs.gugs[0].key(), saved.gugs.gugs[1].key());
    let AnyGug::Nested(n) = &saved.gugs.gugs[2] else {
        panic!("nested")
    };
    assert_eq!(n.gugs.len(), 2);
    assert_eq!(
        saved
            .gugs
            .gallery_urls(&saved.gugs.gugs[2], "red hair", GugOptions::default())
            .unwrap()
            .len(),
        2
    );
    let list = windows::open(&store, &slots, false).unwrap();
    list.invoke_row_clicked(0, false, false);
    list.invoke_action("delete".into());
    assert_eq!(list.get_question(), "Remove all selected?");
    list.invoke_answered(true);
    // (the reference then asks about the nested generator that uses it)
    assert!(list.get_question().contains("combined search"));
    assert!(list.get_question().starts_with("The GUG \""));
    list.invoke_answered(false);
    assert_eq!(list.get_rows().row_count(), 2, "declined: it stays");
    list.invoke_action("delete".into());
    list.invoke_answered(false);
    assert_eq!(list.get_rows().row_count(), 2);
    list.invoke_action("delete".into());
    list.invoke_answered(true);
    list.invoke_answered(true);
    assert_eq!(list.get_rows().row_count(), 1);
    list.invoke_action("cancel".into());
    list.invoke_answered(true);
    assert_eq!(store.read::<Downloaders>(settings::get).unwrap(), saved);
    let list = windows::open(&store, &slots, false).unwrap();
    list.invoke_row_clicked(0, false, false);
    list.invoke_action("delete".into());
    list.invoke_answered(true);
    list.invoke_answered(true);
    list.invoke_action("apply".into());
    let after: Downloaders = store.read(settings::get).unwrap();
    let AnyGug::Nested(n) = &after.gugs.gugs[1] else {
        panic!("nested")
    };
    assert_eq!(n.gugs.len(), 1, "list Apply repairs deleted members");
    drop(store);
    assert_eq!(
        Store::open(dir.path())
            .unwrap()
            .read::<Downloaders>(settings::get)
            .unwrap(),
        after
    );
}

#[test]
fn lifecycle_string_descendants_cancel_without_orphaning_definition_drafts() {
    use slint::platform::WindowEvent;

    let fixture = hydrus_testkit::fixture_json("downloader_definitions.json");
    let object = SerialisableObject::from_tuple_str(&fixture["class"].to_string()).unwrap();
    let class = domain::url_class(&object).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let original = UrlClassSettings {
        url_classes: vec![class],
        ..Default::default()
    };
    let settings = original.clone();
    store
        .write_and_refresh(move |ctx| settings::set(ctx.conn(), &settings))
        .unwrap();
    let _headless_windows = headless::init();
    let slots = Slots::default();

    for wm_close in [false, true] {
        let list = windows::open(&store, &slots, true).unwrap();
        list.invoke_row_clicked(0, false, false);
        list.invoke_action("edit".into());
        let edit = child(&slots.class_edit);
        assert!(!edit.get_child_open());
        let original_fields = edit.get_fields();
        let preview = edit.get_preview();
        let rules = edit.get_rules().row_count();
        edit.invoke_rule_selected(0);
        edit.invoke_converter(0);
        assert!(edit.get_child_open());
        let converter = slots
            .strings
            .converter
            .borrow()
            .as_ref()
            .unwrap()
            .clone_strong();
        converter.invoke_add();
        assert!(slots.strings.conversion.borrow().is_some());

        // The definition draft and confirmed Apply remain modal while the
        // API converter's unfinished conversion is being edited.
        edit.invoke_text_edited(40, "https://wrong.example/blocked".into());
        edit.invoke_choice_edited(2, 0);
        edit.invoke_toggled(11, false);
        edit.invoke_rule_selected(0);
        edit.invoke_rule_action("delete".into());
        edit.invoke_action("apply".into());
        edit.invoke_answered(true);
        edit.invoke_action("cancel".into());
        edit.window().dispatch_event(WindowEvent::CloseRequested);
        list.window().dispatch_event(WindowEvent::CloseRequested);
        assert_eq!(edit.get_preview(), preview);
        assert_eq!(edit.get_rules().row_count(), rules);
        assert!(slots.class_edit.borrow().is_some());
        assert!(slots.classes.borrow().is_some());
        assert_eq!(
            store.read::<UrlClassSettings>(settings::get).unwrap(),
            original
        );

        if wm_close {
            converter
                .window()
                .dispatch_event(WindowEvent::CloseRequested);
        } else {
            converter.invoke_cancel();
        }
        assert!(
            !slots.strings.has_open(),
            "converter cancellation clears its conversion"
        );
        assert!(!edit.get_child_open());
        for i in 0..original_fields.row_count() {
            let original_field = original_fields.row_data(i).unwrap();
            let current = edit.get_fields().row_data(i).unwrap();
            assert_eq!(current.text, original_field.text);
            assert_eq!(current.chosen, original_field.chosen);
            assert_eq!(current.checked, original_field.checked);
        }
        edit.invoke_converter(0);
        assert!(edit.get_child_open());
        let reopened = slots
            .strings
            .converter
            .borrow()
            .as_ref()
            .unwrap()
            .clone_strong();
        reopened.invoke_add();
        assert!(
            slots.strings.conversion.borrow().is_some(),
            "the converter can be reopened"
        );
        reopened.invoke_cancel();
        assert!(!slots.strings.has_open());
        assert!(!edit.get_child_open());
        if wm_close {
            edit.window().dispatch_event(WindowEvent::CloseRequested);
        } else {
            edit.invoke_action("cancel".into());
        }
        assert!(slots.class_edit.borrow().is_none());
        list.invoke_action("cancel".into());
        assert!(slots.classes.borrow().is_none());
        assert_eq!(
            store.read::<UrlClassSettings>(settings::get).unwrap(),
            original
        );
    }

    // A parameter default processor owns another converter/conversion subtree.
    let list = windows::open(&store, &slots, true).unwrap();
    list.invoke_row_clicked(0, false, false);
    list.invoke_action("edit".into());
    let edit = child(&slots.class_edit);
    edit.invoke_rule_tab_chosen(1);
    edit.invoke_rule_selected(0);
    edit.invoke_rule_action("edit".into());
    let rule = child(&slots.rule);
    assert!(edit.get_child_open());
    assert!(!rule.get_child_open());
    rule.invoke_converter(2);
    assert!(rule.get_child_open());
    let processor = slots
        .strings
        .processor
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    processor.invoke_add();
    processor.invoke_chosen(2);
    let converter = slots
        .strings
        .converter
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    converter.invoke_add();
    assert!(slots.strings.conversion.borrow().is_some());
    let preview = rule.get_preview();
    let original_name = rule.get_fields().row_data(0).unwrap().text;
    rule.invoke_text_edited(0, "blocked rename".into());
    rule.invoke_action("apply".into());
    rule.window().dispatch_event(WindowEvent::CloseRequested);
    assert!(slots.rule.borrow().is_some());
    assert_eq!(rule.get_preview(), preview);
    processor
        .window()
        .dispatch_event(WindowEvent::CloseRequested);
    assert!(
        !slots.strings.has_open(),
        "processor close cancels its entire subtree"
    );
    assert!(!rule.get_child_open());
    assert!(
        edit.get_child_open(),
        "the class still has its parameter editor open"
    );
    rule.invoke_tab_chosen(0);
    assert_eq!(rule.get_fields().row_data(0).unwrap().text, original_name);
    rule.invoke_converter(2);
    assert!(slots.strings.processor.borrow().is_some());
    slots.strings.cancel_all();
    assert!(!rule.get_child_open());
    rule.invoke_action("cancel".into());
    assert!(slots.rule.borrow().is_none());
    assert!(!edit.get_child_open());
    edit.invoke_rule_tab_chosen(2);
    edit.invoke_rule_action("add".into());
    assert!(edit.get_child_open());
    let header = child(&slots.rule);
    header.window().dispatch_event(WindowEvent::CloseRequested);
    assert!(slots.rule.borrow().is_none());
    assert!(!edit.get_child_open());
    edit.invoke_action("cancel".into());
    list.invoke_action("cancel".into());
    assert_eq!(
        store.read::<UrlClassSettings>(settings::get).unwrap(),
        original
    );
    assert_eq!(store.snapshot().url_classes.settings(), &original);
}

#[test]
fn lifecycle_native_fields_disable_typing_until_child_closes() {
    use std::cell::RefCell;
    use std::rc::Rc;

    use hydrus_gui::DefinitionField;
    use slint::platform::{Key, WindowEvent};
    use slint::{ModelRc, SharedString, VecModel};

    let rendered = headless::init();
    let editor = DownloaderDefinitionEditWindow::new().unwrap();
    editor.set_fields(ModelRc::new(VecModel::from(vec![DefinitionField {
        id: 0,
        label: "name:".into(),
        text: "original name".into(),
        enabled: true,
        ..Default::default()
    }])));
    let edits = Rc::new(RefCell::new(Vec::new()));
    editor.on_text_edited({
        let edits = edits.clone();
        move |_, text| edits.borrow_mut().push(text.to_string())
    });
    editor.show().unwrap();
    let window = rendered.get(0).unwrap();
    let send_key = |text: SharedString| {
        window.dispatch_event(WindowEvent::KeyPressed { text: text.clone() });
        window.dispatch_event(WindowEvent::KeyReleased { text });
    };
    let type_name = |name: &str| {
        // Native keyboard focus avoids guessing the ScrollView's widget
        // coordinates. The isolated editor's first enabled control is its
        // only LineEdit; with a child open every input control is disabled.
        send_key(Key::Tab.into());
        let control: SharedString = Key::Control.into();
        window.dispatch_event(WindowEvent::KeyPressed {
            text: control.clone(),
        });
        send_key("a".into());
        window.dispatch_event(WindowEvent::KeyReleased { text: control });
        for character in name.chars() {
            send_key(character.to_string().into());
        }
    };
    editor.set_child_open(true);
    window.dispatch_event(WindowEvent::WindowActiveChanged(true));
    headless::render(&window, 950, 740);
    headless::render(&window, 950, 740);
    type_name("blocked name");
    assert!(
        edits.borrow().is_empty(),
        "a disabled native field cannot accept typing"
    );
    editor.set_child_open(false);
    headless::render(&window, 950, 740);
    let pixels = headless::render(&window, 950, 740);
    if let Ok(directory) = std::env::var("HYDRUS_DEFINITION_SCREENSHOTS") {
        headless::save_png(
            &std::path::Path::new(&directory).join("native-definition-field.png"),
            &pixels,
            950,
            740,
        )
        .unwrap();
    }
    type_name("accepted name");
    assert_eq!(
        edits.borrow().last().map(String::as_str),
        Some("accepted name")
    );
    editor.hide().unwrap();
}

// leaf: url-domain
// leaf: url-preview
#[test]
fn domain_mask_modes_tester_and_selectable_previews_apply_cancel_and_reopen() {
    use hydrus_gui_model::downloader_definitions as definitions;
    let _windows = headless::init();
    let fixture = hydrus_testkit::fixture_json("url_domain_preview.json");
    let mut class = domain::url_class(
        &SerialisableObject::from_tuple_str(&fixture["preview_steps"][0]["class"].to_string())
            .unwrap(),
    )
    .unwrap();
    class.domain_mask =
        hydrus_core::url::DomainMask::new(vec!["mask.example".into()], Vec::new(), false, false);
    class.example_url = "https://mask.example/search?page=2".into();
    let original = UrlClassSettings {
        url_classes: vec![class],
        ..UrlClassSettings::default()
    };
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let saved = original.clone();
    store
        .write_and_refresh(move |ctx| settings::set(ctx.conn(), &saved))
        .unwrap();
    let slots = Slots::default();
    let list = windows::open(&store, &slots, true).unwrap();
    list.invoke_row_clicked(0, false, false);
    list.invoke_action("edit".into());
    let edit = child(&slots.class_edit);
    assert_eq!(edit.get_domain_test(), "mask.example");
    for step in fixture["domain_steps"].as_array().unwrap() {
        let mode = i32::try_from(step["mode"].as_i64().unwrap()).unwrap();
        if mode == 1 {
            edit.invoke_choice_edited(30, mode);
        }
        let raw = step["raw"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| s.as_str().unwrap())
            .collect::<Vec<_>>()
            .join("\n");
        edit.set_domain_raw(raw.clone().into());
        edit.invoke_text_edited(3, raw.into());
        let regex = step["regex"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| s.as_str().unwrap())
            .collect::<Vec<_>>()
            .join("\n");
        edit.set_domain_regex(regex.clone().into());
        edit.invoke_text_edited(4, regex.into());
        if mode == 0 {
            edit.invoke_choice_edited(30, mode);
        }
        edit.invoke_toggled(5, true);
        edit.invoke_toggled(6, step["keep"].as_bool().unwrap());
        edit.invoke_toggled(5, step["match"].as_bool().unwrap());
        edit.invoke_text_edited(31, step["test"].as_str().unwrap().into());
        assert_eq!(edit.get_domain_mode(), mode);
        assert_eq!(
            edit.get_domain_mode_enabled(),
            step["mode_enabled"].as_bool().unwrap()
        );
        assert_eq!(edit.get_domain_keep(), step["keep"].as_bool().unwrap());
        assert_eq!(edit.get_domain_status(), step["status"].as_str().unwrap());
        assert_eq!(
            edit.get_domain_normalised(),
            step["normalised"].as_str().unwrap()
        );
        assert_eq!(
            store.read(settings::get::<UrlClassSettings>).unwrap(),
            original
        );
    }
    edit.invoke_action("apply".into());
    assert_eq!(
        edit.get_error(),
        "Please enter an example url that matches the given rules!"
    );
    assert!(slots.class_edit.borrow().is_some());
    edit.invoke_action("cancel".into());
    list.invoke_action("cancel".into());
    assert_eq!(
        store.read(settings::get::<UrlClassSettings>).unwrap(),
        original
    );
    let list = windows::open(&store, &slots, true).unwrap();
    list.invoke_row_clicked(0, false, false);
    list.invoke_action("edit".into());
    let edit = child(&slots.class_edit);
    assert_eq!(edit.get_domain_mode(), 0);
    edit.invoke_choice_edited(30, 1);
    edit.set_domain_raw("mask.example\n".into());
    edit.invoke_text_edited(3, "mask.example\n".into());
    assert_eq!(
        edit.get_domain_raw(),
        "mask.example\n",
        "unfinished newline stays usable for typing another domain"
    );
    edit.invoke_text_edited(3, "mask.example\nsecond.example".into());
    edit.invoke_choice_edited(30, 0);
    assert_eq!(edit.get_domain_mode(), 1);
    edit.invoke_text_edited(4, r"img\d+\.cdn\.example".into());
    edit.invoke_toggled(5, true);
    edit.invoke_toggled(6, true);
    edit.invoke_toggled(5, false);
    edit.invoke_toggled(6, false);
    assert!(
        edit.get_domain_keep(),
        "disabled keep control cannot mutate its retained value"
    );
    edit.invoke_text_edited(40, "https://img3.cdn.example/search?page=2".into());
    edit.invoke_text_edited(31, " www2.second.example ".into());
    assert_eq!(edit.get_domain_status(), "Matches!");
    assert_eq!(edit.get_domain_normalised(), "www2.second.example");
    edit.invoke_action("apply".into());
    list.invoke_action("apply".into());
    let stored = store.read(settings::get::<UrlClassSettings>).unwrap();
    let installed = &stored.url_classes[0];
    assert!(installed.matches("https://img3.cdn.example/search?page=7", false));
    assert_eq!(
        installed
            .normalise("https://www2.second.example/search?page=7", false, false)
            .unwrap(),
        "https://www2.second.example/search?page=7"
    );
    assert!(
        store.snapshot().url_classes.settings().url_classes[0]
            .domain_mask
            .matches("img3.cdn.example")
    );
    let list = windows::open(&store, &slots, true).unwrap();
    list.invoke_row_clicked(0, false, false);
    list.invoke_action("edit".into());
    let edit = child(&slots.class_edit);
    assert_eq!(edit.get_domain_mode(), 1);
    assert!(!edit.get_domain_mode_enabled());
    assert!(edit.get_domain_keep());
    edit.invoke_action("cancel".into());
    list.invoke_action("cancel".into());
    // Replay preview transitions in one retained editor: invalid input keeps the
    // last referral/next values exactly as the actual Qt owner does.
    let saved_class = domain::url_class(
        &SerialisableObject::from_tuple_str(&fixture["preview_steps"][0]["class"].to_string())
            .unwrap(),
    )
    .unwrap();
    store
        .write_and_refresh(move |ctx| {
            settings::set(
                ctx.conn(),
                &UrlClassSettings {
                    url_classes: vec![saved_class],
                    ..UrlClassSettings::default()
                },
            )
        })
        .unwrap();
    let list = windows::open(&store, &slots, true).unwrap();
    list.invoke_row_clicked(0, false, false);
    list.invoke_action("edit".into());
    let edit = child(&slots.class_edit);
    for step in fixture["preview_steps"].as_array().unwrap() {
        let value = domain::url_class(
            &SerialisableObject::from_tuple_str(&step["class"].to_string()).unwrap(),
        )
        .unwrap();
        edit.invoke_text_edited(40, value.example_url.into());
        assert_eq!(edit.get_preview_status(), step["status"].as_str().unwrap());
        assert_eq!(
            serde_json::json!([
                edit.get_preview_normalised().to_string(),
                edit.get_preview_request().to_string(),
                edit.get_preview_api().to_string(),
                edit.get_preview_referral().to_string(),
                edit.get_preview_next().to_string()
            ]),
            step["outputs"]
        );
    }
    edit.invoke_action("cancel".into());
    list.invoke_action("cancel".into());
    for step in fixture["extra_previews"].as_array().unwrap() {
        let value = domain::url_class(
            &SerialisableObject::from_tuple_str(&step["class"].to_string()).unwrap(),
        )
        .unwrap();
        let expected = definitions::class_preview(&value, false);
        let class = value;
        store
            .write_and_refresh(move |ctx| {
                settings::set(
                    ctx.conn(),
                    &UrlClassSettings {
                        url_classes: vec![class],
                        ..UrlClassSettings::default()
                    },
                )
            })
            .unwrap();
        let list = windows::open(&store, &slots, true).unwrap();
        list.invoke_row_clicked(0, false, false);
        list.invoke_action("edit".into());
        let edit = child(&slots.class_edit);
        assert_eq!(
            serde_json::json!([
                edit.get_preview_normalised().to_string(),
                edit.get_preview_request().to_string(),
                edit.get_preview_api().to_string(),
                edit.get_preview_referral().to_string(),
                edit.get_preview_next().to_string()
            ]),
            step["outputs"]
        );
        assert_eq!(edit.get_preview_request(), expected.request);
        assert_eq!(edit.get_preview_referral(), expected.referral);
        edit.invoke_action("cancel".into());
        list.invoke_action("cancel".into());
    }
}

// leaf: url-domain
#[test]
fn full_domain_mask_owned_entries_questions_and_favourites_reach_persisted_consumers() {
    fn rows(model: &slint::ModelRc<hydrus_gui::TableRow>) -> serde_json::Value {
        serde_json::json!(
            (0..model.row_count())
                .map(|i| model
                    .row_data(i)
                    .unwrap()
                    .cells
                    .row_data(0)
                    .unwrap()
                    .to_string())
                .collect::<Vec<_>>()
        )
    }

    use hydrus_gui_model::regex_favourites::RegexFavourites;
    let _windows = headless::init();
    let fixture = hydrus_testkit::fixture_json("domain_mask_queue.json");
    let mut class = hydrus_gui_model::downloader_definitions::new_class();
    class.domain_mask =
        hydrus_core::url::DomainMask::new(vec!["mask.example".into()], Vec::new(), false, false);
    class.example_url = "https://mask.example/post/page.php?id=123456&s=view".into();
    let original = UrlClassSettings {
        url_classes: vec![class],
        ..UrlClassSettings::default()
    };
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let saved = original.clone();
    store
        .write_and_refresh(move |ctx| {
            settings::set(ctx.conn(), &saved)?;
            settings::set(
                ctx.conn(),
                &RegexFavourites(vec![("a+".into(), "letters".into())]),
            )
        })
        .unwrap();
    let slots = Slots::default();
    let list = windows::open(&store, &slots, true).unwrap();
    list.invoke_row_clicked(0, false, false);
    list.invoke_action("edit".into());
    let edit = child(&slots.class_edit);
    edit.invoke_choice_edited(30, 1);
    for step in fixture["steps"].as_array().unwrap() {
        if step["action"] != "initial" {
            let regex = step["regex"].as_bool().unwrap();
            if let Some(indices) = step["indices"].as_array() {
                for (n, index) in indices.iter().enumerate() {
                    edit.invoke_domain_row_clicked(
                        regex,
                        i32::try_from(index.as_i64().unwrap()).unwrap(),
                        n > 0,
                        false,
                    );
                }
            }
            edit.invoke_domain_action(regex, step["action"].as_str().unwrap().into());
            assert!(edit.get_child_open());
            if step["action"] == "delete" {
                let entry = slots
                    .domains
                    .entry
                    .borrow()
                    .as_ref()
                    .unwrap()
                    .clone_strong();
                assert!(!entry.get_asking_name());
                assert_eq!(
                    entry.get_message(),
                    step["said"][0]["question"].as_str().unwrap()
                );
                entry.invoke_answered(step["response"].as_bool().unwrap());
                entry.invoke_answered(true);
            } else {
                for dialog in step["dialogs"].as_array().unwrap() {
                    let entry = slots
                        .domains
                        .entry
                        .borrow()
                        .as_ref()
                        .unwrap()
                        .clone_strong();
                    assert_eq!(entry.get_window_title(), dialog["title"].as_str().unwrap());
                    assert_eq!(entry.get_text(), dialog["initial"].as_str().unwrap());
                    edit.invoke_action("apply".into());
                    assert!(slots.class_edit.borrow().is_some());
                    entry.set_text(dialog["entered"].as_str().unwrap().into());
                    entry.invoke_regex_changed();
                    if dialog["yes"] == true {
                        entry.invoke_name_entered(entry.get_text());
                    }
                    if dialog["accepted"] != true {
                        if dialog["entered"] == "" {
                            assert_eq!(
                                entry.get_warning(),
                                step["said"][0]["warning"].as_str().unwrap()
                            );
                            assert!(slots.domains.has_open());
                        }
                        entry.invoke_cancelled();
                    }
                    entry.invoke_name_entered("stale.example".into());
                }
            }
            assert!(!slots.domains.has_open());
            assert!(!edit.get_child_open());
        }
        assert_eq!(rows(&edit.get_domain_raw_rows()), step["state"]["raw_rows"]);
        assert_eq!(
            rows(&edit.get_domain_regex_rows()),
            step["state"]["regex_rows"]
        );
        assert_eq!(
            edit.get_domain_mode_enabled(),
            step["state"]["mode_enabled"].as_bool().unwrap()
        );
        assert_eq!(
            store.read(settings::get::<UrlClassSettings>).unwrap(),
            original
        );
    }
    edit.invoke_action("cancel".into());
    list.invoke_action("cancel".into());
    let list = windows::open(&store, &slots, true).unwrap();
    list.invoke_row_clicked(0, false, false);
    list.invoke_action("edit".into());
    let edit = child(&slots.class_edit);
    edit.invoke_choice_edited(30, 1);
    edit.invoke_domain_action(true, "add".into());
    let entry = slots
        .domains
        .entry
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    assert!(entry.get_regex_mode());
    entry.set_text("[".into());
    entry.invoke_regex_changed();
    assert!(
        entry
            .get_regex_validity()
            .starts_with("Invalid expression:")
    );
    entry.invoke_name_entered("[".into());
    edit.invoke_action("apply".into());
    assert_eq!(
        edit.get_error(),
        "Please enter an example url that matches the given rules!"
    );
    edit.invoke_domain_row_clicked(true, 0, false, false);
    edit.invoke_domain_action(true, "edit".into());
    let entry = slots
        .domains
        .entry
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    entry.set_text(r"img\d+\.cdn\.example".into());
    let copied = std::rc::Rc::new(std::cell::RefCell::new(Vec::<String>::new()));
    // The real platform records clipboard writes; favourites copy never inserts.
    let clip = copied.clone();
    hydrus_gui::set_clipper(move |value| {
        if let hydrus_gui::Clip::Text(value) = value {
            clip.borrow_mut().push(value.clone());
        }
    });
    entry.invoke_favourite_menu(0.0, 0.0);
    entry.invoke_favourite_line_clicked(0, 4, 0.0, 0.0, 0.0);
    assert_eq!(*copied.borrow(), vec!["a+".to_owned()]);
    assert_eq!(entry.get_text(), r"img\d+\.cdn\.example");
    entry.invoke_favourite_menu(0.0, 0.0);
    entry.invoke_favourite_line_clicked(0, 0, 0.0, 0.0, 0.0);
    let favourites = slots
        .domains
        .favourites
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    assert!(entry.get_child_open());
    favourites.invoke_action("add".into());
    favourites.set_phrase("z+".into());
    favourites.set_description("new choice".into());
    favourites.invoke_action("save-row".into());
    favourites.invoke_action("cancel".into());
    assert!(!entry.get_child_open());
    assert_eq!(
        store
            .read(hydrus_store::regex_favourites::load)
            .unwrap()
            .0
            .len(),
        1
    );
    entry.invoke_favourite_menu(0.0, 0.0);
    entry.invoke_favourite_line_clicked(0, 0, 0.0, 0.0, 0.0);
    let favourites = slots
        .domains
        .favourites
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    favourites.invoke_action("add".into());
    favourites.set_phrase("z+".into());
    favourites.set_description("new choice".into());
    favourites.invoke_action("save-row".into());
    favourites.invoke_action("apply".into());
    assert_eq!(
        store
            .read(hydrus_store::regex_favourites::load)
            .unwrap()
            .0
            .len(),
        2
    );
    entry.invoke_name_entered(entry.get_text());
    edit.invoke_text_edited(
        40,
        "https://img3.cdn.example/post/page.php?id=123456&s=view".into(),
    );
    edit.invoke_action("apply".into());
    list.invoke_action("apply".into());
    let installed = store.snapshot();
    assert!(
        installed.url_classes.settings().url_classes[0]
            .matches("https://img9.cdn.example/post/page.php?id=2&s=view", false)
    );
    let list = windows::open(&store, &slots, true).unwrap();
    list.invoke_row_clicked(0, false, false);
    list.invoke_action("edit".into());
    let edit = child(&slots.class_edit);
    assert_eq!(edit.get_domain_mode(), 1);
    assert_eq!(edit.get_domain_regex_rows().row_count(), 1);
    edit.invoke_domain_action(true, "add".into());
    let stale = slots
        .domains
        .entry
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    stale.invoke_favourite_menu(0.0, 0.0);
    stale.invoke_favourite_line_clicked(0, 0, 0.0, 0.0, 0.0);
    let stale_favourite = slots
        .domains
        .favourites
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    slots.domains.cancel();
    assert!(!slots.domains.has_open());
    assert!(slots.domains.favourites.borrow().is_none());
    stale.invoke_name_entered("stale.example".into());
    stale_favourite.invoke_action("apply".into());
    assert_eq!(edit.get_domain_regex_rows().row_count(), 1);
    edit.invoke_action("cancel".into());
    list.invoke_action("cancel".into());
    edit.invoke_domain_action(false, "add".into());
    assert!(!slots.domains.has_open());
}
