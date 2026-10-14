//! Help > debug's report modes: the switches, and the reports the model
//! crate makes when they are on.
use hydrus_core::debug_flags::{self, Flag};
use hydrus_core::external_calls::{ActualCall, Inputs, Process};
use hydrus_gui_model::external_calls::test_call;
use std::sync::{Arc, Mutex};

/// The sink and the flags are process-wide: one test at a time.
static ONE_AT_A_TIME: Mutex<()> = Mutex::new(());

fn capture() -> Arc<Mutex<Vec<String>>> {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let sink = seen.clone();
    debug_flags::set_sink(Some(Box::new(move |text| {
        sink.lock().unwrap().push(text.to_owned());
    })));
    seen
}

#[test]
fn a_switch_flips_and_only_reports_while_on() {
    let _one = ONE_AT_A_TIME
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let seen = capture();
    assert!(!Flag::IdleReport.is_on(), "off at boot");
    debug_flags::report(Flag::IdleReport, || "not shown".into());
    assert!(Flag::IdleReport.flip());
    debug_flags::report(Flag::IdleReport, || "shown".into());
    assert!(!Flag::IdleReport.flip());
    debug_flags::report(Flag::IdleReport, || "not shown either".into());
    assert_eq!(*seen.lock().unwrap(), ["shown"]);
    debug_flags::set_sink(None);
}

#[test]
fn a_program_run_says_what_the_reference_says_while_subprocess_report_mode_is_on() {
    let _one = ONE_AT_A_TIME
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let seen = capture();
    let recording = hydrus_testkit::fixture_json("debug_report_modes.json");
    let recorded_env: Vec<(String, String)> = recording["subprocess"]["environment"]
        .as_object()
        .unwrap()
        .iter()
        .map(|(k, v)| (k.clone(), v.as_str().unwrap().to_owned()))
        .collect();
    for run in recording["subprocess"]["commands"].as_array().unwrap() {
        let cmd: Vec<String> = run["cmd"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().to_owned())
            .collect();
        let call = ActualCall::Process(Process {
            executable: cmd[0].clone(),
            arguments: cmd[1..].to_vec(),
            ..Process::default()
        });
        let on = run["on"] == true;
        Flag::SubprocessReport.set(on);
        seen.lock().unwrap().clear();
        test_call(&call, &Inputs::new()).unwrap();
        Flag::SubprocessReport.set(false);
        let said = std::mem::take(&mut *seen.lock().unwrap());
        let recorded: Vec<&str> = run["messages"]
            .as_array()
            .unwrap()
            .iter()
            .map(|m| m["text"].as_str().unwrap())
            .collect();
        assert_eq!(said.len(), recorded.len(), "{cmd:?}: {said:?}");
        if !on {
            continue;
        }
        // the environment, as the reference words it (its own text, for the
        // environment it ran in, is what the formatter must reproduce), then
        // this process's
        assert_eq!(
            hydrus_core::debug_flags::env_text(recorded_env.clone(), ':'),
            recorded[0]
        );
        assert_eq!(said[0], hydrus_core::debug_flags::current_env_text());
        // how the call is made: the reference's words, its Python
        // keyword-argument dict being this program's own arguments
        assert!(recorded[1].starts_with("KWargs are: "));
        assert!(said[1].starts_with("KWargs are: "), "{}", said[1]);
        for part in &cmd {
            assert!(said[1].contains(&format!("{part:?}")), "{}", said[1]);
        }
        // what came back: the same words, a Python repr's quotes being Rust's
        for i in 2..recorded.len() {
            assert_eq!(said[i], recorded[i].replace('\'', "\""), "{cmd:?}");
        }
    }
    debug_flags::set_sink(None);
}

/// What the reference's `DataCache` said for the same run
/// (`oracle/record_debug_report_modes.py`): its keys were the strings `a` to
/// `d`; here they are the hash ids 1 to 4.
fn recorded_cache_steps(which: &str) -> Vec<(String, u64, Vec<String>)> {
    let recording = hydrus_testkit::fixture_json("debug_report_modes.json");
    recording["cache"][which]
        .as_array()
        .unwrap()
        .iter()
        .map(|step| {
            let name = step["step"].as_str().unwrap().to_owned();
            let messages = step["messages"]
                .as_array()
                .unwrap()
                .iter()
                .map(|m| {
                    let mut text = m["text"].as_str().unwrap().to_owned();
                    for (key, id) in [("a", 1), ("b", 2), ("c", 3), ("d", 4)] {
                        text = text.replace(&format!("\"{key}\""), &format!("\"{id}\""));
                    }
                    text.replace("oracle cache", "thumbnail")
                })
                .collect();
            let size = name
                .split(['(', ' '])
                .find_map(|part| part.parse::<u64>().ok())
                .unwrap_or(0);
            (name, size, messages)
        })
        .collect()
}

#[test]
fn the_cache_says_what_the_reference_cache_says_while_its_mode_is_on() {
    use hydrus_core::HashId;
    use hydrus_gui_model::thumbnail_cache::Cache;
    use hydrus_store::settings::ThumbnailCacheSettings;
    use std::time::Duration;

    let _one = ONE_AT_A_TIME
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let seen = capture();
    let recording = hydrus_testkit::fixture_json("debug_report_modes.json");
    let limit = recording["cache"]["size_limit"].as_u64().unwrap();
    for (mode_on, which) in [(false, "off"), (true, "on")] {
        Flag::CacheReport.set(mode_on);
        let mut cache = Cache::new(ThumbnailCacheSettings {
            bytes: limit,
            timeout: 1200,
        });
        for (name, size, expected) in recorded_cache_steps(which) {
            seen.lock().unwrap().clear();
            if let Some(key) = name.strip_prefix("add ") {
                let id = HashId(u32::from(key.as_bytes()[0] - b'a') + 1);
                cache.insert(id, (), size, Duration::ZERO);
            } else {
                let id = HashId(u32::from(name.as_bytes()["delete ".len()] - b'a') + 1);
                cache.remove_if(id, |()| true);
            }
            assert_eq!(*seen.lock().unwrap(), expected, "{which}: {name}");
        }
    }
    Flag::CacheReport.set(false);
    debug_flags::set_sink(None);
}
