//! Actual staged add/edit/delete/cancel flows over a native store.
use hydrus_gui::{MainWindow, Pages, bind, headless};
use slint::{ComponentHandle as _, Model as _};
fn open(ui: &MainWindow) {
    let titles = ui.get_menu_titles();
    let index = titles.iter().position(|t| t.label == "services").unwrap();
    ui.invoke_menu_title_pressed(i32::try_from(index).unwrap(), 10.0, 22.0);
    let lines = ui.get_menu_panes().row_data(0).unwrap().lines;
    let index = lines
        .iter()
        .position(|l| l.label.starts_with("edit"))
        .unwrap();
    assert!(lines.row_data(index).unwrap().usable);
    ui.invoke_menu_line_clicked(0, i32::try_from(index).unwrap(), 0.0, 0.0, 0.0);
}
#[test]
fn staged_rating_config_applies_and_cancel_writes_nothing() {
    let (_dirs, store) = crate::subscriptions::store();
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    let before = store.snapshot().services.all().count();
    open(&ui);
    let manage = bound
        .services_editor
        .manage
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    assert_eq!(manage.get_window_title(), "edit services");
    let unavailable = manage
        .get_rows()
        .iter()
        .position(|r| r.cells.row_data(0).unwrap() == "client api")
        .unwrap();
    manage.invoke_row_clicked(i32::try_from(unavailable).unwrap(), false, false);
    assert!(!manage.get_can_edit());
    // Row activation invokes the same callback as the edit button: it must
    // respect unavailable service kinds even when a caller bypasses the button.
    manage.invoke_edit_clicked();
    assert!(bound.services_editor.edit.borrow().is_none());
    assert!(manage.get_error().contains("not available here yet"));
    manage.invoke_add_clicked(3);
    let edit = bound
        .services_editor
        .edit
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    edit.set_service_name("".into());
    edit.invoke_apply_clicked();
    assert_eq!(edit.get_error(), "Please enter a name!");
    assert!(edit.get_numerical());
    assert_eq!(edit.get_stars(), 5);
    edit.set_service_name("new stars".into());
    edit.set_stars(1);
    edit.set_allow_zero(false);
    edit.set_show_thumbnail(true);
    edit.set_show_null(true);
    edit.set_icon_padding(-12);
    edit.set_fraction(2);
    edit.invoke_colour_edited(0, false, "#123456".into());
    edit.invoke_apply_clicked();
    assert!(bound.services_editor.edit.borrow().is_none());
    assert_eq!(store.snapshot().services.all().count(), before);
    assert!(
        manage
            .get_rows()
            .iter()
            .any(|r| r.cells.row_data(0).unwrap() == "new stars")
    );
    manage.invoke_apply_clicked();
    assert!(bound.services_editor.manage.borrow().is_none());
    let added = store
        .snapshot()
        .services
        .by_name("new stars")
        .unwrap()
        .clone();
    match &added.kind {
        hydrus_store::services::ServiceKind::RatingNumerical(c) => {
            assert_eq!(c.num_stars, 1);
            assert!(c.allow_zero);
            assert_eq!(c.custom_pad, -12);
            assert_eq!(c.show_fraction_beside_stars, 2);
            assert!(c.display.show_in_thumbnail);
            assert!(c.display.show_in_thumbnail_even_when_null);
            assert_eq!(
                c.display.colours.like.brush,
                hydrus_store::services::Rgb([18, 52, 86])
            );
        }
        _ => panic!("wrong rating kind"),
    }
    open(&ui);
    let manage = bound
        .services_editor
        .manage
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    let index = manage
        .get_rows()
        .iter()
        .position(|r| r.cells.row_data(0).unwrap() == "new stars")
        .unwrap();
    manage.invoke_row_clicked(i32::try_from(index).unwrap(), false, false);
    manage.invoke_edit_clicked();
    let edit = bound
        .services_editor
        .edit
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    assert_eq!(edit.get_service_name(), "new stars");
    assert!(edit.get_allow_zero());
    assert_eq!(edit.get_icon_padding(), -12);
    edit.set_service_name("discard me".into());
    edit.invoke_cancel_clicked();
    manage.invoke_delete_clicked();
    assert_eq!(manage.get_question(), "Delete the selected services?");
    manage.invoke_answered(true);
    assert!(store.snapshot().services.by_name("new stars").is_some());
    manage.invoke_cancel_clicked();
    assert_eq!(
        store
            .snapshot()
            .services
            .by_name("new stars")
            .unwrap()
            .as_ref(),
        added.as_ref()
    );
    open(&ui);
    let manage = bound
        .services_editor
        .manage
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    manage.invoke_add_clicked(0);
    let edit = bound
        .services_editor
        .edit
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    edit.invoke_cancel_clicked();
    assert_eq!(manage.get_rows().row_count(), before + 1);
    manage.invoke_cancel_clicked();
    open(&ui);
    let manage = bound
        .services_editor
        .manage
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    let index = manage
        .get_rows()
        .iter()
        .position(|r| r.cells.row_data(0).unwrap() == "new stars")
        .unwrap();
    manage.invoke_row_clicked(i32::try_from(index).unwrap(), false, false);
    manage.invoke_delete_clicked();
    manage.invoke_answered(true);
    manage.invoke_apply_clicked();
    assert!(manage.get_question().contains("Are you absolutely sure"));
    manage.invoke_answered(false);
    assert!(store.snapshot().services.by_name("new stars").is_some());
    manage.invoke_apply_clicked();
    manage.invoke_answered(true);
    assert!(store.snapshot().services.by_name("new stars").is_none());
    open(&ui);
    let manage = bound
        .services_editor
        .manage
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    manage.invoke_add_clicked(3);
    let edit = bound
        .services_editor
        .edit
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    let last = (0..100)
        .take_while(|&n| windows.get(n).is_some())
        .last()
        .unwrap();
    let pixels = headless::render(&windows.get(last).unwrap(), 640, 640);
    headless::save_png(
        &std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("services_editor.png"),
        &pixels,
        640,
        640,
    )
    .unwrap();
    edit.invoke_cancel_clicked();
    manage.invoke_cancel_clicked();
}
