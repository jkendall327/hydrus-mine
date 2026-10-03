//! Siblings/parents menu windows against a real imported store and renderer.
use std::cell::RefCell;
use std::rc::Rc;

use hydrus_core::Tag;
use hydrus_gui::{MainWindow, Pages, SearchPage, bind, headless};
use hydrus_store::Store;
use slint::{ComponentHandle as _, Model as _};

#[test]
fn dialogs_stage_cancel_apply_questions_and_update_display() {
    let windows = headless::init();
    let dir = tempfile::tempdir().unwrap();
    let legacy = hydrus_testkit::legacy_fixture("basic");
    hydrus_store::import::import_legacy(
        legacy.path(),
        &dir.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store = Store::open(dir.path()).unwrap();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::single(SearchPage::new(store.clone())));
    ui.invoke_search_edited("system:everything".into());
    ui.invoke_search_accepted();
    let file = bound.current.borrow().borrow().results()[0];
    let service = store.snapshot().services.by_name("my tags").unwrap().id;
    store
        .write_content(move |writer| {
            let tag =
                hydrus_store::master::intern_tag(writer.conn(), &Tag::new("window old").unwrap())?;
            writer.update_mappings(
                service,
                &hydrus_store::content::MappingAction::Add,
                tag,
                &[file],
            )?;
            Ok(())
        })
        .unwrap();
    ui.invoke_thumbnail_activated(0);
    let viewer = bound.viewer.borrow().as_ref().unwrap().clone_strong();
    // Open through the tags menu, using its public click callbacks.
    fn open(
        ui: &MainWindow,
        bound: &hydrus_gui::Bound,
        name: &str,
    ) -> hydrus_gui::TagRelationshipsWindow {
        let top = (0..ui.get_menu_titles().row_count())
            .find(|&i| ui.get_menu_titles().row_data(i).unwrap().label == "tags")
            .unwrap();
        ui.invoke_menu_title_pressed(i32::try_from(top).unwrap(), 0.0, 22.0);
        let panes = ui.get_menu_panes();
        let pane = panes.row_data(0).unwrap();
        let at = (0..pane.lines.row_count())
            .find(|&i| pane.lines.row_data(i).unwrap().label.starts_with(name))
            .unwrap();
        ui.invoke_menu_line_clicked(0, i32::try_from(at).unwrap(), 0.0, 0.0, 0.0);
        bound
            .tag_relationships
            .borrow()
            .as_ref()
            .unwrap()
            .clone_strong()
    }
    let w = open(&ui, &bound, "siblings");
    let services = w.get_service_names();
    let mine = (0..services.row_count())
        .find(|&i| services.row_data(i).unwrap() == "my tags")
        .unwrap();
    w.invoke_service_chosen(i32::try_from(mine).unwrap());
    w.set_left_input("unfinished text".into());
    let other = if mine == 0 { 1 } else { 0 };
    w.invoke_service_chosen(other);
    assert_eq!(w.get_left_input(), "");
    w.invoke_service_chosen(i32::try_from(mine).unwrap());
    assert_eq!(w.get_left_input(), "unfinished text");
    w.set_left_input("".into());
    w.invoke_enter_tags(false, "window old".into());
    w.invoke_enter_tags(true, "window ideal".into());
    assert!(w.get_can_add());
    w.invoke_apply();
    assert!(w.get_question().contains("uncommitted pair"));
    w.invoke_answered(false);
    assert!(bound.tag_relationships.borrow().is_some());
    w.invoke_add();
    assert!(!w.get_can_add());
    assert!(w.get_rows().row_count() > 0);
    w.invoke_row_clicked(0, false, false);
    let copied = Rc::new(RefCell::new(Vec::new()));
    hydrus_gui::set_clipper({
        let copied = copied.clone();
        move |clip| {
            if let hydrus_gui::Clip::Text(text) = clip {
                copied.borrow_mut().push(text.clone());
            }
        }
    });
    w.invoke_export_pairs(false);
    assert_eq!(*copied.borrow(), ["window old\nwindow ideal"]);
    hydrus_gui::set_paster(|| "clipboard alias\nclipboard ideal\nodd".into());
    w.invoke_import_pairs(false);
    assert_eq!(w.get_error(), "Uneven number of tags in clipboard!");
    hydrus_gui::set_paster(|| "window old\nwindow ideal".into());
    w.invoke_import_pairs(false);
    assert_eq!(w.get_error(), "");
    let text_path = dir.path().join("siblings.txt");
    std::fs::write(&text_path, "file alias\nfile ideal").unwrap();
    hydrus_gui::set_picker(move |_, title| {
        assert_eq!(title, "Select the file to import.");
        vec![text_path.clone()]
    });
    w.invoke_import_pairs(true);
    assert_eq!(w.get_rows().row_count(), 3);
    let pixels = headless::render(&windows.get(windows.count() - 1).unwrap(), 850, 660);
    headless::save_png(
        &std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("tag-siblings.png"),
        &pixels,
        850,
        660,
    )
    .unwrap();
    w.invoke_cancel();
    assert!(bound.tag_relationships.borrow().is_none());
    let service = store.snapshot().services.by_name("my tags").unwrap().id;
    let ids = store
        .write_content(|writer| {
            Ok((
                hydrus_store::master::intern_tag(writer.conn(), &Tag::new("window old").unwrap())?,
                hydrus_store::master::intern_tag(
                    writer.conn(),
                    &Tag::new("window ideal").unwrap(),
                )?,
            ))
        })
        .unwrap();
    assert_eq!(store.snapshot().display.get(service).ideal(ids.0), ids.0);
    let w = open(&ui, &bound, "siblings");
    w.invoke_service_chosen(i32::try_from(mine).unwrap());
    w.invoke_enter_tags(false, "window old".into());
    w.invoke_enter_tags(true, "window ideal".into());
    w.invoke_add();
    w.invoke_apply();
    assert!(bound.tag_relationships.borrow().is_none());
    assert_eq!(store.snapshot().display.get(service).ideal(ids.0), ids.1);
    assert!(
        ui.get_tags()
            .iter()
            .any(|r| r.text.contains("window ideal"))
    );
    assert!(viewer.get_tags().iter().any(|r| r.text == "window ideal"));
    let w = open(&ui, &bound, "parents");
    w.invoke_service_chosen(i32::try_from(mine).unwrap());
    w.invoke_enter_tags(false, "window old".into());
    w.invoke_enter_tags(true, "parent one\nparent two".into());
    w.invoke_add();
    assert!(w.get_rows().row_count() >= 2);
    let pixels = headless::render(&windows.get(windows.count() - 1).unwrap(), 850, 660);
    headless::save_png(
        &std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("tag-parents.png"),
        &pixels,
        850,
        660,
    )
    .unwrap();
    w.invoke_row_clicked(0, false, false);
    w.invoke_delete();
    assert!(w.get_question().contains("pending"));
    w.invoke_answered(false);
    w.invoke_apply();
    assert_eq!(
        store.snapshot().display.get(service).ancestors(ids.0).len(),
        2
    );
    let w = open(&ui, &bound, "siblings");
    w.invoke_service_chosen(i32::try_from(mine).unwrap());
    w.set_show_all(true);
    w.invoke_filters_changed();
    let rows = w.get_rows();
    let at = (0..rows.row_count())
        .find(|&i| rows.row_data(i).unwrap().cells.row_data(1).unwrap() == "window old")
        .unwrap();
    w.invoke_row_clicked(i32::try_from(at).unwrap(), false, false);
    w.invoke_delete();
    w.invoke_apply();
    assert_eq!(store.snapshot().display.get(service).ideal(ids.0), ids.0);
    assert!(viewer.get_tags().iter().any(|r| r.text == "window old"));
    assert!(!viewer.get_tags().iter().any(|r| r.text == "window ideal"));
    viewer.invoke_close_requested();
    store
        .write_and_refresh(|ctx| {
            hydrus_store::services::insert(
                ctx.conn(),
                &hydrus_core::ServiceKey::new(vec![61; 16]),
                "window repository",
                &hydrus_store::services::ServiceKind::TagRepository(
                    hydrus_store::services::RepositoryConfig::default(),
                ),
            )?;
            Ok(())
        })
        .unwrap();
    let w = open(&ui, &bound, "siblings");
    let at = w
        .get_service_names()
        .iter()
        .position(|name| name == "window repository")
        .unwrap();
    w.invoke_service_chosen(i32::try_from(at).unwrap());
    w.invoke_enter_tags(false, "repository alias".into());
    w.invoke_enter_tags(true, "repository ideal".into());
    w.invoke_add();
    assert!(w.get_ask_reason());
    w.invoke_answered(true);
    assert!(!w.get_question().is_empty());
    assert_eq!(w.get_rows().row_count(), 0);
    w.set_reason("same tag".into());
    w.invoke_answered(true);
    assert!(w.get_question().is_empty());
    assert_eq!(w.get_rows().row_count(), 1);
    w.invoke_apply();
    assert!(bound.tag_relationships.borrow().is_none());
}
