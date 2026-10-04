//! The sidecar editors, from the import and export folder dialogs: a
//! router made for an import folder (a .txt sidecar's lines, as tags), its
//! source and destination edited in their own windows, and an export
//! folder's routers from the templates menu, written with their folders.
//! (What the editors show and do at each step is tested against the
//! reference's in hydrus-gui-model's tests.)

use slint::{ComponentHandle as _, Model as _};

use hydrus_gui::{MainWindow, Pages, SidecarNodeWindow, bind, headless};
use hydrus_parse::sidecar::{Exporter, Source};
use hydrus_store::{import_folders, settings};

use crate::folders::open;
use crate::subscriptions::store;

fn labels(rows: &slint::ModelRc<hydrus_gui::TableRow>) -> Vec<String> {
    (0..rows.row_count())
        .map(|r| {
            rows.row_data(r)
                .unwrap()
                .cells
                .row_data(0)
                .unwrap()
                .to_string()
        })
        .collect()
}

fn choices(node: &SidecarNodeWindow) -> Vec<String> {
    let choices = node.get_asking_choices();
    (0..choices.row_count())
        .map(|i| choices.row_data(i).unwrap().to_string())
        .collect()
}

#[test]
fn an_import_folders_sidecars_are_edited_and_written() {
    let (_dirs, store) = store();
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    let watched = tempfile::tempdir().unwrap();

    open(&ui, "manage import folders\u{2026}");
    let list = bound
        .folders
        .import_list
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    list.invoke_add();
    let edit = bound
        .folders
        .import_edit
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    edit.set_path(watched.path().to_string_lossy().into_owned().into());
    assert_eq!(edit.get_sidecars(), "no sidecars");

    // the routers: none yet
    edit.invoke_edit_sidecars();
    let slots = &bound.folders.sidecars;
    let routers = slots.routers.borrow().as_ref().unwrap().clone_strong();
    assert_eq!(
        routers.get_window_title(),
        "edit metadata migration routers"
    );
    assert_eq!(routers.get_rows().row_count(), 0);
    // a new one: no sources, a human sort, to a file's tags on my tags
    routers.invoke_add();
    let router = slots.router.borrow().as_ref().unwrap().clone_strong();
    assert_eq!(router.get_window_title(), "edit metadata migration router");
    assert_eq!(router.get_processing(), "sorting human sort (ascending)");
    assert_eq!(router.get_destination(), "tags to media, on \"my tags\"");
    // a source: which kind (an import's are sidecars)
    router.invoke_add();
    assert!(router.get_asking());
    assert_eq!(router.get_asking_title(), "Which type?");
    router.invoke_chosen(0);
    let node = slots.node.borrow().as_ref().unwrap().clone_strong();
    assert_eq!(node.get_window_title(), "edit metadata migration source");
    assert_eq!(node.get_type_label(), "a .txt sidecar");
    assert_eq!(node.get_result(), "my_image.jpg.txt");
    node.set_suffix("tags".into());
    node.set_remove_ext(true);
    node.invoke_changed();
    assert_eq!(node.get_result(), "my_image.tags.txt");
    // a .json sidecar, then back, keeping the naming
    node.invoke_change_type();
    assert_eq!(choices(&node), ["a .json sidecar"]);
    node.invoke_chosen(0);
    assert_eq!(node.get_type_label(), "a .json sidecar");
    assert_eq!(node.get_result(), "my_image.tags.json");
    node.invoke_change_type();
    node.invoke_chosen(0);
    assert_eq!(node.get_type_label(), "a .txt sidecar");
    let last = (0..100)
        .take_while(|&n| windows.get(n).is_some())
        .last()
        .unwrap();
    let pixels = headless::render(&windows.get(last).unwrap(), 640, 560);
    let shots = std::path::Path::new(env!("CARGO_TARGET_TMPDIR"));
    headless::save_png(&shots.join("sidecar_source.png"), &pixels, 640, 560).unwrap();
    node.invoke_apply();
    assert!(slots.node.borrow().is_none());
    assert_eq!(labels(&router.get_sources()), ["from .txt sidecar"]);
    // the destination: a file's notes, then back to tags
    router.invoke_edit_destination();
    let node = slots.node.borrow().as_ref().unwrap().clone_strong();
    assert_eq!(
        node.get_window_title(),
        "edit metadata migration destination"
    );
    assert_eq!(node.get_service_label(), "my tags");
    node.invoke_change_type();
    let offered = choices(&node);
    let notes = offered.iter().position(|c| c == "a file's notes").unwrap();
    node.invoke_chosen(i32::try_from(notes).unwrap());
    assert!(node.get_show_forced());
    node.invoke_apply();
    assert_eq!(router.get_destination(), "notes to media");
    // notes from a .txt split by newlines: "ok" asks first
    router.invoke_apply();
    assert!(
        router
            .get_asking_message()
            .starts_with("Hey, you are importing notes from a .txt file")
    );
    router.invoke_chosen(1);
    assert!(slots.router.borrow().is_some(), "kept open");
    router.invoke_edit_destination();
    let node = slots.node.borrow().as_ref().unwrap().clone_strong();
    node.invoke_change_type();
    let offered = choices(&node);
    let tags = offered.iter().position(|c| c == "a file's tags").unwrap();
    node.invoke_chosen(i32::try_from(tags).unwrap());
    node.invoke_apply();
    router.invoke_apply();
    assert!(slots.router.borrow().is_none());
    assert_eq!(
        labels(&routers.get_rows()),
        ["Taking from .txt sidecar, applying some sorting, sending tags to media, on \"my tags\"."]
    );
    routers.invoke_apply();
    assert!(slots.routers.borrow().is_none());
    assert_eq!(
        edit.get_sidecars(),
        "Taking from .txt sidecar, applying some sorting, sending tags t\u{2026}"
    );
    edit.invoke_apply();
    list.invoke_apply();
    let written = store.read(import_folders::import_folders).unwrap();
    let routers = &written[0].settings.routers;
    assert_eq!(routers.len(), 1);
    let Source::Txt { naming, separator } = &routers[0].importers[0].source else {
        panic!("{routers:?}");
    };
    assert_eq!(naming.suffix, "tags");
    assert!(naming.remove_actual_filename_ext);
    assert_eq!(separator, "\n");
    assert!(matches!(routers[0].exporter, Exporter::MediaTags { .. }));
}

