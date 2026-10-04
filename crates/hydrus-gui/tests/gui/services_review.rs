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
    assert!(window.get_unavailable().is_empty());
    assert!(window.get_trash_service());
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

fn choose_service(window: &hydrus_gui::ServicesReviewWindow, name: &str) -> i32 {
    let index = window
        .get_services()
        .iter()
        .position(|row| row.ends_with(&format!(": {name}")))
        .unwrap();
    let index = i32::try_from(index).unwrap();
    window.invoke_selected_service(index);
    index
}

#[test]
fn local_bulk_review_replays_confirmations_store_changes_and_reopens() {
    use hydrus_core::{ServiceType, Sha256};
    use hydrus_store::{content::RatingClearScope, settings};
    use slint::platform::{Key, WindowEvent};
    use std::{cell::Cell, rc::Rc};
    let fixture = hydrus_testkit::fixture_json("service_bulk.json");
    let (_dirs, store) = crate::subscriptions::store();
    let windows = headless::init();
    let hashes: Vec<Sha256> = fixture["rating_corpus"]
        .as_array()
        .unwrap()
        .iter()
        .map(|hash| hash.as_str().unwrap().parse().unwrap())
        .collect();
    let seed_hashes: Vec<Sha256> = fixture["trash_seed"]
        .as_array()
        .unwrap()
        .iter()
        .map(|hash| hash.as_str().unwrap().parse().unwrap())
        .collect();
    let seed = store
        .write(move |ctx| {
            seed_hashes
                .iter()
                .map(|h| hydrus_store::master::intern_hash(ctx.conn(), h))
                .collect::<hydrus_store::Result<Vec<_>>>()
        })
        .unwrap();
    let ids = store
        .write(move |ctx| {
            hashes
                .iter()
                .map(|h| hydrus_store::master::intern_hash(ctx.conn(), h))
                .collect::<hydrus_store::Result<Vec<_>>>()
        })
        .unwrap();
    store
        .write(|ctx| {
            settings::set(
                ctx.conn(),
                &hydrus_store::delete_lock::DeleteLock::default(),
            )
        })
        .unwrap();
    store
        .write_content(|writer| writer.undelete_trash())
        .unwrap();
    let domain = store
        .snapshot()
        .services
        .of_type(ServiceType::CombinedLocalFileDomains)
        .next()
        .unwrap()
        .id;
    let trash = store
        .snapshot()
        .services
        .of_type(ServiceType::LocalFileTrashDomain)
        .next()
        .unwrap()
        .clone();
    let changed = Rc::new(Cell::new(0));
    let window = hydrus_gui::services_review_window::open_with_changed(
        store.clone(),
        Rc::new({
            let changed = changed.clone();
            move || changed.set(changed.get() + 1)
        }),
    )
    .unwrap();
    let trash_index = choose_service(&window, &trash.name);
    for event in fixture["events"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["kind"] == "trash")
    {
        if !event["accepted"].as_bool().unwrap() {
            let seed = seed.clone();
            store
                .write_content(move |writer| writer.delete_files(domain, &seed, None))
                .unwrap();
            window.invoke_refresh_clicked();
        }
        assert_eq!(
            window.get_trash_nonempty(),
            event["before"]["clear_enabled"].as_bool().unwrap()
        );
        let action = if event["action"] == "clear" { 0 } else { 1 };
        window.invoke_maintenance(action);
        assert_eq!(
            window.get_question(),
            event["asked"][0]["message"].as_str().unwrap()
        );
        if action == 1 && !event["accepted"].as_bool().unwrap() {
            window.window().dispatch_event(WindowEvent::KeyPressed {
                text: Key::Escape.into(),
            });
            assert!(window.get_question().is_empty());
            assert_eq!(changed.get(), 0);
            window.invoke_maintenance(action);
            assert_eq!(
                window.get_question(),
                event["asked"][0]["message"].as_str().unwrap()
            );
        }
        window.invoke_selected_service(0); // A question owns its captured service.
        assert_eq!(window.get_selected(), trash_index);
        window.invoke_maintenance(4); // Another action cannot replace a pending prompt.
        assert_eq!(
            window.get_question(),
            event["asked"][0]["message"].as_str().unwrap()
        );
        if action == 0 && event["accepted"].as_bool().unwrap() {
            let pixels = headless::render(&windows.get(windows.count() - 1).unwrap(), 700, 440);
            assert!(pixels.chunks_exact(4).any(|pixel| pixel[3] == 255));
            assert!(pixels.chunks_exact(4).any(|pixel| pixel != &pixels[..4]));
            headless::save_png(
                &std::path::Path::new(env!("CARGO_TARGET_TMPDIR"))
                    .join("services-bulk-trash-confirmation.png"),
                &pixels,
                700,
                440,
            )
            .unwrap();
        }
        window.invoke_answer(event["accepted"].as_bool().unwrap());
        assert!(window.get_question().is_empty());
        assert!(window.get_error().is_empty());
        assert_eq!(
            window.get_trash_nonempty(),
            event["after"]["clear_enabled"].as_bool().unwrap()
        );
        let remaining: usize = store
            .read(|conn| {
                Ok(conn.query_row(
                    "SELECT count(*) FROM file_domain_current WHERE service_id = ?",
                    [trash.id],
                    |r| r.get(0),
                )?)
            })
            .unwrap();
        assert_eq!(
            remaining,
            event["after"]["hashes"].as_array().unwrap().len()
        );
    }
    assert_eq!(changed.get(), 2);
    let corpus = ids.clone();
    store
        .write_content(move |writer| writer.delete_files(domain, &corpus[1..2], None))
        .unwrap();
    for event in fixture["events"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["kind"] == "rating")
    {
        let service = store
            .snapshot()
            .services
            .by_name(event["service"].as_str().unwrap())
            .unwrap()
            .clone();
        let incdec = service.service_type() == ServiceType::LocalRatingIncDec;
        if !event["accepted"].as_bool().unwrap() {
            let ids = ids.clone();
            let service_id = service.id;
            store
                .write_content(move |writer| {
                    writer.clear_ratings(service_id, RatingClearScope::All)?;
                    if incdec {
                        writer.set_incdec(service_id, &ids, 7)
                    } else {
                        writer.set_rating(service_id, &ids, Some(1.0))
                    }
                })
                .unwrap();
        }
        window.invoke_refresh_clicked();
        choose_service(&window, &service.name);
        assert_eq!(
            window.get_statistics(),
            event["before"]["label"].as_str().unwrap()
        );
        window.invoke_rating_menu_open();
        assert!(window.get_rating_menu());
        let action = match event["action"].as_str().unwrap() {
            "delete_for_deleted_files" => 2,
            "delete_for_non_local_files" => 3,
            "delete_for_all_files" => 4,
            unknown => panic!("unrecorded {unknown}"),
        };
        window.invoke_maintenance(action);
        assert!(!window.get_rating_menu());
        assert_eq!(
            window.get_question(),
            event["asked"][0]["message"].as_str().unwrap()
        );
        if action == 3 && event["accepted"].as_bool().unwrap() && incdec {
            let pixels = headless::render(&windows.get(windows.count() - 1).unwrap(), 700, 440);
            assert!(pixels.chunks_exact(4).any(|pixel| pixel[3] == 255));
            assert!(pixels.chunks_exact(4).any(|pixel| pixel != &pixels[..4]));
            headless::save_png(
                &std::path::Path::new(env!("CARGO_TARGET_TMPDIR"))
                    .join("services-bulk-rating-confirmation.png"),
                &pixels,
                700,
                440,
            )
            .unwrap();
        }
        window.invoke_answer(event["accepted"].as_bool().unwrap());
        assert_eq!(
            window.get_statistics(),
            event["after"]["label"].as_str().unwrap()
        );
        assert!(window.get_error().is_empty());
        let mut expected = if !event["accepted"].as_bool().unwrap() {
            ids.clone()
        } else {
            match action {
                2 => vec![ids[0], ids[1], ids[3]],
                3 => vec![ids[0], ids[1]],
                4 => Vec::new(),
                _ => unreachable!(),
            }
        };
        expected.sort();
        let service_id = service.id;
        let persisted = store
            .read(|conn| {
                let table = if incdec { "ratings_incdec" } else { "ratings" };
                Ok(conn
                    .prepare(&format!(
                        "SELECT hash_id FROM {table} WHERE service_id = ? ORDER BY hash_id"
                    ))?
                    .query_map([service_id], |row| row.get::<_, hydrus_core::HashId>(0))?
                    .collect::<rusqlite::Result<Vec<_>>>()?)
            })
            .unwrap();
        assert_eq!(persisted, expected);
        let reopened = hydrus_store::Store::open(store.dir()).unwrap();
        let row = hydrus_gui_model::services_review::rows(&reopened)
            .unwrap()
            .into_iter()
            .find(|row| row.key == service.key)
            .unwrap();
        assert_eq!(row.statistics, event["after"]["label"].as_str().unwrap());
    }
    assert_eq!(changed.get(), 11);
    // Owner close cancels a live destructive question; retained callbacks stay retired.
    choose_service(&window, "favourites");
    window.invoke_maintenance(4);
    assert!(!window.get_question().is_empty());
    window.invoke_close_clicked();
    window.show().unwrap();
    window.invoke_answer(true);
    window.invoke_maintenance(4);
    assert!(window.get_question().is_empty());
    assert_eq!(changed.get(), 11);
    window.hide().unwrap();
    let reopened = hydrus_gui::services_review_window::open(store).unwrap();
    choose_service(&reopened, "favourites");
    assert_eq!(reopened.get_statistics(), "0 files are rated");
    assert!(reopened.get_question().is_empty());
    reopened.invoke_close_clicked();
}
