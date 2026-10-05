//! Checked runtime facts and actual reference worker entry eligibility.
use hydrus_gui_model::main_menu::{self, Command, Entry, Facts};
use hydrus_store::maintenance_gates::{Preferences, Worker};
fn find<'a>(entries: &'a [Entry], labels: &[&str]) -> &'a Entry {
    let entry = entries
        .iter()
        .find(|e| e.label().trim_start_matches('&') == labels[0])
        .unwrap();
    if labels.len() == 1 {
        return entry;
    }
    let Entry::Menu { entries, .. } = entry else {
        panic!("not a menu")
    };
    find(entries, &labels[1..])
}
#[test]
fn actual_toggle_checks_and_real_worker_entry_matrix() {
    let fixture = hydrus_testkit::fixture_json("force_idle_mode.json");
    for case in fixture["toggles"].as_array().unwrap() {
        for (field, checked) in [("before", "checked"), ("after", "reopened")] {
            let facts = Facts {
                force_idle: case[field].as_bool().unwrap(),
                ..Default::default()
            };
            let menu = main_menu::menubar(&facts);
            let Entry::Check {
                command,
                checked: actual,
                ..
            } = find(&menu, &["help", "debug", "debug modes", "force idle mode"])
            else {
                panic!("not a checkbox")
            };
            assert_eq!(command.as_ref(), Some(&Command::DebugForceIdleMode));
            assert_eq!(*actual, case[checked].as_bool().unwrap());
        }
    }
    for case in fixture["worker_entries"].as_array().unwrap() {
        let worker = if case["worker"] == "trash" {
            Worker::Trash
        } else {
            Worker::Deferred
        };
        let normal = case["normal"].as_bool().unwrap();
        let preferences = Preferences {
            trash_normal: normal,
            deferred_normal: normal,
        };
        let idle = case["forced"].as_bool().unwrap() || case["normally_idle"].as_bool().unwrap();
        let entered = case["events"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e[0] == "read");
        assert_eq!(preferences.allows(worker, idle), entered, "{case}");
    }
}
