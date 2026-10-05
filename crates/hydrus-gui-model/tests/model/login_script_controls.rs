//! The actual script-panel cog recording and request replacement boundaries.
use hydrus_core::network::NetworkContext;
use hydrus_gui_model::{login_workflows::TestControl, network_data::Review};
use hydrus_store::network_runtime::{JobAction, JobControl, NetworkJob, Snapshot, WaitReason};

fn review(now: i64) -> Review {
    Review {
        settings: hydrus_store::bandwidth::BandwidthSettings::default(),
        usage: Vec::new(),
        live: true,
        runtime: Snapshot {
            epoch: "isolated-test".into(),
            at: now,
            jobs: vec![NetworkJob {
                id: 1,
                url: "https://example.com/login%20step".into(),
                status: "downloading".into(),
                wait: WaitReason::Downloading,
                bytes_read: 0,
                bytes_total: None,
                speed: 0,
                contexts: vec![
                    NetworkContext::global(),
                    NetworkContext::domain("example.com"),
                ],
                obeys_bandwidth: true,
            }],
            controls: vec![JobControl {
                id: 1,
                created: now,
                gallery: false,
                domain_ok: true,
                tokens_ok: true,
                auto_override: false,
            }],
            ..Default::default()
        },
    }
}
#[test]
fn script_cog_matches_qt_login_context_and_rejects_replaced_requests() {
    let fixture = hydrus_testkit::fixture_json("login_script_controls.json");
    let now = fixture["now"].as_i64().unwrap();
    let mut control = TestControl::default();
    let empty = control.menu(None, now);
    assert!(empty.url.is_none());
    assert!(empty.rules.is_empty());
    assert!(!empty.auto_override);
    let mut review = review(now);
    let generic = control.menu(Some(&review), now);
    assert_eq!(
        generic.url.unwrap().label,
        fixture["menus"]["running"][0]["label"]
    );
    let labels = generic
        .rules
        .iter()
        .map(|entry| entry.label.as_str())
        .collect::<Vec<_>>();
    let recorded = fixture["menus"]["running"][2]["children"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| entry["label"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(labels, recorded);
    assert_eq!(
        generic.actions[0].label,
        fixture["menus"]["running"][4]["label"]
    );
    review.runtime.jobs[0].obeys_bandwidth = fixture["actions"]["login_obeys_bandwidth"]
        .as_bool()
        .unwrap();
    let login = control.menu(Some(&review), now);
    assert!(
        login.actions.is_empty(),
        "actual login requests already bypass bandwidth rules"
    );
    let command = control
        .command(&review.runtime, now, JobAction::ScrubDomainErrors)
        .unwrap();
    assert_eq!(command.job, 1);
    review.runtime.jobs[0].id = 2;
    review.runtime.controls[0].id = 2;
    assert!(
        control
            .command(&review.runtime, now, JobAction::ScrubDomainErrors)
            .is_none()
    );
    control.menu(Some(&review), now);
    assert_eq!(
        control
            .command(&review.runtime, now, JobAction::OverrideConnectionWait)
            .unwrap()
            .job,
        2
    );
    review.runtime.epoch = "successor-run".into();
    assert!(
        control
            .command(&review.runtime, now, JobAction::OverrideConnectionWait)
            .is_none()
    );
    control.retained.flip_auto_override();
    assert_eq!(
        control
            .auto_command(&review.runtime, now, 77)
            .unwrap()
            .action,
        JobAction::AutoOverrideBandwidthFor {
            owner: 77,
            enabled: true
        }
    );
    assert!(control.auto_command(&review.runtime, now, 77).is_none());
    review.runtime.jobs.clear();
    review.runtime.controls.clear();
    assert!(control.menu(Some(&review), now).auto_override);
    control
        .retained
        .set_error(fixture["errors"][0]["message"].as_str().unwrap().into());
    control.menu(None, now);
    assert_eq!(
        control.retained.error().unwrap(),
        fixture["actions"]["clear_job_keeps_error"]["error"]
    );
    control.retained.clear_error();
    assert!(control.retained.error().is_none());
}