#[test]
fn an_export_folders_sidecars_come_from_the_templates() {
    let (_dirs, store) = store();
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    open(&ui, "manage export folders\u{2026}");
    let list = bound
        .folders
        .export_list
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    list.invoke_add();
    let edit = bound
        .folders
        .export_edit
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    edit.set_path("/tmp/exported".into());
    edit.invoke_edit_sidecars();
    let slots = &bound.folders.sidecars;
    let routers = slots.routers.borrow().as_ref().unwrap().clone_strong();
    let templates = routers.get_templates();
    assert_eq!(
        templates.row_data(0).unwrap(),
        "easy one-click JSON that covers the basics"
    );
    routers.invoke_template(0);
    let rows = labels(&routers.get_rows());
    assert_eq!(
        rows[0],
        "Taking notes from media, applying some sorting, sending to .json sidecar (notes)."
    );
    // one taken out, asking first
    routers.invoke_row_clicked(0, false, false);
    routers.invoke_delete();
    assert_eq!(routers.get_asking_message(), "Remove 1 selected?");
    routers.invoke_chosen(0);
    assert_eq!(labels(&routers.get_rows()).len(), rows.len() - 1);
    routers.invoke_apply();
    assert_eq!(
        edit.get_sidecars(),
        format!("{} sidecar actions", rows.len() - 1)
    );
    edit.invoke_apply();
    list.invoke_apply();
    let settings::ExportFolders(written) = store.read(settings::get).unwrap();
    assert_eq!(written[0].routers.len(), rows.len() - 1);
}

fn table(rows: &slint::ModelRc<hydrus_gui::TableRow>) -> Vec<String> {
    labels(rows)
}

