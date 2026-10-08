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

// leaf: audit-shared-sidecar-import
#[test]
fn router_import_replays_permitted_subsets_and_ordered_png_failures() {
    use hydrus_downloader_exchange::routers as exchange;
    use hydrus_gui::{Pick, sidecars_window};
    use hydrus_gui_model::sidecar_editors::Context;
    use std::{cell::RefCell, rc::Rc};

    let (_dirs, store) = store();
    let windows = headless::init();
    let reference = hydrus_testkit::fixture_json("router_import.json");
    for case in reference["cases"].as_array().unwrap() {
        let slots = sidecars_window::Slots::default();
        let saved = Rc::new(RefCell::new(Vec::new()));
        let context = if case["context"] == "import" {
            Context::Import
        } else {
            Context::Export
        };
        let queue = sidecars_window::open_routers(
            &store,
            context,
            Vec::new(),
            &slots,
            Rc::new({
                let saved = saved.clone();
                move |routers| *saved.borrow_mut() = routers
            }),
        )
        .unwrap();
        *slots.routers.borrow_mut() = Some(queue.clone_strong());
        queue.invoke_exchange(true);
        let child = slots.exchange.0.borrow().as_ref().unwrap().clone_strong();
        assert!(child.get_router_import());
        assert!(
            !child.get_json_enabled(),
            "Qt's router list does not offer JSON-file import"
        );
        if let Some(files) = case["files"].as_array() {
            let paths = files
                .iter()
                .map(|name| {
                    hydrus_testkit::fixtures_dir()
                        .join(format!("router_import_{}", name.as_str().unwrap()))
                })
                .collect::<Vec<_>>();
            hydrus_gui::set_picker(move |kind, title| {
                assert_eq!(kind, Pick::Files);
                assert_eq!(title, "select the png or pngs with the encoded data");
                paths.clone()
            });
            child.invoke_action("import-pngs".into());
        } else {
            child.set_text(case["text"].to_string().into());
            child.invoke_action("review".into());
        }
        let expected = case["added"].as_array().unwrap();
        let warnings = case["messages"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|message| match message[0].as_str().unwrap() {
                "warning" => Some(message[1].as_str().unwrap().to_owned()),
                "critical" => Some(format!(
                    "{}\n\n{}",
                    message[1].as_str().unwrap(),
                    message[2].as_str().unwrap()
                )),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(
            child.get_error().as_str(),
            warnings.join("\n\n"),
            "{} {}",
            case["name"],
            case["context"]
        );
        assert_eq!(child.get_ready(), !expected.is_empty());
        if case["name"] == "ordered_pngs" {
            let adapter = windows.get(windows.count() - 1).unwrap();
            let pixels = headless::render(&adapter, 900, 800);
            headless::save_png(
                &std::path::Path::new(env!("CARGO_TARGET_TMPDIR"))
                    .join("router-import-permitted-pngs.png"),
                &pixels,
                900,
                800,
            )
            .unwrap();
            assert!(
                pixels
                    .chunks_exact(4)
                    .any(|pixel| pixel[0] != pixel[1] || pixel[1] != pixel[2])
            );
        }
        assert!(saved.borrow().is_empty());
        assert_eq!(
            queue.get_rows().row_count(),
            0,
            "review must not mutate the owning draft"
        );
        if expected.is_empty() {
            child.invoke_action("accept".into());
            assert_eq!(queue.get_rows().row_count(), 0);
            child.invoke_action("cancel".into());
            queue.invoke_cancel();
        } else {
            child.invoke_action("accept".into());
            assert_eq!(queue.get_rows().row_count(), expected.len());
            assert_eq!(
                (0..expected.len())
                    .filter(|&i| queue.get_rows().row_data(i).unwrap().selected)
                    .count(),
                expected.len()
            );
            assert!(
                saved.borrow().is_empty(),
                "only queue Apply may hand back routers"
            );
            queue.invoke_apply();
            assert_eq!(
                saved
                    .borrow()
                    .iter()
                    .map(|r| exchange::tuple(r).unwrap())
                    .collect::<Vec<_>>(),
                *expected,
                "{} {}",
                case["name"],
                case["context"]
            );
        }
        assert!(slots.exchange.0.borrow().is_none());
        assert!(slots.routers.borrow().is_none());
    }
}

#[test]
fn router_import_child_obeys_hidden_cancel_replacement_and_dropped_owner() {
    use hydrus_gui::sidecars_window;
    use hydrus_gui_model::sidecar_editors::Context;
    use std::{cell::RefCell, rc::Rc};

    let (_dirs, store) = store();
    let _windows = headless::init();
    let reference = hydrus_testkit::fixture_json("router_import.json");
    let slots = sidecars_window::Slots::default();
    let saved = Rc::new(RefCell::new(Vec::new()));
    let applied: Rc<dyn Fn(Vec<hydrus_parse::sidecar::Router>)> = Rc::new({
        let saved = saved.clone();
        move |routers| *saved.borrow_mut() = routers
    });
    let queue =
        sidecars_window::open_routers(&store, Context::Import, Vec::new(), &slots, applied.clone())
            .unwrap();
    *slots.routers.borrow_mut() = Some(queue.clone_strong());
    queue.invoke_exchange(true);
    let child = slots.exchange.0.borrow().as_ref().unwrap().clone_strong();
    child.set_text(reference["cases"][0]["text"].to_string().into());
    queue.hide().unwrap();
    child.invoke_action("review".into());
    assert!(!child.get_ready());
    queue.show().unwrap();
    child.hide().unwrap();
    child.invoke_action("review".into());
    assert!(!child.get_ready());
    child.show().unwrap();
    child.invoke_action("review".into());
    assert!(child.get_ready());
    queue.hide().unwrap();
    child.invoke_action("accept".into());
    assert_eq!(queue.get_rows().row_count(), 0);
    queue.show().unwrap();
    child.invoke_action("accept".into());
    assert_eq!(queue.get_rows().row_count(), 2);
    queue.invoke_row_clicked(0, false, false);
    queue.invoke_exchange(true);
    let retired = slots.exchange.0.borrow().as_ref().unwrap().clone_strong();
    retired.set_text(reference["cases"][1]["text"].to_string().into());
    retired.invoke_action("review".into());
    queue.invoke_cancel();
    assert!(saved.borrow().is_empty());
    assert!(!retired.get_active());
    let successor =
        sidecars_window::open_routers(&store, Context::Import, Vec::new(), &slots, applied.clone())
            .unwrap();
    *slots.routers.borrow_mut() = Some(successor.clone_strong());
    retired.show().unwrap();
    retired.invoke_action("accept".into());
    retired.invoke_action("cancel".into());
    assert_eq!(successor.get_rows().row_count(), 0);
    assert!(slots.routers.borrow().is_some());
    successor.invoke_exchange(true);
    let orphan = slots.exchange.0.borrow().as_ref().unwrap().clone_strong();
    orphan.set_text(reference["cases"][0]["text"].to_string().into());
    orphan.invoke_action("review".into());
    assert!(orphan.get_ready());
    let weak = successor.as_weak();
    successor.hide().unwrap();
    drop(successor);
    drop(slots);
    assert!(
        weak.upgrade().is_none(),
        "queue callbacks must not retain their own window slot"
    );
    orphan.show().unwrap();
    orphan.invoke_action("accept".into());
    assert!(saved.borrow().is_empty());
    orphan.invoke_action("cancel".into());
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

// leaf: string-conversion-tests
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

// leaf: audit-network-conversion-date-fields
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

// leaf: sidecar-test
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
    assert_eq!(persisted.routers.as_slice(), std::slice::from_ref(&saved));
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
#[test]
fn router_queue_exchange_is_staged_context_checked_and_reaches_manual_export() {
    use hydrus_downloader_exchange::routers as exchange;
    use hydrus_gui::{Clip, sidecars_window};
    use hydrus_gui_model::{export_files, sidecar_editors::Context};
    use serde_json::Value;
    use std::{cell::RefCell, rc::Rc, sync::atomic::AtomicBool};
    let (_dirs, store) = store();
    let windows = headless::init();
    let reference = hydrus_testkit::fixture_json("router_exchange.json");
    let slots = sidecars_window::Slots::default();
    let applied = Rc::new({
        let store = store.clone();
        move |routers| {
            store
                .write_and_refresh(move |ctx| {
                    settings::set(
                        ctx.conn(),
                        &export_files::Preferences {
                            routers,
                            ..export_files::Preferences::default()
                        },
                    )
                })
                .unwrap();
        }
    });
    let open = || {
        let window = sidecars_window::open_routers(
            &store,
            Context::Export,
            Vec::new(),
            &slots,
            applied.clone(),
        )
        .unwrap();
        *slots.routers.borrow_mut() = Some(window.clone_strong());
        window
    };
    let child = || slots.exchange.0.borrow().as_ref().unwrap().clone_strong();
    let selected = |window: &hydrus_gui::SidecarRoutersWindow| {
        let rows = window.get_rows();
        (0..rows.row_count())
            .filter(|&row| rows.row_data(row).unwrap().selected)
            .count()
    };
    let window = open();
    window.invoke_exchange(true);
    let import = child();
    import.set_text(reference["imports"][0].to_string().into());
    import.invoke_action("review".into());
    assert!(!import.get_ready());
    assert_eq!(
        import.get_error(),
        format!(
            "The imported objects were wrong for this control:\n\n{}",
            reference["vetoes"][1]["error"].as_str().unwrap()
        )
    );
    assert_eq!(window.get_rows().row_count(), 0);
    let subset_reference = hydrus_testkit::fixture_json("router_import.json");
    let mixed = subset_reference["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|case| case["name"] == "mixed" && case["context"] == "export")
        .unwrap();
    import.set_text(mixed["text"].to_string().into());
    import.invoke_action("review".into());
    assert!(import.get_ready(), "{}", import.get_error());
    window.invoke_apply();
    assert!(slots.routers.borrow().is_some());
    assert!(
        store
            .read(settings::get::<export_files::Preferences>)
            .unwrap()
            .routers
            .is_empty()
    );
    import.invoke_action("accept".into());
    assert_eq!(window.get_rows().row_count(), 2);
    assert_eq!(selected(&window), 2);
    assert!(!window.get_child_open());
    window.invoke_exchange(false);
    let export = child();
    assert_eq!(
        serde_json::from_str::<Value>(&export.get_text()).unwrap(),
        reference["queues"][1]["text"]
    );
    let copied = Rc::new(RefCell::new(String::new()));
    hydrus_gui::set_clipper({
        let copied = copied.clone();
        move |clip| {
            if let Clip::Text(text) = clip {
                *copied.borrow_mut() = text.clone();
            }
        }
    });
    export.invoke_action("copy".into());
    assert_eq!(
        serde_json::from_str::<Value>(&copied.borrow()).unwrap(),
        reference["queues"][1]["text"]
    );
    let files = tempfile::tempdir().unwrap();
    let png = files.path().join("routers.png");
    export.set_path(png.to_string_lossy().into_owned().into());
    export.invoke_action("save".into());
    assert_eq!(
        exchange::decode_png(&std::fs::read(&png).unwrap())
            .unwrap()
            .len(),
        2
    );
    export.invoke_action("cancel".into());
    window.invoke_duplicate();
    assert_eq!(window.get_rows().row_count(), 4);
    assert_eq!(selected(&window), 4);
    assert_eq!(reference["queues"][1]["duplicate_selected"], 4);
    window.invoke_row_clicked(2, false, false);
    window.invoke_row_clicked(3, true, false);
    window.invoke_delete();
    assert_eq!(window.get_asking_message(), "Remove 2 selected?");
    window.invoke_duplicate();
    assert_eq!(window.get_rows().row_count(), 4);
    window.invoke_chosen(1);
    assert_eq!(window.get_rows().row_count(), 4);
    window.invoke_delete();
    window.invoke_chosen(0);
    assert_eq!(window.get_rows().row_count(), 2);
    let pixels = headless::render(&windows.get(0).unwrap(), 900, 450);
    headless::save_png(
        &std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("router_queue_exchange.png"),
        &pixels,
        900,
        450,
    )
    .unwrap();
    window.invoke_row_clicked(1, false, false);
    window.invoke_edit();
    let router = slots.router.borrow().as_ref().unwrap().clone_strong();
    assert!(window.get_child_open());
    window.invoke_apply();
    assert!(
        store
            .read(settings::get::<export_files::Preferences>)
            .unwrap()
            .routers
            .is_empty()
    );
    router.invoke_cancel();
    assert!(!window.get_child_open());
    window.invoke_apply();
    let saved = store
        .read(settings::get::<export_files::Preferences>)
        .unwrap()
        .routers;
    assert_eq!(saved.len(), 2);
    assert_eq!(exchange::tuple(&saved[1]).unwrap(), reference["exports"][1]);
    // An accepted queue reaches the actual export worker and its written sidecar.
    let recorded = hydrus_testkit::fixture_json("export_files.json");
    let file = hydrus_core::HashId(
        u32::try_from(recorded["files"][0]["file_id"].as_u64().unwrap()).unwrap(),
    );
    let url = reference["consumer"]["url"].as_str().unwrap().to_owned();
    store
        .write_content(move |writer| writer.add_urls(&[file], &[url]))
        .unwrap();
    let plan = export_files::Plan {
        directory: files.path().to_owned(),
        rows: export_files::preview(&store, &[file], files.path().to_str().unwrap(), "{file_id}")
            .unwrap(),
        routers: vec![saved[1].clone()],
        trash: false,
        symlinks: false,
    };
    let Exporter::Txt { naming, .. } = &saved[1].exporter else {
        panic!("TXT destination")
    };
    let path = naming.path(plan.rows[0].destination.to_str().unwrap(), "txt");
    let result = export_files::run(&store, &plan, &AtomicBool::new(false), |_| {});
    assert_eq!(result.completed, 1, "{:?}", result.error);
    assert!(result.error.is_none());
    assert_eq!(
        std::fs::read_to_string(path).unwrap(),
        reference["consumer"]["sidecar"]
    );
    // Owner cancellation discards a reviewed package and invalidates old handles.
    let stale_owner = open();
    stale_owner.invoke_exchange(true);
    let stale_import = child();
    stale_import.set_text(reference["queues"][1]["text"].to_string().into());
    stale_import.invoke_action("review".into());
    assert!(stale_import.get_ready());
    slots.cancel();
    assert!(slots.routers.borrow().is_none());
    assert!(!slots.exchange.has_open());
    stale_import.invoke_action("accept".into());
    stale_owner.invoke_apply();
    assert_eq!(
        store
            .read(settings::get::<export_files::Preferences>)
            .unwrap()
            .routers,
        saved
    );
    let stale_owner = open();
    stale_owner.invoke_add();
    let stale_router = slots.router.borrow().as_ref().unwrap().clone_strong();
    stale_router.invoke_add();
    stale_router.invoke_chosen(2);
    let stale_source = slots.node.borrow().as_ref().unwrap().clone_strong();
    slots.cancel();
    assert!(slots.router.borrow().is_none());
    assert!(slots.node.borrow().is_none());
    stale_source.invoke_apply();
    stale_router.invoke_apply();
    stale_owner.invoke_apply();
    assert_eq!(
        store
            .read(settings::get::<export_files::Preferences>)
            .unwrap()
            .routers,
        saved
    );
}

#[test]
fn closing_manual_export_discards_its_pending_router_exchange() {
    use hydrus_gui::{export_files_window, sidecar_editors::Context};
    use std::rc::Rc;
    let (_dirs, store) = store();
    let _windows = headless::init();
    let slots = export_files_window::Slots::default();
    let owner = export_files_window::open(&store, Vec::new(), &slots, Rc::new(|| {})).unwrap();
    *slots.window.borrow_mut() = Some(owner.clone_strong());
    let original = store
        .read(settings::get::<hydrus_gui_model::export_files::Preferences>)
        .unwrap()
        .routers;
    owner.invoke_edit_sidecars();
    let routers = slots
        .sidecars
        .routers
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    routers.invoke_exchange(true);
    let exchange = slots
        .sidecars
        .exchange
        .0
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    let reference = hydrus_testkit::fixture_json("router_exchange.json");
    exchange.set_text(reference["queues"][1]["text"].to_string().into());
    exchange.invoke_action("review".into());
    assert!(exchange.get_ready(), "{}", exchange.get_error());
    owner.invoke_dismissed();
    assert!(slots.window.borrow().is_none());
    assert!(slots.sidecars.routers.borrow().is_none());
    assert!(!slots.sidecars.exchange.has_open());
    exchange.invoke_action("accept".into());
    routers.invoke_apply();
    assert_eq!(
        store
            .read(settings::get::<hydrus_gui_model::export_files::Preferences>)
            .unwrap()
            .routers,
        original
    );
    // Reopening the concrete owner inherits its persisted choices, not the discarded draft.
    let owner = export_files_window::open(&store, Vec::new(), &slots, Rc::new(|| {})).unwrap();
    *slots.window.borrow_mut() = Some(owner.clone_strong());
    owner.invoke_edit_sidecars();
    let routers = slots
        .sidecars
        .routers
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    assert_eq!(routers.get_rows().row_count(), original.len());
    hydrus_gui_model::sidecar_editors::validate_router_import(Context::Export, &original).unwrap();
    owner.invoke_dismissed();
}

#[test]
fn router_png_child_has_recorded_parameters_and_closes_with_its_queue_owner() {
    use hydrus_gui::sidecars_window;
    use hydrus_gui_model::{png_export, sidecar_editors::Context};
    use std::{cell::Cell, rc::Rc};
    let (_dirs, store) = store();
    let windows = headless::init();
    let reference = hydrus_testkit::fixture_json("parser_png_export.json");
    let case = reference
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["case"] == "router_queue")
        .unwrap();
    let routers =
        hydrus_downloader_exchange::routers::decode_text(&case["payload"].to_string()).unwrap();
    let slots = sidecars_window::Slots::default();
    let applied = Rc::new(Cell::new(false));
    let owner = sidecars_window::open_routers(
        &store,
        Context::Export,
        routers,
        &slots,
        Rc::new({
            let applied = applied.clone();
            move |_| applied.set(true)
        }),
    )
    .unwrap();
    *slots.routers.borrow_mut() = Some(owner.clone_strong());
    owner.invoke_row_clicked(0, false, false);
    owner.invoke_row_clicked(1, true, false);
    owner.invoke_exchange(false);
    let exchange = slots.exchange.0.borrow().as_ref().unwrap().clone_strong();
    assert!(exchange.get_png_enabled());
    exchange.invoke_export_png();
    let png = slots.exchange.1.window().unwrap();
    assert_eq!(png.get_png_title(), case["default_title"].as_str().unwrap());
    assert_eq!(
        png.get_payload_description(),
        case["summary"].as_str().unwrap()
    );
    assert_eq!(png.get_description(), "");
    assert_eq!(png.get_png_width(), 512);
    assert!(exchange.get_png_child());
    let files = tempfile::tempdir().unwrap();
    let path = files.path().join("typed-router");
    png.set_path(path.to_string_lossy().into_owned().into());
    png.set_png_title("recorded queue 日本".into());
    png.set_description("synthetic typed export".into());
    png.set_png_width(300);
    png.invoke_action("update".into());
    assert!(png.get_can_export());
    owner.invoke_apply();
    assert!(!applied.get());
    let blocked = files.path().join("blocked.png");
    exchange.set_path(blocked.to_string_lossy().into_owned().into());
    exchange.invoke_action("save".into());
    assert!(!blocked.exists());
    png.invoke_action("export".into());
    assert!(png.get_done(), "{}", png.get_error());
    let data = std::fs::read(path.with_extension("png")).unwrap();
    assert_eq!(hydrus_media::decode_image(&data).unwrap().width(), 300);
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(
            &hydrus_downloader_exchange::text_png::decode(&data).unwrap()
        )
        .unwrap(),
        case["loaded"]
    );
    assert_eq!(
        store
            .read(settings::get::<png_export::Directory>)
            .unwrap()
            .0,
        Some(files.path().to_string_lossy().into_owned())
    );
    let pixels = headless::render(&windows.get(windows.count() - 1).unwrap(), 720, 380);
    headless::save_png(
        &std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("typed_router_png.png"),
        &pixels,
        720,
        380,
    )
    .unwrap();
    png.invoke_action("close".into());
    assert!(!exchange.get_png_child());
    exchange.invoke_export_png();
    let stale = slots.exchange.1.window().unwrap();
    let stale_path = files.path().join("stale.png");
    stale.set_path(stale_path.to_string_lossy().into_owned().into());
    slots.cancel();
    assert!(slots.routers.borrow().is_none());
    assert!(slots.exchange.1.window().is_none());
    stale.invoke_action("export".into());
    exchange.invoke_export_png();
    owner.invoke_apply();
    assert!(!stale_path.exists());
    assert!(!applied.get());
    assert!(slots.exchange.1.window().is_none());
}

// leaf: audit-network-export-folder-examples
// leaf: sidecar-test
#[test]
fn export_folder_query_examples_refresh_media_and_reach_source_children() {
    use hydrus_core::search::context::FileSearchContext;
    use hydrus_core::url::strings::StringProcessor;
    use hydrus_gui_model::folders::new_export_folder;
    use hydrus_parse::sidecar::Importer;
    let (_dirs, store) = store();
    let reference = hydrus_testkit::fixture_json("export_folder_examples.json");
    let exchange = hydrus_testkit::fixture_json("router_exchange.json");
    let mut router =
        hydrus_downloader_exchange::routers::decode_text(&exchange["exports"][1].to_string())
            .unwrap()
            .remove(0);
    router.importers = vec![Importer {
        source: Source::MediaUrls,
        processor: StringProcessor::default(),
    }];
    router.processor = StringProcessor::default();
    let mut folder = new_export_folder("{hash}".into(), FileSearchContext::default());
    folder.routers = vec![router];
    let original = settings::ExportFolders(vec![folder]);
    store
        .write({
            let original = original.clone();
            move |ctx| settings::set(ctx.conn(), &original)
        })
        .unwrap();
    let rendered = headless::init();
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
    list.invoke_row_clicked(0, false, false);
    list.invoke_edit();
    let edit = bound
        .folders
        .export_edit
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    let slots = &bound.folders.sidecars;
    assert_eq!(
        edit.get_examples_label(),
        reference["initial"]["text"].as_str().unwrap()
    );
    assert!(edit.get_examples_enabled());
    assert!(slots.test_objects.borrow().is_empty());
    for case in reference["cases"].as_array().unwrap() {
        while edit.get_predicates().row_count() > 0 {
            edit.invoke_predicate_removed(0);
        }
        for tag in case["tags"].as_array().unwrap() {
            edit.set_typed(tag.as_str().unwrap().into());
            edit.invoke_typed_accepted();
            assert!(edit.get_error().is_empty());
        }
        assert_eq!(
            edit.get_examples_label(),
            case["before"]["text"].as_str().unwrap()
        );
        assert!(edit.get_examples_enabled());
        edit.invoke_update_examples();
        assert_eq!(
            edit.get_examples_label(),
            case["loading"]["text"].as_str().unwrap()
        );
        assert!(!edit.get_examples_enabled());
        crate::parser_editors::until_fetch(|| edit.get_examples_label() != "loading\u{2026}");
        assert!(edit.get_error().is_empty());
        assert_eq!(
            edit.get_examples_label(),
            case["finished"]["text"].as_str().unwrap()
        );
        assert!(!edit.get_examples_enabled());
        edit.invoke_edit_sidecars();
        let queue = slots.routers.borrow().as_ref().unwrap().clone_strong();
        queue.invoke_row_clicked(0, false, false);
        queue.invoke_edit();
        let router = slots.router.borrow().as_ref().unwrap().clone_strong();
        let rows = test_table(&router.get_test_rows());
        assert_eq!(
            rows.iter().map(|r| r[0].clone()).collect::<Vec<_>>(),
            case["finished"]["media"]
                .as_array()
                .unwrap()
                .iter()
                .map(|m| m["hash"].as_str().unwrap().to_owned())
                .collect::<Vec<_>>()
        );
        router.invoke_row_clicked(0, false, false);
        router.invoke_edit();
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
            case["finished"]["source_strings"]
        );
        processor.invoke_cancel();
        source.invoke_cancel();
        router.invoke_cancel();
        queue.invoke_cancel();
    }
    let pixels = headless::render(&rendered.get(2).unwrap(), 760, 820);
    assert_eq!(pixels.len(), 760 * 820 * 4);
    headless::save_png(
        &std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("export-folder-examples.png"),
        &pixels,
        760,
        820,
    )
    .unwrap();
    // A query edit keeps the last examples until refresh, as the reference does.
    let previous = slots.test_objects.borrow().clone();
    while edit.get_predicates().row_count() > 0 {
        edit.invoke_predicate_removed(0);
    }
    edit.set_typed("synthetic:export-example-no-match".into());
    edit.invoke_typed_accepted();
    assert!(edit.get_examples_enabled());
    edit.invoke_edit_sidecars();
    assert_eq!(*slots.test_objects.borrow(), previous);
    edit.invoke_apply();
    assert!(
        bound.folders.export_edit.borrow().is_some(),
        "a modal router owns the staged edit"
    );
    slots.cancel();
    // Close the list while the real query is pending. Old callbacks cannot publish
    // examples, reopen children or commit a folder into a subsequent owner.
    edit.invoke_update_examples();
    assert_eq!(edit.get_examples_label(), "loading\u{2026}");
    list.invoke_cancel();
    assert!(bound.folders.export_edit.borrow().is_none());
    edit.invoke_apply();
    edit.invoke_edit_sidecars();
    edit.invoke_update_examples();
    slint::platform::update_timers_and_animations();
    assert_eq!(edit.get_examples_label(), "loading\u{2026}");
    assert!(slots.routers.borrow().is_none());
    assert_eq!(
        store
            .read(settings::get::<settings::ExportFolders>)
            .unwrap(),
        original
    );
    open(&ui, "manage export folders\u{2026}");
    let list = bound
        .folders
        .export_list
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    list.invoke_row_clicked(0, false, false);
    list.invoke_edit();
    let reopened = bound
        .folders
        .export_edit
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    assert_eq!(reopened.get_examples_label(), "update test example files");
    assert!(slots.test_objects.borrow().is_empty());
    let destination = tempfile::tempdir().unwrap();
    reopened.set_path(destination.path().to_string_lossy().into_owned().into());
    reopened.set_typed("system:limit=2".into());
    reopened.invoke_typed_accepted();
    reopened.invoke_update_examples();
    crate::parser_editors::until_fetch(|| reopened.get_examples_label() != "loading\u{2026}");
    assert_eq!(reopened.get_examples_label(), "got 2 files!");
    assert_eq!(std::fs::read_dir(destination.path()).unwrap().count(), 0);
    reopened.invoke_apply();
    assert!(bound.folders.export_edit.borrow().is_none());
    assert_eq!(
        store
            .read(settings::get::<settings::ExportFolders>)
            .unwrap(),
        original
    );
    list.invoke_apply();
    let written = store
        .read(settings::get::<settings::ExportFolders>)
        .unwrap();
    assert_eq!(written.0.len(), 1);
    let ids =
        hydrus_gui_model::folders::export_test_examples(&store, &written.0[0].search).unwrap();
    assert_eq!(
        ids.iter().map(|id| u64::from(id.0)).collect::<Vec<_>>(),
        reference["cases"][1]["finished"]["media"]
            .as_array()
            .unwrap()
            .iter()
            .map(|m| m["id"].as_u64().unwrap())
            .collect::<Vec<_>>()
    );
}

fn json_object_name_child(slots: &hydrus_gui::sidecars_window::Slots) -> hydrus_gui::SessionDialog {
    slots.object_name.borrow().as_ref().unwrap().clone_strong()
}

#[test]
fn json_object_names_use_staged_text_children_and_saved_router_worker() {
    use hydrus_core::url::strings::StringProcessor;
    use hydrus_gui::{
        sidecar_editors::Context,
        sidecars_window::{self, Node, Slots},
    };
    use hydrus_parse::sidecar::{Importer, Router, SidecarNaming};
    use std::{cell::RefCell, rc::Rc, sync::atomic::AtomicBool};
    let (_dirs, store) = store();
    let windows = headless::init();
    let slots = Slots::default();
    let accepted = Rc::new(RefCell::new(None));
    let initial = Exporter::Json {
        naming: SidecarNaming {
            remove_actual_filename_ext: false,
            suffix: String::new(),
            filename_converter: hydrus_core::url::strings::StringConverter::default(),
        },
        nested_object_names: Vec::new(),
    };
    let open = || {
        sidecars_window::open_node(
            &store,
            Context::Export,
            &Node::Destination(initial.clone()),
            &slots,
            Rc::new({
                let accepted = accepted.clone();
                move |value| *accepted.borrow_mut() = Some(value)
            }),
        )
        .unwrap()
    };
    let node = open();
    let node_window = windows.get(windows.count() - 1).unwrap();
    *slots.node.borrow_mut() = Some(node.clone_strong());
    let reference = hydrus_testkit::fixture_json("sidecar_json_names.json");
    for state in reference["states"].as_array().unwrap() {
        match state["case"].as_str().unwrap() {
            "add" => {
                node.invoke_nested_action("add".into());
                let child = json_object_name_child(&slots);
                assert_eq!(child.get_window_title(), "Enter Text");
                assert_eq!(
                    child.get_message(),
                    reference["inputs"][0]["message"].as_str().unwrap()
                );
                assert!(child.get_text().is_empty());
                // The parent cannot Apply while its modal text child owns input.
                node.invoke_apply();
                assert!(accepted.borrow().is_none());
                child.invoke_name_entered(
                    state["names"]
                        .as_array()
                        .unwrap()
                        .last()
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .into(),
                );
            }
            "cancel_add" | "blank_add_veto" => {
                node.invoke_nested_action("add".into());
                let child = json_object_name_child(&slots);
                if state["case"] == "blank_add_veto" {
                    child.invoke_name_entered("".into());
                    assert_eq!(
                        child.get_warning(),
                        reference["inputs"][6]["error"].as_str().unwrap()
                    );
                    assert!(slots.object_name.borrow().is_some());
                }
                child.invoke_cancelled();
                child.invoke_name_entered("stale must not add".into());
            }
            "edit_first_of_selection" => {
                node.invoke_nested_clicked(1, false, false);
                node.invoke_nested_clicked(3, true, false);
                node.invoke_nested_action("edit".into());
                let child = json_object_name_child(&slots);
                assert_eq!(
                    child.get_text(),
                    reference["inputs"][7]["default"].as_str().unwrap()
                );
                child
                    .invoke_name_entered(reference["inputs"][7]["answer"].as_str().unwrap().into());
            }
            "cancel_edit" => {
                node.invoke_nested_action("edit".into());
                json_object_name_child(&slots).invoke_cancelled();
            }
            "up_selected" => {
                node.invoke_nested_action("up".into());
            }
            "down_selected" => {
                node.invoke_nested_action("down".into());
            }
            "cancel_delete" | "delete_selected" => {
                node.invoke_nested_action("delete".into());
                let child = json_object_name_child(&slots);
                assert_eq!(child.get_message(), "Remove 2 selected?");
                child.invoke_answered(state["case"] == "delete_selected");
            }
            _ => {}
        }
        assert_eq!(
            serde_json::json!(labels(&node.get_nested_rows())),
            state["names"]
        );
        assert_eq!(
            serde_json::json!(
                node.get_nested_rows()
                    .iter()
                    .enumerate()
                    .filter_map(|(i, row)| row.selected.then_some(i))
                    .collect::<Vec<_>>()
            ),
            state["selected"]
        );
        assert!(slots.object_name.borrow().is_none());
    }
    // Later text-entry windows are retired; render the still-open JSON editor.
    let rendered = headless::render(&node_window, 640, 560);
    assert!(rendered.chunks_exact(4).any(|pixel| pixel[3] != 0));
    headless::save_png(
        &std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("sidecar_json_names.png"),
        &rendered,
        640,
        560,
    )
    .unwrap();
    node.invoke_apply();
    let Node::Destination(exporter) = accepted.borrow_mut().take().unwrap() else {
        panic!("JSON exporter");
    };
    let Exporter::Json {
        ref nested_object_names,
        ..
    } = exporter
    else {
        panic!("JSON exporter");
    };
    assert_eq!(
        serde_json::json!(nested_object_names),
        reference["states"].as_array().unwrap().last().unwrap()["names"]
    );
    let router = Router {
        importers: vec![Importer {
            processor: StringProcessor::default(),
            source: Source::MediaUrls,
        }],
        processor: StringProcessor::default(),
        exporter,
    };
    let persisted = hydrus_gui_model::export_files::Preferences {
        routers: vec![router],
        ..hydrus_gui_model::export_files::Preferences::default()
    };
    store
        .write(move |ctx| settings::set(ctx.conn(), &persisted))
        .unwrap();
    let saved: hydrus_gui_model::export_files::Preferences = store.read(settings::get).unwrap();
    let file = hydrus_core::HashId(1);
    let url = "https://sidecar-keys.example/one".to_owned();
    store
        .write_content(move |writer| writer.add_urls(&[file], &[url]))
        .unwrap();
    let directory = tempfile::tempdir().unwrap();
    let plan = hydrus_gui_model::export_files::Plan {
        directory: directory.path().to_owned(),
        rows: hydrus_gui_model::export_files::preview(
            &store,
            &[file],
            directory.path().to_str().unwrap(),
            "{file_id}",
        )
        .unwrap(),
        routers: saved.routers,
        trash: false,
        symlinks: false,
    };
    let Exporter::Json { naming, .. } = &plan.routers[0].exporter else {
        panic!("JSON exporter");
    };
    let path = naming.path(plan.rows[0].destination.to_str().unwrap(), "json");
    std::fs::write(
        &path,
        r#"{"keep":{"unknown":true},"files":{"other":"preserved"}}"#,
    )
    .unwrap();
    let result =
        hydrus_gui_model::export_files::run(&store, &plan, &AtomicBool::new(false), |_| {});
    assert!(result.error.is_none(), "{:?}", result.error);
    assert_eq!(result.completed, 1);
    let output: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    assert_eq!(output["keep"]["unknown"], true);
    assert_eq!(output["files"]["other"], "preserved");
    assert!(
        output["files"]["files"]["line\nbreak"]
            .as_array()
            .unwrap()
            .iter()
            .any(|row| row == "https://sidecar-keys.example/one")
    );
    // Force-closing the parent discards the pending child and invalidates both
    // retained handles without touching the saved router.
    let owner = open();
    *slots.node.borrow_mut() = Some(owner.clone_strong());
    owner.invoke_nested_action("add".into());
    let stale = json_object_name_child(&slots);
    slots.cancel();
    stale.invoke_name_entered("late".into());
    owner.invoke_apply();
    assert!(accepted.borrow().is_none());
    assert!(slots.node.borrow().is_none());
    assert!(slots.object_name.borrow().is_none());
    assert_eq!(
        store
            .read(settings::get::<hydrus_gui_model::export_files::Preferences>)
            .unwrap()
            .routers,
        plan.routers
    );
}

// leaf: audit-shared-sidecar-export
#[test]
fn the_router_list_exports_its_selected_routers_duplicates_and_reads_its_own_export_back() {
    use hydrus_downloader_exchange::routers as exchange;
    use hydrus_gui::{Clip, sidecars_window};
    use hydrus_gui_model::sidecar_editors::Context;
    use std::{cell::RefCell, rc::Rc};

    let (_dirs, store) = store();
    let _windows = headless::init();
    let copied: Rc<RefCell<Vec<String>>> = Rc::default();
    hydrus_gui::set_clipper({
        let copied = copied.clone();
        move |clip| {
            if let Clip::Text(text) = clip {
                copied.borrow_mut().push(text.clone());
            }
        }
    });
    // routers the reference itself wrote (its own import fixture's)
    let reference = hydrus_testkit::fixture_json("router_import.json");
    let case = reference["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| {
            c["context"] == "import"
                && c["text"].is_array()
                && !c["added"].as_array().unwrap().is_empty()
        })
        .expect("a case that adds routers");
    let routers = exchange::inspect_text(&case["text"].to_string())
        .unwrap()
        .routers;
    assert!(!routers.is_empty());
    let n = routers.len();
    let slots = sidecars_window::Slots::default();
    let queue = sidecars_window::open_routers(
        &store,
        Context::Import,
        routers.clone(),
        &slots,
        Rc::new(|_| {}),
    )
    .unwrap();
    *slots.routers.borrow_mut() = Some(queue.clone_strong());
    assert_eq!(queue.get_rows().row_count(), n);

    // nothing selected: nothing to export
    assert!(!queue.get_any_selected());
    queue.invoke_exchange(false);
    assert!(slots.exchange.0.borrow().is_none());

    // select the first and export: the reference's text for it, copyable
    queue.invoke_row_clicked(0, false, false);
    queue.invoke_exchange(false);
    let child = slots
        .exchange
        .0
        .borrow()
        .as_ref()
        .expect("opens")
        .clone_strong();
    assert!(!child.get_router_import());
    let exported = exchange::decode_text(child.get_text().as_str()).unwrap();
    assert_eq!(
        exchange::tuple(&exported[0]).unwrap(),
        exchange::tuple(&routers[0]).unwrap()
    );
    assert_eq!(exported.len(), 1);
    child.invoke_action("copy".into());
    assert_eq!(copied.borrow().last().unwrap(), child.get_text().as_str());
    child.invoke_action("cancel".into());
    assert!(slots.exchange.0.borrow().is_none());

    // duplicate puts a whole copy after the list and selects both
    queue.invoke_duplicate();
    assert_eq!(queue.get_rows().row_count(), n + 1);
    let rows = queue.get_rows();
    assert_eq!(
        rows.row_data(0).unwrap().cells.row_data(0),
        rows.row_data(n).unwrap().cells.row_data(0)
    );
}
