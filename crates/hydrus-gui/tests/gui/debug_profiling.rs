//! Help > debug > profiling and the db report mode, clicked in the real menu
//! and read back where the reference puts them: the popups, the menu's
//! checks, and the logs in the database directory
//! (`oracle/fixtures/debug_profiling.json`, recorded by
//! `oracle/record_debug_profiling.py` from the reference client's own menu).
use super::debug_menu_actions::{ONE_AT_A_TIME, capture, start};
use hydrus_core::profiling;
use regex::Regex;
use serde_json::Value;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

fn recording() -> Value {
    hydrus_testkit::fixture_json("debug_profiling.json")
}

/// Numbers made alike: how many jobs there were depends on what else the
/// client was doing.
fn alike(text: &str) -> String {
    Regex::new(r"\d+")
        .unwrap()
        .replace_all(text, "N")
        .into_owned()
}

fn strings(value: &Value) -> Vec<String> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_owned())
        .collect()
}

/// The recorded step called `name`: its messages and the menu's checks.
fn step(name: &str) -> (Vec<String>, Value) {
    let recording = recording();
    let step = recording["sequence"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["step"] == name)
        .unwrap_or_else(|| panic!("no recorded step {name:?}"))
        .clone();
    (strings(&step["messages"]), step["checked"].clone())
}

const DB: &str = "profile mode (db)";
const UI: &str = "profile mode (ui)";
const CLIENT_API: &str = "profile mode (client api)";
const PLANNER: &str = "query planner mode";

/// Everything these tests switch is process-wide: one at a time, from a
/// clean start.
fn alone() -> MutexGuard<'static, ()> {
    let one = ONE_AT_A_TIME
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    profiling::reset();
    one
}

fn checks(d: &super::debug_menu_actions::Debug) -> [bool; 4] {
    [CLIENT_API, DB, UI, PLANNER].map(|label| d.checked_in("profiling", label))
}

fn recorded_checks(checked: &Value) -> [bool; 4] {
    ["client_api", "db", "ui", "planner"].map(|k| checked[k].as_bool().unwrap())
}

fn said(seen: &Arc<Mutex<Vec<String>>>) -> Vec<String> {
    std::mem::take(&mut *seen.lock().unwrap())
}

/// Logs in the store's directory called `client <what> ...`.
fn logs(d: &super::debug_menu_actions::Debug, what: &str) -> Vec<std::path::PathBuf> {
    let mut found: Vec<_> = std::fs::read_dir(d.store.dir())
        .unwrap()
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with(&format!("client {what}")))
        })
        .collect();
    found.sort();
    found
}

// leaf: audit-options-help-debug-action-profile-mode-client-api
// leaf: audit-options-help-debug-action-profile-mode-ui
#[test]
fn profile_modes_turn_each_other_off_and_say_how_many_jobs_there_were() {
    let _one = alone();
    let d = start();
    let seen = capture();
    let recording = recording();
    // the reference's entries, less the threads one (hydrus-rs has no thread
    // pool to profile)
    let labels = strings(&recording["menu_labels"]);
    for label in labels.iter().filter(|l| {
        l.starts_with("profile mode") && !l.contains("threads") || l.starts_with("query")
    }) {
        // (panics if there is no such check item)
        d.checked_in("profiling", label);
    }
    for (name, label) in [
        ("db on", DB),
        ("ui on (ends db)", UI),
        ("client_api on", CLIENT_API),
    ] {
        d.click(&["profiling", label]);
        let (recorded, checked) = step(name);
        // (the client reads the database all the time: how many db jobs
        // there were is not the reference's)
        let said_now: Vec<_> = said(&seen).iter().map(|m| alike(m)).collect();
        assert_eq!(
            said_now,
            recorded.iter().map(|m| alike(m)).collect::<Vec<_>>(),
            "{name}"
        );
        assert_eq!(checks(&d), recorded_checks(&checked), "{name}");
        if name == "ui on (ends db)" {
            // while ui mode is on, a menu click is a job it counts
            d.click(&["data actions", "flush log"]);
            d.click(&["profiling", UI]);
            let (recorded, checked) = step("ui off");
            let said = said(&seen);
            assert_eq!(said.len(), 1, "{said:?}");
            assert_eq!(alike(&said[0]), alike(&recorded[0]));
            assert_eq!(said[0], "Profiling \"ui\" done: 0 slow jobs, 1 fast jobs");
            assert_eq!(checks(&d), recorded_checks(&checked));
        }
    }
    // client api mode ends as the next one starts
    d.click(&["profiling", DB]);
    let (recorded, _) = step("threads on (ends client api)");
    assert_eq!(
        said(&seen).iter().map(|m| alike(m)).collect::<Vec<_>>(),
        [alike(&recorded[0]), "Profile mode \"db\" on!".to_owned()]
    );
    d.click(&["profiling", DB]);
    assert_eq!(said(&seen).len(), 1);
    assert_eq!(checks(&d), [false; 4]);
}

