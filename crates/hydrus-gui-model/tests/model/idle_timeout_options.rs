//! Actual Qt NoneableSpinCtrl display/seconds conversion and live idle gates.
use hydrus_gui_model::{
    options::{Editor, Kind, Row, Settings, Value},
    session_lifecycle::Idle,
};
use hydrus_store::{
    Store,
    settings::{self, GuiIdleSettings},
};
const LABELS: [&str; 3] = [
    "Permit idle mode if no general browsing activity has occurred in the past: ",
    "Permit idle mode if your mouse cursor has not been moved in the past: ",
    "Permit idle mode if no Client API requests in the past: ",
];
fn editor(settings: Settings) -> (Editor, Vec<usize>) {
    let mut editor = Editor::new(settings);
    let page = editor
        .page_names()
        .iter()
        .position(|name| *name == "maintenance and processing")
        .unwrap();
    editor.show_page(page);
    let rows = LABELS
        .iter()
        .map(|label| {
            editor
                .rows()
                .iter()
                .position(|row| matches!(row,Row::Opt{option,..} if option.label==*label))
                .unwrap()
        })
        .collect();
    (editor, rows)
}
fn snapshot(editor: &Editor, rows: &[usize]) -> serde_json::Value {
    let all = editor.rows();
    let values: Vec<_> = rows
        .iter()
        .map(|&row| {
            let Row::Opt {
                option,
                value,
                number,
                enabled,
                ..
            } = &all[row]
            else {
                panic!("actual timeout option");
            };
            let Kind::Noneable {
                none_phrase,
                min,
                max,
                unit,
                ..
            } = &option.kind
            else {
                panic!("actual noneable spinner");
            };
            let Value::Noneable(minutes) = value else {
                panic!("displayed minute value");
            };
            serde_json::json!({
                "number": minutes.unwrap_or(*number),
                "minimum": min,
                "maximum": max,
                "none": minutes.is_none(),
                "phrase": none_phrase,
                "unit": unit,
                "enabled": enabled,
                "number_enabled": *enabled && minutes.is_some(),
                "seconds": minutes.map(|minutes| minutes * 60)
            })
        })
        .collect();
    serde_json::json!(values)
}
#[test]
fn noneable_timeout_controls_replay_qt_defaults_floor_bounds_retention_and_reopen() {
    let fixture = hydrus_testkit::fixture_json("idle_timeout_options.json");
    let directory = tempfile::tempdir().unwrap();
    let store = Store::open(directory.path()).unwrap();
    let before = store.read(Settings::load).unwrap();
    let (mut editor, rows) = editor(before.clone());
    assert_eq!(snapshot(&editor, &rows), fixture["initial"]);
    for step in fixture["edits"].as_array().unwrap() {
        for &row in &rows {
            match step["action"].as_str().unwrap() {
                "set_seconds" => {
                    editor.none(row, step["input"].is_null());
                    if let Some(seconds) = step["input"].as_i64() {
                        editor.number(row, seconds / 60);
                    }
                }
                "edit_minutes" => {
                    editor.none(row, false);
                    editor.number(row, step["input"].as_i64().unwrap());
                }
                "none" => editor.none(row, step["input"].as_bool().unwrap()),
                other => panic!("unknown Qt event {other}"),
            }
        }
        assert_eq!(snapshot(&editor, &rows), step["controls"], "{step}");
        let (after, _, errors) = editor.applied();
        assert!(errors.is_empty(), "{errors:?}");
        assert_eq!(
            serde_json::json!([
                after.gui_idle.user_seconds,
                after.gui_idle.mouse_seconds,
                after.gui_idle.api_seconds
            ]),
            step["saved"]
        );
    }
    assert_eq!(
        store.read(Settings::load).unwrap(),
        before,
        "draft/Cancel leaves store untouched"
    );
    let (mut disabled, _, _) = editor.applied();
    disabled.gui_idle.enabled = false;
    let (disabled, disabled_rows) = self::editor(disabled);
    assert_eq!(snapshot(&disabled, &disabled_rows), fixture["disabled"]);
    for &row in &rows {
        editor.none(row, true);
    }
    let (after, _, _) = editor.applied();
    store
        .write(move |ctx| after.save(ctx.conn(), &before))
        .unwrap();
    let reopened = Store::open(directory.path()).unwrap();
    let (editor, rows) = self::editor(reopened.read(Settings::load).unwrap());
    let shown = snapshot(&editor, &rows);
    for (native, reference) in shown
        .as_array()
        .unwrap()
        .iter()
        .zip(fixture["reopened_none"].as_array().unwrap())
    {
        for field in ["number", "none", "seconds"] {
            assert_eq!(native[field], reference[field]);
        }
    }
}
#[test]
fn edited_idle_thresholds_merge_without_overwriting_live_unrelated_preferences() {
    let directory = tempfile::tempdir().unwrap();
    let store = Store::open(directory.path()).unwrap();
    let before = store.read(Settings::load).unwrap();
    let (mut editor, rows) = editor(before.clone());
    editor.number(rows[0], 1);
    let (after, _, errors) = editor.applied();
    assert!(errors.is_empty());
    store
        .write(|ctx| {
            settings::set(
                ctx.conn(),
                &GuiIdleSettings {
                    enabled: false,
                    user_seconds: Some(1800),
                    mouse_seconds: Some(3000),
                    api_seconds: Some(900),
                    ..GuiIdleSettings::default()
                },
            )
        })
        .unwrap();
    store
        .write(move |ctx| after.save(ctx.conn(), &before))
        .unwrap();
    assert_eq!(
        store.read(settings::get::<GuiIdleSettings>).unwrap(),
        GuiIdleSettings {
            enabled: false,
            user_seconds: Some(60),
            mouse_seconds: Some(3000),
            api_seconds: Some(900),
            ..GuiIdleSettings::default()
        }
    );
}
#[test]
fn saved_timeout_choices_replay_strict_qt_boot_activity_and_ignore_gates() {
    let fixture = hydrus_testkit::fixture_json("idle_timeout_options.json");
    for step in fixture["gates"].as_array().unwrap() {
        let values = &step["values"];
        let settings = GuiIdleSettings {
            enabled: step["enabled"].as_bool().unwrap(),
            user_seconds: values[0].as_u64(),
            mouse_seconds: values[1].as_u64(),
            api_seconds: values[2].as_u64(),
            ..GuiIdleSettings::default()
        };
        let mut idle = Idle::new(0);
        let times = &step["times"];
        idle.user(times["last_user_action"].as_i64().unwrap());
        idle.mouse(times["last_mouse_action"].as_i64().unwrap());
        idle.api(times["last_client_api_action"].as_i64().unwrap());
        assert_eq!(
            idle.eligible(step["now"].as_i64().unwrap(), &settings),
            step["eligible"].as_bool().unwrap(),
            "{step}"
        );
    }
}

