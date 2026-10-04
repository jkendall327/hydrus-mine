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