// leaf: audit-options-help-debug-action-profile-mode-db
#[test]
fn db_profile_mode_logs_the_slow_jobs_and_counts_the_fast_ones() {
    let _one = alone();
    let d = start();
    let seen = capture();
    let recording = recording();
    d.click(&["profiling", DB]);
    assert_eq!(said(&seen), strings(&recording["profile_started_with"]));
    for _ in 0..3 {
        d.store.read(|_| Ok(())).unwrap();
    }
    d.store.write(|_| Ok(())).unwrap();
    d.store
        .write(|_| {
            std::thread::sleep(Duration::from_millis(60));
            Ok(())
        })
        .unwrap();
    d.click(&["profiling", DB]);
    let done = said(&seen);
    assert_eq!(done.len(), 1);
    assert_eq!(
        alike(&done[0]),
        alike(&strings(&recording["profile_stopped_with"])[0])
    );
    // one log, called as the reference's is, in the database directory
    let found = logs(&d, "profile");
    assert_eq!(found.len(), 1, "{found:?}");
    let name = found[0].file_name().unwrap().to_str().unwrap();
    assert!(
        Regex::new(recording["profile_log_name_pattern"].as_str().unwrap())
            .unwrap()
            .is_match(name),
        "{name}"
    );
    // each slow job is a line led by the time, then the reference's wording
    let text = std::fs::read_to_string(&found[0]).unwrap();
    let line = Regex::new(r"(?m)^\d{4}-\d\d-\d\d \d\d:\d\d:\d\d: (?:EXCLUSIVE: )?Profiling db job: (.*)$").unwrap();
    let logged: Vec<_> = line.captures_iter(&text).map(|c| c[1].to_owned()).collect();
    assert!(
        logged.iter().any(|job| job.contains("debug_profiling")),
        "the slow write is logged: {logged:?}"
    );
    // the count of slow jobs is the lines in the log (every job is one or
    // the other, and the fast ones are not written down)
    let slow: usize = Regex::new(r"(\d+) slow jobs")
        .unwrap()
        .captures(&done[0])
        .unwrap()[1]
        .parse()
        .unwrap();
    assert_eq!(slow, logged.len());
    assert!(slow >= 1);
    // (the reference follows each with the profiler's table; here, the time,
    // or for a job that ran beside another being profiled, `EXCLUSIVE ... took`)
    assert!(text.contains("\n\nTook ") || text.contains("EXCLUSIVE: "), "{text}");
}

// leaf: audit-options-help-debug-action-query-planner-mode
#[test]
fn query_planner_mode_writes_each_new_query_with_its_plan() {
    let _one = alone();
    let d = start();
    let seen = capture();
    let recording = recording();
    d.click(&["profiling", PLANNER]);
    assert_eq!(said(&seen), strings(&recording["planner_on_messages"]));
    assert!(d.checked_in("profiling", PLANNER));
    let query = "SELECT count(*) FROM texts WHERE text = ?1";
    for _ in 0..2 {
        d.store
            .read(|c| {
                c.query_row(query, ["nothing"], |r| r.get::<_, i64>(0))
                    .map_err(Into::into)
            })
            .unwrap();
    }
    d.click(&["profiling", PLANNER]);
    let done = said(&seen);
    assert_eq!(done.len(), 1);
    assert_eq!(
        alike(&done[0]),
        alike(&strings(&recording["planner_off_messages"])[0])
    );
    assert!(!d.checked_in("profiling", PLANNER));
    let found = logs(&d, "query planner");
    assert_eq!(found.len(), 1, "{found:?}");
    let name = found[0].file_name().unwrap().to_str().unwrap();
    assert!(
        Regex::new(recording["planner_log_name_pattern"].as_str().unwrap())
            .unwrap()
            .is_match(name),
        "{name}"
    );
    // the recorded block: "<time>: SELECT", the query, then one tuple per
    // line of the plan
    let text = std::fs::read_to_string(&found[0]).unwrap();
    let blocks: Vec<_> = text.split("\n\n").filter(|b| !b.is_empty()).collect();
    let mine: Vec<_> = blocks.iter().filter(|b| b.contains(query)).collect();
    assert_eq!(mine.len(), 1, "run twice, written once: {text}");
    let lines: Vec<_> = mine[0].split('\n').collect();
    assert!(
        Regex::new(r"^\d{4}-\d\d-\d\d \d\d:\d\d:\d\d: SELECT$")
            .unwrap()
            .is_match(lines[0]),
        "{lines:?}"
    );
    assert_eq!(lines[1], query);
    let recorded_plan = recording["planner_first_blocks"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|b| b.as_str())
        .find(|b| b.contains("(3, 0, 0,"))
        .unwrap()
        .to_owned();
    let tuple = Regex::new(r"^\(\d+, \d+, \d+, '.*'\)$").unwrap();
    assert!(
        recorded_plan.split('\n').skip(2).all(|l| tuple.is_match(l)),
        "the recorded plan rows are tuples"
    );
    assert!(lines.len() > 2, "{lines:?}");
    assert!(lines[2..].iter().all(|l| tuple.is_match(l)), "{lines:?}");
    // every block is unique
    let mut queries: Vec<_> = blocks
        .iter()
        .map(|b| b.split('\n').nth(1).unwrap_or_default())
        .collect();
    let count = queries.len();
    queries.sort_unstable();
    queries.dedup();
    assert_eq!(queries.len(), count);
    // and the message counts the distinct queries it analysed (other tests'
    // databases are planned too, while the mode is on)
    let analysed: usize = Regex::new(r"(\d+) queries")
        .unwrap()
        .captures(&done[0])
        .unwrap()[1]
        .parse()
        .unwrap();
    assert!(analysed >= 1, "{done:?}");
}

