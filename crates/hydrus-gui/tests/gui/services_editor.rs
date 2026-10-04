//! Actual staged add/edit/delete/cancel flows over a native store.
use hydrus_gui::{MainWindow, Pages, bind, headless};
use slint::{ComponentHandle as _, Model as _};

#[test]
fn deleting_relation_source_refreshes_open_viewer_and_locked_selection() {
    use hydrus_core::{ServiceKey, Tag};
    use hydrus_store::{master, services};

    let (_dirs, store) = crate::subscriptions::store();
    let target = store.snapshot().services.by_name("my tags").unwrap().id;
    let (raw, ideal, parent) = store
        .write_and_refresh(move |ctx| {
            let conn = ctx.conn();
            let source = services::insert(
                conn,
                &ServiceKey::new(vec![83; 32]),
                "display source",
                &services::ServiceKind::LocalTags,
            )?;
            let raw = master::intern_tag(conn, &Tag::new("lifecycle raw").unwrap())?;
            let ideal = master::intern_tag(conn, &Tag::new("lifecycle ideal").unwrap())?;
            let parent = master::intern_tag(conn, &Tag::new("lifecycle parent").unwrap())?;
            conn.execute(
                "INSERT INTO tag_siblings VALUES(?,0,?,?,NULL)",
                rusqlite::params![source, raw, ideal],
            )?;
            conn.execute(
                "INSERT INTO tag_parents VALUES(?,0,?,?,NULL)",
                rusqlite::params![source, ideal, parent],
            )?;
            for kind in 0..2 {
                conn.execute(
                    "DELETE FROM tag_display_application WHERE display_service_id=? AND kind=?",
                    rusqlite::params![target, kind],
                )?;
                conn.execute(
                    "INSERT INTO tag_display_application VALUES(?,?,0,?)",
                    rusqlite::params![target, kind, source],
                )?;
            }
            hydrus_store::counts::rebuild_all(conn)?;
            Ok((raw, ideal, parent))
        })
        .unwrap();
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    ui.invoke_search_edited("system:everything".into());
    ui.invoke_search_accepted();
    let page = bound.current.borrow().clone();
    let files = page.borrow().results().to_vec();
    let file = files[0];
    store
        .write_content(move |writer| {
            writer.update_mappings(
                target,
                &hydrus_store::content::MappingAction::Add,
                raw,
                &[file],
            )
        })
        .unwrap();
    page.borrow_mut().select_files(&[file]);
    page.borrow_mut().lock_search();
    ui.invoke_refresh_page();
    assert!(ui.get_search_locked());
    assert!(
        ui.get_tags()
            .iter()
            .any(|r| r.text.contains("lifecycle ideal"))
    );
    assert!(
        ui.get_tags()
            .iter()
            .any(|r| r.text.contains("lifecycle parent"))
    );
    ui.invoke_thumbnail_activated(0);
    let viewer = bound.viewer.borrow().as_ref().unwrap().clone_strong();
    assert!(
        viewer
            .get_tags()
            .iter()
            .any(|r| r.text == "lifecycle ideal")
    );
    assert!(
        viewer
            .get_tags()
            .iter()
            .any(|r| r.text == "lifecycle parent")
    );

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
        .position(|r| r.cells.row_data(0).unwrap() == "display source")
        .unwrap();
    manage.invoke_row_clicked(i32::try_from(index).unwrap(), false, false);
    manage.invoke_delete_clicked();
    manage.invoke_answered(true);
    // Staging the removal must leave both views on the committed graph.
    assert!(
        viewer
            .get_tags()
            .iter()
            .any(|r| r.text == "lifecycle ideal")
    );
    manage.invoke_apply_clicked();
    manage.invoke_answered(true);
    assert!(bound.services_editor.manage.borrow().is_none());
    let snapshot = store.snapshot();
    assert!(snapshot.services.by_name("display source").is_none());
    assert_eq!(snapshot.display.get(target).ideal(raw), raw);
    assert!(snapshot.display.get(target).ancestors(ideal).is_empty());
    assert!(snapshot.display.get(target).ancestors(parent).is_empty());
    assert_eq!(page.borrow().results(), files);
    assert_eq!(page.borrow().selected_files(), [file]);
    assert!(ui.get_search_locked());
    assert!(
        ui.get_tags()
            .iter()
            .any(|r| r.text.contains("lifecycle raw"))
    );
    assert!(
        !ui.get_tags()
            .iter()
            .any(|r| { r.text.contains("lifecycle ideal") || r.text.contains("lifecycle parent") })
    );
    assert!(viewer.get_tags().iter().any(|r| r.text == "lifecycle raw"));
    assert!(
        !viewer
            .get_tags()
            .iter()
            .any(|r| { r.text == "lifecycle ideal" || r.text == "lifecycle parent" })
    );
    assert!(bound.viewer.borrow().is_some());
    viewer.invoke_close_requested();
}

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
    store
        .write_and_refresh(|ctx| {
            hydrus_store::services::insert(
                ctx.conn(),
                &hydrus_core::ServiceKey::new(vec![91; 32]),
                "protected IPFS",
                &hydrus_store::services::ServiceKind::Ipfs(
                    hydrus_store::services::RepositoryConfig::default(),
                ),
            )
            .map(|_| ())
        })
        .unwrap();
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
    let api = manage
        .get_rows()
        .iter()
        .position(|r| r.cells.row_data(0).unwrap() == "client api")
        .unwrap();
    manage.invoke_row_clicked(i32::try_from(api).unwrap(), false, false);
    assert!(manage.get_can_edit(), "supported API settings are editable");
    assert!(
        !manage.get_can_delete(),
        "the built-in API service remains protected"
    );
    let rows = manage.get_rows().row_count();
    manage.invoke_delete_clicked();
    assert!(manage.get_question().is_empty());
    assert_eq!(manage.get_rows().row_count(), rows);
    manage.invoke_edit_clicked();
    let api_edit = bound
        .services_editor
        .edit
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    assert!(api_edit.get_client_api());
    assert!(!api_edit.get_rating());
    api_edit.invoke_cancel_clicked();
    let unavailable = manage
        .get_rows()
        .iter()
        .position(|r| r.cells.row_data(0).unwrap() == "protected IPFS")
        .unwrap();
    manage.invoke_row_clicked(i32::try_from(unavailable).unwrap(), false, false);
    assert!(!manage.get_can_edit());
    assert!(!manage.get_can_delete());
    // Direct activation still respects unsupported remote-service protection.
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
