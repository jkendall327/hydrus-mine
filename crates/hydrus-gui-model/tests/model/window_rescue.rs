//! Replay real Qt rescue settings and explicit monitor-topology decisions.
use hydrus_gui_model::{
    options::{Editor, Row, Settings},
    window_rescue::{self, Rect, Screen},
};
use hydrus_store::{
    Store,
    settings::{self, WindowRescueSettings},
};
use serde_json::{Value, json};
const LABELS: [&str; 3] = [
    "BUGFIX: Disable off-screen window rescue: ",
    "When rescuing, add top-left safety padding:",
    "DEBUG: top-left padding to use (px): ",
];
fn value(p: &WindowRescueSettings) -> Value {
    json!({"disabled":p.disabled,"add_padding":p.add_padding,"padding":p.padding})
}
fn screens(value: &Value) -> Vec<Screen> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|s| {
            let g = &s["geometry"];
            let a = &s["available"];
            Screen {
                geometry: Rect {
                    x: g[0].as_i64().unwrap(),
                    y: g[1].as_i64().unwrap(),
                    width: g[2].as_i64().unwrap(),
                    height: g[3].as_i64().unwrap(),
                },
                available_top_left: (a[0].as_i64().unwrap(), a[1].as_i64().unwrap()),
            }
        })
        .collect()
}
#[test]
fn staged_bounds_import_cancel_persistence_and_reopen_match_actual_qt() {
    let fixture = hydrus_testkit::fixture_json("window_rescue.json");
    let legacy = hydrus_testkit::legacy_fixture("basic");
    let dir = tempfile::tempdir().unwrap();
    hydrus_store::import::import_legacy(
        legacy.path(),
        &dir.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store = Store::open(dir.path()).unwrap();
    assert_eq!(
        value(&store.read(settings::get).unwrap()),
        fixture["defaults"]
    );
    for event in fixture["stages"].as_array().unwrap() {
        let before = store.read(Settings::load).unwrap();
        let mut editor = Editor::new(before.clone());
        let page = editor
            .page_names()
            .iter()
            .position(|name| *name == "gui")
            .unwrap();
        editor.show_page(page);
        let rows: Vec<_> = LABELS
            .iter()
            .map(|label| {
                editor
                    .rows()
                    .iter()
                    .position(|row| matches!(row,Row::Opt{option,..} if option.label==*label))
                    .unwrap()
            })
            .collect();
        editor.check(rows[0], event["input"][0].as_bool().unwrap());
        editor.check(rows[1], event["input"][1].as_bool().unwrap());
        editor.number(rows[2], event["input"][2].as_i64().unwrap());
        let (after, _, errors) = editor.applied();
        assert!(errors.is_empty());
        assert_eq!(value(&after.window_rescue), event["draft"]);
        assert_eq!(store.read(Settings::load).unwrap(), before);
        let cancelled = Editor::new(store.read(Settings::load).unwrap());
        assert_eq!(value(&cancelled.applied().0.window_rescue), event["before"]);
        store
            .write(move |ctx| after.save(ctx.conn(), &before))
            .unwrap();
        let reopened = Store::open(dir.path()).unwrap();
        assert_eq!(
            value(&reopened.read(settings::get).unwrap()),
            event["reopened"]
        );
    }
    assert_eq!(
        value(&store.read(settings::get).unwrap()),
        fixture["cancel_after"]
    );
    assert_eq!(
        serde_json::from_str::<WindowRescueSettings>("{}").unwrap(),
        WindowRescueSettings::default()
    );
}
#[test]
fn actual_visibility_corner_order_fuzzy_tests_and_fallback_match_qt() {
    let fixture = hydrus_testkit::fixture_json("window_rescue.json");
    for case in fixture["cases"].as_array().unwrap() {
        let monitors = screens(
            &fixture[if case["environment"] == "synthetic_topology" {
                "topology"
            } else {
                "actual_screens"
            }],
        );
        let settings: WindowRescueSettings =
            serde_json::from_value(case["settings"].clone()).unwrap();
        let point = (
            case["position"][0].as_i64().unwrap(),
            case["position"][1].as_i64().unwrap(),
        );
        let size = case["size"]
            .as_array()
            .map(|s| (s[0].as_i64().unwrap(), s[1].as_i64().unwrap()));
        let result = window_rescue::safe_position(point, size, &settings, &monitors);
        assert_eq!(json!(result.map(|(x, y)| [x, y])), case["result"], "{case}");
        assert_eq!(
            result != Some(point),
            case["rescued"].as_bool().unwrap(),
            "{case}"
        );
    }
    assert_eq!(
        window_rescue::safe_position((-9, -9), None, &WindowRescueSettings::default(), &[]),
        None
    );
    let disabled = WindowRescueSettings {
        disabled: true,
        ..WindowRescueSettings::default()
    };
    assert_eq!(
        window_rescue::safe_position((-9, -9), None, &disabled, &[]),
        Some((-9, -9))
    );
}
