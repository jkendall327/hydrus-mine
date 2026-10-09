//! The idle box of Options > maintenance and processing against the
//! reference's own `SystemBusy`, `CurrentlyIdle` and
//! `GoodTimeToStartBackgroundWork`, replaying
//! `oracle/fixtures/cpu_busy.json` (`oracle/record_cpu_busy.py`): each
//! recorded case sets its options through the real window, feeds the
//! recorded `/proc/stat` texts to the real maintenance runtime at the
//! recorded times, and compares what it says (busy, idle, whether background
//! work may start) with what the reference said.

use std::cell::RefCell;
use std::rc::Rc;

use hydrus_store::idle_state;

use super::options_system_consumers::{Client, client, row_in};

const IDLE_ENABLED: &str =
    "Run maintenance jobs when the client is idle and the system is not otherwise busy: ";
const CPU_PERCENT: &str = "Consider the system busy if CPU usage is above: ";
const CPU_CORES: &str = "% on ";

/// The core count to set, if any.
#[derive(Clone, Copy)]
enum Cores {
    Leave,
    Ignore,
    Count(i64),
}

impl Cores {
    fn of(value: &serde_json::Value) -> Self {
        value.as_i64().map_or(Self::Ignore, Self::Count)
    }
}

/// The idle box's options, set through File > options and applied.
fn set(client: &Client, percent: Option<i64>, cores: Cores, idle: Option<bool>) {
    let options = client.options("maintenance and processing");
    if let Some(on) = idle {
        let (i, _) = row_in(&options, "idle", IDLE_ENABLED);
        options.invoke_check_toggled(i, on);
        // no activity timeouts: idle as soon as allowed
        for label in [
            "Permit idle mode if no general browsing activity has occurred in the past: ",
            "Permit idle mode if your mouse cursor has not been moved in the past: ",
            "Permit idle mode if no Client API requests in the past: ",
        ] {
            let (i, _) = row_in(&options, "idle", label);
            options.invoke_none_toggled(i, true);
        }
    }
    if let Some(percent) = percent {
        let (i, _) = row_in(&options, "idle", CPU_PERCENT);
        options.invoke_number_edited(i, percent as i32);
    }
    let (i, _) = row_in(&options, "idle", CPU_CORES);
    match cores {
        Cores::Leave => {}
        Cores::Ignore => options.invoke_none_toggled(i, true),
        Cores::Count(count) => {
            options.invoke_number_edited(i, count as i32);
            options.invoke_none_toggled(i, false);
        }
    }
    options.invoke_apply();
}

