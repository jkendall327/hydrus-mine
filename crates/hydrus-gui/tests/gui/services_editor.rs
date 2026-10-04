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

#[test]
fn live_rating_examples_stage_only_configuration_and_retire_cancelled_owners() {
    use hydrus_core::{media_viewer::MediaViewerSettings, thumbnail::ThumbnailRatingSettings};
    use hydrus_store::{services::ServiceKind, settings};

    let recorded = hydrus_testkit::fixture_json("service_rating_preview.json");
    let (_dirs, store) = crate::subscriptions::store();
    store
        .write(|ctx| {
            settings::set(
                ctx.conn(),
                &ThumbnailRatingSettings {
                    icon_size: 20.9,
                    incdec_height: 17.4,
                    ..ThumbnailRatingSettings::default()
                },
            )?;
            settings::set(
                ctx.conn(),
                &MediaViewerSettings {
                    rating_icon_size: 15.9,
                    rating_incdec_height: 19.4,
                    ..MediaViewerSettings::default()
                },
            )
        })
        .unwrap();
    let before = store
        .snapshot()
        .services
        .all()
        .map(|s| (**s).clone())
        .collect::<Vec<_>>();
    let rating_counts = || {
        store
            .read(|conn| {
                Ok((
                    conn.query_row("SELECT COUNT(*) FROM ratings", [], |r| r.get::<_, i64>(0))?,
                    conn.query_row("SELECT COUNT(*) FROM ratings_incdec", [], |r| {
                        r.get::<_, i64>(0)
                    })?,
                ))
            })
            .unwrap()
    };
    let counts = rating_counts();
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    open(&ui);
    let manage = bound
        .services_editor
        .manage
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    for result in recorded["results"].as_array().unwrap() {
        let index = manage
            .get_rows()
            .iter()
            .position(|r| r.cells.row_data(0).unwrap() == result["name"].as_str().unwrap())
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
        assert_eq!(edit.get_examples().row_count(), 4);
        assert!(edit.get_example_expanded());
        assert_eq!(
            serde_json::json!(
                edit.get_examples()
                    .iter()
                    .map(|r| r.label.to_string())
                    .collect::<Vec<_>>()
            ),
            result["labels"]
        );
        assert_eq!(edit.get_examples().row_data(0).unwrap().icon_size, 20.0);
        assert_eq!(edit.get_examples().row_data(1).unwrap().icon_size, 15.0);
        assert_eq!(edit.get_examples().row_data(2).unwrap().icon_size, 12.0);
        assert_eq!(edit.get_examples().row_data(3).unwrap().icon_size, 12.0);
        assert_eq!(edit.get_examples().row_data(0).unwrap().incdec_height, 17.0);
        edit.invoke_preview_clicked(0, false, 0.5);
        edit.invoke_preview_clicked(2, false, 0.5);
        manage.invoke_apply_clicked();
        assert!(bound.services_editor.manage.borrow().is_some());
        assert!(bound.services_editor.edit.borrow().is_some());
        edit.invoke_colour_edited(0, true, "#112233".into());
        edit.invoke_colour_edited(0, false, "#445566".into());
        if edit.get_numerical() {
            edit.set_stars(7);
            edit.set_icon_padding(3);
            edit.set_fraction(2);
            edit.set_appearance(12); // recorded shape code 40: diamond
            edit.invoke_preview_edited();
            assert_eq!(edit.get_examples().row_data(0).unwrap().fraction, "3/7");
            assert_eq!(edit.get_examples().row_data(1).unwrap().fraction, "-/7");
            assert_eq!(
                edit.get_examples()
                    .row_data(0)
                    .unwrap()
                    .graphic
                    .shapes
                    .row_count(),
                7
            );
            assert_eq!(edit.get_examples().row_data(0).unwrap().graphic.pad, 3.0);
            assert_eq!(
                edit.get_examples().row_data(0).unwrap().fraction_placement,
                2
            );
        }
        let example = edit.get_examples().row_data(0).unwrap();
        if example.graphic.kind == 2 {
            edit.invoke_counter_edit(3);
            assert!(edit.get_counter_editing());
            edit.set_counter_value(12345);
            edit.invoke_apply_clicked();
            assert!(bound.services_editor.edit.borrow().is_some());
            edit.invoke_counter_answered(false);
            assert_eq!(edit.get_examples().row_data(3).unwrap().graphic.text, "0");
            edit.invoke_counter_edit(3);
            edit.set_counter_value(12345);
            edit.invoke_counter_answered(true);
            assert_eq!(
                edit.get_examples().row_data(3).unwrap().graphic.text,
                "12,345"
            );
            assert_eq!(edit.get_examples().row_data(3).unwrap().counter_width, 38.0);
            assert_eq!(edit.get_examples().row_data(0).unwrap().graphic.text, "1");
        } else {
            let pen = example.graphic.shapes.row_data(0).unwrap().pen;
            let brush = example.graphic.shapes.row_data(0).unwrap().brush;
            assert_eq!(pen, slint::Color::from_rgb_u8(17, 34, 51));
            assert_eq!(brush, slint::Color::from_rgb_u8(68, 85, 102));
            assert_ne!(
                example.graphic.shapes.row_data(0).unwrap().brush,
                edit.get_examples()
                    .row_data(1)
                    .unwrap()
                    .graphic
                    .shapes
                    .row_data(0)
                    .unwrap()
                    .brush
            );
        }
        // Child Cancel discards samples and configuration; a retained child is inert.
        edit.invoke_cancel_clicked();
        let old_samples = edit.get_examples().iter().collect::<Vec<_>>();
        manage.invoke_edit_clicked();
        let successor = bound
            .services_editor
            .edit
            .borrow()
            .as_ref()
            .unwrap()
            .clone_strong();
        edit.invoke_preview_clicked(1, false, 0.5);
        edit.invoke_counter_edit(0);
        edit.invoke_counter_answered(true);
        edit.invoke_apply_clicked();
        edit.invoke_cancel_clicked();
        assert_eq!(edit.get_examples().iter().collect::<Vec<_>>(), old_samples);
        assert!(bound.services_editor.edit.borrow().is_some());
        assert_eq!(
            successor.get_examples().row_data(0).unwrap().graphic.text,
            if example.graphic.kind == 2 { "0" } else { "" }
        );
        if successor.get_numerical() {
            assert_eq!(successor.get_stars(), 5);
        }
        successor.invoke_cancel_clicked();
    }
    manage.invoke_cancel_clicked();
    assert_eq!(
        store
            .snapshot()
            .services
            .all()
            .map(|s| (**s).clone())
            .collect::<Vec<_>>(),
        before
    );
    assert_eq!(rating_counts(), counts);

    // Accepting an example configuration stages the actual service edit; only
    // the parent Apply persists it. Sample values never rate existing files.
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
        .position(|r| r.cells.row_data(0).unwrap() == "stars")
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
    edit.set_stars(7);
    edit.set_icon_padding(3);
    edit.set_fraction(2);
    edit.set_appearance(12);
    edit.invoke_colour_edited(0, true, "#112233".into());
    edit.invoke_colour_edited(0, false, "#445566".into());
    edit.invoke_preview_clicked(0, false, 0.5);
    edit.invoke_preview_edited();
    let last = (0..100)
        .take_while(|&n| windows.get(n).is_some())
        .last()
        .unwrap();
    let pixels = headless::render(&windows.get(last).unwrap(), 640, 900);
    assert!(pixels.chunks_exact(4).any(|pixel| pixel[3] == 255));
    assert!(pixels.chunks_exact(4).any(|pixel| pixel != &pixels[..4]));
    headless::save_png(
        &std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("service-rating-examples.png"),
        &pixels,
        640,
        900,
    )
    .unwrap();
    edit.invoke_apply_clicked();
    assert_eq!(
        store
            .snapshot()
            .services
            .all()
            .map(|s| (**s).clone())
            .collect::<Vec<_>>(),
        before
    );
    manage.invoke_apply_clicked();
    let committed = store.snapshot().services.by_name("stars").unwrap().clone();
    let ServiceKind::RatingNumerical(config) = &committed.kind else {
        panic!("wrong rating kind")
    };
    assert_eq!(config.num_stars, 7);
    assert_eq!(config.custom_pad, 3);
    assert_eq!(config.show_fraction_beside_stars, 2);
    assert_eq!(config.display.colours.like.brush.0, [68, 85, 102]);
    assert_eq!(rating_counts(), counts);
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
        .position(|r| r.cells.row_data(0).unwrap() == "stars")
        .unwrap();
    manage.invoke_row_clicked(i32::try_from(index).unwrap(), false, false);
    manage.invoke_edit_clicked();
    let reopened = bound
        .services_editor
        .edit
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    assert_eq!(reopened.get_stars(), 7);
    assert_eq!(reopened.get_examples().row_data(0).unwrap().fraction, "-/7");
    reopened.invoke_preview_clicked(0, false, 0.5);
    // Closing the parent while a sample is active invalidates all child callbacks.
    manage.invoke_cancel_clicked();
    reopened.invoke_colour_edited(0, false, "#ffffff".into());
    reopened.invoke_apply_clicked();
    assert!(bound.services_editor.edit.borrow().is_none());
    assert_eq!(
        store.snapshot().services.by_name("stars").unwrap().as_ref(),
        committed.as_ref()
    );
    assert_eq!(rating_counts(), counts);
}

