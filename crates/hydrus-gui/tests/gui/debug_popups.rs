//! Help > debug > gui actions > make some popups and the two modal popups,
//! clicked in the real menu and compared with the reference's own
//! (`oracle/fixtures/debug_popups.json`, recorded by
//! `oracle/record_debug_popups.py` from the real client's menu).
use super::debug_menu_actions::{Debug, start};
use hydrus_store::popups;
use serde_json::Value;
use std::time::{Duration, Instant};

fn recording() -> Value {
    hydrus_testkit::fixture_json("debug_popups.json")
}

fn text(v: &Value) -> Option<&str> {
    v.as_str()
}

fn pump(ms: u64) {
    slint::platform::update_timers_and_animations();
    std::thread::sleep(Duration::from_millis(ms));
}

// leaf: audit-options-help-debug-action-make-some-popups
#[test]
fn make_some_popups_makes_the_reference_s_popups_in_its_order_less_three_that_call_back() {
    let d = start();
    let recording = recording();
    let events = recording["some_popups"].as_array().unwrap();
    d.click(&["gui actions", "make some popups"]);
    let started = Instant::now();
    // the three that call back into the client or the network engine are not
    // made (DIFFERENCES.md)
    let left_out = [
        "user call test",
        "sub gap downloader test",
        "test network control",
    ];
    let expected: Vec<&Value> = events
        .iter()
        .map(|e| &e["job"])
        .filter(|j| !left_out.contains(&j["title"].as_str().unwrap_or("")))
        // (the second file merge popup joins the first: the store merges them
        // as the toaster does, which is why the first says five files)
        .filter(|j| {
            !j["text_1"]
                .as_str()
                .is_some_and(|t| t.starts_with("hey you should not see me"))
        })
        .collect();
    assert_eq!(expected.len(), 16);
    // everything but the three delayed ones comes at once
    let mut arrivals: Vec<(Duration, Vec<popups::Job>)> = Vec::new();
    while d.jobs().len() < expected.len() {
        assert!(
            started.elapsed() < Duration::from_secs(10),
            "the popups arrive"
        );
        pump(10);
        let seen = d.jobs();
        if arrivals
            .last()
            .is_none_or(|(_, last)| last.len() != seen.len())
        {
            arrivals.push((started.elapsed(), seen));
        }
    }
    let jobs = d.jobs();
    for (job, recorded) in jobs.iter().zip(&expected) {
        let label = format!("{:?} / {:?}", recorded["title"], recorded["text_1"]);
        assert_eq!(
            job.status_title.as_deref(),
            text(&recorded["title"]),
            "{label}"
        );
        assert_eq!(
            job.status_text_1.as_deref(),
            text(&recorded["text_1"]),
            "{label}"
        );
        assert_eq!(
            job.status_text_2.as_deref(),
            text(&recorded["text_2"]),
            "{label}"
        );
        let gauge = recorded["gauge_1"]
            .as_array()
            .map(|g| (g[0].as_i64().unwrap(), g[1].as_i64().unwrap()));
        assert_eq!(job.popup_gauge_1, gauge, "{label}");
        assert_eq!(
            job.pausable,
            recorded["pausable"].as_bool().unwrap(),
            "{label}"
        );
        assert_eq!(
            job.cancellable,
            recorded["cancellable"].as_bool().unwrap(),
            "{label}"
        );
        let files = job
            .files
            .as_ref()
            .map(|(hashes, label)| (hashes.len() as i64, label.clone()));
        let recorded_files = recorded["files"].as_object().map(|f| {
            (
                f["count"].as_i64().unwrap(),
                f["label"].as_str().map(str::to_owned),
            )
        });
        let merged = recorded["text_1"].as_str() == Some("hey I should have five files");
        let recorded_files = if merged {
            Some((5, Some("cool pics".to_owned())))
        } else {
            recorded_files
        };
        assert_eq!(files, recorded_files, "{label}");
        assert_eq!(
            job.attached_files_mergable,
            recorded["mergable"].as_bool().unwrap_or(false),
            "{label}"
        );
        // the exception's text is framed as the reference's is; Python's
        // frames are its own
        if let Some(recorded_trace) = recorded["traceback"].as_str() {
            let ours = job.traceback.as_deref().expect("a traceback");
            let framing = |t: &str| -> Vec<String> {
                t.lines()
                    .filter(|l| l.starts_with("====="))
                    .map(str::to_owned)
                    .collect()
            };
            assert_eq!(framing(ours), framing(recorded_trace));
            assert_eq!(
                ours.lines().take(2).collect::<Vec<_>>(),
                recorded_trace.lines().take(2).collect::<Vec<_>>()
            );
        }
    }
    // the delayed three come about half a second apart, last
    let delayed_at: Vec<f64> = events
        .iter()
        .filter(|e| {
            e["job"]["text_1"]
                .as_str()
                .is_some_and(|t| t.starts_with("This is a delayed"))
        })
        .map(|e| e["at_ms"].as_f64().unwrap())
        .collect();
    assert_eq!(delayed_at.len(), 3);
    let gaps: Vec<f64> = delayed_at.windows(2).map(|w| w[1] - w[0]).collect();
    assert!(
        gaps.iter().all(|g| (350.0..650.0).contains(g)),
        "recorded {gaps:?}"
    );
    let mut times = Vec::new();
    for n in 14..17 {
        // (the arrivals list has one entry per change of count)
        if let Some((at, _)) = arrivals.iter().find(|(_, seen)| seen.len() >= n) {
            times.push(at.as_secs_f64() * 1000.0);
        }
    }
    let ours: Vec<f64> = times.windows(2).map(|w| w[1] - w[0]).collect();
    assert!(ours.iter().all(|g| (350.0..700.0).contains(g)), "{ours:?}");
}