#[test]
fn raw_imported_seconds_normalise_only_on_unchanged_acceptance_as_qt_does() {
    let fixture = hydrus_testkit::fixture_json("idle_timeout_constructors.json");
    for case in fixture["cases"].as_array().unwrap() {
        let directory = tempfile::tempdir().unwrap();
        let store = Store::open(directory.path()).unwrap();
        let raw = case["raw"].as_u64();
        store
            .write(move |ctx| {
                settings::set(
                    ctx.conn(),
                    &GuiIdleSettings {
                        user_seconds: raw,
                        mouse_seconds: raw,
                        api_seconds: raw,
                        ..GuiIdleSettings::default()
                    },
                )
            })
            .unwrap();
        let before = store.read(Settings::load).unwrap();
        let (draft, rows) = editor(before.clone());
        let shown = snapshot(&draft, &rows);
        for (shown, expected) in shown
            .as_array()
            .unwrap()
            .iter()
            .zip(case["controls"].as_array().unwrap())
        {
            for field in ["number", "none", "seconds"] {
                assert_eq!(shown[field], expected[field]);
            }
        }
        drop(draft);
        let cancelled = store.read(settings::get::<GuiIdleSettings>).unwrap();
        assert_eq!(
            serde_json::json!([
                cancelled.user_seconds,
                cancelled.mouse_seconds,
                cancelled.api_seconds
            ]),
            case["abandoned"]
        );
        let (draft, _) = editor(before.clone());
        let (after, _, errors) = draft.applied();
        assert!(errors.is_empty());
        assert_eq!(
            serde_json::json!([
                after.gui_idle.user_seconds,
                after.gui_idle.mouse_seconds,
                after.gui_idle.api_seconds
            ]),
            case["accepted"]
        );
        assert_eq!(
            store.read(Settings::load).unwrap().gui_idle,
            before.gui_idle
        );
        store
            .write(move |ctx| after.save(ctx.conn(), &before))
            .unwrap();
        let reopened = Store::open(directory.path()).unwrap();
        let saved = reopened.read(settings::get::<GuiIdleSettings>).unwrap();
        assert_eq!(
            serde_json::json!([saved.user_seconds, saved.mouse_seconds, saved.api_seconds]),
            case["accepted"]
        );
    }
}

#[test]
fn implicit_idle_normalisation_preserves_newer_fields_and_explicit_edits_still_apply() {
    for explicit in [false, true] {
        let directory = tempfile::tempdir().unwrap();
        let store = Store::open(directory.path()).unwrap();
        store
            .write(|ctx| {
                settings::set(
                    ctx.conn(),
                    &GuiIdleSettings {
                        enabled: true,
                        user_seconds: Some(119),
                        mouse_seconds: Some(0),
                        api_seconds: Some(60060),
                        ..GuiIdleSettings::default()
                    },
                )
            })
            .unwrap();
        let before = store.read(Settings::load).unwrap();
        let (mut draft, rows) = editor(before.clone());
        if explicit {
            draft.number(rows[0], 2);
        }
        let (after, _, errors) = draft.applied();
        assert!(errors.is_empty());
        store
            .write(|ctx| {
                settings::set(
                    ctx.conn(),
                    &GuiIdleSettings {
                        enabled: false,
                        user_seconds: Some(300),
                        mouse_seconds: Some(0),
                        api_seconds: Some(777),
                        ..GuiIdleSettings::default()
                    },
                )
            })
            .unwrap();
        store
            .write(move |ctx| after.save(ctx.conn(), &before))
            .unwrap();
        assert_eq!(
            store.read(settings::get::<GuiIdleSettings>).unwrap(),
            GuiIdleSettings {
                enabled: false,
                user_seconds: Some(if explicit { 120 } else { 300 }),
                mouse_seconds: Some(60),
                api_seconds: Some(777),
                ..GuiIdleSettings::default()
            }
        );
    }
}