#[test]
fn a_routers_processing_is_edited_in_the_string_processor_editor() {
    let (_dirs, store) = store();
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    open(&ui, "manage import folders\u{2026}");
    let list = bound
        .folders
        .import_list
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    list.invoke_add();
    let edit = bound
        .folders
        .import_edit
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    let watched = tempfile::tempdir().unwrap();
    edit.set_path(watched.path().to_string_lossy().into_owned().into());
    edit.invoke_edit_sidecars();
    let slots = &bound.folders.sidecars;
    let routers = slots.routers.borrow().as_ref().unwrap().clone_strong();
    routers.invoke_add();
    let router = slots.router.borrow().as_ref().unwrap().clone_strong();
    assert_eq!(router.get_processing(), "sorting human sort (ascending)");

    // the router's processing: its one step, a sort
    router.invoke_edit_processing();
    let strings = &slots.strings;
    let editor = strings.processor.borrow().as_ref().unwrap().clone_strong();
    let processor_window = (0..100)
        .take_while(|&n| windows.get(n).is_some())
        .last()
        .unwrap();
    assert_eq!(editor.get_window_title(), "edit string processor");
    assert_eq!(
        table(&editor.get_steps()),
        ["SORT: sorting human sort (ascending)"]
    );
    // a single example, through each step
    editor.set_example("b,a,c".into());
    editor.invoke_example_edited();
    assert_eq!(editor.get_tabs().iter().collect::<Vec<_>>(), ["sorter (1)"]);
    // a splitter added, asking which kind first
    editor.invoke_add();
    assert!(editor.get_asking());
    assert_eq!(editor.get_asking_title(), "Which type of processing step?");
    editor.invoke_chosen(3);
    let step = strings.step.borrow().as_ref().unwrap().clone_strong();
    assert_eq!(step.get_window_title(), "edit processing step");
    assert_eq!(step.get_kind(), 0);
    assert_eq!(step.get_separator(), ",");
    // (its example: the example through the steps before it)
    assert_eq!(step.get_split_example(), "b,a,c");
    assert_eq!(table(&step.get_results()), ["b", "a", "c"]);
    // no separator: said so on "apply"
    step.set_separator("".into());
    step.invoke_changed();
    assert!(step.get_invalid());
    step.invoke_apply();
    assert_eq!(
        step.get_veto(),
        "Sorry, you have to have a value in the separator field!"
    );
    step.set_separator(",".into());
    step.invoke_changed();
    assert_eq!(step.get_veto(), "");
    step.invoke_apply();
    assert!(strings.step.borrow().is_none());
    assert_eq!(
        table(&editor.get_steps()),
        [
            "SORT: sorting human sort (ascending)",
            "SPLIT: splitting by \",\""
        ]
    );
    // moved up, to sort what it splits
    editor.invoke_row_clicked(1, false, false);
    editor.invoke_up();
    assert_eq!(table(&editor.get_steps())[0], "SPLIT: splitting by \",\"");
    editor.invoke_tab_chosen(1);
    assert_eq!(table(&editor.get_tab_rows()), ["a", "b", "c"]);
    let pixels = headless::render(&windows.get(processor_window).unwrap(), 820, 640);
    let shots = std::path::Path::new(env!("CARGO_TARGET_TMPDIR"));
    headless::save_png(&shots.join("string_processor.png"), &pixels, 820, 640).unwrap();
    // the sort edited: descending
    editor.invoke_row_clicked(1, false, false);
    editor.invoke_edit();
    let step = strings.step.borrow().as_ref().unwrap().clone_strong();
    assert_eq!(step.get_kind(), 3);
    step.set_ascending(false);
    step.invoke_changed();
    step.invoke_apply();
    editor.invoke_tab_chosen(1);
    assert_eq!(table(&editor.get_tab_rows()), ["c", "b", "a"]);
    // a match added: a regex its example must match
    editor.invoke_add();
    editor.invoke_chosen(0);
    let step = strings.step.borrow().as_ref().unwrap().clone_strong();
    assert_eq!(step.get_kind(), 4);
    assert_eq!(step.get_test_result(), "Example matches ok!");
    step.set_match_type(3);
    step.invoke_changed();
    // (the regex takes the example)
    assert!(step.get_show_match_regex());
    assert_eq!(step.get_match_regex(), step.get_match_example());
    step.set_match_regex("\\d".into());
    step.invoke_changed();
    assert!(!step.get_test_ok());
    assert!(
        step.get_test_result()
            .starts_with("Example does not match - ")
    );
    step.invoke_apply();
    assert_eq!(
        step.get_veto(),
        "Please enter an example text that matches the given rules!"
    );
    step.set_match_example("a1".into());
    step.invoke_changed();
    assert_eq!(step.get_test_result(), "Example matches ok!");
    step.invoke_apply();
    let added = table(&editor.get_steps());
    assert!(added[2].starts_with("MATCH: "), "{added:?}");
    editor.invoke_row_clicked(2, false, false);
    editor.invoke_delete();
    editor.invoke_chosen(0);
    // a tag filter added: its example tested, its filter in the tag
    // filter editor
    editor.invoke_add();
    editor.invoke_chosen(1);
    let step = strings.step.borrow().as_ref().unwrap().clone_strong();
    assert_eq!(step.get_kind(), 5);
    assert_eq!(step.get_test_result(), "Example matches ok!");
    step.set_tag_example("   ".into());
    step.invoke_changed();
    assert_eq!(
        step.get_test_result(),
        "Example does not match - \"   \" was not a valid tag!"
    );
    step.set_tag_example("blue eyes".into());
    step.invoke_changed();
    step.invoke_edit_tag_filter();
    let filter = strings.tag_filter.borrow().as_ref().unwrap().clone_strong();
    assert_eq!(filter.get_window_title(), "edit tag filter");
    filter.invoke_apply();
    step.invoke_apply();
    let added = table(&editor.get_steps());
    assert!(added[2].starts_with("TAG FILTER: "), "{added:?}");
    editor.invoke_row_clicked(2, false, false);
    editor.invoke_delete();
    editor.invoke_chosen(0);
    // a joiner added and taken out again, asking first
    editor.invoke_add();
    editor.invoke_chosen(4);
    let step = strings.step.borrow().as_ref().unwrap().clone_strong();
    step.invoke_apply();
    assert_eq!(table(&editor.get_steps()).len(), 3);
    editor.invoke_row_clicked(2, false, false);
    editor.invoke_delete();
    assert_eq!(editor.get_asking_message(), "Remove 1 selected?");
    editor.invoke_chosen(0);
    assert_eq!(table(&editor.get_steps()).len(), 2);
    editor.invoke_apply();
    assert!(strings.processor.borrow().is_none());
    assert_eq!(
        router.get_processing(),
        "splitting by \",\"\nsorting human sort (descending)"
    );
    router.invoke_apply();
    routers.invoke_apply();
    edit.invoke_apply();
    list.invoke_apply();
    let written = store.read(import_folders::import_folders).unwrap();
    let steps = &written[0].settings.routers[0].processor.steps;
    assert_eq!(steps.len(), 2);
    assert!(matches!(
        steps[1],
        hydrus_core::url::strings::ProcessingStep::Sort {
            ascending: false,
            ..
        }
    ));
}

