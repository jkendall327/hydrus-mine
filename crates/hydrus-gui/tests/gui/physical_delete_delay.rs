//! Real Options fields persist the preference used by the physical maintenance loop.
use hydrus_gui::{MainWindow, OptionsWindow, Pages, bind, headless};
use hydrus_store::{
    physical_delete::{self, Preferences},
    settings,
};
use slint::{ComponentHandle as _, Model as _};
const LABEL: &str =
    "When maintenance physically deletes files, wait this long between each delete: ";
fn open(ui: &MainWindow, bound: &hydrus_gui::Bound) -> (OptionsWindow, i32) {
    ui.invoke_menu_title_pressed(0, 20., 22.);
    let lines = ui.get_menu_panes().row_data(0).unwrap().lines;
    let i = lines
        .iter()
        .position(|row| row.label == "options…")
        .unwrap();
    ui.invoke_menu_line_clicked(0, i as i32, 0., 0., 0.);
    let w = bound.options.borrow().as_ref().unwrap().clone_strong();
    let page = w
        .get_pages()
        .iter()
        .position(|page| page.text == "files and trash")
        .unwrap();
    w.invoke_page_chosen(page as i32);
    let row = w
        .get_rows()
        .iter()
        .position(|row| row.label == LABEL)
        .unwrap() as i32;
    (w, row)
}
fn values(w: &OptionsWindow, row: i32) -> Vec<i32> {
    w.get_rows()
        .row_data(row as usize)
        .unwrap()
        .fields
        .iter()
        .map(|field| field.value)
        .collect()
}
#[test]
fn actual_fields_cancel_raw_apply_reopen_hidden_retired_and_rebind_ownership() {
    let (_dirs, store) = super::subscriptions::store();
    store
        .write(|ctx| settings::set(ctx.conn(), &Preferences { wait_ms: 19 }))
        .unwrap();
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    ui.show().unwrap();
    let (w, row) = open(&ui, &bound);
    let shown = w.get_rows().row_data(row as usize).unwrap();
    assert_eq!(shown.kind, 8);
    assert!(shown.enabled);
    assert_eq!(values(&w, row), [0, 20]);
    assert_eq!(
        shown.fields.iter().map(|f| f.maximum).collect::<Vec<_>>(),
        [59, 999]
    );
    w.invoke_field_edited(row, 0, 1);
    w.invoke_field_edited(row, 1, 234);
    w.invoke_cancel();
    assert_eq!(store.read(physical_delete::load).unwrap().wait_ms, 19);
    w.show().unwrap();
    w.invoke_field_edited(row, 1, 900);
    w.invoke_apply();
    w.hide().unwrap();
    assert_eq!(store.read(physical_delete::load).unwrap().wait_ms, 19);
    let (w, row) = open(&ui, &bound);
    w.hide().unwrap();
    w.invoke_field_edited(row, 1, 900);
    w.show().unwrap();
    w.invoke_apply();
    assert_eq!(
        store.read(physical_delete::load).unwrap().wait_ms,
        20,
        "visible unchanged Apply normalizes raw19; hidden fields cannot edit"
    );
    let (w, row) = open(&ui, &bound);
    w.invoke_field_edited(row, 0, 1);
    w.invoke_field_edited(row, 1, 29);
    w.invoke_apply();
    assert_eq!(store.read(physical_delete::load).unwrap().wait_ms, 1029);
    let (w, row) = open(&ui, &bound);
    assert_eq!(values(&w, row), [1, 28]);
    let image = headless::render(&windows.get(windows.count() - 1).unwrap(), 900, 640);
    headless::save_png(
        &hydrus_testkit::artifacts_dir().join("physical_delete_delay_options.png"),
        &image,
        900,
        640,
    )
    .unwrap();
    w.invoke_field_edited(row, 1, 29);
    w.invoke_apply();
    assert_eq!(store.read(physical_delete::load).unwrap().wait_ms, 1029);
    let (old, row) = open(&ui, &bound);
    old.invoke_field_edited(row, 1, 700);
    let replacement = bind(&ui, Pages::open(store.clone()).unwrap());
    old.show().unwrap();
    old.invoke_apply();
    old.hide().unwrap();
    assert_eq!(store.read(physical_delete::load).unwrap().wait_ms, 1029);
    let (w, row) = open(&ui, &replacement);
    w.invoke_field_edited(row, 0, 0);
    w.invoke_field_edited(row, 1, 45);
    w.invoke_apply();
    assert_eq!(store.read(physical_delete::load).unwrap().wait_ms, 45);
}
