//! Setting a watcher's checker options, against the reference's, recorded
//! by `oracle/record_watcher_checker.py`: different ones time the next
//! check again (and may find the thread dead, pausing checking), the same
//! change nothing; and the file velocity is worded by the new ones.

use serde_json::Value as Json;

use hydrus_core::subscriptions::{CheckerOptions, SeedTime};
use hydrus_core::watchers::{CheckerStatus, WatcherState};

fn recorded() -> Json {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../oracle/fixtures/watcher_checker.json"
    );
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

fn options(v: &Json) -> CheckerOptions {
    CheckerOptions {
        intended_files_per_check: v[0].as_f64().unwrap(),
        never_faster_than: v[1].as_i64().unwrap(),
        never_slower_than: v[2].as_i64().unwrap(),
        death_file_velocity: (v[3][0].as_i64().unwrap(), v[3][1].as_i64().unwrap()),
    }
}

fn status(n: &Json) -> CheckerStatus {
    match n.as_i64().unwrap() {
        0 => CheckerStatus::Ok,
        1 => CheckerStatus::Dead,
        2 => CheckerStatus::NotFound,
        other => panic!("{other}"),
    }
}

/// How the watcher differs from the recorded state, if it does.
fn problem(watcher: &WatcherState, seeds: &[SeedTime], theirs: &Json) -> Option<String> {
    let ours = (
        watcher.checker.clone(),
        watcher.next_check_time,
        watcher.status,
        watcher.checking_paused,
        watcher
            .checker
            .pretty_current_velocity(seeds, watcher.last_check_time),
    );
    let expected = (
        options(&theirs["checker"]),
        theirs["next_check_time"].as_i64().unwrap(),
        status(&theirs["status"]),
        theirs["checking_paused"].as_bool().unwrap(),
        theirs["file_velocity_status"].as_str().unwrap().to_owned(),
    );
    (ours != expected).then(|| format!("ours {ours:?}, theirs {expected:?}"))
}

#[test]
fn checker_options_set_on_a_watcher_act_as_the_references() {
    let recorded = recorded();
    let now = recorded["now"].as_i64().unwrap();
    for case in recorded["watchers"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let seeds: Vec<SeedTime> = case["seeds"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| SeedTime {
                source_time: s[0].as_i64().map(|ago| now - ago),
                created: now - s[1].as_i64().unwrap(),
            })
            .collect();
        let before = &case["before"];
        let mut watcher = WatcherState::new(
            "https://example.com/thread/1",
            options(&before["checker"]),
            now,
        );
        watcher.last_check_time = case["last_check_time"].as_i64().unwrap();
        let extra = &case["extra"];
        watcher.check_now = extra["check_now"] == true;
        watcher.no_work_until = extra["no_work_until"].as_i64().map_or(0, |s| now + s);
        watcher.status = extra.get("status").map_or(CheckerStatus::Ok, status);
        watcher.checking_paused = watcher.status != CheckerStatus::Ok;
        watcher.update_next_check_time(&seeds, now);
        if let Some(why) = problem(&watcher, &seeds, before) {
            panic!("{name}, as made: {why}");
        }
        watcher.set_checker_options(options(&case["set"]), &seeds, now);
        if let Some(why) = problem(&watcher, &seeds, &case["after"]) {
            panic!("{name}, set: {why}");
        }
    }
}

/// The same checker options change nothing: not even a next check time
/// set otherwise.
#[test]
fn the_same_checker_options_change_nothing() {
    let checker = CheckerOptions::default();
    let mut watcher = WatcherState::new("https://example.com/thread/1", checker.clone(), 1000);
    watcher.next_check_time = 12345;
    watcher.set_checker_options(checker, &[], 1000);
    assert_eq!(watcher.next_check_time, 12345);
}
