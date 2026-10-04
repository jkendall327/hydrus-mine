//! Real domain memories produce the recorded sampled chart and range behavior.
use hydrus_core::search::context::FileSearchContext;
use hydrus_gui_model::file_history::{self as model, Chart};
use hydrus_store::Store;
use serde_json::{Value, json};
use std::sync::atomic::AtomicBool;
#[path = "../support/file_history_seed.rs"]
mod seeding;
fn chart_state(chart: &Chart) -> Value {
    json!({"visible":chart.visible,"x":[chart.x.0,chart.x.1],"y":[chart.y.0,chart.y.1],"custom":[chart.custom_x,chart.custom_y]})
}
fn assert_state(chart: &Chart, row: &Value) {
    let actual = chart_state(chart);
    for key in ["visible", "x", "y", "custom"] {
        assert_eq!(actual[key], row[key], "{} {key}", row["case"]);
    }
}
#[test]
fn global_and_filtered_series_ranges_visibility_refresh_and_cancel_match_actual_chart() {
    let recorded = hydrus_testkit::fixture_json("file_history.json");
    let legacy = hydrus_testkit::legacy_fixture("basic");
    let native = tempfile::tempdir().unwrap();
    hydrus_store::import::import_legacy(
        legacy.path(),
        &native.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store = Store::open(native.path()).unwrap();
    seeding::seed(&store, &recorded);
    let context = FileSearchContext::default();
    let history = model::load(&store, &context, 8, &AtomicBool::new(false)).unwrap();
    assert_eq!(json!(history), recorded["compact"]);
    let history = model::load(&store, &context, 7680, &AtomicBool::new(false)).unwrap();
    let mut chart = Chart::default();
    chart.publish(history.clone());
    assert_state(&chart, &recorded["events"][1]);
    for (name, rows) in ["current", "inbox", "archive", "deleted"]
        .into_iter()
        .zip(history.series())
    {
        let summary = &recorded["events"][1]["history"][name];
        assert_eq!(json!(rows.len()), summary["length"]);
        assert_eq!(json!(rows.first()), summary["first"]);
        assert_eq!(json!(rows.last()), summary["last"]);
    }
    chart.toggle(0);
    assert_state(&chart, &recorded["events"][2]);
    chart.range_y(2, 12).unwrap();
    chart
        .range_x(
            model::parse_date("2024-01-02").unwrap(),
            model::parse_date("2024-01-05").unwrap(),
        )
        .unwrap();
    assert_state(&chart, &recorded["events"][3]);
    chart.toggle(1);
    assert_state(&chart, &recorded["events"][4]);
    chart.publish(history);
    assert_state(&chart, &recorded["events"][5]);
    chart.refit_x();
    chart.refit_y();
    assert_state(&chart, &recorded["events"][6]);
    assert!(chart.paths()[0].is_empty());
    assert!(chart.paths()[1].is_empty());
    assert!(!chart.paths()[2].is_empty());
    let filtered = FileSearchContext {
        predicates: hydrus_search::parse_api_search(&json!([recorded["filter_tag"]])).unwrap(),
        ..context.clone()
    };
    assert_eq!(
        json!(model::load(&store, &filtered, 8, &AtomicBool::new(false)).unwrap()),
        recorded["filtered_compact"]
    );
    assert_eq!(
        model::load(&store, &context, 8, &AtomicBool::new(true))
            .unwrap_err()
            .to_string(),
        "Cancelled!"
    );
    assert!(model::load(&store, &context, 0, &AtomicBool::new(false)).is_err());
    let before = chart_state(&chart);
    assert!(chart.range_y(12, 2).is_err());
    assert!(chart.range_x(2, 1).is_err());
    assert_eq!(chart_state(&chart), before);
    let complex = FileSearchContext {
        location: hydrus_core::search::context::LocationContext::new(
            context.location.current().iter().cloned(),
            context.location.current().iter().cloned(),
        ),
        ..context
    };
    assert_eq!(
        model::load(&store, &complex, 8, &AtomicBool::new(false))
            .unwrap_err()
            .to_string(),
        model::COMPLEX_DOMAIN
    );
}