#[test]
fn one_star_rating_preview_replays_four_samples_and_preserves_saved_normalization() {
    use hydrus_store::services::{self, ServiceKind};
    let fixture = hydrus_testkit::fixture_json("rating_preview_one_star.json");
    let (_dirs, store) = crate::subscriptions::store();
    headless::init();
    let service = store
        .snapshot()
        .services
        .by_name(fixture["name"].as_str().unwrap())
        .unwrap()
        .clone();
    let mut kind = service.kind.clone();
    let ServiceKind::RatingNumerical(config) = &mut kind else {
        panic!("expected numerical configuration")
    };
    config.allow_zero = fixture["opening_allow_zero"].as_bool().unwrap();
    let opening = kind.clone();
    store
        .write_and_refresh(move |ctx| services::update_config(ctx.conn(), service.id, &kind))
        .unwrap();
    let ratings = store
        .read(|conn| {
            Ok(conn
                .prepare(
                    "SELECT service_id, hash_id, rating FROM ratings ORDER BY service_id, hash_id",
                )?
                .query_map([], |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, f64>(2)?.to_bits(),
                    ))
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?)
        })
        .unwrap();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
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
        .position(|row| row.cells.row_data(0).unwrap() == fixture["name"].as_str().unwrap())
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
    assert_eq!(
        edit.get_stars(),
        i32::try_from(fixture["opening_num_stars"].as_i64().unwrap()).unwrap()
    );
    for index in 0..4 {
        edit.invoke_preview_clicked(index, false, 0.5);
    }
    for event in fixture["events"].as_array().unwrap() {
        edit.set_stars(i32::try_from(event["num_stars"].as_i64().unwrap()).unwrap());
        edit.set_allow_zero(event["checkbox_allow_zero"].as_bool().unwrap());
        edit.invoke_preview_edited();
        assert!(edit.get_error().is_empty());
        for (index, row) in edit.get_examples().iter().enumerate() {
            assert_eq!(
                row.fraction,
                event["samples"][index]["fraction"].as_str().unwrap()
            );
            assert_eq!(
                row.graphic.shapes.row_count(),
                usize::try_from(event["num_stars"].as_u64().unwrap()).unwrap()
            );
        }
        assert_eq!(
            store.snapshot().services.by_name("stars").unwrap().kind,
            opening
        );
    }
    edit.set_stars(1);
    edit.set_allow_zero(false);
    edit.invoke_preview_edited();
    assert_eq!(edit.get_examples().row_data(0).unwrap().fraction, "1/1");
    edit.invoke_apply_clicked();
    assert_eq!(
        store.snapshot().services.by_name("stars").unwrap().kind,
        opening
    );
    manage.invoke_apply_clicked();
    let reopened = hydrus_store::Store::open(store.dir()).unwrap();
    let saved = reopened
        .snapshot()
        .services
        .by_name("stars")
        .unwrap()
        .clone();
    let ServiceKind::RatingNumerical(config) = &saved.kind else {
        panic!("expected numerical configuration")
    };
    assert_eq!(config.num_stars, 1);
    assert!(config.allow_zero);
    assert_eq!(
        reopened
            .read(|conn| {
                Ok(conn
                    .prepare("SELECT service_id, hash_id, rating FROM ratings ORDER BY service_id, hash_id")?
                    .query_map([], |row| {
                        Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?, row.get::<_, f64>(2)?.to_bits()))
                    })?
                    .collect::<rusqlite::Result<Vec<_>>>()?)
            })
            .unwrap(),
        ratings
    );
    ui.hide().unwrap();
}

