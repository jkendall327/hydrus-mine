//! Copy and paste in the import options editor, compared with
//! `oracle/fixtures/import_options_paste.json` (`record_import_options_paste.py`):
//! the reference's container editor given the same clipboard, for a specific
//! importer and for the "global" defaults.
use hydrus_core::import_options::{ImportOptionsManager, ImportOptionsSlice};
use hydrus_downloader_exchange::import_options;
use hydrus_gui::{MainWindow, Pages, bind, headless, import_options_panel_window as panel};
use hydrus_store::settings;
use serde_json::Value;
use slint::{ComponentHandle as _, Model as _};

use crate::import_options_fields::{Setup, setup};

fn fixture() -> Value {
    hydrus_testkit::fixture_json("import_options_paste.json")
}

fn slice(value: &Value) -> ImportOptionsSlice {
    import_options::decode_text(&value.to_string()).unwrap()
}

/// A container's tuple with each tag service's rules in the order of the
/// services' keys (the reference lists the services in its own order).
fn sorted(mut tuple: Value) -> Value {
    fn walk(value: &mut Value) {
        if let Value::Array(items) = value {
            if items.len() == 3
                && items[0] == 151
                && items[1] == 1
                && let Value::Array(services) = &mut items[2]
            {
                services.sort_by_key(|s| s[0].to_string());
            }
            items.iter_mut().for_each(walk);
        }
    }
    walk(&mut tuple);
    tuple
}

/// The editor of a specific importer, whose options are `current`.
fn editor_of(s: &Setup, current: &ImportOptionsSlice) -> hydrus_gui::ImportOptionsWindow {
    let (queue, options) = (s.queue, current.clone());
    s.store
        .write(move |ctx| hydrus_store::queues::set_queue_options(ctx.conn(), queue, &options))
        .unwrap();
    // (the page reads its importer's options when it is shown)
    (s.bound.sync)();
    s.editor()
}

fn paste_with(text: String) {
    hydrus_gui::set_paster(move || text.clone());
}

// leaf: import-paste
#[test]
fn a_specific_importers_editor_pastes_as_the_references_does() {
    let _windows = headless::init();
    let fixture = fixture();
    let containers = &fixture["containers"];
    let mode = |name: &str| match name {
        "merge-paste" => 0,
        "fill-in-gaps-paste" => 1,
        _ => 2,
    };
    for scenario in fixture["specific"].as_array().unwrap() {
        let (current, pasted, how) = (
            scenario["current"].as_str().unwrap(),
            scenario["pasted"].as_str().unwrap(),
            scenario["mode"].as_str().unwrap(),
        );
        let s = setup();
        let editor = editor_of(&s, &slice(&containers[current]));
        paste_with(containers[pasted].to_string());
        editor.invoke_paste_options(mode(how));
        assert_eq!(
            editor.get_clipboard_error().as_str(),
            "",
            "{current} + {pasted} ({how}): no message"
        );
        editor.invoke_apply();
        assert_eq!(
            sorted(import_options::tuple(&s.saved()).unwrap()),
            sorted(scenario["value"].clone()),
            "{current} + {pasted} ({how})"
        );
    }
}

// leaf: import-paste
#[test]
fn a_specific_importers_editor_copies_what_it_holds_as_the_references_does() {
    use std::cell::RefCell;
    use std::rc::Rc;
    let _windows = headless::init();
    let fixture = fixture();
    for key in ["specific_copy", "specific_copy_every"] {
        let copy = &fixture[key];
        let s = setup();
        let current = if key == "specific_copy" {
            "existing"
        } else {
            "every kind customised"
        };
        let editor = editor_of(&s, &slice(&fixture["containers"][current]));
        let copied = Rc::new(RefCell::new(Vec::<hydrus_gui::Clip>::new()));
        hydrus_gui::set_clipper({
            let copied = copied.clone();
            move |clip| copied.borrow_mut().push(clip.clone())
        });
        editor.invoke_copy_options();
        let hydrus_gui::Clip::Text(text) = copied.borrow()[0].clone() else {
            panic!("copied as text")
        };
        // the reference's text as it wrote it, and ours, read the same way
        let theirs: Value = serde_json::from_str(copy["text"].as_str().unwrap()).unwrap();
        let ours: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(
            sorted(import_options::tuple(&import_options::decode_text(&text).unwrap()).unwrap()),
            sorted(copy["value"].clone()),
            "{key}: what was copied"
        );
        assert_eq!(
            (&ours[0], &ours[1]),
            (&theirs[0], &theirs[1]),
            "{key}: the same kind of object and version"
        );
    }
}

// leaf: import-paste
#[test]
fn pasting_what_is_not_import_options_says_what_the_reference_says() {
    let _windows = headless::init();
    let fixture = fixture();
    for error in fixture["errors"].as_array().unwrap() {
        let text = error["text"].as_str().unwrap();
        let s = setup();
        let editor = editor_of(&s, &slice(&fixture["containers"]["existing"]));
        paste_with(text.to_owned());
        editor.invoke_paste_options(2);
        let shown = editor.get_clipboard_error().to_string();
        let reference = error["criticals"][0]["message"].as_str().unwrap();
        assert_eq!(shown, reference, "pasted {text:?}");
        // and nothing changes
        editor.invoke_apply();
        assert_eq!(
            sorted(import_options::tuple(&s.saved()).unwrap()),
            sorted(error["value"].clone()),
            "pasted {text:?}: the options"
        );
    }
}

