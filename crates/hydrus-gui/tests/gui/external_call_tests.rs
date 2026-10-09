//! The callable editor's 'testing' box (test availability, test inputs, test
//! call), through the real Options > external programs > edit window and its
//! worker, replayed against `oracle/fixtures/external_call_tests.json`
//! (`oracle/record_external_call_tests.py`, the reference's
//! `TestCallablePanel` clicked for the same calls).
use super::external_calls::{call, named, open, saved, store};
use hydrus_core::external_calls::{
    ActualCall, Callable, Manager, Parameter, Pipeline, Process, Rule,
};
use hydrus_gui::{ExternalCallWindow, MainWindow, Pages, bind, headless};
use serde_json::Value;
use slint::Model as _;

/// The recorded call, as the native definition the editor opens.
fn recorded_call(recorded: &Value) -> (Pipeline, ActualCall) {
    match recorded["kind"].as_str().unwrap() {
        "process" => {
            let rules = recorded["parameters"]
                .as_array()
                .unwrap()
                .iter()
                .map(|p| Rule::new(Parameter::from_code(p.as_i64().unwrap()).unwrap()))
                .collect::<Vec<_>>();
            let pipeline = if rules.iter().any(|r| r.parameter == Parameter::Url) {
                Pipeline::Url
            } else {
                Pipeline::File
            };
            let process = Process {
                executable: recorded["executable"].as_str().unwrap().to_owned(),
                arguments: recorded["arguments"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|a| a.as_str().unwrap().to_owned())
                    .collect(),
                rules,
                long_lived: recorded["long_lived"].as_bool().unwrap(),
                ..Process::default()
            };
            (pipeline, ActualCall::Process(process))
        }
        "ExecutableLocalProcessDefaultLaunchFile" => (Pipeline::File, ActualCall::DefaultFile),
        "ExecutableLocalProcessDefaultLaunchURL" => (Pipeline::Url, ActualCall::DefaultUrl),
        other => panic!("{other}"),
    }
}

struct Editing {
    _dirs: [tempfile::TempDir; 2],
    _ui: MainWindow,
    _bound: hydrus_gui::Bound,
    options: hydrus_gui::OptionsWindow,
    child: ExternalCallWindow,
}

/// Options > external programs, the recorded call saved, selected and edited.
fn edit(recorded: &Value) -> Editing {
    let (dirs, store) = store();
    let (pipeline, actual) = recorded_call(recorded);
    let mut c = Callable::new("recorded test call");
    c.pipeline = pipeline;
    c.call = actual;
    let manager = Manager { calls: vec![c] };
    store
        .write(move |ctx| hydrus_store::settings::set(ctx.conn(), &manager))
        .unwrap();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    let options = open(&ui, &bound);
    options.invoke_external_call_clicked(named(&options, "recorded test call"), false, false);
    options.invoke_external_call_action("edit".into());
    let child = call(&bound);
    assert!(saved(&store).calls.len() == 1);
    Editing {
        _dirs: dirs,
        _ui: ui,
        _bound: bound,
        options,
        child,
    }
}

/// The test input rows the call uses: (label, value).
fn rows(child: &ExternalCallWindow) -> Vec<(String, String)> {
    child
        .get_rules()
        .iter()
        .filter(|r| r.enabled)
        .map(|r| (r.label.to_string(), r.value.to_string()))
        .collect()
}

fn recorded_rows(rows: &Value) -> Vec<(String, String)> {
    rows.as_array()
        .unwrap()
        .iter()
        .map(|r| {
            (
                r["name"].as_str().unwrap().to_owned(),
                r["value"].as_str().unwrap().to_owned(),
            )
        })
        .collect()
}

/// A test button is enabled as the editor's own binding says.
fn enabled(child: &ExternalCallWindow) -> bool {
    child.get_call_type() == 0 && !child.get_testing() && !child.get_child_open()
}

fn settle(child: &ExternalCallWindow) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    while child.get_testing() {
        assert!(
            std::time::Instant::now() < deadline,
            "the test did not finish: {}",
            child.get_test_status()
        );
        slint::platform::update_timers_and_animations();
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
}

