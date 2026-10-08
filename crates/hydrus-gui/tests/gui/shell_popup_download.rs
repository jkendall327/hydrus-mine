//! A popup's attached network job as the main window shows it: the first
//! line of its status, what it has read and its speed, the gauge; and the
//! control left, blank, when the job goes (hydrus `PopupMessage` with a
//! `NetworkJobControl`, recorded in `popup_network_job.json`).

use std::time::{Duration, Instant};

use slint::{ComponentHandle as _, Model as _};

use hydrus_gui::{MainWindow, Pages, bind, headless};
use hydrus_store::live::JobLive;
use hydrus_store::popups::{self, Job};

fn now() -> i64 {
    hydrus_core::time::TimestampMs::now().millis() / 1000
}

fn pump_until(what: &str, mut done: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !done() {
        assert!(Instant::now() < deadline, "{what}");
        slint::platform::update_timers_and_animations();
        std::thread::sleep(Duration::from_millis(10));
    }
}

// leaf: audit-options-popups-download
#[test]
fn a_popup_shows_its_network_jobs_status_speed_and_gauge_and_leaves_the_control_blank_after() {
    let recorded = hydrus_testkit::fixture_json("popup_network_job.json");
    let step = |label: &str| {
        recorded
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["label"] == label)
            .unwrap()
            .clone()
    };
    let (_dirs, store) = super::subscriptions::store();
    let mut job = Job::new(false, true, 0.0);
    job.status_title = Some("a download".into());
    job.network_job = Some(JobLive {
        url: "https://example.com/file".into(),
        status: "downloading\nsecond line".into(),
        speed: 12345,
        bytes_read: 500_000,
        bytes_to_read: Some(2_000_000),
        done: false,
        error: false,
    });
    let key = job.key;
    let added = job.clone();
    store
        .write(move |ctx| popups::add(ctx.conn(), &added, now()))
        .unwrap();
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let _bound = bind(&ui, Pages::open(store.clone()).unwrap());

    let row = ui.get_popups().row_data(0).unwrap();
    let theirs = step("downloading with a known size");
    assert!(row.has_download);
    assert_eq!(row.download.left, theirs["left"].as_str().unwrap());
    assert_eq!(row.download.right, theirs["right"].as_str().unwrap());
    let fraction =
        theirs["gauge_qt_value"].as_f64().unwrap() / theirs["gauge_maximum"].as_f64().unwrap();
    assert!((f64::from(row.download.fraction) - fraction).abs() < 1e-6);

    // finished: the reference's quick-download form, its stop button off
    store
        .write(move |ctx| {
            popups::update(ctx.conn(), &key, now(), |j| {
                j.network_job = Some(JobLive {
                    url: "https://example.com/file".into(),
                    status: "done!".into(),
                    speed: 4096,
                    bytes_read: 4096,
                    bytes_to_read: Some(4096),
                    done: true,
                    error: false,
                });
            })
            .map(|_| ())
        })
        .unwrap();
    pump_until("the finished job shows", || {
        ui.get_popups().row_data(0).unwrap().download.left == "done!"
    });
    let theirs = step("a quick download");
    let row = ui.get_popups().row_data(0).unwrap();
    assert_eq!(row.download.right, theirs["right"].as_str().unwrap());
    assert!((row.download.fraction - 1.0).abs() < 1e-6);
    assert!(!row.download.can_cancel);

    // the job goes: the control is cleared at once, and still there
    store
        .write(move |ctx| {
            popups::update(ctx.conn(), &key, now(), |j| j.network_job = None).map(|_| ())
        })
        .unwrap();
    pump_until("the control clears", || {
        ui.get_popups()
            .row_data(0)
            .unwrap()
            .download
            .left
            .is_empty()
    });
    let gone = step("the job goes: cleared at once, still shown");
    let row = ui.get_popups().row_data(0).unwrap();
    assert_eq!(gone["shown"], true);
    assert!(row.has_download, "ten seconds more");
    assert_eq!(row.download.left, "");
    assert_eq!(row.download.right, "");
    assert!(row.download.fraction.abs() < 1e-6);
    assert!(!row.download.can_cancel);
}