#[test]
fn a_sidecars_filename_conversion_is_edited_in_the_converter_editor() {
    let (_dirs, store) = store();
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    open(&ui, "manage import folders\u{2026}");
    let list = bound
        .folders
        .import_list
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    list.invoke_add();
    let edit = bound
        .folders
        .import_edit
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    edit.invoke_edit_sidecars();
    let slots = &bound.folders.sidecars;
    let routers = slots.routers.borrow().as_ref().unwrap().clone_strong();
    routers.invoke_add();
    let router = slots.router.borrow().as_ref().unwrap().clone_strong();
    router.invoke_add();
    router.invoke_chosen(0);
    let node = slots.node.borrow().as_ref().unwrap().clone_strong();
    assert_eq!(node.get_converter_label(), "no string conversions");
    assert_eq!(node.get_result(), "my_image.jpg.txt");

    // the converter, with the sidecar path as its example
    node.invoke_edit_converter();
    let strings = &slots.strings;
    let converter = strings.converter.borrow().as_ref().unwrap().clone_strong();
    assert_eq!(converter.get_window_title(), "edit string converter");
    assert_eq!(converter.get_example(), "my_image.jpg.txt");
    assert_eq!(converter.get_rows().row_count(), 0);
    // a conversion: "append extra text" at first, made a removal
    converter.invoke_add();
    let conversion = strings.conversion.borrow().as_ref().unwrap().clone_strong();
    assert_eq!(conversion.get_window_title(), "edit conversion");
    assert_eq!(conversion.get_text_label(), "text to append: ");
    assert_eq!(conversion.get_text(), "extra text");
    assert_eq!(conversion.get_result(), "my_image.jpg.txtextra text");
    conversion.set_kind(0);
    conversion.invoke_changed();
    assert_eq!(conversion.get_text_label(), "");
    assert_eq!(conversion.get_number_label(), "characters to remove: ");
    conversion.set_number(3);
    conversion.invoke_changed();
    assert_eq!(conversion.get_result(), "image.jpg.txt");
    conversion.invoke_apply();
    assert!(strings.conversion.borrow().is_none());
    let rows: Vec<Vec<String>> = (0..converter.get_rows().row_count())
        .map(|r| {
            let cells = converter.get_rows().row_data(r).unwrap().cells;
            (0..cells.row_count())
                .map(|c| cells.row_data(c).unwrap().to_string())
                .collect()
        })
        .collect();
    assert_eq!(
        rows,
        [["1", "remove the first 3 characters", "image.jpg.txt"]]
    );
    // "add" again starts from the last conversion made
    converter.invoke_add();
    let conversion = strings.conversion.borrow().as_ref().unwrap().clone_strong();
    assert_eq!(conversion.get_number_label(), "characters to remove: ");
    assert_eq!(conversion.get_example(), "image.jpg.txt");
    conversion.invoke_cancel();
    // a regex that captures a group and replaces it with nothing: asked
    converter.invoke_row_clicked(0, false, false);
    converter.invoke_edit();
    let conversion = strings.conversion.borrow().as_ref().unwrap().clone_strong();
    conversion.set_kind(10);
    conversion.invoke_changed();
    conversion.set_pattern("(_)".into());
    conversion.invoke_changed();
    conversion.invoke_apply();
    assert!(conversion.get_asking());
    conversion.invoke_chosen(0);
    assert!(strings.conversion.borrow().is_none());
    converter.invoke_apply();
    assert!(strings.converter.borrow().is_none());
    assert_eq!(
        node.get_converter_label(),
        "regex substitution: ('(_)', '')"
    );
    assert_eq!(node.get_result(), "myimage.jpg.txt");
}

