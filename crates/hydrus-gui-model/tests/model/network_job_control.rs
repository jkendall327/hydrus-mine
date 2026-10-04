//! Replay the real cog/error recording and owner/request boundaries.
use hydrus_core::{
    bandwidth::{BandwidthType, Rule, Rules, Tracker},
    network::NetworkContext,
};
use hydrus_gui_model::{
    network_data::Review,
    network_job_control::{self as model, Action, Control, Target},
};
use hydrus_store::{
    bandwidth::BandwidthSettings,
    network_runtime::{JobAction, JobControl, JobError, NetworkJob, Snapshot, WaitReason},
};

fn job(id: u64, queue: i64, wait: WaitReason) -> NetworkJob {
    NetworkJob {
        id,
        url: "https://example.com/file".into(),
        status: "waiting".into(),
        wait,
        bytes_read: 0,
        bytes_total: None,
        speed: 0,
        contexts: vec![
            NetworkContext::global(),
            NetworkContext::domain("example.com"),
            NetworkContext::domain("sub.example.com"),
            NetworkContext::downloader_page(format!("{queue:016x}")),
        ],
        obeys_bandwidth: true,
    }
}
fn meta(id: u64, now: i64) -> JobControl {
    JobControl {
        id,
        created: now,
        gallery: false,
        domain_ok: false,
        tokens_ok: false,
        auto_override: false,
    }
}

#[test]
fn cog_replays_actual_qt_context_submenu_and_phase_actions() {
    let f = hydrus_testkit::fixture_json("network_job_control.json");
    let now = f["now"].as_i64().unwrap();
    let mut tracker = Tracker::new(now);
    tracker.report_requests(1, now);
    let review = Review {
        settings: BandwidthSettings {
            rules: vec![(
                NetworkContext::global(),
                Rules::new([Rule::new(BandwidthType::Requests, Some(60), 1)]),
            )],
            ..BandwidthSettings::default()
        },
        runtime: Snapshot::default(),
        usage: vec![(NetworkContext::global(), tracker)],
        live: true,
    };
    let mut job = job(1, 42, WaitReason::Connection);
    let meta = meta(1, now);
    let menu = model::cog(&review, Some((&job, &meta)), false, now);
    assert_eq!(
        menu.url.as_ref().unwrap().label,
        f["menus"]["waiting"][0]["label"]
    );
    let recorded_rules = f["menus"]["waiting"][2]["children"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["label"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        menu.rules
            .iter()
            .map(|r| r.label.as_str())
            .collect::<Vec<_>>(),
        recorded_rules
    );
    assert!(
        menu.rules
            .iter()
            .all(|r| !matches!(&r.action, Action::Rules(c) if c.is_ephemeral() && !c.is_default()))
    );
    let expected = f["menus"]["waiting"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|r| r["label"].as_str())
        .filter(|s| {
            !s.is_empty()
                && *s != "edit bandwidth rules"
                && *s != job.url
                && *s != model::AUTO_LABEL
                && *s != model::SERVER_LABEL
        })
        .collect::<Vec<_>>();
    assert_eq!(
        menu.actions
            .iter()
            .map(|r| r.label.as_str())
            .collect::<Vec<_>>(),
        expected
    );
    job.wait = WaitReason::ServerBandwidth;
    let menu = model::cog(&review, Some((&job, &meta)), true, now);
    assert!(menu.actions.iter().any(|r| r.label == model::SERVER_LABEL));
    assert!(
        !menu
            .actions
            .iter()
            .any(|r| r.label == model::CONNECTION_LABEL)
    );
    assert!(menu.auto_override);
    assert_eq!(f["menus"]["empty"][0]["label"], model::AUTO_LABEL);
    assert!(model::cog(&review, None, true, now).rules.is_empty());
    job.url = "https://example.com/a%20b?token=%E2%9C%93".into();
    let menu = model::cog(&review, Some((&job, &meta)), false, now);
    assert_eq!(
        menu.url.as_ref().unwrap().label,
        "https://example.com/a b?token=✓"
    );
    assert_eq!(menu.url.unwrap().action, Action::CopyUrl(job.url));
}

#[test]
fn control_auto_policy_targets_future_requests_and_owner_clears_error_once() {
    let f = hydrus_testkit::fixture_json("network_job_control.json");
    let now = f["now"].as_i64().unwrap();
    let target = Target {
        queue: 42,
        gallery: false,
    };
    let mut snapshot = Snapshot {
        epoch: "daemon".into(),
        at: now,
        jobs: vec![
            job(1, 42, WaitReason::Bandwidth),
            job(2, 99, WaitReason::Bandwidth),
        ],
        controls: vec![meta(1, now), meta(2, now)],
        ..Snapshot::default()
    };
    let mut control = Control::default();
    control.flip_auto_override();
    let command = control.sync(target, &snapshot, now).unwrap();
    assert_eq!(
        (command.job, command.action),
        (1, JobAction::AutoOverrideBandwidth(true))
    );
    assert!(control.sync(target, &snapshot, now).is_none());
    control.flip_auto_override();
    assert_eq!(
        control.sync(target, &snapshot, now).unwrap().action,
        JobAction::AutoOverrideBandwidth(false)
    );
    control.flip_auto_override();
    snapshot.jobs[0].id = 3;
    snapshot.controls[0].id = 3;
    assert_eq!(control.sync(target, &snapshot, now).unwrap().job, 3);
    assert!(target.job(&snapshot, now + 6).is_none());
    assert!(control.sync(target, &snapshot, now + 6).is_none());
    let text = f["actions"]["clear_job_keeps_error"].as_str().unwrap();
    snapshot.errors.push(JobError {
        id: 3,
        url: snapshot.jobs[0].url.clone(),
        contexts: snapshot.jobs[0].contexts.clone(),
        gallery: false,
        text: text.into(),
    });
    snapshot.jobs.clear();
    snapshot.controls.clear();
    control.sync(target, &snapshot, now);
    assert_eq!(control.error(), Some(text));
    control.clear_error();
    control.sync(target, &snapshot, now);
    assert_eq!(control.error(), None); // the retained daemon journal does not resurrect a cleared error
    assert!(f["actions"]["owner_clear_error"].is_null());
    snapshot.errors[0].id = 4;
    control.sync(target, &snapshot, now);
    assert_eq!(control.error(), Some(text));
    let other = Target {
        queue: 99,
        gallery: false,
    };
    let mut control = Control::default();
    control.sync(other, &snapshot, now);
    assert_eq!(control.error(), None);
}
