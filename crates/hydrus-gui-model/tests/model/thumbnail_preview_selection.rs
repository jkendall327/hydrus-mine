//! Actual Qt options dependencies and every recorded modifier focus publication.
use hydrus_core::HashId;
use hydrus_gui_model::{
    options::{Editor, Row, Settings},
    selection::{Move, Selection},
    thumbnail_preview_selection::has_duration,
};
use hydrus_store::{Store, settings, thumbnail_preview_selection::Preferences};

fn flags(value: &Preferences) -> serde_json::Value {
    serde_json::json!([
        value.ctrl_focus,
        value.ctrl_only_static,
        value.shift_focus,
        value.shift_only_static
    ])
}
fn policy(value: &serde_json::Value) -> Preferences {
    Preferences {
        ctrl_focus: value[0].as_bool().unwrap(),
        ctrl_only_static: value[1].as_bool().unwrap(),
        shift_focus: value[2].as_bool().unwrap(),
        shift_only_static: value[3].as_bool().unwrap(),
    }
}
fn indices(editor: &Editor) -> [usize; 4] {
    let rows = editor.rows();
    let ctrl = rows.iter().position(|row| matches!(row, Row::Opt{option,..} if option.label.starts_with("On ctrl-selection"))).unwrap();
    let shift = rows.iter().position(|row| matches!(row, Row::Opt{option,..} if option.label.starts_with("On shift-selection"))).unwrap();
    [ctrl, ctrl + 1, shift, shift + 1]
}
fn editor(store: &Store) -> Editor {
    let mut editor = Editor::new(store.read(Settings::load).unwrap());
    let page = editor
        .page_names()
        .iter()
        .position(|name| *name == "thumbnails")
        .unwrap();
    editor.show_page(page);
    editor
}
#[test]
fn actual_defaults_disabled_values_cancel_reopen_and_edited_field_merge() {
    let fixture = hydrus_testkit::fixture_json("thumbnail_preview_selection.json");
    let legacy = hydrus_testkit::legacy_fixture("basic");
    let dir = tempfile::tempdir().unwrap();
    hydrus_store::import::import_legacy(
        legacy.path(),
        &dir.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store = Store::open(dir.path()).unwrap();
    assert_eq!(
        flags(&store.read(settings::get).unwrap()),
        fixture["defaults"]["values"]
    );
    let before = store.read(Settings::load).unwrap();
    let mut cancelled = editor(&store);
    for row in indices(&cancelled) {
        cancelled.check(row, true);
    }
    drop(cancelled);
    assert_eq!(store.read(Settings::load).unwrap(), before);
    assert_eq!(
        flags(&before.thumbnail_preview_selection),
        fixture["cancelled"]
    );
    for case in fixture["controls"].as_array().unwrap() {
        let before = store.read(Settings::load).unwrap();
        let mut draft = editor(&store);
        let rows = indices(&draft);
        for (index, value) in rows.into_iter().zip(case["input"].as_array().unwrap()) {
            draft.check(index, value.as_bool().unwrap());
        }
        let shown = draft.rows();
        for (index, (row, expected)) in rows
            .into_iter()
            .zip(case["enabled"].as_array().unwrap())
            .enumerate()
        {
            let Row::Opt {
                option, enabled, ..
            } = shown[row]
            else {
                panic!("checkbox");
            };
            assert_eq!(option.label, fixture["labels"][index].as_str().unwrap());
            assert_eq!(enabled, expected.as_bool().unwrap());
        }
        let ghost = shown
            .iter()
            .find_map(|row| match row {
                Row::Opt {
                    option, enabled, ..
                } if option.label.starts_with("When shift-selecting") => Some(*enabled),
                _ => None,
            })
            .unwrap();
        assert_eq!(ghost, case["ghost_enabled"].as_bool().unwrap());
        let (after, _, errors) = draft.applied();
        assert!(errors.is_empty());
        assert_eq!(flags(&after.thumbnail_preview_selection), case["saved"]);
        assert_eq!(store.read(Settings::load).unwrap(), before);
        store.write(|ctx| after.save(ctx.conn(), &before)).unwrap();
        let reopened = Store::open(dir.path()).unwrap();
        assert_eq!(
            flags(&reopened.read(settings::get).unwrap()),
            case["reopened"]
        );
    }
    let before = Preferences::default();
    let edited = Preferences {
        ctrl_focus: true,
        ..before.clone()
    };
    let concurrent = Preferences {
        shift_focus: true,
        shift_only_static: true,
        ..before.clone()
    };
    store
        .write(|ctx| {
            settings::set(ctx.conn(), &concurrent)?;
            edited.save_changed(ctx.conn(), &before)
        })
        .unwrap();
    assert_eq!(
        store.read(settings::get::<Preferences>).unwrap(),
        Preferences {
            ctrl_focus: true,
            ..concurrent
        }
    );
    assert_eq!(
        serde_json::from_str::<Preferences>("{}").unwrap(),
        Preferences::default()
    );
    let object = hydrus_legacy::serialisable::SerialisableObject::from_tuple_str(
        &fixture["legacy_options"].to_string(),
    )
    .unwrap();
    let decoded = hydrus_legacy::objects::ClientOptions::from_object(&object).unwrap();
    let mut legacy = Preferences::default();
    legacy.apply_legacy(&decoded.booleans);
    assert_eq!(flags(&legacy), serde_json::json!([true, true, true, true]));
}
#[test]
fn all_qt_modifier_ranges_focus_anchor_ghost_and_keyboard_publications() {
    let fixture = hydrus_testkit::fixture_json("thumbnail_preview_selection.json");
    let durations = fixture["durations"].as_array().unwrap();
    let files: Vec<HashId> = (0..u32::try_from(durations.len()).unwrap())
        .map(HashId)
        .collect();
    for case in fixture["selections"].as_array().unwrap() {
        let prefs = policy(&case["policy"]);
        let mut selection = Selection::default();
        let mut published = Vec::new();
        for step in case["steps"].as_array().unwrap() {
            let before = selection.focused();
            let ctrl = step["ctrl"].as_bool().unwrap();
            let shift = step["shift"].as_bool().unwrap();
            let eligible = |file: HashId| {
                prefs.focus_target(ctrl, shift, !durations[file.0 as usize].is_null())
            };
            if matches!(step["action"].as_str().unwrap(), "move" | "shift-move") {
                selection.move_focus_with_preview(
                    &files,
                    Move::Right,
                    shift,
                    (5, 4),
                    false,
                    &eligible,
                );
            } else {
                let file = step["index"].as_u64().map(|index| files[index as usize]);
                selection.hit_with_preview_focus(
                    &files,
                    file,
                    ctrl,
                    shift,
                    file.is_some_and(eligible),
                );
            }
            if before != selection.focused() {
                published.push(selection.focused());
            }
            assert_eq!(
                serde_json::json!({"selected":selection.files(&files),"focused":selection.focused(),"last_hit":selection.last_hit(),"anchor":selection.range_anchor(),"ghost":selection.ghost_focus(),"published":published}),
                step["after"],
                "policy {}, step {}",
                case["policy"],
                step["action"]
            );
        }
    }
}
#[test]
fn exact_singleton_zero_and_collection_aggregate_duration_eligibility() {
    let prefs = Preferences {
        ctrl_focus: true,
        ctrl_only_static: true,
        shift_focus: true,
        shift_only_static: true,
    };
    let fixture = hydrus_testkit::fixture_json("thumbnail_preview_selection.json");
    for case in fixture["duration_shapes"].as_array().unwrap() {
        let durations = case["input"]
            .as_array()
            .unwrap()
            .iter()
            .map(serde_json::Value::as_u64);
        let timed = has_duration(durations, case["collection"].as_bool().unwrap());
        let expected = !case["duration"].is_null();
        assert_eq!(timed, expected);
        assert_eq!(prefs.focus_target(true, false, timed), !expected);
        assert_eq!(prefs.focus_target(true, true, timed), !expected);
    }
}
