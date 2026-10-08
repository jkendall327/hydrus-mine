//! The network job control in a popup against the reference's
//! (`oracle/fixtures/popup_network_job.json`, from
//! `oracle/record_popup_network_job.py`): its left text, right text and
//! gauge for each state of the job, and how long it stays after the job.

use hydrus_gui_model::popup_network_job::Control;
use hydrus_store::live::JobLive;
use serde_json::Value;

fn live(
    status: &str,
    speed: u64,
    read: u64,
    to_read: Option<u64>,
    done: bool,
    error: bool,
) -> JobLive {
    JobLive {
        url: "https://example.com/file".into(),
        status: status.into(),
        speed,
        bytes_read: read,
        bytes_to_read: to_read,
        done,
        error,
    }
}

fn step<'a>(steps: &'a [Value], label: &str) -> &'a Value {
    steps
        .iter()
        .find(|s| s["label"] == label)
        .unwrap_or_else(|| panic!("no recorded step {label:?}"))
}

// leaf: audit-options-popups-download
#[test]
fn a_popups_network_job_control_shows_status_speed_and_gauge_as_the_references_does() {
    let recorded = hydrus_testkit::fixture_json("popup_network_job.json");
    let steps = recorded.as_array().unwrap();
    let mut control = Control::default();
    assert_eq!(
        control.update(None, 100),
        None,
        "{}",
        step(steps, "popup with no network job")["label"]
    );
    assert_eq!(step(steps, "popup with no network job")["shown"], false);
    for (label, job) in [
        (
            "connecting, nothing read",
            live("connecting...\nsecond line", 0, 0, None, false, false),
        ),
        (
            "downloading with a known size",
            live("downloading", 12345, 500_000, Some(2_000_000), false, false),
        ),
        (
            "downloading a small file",
            live("downloading", 100, 400, Some(800), false, false),
        ),
        (
            "downloading with no known size",
            live("downloading", 2048, 3000, None, false, false),
        ),
        (
            "a quick download",
            live("done!", 4096, 4096, Some(4096), true, false),
        ),
        (
            "a failed download",
            live("ERROR: 404", 0, 1500, Some(9000), true, true),
        ),
    ] {
        let recorded = step(steps, label);
        let shown = control.update(Some(job.line()), 100).expect(label);
        assert_eq!(recorded["shown"], true, "{label}");
        assert_eq!(
            shown.left,
            recorded["left"].as_str().unwrap(),
            "{label}: left"
        );
        // (the reference hides its right text when it has nothing to say,
        // and leaves the last text in it)
        if !shown.right.is_empty() {
            assert_eq!(
                shown.right,
                recorded["right"].as_str().unwrap(),
                "{label}: right"
            );
        }
        assert_eq!(
            [shown.gauge.0, shown.gauge.1],
            [
                recorded["gauge_qt_value"].as_u64().unwrap(),
                recorded["gauge_maximum"].as_u64().unwrap()
            ],
            "{label}: gauge"
        );
        assert_eq!(
            shown.can_cancel,
            recorded["stop_enabled"].as_bool().unwrap(),
            "{label}: stop button enabled while the job is not done"
        );
    }

    // the job goes: cleared at once, still shown, for ten seconds
    let gone = step(steps, "the job goes: cleared at once, still shown");
    let cleared = control.update(None, 200).expect("still shown");
    assert_eq!(gone["shown"], true);
    assert_eq!(cleared.left, gone["left"].as_str().unwrap());
    assert_eq!(cleared.right, gone["right"].as_str().unwrap());
    assert_eq!(
        [cleared.gauge.0, cleared.gauge.1],
        [
            gone["gauge"][0].as_u64().unwrap(),
            gone["gauge"][1].as_u64().unwrap()
        ]
    );
    assert_eq!(cleared.can_cancel, gone["stop_enabled"].as_bool().unwrap());
    assert_eq!(step(steps, "nine seconds on: still shown")["shown"], true);
    assert!(control.update(None, 209).is_some());
    assert!(
        control.update(None, 210).is_some(),
        "ten seconds is not past it"
    );
    assert_eq!(step(steps, "eleven seconds on: hidden")["shown"], false);
    assert_eq!(control.update(None, 211), None);
    assert_eq!(control.update(None, 5000), None);
    // and a new job shows it again
    assert!(
        control
            .update(Some(live("x", 0, 0, None, false, false).line()), 300)
            .is_some()
    );
}