#[test]
fn date_conversion_fields_preview_and_cancel_reach_the_converter() {
    use hydrus_core::url::strings::{Conversion, DateTimezone, StringConverter};
    use hydrus_gui::string_processor_window::{Slots, open_converter};
    use std::{cell::RefCell, rc::Rc};
    let _windows = headless::init();
    let slots = Slots::default();
    let accepted = Rc::new(RefCell::new(None));
    let original = StringConverter {
        example: "2024-02-29".into(),
        conversions: vec![Conversion::DateDecode {
            phrase: "%Y-%m-%d".into(),
            timezone: DateTimezone::Utc,
            offset: 0,
        }],
    };
    let window = open_converter(
        &original,
        None,
        &slots,
        Rc::new({
            let accepted = accepted.clone();
            move |value| *accepted.borrow_mut() = Some(value)
        }),
    )
    .unwrap();
    window.invoke_row_clicked(0, false, false);
    window.invoke_edit();
    let child = slots.conversion.borrow().as_ref().unwrap().clone_strong();
    assert_eq!(child.get_result(), "1709164800");
    assert!(child.get_show_timezone_decode());
    assert!(!child.get_show_timezone_offset());
    child.set_timezone_decode(2);
    child.set_timezone_offset(3600);
    child.invoke_changed();
    assert!(child.get_show_timezone_offset());
    assert_eq!(child.get_result(), "1709161200");
    child.invoke_cancel();
    window.invoke_edit();
    let child = slots.conversion.borrow().as_ref().unwrap().clone_strong();
    assert_eq!(child.get_timezone_decode(), 0);
    child.set_text("%Y".into());
    child.invoke_changed();
    assert!(child.get_result().starts_with("ERROR:"));
    child.set_text("%Y-%m-%d".into());
    child.set_timezone_decode(2);
    child.set_timezone_offset(3600);
    child.invoke_changed();
    child.invoke_apply();
    window.invoke_apply();
    let value = accepted.borrow().clone().unwrap();
    assert_eq!(value.convert("2024-02-29").unwrap(), "1709161200");
    assert!(matches!(
        value.conversions[0],
        Conversion::DateDecode {
            timezone: DateTimezone::Offset,
            offset: 3600,
            ..
        }
    ));
}

