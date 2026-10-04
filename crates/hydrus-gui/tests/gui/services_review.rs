//! The services menu opens review, whose real store counts refresh.
use hydrus_gui::{MainWindow, Pages, bind, headless};
use slint::{ComponentHandle as _, Model as _};

#[test]
fn services_menu_opens_review_and_refreshes() {
    let (_dirs, store) = crate::subscriptions::store();
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    let titles = ui.get_menu_titles();
    let index = titles.iter().position(|t| t.label == "services").unwrap();
    ui.invoke_menu_title_pressed(i32::try_from(index).unwrap(), 10.0, 22.0);
    let lines = ui.get_menu_panes().row_data(0).unwrap().lines;
    let index = lines.iter().position(|l| l.label == "review").unwrap();
    assert!(lines.row_data(index).unwrap().usable);
    ui.invoke_menu_line_clicked(0, i32::try_from(index).unwrap(), 0.0, 0.0, 0.0);
    let window = bound
        .services_review
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    assert_eq!(window.get_window_title(), "review services");
    let index = window
        .get_services()
        .iter()
        .position(|s| s.ends_with(": my tags"))
        .unwrap();
    window.invoke_selected_service(i32::try_from(index).unwrap());
    assert!(window.get_statistics().contains("total mappings involving"));
    assert!(window.get_unavailable().is_empty());
    let trash = store
        .snapshot()
        .services
        .all()
        .find(|service| matches!(service.kind, hydrus_store::services::ServiceKind::Trash))
        .unwrap()
        .name
        .clone();
    let trash_index = window
        .get_services()
        .iter()
        .position(|service| service.ends_with(&format!(": {trash}")))
        .unwrap();
    window.invoke_selected_service(i32::try_from(trash_index).unwrap());
    assert_eq!(
        window.get_unavailable(),
        "Bulk clear trash and undelete all are not available here yet."
    );
    window.invoke_selected_service(i32::try_from(index).unwrap());
    assert!(window.get_unavailable().is_empty());
    window.invoke_show_id();
    assert!(window.get_database_id().starts_with("service id: "));
    let copied = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    hydrus_gui::set_clipper({
        let copied = copied.clone();
        move |clip| copied.borrow_mut().push(clip.clone())
    });
    window.invoke_copy_key();
    let key = store
        .snapshot()
        .services
        .by_name("my tags")
        .unwrap()
        .key
        .to_hex();
    assert_eq!(copied.borrow().as_slice(), &[hydrus_gui::Clip::Text(key)]);

    let service = store.snapshot().services.by_name("my tags").unwrap().id;
    store
        .write_and_refresh(move |ctx| {
            ctx.conn().execute(
                "UPDATE services SET name='renamed tags' WHERE service_id=?",
                [service],
            )?;
            Ok(())
        })
        .unwrap();
    window.invoke_refresh_clicked();
    assert!(window.get_name_and_type().starts_with("renamed tags - "));
    let last = (0..100)
        .take_while(|&n| windows.get(n).is_some())
        .last()
        .unwrap();
    let pixels = headless::render(&windows.get(last).unwrap(), 700, 400);
    headless::save_png(
        &std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("services_review.png"),
        &pixels,
        700,
        400,
    )
    .unwrap();
    window.invoke_close_clicked();
}
