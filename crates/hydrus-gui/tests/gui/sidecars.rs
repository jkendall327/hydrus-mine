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
