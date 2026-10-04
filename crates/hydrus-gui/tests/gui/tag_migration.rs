//! Actual entry points, confirmations, asynchronous writes and rendered controls.
use hydrus_gui::{MainWindow, Pages, bind, headless, tag_migration_window};
use slint::{ComponentHandle as _, Model as _};
#[test]
fn service_review_opens_migration_with_reference_questions_and_renders() {
    let (_dirs, store) = crate::subscriptions::store();
    let windows = headless::init();
    let review = hydrus_gui::services_review_window::open(store.clone()).unwrap();
    let index = review
        .get_services()
        .iter()
        .position(|s| s.ends_with(": my tags"))
        .unwrap();
    review.invoke_selected_service(i32::try_from(index).unwrap());
    assert!(review.get_tag_service());
    review.invoke_migrate_tags();
    let window = tag_migration_window::last_opened().unwrap();
    assert_eq!(window.get_actions().row_data(0).unwrap(), "delete");
    window.invoke_go();
    assert!(
        window
            .get_question()
            .contains("Migrations can make huge changes.")
    );
    window.invoke_answer(false);
    assert!(window.get_question().is_empty());
    window.invoke_go();
    window.invoke_answer(true);
    assert_eq!(
        window.get_question(),
        hydrus_gui_model::tag_migration::LAST_CHANCE
    );
    let last = (0..100)
        .take_while(|&i| windows.get(i).is_some())
        .last()
        .unwrap();
    let pixels = headless::render(&windows.get(last).unwrap(), 760, 590);
    headless::save_png(
        &std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("tag_migration.png"),
        &pixels,
        760,
        590,
    )
    .unwrap();
    window.invoke_answer(false);
    window.invoke_close_clicked();
    review.invoke_close_clicked();
}
#[test]
fn manage_tags_launches_selected_scope_and_refreshes_after_background_delete() {
    let (_dirs, store) = crate::subscriptions::store();
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(
        &ui,
        Pages::single(hydrus_gui::SearchPage::new(store.clone())),
    );
    // Use the public native editor on one imported fixture file.
    ui.invoke_search_edited("system:everything".into());
    ui.invoke_search_accepted();
    let hash = bound.current.borrow().borrow().results()[0];
    ui.invoke_select_all();
    ui.invoke_manage_tags_selected();
    let manage = bound.manage_tags.borrow().as_ref().unwrap().clone_strong();
    let mine = manage
        .get_service_names()
        .iter()
        .position(|name| name == "my tags")
        .unwrap();
    manage.invoke_service_chosen(i32::try_from(mine).unwrap());
    manage.invoke_migrate_tags();
    let window = tag_migration_window::last_opened().unwrap();
    assert!(window.get_selected_files());
    assert!(window.get_have_files());
    window.invoke_go();
    window.invoke_answer(true);
    window.invoke_answer(true);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
    while window.get_running() && std::time::Instant::now() < deadline {
        std::thread::sleep(std::time::Duration::from_millis(100));
        slint::platform::update_timers_and_animations();
    }
    assert!(!window.get_running());
    assert!(window.get_error().is_empty(), "{}", window.get_error());
    assert!(window.get_progress().contains("done!"));
    let id = store.snapshot().services.by_name("my tags").unwrap().id;
    let table = hydrus_store::schema::MappingTables::new(id).current;
    let count = store
        .read(|conn| {
            Ok(conn.query_row(
                &format!("SELECT count(*) FROM {table} WHERE hash_id=?"),
                [hash],
                |r| r.get::<_, i64>(0),
            )?)
        })
        .unwrap();
    assert_eq!(count, 0);
    assert_eq!(manage.get_tags().row_count(), 0);
    window.invoke_close_clicked();
    manage.invoke_cancel();
}