/// The input rows, preview and buttons each recorded call opens with.
// leaf: audit-options-nested-external-call-test-execution
#[test]
fn test_inputs_and_preview_start_as_the_reference_s_for_each_call() {
    let recorded = hydrus_testkit::fixture_json("external_call_tests.json");
    let _windows = headless::init();
    for setup in recorded["setups"].as_array().unwrap() {
        let editing = edit(&setup["call"]);
        let child = &editing.child;
        assert_eq!(rows(child), recorded_rows(&setup["rows"]), "{setup}");
        assert_eq!(child.get_preview().as_str(), setup["preview"], "{setup}");
        assert_eq!(child.get_test_status().as_str(), setup["output"]);
        assert_eq!(
            enabled(child),
            setup["availability_enabled"].as_bool().unwrap(),
            "{setup}"
        );
        // (the reference also offers 'test call!' for the OS launchers, which
        // opens the example path for real; hydrus-rs does not: DIFFERENCES.md)
        if setup["call"]["kind"] == "process" {
            assert_eq!(
                enabled(child),
                setup["test_call_enabled"].as_bool().unwrap()
            );
        }
        child.invoke_cancel();
        editing.options.invoke_cancel();
    }
}

/// 'test availability!': its interim text and disabled button, then the
/// verdict, for a call on the PATH, a full path and a missing program.
// leaf: audit-options-nested-external-call-test-availability
#[test]
fn test_availability_replays_the_reference_s_texts_and_button_states() {
    let recorded = hydrus_testkit::fixture_json("external_call_tests.json");
    let _windows = headless::init();
    for case in recorded["availability"].as_array().unwrap() {
        let editing = edit(&case["call"]);
        let child = &editing.child;
        assert_eq!(enabled(child), case["enabled"].as_bool().unwrap(), "{case}");
        if enabled(child) {
            child.invoke_test(true);
            assert_eq!(child.get_test_status().as_str(), case["interim"]["output"]);
            assert_eq!(
                enabled(child),
                case["interim"]["enabled"].as_bool().unwrap()
            );
            settle(child);
            assert_eq!(child.get_test_status().as_str(), case["output"], "{case}");
            assert_eq!(enabled(child), case["enabled_after"].as_bool().unwrap());
        }
        child.invoke_cancel();
        editing.options.invoke_cancel();
    }
}

/// 'test call!': typed inputs run the call for real, with the reference's
/// interim text (the forced timeout for a long-lived call), its success text
/// and its error texts for a program that is missing and one that fails.
// leaf: audit-options-nested-external-call-test-execution
#[test]
fn test_call_runs_the_call_and_reports_as_the_reference_does() {
    let recorded = hydrus_testkit::fixture_json("external_call_tests.json");
    let recorded_path = recorded["output_path"].as_str().unwrap();
    let _windows = headless::init();
    for case in recorded["calls"].as_array().unwrap() {
        let owned = tempfile::tempdir().unwrap();
        let path = owned.path().join("owned output.txt");
        let path = path.to_str().unwrap();
        let ours = |text: &str| text.replace(recorded_path, path);
        let editing = edit(&case["call"]);
        let child = &editing.child;
        child.invoke_test_input(0, ours(case["input"].as_str().unwrap()).into());
        assert_eq!(
            child.get_preview().as_str(),
            ours(case["preview"].as_str().unwrap())
        );
        child.invoke_test(false);
        assert_eq!(child.get_test_status().as_str(), case["interim"]["output"]);
        assert_eq!(
            enabled(child),
            case["interim"]["enabled"].as_bool().unwrap()
        );
        settle(child);
        assert_eq!(
            child.get_test_status().as_str(),
            ours(case["output"].as_str().unwrap()),
            "{}",
            case["name"]
        );
        assert_eq!(enabled(child), case["enabled_after"].as_bool().unwrap());
        assert_eq!(
            std::fs::read_to_string(path).ok().as_deref(),
            case["written"].as_str(),
            "{}",
            case["name"]
        );
        child.invoke_cancel();
        editing.options.invoke_cancel();
    }

    // typing a test value redoes the preview and clears the last result
    let typed = &recorded["typed"];
    let mut failing = recorded["calls"][0]["call"].clone();
    failing["arguments"][2] = "pre:%path%".into();
    let editing = edit(&failing);
    let child = &editing.child;
    child.invoke_test(false);
    settle(child);
    assert!(
        child.get_test_status().starts_with(
            "BadReturnCodeException: A call to another executable gave a non-zero return code (2)!"
        ),
        "{} / {}",
        child.get_test_status(),
        typed["output_before"]
    );
    child.invoke_test_input(0, "/typed/漢 value.png".into());
    assert_eq!(child.get_test_status().as_str(), typed["output"]);
    assert_eq!(child.get_preview().as_str(), typed["preview"]);
    child.invoke_cancel();
    editing.options.invoke_cancel();
}
