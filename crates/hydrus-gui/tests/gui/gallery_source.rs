//! Options' owned gallery picker, reference groups, persistence and real consumers.
use std::rc::Rc;
use std::sync::Arc;

use hydrus_core::{
    subscriptions::GalleryDefaults,
    url::{Gugs, UrlClassSettings},
};
use hydrus_gui::{MainWindow, OptionsWindow, Pages, bind, gallery_source_window, headless};
use hydrus_legacy::{
    objects::{domain, parsers},
    serialisable::SerialisableObject,
};
use hydrus_parse::Downloaders;
use hydrus_store::{Store, settings};
use serde_json::{Value, json};
use slint::{ComponentHandle as _, Model as _};

fn object(value: &Value) -> SerialisableObject {
    SerialisableObject::from_tuple_str(&value.to_string()).unwrap()
}

fn store() -> ([tempfile::TempDir; 2], Arc<Store>, Value) {
    let legacy = hydrus_testkit::legacy_fixture("basic");
    let native = tempfile::tempdir().unwrap();
    hydrus_store::import::import_legacy(
        legacy.path(),
        &native.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store = Store::open(native.path()).unwrap();
    let f = hydrus_testkit::fixture_json("gallery_source.json");
    let parser = parsers::page_parser(&object(&f["parser"])).unwrap();
    let classes = UrlClassSettings {
        url_classes: f["classes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| domain::url_class(&object(c)).unwrap())
            .collect(),
        parser_links: vec![
            ("51".repeat(32), Some(parser.key.clone())),
            ("53".repeat(32), Some("63".repeat(32))),
        ],
        parser_keys: vec![parser.key.clone(), "63".repeat(32)],
        ..Default::default()
    };
    let downloaders = Downloaders {
        gugs: Gugs {
            gugs: f["gugs"]
                .as_array()
                .unwrap()
                .iter()
                .map(|g| parsers::gug(&object(g)).unwrap())
                .collect(),
            keys_to_display: serde_json::from_value(f["display"].clone()).unwrap(),
        },
        parsers: vec![parser],
        ..Default::default()
    };
    store
        .write(move |ctx| {
            settings::set(ctx.conn(), &downloaders)?;
            settings::set(ctx.conn(), &classes)?;
            settings::set(
                ctx.conn(),
                &GalleryDefaults {
                    gug: Some(("0b".repeat(32), "zeta visible".into())),
                    ..Default::default()
                },
            )
        })
        .unwrap();
    ([legacy, native], store, f)
}

fn open_options(ui: &MainWindow, bound: &hydrus_gui::Bound) -> OptionsWindow {
    ui.invoke_menu_title_pressed(0, 20.0, 22.0);
    let lines = ui.get_menu_panes().row_data(0).unwrap().lines;
    let at = (0..lines.row_count())
        .position(|i| lines.row_data(i).unwrap().label == "options…")
        .unwrap();
    ui.invoke_menu_line_clicked(0, at as i32, 0.0, 0.0, 0.0);
    let window = bound.options.borrow().as_ref().unwrap().clone_strong();
    let pages = window.get_pages();
    let at = (0..pages.row_count())
        .position(|i| pages.row_data(i).unwrap().text == "downloading")
        .unwrap() as i32;
    window.invoke_page_chosen(at);
    window
}

fn caption(window: &OptionsWindow) -> String {
    let rows = window.get_rows();
    let row = (0..rows.row_count())
        .map(|i| rows.row_data(i).unwrap())
        .find(|r| r.label == "Default download source:")
        .unwrap();
    assert_eq!(row.kind, 19);
    row.text.to_string()
}

fn choices(window: &hydrus_gui::GallerySourceWindow) -> Vec<String> {
    let model = window.get_galleries();
    (0..model.row_count())
        .map(|i| model.row_data(i).unwrap().to_string())
        .collect()
}

#[test]
fn actual_widget_replays_reference_groups_cancels_and_empty_warnings() {
    let (_dirs, store, f) = store();
    let windows = headless::init();
    let slot = Rc::default();
    let values = Rc::new(std::cell::RefCell::new(Vec::new()));
    for case in f["cases"].as_array().unwrap().iter().take(5) {
        values.borrow_mut().clear();
        let applied: gallery_source_window::Applied = Rc::new({
            let values = values.clone();
            move |value| {
                values.borrow_mut().push(value);
                Ok(())
            }
        });
        let window = gallery_source_window::open(
            &store,
            Some(("4d".repeat(32), "beta hidden".into())),
            false,
            &slot,
            applied,
        )
        .unwrap();
        for dialog in case["dialogs"].as_array().unwrap() {
            assert_eq!(json!(choices(&window)), dialog["choices"]);
            assert_eq!(
                json!([choices(&window)[usize::try_from(window.get_selected()).unwrap()].clone()]),
                dialog["selected"]
            );
            if let Some(index) = dialog["answer"].as_i64() {
                window.set_selected(i32::try_from(index).unwrap());
                window.invoke_accept_clicked();
            } else {
                window.invoke_cancel();
            }
        }
        assert_eq!(json!(values.borrow().first()), case["value"]);
        window.invoke_accept_clicked();
        assert_eq!(
            values.borrow().len(),
            usize::from(!case["value"].is_null()),
            "old callback is inactive"
        );
        assert!(slot.borrow().is_none());
    }
    let window =
        gallery_source_window::open(&store, None, false, &slot, Rc::new(|_| Ok(()))).unwrap();
    window.set_selected(5);
    window.invoke_accept_clicked();
    let pixels = headless::render(&windows.get(windows.count() - 1).unwrap(), 720, 340);
    headless::save_png(
        &std::path::Path::new(env!("CARGO_TARGET_TMPDIR"))
            .join("gallery_source_non_functional.png"),
        &pixels,
        720,
        340,
    )
    .unwrap();
    window.invoke_cancel();
    store
        .write(|ctx| settings::set(ctx.conn(), &Downloaders::default()))
        .unwrap();
    for (for_sub, case) in [(false, &f["cases"][5]), (true, &f["cases"][6])] {
        let window = gallery_source_window::open(
            &store,
            None,
            for_sub,
            &slot,
            Rc::new(|_| panic!("warning never selects")),
        )
        .unwrap();
        assert_eq!(
            window.get_message().as_str(),
            case["warnings"][0].as_str().unwrap()
        );
        window.invoke_accept_clicked();
        assert!(slot.borrow().is_none());
    }
}

#[test]
fn options_source_child_is_staged_cancel_safe_and_reaches_page_and_subscription() {
    let (_dirs, store, f) = store();
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    let saved = || {
        store
            .read(settings::get::<GalleryDefaults>)
            .unwrap()
            .gug
            .unwrap()
    };
    let original = saved();
    let options = open_options(&ui, &bound);
    assert_eq!(
        json!({"caption": caption(&options), "saved": saved()}),
        f["options_states"][0]
    );
    options.invoke_gallery_source_clicked();
    let cancelled = gallery_source_window::last_opened().unwrap();
    cancelled.set_selected(4);
    cancelled.invoke_accept_clicked();
    assert_eq!(choices(&cancelled), ["beta hidden"]);
    cancelled
        .window()
        .dispatch_event(slint::platform::WindowEvent::CloseRequested);
    cancelled.invoke_accept_clicked();
    assert_eq!(
        json!({"caption": caption(&options), "saved": saved()}),
        f["options_states"][1]
    );
    assert_eq!(saved(), original);
    options.invoke_gallery_source_clicked();
    let stale = gallery_source_window::last_opened().unwrap();
    options.invoke_cancel();
    stale.set_selected(0);
    stale.invoke_accept_clicked();
    assert!(!stale.window().is_visible());
    assert_eq!(saved(), original);
    options.invoke_gallery_source_clicked();
    assert!(
        gallery_source_window::last_opened().is_none(),
        "closed owner cannot reopen its child"
    );

    let options = open_options(&ui, &bound);
    options.invoke_gallery_source_clicked();
    let child = gallery_source_window::last_opened().unwrap();
    child.set_selected(4);
    child.invoke_accept_clicked();
    child.invoke_accept_clicked();
    assert_eq!(
        json!({"caption": caption(&options), "saved": saved()}),
        f["options_states"][2]
    );
    assert_eq!(saved(), original, "child OK changes the parent draft only");
    options.invoke_cancel();
    assert_eq!(saved(), original);

    let options = open_options(&ui, &bound);
    options.invoke_gallery_source_clicked();
    let child = gallery_source_window::last_opened().unwrap();
    child.set_selected(4);
    child.invoke_accept_clicked();
    child.invoke_accept_clicked();
    options.invoke_apply();
    assert_eq!(
        json!({"caption": caption(&options), "saved": saved()}),
        f["options_states"][3]
    );
    assert_eq!(saved(), ("0d".repeat(32), "beta hidden".into()));
    let reopened = open_options(&ui, &bound);
    assert_eq!(caption(&reopened), "beta hidden");
    reopened.invoke_cancel();

    ui.invoke_new_page();
    ui.invoke_chooser_pressed(4);
    ui.invoke_chooser_pressed(6);
    let page = bound.pages.borrow_mut().current();
    let gallery = page.borrow().gallery().unwrap().gallery();
    assert_eq!((gallery.gug_key, gallery.gug_name), saved());
    let list = super::subscriptions::open_dialog(&ui, &bound);
    list.invoke_add();
    let chooser = bound
        .subscription_gallery
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    let model = chooser.get_galleries();
    assert_eq!(
        model
            .row_data(usize::try_from(chooser.get_selected()).unwrap())
            .unwrap(),
        "beta hidden"
    );
    chooser.invoke_accept_clicked();
    let editor = bound
        .edit_subscription
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    assert_eq!(editor.get_downloader(), "beta hidden");
    editor.invoke_cancel();
    list.invoke_cancel();
}