#[test]
fn closing_a_running_job_refreshes_committed_state_before_hiding() {
    let (_dirs, store) = crate::subscriptions::store();
    let _windows = headless::init();
    let key = store
        .snapshot()
        .services
        .by_name("my tags")
        .unwrap()
        .key
        .clone();
    let changed = std::rc::Rc::new(std::cell::Cell::new(0));
    let notify = std::rc::Rc::new({
        let changed = changed.clone();
        move || {
            changed.set(changed.get() + 1);
        }
    });
    let slot = tag_migration_window::Slot::default();
    let window = tag_migration_window::open(&store, &key, vec![], &slot, notify).unwrap();
    window.invoke_go();
    window.invoke_answer(true);
    window.invoke_answer(true);
    assert!(window.get_running());
    window.invoke_close_clicked();
    assert!(window.window().is_visible());
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
    while window.window().is_visible() && std::time::Instant::now() < deadline {
        std::thread::sleep(std::time::Duration::from_millis(100));
        slint::platform::update_timers_and_animations();
    }
    assert!(!window.window().is_visible());
    assert!(!window.get_running());
    assert!(changed.get() > 0);
    assert!(slot.borrow().is_none());
}

#[test]
fn an_open_filter_child_cannot_broaden_a_confirmed_delete() {
    let dir = tempfile::tempdir().unwrap();
    let store = hydrus_store::Store::open(dir.path()).unwrap();
    let _windows = headless::init();
    let key = store
        .snapshot()
        .services
        .by_name("my tags")
        .unwrap()
        .key
        .clone();
    let service = store.snapshot().services.by_key(&key).unwrap().id;
    let hash = store
        .write_content(move |writer| {
            let hash =
                hydrus_store::master::intern_hash(writer.conn(), &hydrus_core::Sha256([91; 32]))?;
            for text in ["series:delete", "creator:retain"] {
                let tag = hydrus_store::master::intern_tag(
                    writer.conn(),
                    &hydrus_core::Tag::new(text).unwrap(),
                )?;
                writer.update_mappings(
                    service,
                    &hydrus_store::content::MappingAction::Add,
                    tag,
                    &[hash],
                )?;
            }
            Ok(hash)
        })
        .unwrap();
    let slot = tag_migration_window::Slot::default();
    let window =
        tag_migration_window::open(&store, &key, vec![hash], &slot, std::rc::Rc::new(|| {}))
            .unwrap();
    window.invoke_edit_filter(false);
    let filter = hydrus_gui::tag_filter_window::last_opened().unwrap();
    filter.invoke_typed(1, "creator:retain".into());
    filter.invoke_apply();
    assert!(window.get_left_label().contains("creator"));
    window.invoke_edit_filter(false);
    let filter = hydrus_gui::tag_filter_window::last_opened().unwrap();
    window.invoke_go();
    let confirmed = window.get_question();
    assert!(confirmed.contains("creator"));
    filter.invoke_row_activated(1, 0);
    filter.invoke_apply();
    assert!(window.get_left_label().contains("all tags"));
    assert_eq!(window.get_question(), confirmed);
    window.invoke_answer(true);
    window.invoke_answer(true);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
    while window.get_running() && std::time::Instant::now() < deadline {
        std::thread::sleep(std::time::Duration::from_millis(100));
        slint::platform::update_timers_and_animations();
    }
    assert!(!window.get_running());
    assert!(window.get_error().is_empty(), "{}", window.get_error());
    let table = hydrus_store::schema::MappingTables::new(service).current;
    let tags = store
        .read(|conn| {
            let ids = conn
                .prepare(&format!("SELECT tag_id FROM {table} WHERE hash_id=?"))?
                .query_map([hash], |r| r.get::<_, hydrus_core::TagId>(0))?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            ids.into_iter()
                .map(|id| {
                    hydrus_store::master::tag(conn, id).map(|tag| tag.unwrap().as_str().to_owned())
                })
                .collect::<hydrus_store::Result<Vec<_>>>()
        })
        .unwrap();
    assert_eq!(tags, vec!["creator:retain"]);
    window.invoke_close_clicked();
}
