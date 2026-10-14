//! Rendered dialogs, detached cancellation and local daemon command boundaries.
use hydrus_core::{
    bandwidth::{BandwidthType, Tracker},
    network::NetworkContext,
    time::TimestampMs,
};
use hydrus_gui::{headless, network_data_window as windows};
use hydrus_store::{
    Store,
    network_runtime::{self, NetworkJob, Snapshot, WaitReason},
    settings,
};
use slint::{ComponentHandle as _, Model as _};

fn until(mut condition: impl FnMut() -> bool) {
    let start = std::time::Instant::now();
    while !condition() {
        assert!(start.elapsed() < std::time::Duration::from_secs(8));
        slint::platform::update_timers_and_animations();
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}
fn capture(windows: &headless::Windows, index: usize, name: &str, width: u32, height: u32) {
    let pixels = headless::render(&windows.get(index).unwrap(), width, height);
    let directory = std::env::var_os("HYDRUS_NETWORK_SCREENSHOTS").map_or_else(
        || std::path::PathBuf::from(env!("CARGO_TARGET_TMPDIR")),
        std::path::PathBuf::from,
    );
    std::fs::create_dir_all(&directory).unwrap();
    headless::save_png(&directory.join(name), &pixels, width, height).unwrap();
}

// leaf: audit-network-bandwidth-domain
// leaf: audit-network-rules-commit
#[test]
fn bandwidth_rules_cancel_apply_reopen_and_render() {
    let headless = headless::init();
    let slots = windows::Slots::default();
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let now = TimestampMs::now().millis() / 1000;
    let mut tracker = Tracker::new(now);
    tracker.report_data(2048, now);
    tracker.report_requests(1, now);
    store
        .write(move |ctx| {
            hydrus_store::bandwidth::save_usage(
                ctx.conn(),
                &[(NetworkContext::domain("example.com"), tracker)],
            )
        })
        .unwrap();
    let review = windows::open_bandwidth(store.clone(), &slots).unwrap();
    until(|| review.get_rows().row_count() == 1);
    assert!(review.get_status().contains("offline"));
    review.set_domain("example.com".into());
    review.invoke_domain_clicked();
    let edit = windows::last_rules().unwrap();
    let before = store
        .read(settings::get::<hydrus_store::bandwidth::BandwidthSettings>)
        .unwrap();
    edit.set_amount("0".into());
    edit.invoke_add_rule();
    assert!(edit.get_error().contains("positive"));
    edit.set_requests(true);
    edit.set_amount("23".into());
    edit.set_seconds("60".into());
    edit.set_monthly(false);
    edit.invoke_add_rule();
    assert_eq!(edit.get_rows().row_count(), 3);
    edit.invoke_cancel_clicked();
    assert_eq!(
        store
            .read(settings::get::<hydrus_store::bandwidth::BandwidthSettings>)
            .unwrap(),
        before
    );
    review.invoke_domain_clicked();
    let edit = windows::last_rules().unwrap();
    edit.set_requests(true);
    edit.set_amount("23".into());
    edit.set_seconds("60".into());
    edit.invoke_add_rule();
    capture(
        &headless,
        headless.count() - 1,
        "network_bandwidth_rules.png",
        670,
        460,
    );
    edit.invoke_apply_clicked();
    until(|| windows::last_rules().is_none());
    let saved = store
        .read(settings::get::<hydrus_store::bandwidth::BandwidthSettings>)
        .unwrap();
    let rules = saved
        .rules
        .iter()
        .find(|(c, _)| c == &NetworkContext::domain("example.com"))
        .unwrap()
        .1
        .rules();
    assert!(
        rules
            .iter()
            .any(|r| r.kind == BandwidthType::Requests && r.max_allowed == 23)
    );
    let row = review
        .get_rows()
        .iter()
        .position(|r| r.cells.row_data(0).unwrap().contains("example.com"))
        .unwrap();
    review.invoke_row_clicked(i32::try_from(row).unwrap(), false, false);
    until(|| review.get_can_revert());
    capture(&headless, 0, "network_bandwidth_review.png", 1120, 720);
    review.invoke_revert_clicked();
    assert!(review.get_question().contains("default rules"));
    review.invoke_answer(false);
    assert_eq!(
        store
            .read(settings::get::<hydrus_store::bandwidth::BandwidthSettings>)
            .unwrap(),
        saved
    );
    review.invoke_edit_clicked();
    let abandoned = windows::last_rules().unwrap();
    review.invoke_close_clicked();
    assert!(!abandoned.window().is_visible());
    abandoned.invoke_apply_clicked();
    let reopened = windows::open_bandwidth(store.clone(), &slots).unwrap();
    until(|| reopened.get_rows().row_count() == 1);
    assert_eq!(
        store
            .read(settings::get::<hydrus_store::bandwidth::BandwidthSettings>)
            .unwrap(),
        saved
    );
    reopened.set_domain("example.com".into());
    reopened.invoke_domain_clicked();
    let stale = windows::last_rules().unwrap();
    store
        .write(|ctx| {
            let mut current =
                settings::get::<hydrus_store::bandwidth::BandwidthSettings>(ctx.conn())?;
            current
                .rules
                .retain(|(c, _)| c != &NetworkContext::domain("example.com"));
            current.rules.push((
                NetworkContext::domain("example.com"),
                hydrus_core::bandwidth::Rules::new([hydrus_core::bandwidth::Rule::new(
                    BandwidthType::Requests,
                    Some(60),
                    31,
                )]),
            ));
            settings::set(ctx.conn(), &current)
        })
        .unwrap();
    stale.invoke_apply_clicked();
    until(|| !stale.get_busy());
    assert!(stale.get_error().contains("changed in another editor"));
    assert!(reopened.get_status().contains("changed in another editor"));
    reopened.invoke_close_clicked();
}

// leaf: audit-network-jobs-cancel
// leaf: network-job-actions
// leaf: audit-network-jobs-refresh
// leaf: audit-network-jobs-expiry
// leaf: audit-network-jobs-override
#[test]
fn jobs_live_progress_commands_and_offline_expiry() {
    let headless = headless::init();
    let slots = windows::Slots::default();
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let now = TimestampMs::now().millis() / 1000;
    store
        .write(move |ctx| {
            settings::set(
                ctx.conn(),
                &Snapshot {
                    epoch: "test daemon".into(),
                    at: now,
                    usage: Vec::new(),
                    jobs: vec![NetworkJob {
                        id: 42,
                        url: "https://example.com/file".into(),
                        status: "downloading…".into(),
                        wait: WaitReason::Downloading,
                        speed: 2048,
                        bytes_read: 2048,
                        bytes_total: Some(4096),
                        contexts: vec![
                            NetworkContext::global(),
                            NetworkContext::domain("example.com"),
                        ],
                        obeys_bandwidth: true,
                    }],
                    ..Snapshot::default()
                },
            )
        })
        .unwrap();
    let jobs = windows::open_jobs(store.clone(), &slots).unwrap();
    until(|| jobs.get_rows().row_count() == 1);
    jobs.invoke_row_clicked(0, false, false);
    assert!(jobs.get_can_act());
    assert!(jobs.get_detail().contains("example.com"));
    assert_eq!(
        jobs.get_rows()
            .row_data(0)
            .unwrap()
            .cells
            .row_data(4)
            .unwrap(),
        "2 KB/4 KB"
    );
    capture(&headless, 0, "network_current_jobs.png", 1100, 590);
    jobs.invoke_override_jobs();
    jobs.invoke_cancel_jobs();
    let mut commands = Vec::new();
    until(|| {
        commands.extend(
            store
                .write(|ctx| network_runtime::take_commands(ctx.conn()))
                .unwrap(),
        );
        commands.len() == 2
    });
    assert!(
        commands
            .iter()
            .all(|c| c.epoch == "test daemon" && c.job == 42)
    );
    assert_eq!(
        commands[0].action,
        network_runtime::JobAction::OverrideBandwidth
    );
    assert_eq!(commands[1].action, network_runtime::JobAction::Cancel);
    // A replacement daemon can reuse the numeric job id; selection cannot.
    store
        .write(|ctx| {
            let mut snapshot = settings::get::<Snapshot>(ctx.conn())?;
            snapshot.epoch = "replacement daemon".into();
            settings::set(ctx.conn(), &snapshot)
        })
        .unwrap();
    jobs.invoke_refresh();
    until(|| !jobs.get_can_act());
    jobs.invoke_cancel_jobs();
    assert!(
        store
            .write(|ctx| network_runtime::take_commands(ctx.conn()))
            .unwrap()
            .is_empty()
    );
    store
        .write(|ctx| settings::set(ctx.conn(), &Snapshot::default()))
        .unwrap();
    jobs.invoke_refresh();
    until(|| jobs.get_rows().row_count() == 0);
    assert!(!jobs.get_can_act());
    assert!(jobs.get_status().contains("offline"));
    jobs.invoke_close_clicked();
    jobs.invoke_cancel_jobs();
    assert!(
        store
            .write(|ctx| network_runtime::take_commands(ctx.conn()))
            .unwrap()
            .is_empty()
    );
}

// leaf: audit-network-bandwidth-filters
// leaf: bandwidth-delete-history
// leaf: bandwidth-month-chart
#[test]
fn bandwidth_filters_chart_multiselect_delete_cancel_live_reset_and_saved_age() {
    use hydrus_core::bandwidth::{Rule, Rules};
    use hydrus_store::bandwidth::{self, BandwidthSettings};
    let rendered = headless::init();
    let slots = windows::Slots::default();
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let now = TimestampMs::now().millis() / 1000;
    let context = |name: &str| NetworkContext::domain(format!("{name}.example.com"));
    let mut recent = Tracker::new(now);
    recent.report_data(1024, now - 70 * 86400);
    recent.report_requests(1, now - 70 * 86400);
    recent.report_data(2048, now - 35 * 86400);
    recent.report_requests(1, now - 35 * 86400);
    recent.report_data(4096, now);
    recent.report_requests(1, now);
    let mut old = Tracker::new(now);
    old.report_data(512, now - 10 * 86400);
    old.report_requests(1, now - 10 * 86400);
    let mut bytes = Tracker::new(now);
    bytes.report_data(123, now);
    let usage = vec![
        (context("recent"), recent),
        (context("old"), old),
        (context("bytes"), bytes),
    ];
    store
        .write({
            let usage = usage.clone();
            move |ctx| {
                bandwidth::save_usage(ctx.conn(), &usage)?;
                settings::set(
                    ctx.conn(),
                    &Snapshot {
                        epoch: "live review".into(),
                        at: now,
                        usage,
                        jobs: Vec::new(),
                        ..Snapshot::default()
                    },
                )?;
                let mut settings = settings::get::<BandwidthSettings>(ctx.conn())?;
                settings.rules.push((
                    NetworkContext::domain("rules.example.com"),
                    Rules::new([Rule::new(BandwidthType::Data, Some(86400), 100_000)]),
                ));
                settings::set(ctx.conn(), &settings)
            }
        })
        .unwrap();
    let review = windows::open_bandwidth(store.clone(), &slots).unwrap();
    until(|| review.get_rows().row_count() == 1);
    review.invoke_row_clicked(0, false, false);
    assert_eq!(review.get_monthly_bars().row_count(), 3);
    assert!(
        review.get_monthly_bars().row_data(2).unwrap().fraction
            > review.get_monthly_bars().row_data(0).unwrap().fraction
    );
    assert_eq!(review.get_monthly_bars().row_data(2).unwrap().usage, "4 KB");
    capture(&rendered, 0, "bandwidth-monthly-chart.png", 1120, 720);
    review.set_include_rules(true);
    review.invoke_refresh();
    until(|| review.get_rows().row_count() == 2);
    review.set_include_rules(false);
    review.set_history(4);
    review.set_history_seconds("1209600".into());
    review.invoke_refresh();
    until(|| review.get_rows().row_count() == 2);
    review.set_history_seconds("0".into());
    review.invoke_refresh();
    assert!(review.get_status().contains("positive history"));
    review.set_history_seconds("1209600".into());
    review.set_show_all(true);
    review.invoke_refresh();
    until(|| review.get_rows().row_count() == 4);
    let locate = |name: &str| {
        review
            .get_rows()
            .iter()
            .position(|r| r.cells.row_data(0).unwrap().contains(name))
            .unwrap()
    };
    let recent = locate("recent.example.com");
    let rules = locate("rules.example.com");
    review.invoke_row_clicked(i32::try_from(recent).unwrap(), false, false);
    review.invoke_row_clicked(i32::try_from(rules).unwrap(), true, false);
    assert!(review.get_can_delete());
    assert!(!review.get_can_edit());
    assert_eq!(
        review.get_rows().iter().filter(|row| row.selected).count(),
        2
    );
    review.invoke_delete_history_clicked();
    let fixture = hydrus_testkit::fixture_json("bandwidth_history.json");
    assert_eq!(
        review.get_question(),
        fixture["questions"][0].as_str().unwrap()
    );
    review.invoke_answer(false);
    assert_eq!(store.read(|c| bandwidth::usage(c, now)).unwrap().len(), 3);
    review.invoke_delete_history_clicked();
    review.invoke_answer(true);
    until(|| !review.get_busy() && review.get_rows().row_count() == 3);
    assert_eq!(store.read(|c| bandwidth::usage(c, now)).unwrap().len(), 2);
    assert_eq!(
        store.read(settings::get::<Snapshot>).unwrap().usage.len(),
        2
    );
    assert!(
        store
            .read(settings::get::<BandwidthSettings>)
            .unwrap()
            .rules
            .iter()
            .any(|(c, _)| c == &context("rules"))
    );
    assert!(
        !review.get_rows().iter().any(|r| r
            .cells
            .row_data(0)
            .unwrap()
            .contains("recent.example.com"))
    );
    let rules = locate("rules.example.com");
    review.invoke_row_clicked(i32::try_from(rules).unwrap(), false, false);
    assert_eq!(review.get_monthly_bars().row_count(), 0);
    assert!(review.get_chart_context().contains("rules.example.com"));
    review.set_show_all(false);
    review.invoke_refresh();
    until(|| {
        store
            .read(settings::get::<hydrus_gui_model::network_data::BandwidthReviewPreferences>)
            .unwrap()
            .history
            == Some(1_209_600)
    });
    review.invoke_close_clicked();
    let reopened = windows::open_bandwidth(store.clone(), &slots).unwrap();
    assert_eq!(reopened.get_history(), 4);
    assert_eq!(reopened.get_history_seconds(), "1209600");
    until(|| reopened.get_rows().row_count() == 1);
    reopened.invoke_sort(4, false);
    assert!(
        reopened
            .get_rows()
            .row_data(0)
            .unwrap()
            .cells
            .row_data(0)
            .unwrap()
            .contains("old.example.com")
    );
    reopened.invoke_close_clicked();
}

// leaf: audit-network-bandwidth-rows
// leaf: audit-network-bandwidth-history
// leaf: audit-network-bandwidth-defaults
// leaf: audit-network-bandwidth-detail
#[test]
fn bandwidth_history_columns_sort_detail_and_default_kind_routing() {
    use hydrus_core::bandwidth::{Rule, Rules};
    use hydrus_store::bandwidth::{self, BandwidthSettings};
    let _rendered = headless::init();
    let slots = windows::Slots::default();
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let now = TimestampMs::now().millis() / 1000;
    let mut a = Tracker::new(now);
    for (amount, at) in [
        (8192, now - 35 * 86400),
        (4096, now - 2 * 86400),
        (2048, now),
    ] {
        a.report_data(amount, at);
        a.report_requests(1, at);
    }
    let mut z = Tracker::new(now);
    z.report_data(900, now);
    z.report_requests(1, now);
    store
        .write(move |ctx| {
            bandwidth::save_usage(
                ctx.conn(),
                &[
                    (NetworkContext::domain("a.example.com"), a),
                    (NetworkContext::domain("z.example.com"), z),
                ],
            )
        })
        .unwrap();
    let review = windows::open_bandwidth(store.clone(), &slots).unwrap();
    until(|| review.get_rows().row_count() == 2);
    let a_row = || {
        review
            .get_rows()
            .iter()
            .position(|r| r.cells.row_data(0).unwrap().contains("a.example.com"))
            .unwrap()
    };
    let history_cell = || {
        review
            .get_rows()
            .row_data(a_row())
            .unwrap()
            .cells
            .row_data(4)
            .unwrap()
    };
    assert_eq!(history_cell(), "6 KB in 2 requests");
    review.set_history(1);
    review.invoke_refresh();
    assert_eq!(history_cell(), "2 KB in 1 requests");
    review.set_history(2);
    review.invoke_refresh();
    assert_eq!(
        history_cell(),
        review
            .get_rows()
            .row_data(a_row())
            .unwrap()
            .cells
            .row_data(5)
            .unwrap()
    );
    review.set_history(3);
    review.invoke_refresh();
    assert_eq!(history_cell(), "14 KB in 3 requests");
    review.set_history(1);
    review.invoke_refresh();
    review.invoke_sort(4, true);
    assert!(
        review
            .get_rows()
            .row_data(0)
            .unwrap()
            .cells
            .row_data(0)
            .unwrap()
            .contains("z.example.com")
    );
    review.invoke_sort(4, false);
    assert!(
        review
            .get_rows()
            .row_data(0)
            .unwrap()
            .cells
            .row_data(0)
            .unwrap()
            .contains("a.example.com")
    );
    review.invoke_row_clicked(i32::try_from(a_row()).unwrap(), false, false);
    assert!(review.get_detail().contains("uses default rules"));
    assert!(
        review
            .get_detail()
            .contains("All time usage: 14 KB in 3 requests")
    );
    assert!(review.get_detail().contains("2 KB used"));
    assert!(!review.get_can_revert());
    store
        .write(|ctx| {
            let mut options = settings::get::<BandwidthSettings>(ctx.conn())?;
            options.rules.push((
                NetworkContext::domain("a.example.com"),
                Rules::new([Rule::new(BandwidthType::Requests, Some(86400), 10)]),
            ));
            settings::set(ctx.conn(), &options)
        })
        .unwrap();
    review.invoke_refresh();
    until(|| review.get_can_revert());
    assert!(review.get_detail().contains("has its own rules"));
    assert!(
        review
            .get_detail()
            .contains("10 requests every 1 day (1 used)")
    );
    let f = hydrus_testkit::fixture_json("bandwidth_history.json");
    let kinds = [0, 2, 1, 4, 5, 6];
    let reference_kinds: Vec<_> = f["default_choices"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["type"].as_i64().unwrap())
        .collect();
    for (index, kind) in kinds.into_iter().enumerate() {
        assert!(reference_kinds.contains(&kind));
        review.set_default_kind(i32::try_from(index).unwrap());
        review.invoke_default_clicked();
        let editor = windows::last_rules().unwrap();
        assert_eq!(
            editor.get_window_title(),
            format!(
                "edit bandwidth rules for {}",
                NetworkContext::default_of_kind(kind).to_human_string()
            )
        );
        editor.set_requests(true);
        editor.set_monthly(false);
        editor.set_amount("113".into());
        editor.set_seconds("67".into());
        editor.invoke_add_rule();
        editor.invoke_apply_clicked();
        until(|| windows::last_rules().is_none());
        assert!(
            store
                .read(settings::get::<BandwidthSettings>)
                .unwrap()
                .rules
                .iter()
                .any(
                    |(context, rules)| context == &NetworkContext::default_of_kind(kind)
                        && rules.rules().contains(&Rule::new(
                            BandwidthType::Requests,
                            Some(67),
                            113
                        ))
                )
        );
    }
    review.invoke_close_clicked();
}