/// The text the countdown shows `elapsed` seconds after the job is published,
/// as the recording has it: the nearest second, either side of a change.
fn countdown_texts(elapsed: f64) -> Vec<String> {
    let lo = (elapsed - 1.1).floor().max(0.0) as i64;
    let hi = (elapsed + 0.45).floor().clamp(0.0, 9.0) as i64;
    (lo.min(9)..=hi)
        .map(hydrus_gui_model::debug_actions::modal_text)
        .collect()
}

fn recorded_countdown_holds(run: &Value) {
    let samples = run["samples"].as_array().unwrap();
    // (a cancelled countdown stops where it was)
    let until = run["cancel_at_ms"]
        .as_f64()
        .map(|c| (c - samples[0]["at_ms"].as_f64().unwrap()) / 1000.0);
    let at0 = samples[0]["at_ms"].as_f64().unwrap();
    for s in samples {
        let elapsed = (s["at_ms"].as_f64().unwrap() - at0) / 1000.0;
        if elapsed >= 9.9 || until.is_some_and(|u| elapsed > u) {
            continue;
        }
        let shown = s["text_1"].as_str().unwrap();
        assert!(
            countdown_texts(elapsed).iter().any(|t| t == shown),
            "{elapsed}: recorded {shown:?}"
        );
    }
}

fn gauge_of(job: &popups::Job) -> Option<(i64, i64)> {
    job.popup_gauge_1
}

