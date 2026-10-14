//! Help > debug > gui actions > make some popups and the two modal popups,
//! clicked in the real menu and compared with the reference's own
//! (`oracle/fixtures/debug_popups.json`, recorded by
//! `oracle/record_debug_popups.py` from the real client's menu).
use super::debug_menu_actions::start;
use hydrus_store::popups;
use serde_json::Value;
use slint::Model as _;
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
fn make_some_popups_makes_the_reference_s_popups_in_its_order() {
    let d = start();
    let recording = recording();
    let events = recording["some_popups"].as_array().unwrap();
    d.click(&["gui actions", "make some popups"]);
    let started = Instant::now();
    let expected: Vec<&Value> = events
        .iter()
        .map(|e| &e["job"])
        // (the second file merge popup joins the first: the store merges them
        // as the toaster does, which is why the first says five files)
        .filter(|j| {
            !j["text_1"]
                .as_str()
                .is_some_and(|t| t.starts_with("hey you should not see me"))
        })
        .collect();
    assert_eq!(expected.len(), 19);
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
        assert_eq!(
            job.user_callable_label.as_deref(),
            text(&recorded["user_callable_label"]),
            "{label}"
        );
        assert_eq!(
            job.network_job.is_some(),
            recorded["has_network_job"].as_bool().unwrap(),
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
    for n in 17..20 {
        // (the arrivals list has one entry per change of count)
        if let Some((at, _)) = arrivals.iter().find(|(_, seen)| seen.len() >= n) {
            times.push(at.as_secs_f64() * 1000.0);
        }
    }
    assert_eq!(times.len(), 3, "the three delayed popups arrive one by one");
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

/// Follow a countdown for its ten seconds: `read` gives the job's text and
/// gauge now (`None` once it is gone). Texts must be the recorded ones for
/// the time, and the gauge only goes up and reaches the end.
fn follow_countdown(
    published: Instant,
    mut read: impl FnMut() -> Option<(String, Option<(i64, i64)>)>,
) {
    let mut last_gauge = 0;
    let mut gauges = std::collections::BTreeSet::new();
    while published.elapsed() < Duration::from_millis(10_600) {
        pump(100);
        let elapsed = published.elapsed().as_secs_f64();
        let Some((text, gauge)) = read() else {
            continue;
        };
        if elapsed < 9.8 {
            assert!(
                countdown_texts(elapsed).contains(&text),
                "{elapsed:.2}: shows {text:?}"
            );
        }
        if let Some((value, range)) = gauge {
            assert_eq!(range, 10);
            assert!(value >= last_gauge, "the gauge only goes up");
            last_gauge = value;
            gauges.insert(value);
        }
    }
    assert!(gauges.len() >= 9, "{gauges:?}");
}

fn wait_for(what: &str, limit: u64, mut done: impl FnMut() -> bool) -> Instant {
    let started = Instant::now();
    while !done() {
        assert!(started.elapsed() < Duration::from_secs(limit), "{what}");
        pump(20);
    }
    Instant::now()
}

fn recorded_end(recording: &Value, run: &str) -> f64 {
    let first = recording[run]["events"][0]["at_ms"].as_f64().unwrap();
    (recording[run]["finish_ms"].as_f64().unwrap() - first) / 1000.0
}

// leaf: audit-options-help-debug-action-make-a-non-cancellable-modal-popup-in-five-seconds
#[test]
fn the_non_cancellable_modal_popup_is_done_from_the_start_and_counts_down_ten_seconds() {
    let d = start();
    let recording = recording();
    recorded_countdown_holds(&recording["non_cancellable"]);
    d.click(&[
        "gui actions",
        "make a non-cancellable modal popup in five seconds",
    ]);
    let started = Instant::now();
    assert!(
        d.jobs().is_empty() && d.bound.popup_modal.dialog().is_none(),
        "nothing before five seconds"
    );
    let published = wait_for("it arrives", 15, || !d.jobs().is_empty());
    assert!(started.elapsed() >= Duration::from_secs(5));
    // recorded: not pausable or cancellable, done from the start, so it goes
    // straight to the popups and no dialog holds it
    let recorded = &recording["non_cancellable"]["events"][0]["job"];
    assert!(!recorded["cancellable"].as_bool().unwrap() && recorded["done"].as_bool().unwrap());
    let job = d.jobs().remove(0);
    assert_eq!(job.status_title.as_deref(), Some("debug modal job"));
    assert!(!job.pausable && !job.cancellable);
    assert!(d.bound.popup_modal.dialog().is_none());
    follow_countdown(published, || {
        d.jobs().first().map(|job| {
            (
                job.status_text_1.clone().unwrap_or_default(),
                job.popup_gauge_1,
            )
        })
    });
    let end = published.elapsed().as_secs_f64();
    // (the countdown ends ten seconds on, as the cancellable one's recorded end shows)
    let reference = recorded_end(&recording, "cancellable");
    assert!(
        (end - reference).abs() < 3.0,
        "ends {end:.2}s in, recorded {reference:.2}s"
    );
}

// leaf: audit-options-help-debug-action-make-a-modal-popup-in-five-seconds
#[test]
fn the_cancellable_modal_popup_is_held_by_a_dialog_and_counts_down_ten_seconds() {
    let d = start();
    let recording = recording();
    recorded_countdown_holds(&recording["cancellable"]);
    d.click(&["gui actions", "make a modal popup in five seconds"]);
    let started = Instant::now();
    assert!(
        d.jobs().is_empty() && d.bound.popup_modal.dialog().is_none(),
        "nothing before five seconds"
    );
    let published = wait_for("its dialog opens", 15, || {
        d.bound.popup_modal.dialog().is_some()
    });
    assert!(started.elapsed() >= Duration::from_secs(5));
    // recorded: cancellable and not done, so a dialog holds it; the popups don't have it yet
    let recorded = &recording["cancellable"]["events"][0]["job"];
    assert!(recorded["cancellable"].as_bool().unwrap() && !recorded["done"].as_bool().unwrap());
    assert!(d.jobs().is_empty());
    let dialog = d.bound.popup_modal.dialog().unwrap();
    assert_eq!(dialog.get_window_title(), "debug modal job");
    assert!(!dialog.get_hide_close_button(), "it can be cancelled");
    let held = || {
        d.store
            .read(|c| popups::held(c, hydrus_core::TimestampMs::now().0 / 1000))
            .unwrap()
            .into_iter()
            .next()
    };
    assert!(held().is_some_and(|job| job.cancellable && !job.pausable));
    follow_countdown(published, || {
        held().map(|job| {
            (
                job.status_text_1.clone().unwrap_or_default(),
                job.popup_gauge_1,
            )
        })
    });
    // the dialog closes itself when the job finishes, about when the reference's did
    wait_for("the dialog closes", 8, || {
        d.bound.popup_modal.dialog().is_none()
    });
    let end = published.elapsed().as_secs_f64();
    let reference = recorded_end(&recording, "cancellable");
    assert!(
        (end - reference).abs() < 3.0,
        "ends {end:.2}s in, recorded {reference:.2}s"
    );
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
    let published = wait_for("its dialog opens", 15, || {
        d.bound.popup_modal.dialog().is_some()
    });
    while published.elapsed().as_secs_f64() < cancel_after {
        pump(50);
    }
    let dialog = d.bound.popup_modal.dialog().unwrap();
    dialog.invoke_close_clicked();
    d.bound.popup_modal.question().unwrap().invoke_chosen(0);
    // cancelled: released to the popups, the countdown stops where it was, and
    // the job is dismissed within a second or two, as recorded
    let read = || {
        d.store
            .read(|c| popups::all(c, hydrus_core::TimestampMs::now().0 / 1000))
            .unwrap()
            .into_iter()
            .find(|job| job.status_title.as_deref() == Some("debug modal job"))
    };
    let first = read().expect("it is in the popups, cancelled");
    assert!(first.cancelled);
    let text_at_cancel = first.status_text_1.clone().unwrap();
    assert!(
        countdown_texts(cancel_after).contains(&text_at_cancel),
        "{text_at_cancel}"
    );
    let stopped = Instant::now();
    let mut samples = 1;
    while let Some(job) = read() {
        assert_eq!(
            job.status_text_1.as_deref(),
            Some(text_at_cancel.as_str()),
            "frozen"
        );
        assert_eq!(job.popup_gauge_1, first.popup_gauge_1, "frozen");
        samples += 1;
        assert!(stopped.elapsed() < Duration::from_secs(4), "dismissed");
        pump(50);
    }
    let _ = samples;
    let recorded_gap = cancelled["samples"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["dismissed"].as_bool().unwrap())
        .map(|s| s["at_ms"].as_f64().unwrap() - cancelled["cancel_at_ms"].as_f64().unwrap())
        .unwrap();
    assert!(recorded_gap < 1600.0);
    assert!(
        stopped.elapsed() < Duration::from_secs(3),
        "dismissed {:?} after",
        stopped.elapsed()
    );
}

// leaf: audit-options-help-debug-action-make-some-popups
#[test]
fn the_popups_that_carry_buttons_do_what_the_reference_s_buttons_do_and_the_test_job_pulses() {
    let d = start();
    let recording = recording();
    d.click(&["gui actions", "make some popups"]);
    let started = Instant::now();
    while d.jobs().len() < 19 {
        assert!(
            started.elapsed() < Duration::from_secs(10),
            "the popups arrive"
        );
        pump(10);
    }
    let gui_owner =
        d.ui.get_popups()
            .row_data(0)
            .expect("a popup is shown")
            .gui_owner;
    let pages_named = |d: &super::debug_menu_actions::Debug| -> Vec<String> {
        d.bound
            .pages
            .borrow()
            .session()
            .pages
            .iter()
            .map(|page| page.name.clone())
            .collect()
    };
    let calls = recording["user_calls"].as_array().unwrap();
    assert_eq!(calls.len(), 2);
    for recorded in calls {
        let title = recorded["title"].as_str().unwrap();
        let job = d
            .jobs()
            .into_iter()
            .find(|j| j.status_title.as_deref() == Some(title))
            .unwrap();
        assert_eq!(
            job.user_callable_label.as_deref(),
            recorded["label"].as_str()
        );
        let before = d.jobs().len();
        let recorded_before: Vec<&str> = recorded["pages_before"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| p.as_str().unwrap())
            .collect();
        assert_eq!(pages_named(&d), recorded_before, "{title}");
        // the button, clicked as the popup control does
        d.ui.invoke_popup_call(
            hex::encode(job.key).into(),
            hex::encode(job.action_owner.unwrap()).into(),
            gui_owner.clone(),
        );
        let messages: Vec<&Value> = recorded["messages"].as_array().unwrap().iter().collect();
        let waited = Instant::now();
        while d.jobs().len() < before + messages.len() {
            assert!(
                waited.elapsed() < Duration::from_secs(5),
                "{title}: it acts"
            );
            pump(20);
        }
        // the text popups it shows
        let made: Vec<popups::Job> = d.jobs().into_iter().skip(before).collect();
        assert_eq!(made.len(), messages.len(), "{title}");
        for (job, recorded) in made.iter().zip(&messages) {
            assert_eq!(job.status_text_1.as_deref(), text(&recorded["text_1"]));
            assert_eq!(job.status_title.as_deref(), text(&recorded["title"]));
            assert_eq!(job.had_error, recorded["had_error"].as_bool().unwrap());
        }
        // the pages it left
        let after: Vec<&str> = recorded["pages_after"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| p["name"].as_str().unwrap())
            .collect();
        assert_eq!(pages_named(&d), after, "{title}");
        // and each is a gallery downloader page where recorded so
        for page in recorded["pages_after"].as_array().unwrap() {
            let name = page["name"].as_str().unwrap();
            let ours = d
                .bound
                .pages
                .borrow()
                .session()
                .pages
                .iter()
                .any(|p| {
                    p.name == name
                        && matches!(
                            p.content,
                            hydrus_core::pages::PageContent::Downloader {
                                kind: hydrus_core::pages::DownloaderKind::Gallery,
                                ..
                            }
                        )
                });
            assert_eq!(ours, page["gallery"].as_bool().unwrap(), "{title}: {name}");
        }
    }
    // the test job gets its pulsing sub-job two seconds in, and not before
    let recorded = &recording["some_popups_two_seconds_on"][0];
    let at = recorded["index"].as_u64().unwrap();
    assert_eq!(
        recording["some_popups"][usize::try_from(at).unwrap()]["job"]["title"],
        "test job"
    );
    let test_job = || {
        d.jobs()
            .into_iter()
            .find(|j| j.status_title.as_deref() == Some("test job"))
            .unwrap()
    };
    // (recorded as `gauge_2: [0, null]`: no range, so a pulse)
    assert!(recorded["gauge_2"][1].is_null());
    let pulsed = wait_for("it pulses", 6, || test_job().popup_gauge_2.is_some());
    assert!(
        pulsed.duration_since(started) >= Duration::from_millis(1900),
        "not before two seconds"
    );
    let job = test_job();
    assert_eq!(job.status_text_2.as_deref(), text(&recorded["text_2"]));
    assert_eq!(job.popup_gauge_2, Some((0, 0)));
}