// leaf: audit-options-maintenance-and-processing-when-to-run-high-cpu-jobs-idle-run-maintenance-jobs-when-the-client-is-idle-and-the-system-is-not-otherwise-busy
// leaf: audit-options-maintenance-and-processing-when-to-run-high-cpu-jobs-idle-consider-the-system-busy-if-cpu-usage-is-above
// leaf: audit-options-maintenance-and-processing-when-to-run-high-cpu-jobs-idle-on
#[test]
fn idle_and_cpu_busy_options_decide_whether_background_work_may_run_as_recorded() {
    let recorded = hydrus_testkit::fixture_json("cpu_busy.json");
    let cases = recorded.as_array().unwrap();
    assert_eq!(cases.len(), 29);
    let mut client = client();
    for case in cases {
        let name = case["name"].as_str().unwrap();
        // the window bound afresh: its runtime has not looked at the CPU yet
        client.bound = hydrus_gui::bind(
            &client.ui,
            hydrus_gui::Pages::open(client.store.clone()).unwrap(),
        );
        set(
            &client,
            Some(case["percent"].as_i64().unwrap()),
            Cores::of(&case["count"]),
            Some(case["idle_on"].as_bool().unwrap()),
        );
        let saved: hydrus_store::settings::GuiIdleSettings = client.get();
        assert_eq!(
            (
                saved.enabled,
                i64::from(saved.busy_cpu_percent),
                saved.busy_cpu_count.map(i64::from)
            ),
            (
                case["idle_on"].as_bool().unwrap(),
                case["percent"].as_i64().unwrap(),
                case["count"].as_i64()
            ),
            "{name}"
        );
        let stat = Rc::new(RefCell::new(case["baseline"].as_str().unwrap().to_owned()));
        client.bound.maintenance.use_proc_stat({
            let stat = stat.clone();
            move || stat.borrow().clone()
        });
        let dir = client.store.dir().to_owned();
        // long after the client started, so idle is allowed
        let base = hydrus_core::TimestampMs::now().0 + 10_000_000;
        // the baseline look (the reference's psutil had its first look then)
        client.bound.maintenance.poll_at(base).unwrap();
        for step in case["steps"].as_array().unwrap() {
            let ms = step["ms"].as_i64().unwrap();
            let change = &step["change"];
            if change.as_object().is_some_and(|c| !c.is_empty()) {
                set(
                    &client,
                    change["percent"].as_i64(),
                    change.get("count").map_or(Cores::Leave, Cores::of),
                    None,
                );
            }
            *stat.borrow_mut() = step["stat"].as_str().unwrap().to_owned();
            let at = base + ms;
            client.bound.maintenance.poll_at(at).unwrap();
            let busy = step["busy"].as_bool().unwrap();
            let idle = step["idle"].as_bool().unwrap();
            let good = step["good_time_for_background_work"].as_bool().unwrap();
            let context = format!("{name} at {ms} ms");
            assert_eq!(
                client.bound.maintenance.system_busy(),
                busy,
                "busy: {context}"
            );
            assert_eq!(
                client.ui.get_status_busy(),
                if busy { "CPU busy" } else { "" },
                "{context}"
            );
            assert_eq!(
                client.ui.get_status_idle(),
                if idle { "idle" } else { "" },
                "idle: {context}"
            );
            assert_eq!(
                idle_state::is_idle(&dir, at),
                good,
                "background work: {context}"
            );
        }
    }
}

#[test]
fn the_idle_box_rows_are_the_references() {
    let client = client();
    let options = client.options("maintenance and processing");
    let (_, percent) = row_in(&options, "idle", CPU_PERCENT);
    assert_eq!(
        (
            percent.kind,
            percent.minimum,
            percent.maximum,
            percent.number
        ),
        (2, 5, 99, 50)
    );
    let (_, cores) = row_in(&options, "idle", CPU_CORES);
    assert_eq!(
        (
            cores.kind,
            cores.none_phrase.as_str(),
            cores.minimum,
            cores.maximum,
            cores.number
        ),
        (3, "ignore cpu usage", 1, 64, 1)
    );
    // the percent is no use while the core count is none
    assert!(percent.enabled);
    set(&client, None, Cores::Ignore, None);
    let options = client.options("maintenance and processing");
    assert!(!row_in(&options, "idle", CPU_PERCENT).1.enabled);
    options.invoke_cancel();
    set(&client, None, Cores::Count(2), None);
    let options = client.options("maintenance and processing");
    assert!(row_in(&options, "idle", CPU_PERCENT).1.enabled);
    options.invoke_cancel();
    // the status bar's tooltips are the reference's
    let stat = |busy: u64| {
        format!(
            "cpu  0 0 0 0\ncpu0 {busy} 0 0 {} 0 0 0 0 0 0\n",
            1000 - busy
        )
    };
    set(&client, Some(50), Cores::Count(1), Some(true));
    let text = Rc::new(RefCell::new(stat(0)));
    client.bound.maintenance.use_proc_stat({
        let text = text.clone();
        move || text.borrow().clone()
    });
    let base = hydrus_core::TimestampMs::now().0 + 10_000_000;
    client.bound.maintenance.poll_at(base).unwrap();
    *text.borrow_mut() = format!("cpu  0 0 0 0\ncpu0 {} 0 0 {} 0 0 0 0 0 0\n", 90, 1010);
    client.bound.maintenance.poll_at(base + 60_001).unwrap();
    assert_eq!(client.ui.get_status_busy(), "CPU busy");
    assert_eq!(
        client.ui.get_status_idle_tip(),
        "client is idle, it can do maintenance work"
    );
    assert_eq!(
        client.ui.get_status_busy_tip(),
        "this computer has been doing work recently, so some hydrus maintenance will not start"
    );
}