fn open_options(ui: &MainWindow) {
    ui.invoke_menu_title_pressed(0, 20.0, 22.0);
    let lines = ui.get_menu_panes().row_data(0).unwrap().lines;
    let index = (0..lines.row_count())
        .find(|&index| lines.row_data(index).unwrap().label == "options…")
        .unwrap();
    ui.invoke_menu_line_clicked(0, i32::try_from(index).unwrap(), 0.0, 0.0, 0.0);
}

// leaf: import-paste
#[test]
fn the_global_defaults_editor_pastes_as_the_references_does() {
    let _windows = headless::init();
    let fixture = fixture();
    let containers = &fixture["containers"];
    for scenario in fixture["global"].as_array().unwrap() {
        let (pasted, how) = (
            scenario["pasted"].as_str().unwrap(),
            scenario["mode"].as_str().unwrap(),
        );
        let (_dirs, store) = crate::subscriptions::store();
        // (the defaults the recording started from)
        let mut manager = store.read(settings::get::<ImportOptionsManager>).unwrap();
        let start = slice(&containers["full"]);
        manager
            .caller_defaults
            .retain(|(caller, _)| *caller != hydrus_core::import_options::CallerType::Global);
        manager
            .caller_defaults
            .push((hydrus_core::import_options::CallerType::Global, start));
        store
            .write(move |ctx| settings::set(ctx.conn(), &manager))
            .unwrap();
        let ui = MainWindow::new().unwrap();
        let bound = bind(&ui, Pages::open(store.clone()).unwrap());
        open_options(&ui);
        let options = bound.options.borrow().as_ref().unwrap().clone_strong();
        let index = (0..options.get_pages().row_count())
            .find(|&index| options.get_pages().row_data(index).unwrap().text == "import options")
            .unwrap();
        options.set_page(index as i32);
        options.invoke_page_chosen(index as i32);
        options.invoke_import_options_clicked();
        let window = panel::last_opened().unwrap();
        let row = window
            .get_defaults()
            .iter()
            .position(|row| row.cells.row_data(0).unwrap() == "global")
            .unwrap();
        window.invoke_clicked(0, row as i32, false, false);
        window.invoke_action(0, "edit".into());
        let editor = panel::editing_window().unwrap();
        // the reference's global editor offers custom paste and merge-paste
        // only; a specific importer's offers fill-in and replace as well
        let menu = |key: &str| -> Vec<String> {
            fixture[key]
                .as_array()
                .unwrap()
                .iter()
                .map(|e| e.as_str().unwrap().to_owned())
                .collect()
        };
        assert!(!menu("global_menu").contains(&"replace-paste".to_owned()));
        assert!(!menu("global_menu").contains(&"fill-in-gaps-paste".to_owned()));
        assert!(menu("specific_menu").contains(&"replace-paste".to_owned()));
        assert!(editor.get_global_defaults(), "the paste menu hides them");
        paste_with(containers[pasted].to_string());
        editor.invoke_paste_options(if how == "merge-paste" { 0 } else { 2 });
        let information = scenario["information"].as_array().unwrap();
        match information.first() {
            Some(message) => assert_eq!(
                editor.get_clipboard_error().as_str(),
                message.as_str().unwrap(),
                "{pasted} ({how}): the message"
            ),
            None => assert_eq!(
                editor.get_clipboard_error().as_str(),
                "",
                "{pasted} ({how})"
            ),
        }
        editor.invoke_apply();
        window.invoke_apply();
        options.invoke_apply();
        let manager = store.read(settings::get::<ImportOptionsManager>).unwrap();
        let global = manager
            .caller_default(hydrus_core::import_options::CallerType::Global)
            .cloned()
            .unwrap_or_default();
        assert_eq!(
            sorted(import_options::tuple(&global).unwrap()),
            sorted(scenario["value"].clone()),
            "{pasted} ({how}): the global defaults"
        );
    }
}

// leaf: import-paste
#[test]
fn custom_paste_opens_the_overwrite_chooser_and_its_choice_reaches_the_editor() {
    let _windows = headless::init();
    // (the reference's chooser, recorded over an editor's options and a
    // pasted container: `record_subscription_import_options.py`)
    let fixture = hydrus_testkit::fixture_json("subscription_import_options.json");
    let existing = import_options::decode_text(&fixture["existing"].to_string()).unwrap();
    for (preset, step) in [(0, 1), (1, 2), (2, 3)] {
        let s = setup();
        let editor = editor_of(&s, &existing);
        paste_with(fixture["incoming"].to_string());
        editor.invoke_paste_options(3);
        assert!(editor.get_overwrite_open(), "the chooser is open");
        let chooser = hydrus_gui::import_options_overwrite_window::last_opened().unwrap();
        chooser.invoke_preset(preset);
        chooser.invoke_apply();
        assert!(!editor.get_overwrite_open());
        editor.invoke_apply();
        assert_eq!(
            sorted(import_options::tuple(&s.saved()).unwrap()),
            sorted(fixture["overwrite"][step]["options"].clone()),
            "{}",
            fixture["overwrite"][step]["action"]
        );
    }
    // closing the chooser keeps the options
    let s = setup();
    let editor = editor_of(&s, &existing);
    paste_with(fixture["incoming"].to_string());
    editor.invoke_paste_options(3);
    hydrus_gui::import_options_overwrite_window::last_opened()
        .unwrap()
        .invoke_cancel();
    assert!(!editor.get_overwrite_open());
    editor.invoke_apply();
    assert_eq!(
        sorted(import_options::tuple(&s.saved()).unwrap()),
        sorted(fixture["existing"].clone())
    );
}
