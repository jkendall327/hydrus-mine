//! Actual Options owner Apply/Cancel, captured controls and retained callbacks.
use hydrus_gui::{MainWindow, OptionsWindow, Pages, bind, headless};
use hydrus_store::settings::{self, ImportWorkSlots};
use serde_json::{Value, json};
use slint::{ComponentHandle as _, Model as _};
const LABELS: [&str; 5] = [
    "Number of gallery downloader file queues that can import at the same time:",
    "Number of gallery downloader searches that can run at the same time:",
    "Number of watcher page file queues that can run at the same time:",
    "Number of watcher page checkers that can run at the same time:",
    "Number of other paged importer jobs that can run at the same time:",
];
fn options(ui: &MainWindow, bound: &hydrus_gui::Bound) -> OptionsWindow {
    ui.invoke_menu_title_pressed(0, 20.0, 22.0);
    let lines = ui.get_menu_panes().row_data(0).unwrap().lines;
    let index = lines
        .iter()
        .position(|line| line.label == "options…")
        .unwrap();
    ui.invoke_menu_line_clicked(0, i32::try_from(index).unwrap(), 0.0, 0.0, 0.0);
    let window = bound.options.borrow().as_ref().unwrap().clone_strong();
    let index = window
        .get_pages()
        .iter()
        .position(|page| page.text == "importing")
        .unwrap();
    window.invoke_page_chosen(i32::try_from(index).unwrap());
    window
}
fn row(window: &OptionsWindow, label: &str) -> i32 {
    i32::try_from(
        window
            .get_rows()
            .iter()
            .position(|row| row.label == label)
            .unwrap(),
    )
    .unwrap()
}
fn edit(window: &OptionsWindow, event: &Value) {
    for (label, value) in LABELS.iter().zip(event["input"].as_array().unwrap()) {
        window.invoke_number_edited(
            row(window, label),
            i32::try_from(value.as_i64().unwrap()).unwrap(),
        );
    }
}
// leaf: audit-options-importing-work-slots-number-of-gallery-downloader-file-queues-that-can-import-at-the-same-time
// leaf: audit-options-importing-work-slots-number-of-gallery-downloader-searches-that-can-run-at-the-same-time
// leaf: audit-options-importing-work-slots-number-of-watcher-page-checkers-that-can-run-at-the-same-time
// leaf: audit-options-importing-work-slots-number-of-watcher-page-file-queues-that-can-run-at-the-same-time
#[test]
fn actual_importing_controls_cancel_apply_reopen_and_ignore_retired_edits() {
    let fixture = hydrus_testkit::fixture_json("import_work_slots.json");
    let (_dirs, store) = super::subscriptions::store();
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    let native = windows.get(0).unwrap();
    headless::render(&native, 1100, 750);
    for event in fixture["events"].as_array().unwrap() {
        let before = store.read(settings::get::<ImportWorkSlots>).unwrap();
        assert_eq!(json!(before), event["before"]);
        let cancelled = options(&ui, &bound);
        edit(&cancelled, event);
        assert_eq!(
            store.read(settings::get::<ImportWorkSlots>).unwrap(),
            before
        );
        cancelled.invoke_cancel();
        cancelled.invoke_number_edited(row(&cancelled, LABELS[0]), 123);
        cancelled.invoke_apply();
        assert!(bound.options.borrow().is_none());
        assert_eq!(
            store.read(settings::get::<ImportWorkSlots>).unwrap(),
            before
        );
        let accepted = options(&ui, &bound);
        cancelled.invoke_number_edited(row(&cancelled, LABELS[0]), 321);
        cancelled.invoke_apply();
        assert!(bound.options.borrow().is_some());
        assert_eq!(
            store.read(settings::get::<ImportWorkSlots>).unwrap(),
            before
        );
        edit(&accepted, event);
        accepted.invoke_apply();
        assert_eq!(
            json!(store.read(settings::get::<ImportWorkSlots>).unwrap()),
            event["saved"]
        );
        let reopened = options(&ui, &bound);
        for (label, key) in LABELS.iter().zip([
            "gallery_files",
            "gallery_search",
            "watcher_files",
            "watcher_check",
            "misc",
        ]) {
            let control = reopened
                .get_rows()
                .row_data(usize::try_from(row(&reopened, label)).unwrap())
                .unwrap();
            assert_eq!(json!(control.number), event["reopened"][key]);
        }
        reopened.invoke_cancel();
    }
    // Raw loaded values survive Cancel; untouched displayed controls normalize
    // on Apply just as the actual Qt spin boxes do.
    let boundary = &fixture["loaded_boundaries"];
    let raw: ImportWorkSlots = serde_json::from_value(boundary["before"].clone()).unwrap();
    store
        .write(move |ctx| settings::set(ctx.conn(), &raw))
        .unwrap();
    let cancelled = options(&ui, &bound);
    cancelled.invoke_cancel();
    assert_eq!(
        json!(store.read(settings::get::<ImportWorkSlots>).unwrap()),
        boundary["before"]
    );
    let accepted = options(&ui, &bound);
    for (label, key) in LABELS.iter().zip([
        "gallery_files",
        "gallery_search",
        "watcher_files",
        "watcher_check",
        "misc",
    ]) {
        let control = accepted
            .get_rows()
            .row_data(usize::try_from(row(&accepted, label)).unwrap())
            .unwrap();
        assert_eq!(json!(control.number), boundary["controls"][key]);
    }
    // Capture the exact importing page after its recorded loaded-boundary assertions.
    let pixels = headless::render(&windows.get(windows.count() - 1).unwrap(), 1100, 760);
    headless::save_png(
        &std::path::Path::new(env!("CARGO_TARGET_TMPDIR"))
            .join("options_import_work_slots_loaded.png"),
        &pixels,
        1100,
        760,
    )
    .unwrap();
    accepted.invoke_apply();
    assert_eq!(
        json!(store.read(settings::get::<ImportWorkSlots>).unwrap()),
        boundary["saved"]
    );
    let reopened_store = hydrus_store::Store::open(store.dir()).unwrap();
    assert_eq!(
        json!(
            reopened_store
                .read(settings::get::<ImportWorkSlots>)
                .unwrap()
        ),
        boundary["saved"]
    );
}
