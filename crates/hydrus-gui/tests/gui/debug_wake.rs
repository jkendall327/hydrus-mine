//! Help > debug > debug modes > simulate a wake from sleep, clicked in the real
//! menu and compared with the reference's own popup and idle reset (the
//! recording `oracle/fixtures/debug_wake_from_sleep.json`, made by
//! `oracle/record_debug_wake_from_sleep.py` from the real client's menu).
use super::debug_menu_actions::{ONE_AT_A_TIME, start};
use hydrus_store::network::NetworkSettings;
use hydrus_store::settings::{self, GuiIdleSettings};
use hydrus_store::{Store, popups};
use serde_json::Value;
use std::time::{Duration, Instant};

fn pump(ms: u64) {
    slint::platform::update_timers_and_animations();
    std::thread::sleep(Duration::from_millis(ms));
}

fn the_popup(store: &Store) -> Option<popups::Job> {
    store
        .read(|c| popups::all(c, hydrus_core::TimestampMs::now().0 / 1000))
        .unwrap()
        .into_iter()
        .find(|j| j.status_title.as_deref() == Some("just woke up from sleep"))
}

/// The texts in the order they changed.
fn changes(texts: impl IntoIterator<Item = String>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for t in texts {
        if out.last() != Some(&t) {
            out.push(t);
        }
    }
    out
}

struct Seen {
    first: Option<popups::Job>,
    texts: Vec<String>,
    gone_after: Option<Duration>,
    cancelled_after: Option<Duration>,
    idle_after: bool,
    woke_during: bool,
    woke_after: bool,
}

fn run(name: &str) -> (Value, Seen) {
    let recording = hydrus_testkit::fixture_json("debug_wake_from_sleep.json");
    assert_eq!(
        recording["menu_path"],
        serde_json::json!(["help", "debug", "debug modes", "simulate a wake from sleep"])
    );
    let run = recording["runs"][name].clone();
    let detect = run["detect"] == true;
    let delay = recording["delay_seconds"].as_u64().unwrap();
    let d = start();
    d.store
        .write_and_refresh(move |ctx| {
            settings::set(
                ctx.conn(),
                &NetworkSettings {
                    detect_sleep: detect,
                    wake_delay_period: delay,
                    ..Default::default()
                },
            )?;
            settings::set(ctx.conn(), &GuiIdleSettings::default())
        })
        .unwrap();
    // long idle by the user's own clock, to see whether the wake resets it
    let monitor = d.bound.session_autosave.clone();
    let now = hydrus_core::TimestampMs::now().0;
    monitor.boot_at(now - 7_200_000);
    monitor.user_at(now - 7_200_000);
    monitor.mouse_at(now - 7_200_000);
    assert!(monitor.idle_at(now), "idle before");
    assert!(the_popup(&d.store).is_none());

    d.click(&["debug modes", "simulate a wake from sleep"]);
    let started = Instant::now();
    let cancel_at = run["cancel_at_ms"]
        .as_f64()
        .map(|ms| Duration::from_secs_f64(ms / 1000.0));
    let mut seen = Seen {
        first: None,
        texts: Vec::new(),
        gone_after: None,
        cancelled_after: None,
        idle_after: true,
        woke_during: false,
        woke_after: false,
    };
    while started.elapsed() < Duration::from_secs(delay + 4) {
        pump(50);
        if let Some(job) = the_popup(&d.store) {
            seen.first.get_or_insert_with(|| job.clone());
            seen.texts.extend(job.status_text_1.clone());
            seen.woke_during |= d.store.wake_ends_at().is_some();
            if let Some(at) = cancel_at
                && seen.cancelled_after.is_none()
                && started.elapsed() >= at
            {
                seen.cancelled_after = Some(started.elapsed());
                let key = job.key;
                d.store
                    .write(move |ctx| popups::update(ctx.conn(), &key, 0, popups::Job::cancel))
                    .unwrap();
            }
        } else if seen.first.is_some() {
            seen.gone_after = Some(started.elapsed());
            break;
        } else if !detect && started.elapsed() > Duration::from_secs(2) {
            break;
        }
    }
    let at = hydrus_core::TimestampMs::now().0;
    seen.idle_after = monitor.idle_at(at);
    seen.woke_after = d.store.wake_ends_at().is_some_and(|ends| ends > at);
    (run, seen)
}

fn recorded_countdown(run: &Value) -> Vec<String> {
    changes(
        run["samples"]
            .as_array()
            .unwrap()
            .iter()
            .take_while(|s| s["dismissed"] != true)
            .filter_map(|s| s["text"].as_str().map(str::to_owned)),
    )
}

// leaf: audit-options-help-debug-action-simulate-a-wake-from-sleep
#[test]
fn simulate_a_wake_shows_the_just_woke_popup_counting_down_and_resets_the_idle_timer() {
    let _one = ONE_AT_A_TIME
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let (recorded, seen) = run("detection on");
    let first = seen.first.expect("the popup appears");
    assert_eq!(first.status_title.as_deref(), recorded["title"].as_str());
    assert_eq!(first.cancellable, recorded["cancellable"] == true);
    assert!(!first.done);
    // the same words in the same order as the reference's countdown
    assert_eq!(changes(seen.texts), recorded_countdown(&recorded));
    assert!(seen.woke_during, "the computer counts as just woken");
    // dismissed when the delay is over, as in the recording (4.1 s of 4)
    let gone = seen.gone_after.expect("it goes").as_secs_f64();
    assert!((3.5..9.0).contains(&gone), "{gone}");
    assert!(!seen.woke_after);
    assert_eq!(recorded["idle_timer_reset"], true);
    assert!(!seen.idle_after, "the idle timer was reset");
}

#[test]
fn cancelling_the_just_woke_popup_enables_input_and_output_at_once() {
    let _one = ONE_AT_A_TIME
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let (recorded, seen) = run("cancelled");
    assert!(seen.first.is_some());
    let cancelled = seen.cancelled_after.expect("it was cancelled");
    // the countdown so far is the start of the recorded one
    let countdown = recorded_countdown(&recorded);
    let mine = changes(seen.texts);
    assert!(countdown.starts_with(&mine), "{mine:?} of {countdown:?}");
    // and it goes about as soon as the reference's does (0.8 s of its thread
    // waking; the bound is loose, the time being real), the delay no longer running
    let gone = seen.gone_after.expect("it goes");
    assert!(
        gone.saturating_sub(cancelled) < Duration::from_secs(4),
        "{gone:?}"
    );
    assert!(!seen.woke_after);
}

#[test]
fn simulate_a_wake_with_detection_off_does_nothing() {
    let _one = ONE_AT_A_TIME
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let (recorded, seen) = run("detection off");
    assert_eq!(recorded["jobs"], 0);
    assert!(seen.first.is_none(), "no popup");
    assert!(!seen.woke_after);
    assert_eq!(recorded["idle_timer_reset"], false);
    assert!(seen.idle_after, "the idle timer is left alone");
}
