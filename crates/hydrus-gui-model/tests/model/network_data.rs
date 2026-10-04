//! Qt fixture replay and durable, isolated bandwidth edits.
use hydrus_core::{
    bandwidth::{BandwidthType, Rule, Rules, Tracker},
    network::NetworkContext,
};
use hydrus_gui_model::network_data::{self as model, Review, RulesDraft};
use hydrus_store::{
    Store,
    bandwidth::BandwidthSettings,
    network_runtime::{NetworkJob, Snapshot, WaitReason},
    settings,
};

#[test]
fn replay_qt_bandwidth_rows_rules_and_questions() {
    let f = hydrus_testkit::fixture_json("network_data.json");
    let now = f["now"].as_i64().unwrap();
    let rules = Rules::new(f["rule_values"].as_array().unwrap().iter().map(|r| {
        Rule::new(
            BandwidthType::from_code(r[0].as_i64().unwrap()).unwrap(),
            r[1].as_u64(),
            r[2].as_u64().unwrap(),
        )
    }));
    let mut tracker = Tracker::new(now);
    tracker.report_data(2048, now);
    tracker.report_requests(1, now);
    let contexts = [
        NetworkContext::global(),
        NetworkContext::domain("example.com"),
        NetworkContext::domain("empty.example"),
    ];
    let review = Review {
        settings: BandwidthSettings {
            rules: vec![
                (contexts[0].clone(), rules.clone()),
                (contexts[1].clone(), rules.clone()),
            ],
            ..BandwidthSettings::default()
        },
        runtime: Snapshot::default(),
        usage: vec![(contexts[1].clone(), tracker)],
        live: true,
    };
    for (i, c) in contexts.iter().enumerate() {
        assert_eq!(
            serde_json::to_value(review.row(c, Some(604_800), now)).unwrap(),
            f["usage_rows"][i]
        );
    }
    for (i, r) in rules.rules().iter().enumerate() {
        assert_eq!(
            serde_json::to_value(model::rule_row(*r)).unwrap(),
            f["rule_rows"][i]
        );
    }
    assert_eq!(model::REVERT_QUESTION, f["questions"][0]);
    assert_eq!(model::RESET_QUESTION, f["questions"][1]);
    let job = NetworkJob {
        id: 1,
        url: "https://example.com/file".into(),
        status: "downloading…".into(),
        wait: WaitReason::Downloading,
        speed: 2048,
        bytes_read: 2048,
        bytes_total: Some(4096),
        contexts: Vec::new(),
        obeys_bandwidth: true,
    };
    assert_eq!(
        &serde_json::to_value(model::job_row(&job))
            .unwrap()
            .as_array()
            .unwrap()[1..],
        &f["job_rows"][4].as_array().unwrap()[1..]
    );
}

#[test]
fn drafts_preserve_unrelated_edits_and_reject_changed_defaults() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let context = NetworkContext::domain("example.com");
    let review = Review::load(&store, 100).unwrap();
    let mut draft = RulesDraft::new(&review, context.clone());
    draft.rules = vec![model::parse_rule(true, "10", "60", false).unwrap()];
    assert!(model::parse_rule(true, "0", "60", false).is_err());
    assert!(model::parse_rule(false, "8", "0", false).is_err());
    assert!(Review::load(&store, 100).unwrap().inherits(&context));
    store
        .write(|ctx| {
            let mut settings = settings::get::<BandwidthSettings>(ctx.conn())?;
            settings.watcher_page_wait = 17;
            settings::set(ctx.conn(), &settings)
        })
        .unwrap();
    draft.apply(&store, false).unwrap();
    let fresh = Review::load(&store, 100).unwrap();
    assert!(!fresh.inherits(&context));
    assert_eq!(fresh.settings.watcher_page_wait, 17);
    let stale_changed = RulesDraft::new(&fresh, context.clone());
    let mut concurrent = RulesDraft::new(&fresh, context.clone());
    concurrent.rules.clear();
    concurrent.apply(&store, false).unwrap();
    assert!(stale_changed.apply(&store, false).is_err());
    assert!(Review::load(&store, 100).unwrap().inherits(&context));
    let mut fresh_draft = RulesDraft::new(&Review::load(&store, 100).unwrap(), context.clone());
    fresh_draft.rules = vec![Rule::new(BandwidthType::Data, None, 123)];
    fresh_draft.apply(&store, false).unwrap();
    model::reset_defaults(&store).unwrap();
    assert_eq!(
        Review::load(&store, 100).unwrap().rules(&context).rules(),
        fresh_draft.rules
    );
    drop(store);
    let store = Store::open(dir.path()).unwrap();
    let reopened = Review::load(&store, 100).unwrap();
    RulesDraft::new(&reopened, context.clone())
        .apply(&store, true)
        .unwrap();
    assert!(Review::load(&store, 100).unwrap().inherits(&context));
    let stale = RulesDraft::new(&Review::load(&store, 100).unwrap(), context);
    let mut default = RulesDraft::new(
        &Review::load(&store, 100).unwrap(),
        NetworkContext::default_of_kind(2),
    );
    default.rules.clear();
    default.apply(&store, false).unwrap();
    assert!(stale.apply(&store, false).is_err());
}

