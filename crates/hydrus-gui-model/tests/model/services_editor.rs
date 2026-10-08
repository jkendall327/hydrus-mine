//! Service editor reference guards and atomic lifecycle ownership regressions.
use hydrus_core::{ServiceKey, ServiceType};
use hydrus_gui_model::services_editor::{self, Editor};
use hydrus_store::{
    Store,
    autocomplete::{
        CountDomain, CountRange, TagDisplayType, TagQuery, TagSearchScope, search_tags,
    },
    services::{self, ServiceKind},
    services_management,
};
use serde_json::json;

fn store() -> (tempfile::TempDir, std::sync::Arc<Store>) {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    (dir, store)
}
fn registry(store: &Store) -> Vec<services::Service> {
    store
        .snapshot()
        .services
        .all()
        .map(|s| (**s).clone())
        .collect()
}
// leaf: audit-media-services-delete
#[test]
fn reference_rows_guards_and_defaults() {
    let recorded = hydrus_testkit::fixture_json("services.json");
    let legacy = hydrus_testkit::legacy_fixture("basic");
    let dir = tempfile::tempdir().unwrap();
    hydrus_store::import::import_legacy(
        legacy.path(),
        &dir.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store = Store::open(dir.path()).unwrap();
    let mut editor = Editor::new(&store).unwrap();
    assert_eq!(json!(services_editor::COLUMNS), recorded["columns"]);
    assert_eq!(
        json!([editor.sort_column, editor.ascending]),
        recorded["sort"]
    );
    let first_token = editor.rows[0].token;
    assert_eq!(
        editor.rename(first_token, "").unwrap_err(),
        recorded["empty_name_error"]
    );
    for r in &editor.rows {
        let expected = recorded["rows"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["key"] == r.service.key.to_hex())
            .unwrap();
        assert_eq!(json!(r.cells()), expected["row"]);
    }
    for action in recorded["actions"].as_array().unwrap() {
        let kind = u8::try_from(action["type"].as_u64().unwrap()).unwrap();
        let tokens = editor
            .rows
            .iter()
            .filter(|r| r.service.service_type().code() == kind)
            .map(|r| r.token)
            .collect::<Vec<_>>();
        editor.selection.select_many(&tokens);
        let result = editor.delete_question(&store);
        let message = match result {
            Ok(Some(q)) => q,
            Err(e) => e,
            _ => panic!("expected a question"),
        };
        assert_eq!(message, action["messages"][0]);
    }
    let token = editor
        .rows
        .iter()
        .find(|r| r.service.name == "my files")
        .unwrap()
        .token;
    editor.selection.select_only(Some(token));
    assert_eq!(
        editor.delete_question(&store).unwrap_err(),
        recorded["nonempty_delete"][0]
    );
    let downloader = editor
        .rows
        .iter()
        .find(|r| r.service.name == "downloader tags")
        .unwrap()
        .token;
    editor.selection.select_only(Some(downloader));
    editor.delete_selected();
    assert_eq!(
        editor.apply_question().unwrap(),
        recorded["apply_deletion"][0]
    );
    for expected in recorded["rating_defaults"].as_array().unwrap() {
        let kind = services_editor::default_kind(
            ServiceType::from_code(u8::try_from(expected["type"].as_u64().unwrap()).unwrap())
                .unwrap(),
        )
        .unwrap();
        let (d, shape) = match &kind {
            ServiceKind::RatingLike(c) => (
                c.display.clone(),
                Some(match c.appearance {
                    services::StarAppearance::Shape(s) => s.0,
                    services::StarAppearance::Svg(_) => panic!(),
                }),
            ),
            ServiceKind::RatingNumerical(c) => (
                c.display.clone(),
                Some(match c.appearance {
                    services::StarAppearance::Shape(s) => s.0,
                    services::StarAppearance::Svg(_) => panic!(),
                }),
            ),
            ServiceKind::RatingIncDec(d) => (d.clone(), None),
            _ => panic!(),
        };
        assert_eq!(json!(shape), expected["shape"]);
        assert_eq!(json!(d.show_in_thumbnail), expected["thumbnail"]);
        assert_eq!(
            json!(d.show_in_thumbnail_even_when_null),
            expected["null_thumbnail"]
        );
        for (state, c) in [
            (0, d.colours.like),
            (1, d.colours.dislike),
            (2, d.colours.null),
            (4, d.colours.mixed),
        ] {
            let recorded = expected["colours"]
                .as_array()
                .unwrap()
                .iter()
                .find(|c| c[0] == state)
                .unwrap();
            assert_eq!(json!([c.pen.0, c.brush.0]), recorded[1]);
        }
    }
}
// leaf: audit-media-services-apply, audit-media-service-name
#[test]
fn cancel_name_collision_and_apply_publish_snapshot() {
    let (_dir, store) = store();
    let before = registry(&store);
    let mut editor = Editor::new(&store).unwrap();
    let token = editor
        .add(ServiceKey::new(vec![71; 32]), ServiceKind::LocalTags)
        .unwrap();
    editor.rename(token, "MY TAGS").unwrap();
    assert!(editor.rows.iter().any(|r| r.service.name == "MY TAGS (1)"));
    assert_eq!(registry(&store), before);
    editor.apply(&store).unwrap();
    assert!(store.snapshot().services.by_name("MY TAGS (1)").is_some());
    assert_eq!(before.len() + 1, registry(&store).len());
    let stale = Editor::new(&store).unwrap();
    store
        .write_and_refresh(|ctx| {
            ctx.conn().execute(
                "UPDATE services SET name='renamed' WHERE name='favourites'",
                [],
            )?;
            Ok(())
        })
        .unwrap();
    let current = registry(&store);
    assert!(
        stale
            .apply(&store)
            .unwrap_err()
            .to_string()
            .contains("changed while")
    );
    assert_eq!(registry(&store), current);
}
#[test]
fn deletion_reconciles_deleted_union_and_display_counts() {
    let (_dir, store) = store();
    let media_path = store.dir().join("client_files/f00/owned-media.bin");
    std::fs::create_dir_all(media_path.parent().unwrap()).unwrap();
    std::fs::write(&media_path, b"owned media").unwrap();
    let (domain, source) = store
        .write_and_refresh(|ctx| {
            let domain = services::insert(
                ctx.conn(),
                &ServiceKey::new(vec![81; 32]),
                "empty domain",
                &ServiceKind::LocalFiles,
            )?;
            let source = services::insert(
                ctx.conn(),
                &ServiceKey::new(vec![82; 32]),
                "source tags",
                &ServiceKind::LocalTags,
            )?;
            let reg = services::ServiceRegistry::load(ctx.conn())?;
            let target = reg.by_name("my tags").unwrap().id;
            let local = reg.by_name("my files").unwrap().id;
            ctx.conn()
                .execute("INSERT INTO files(hash_id,size,mime) VALUES(100,11,11)", [])?;
            ctx.conn().execute(
                "INSERT INTO file_domain_current VALUES(?,100,NULL)",
                [local],
            )?;
            let combined = reg
                .of_type(ServiceType::CombinedDeletedFile)
                .next()
                .unwrap()
                .id;
            let all = reg.of_type(ServiceType::CombinedFile).next().unwrap().id;
            ctx.conn().execute(
                "INSERT INTO file_domain_deleted(service_id,hash_id,deleted_ms) VALUES(?,99,10)",
                [domain],
            )?;
            ctx.conn().execute(
                "INSERT INTO file_domain_current(service_id,hash_id,added_ms) VALUES(?,99,10)",
                [combined],
            )?;
            ctx.conn()
                .execute("INSERT INTO tag_siblings VALUES(?,0,1,2,NULL)", [source])?;
            ctx.conn().execute(
                "INSERT INTO tag_display_application VALUES(?,0,0,?)",
                rusqlite::params![target, source],
            )?;
            assert_eq!(
                hydrus_store::master::intern_tag(
                    ctx.conn(),
                    &hydrus_core::tag::Tag::new("bad").unwrap()
                )?,
                hydrus_core::TagId(1)
            );
            assert_eq!(
                hydrus_store::master::intern_tag(
                    ctx.conn(),
                    &hydrus_core::tag::Tag::new("good").unwrap()
                )?,
                hydrus_core::TagId(2)
            );
            let tables = hydrus_store::schema::MappingTables::new(target);
            ctx.conn()
                .execute(&format!("INSERT INTO {} VALUES(1,99)", tables.current), [])?;
            hydrus_store::counts::rebuild_all(ctx.conn())?;
            let display: i64 = ctx.conn().query_row(
                &format!(
                    "SELECT current FROM {} WHERE domain_id=? AND tag_id=2",
                    tables.display_counts
                ),
                [all],
                |r| r.get(0),
            )?;
            assert_eq!(display, 1);
            Ok((domain, source))
        })
        .unwrap();
    let cached_snapshot = store.snapshot();
    let combined = cached_snapshot
        .services
        .of_type(ServiceType::CombinedDeletedFile)
        .next()
        .unwrap()
        .id;
    assert!(
        store
            .read(|conn| cached_snapshot
                .domains
                .for_read(conn)?
                .files(conn, combined, false))
            .unwrap()
            .contains(99)
    );
    let before = registry(&store);
    let desired = before
        .iter()
        .filter(|s| s.id != domain && s.id != source)
        .cloned()
        .collect();
    services_management::apply(&store, before, desired).unwrap();
    assert_eq!(std::fs::read(&media_path).unwrap(), b"owned media");
    assert!(
        !store
            .read(|conn| cached_snapshot
                .domains
                .for_read(conn)?
                .files(conn, combined, false))
            .unwrap()
            .contains(99)
    );
    let snap = store.snapshot();
    let target = snap.services.by_name("my tags").unwrap().id;
    assert_eq!(
        snap.display.get(target).ideal(hydrus_core::TagId(1)),
        hydrus_core::TagId(1)
    );
    let table = hydrus_store::schema::MappingTables::new(target).display_counts;
    let counts = move |conn: &rusqlite::Connection| -> hydrus_store::error::Result<_> {
        Ok(conn
            .prepare(&format!(
                "SELECT domain_id,tag_id,current,pending FROM {table} ORDER BY domain_id,tag_id"
            ))?
            .query_map([], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, i64>(1)?,
                    r.get::<_, i64>(2)?,
                    r.get::<_, i64>(3)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?)
    };
    let before = store
        .read(|conn| {
            let remaining: i64 = conn.query_row(
                "SELECT count(*) FROM file_domain_current WHERE hash_id=99",
                [],
                |r| r.get(0),
            )?;
            assert_eq!(remaining, 0);
            let tables = hydrus_store::schema::MappingTables::new(target);
            let display: i64 = conn.query_row(
                &format!("SELECT count(*) FROM {} WHERE tag_id=2", tables.display_counts),
                [],
                |r| r.get(0),
            )?;
            assert_eq!(display, 0);
            let mapping: i64 = conn.query_row(
                &format!("SELECT count(*) FROM {}", tables.current),
                [],
                |r| r.get(0),
            )?;
            assert_eq!(mapping, 1);
            let references: i64 = conn.query_row(
                "SELECT count(*) FROM tag_display_application WHERE source_service_id=? OR display_service_id=?",
                [source, source],
                |r| r.get(0),
            )?;
            assert_eq!(references, 0);
            let all = snap.services.of_type(ServiceType::CombinedFile).next().unwrap().id;
            let matches = search_tags(
                conn,
                &snap.services,
                &snap.display,
                &TagSearchScope {
                    domains: vec![CountDomain { service: all, exact: true }],
                    tag_service: Some(target),
                    display: TagDisplayType::Display,
                    include_current: true,
                    include_pending: true,
                },
                &TagQuery {
                    text: "*".into(),
                    search_namespaces_into_full_tags: false,
                },
            )?;
            assert_eq!(matches.len(), 1);
            assert_eq!(matches[0].tag, "bad");
            assert_eq!(matches[0].count, CountRange::current(1));
            counts(conn)
        })
        .unwrap();
    let after = store
        .write(move |ctx| {
            hydrus_store::counts::rebuild_all(ctx.conn())?;
            counts(ctx.conn())
        })
        .unwrap();
    assert_eq!(before, after);
}
#[test]
fn nonempty_domain_race_rejects_and_rolls_back_all_edits() {
    let (_dir, store) = store();
    let domain = store
        .write_and_refresh(|ctx| {
            services::insert(
                ctx.conn(),
                &ServiceKey::new(vec![91; 32]),
                "race domain",
                &ServiceKind::LocalFiles,
            )
        })
        .unwrap();
    let before = registry(&store);
    let mut desired = before
        .iter()
        .filter(|s| s.id != domain)
        .cloned()
        .collect::<Vec<_>>();
    desired[0].name = "must rollback".into();
    store
        .write(move |ctx| {
            ctx.conn().execute(
                "INSERT INTO file_domain_current VALUES(?,123,NULL)",
                [domain],
            )?;
            Ok(())
        })
        .unwrap();
    assert!(
        services_management::apply(&store, before.clone(), desired)
            .unwrap_err()
            .to_string()
            .contains("needs to be empty")
    );
    assert_eq!(registry(&store), before);
    assert_eq!(
        store
            .read(|conn| Ok(conn.query_row(
                "SELECT count(*) FROM file_domain_current WHERE service_id=?",
                [domain],
                |r| r.get::<_, i64>(0)
            )?))
            .unwrap(),
        1
    );
}

// leaf: audit-media-service-numerical
#[test]
fn numerical_settings_normalize_as_reference() {
    let recorded = hydrus_testkit::fixture_json("services.json");
    let ServiceKind::RatingNumerical(mut config) =
        services_editor::default_kind(ServiceType::LocalRatingNumerical).unwrap()
    else {
        panic!()
    };
    config.num_stars = 1;
    config.allow_zero = false;
    services_editor::normalize_numerical(&mut config);
    assert_eq!(
        json!({"num_stars":config.num_stars,"allow_zero":config.allow_zero,"custom_pad":config.custom_pad,"show_fraction_beside_stars":config.show_fraction_beside_stars}),
        recorded["numerical_one_star"]
    );
    config.num_stars = 2;
    config.allow_zero = false;
    services_editor::normalize_numerical(&mut config);
    assert!(!config.allow_zero);
    config.allow_zero = true;
    services_editor::normalize_numerical(&mut config);
    assert!(config.allow_zero);
}

#[test]
fn writer_rechecks_last_domain_and_rating_limits() {
    let (_dir, store) = store();
    for service_type in [ServiceType::LocalTag, ServiceType::LocalFileDomain] {
        let before = registry(&store);
        let desired = before
            .iter()
            .filter(|s| s.service_type() != service_type)
            .cloned()
            .collect();
        assert!(
            services_management::apply(&store, before.clone(), desired)
                .unwrap_err()
                .to_string()
                .contains("must have at least one")
        );
        assert_eq!(registry(&store), before);
    }
    let before = registry(&store);
    let desired = before
        .iter()
        .filter(|s| !matches!(s.kind, ServiceKind::Trash))
        .cloned()
        .collect();
    assert!(
        services_management::apply(&store, before.clone(), desired)
            .unwrap_err()
            .to_string()
            .contains("cannot be deleted")
    );
    assert_eq!(registry(&store), before);
    let ServiceKind::RatingNumerical(config) =
        services_editor::default_kind(ServiceType::LocalRatingNumerical).unwrap()
    else {
        panic!()
    };
    for (stars, zero, padding, fraction, valid) in [
        (1, true, -64, 0, true),
        (20, false, 64, 2, true),
        (0, true, 0, 0, false),
        (21, true, 0, 0, false),
        (1, false, 0, 0, false),
        (5, true, -65, 0, false),
        (5, true, 65, 0, false),
        (5, true, 0, 3, false),
    ] {
        let c = services::NumericalRatingConfig {
            num_stars: stars,
            allow_zero: zero,
            custom_pad: padding,
            show_fraction_beside_stars: fraction,
            ..config.clone()
        };
        assert_eq!(
            services_management::validate(&ServiceKind::RatingNumerical(c)).is_ok(),
            valid
        );
    }
}

#[test]
fn rating_examples_replay_qt_samples_live_configuration_and_no_saved_values() {
    use hydrus_gui_model::rating_example::{self, Example, Sample};
    use hydrus_store::services::{PenBrush, Rgb, StarAppearance, StarShape};

    let recorded = hydrus_testkit::fixture_json("service_rating_preview.json");
    let legacy = hydrus_testkit::legacy_fixture("basic");
    let dir = tempfile::tempdir().unwrap();
    hydrus_store::import::import_legacy(
        legacy.path(),
        &dir.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store = Store::open(dir.path()).unwrap();
    let before = registry(&store);
    for result in recorded["results"].as_array().unwrap() {
        let service = store
            .snapshot()
            .services
            .by_name(result["name"].as_str().unwrap())
            .unwrap()
            .clone();
        let mut kind = service.kind.clone();
        let mut example = Example::new(&kind).unwrap();
        assert_eq!(json!(rating_example::LABELS), result["labels"]);
        assert_eq!(result["example_value"], json!({}));
        assert_eq!(result["original_unchanged"], true);
        for event in result["events"].as_array().unwrap() {
            let index = event["index"].as_u64().map(|i| usize::try_from(i).unwrap());
            match event["action"].as_str().unwrap() {
                "left" | "right" => example.click(index.unwrap(), event["action"] == "right", 0.5),
                "middle_accept" => example.set_counter(
                    index.unwrap(),
                    u32::try_from(event["value"].as_u64().unwrap()).unwrap(),
                ),
                "middle_cancel" | "initial" | "configured" => {}
                _ => panic!("unrecorded example action"),
            }
            for (sample, expected) in example
                .samples()
                .iter()
                .zip(event["samples"].as_array().unwrap())
            {
                let (state, value) = match sample {
                    Sample::Like(None) => (2, serde_json::Value::Null),
                    Sample::Like(Some(true)) => (0, serde_json::Value::Null),
                    Sample::Like(Some(false)) => (1, serde_json::Value::Null),
                    Sample::Numerical(value) => (
                        if value.is_some() { 3 } else { 2 },
                        json!(value.unwrap_or(0.0)),
                    ),
                    Sample::IncDec(value) => (3, json!(value)),
                };
                assert_eq!(json!(state), expected["state"]);
                assert_eq!(value, expected["rating"]);
                if let Sample::IncDec(value) = sample {
                    assert_eq!(
                        rating_example::counter_width(12.0, *value).to_bits(),
                        expected["icon"][0].as_f64().unwrap().to_bits()
                    );
                }
            }
        }
        let colour = PenBrush {
            pen: Rgb([17, 34, 51]),
            brush: Rgb([68, 85, 102]),
        };
        assert_eq!(
            json!([colour.pen.0, colour.brush.0]),
            result["configured"]["colours"][0][1]
        );
        match &mut kind {
            ServiceKind::RatingLike(c) => {
                assert_eq!(result["configured"]["shape"], 40);
                c.appearance = StarAppearance::Shape(StarShape(40));
                c.display.colours.like = colour;
            }
            ServiceKind::RatingNumerical(c) => {
                c.appearance = StarAppearance::Shape(StarShape(40));
                c.display.colours.like = colour;
                c.num_stars = 7;
                c.custom_pad = 3;
                c.show_fraction_beside_stars = 2;
            }
            ServiceKind::RatingIncDec(c) => c.colours.like = colour,
            _ => panic!("not a rating fixture"),
        }
        for i in 0..4 {
            let control = example.control(i, &kind).unwrap();
            assert_eq!(control.colours.like, colour);
            if matches!(kind, ServiceKind::RatingNumerical(_)) {
                let configured = result["events"].as_array().unwrap().last().unwrap();
                assert_eq!(
                    json!(example.fraction(i, &kind)),
                    configured["rendered"][i]["fraction"]
                );
                assert_eq!(control.shapes().len(), 7);
            }
        }
        // Samples are deliberately absent from the ServiceKind serialization.
        let saved = kind.config_json().unwrap();
        assert!(!saved.contains("12345"));
        let reopened = Example::new(&kind).unwrap();
        assert_ne!(reopened.samples(), example.samples());
    }
    assert_eq!(registry(&store), before);
    assert!(Example::new(&ServiceKind::LocalTags).is_none());
}

#[test]
fn numerical_example_one_star_retains_live_conversion_but_saves_normalized_scale() {
    use hydrus_gui_model::rating_example::Example;
    let recorded = hydrus_testkit::fixture_json("rating_preview_one_star.json");
    let mut kind = services_editor::default_kind(ServiceType::LocalRatingNumerical).unwrap();
    let ServiceKind::RatingNumerical(config) = &mut kind else {
        panic!("expected numerical configuration")
    };
    config.num_stars = u32::try_from(recorded["opening_num_stars"].as_u64().unwrap()).unwrap();
    config.allow_zero = recorded["opening_allow_zero"].as_bool().unwrap();
    let mut example = Example::new(&kind).unwrap();
    for index in 0..4 {
        example.click(index, false, 0.5);
    }
    for event in recorded["events"].as_array().unwrap() {
        let ServiceKind::RatingNumerical(config) = &mut kind else {
            panic!("expected numerical configuration")
        };
        config.num_stars = u32::try_from(event["num_stars"].as_u64().unwrap()).unwrap();
        config.allow_zero = event["live_allow_zero"].as_bool().unwrap();
        let mut saved = config.clone();
        saved.allow_zero = event["checkbox_allow_zero"].as_bool().unwrap();
        services_editor::normalize_numerical(&mut saved);
        assert_eq!(json!(saved.allow_zero), event["saved_allow_zero"]);
        for index in 0..4 {
            assert_eq!(
                json!(example.fraction(index, &kind)),
                event["samples"][index]["fraction"]
            );
            assert_eq!(
                example.control(index, &kind).unwrap().shapes().len(),
                usize::try_from(event["num_stars"].as_u64().unwrap()).unwrap()
            );
        }
    }
    assert_eq!(recorded["original_unchanged"], true);
}

#[test]
fn rating_example_whole_widget_pointer_routes_replay_qt() {
    use hydrus_gui_model::rating_example::{Example, Sample};
    let recorded = hydrus_testkit::fixture_json("rating_preview_pointer.json");
    assert_eq!(recorded["original_unchanged"], true);
    for case in recorded["cases"].as_array().unwrap() {
        let mut kind = services_editor::default_kind(ServiceType::LocalRatingNumerical).unwrap();
        let ServiceKind::RatingNumerical(config) = &mut kind else {
            unreachable!()
        };
        config.num_stars = 5;
        config.allow_zero = true;
        config.custom_pad = 3;
        config.show_fraction_beside_stars = u8::try_from(case["side"].as_u64().unwrap()).unwrap();
        let mut example = Example::new(&kind).unwrap();
        for event in case["events"].as_array().unwrap() {
            let action = event["action"].as_str().unwrap();
            if action == "press" || (action == "move" && event["held"] == true) {
                example.pointer(
                    0,
                    event["right"].as_bool().unwrap(),
                    event["x"].as_f64().unwrap(),
                    event["width"].as_f64().unwrap(),
                    event["icon"].as_f64().unwrap(),
                    action == "move",
                );
            }
            let Sample::Numerical(value) = example.samples()[0] else {
                unreachable!()
            };
            assert_eq!(json!(value.unwrap_or(0.0)), event["rating"], "{event}");
            assert_eq!(json!(if value.is_some() { 3 } else { 2 }), event["state"]);
            assert_eq!(json!(example.fraction(0, &kind)), event["fraction"]);
            assert_eq!(example.samples()[1], Sample::Numerical(None));
        }
        for event in case["chord_events"].as_array().unwrap() {
            let action = event["action"].as_str().unwrap();
            if action == "press" || (action == "move" && event["left_held"] == true) {
                let right = event["button"] == 2 && event["left_held"] == false;
                example.pointer(
                    0,
                    right,
                    event["x"].as_f64().unwrap(),
                    event["width"].as_f64().unwrap(),
                    event["icon"].as_f64().unwrap(),
                    action == "move",
                );
            }
            let Sample::Numerical(value) = example.samples()[0] else {
                unreachable!()
            };
            assert_eq!(json!(value.unwrap_or(0.0)), event["rating"], "{event}");
            assert_eq!(json!(example.fraction(0, &kind)), event["fraction"]);
        }
        assert_eq!(case["example_value"], json!({}));
    }
}