// leaf: audit-options-help-debug-action-db-report-mode
#[test]
fn db_report_mode_names_each_job_as_it_runs() {
    let _one = alone();
    let d = start();
    let seen = capture();
    let recording = recording();
    assert!(!d.report_mode_checked("db report mode"));
    d.click(&["report modes", "db report mode"]);
    assert!(d.report_mode_checked("db report mode"));
    said(&seen);
    d.store.read(|_| Ok(())).unwrap();
    d.store.write(|_| Ok(())).unwrap();
    d.click(&["report modes", "db report mode"]);
    let reports = said(&seen);
    let prefix = strings(&recording["db_report_messages"])[0]
        .split("job: ")
        .next()
        .unwrap()
        .to_owned()
        + "job: ";
    let mine: Vec<_> = reports
        .iter()
        .filter(|r| r.contains("debug_profiling"))
        .collect();
    assert_eq!(mine.len(), 2, "{reports:?}");
    assert!(mine.iter().all(|r| r.starts_with(&prefix)));
    // switched off, nothing more is said
    d.store.read(|_| Ok(())).unwrap();
    assert!(said(&seen).is_empty());
}

// leaf: audit-options-help-debug-action-profile-mode-client-api
#[test]
fn client_api_profile_mode_counts_each_request() {
    use tower::ServiceExt as _;
    let _one = alone();
    let d = start();
    let seen = capture();
    let recording = recording();
    let key = [71u8; 32];
    let permissions = hydrus_api::auth::AccessPermissions {
        access_key: key.to_vec(),
        name: "profile test".into(),
        permits_everything: true,
        basic: Default::default(),
        search_filter: hydrus_core::TagFilter::default(),
    };
    d.store
        .write(move |ctx| hydrus_api::auth::save_key(ctx.conn(), &permissions))
        .unwrap();
    let router = hydrus_api::router(hydrus_api::AppState::new(d.store.clone()).unwrap());
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let get = |path: &str| {
        let request = axum::http::Request::builder()
            .uri(path)
            .header("Hydrus-Client-API-Access-Key", hex::encode(key))
            .body(axum::body::Body::empty())
            .unwrap();
        runtime.block_on(router.clone().oneshot(request)).unwrap()
    };
    // before the mode, a request is not counted
    get("/verify_access_key");
    d.click(&["profiling", CLIENT_API]);
    let (recorded, _) = step("client_api on");
    assert_eq!(said(&seen), recorded);
    for _ in 0..3 {
        assert!(get("/verify_access_key").status().is_success());
    }
    // a request made slow for certain: the database's readers are held
    // (authenticating reads the database) for a quarter of a second
    let holder = {
        let store = d.store.clone();
        std::thread::spawn(move || {
            let paused = store.pause().unwrap();
            std::thread::sleep(Duration::from_millis(250));
            drop(paused);
        })
    };
    std::thread::sleep(Duration::from_millis(60));
    assert!(get("/verify_access_key").status().is_success());
    holder.join().unwrap();
    d.click(&["profiling", CLIENT_API]);
    let done = said(&seen);
    assert_eq!(done.len(), 1);
    assert_eq!(
        alike(&done[0]),
        alike(&step("threads on (ends client api)").0[0])
    );
    // this database's own requests: one slow, three fast
    assert_eq!(done[0], "Profiling \"client_api\" done: 1 slow jobs, 3 fast jobs");
    // the slow one is logged by the reference's wording, in a log named as its
    let logged = logs(&d, "profile");
    assert_eq!(logged.len(), 1, "{logged:?}");
    let text = std::fs::read_to_string(&logged[0]).unwrap();
    let lines = Regex::new(r"(?m)^\d{4}-\d\d-\d\d \d\d:\d\d:\d\d: (?:EXCLUSIVE: )?Profiling client api: /verify_access_key")
        .unwrap()
        .find_iter(&text)
        .count();
    assert_eq!(lines, 1, "{text}");
    assert!(
        Regex::new(&recording["profile_log_name_pattern"].as_str().unwrap().replace("db", "client_api"))
            .unwrap()
            .is_match(logged[0].file_name().unwrap().to_str().unwrap())
    );
}