#[test]
fn numerical_examples_drag_and_fraction_text_use_the_whole_widget_hit_area() {
    use hydrus_gui::{services_editor_window, services_editor_window::Slots};
    use hydrus_store::{services, services::ServiceKind};
    use slint::platform::{PointerEventButton, WindowEvent};
    use std::rc::Rc;

    let (_dirs, store) = crate::subscriptions::store();
    let reference = hydrus_testkit::fixture_json("rating_preview_pointer.json");
    let windows = headless::init();
    for case in reference["cases"].as_array().unwrap() {
        let side = u8::try_from(case["side"].as_u64().unwrap()).unwrap();
        let service = store.snapshot().services.by_name("stars").unwrap().clone();
        let id = service.id;
        let mut kind = service.kind.clone();
        let ServiceKind::RatingNumerical(config) = &mut kind else {
            unreachable!()
        };
        config.num_stars = 5;
        config.allow_zero = true;
        config.custom_pad = 3;
        config.show_fraction_beside_stars = side;
        store
            .write_and_refresh(move |ctx| services::update_config(ctx.conn(), id, &kind))
            .unwrap();
        let before = store.snapshot().services.by_name("stars").unwrap().clone();
        let slots = Slots::default();
        let manage = services_editor_window::open(&store, &slots, Rc::new(|| {})).unwrap();
        let index = manage
            .get_rows()
            .iter()
            .position(|r| r.cells.row_data(0).unwrap() == "stars")
            .unwrap();
        manage.invoke_row_clicked(i32::try_from(index).unwrap(), false, false);
        manage.invoke_edit_clicked();
        let edit = slots.edit.borrow().as_ref().unwrap().clone_strong();
        let last = (0..100)
            .take_while(|&n| windows.get(n).is_some())
            .last()
            .unwrap();
        let window = windows.get(last).unwrap();
        for _ in 0..3 {
            headless::render(&window, 640, 1000);
        }
        let x = edit.get_first_preview_x();
        let y = edit.get_first_preview_y() + edit.get_first_preview_height() / 2.0;
        let width = edit.get_first_preview_width();
        assert!(width > 200.0 && y > 0.0 && y < 1000.0);
        let move_to = |at: f32| {
            edit.window().dispatch_event(WindowEvent::PointerMoved {
                position: slint::LogicalPosition::new(x + at, y),
            });
            headless::render(&window, 640, 1000);
        };
        let press = |at: f32, button| {
            edit.window().dispatch_event(WindowEvent::PointerMoved {
                position: slint::LogicalPosition::new(x + at, y),
            });
            edit.window().dispatch_event(WindowEvent::PointerPressed {
                position: slint::LogicalPosition::new(x + at, y),
                button,
            });
            headless::render(&window, 640, 1000);
        };
        let release = |at: f32| {
            edit.window().dispatch_event(WindowEvent::PointerReleased {
                position: slint::LogicalPosition::new(x + at, y),
                button: PointerEventButton::Left,
            });
            headless::render(&window, 640, 1000);
        };
        press(2.0, PointerEventButton::Left);
        assert_eq!(edit.get_examples().row_data(0).unwrap().fraction, "0/5");
        move_to(width * 0.85);
        assert_eq!(edit.get_examples().row_data(0).unwrap().fraction, "4/5");
        if side == 2 {
            let pixels = headless::render(&window, 640, 1000);
            assert!(pixels.chunks_exact(4).any(|pixel| pixel[3] == 255));
            assert!(pixels.chunks_exact(4).any(|pixel| pixel != &pixels[..4]));
            headless::save_png(
                &std::path::Path::new(env!("CARGO_TARGET_TMPDIR"))
                    .join("service-rating-pointer.png"),
                &pixels,
                640,
                1000,
            )
            .unwrap();
        }
        move_to(-10.0);
        assert_eq!(edit.get_examples().row_data(0).unwrap().fraction, "4/5");
        release(width * 0.85);
        move_to(width * 0.2);
        assert_eq!(
            edit.get_examples().row_data(0).unwrap().fraction,
            "4/5",
            "hover after release must not rate"
        );
        assert_eq!(edit.get_examples().row_data(1).unwrap().fraction, "-/5");
        if side != 0 {
            let fraction = edit.get_first_preview_fraction_x() - x + 3.0;
            press(fraction, PointerEventButton::Left);
            release(fraction);
            // Clicking the actual text is handled by the same stretched widget,
            // not treated as an isolated star/fraction segment.
            assert_eq!(
                edit.get_examples().row_data(0).unwrap().fraction,
                if side == 1 { "0/5" } else { "1/5" }
            );
        }
        press(width - 2.0, PointerEventButton::Left);
        release(width - 2.0);
        assert_eq!(edit.get_examples().row_data(0).unwrap().fraction, "5/5");
        press(0.0, PointerEventButton::Left);
        release(0.0);
        assert_eq!(edit.get_examples().row_data(0).unwrap().fraction, "-/5");
        press(width * 0.5, PointerEventButton::Right);
        edit.window().dispatch_event(WindowEvent::PointerReleased {
            position: slint::LogicalPosition::new(x + width * 0.5, y),
            button: PointerEventButton::Right,
        });
        assert_eq!(edit.get_examples().row_data(0).unwrap().fraction, "-/5");
        manage.invoke_cancel_clicked();
        let retired = edit.get_examples().row_data(0).unwrap().fraction;
        edit.invoke_preview_pointer(0, false, width * 0.85, width, 12.0, true);
        edit.invoke_apply_clicked();
        assert_eq!(edit.get_examples().row_data(0).unwrap().fraction, retired);
        assert!(slots.edit.borrow().is_none());
        assert_eq!(
            store.snapshot().services.by_name("stars").unwrap().as_ref(),
            before.as_ref()
        );
    }
}
