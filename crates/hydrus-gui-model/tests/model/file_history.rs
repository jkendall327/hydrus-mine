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
    store
        .write(|ctx| {
            let mut prefs = hydrus_store::settings::get::<
                hydrus_store::settings::FileSearchSettings,
            >(ctx.conn())?;
            prefs.implicit_limit = Some(1);
            hydrus_store::settings::set(ctx.conn(), &prefs)
        })
        .unwrap();
    let ordinary = store
        .read(|conn| {
            Ok(hydrus_search::search_files(
                conn,
                &store.snapshot(),
                &filtered,
                hydrus_search::FileSort::default(),
                &hydrus_search::Clock::system(),
            )
            .unwrap())
        })
        .unwrap();
    assert_eq!(ordinary.len(), 1);
    assert_eq!(
        json!(model::load(&store, &filtered, 8, &AtomicBool::new(false)).unwrap()),
        recorded["filtered_compact"]
    );
    assert_eq!(
        json!(model::load(&store, &filtered, 8, &AtomicBool::new(false)).unwrap()),
        recorded["implicit_limit_case"]["history"]
    );
    let explicit = FileSearchContext {
        predicates: vec![hydrus_search::Predicate::System(
            hydrus_search::SystemPredicate::Limit(0),
        )],
        ..context.clone()
    };
    assert!(
        model::load(&store, &explicit, 8, &AtomicBool::new(false))
            .unwrap()
            .current
            .is_empty()
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

#[test]
fn owned_worker_runs_only_latest_queued_query_and_discards_cancelled_closed_results() {
    use hydrus_gui_model::file_history_worker::Worker;
    use hydrus_search::{Predicate, SystemPredicate};
    use hydrus_store::file_history::History;
    use std::{
        sync::{
            Arc, Mutex,
            atomic::{AtomicUsize, Ordering},
            mpsc,
        },
        time::{Duration, Instant},
    };
    let starts = Arc::new(AtomicUsize::new(0));
    let calls = Arc::new(Mutex::new(Vec::new()));
    let (entered, enter) = mpsc::channel();
    let (release, wait) = mpsc::channel();
    let (exited, exit) = mpsc::channel();
    let mut worker = Worker::start_with(
        {
            let calls = calls.clone();
            move |context, cancel| {
                let [Predicate::System(SystemPredicate::Limit(id))] = context.predicates.as_slice()
                else {
                    panic!("typed query marker");
                };
                let id = *id;
                calls
                    .lock()
                    .unwrap()
                    .push((id, std::thread::current().id()));
                entered.send(id).unwrap();
                if id == 1 || id == 101 {
                    wait.recv_timeout(Duration::from_secs(10)).unwrap();
                }
                // Even an executor that finishes after cancellation cannot publish.
                if id == 1 || id == 101 {
                    assert!(cancel.load(Ordering::Acquire));
                }
                Ok(History {
                    current: vec![(i64::try_from(id).unwrap(), 1)],
                    ..History::default()
                })
            }
        },
        {
            let starts = starts.clone();
            move |task| {
                starts.fetch_add(1, Ordering::SeqCst);
                std::thread::Builder::new()
                    .spawn(move || {
                        task();
                        exited.send(()).unwrap();
                    })
                    .map(drop)
            }
        },
    )
    .unwrap();
    let context = |id| FileSearchContext {
        predicates: vec![Predicate::System(SystemPredicate::Limit(id))],
        ..FileSearchContext::default()
    };
    worker.submit(context(1)).unwrap();
    assert_eq!(enter.recv_timeout(Duration::from_secs(10)).unwrap(), 1);
    for id in 2..=100 {
        worker.submit(context(id)).unwrap();
    }
    release.send(()).unwrap();
    assert_eq!(enter.recv_timeout(Duration::from_secs(10)).unwrap(), 100);
    let deadline = Instant::now() + Duration::from_secs(10);
    let published = loop {
        if let Some(result) = worker.poll() {
            break result.unwrap();
        }
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(1));
    };
    assert_eq!(published.current, [(100, 1)]);
    assert_eq!(starts.load(Ordering::SeqCst), 1);
    worker.submit(context(101)).unwrap();
    assert_eq!(enter.recv_timeout(Duration::from_secs(10)).unwrap(), 101);
    worker.submit(context(102)).unwrap();
    worker.close();
    assert!(worker.submit(context(103)).is_err());
    release.send(()).unwrap();
    exit.recv_timeout(Duration::from_secs(10)).unwrap();
    assert!(worker.poll().is_none());
    let calls = calls.lock().unwrap();
    assert_eq!(
        calls.iter().map(|(id, _)| *id).collect::<Vec<_>>(),
        [1, 100, 101]
    );
    assert!(calls.iter().all(|(_, thread)| *thread == calls[0].1));
}

#[test]
fn dropping_worker_cancels_inflight_read_and_never_runs_pending_read() {
    use hydrus_gui_model::file_history_worker::Worker;
    use hydrus_store::file_history::History;
    use std::{
        sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
            mpsc,
        },
        time::Duration,
    };
    let calls = Arc::new(AtomicUsize::new(0));
    let (entered, enter) = mpsc::channel();
    let (release, wait) = mpsc::channel();
    let (exited, exit) = mpsc::channel();
    let mut worker = Worker::start_with(
        {
            let calls = calls.clone();
            move |_, cancel| {
                calls.fetch_add(1, Ordering::SeqCst);
                entered.send(()).unwrap();
                wait.recv_timeout(Duration::from_secs(10)).unwrap();
                assert!(cancel.load(Ordering::Acquire));
                Ok(History::default())
            }
        },
        move |task| {
            std::thread::Builder::new()
                .spawn(move || {
                    task();
                    exited.send(()).unwrap();
                })
                .map(drop)
        },
    )
    .unwrap();
    worker.submit(FileSearchContext::default()).unwrap();
    enter.recv_timeout(Duration::from_secs(10)).unwrap();
    worker.submit(FileSearchContext::default()).unwrap();
    drop(worker);
    release.send(()).unwrap();
    exit.recv_timeout(Duration::from_secs(10)).unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}