#[test]
fn fresh_live_usage_wins_until_the_daemon_heartbeat_expires() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let mut tracker = Tracker::new(100);
    tracker.report_data(2048, 100);
    store
        .write(move |ctx| {
            settings::set(
                ctx.conn(),
                &Snapshot {
                    epoch: "daemon".into(),
                    at: 100,
                    usage: vec![(NetworkContext::global(), tracker)],
                    jobs: Vec::new(),
                },
            )
        })
        .unwrap();
    assert_eq!(
        Review::load(&store, 105)
            .unwrap()
            .row(&NetworkContext::global(), None, 105)[4],
        "2 KB in 0 requests"
    );
    assert!(!Review::load(&store, 106).unwrap().live);
    assert_eq!(
        Review::load(&store, 106)
            .unwrap()
            .row(&NetworkContext::global(), None, 106)[4],
        "0B in 0 requests"
    );
}

#[test]
fn reference_bandwidth_age_rule_filters_months_and_history_deletion() {
    use hydrus_store::bandwidth::{self, HistoryResets};
    use std::collections::BTreeMap;
    let f = hydrus_testkit::fixture_json("bandwidth_history.json");
    let now = f["now"].as_i64().unwrap();
    let context = |name: &str| {
        if name == "page" {
            NetworkContext::downloader_page("91".repeat(16))
        } else {
            NetworkContext::domain(format!("{name}.example.com"))
        }
    };
    let mut trackers = BTreeMap::new();
    for event in f["events"].as_array().unwrap() {
        let at = event[0].as_i64().unwrap();
        let name = event[1].as_str().unwrap();
        let tracker = trackers
            .entry(context(name))
            .or_insert_with(|| Tracker::new(at));
        tracker.report_data(event[2].as_u64().unwrap(), at);
        tracker.report_requests(event[3].as_u64().unwrap(), at);
    }
    let rules = Rules::new([Rule::new(BandwidthType::Data, Some(86400), 100_000)]);
    let review = Review {
        settings: BandwidthSettings {
            rules: vec![(context("rules"), rules.clone())],
            ..BandwidthSettings::default()
        },
        runtime: Snapshot::default(),
        usage: trackers.into_iter().collect(),
        live: false,
    };
    for filter in f["filters"].as_array().unwrap() {
        let mut visible = review
            .filtered_contexts(
                filter["span"].as_u64(),
                filter["include_rules"].as_bool().unwrap(),
                now,
            )
            .iter()
            .map(NetworkContext::to_human_string)
            .collect::<Vec<_>>();
        visible.sort();
        assert_eq!(serde_json::json!(visible), filter["contexts"]);
    }
    let mut live_sort = review.clone();
    live_sort.live = true;
    for name in ["recent", "old", "bytes"] {
        let recorded = &f["sort_rows"][name];
        assert_eq!(
            live_sort.sort_key(&context(name), 2, None, now),
            model::SortKey::Counts(recorded[2].as_u64().unwrap(), 0)
        );
        for column in [3, 4, 5] {
            assert_eq!(
                live_sort.sort_key(&context(name), column, None, now),
                model::SortKey::Counts(
                    recorded[column][0].as_u64().unwrap(),
                    recorded[column][1].as_u64().unwrap()
                )
            );
        }
    }
    let months = model::monthly_history(&review.tracker(&context("recent"), now));
    assert_eq!(
        serde_json::json!(
            months
                .iter()
                .map(|m| (&m.month, m.bytes))
                .collect::<Vec<_>>()
        ),
        f["months"]
    );
    assert!(months.windows(2).all(|pair| pair[0].month < pair[1].month));
    assert!((months[2].fraction - 1.0 / 1.2).abs() < 0.0001);
    assert_eq!(model::DELETE_HISTORY_QUESTION, f["questions"][0]);
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let usage = review.usage.clone();
    let options = review.settings.clone();
    store
        .write(move |ctx| {
            bandwidth::save_usage(ctx.conn(), &usage)?;
            settings::set(ctx.conn(), &options)
        })
        .unwrap();
    let original = store.read(|c| bandwidth::usage(c, now)).unwrap();
    let delete = vec![context("recent"), context("rules")];
    store
        .write(move |ctx| bandwidth::delete_history(ctx.conn(), &delete))
        .unwrap();
    // A delayed pre-delete engine flush cannot restore the deleted usage.
    store
        .write(move |ctx| {
            bandwidth::save_usage_after_resets(ctx.conn(), &original, &HistoryResets::default())
        })
        .unwrap();
    let fresh = Review::load(&store, now).unwrap();
    assert!(model::monthly_history(&fresh.tracker(&context("recent"), now)).is_empty());
    assert_eq!(fresh.rules(&context("rules")), rules);
    assert!(fresh.usage.iter().any(|(c, _)| c == &context("old")));
    let mut remaining = fresh
        .filtered_contexts(None, false, now)
        .iter()
        .map(NetworkContext::to_human_string)
        .collect::<Vec<_>>();
    remaining.sort();
    assert_eq!(serde_json::json!(remaining), f["contexts_after_delete"]);
    let revisions = store.read(settings::get::<HistoryResets>).unwrap();
    assert_eq!(revisions.generation(&context("recent")), 1);
    drop(store);
    let store = Store::open(dir.path()).unwrap();
    assert_eq!(
        store.read(settings::get::<HistoryResets>).unwrap(),
        revisions
    );
    assert!(
        model::monthly_history(
            &Review::load(&store, now)
                .unwrap()
                .tracker(&context("recent"), now)
        )
        .is_empty()
    );
}
