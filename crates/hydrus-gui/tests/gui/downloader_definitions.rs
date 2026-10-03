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
    path.invoke_toggled(2, true);
    path.invoke_text_edited(1, "wrong".into());
    path.invoke_action("apply".into());
    assert_eq!(
        path.get_error(),
        "That default value does not match the rule!"
    );
    path.invoke_action("cancel".into());
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
    assert!(list.get_question().contains("combined search"));
    list.invoke_answered(false);
    assert_eq!(list.get_rows().row_count(), 2);
    list.invoke_action("delete".into());
    list.invoke_answered(true);
    assert_eq!(list.get_rows().row_count(), 1);
    list.invoke_action("cancel".into());
    list.invoke_answered(true);
    assert_eq!(store.read::<Downloaders>(settings::get).unwrap(), saved);
    let list = windows::open(&store, &slots, false).unwrap();
    list.invoke_row_clicked(0, false, false);
    list.invoke_action("delete".into());
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
    headless::init();
    let slots = Slots::default();

    for wm_close in [false, true] {
        let list = windows::open(&store, &slots, true).unwrap();
        list.invoke_row_clicked(0, false, false);
        list.invoke_action("edit".into());
        let edit = child(&slots.class_edit);
        let preview = edit.get_preview();
        let rules = edit.get_rules().row_count();
        edit.invoke_rule_selected(0);
        edit.invoke_converter(0);
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
        edit.invoke_converter(0);
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
    rule.invoke_converter(2);
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
    rule.invoke_tab_chosen(0);
    assert_eq!(rule.get_fields().row_data(0).unwrap().text, original_name);
    rule.invoke_converter(2);
    assert!(slots.strings.processor.borrow().is_some());
    slots.strings.cancel_all();
    rule.invoke_action("cancel".into());
    assert!(slots.rule.borrow().is_none());
    edit.invoke_action("cancel".into());
    list.invoke_action("cancel".into());
    assert_eq!(
        store.read::<UrlClassSettings>(settings::get).unwrap(),
        original
    );
    assert_eq!(store.snapshot().url_classes.settings(), &original);
}