#[test]
fn processing_exchange_reviews_append_and_parent_cancel_invalidates_children() {
    use hydrus_core::url::strings::StringProcessor;
    use hydrus_gui::string_processor_window::{Slots, open};
    use std::{cell::RefCell, rc::Rc};
    let (_dirs, store) = store();
    let _windows = headless::init();
    let slots = Slots::default();
    let accepted = Rc::new(RefCell::new(None));
    let window = open(
        &store,
        &StringProcessor::default(),
        vec!["a,b".into()],
        &slots,
        Rc::new({
            let accepted = accepted.clone();
            move |value| *accepted.borrow_mut() = Some(value)
        }),
    )
    .unwrap();
    let fixture = hydrus_testkit::fixture_json("processing_exchange.json");
    window.invoke_exchange("import".into());
    let child = slots.exchange.0.borrow().as_ref().unwrap().clone_strong();
    assert!(window.get_child_open());
    child.set_text("not JSON".into());
    child.invoke_action("review".into());
    assert!(!child.get_ready());
    assert!(!child.get_error().is_empty());
    assert_eq!(window.get_steps().row_count(), 0);
    child.set_text(fixture["multiple"].to_string().into());
    child.invoke_action("review".into());
    assert!(child.get_ready());
    assert!(child.get_review().contains("Append 2 processing steps"));
    assert_eq!(window.get_steps().row_count(), 0);
    child.invoke_action("accept".into());
    assert_eq!(window.get_steps().row_count(), 2);
    assert!(!window.get_child_open());
    window.invoke_row_clicked(1, false, false);
    window.invoke_exchange("export".into());
    let export = slots.exchange.0.borrow().as_ref().unwrap().clone_strong();
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(export.get_text().as_str()).unwrap(),
        fixture["single"]
    );
    window.invoke_cancel();
    assert!(!slots.has_open());
    assert!(!export.window().is_visible());
    child.invoke_action("accept".into());
    window.invoke_apply();
    assert!(accepted.borrow().is_none());
}

fn test_table(rows: &slint::ModelRc<hydrus_gui::TableRow>) -> Vec<Vec<String>> {
    rows.iter()
        .map(|row| row.cells.iter().map(|cell| cell.to_string()).collect())
        .collect()
}

