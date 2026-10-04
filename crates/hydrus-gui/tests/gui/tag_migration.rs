//! Actual entry points, confirmations, asynchronous writes and rendered controls.
use hydrus_gui::{MainWindow, Pages, bind, headless, tag_migration_window};
use slint::{ComponentHandle as _, Model as _};

#[test]
fn global_tags_menu_opens_unrestricted_migration_and_can_reopen_after_cancel() {
    let (_dirs, store) = crate::subscriptions::store();
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(
        &ui,
        Pages::single(hydrus_gui::SearchPage::new(store.clone())),
    );
    let recorded = hydrus_testkit::fixture_json("main_menu.json");
    assert!(recorded.to_string().contains("migrate…"));
    let migration = hydrus_testkit::fixture_json("tag_migration.json");
    for _ in 0..2 {
        let title = ui
            .get_menu_titles()
            .iter()
            .position(|t| t.label == "tags")
            .unwrap();
        ui.invoke_menu_title_pressed(i32::try_from(title).unwrap(), 0.0, 22.0);
        let pane = ui.get_menu_panes().row_data(0).unwrap();
        let row = pane
            .lines
            .iter()
            .position(|r| r.label == "migrate…")
            .unwrap();
        assert!(pane.lines.row_data(row).unwrap().usable);
        ui.invoke_menu_line_clicked(0, i32::try_from(row).unwrap(), 0.0, 0.0, 0.0);
        assert_eq!(ui.get_menu_open(), -1);
        let window = bound
            .tag_migration
            .borrow()
            .as_ref()
            .unwrap()
            .clone_strong();
        assert!(window.window().is_visible());
        assert!(!window.get_have_files());
        assert!(!window.get_selected_files());
        assert_eq!(
            window
                .get_services()
                .row_data(usize::try_from(window.get_source()).unwrap())
                .unwrap(),
            "my tags"
        );
        window.invoke_go();
        let warning = migration["asked"][0]["message"]
            .as_str()
            .unwrap()
            .split("\n\n")
            .next()
            .unwrap();
        assert!(window.get_question().starts_with(warning));
        window.invoke_answer(false);
        assert!(window.get_question().is_empty());
        assert!(!window.get_running());
        window.invoke_close_clicked();
        assert!(bound.tag_migration.borrow().is_none());
    }
}

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
    window.invoke_pause_job();
    assert!(window.get_paused());
    window.invoke_pause_job();
    assert!(!window.get_paused());
    window.invoke_pause_job();
    assert!(window.get_paused());
    window.invoke_close_clicked();
    assert!(window.window().is_visible());
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
    while window.window().is_visible() && std::time::Instant::now() < deadline {
        std::thread::sleep(std::time::Duration::from_millis(100));
        slint::platform::update_timers_and_animations();
    }
    assert!(!window.window().is_visible());
    assert!(!window.get_running());
    assert!(!window.get_paused());
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

#[test]
fn archive_controls_inspect_reject_wrong_pair_types_and_freeze_confirmed_paths() {
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
    let slot = tag_migration_window::Slot::default();
    let window =
        tag_migration_window::open(&store, &key, vec![], &slot, std::rc::Rc::new(|| {})).unwrap();
    let recording = hydrus_testkit::fixture_json("tag_archives.json");
    for case in recording["qt"].as_array().unwrap() {
        // Each reference inspector case starts from a fresh content draft.
        window.set_content(if case["kind"] == "siblings" { 0 } else { 1 });
        window.invoke_choices_changed();
        window.set_content(match case["kind"].as_str().unwrap() {
            "sha256" | "md5" => 0,
            "siblings" => 1,
            _ => 2,
        });
        window.invoke_choices_changed();
        let archive = i32::try_from(window.get_services().row_count() - 1).unwrap();
        window.set_source(archive);
        window.set_destination(archive);
        window.invoke_choices_changed();
        assert!(window.get_source_archive());
        assert!(window.get_destination_archive());
        window.invoke_go();
        assert!(window.get_error().contains("Please set a path"));
        assert!(window.get_question().is_empty());
        let path = hydrus_testkit::fixture_path(case["source"].as_str().unwrap());
        window.invoke_archive_path_chosen(true, path.to_string_lossy().as_ref().into());
        let destination = dir
            .path()
            .join(format!("{}.db", case["kind"].as_str().unwrap()));
        window.invoke_archive_path_chosen(false, destination.to_string_lossy().as_ref().into());
        assert!(window.get_error().is_empty(), "{}", window.get_error());
        assert_eq!(window.get_source_path(), case["source"].as_str().unwrap());
        if window.get_content() == 0 {
            assert_eq!(
                window.get_source_hash(),
                case["source_hash"].as_str().unwrap()
            );
        }
        window.invoke_archive_path_chosen(true, "".into());
        assert_eq!(window.get_source_path(), case["source"].as_str().unwrap());
        window.invoke_go();
        let confirmed = window.get_question();
        assert!(confirmed.contains(case["source"].as_str().unwrap()));
        window.invoke_archive_path_chosen(
            false,
            dir.path()
                .join("stale.db")
                .to_string_lossy()
                .as_ref()
                .into(),
        );
        assert_eq!(window.get_question(), confirmed);
        window.invoke_answer(false);
        assert!(
            !destination.exists(),
            "declining confirmation creates no archive"
        );
        if window.get_content() != 0 {
            window.invoke_go();
            window.invoke_answer(true);
            window.invoke_answer(true);
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
            while window.get_running() && std::time::Instant::now() < deadline {
                std::thread::sleep(std::time::Duration::from_millis(50));
                slint::platform::update_timers_and_animations();
            }
            assert!(!window.get_running());
            assert!(window.get_error().is_empty(), "{}", window.get_error());
            let conn = rusqlite::Connection::open(&destination).unwrap();
            let pairs=conn.prepare("SELECT a.tag,b.tag FROM pairs p JOIN tags a ON p.tag_id_1=a.tag_id JOIN tags b ON p.tag_id_2=b.tag_id ORDER BY a.tag,b.tag").unwrap().query_map([],|r|Ok([r.get::<_,String>(0)?,r.get::<_,String>(1)?])).unwrap().collect::<rusqlite::Result<Vec<_>>>().unwrap();
            let expected = recording["archives"]
                .as_array()
                .unwrap()
                .iter()
                .find(|row| row["kind"] == case["kind"])
                .unwrap();
            assert_eq!(serde_json::json!(pairs), expected["pairs"]);
        }
    }
    window.invoke_archive_path_chosen(
        true,
        hydrus_testkit::fixture_path("tag_archive_siblings.db")
            .to_string_lossy()
            .as_ref()
            .into(),
    );
    assert_eq!(
        window.get_error(),
        recording["qt_warnings"][0].as_str().unwrap()
    );
    assert_eq!(window.get_source_path(), "tag_archive_parents.db");
    window.set_count_either(true);
    window.invoke_choices_changed();
    window.invoke_go();
    assert!(
        window
            .get_question()
            .contains("where the child or parent tag of each pair has count on")
    );
    window.invoke_answer(false);
    window.invoke_close_clicked();
}