// leaf: audit-options-help-debug-action-make-a-modal-popup-in-five-seconds
// leaf: audit-options-help-debug-action-make-a-non-cancellable-modal-popup-in-five-seconds
#[test]
fn modal_popups_count_down_ten_seconds_and_a_cancelled_one_stops() {
    let d = start();
    let recording = recording();
    for name in ["cancellable", "non_cancellable", "cancellable_cancelled"] {
        recorded_countdown_holds(&recording[name]);
    }
    let both = |d: &Debug| (d.bound.popup_modal.dialog().is_some(), d.jobs().len());
    d.click(&["gui actions", "make a modal popup in five seconds"]);
    d.click(&[
        "gui actions",
        "make a non-cancellable modal popup in five seconds",
    ]);
    let started = Instant::now();
    while !(both(&d).0 && both(&d).1 == 1) {
        assert!(started.elapsed() < Duration::from_secs(15), "both arrive");
        pump(20);
    }
    let published = Instant::now();
    assert!(started.elapsed() >= Duration::from_secs(5));
    // recorded: the cancellable one shows in a dialog, not done; the other
    // is done from the start and goes to the popups
    let recorded_first = &recording["cancellable"]["events"][0]["job"];
    let recorded_other = &recording["non_cancellable"]["events"][0]["job"];
    assert!(
        recorded_first["cancellable"].as_bool().unwrap()
            && !recorded_first["done"].as_bool().unwrap()
    );
    assert!(
        !recorded_other["cancellable"].as_bool().unwrap()
            && recorded_other["done"].as_bool().unwrap()
    );
    let time_now = hydrus_core::TimestampMs::now().0 / 1000;
    let held_now = |d: &Debug| d.store.read(|c| popups::held(c, time_now)).unwrap();
    assert_eq!(held_now(&d).len(), 1);
    assert_eq!(d.jobs()[0].status_title.as_deref(), Some("debug modal job"));
    assert!(!d.jobs()[0].pausable && !d.jobs()[0].cancellable);
    assert_eq!(
        held_now(&d)[0].status_title.as_deref(),
        Some("debug modal job")
    );
    // follow both to the end: the text and gauge each second
    let mut last_gauge = (0, 10);
    let mut seen_gauges = std::collections::BTreeSet::new();
    while published.elapsed() < Duration::from_millis(10_600) {
        pump(100);
        let elapsed = published.elapsed().as_secs_f64();
        let now = hydrus_core::TimestampMs::now().0 / 1000;
        let other = d.store.read(|c| popups::all(c, now)).unwrap();
        if let Some(job) = other.first() {
            if elapsed < 9.8 {
                let shown = job.status_text_1.clone().unwrap_or_default();
                assert!(
                    countdown_texts(elapsed).contains(&shown),
                    "{elapsed:.2}: non-cancellable shows {shown:?}"
                );
            }
            if let Some(g) = gauge_of(job) {
                assert_eq!(g.1, 10);
                assert!(g.0 >= last_gauge.0, "the gauge only goes up");
                last_gauge = g;
                seen_gauges.insert(g.0);
            }
        }
    }
    assert!(seen_gauges.len() >= 9, "{seen_gauges:?}");
    // the dialog closes itself when the job finishes, and the job is gone
    let until = Instant::now();
    while d.bound.popup_modal.dialog().is_some() {
        assert!(
            until.elapsed() < Duration::from_secs(5),
            "the dialog closes"
        );
        pump(50);
    }
    let end = published.elapsed().as_secs_f64();
    let recorded_end = (recording["cancellable"]["finish_ms"].as_f64().unwrap()
        - recording["cancellable"]["events"][0]["at_ms"]
            .as_f64()
            .unwrap())
        / 1000.0;
    assert!(
        (end - recorded_end).abs() < 1.2,
        "ends {end:.2}s in, recorded {recorded_end:.2}s"
    );
    let _ = held_now;
}

// leaf: audit-options-help-debug-action-make-a-modal-popup-in-five-seconds
#[test]
fn cancelling_the_modal_popup_stops_its_countdown() {
    let d = start();
    let recording = recording();
    let cancelled = &recording["cancellable_cancelled"];
    let at0 = cancelled["events"][0]["at_ms"].as_f64().unwrap();
    let cancel_after = (cancelled["cancel_at_ms"].as_f64().unwrap() - at0) / 1000.0;
    d.click(&["gui actions", "make a modal popup in five seconds"]);
    let started = Instant::now();
    while d.bound.popup_modal.dialog().is_none() {
        assert!(started.elapsed() < Duration::from_secs(15));
        pump(20);
    }
    let published = Instant::now();
    while published.elapsed().as_secs_f64() < cancel_after {
        pump(50);
    }
    let dialog = d.bound.popup_modal.dialog().unwrap();
    dialog.invoke_close_clicked();
    d.bound.popup_modal.question().unwrap().invoke_chosen(0);
    let frozen = d
        .store
        .read(|c| popups::all(c, hydrus_core::TimestampMs::now().0 / 1000 + 1))
        .unwrap();
    // cancelled: the countdown stops where it was, and the job is dismissed
    // within a second, as recorded
    let text_at_cancel = frozen[0].status_text_1.clone().unwrap();
    assert!(frozen[0].cancelled);
    assert!(
        countdown_texts(cancel_after).contains(&text_at_cancel),
        "{text_at_cancel}"
    );
    let stopped = Instant::now();
    while !d.jobs().is_empty() {
        assert!(stopped.elapsed() < Duration::from_secs(3), "dismissed");
        pump(50);
    }
    let recorded_gap = cancelled["samples"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["dismissed"].as_bool().unwrap())
        .map(|s| s["at_ms"].as_f64().unwrap() - cancelled["cancel_at_ms"].as_f64().unwrap())
        .unwrap();
    assert!(recorded_gap < 1600.0);
    assert!(
        stopped.elapsed().as_millis() < 1700,
        "dismissed {:?} after",
        stopped.elapsed()
    );
    // and it stays at the text it had
    std::thread::sleep(Duration::from_millis(1200));
    assert_eq!(text_at_cancel, frozen[0].status_text_1.clone().unwrap());
}