#[test]
fn router_examples_follow_reference_source_tabs_and_processor_children_without_exports() {
    use hydrus_gui::sidecars_window::{self, Slots};
    use hydrus_gui_model::sidecar_editors::{Context, TestObject};
    use std::{cell::RefCell, rc::Rc};
    let (_dirs, store) = store();
    let rendered = headless::init();
    let directory = tempfile::tempdir().unwrap();
    let cases = hydrus_testkit::fixture_json("sidecar_testing.json");
    let case = &cases.as_array().unwrap()[0];
    for (name, text) in case["documents"].as_object().unwrap() {
        std::fs::write(directory.path().join(name), text.as_str().unwrap()).unwrap();
    }
    let object =
        hydrus_legacy::serialisable::SerialisableObject::from_tuple_str(&case["tuple"].to_string())
            .unwrap();
    let original = hydrus_legacy::objects::sidecars::router(&object).unwrap();
    let slots = Slots::default();
    slots.set_test_objects(
        ["one.png", "two.png", "missing.png"]
            .into_iter()
            .map(|name| {
                TestObject::File(directory.path().join(name).to_string_lossy().into_owned())
            })
            .collect(),
    );
    let accepted = Rc::new(RefCell::new(None));
    let window = sidecars_window::open_router(
        &store,
        Context::Import,
        original.clone(),
        &slots,
        Rc::new({
            let accepted = accepted.clone();
            let store = store.clone();
            move |router| {
                let persisted = hydrus_gui_model::export_files::Preferences {
                    routers: vec![router.clone()],
                    ..hydrus_gui_model::export_files::Preferences::default()
                };
                store
                    .write(move |ctx| settings::set(ctx.conn(), &persisted))
                    .unwrap();
                *accepted.borrow_mut() = Some(router);
            }
        }),
    )
    .unwrap();
    *slots.router.borrow_mut() = Some(window.clone_strong());
    for source in 0..2 {
        window.set_test_source(source);
        window.invoke_test_source_chosen();
        let mut rows = test_table(&window.get_test_rows());
        for row in &mut rows {
            row[0] = row[0]
                .replace(directory.path().to_str().unwrap(), "<examples>")
                .replace('\\', "/");
        }
        assert_eq!(
            serde_json::to_value(rows).unwrap(),
            case["tables"][usize::try_from(source).unwrap()]["rows"]
        );
    }
    let pixels = headless::render(&rendered.get(0).unwrap(), 1000, 720);
    assert!(pixels.iter().any(|pixel| *pixel > 100));
    headless::save_png(
        &std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("sidecar-examples.png"),
        &pixels,
        1000,
        720,
    )
    .unwrap();
    window.invoke_edit_processing();
    let processor = slots
        .strings
        .processor
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    assert_eq!(
        serde_json::to_value(table(&processor.get_starting())).unwrap(),
        case["processor_texts"]
    );
    assert!(window.get_child_open());
    window.invoke_apply();
    assert!(accepted.borrow().is_none());
    processor.invoke_row_clicked(0, false, false);
    processor.invoke_edit();
    let step = slots.strings.step.borrow().as_ref().unwrap().clone_strong();
    step.set_ascending(false);
    step.invoke_changed();
    step.invoke_apply();
    processor.invoke_apply();
    assert!(!window.get_child_open());
    assert!(window.get_processing().contains("descending"));
    window.invoke_apply();
    let saved = accepted.borrow().clone().unwrap();
    assert_ne!(saved.processor, original.processor);
    let persisted: hydrus_gui_model::export_files::Preferences = store.read(settings::get).unwrap();
    assert_eq!(persisted.routers, [saved.clone()]);
    let inputs = hydrus_gui_model::sidecar_editors::router_test_strings(
        &store,
        &saved,
        &slots.test_objects.borrow(),
    );
    assert_eq!(
        persisted.routers[0].route(inputs),
        ["source:item10", "source:item2", "json2", "json1"]
    );
    assert!(!directory.path().join("one.png.export.txt").exists());
    // Source processors inherit their unprocessed first example; formula children
    // inherit the reference source's parsed JSON texts with preserved newlines.
    let window = sidecars_window::open_router(
        &store,
        Context::Import,
        original.clone(),
        &slots,
        Rc::new({
            let accepted = accepted.clone();
            move |router| *accepted.borrow_mut() = Some(router)
        }),
    )
    .unwrap();
    *slots.router.borrow_mut() = Some(window.clone_strong());
    window.invoke_row_clicked(0, false, false);
    window.invoke_edit();
    let source = slots.node.borrow().as_ref().unwrap().clone_strong();
    source.invoke_edit_processing();
    let processor = slots
        .strings
        .processor
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    assert_eq!(
        serde_json::to_value(table(&processor.get_starting())).unwrap(),
        case["source_inputs"][0]["texts"]
    );
    window.invoke_cancel();
    assert!(slots.node.borrow().is_none());
    assert!(slots.strings.processor.borrow().is_none());
    processor.invoke_apply();
    source.invoke_apply();
    window.invoke_apply();
    assert_eq!(accepted.borrow().as_ref(), Some(&saved));
    let persisted: hydrus_gui_model::export_files::Preferences = store.read(settings::get).unwrap();
    assert_eq!(persisted.routers, [saved]);
    let window =
        sidecars_window::open_router(&store, Context::Import, original, &slots, Rc::new(|_| {}))
            .unwrap();
    *slots.router.borrow_mut() = Some(window.clone_strong());
    window.invoke_row_clicked(1, false, false);
    window.invoke_edit();
    let source = slots.node.borrow().as_ref().unwrap().clone_strong();
    source.invoke_edit_formula();
    let formula = slots
        .formula
        .formula
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    assert_eq!(
        formula.get_document(),
        case["source_inputs"][1]["texts"][0].as_str().unwrap()
    );
    assert_eq!(formula.get_examples().row_count(), 2);
    assert!(!formula.get_allow_type_change());
    window.invoke_cancel();
    assert!(slots.formula.formula.borrow().is_none());
}
