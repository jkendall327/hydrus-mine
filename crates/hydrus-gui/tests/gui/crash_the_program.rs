//! Help > debug > tests, do not touch > "induce a program crash"
//! (`ClientGUI.CrashTheProgram`): the question the reference asks
//! (oracle/record_crash_question.py), and "nice crash" ending the process
//! with a panic that the always-on crash log records.
use super::debug_menu_actions::{ONE_AT_A_TIME, start};
use hydrus_gui::message_window;
use hydrus_gui_model::debug_actions as model;
use slint::{ComponentHandle as _, Model as _};

const CHILD: &str = "HYDRUS_CRASH_TEST_DIR";
const THIS: &str = "crash_the_program::nice_crash_ends_a_process_and_the_crash_log_holds_the_panic";

/// The question on screen, as the reference's dialog shows it: its title, its
/// text and its buttons in order (the accepting ones, then the "no").
fn asked(d: &super::debug_menu_actions::Debug) -> (String, String, Vec<String>) {
    d.click(&["tests, do not touch", "induce a program crash"]);
    let window = message_window().expect("the question is asked");
    assert!(window.window().is_visible());
    let mut buttons: Vec<String> = window.get_choices().iter().map(|c| c.to_string()).collect();
    buttons.push(window.get_no_label().to_string());
    (
        window.get_window_title().to_string(),
        window.get_message().to_string(),
        buttons,
    )
}

fn recorded() -> serde_json::Value {
    hydrus_testkit::fixture_json("crash_question.json")
}

// (not a leaf tag by itself: the process-ending half is the subprocess test below)
#[test]
fn the_question_is_the_references_and_only_not_a_nice_crash_panics_a_worker() {
    let _one = ONE_AT_A_TIME
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let dir = tempfile::tempdir().unwrap();
    hydrus_core::crash_log::install(dir.path(), "hydrus-gui-test");
    let d = start();
    let recording = recorded();
    for case in recording["cases"].as_array().unwrap() {
        let seen = &case["dialog"];
        let (title, message, buttons) = asked(&d);
        assert_eq!(title, seen["title"].as_str().unwrap());
        assert_eq!(
            vec![message],
            serde_json::from_value::<Vec<String>>(seen["text"].clone()).unwrap()
        );
        let labels: Vec<String> = seen["buttons"]
            .as_array()
            .unwrap()
            .iter()
            .map(|b| b["label"].as_str().unwrap().to_owned())
            .collect();
        assert_eq!(buttons, labels);
        // what the reference does on "forget it" or a closed dialog: nothing
        let window = message_window().unwrap();
        match case["press"].as_str().unwrap() {
            "forget it" | "closed" => {
                assert!(case["calls"].as_array().unwrap().is_empty());
                window.invoke_cancelled();
            }
            "not a nice crash" => {
                assert_eq!(case["calls"][0], "CallToThread:crashtime_not_nice");
                // a worker panics: the process goes on, and the log has it
                window.invoke_chosen(1);
            }
            // ("nice crash" ends the process: the test below)
            _ => {
                assert_eq!(case["calls"][0], "os.abort");
                window.invoke_cancelled();
            }
        }
    }
    let log = dir.path().join(hydrus_core::crash_log::LOG_NAME);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    let text = loop {
        let text = std::fs::read_to_string(&log).unwrap_or_default();
        if text.contains(model::CRASH_NOT_NICE_PANIC) {
            break text;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "the worker's panic was never logged"
        );
        std::thread::sleep(std::time::Duration::from_millis(20));
    };
    assert!(text.contains("thread: crashtime-not-nice"), "{text}");
    // (and nothing else crashed: not "forget it", not a closed question,
    // not the nice crash that was only looked at)
    assert_eq!(text.matches("==== panic ====").count(), 1, "{text}");
    hydrus_core::crash_log::uninstall();
}

// leaf: audit-options-help-debug-action-induce-a-program-crash
#[test]
fn nice_crash_ends_a_process_and_the_crash_log_holds_the_panic() {
    if let Ok(dir) = std::env::var(CHILD) {
        // the child: the window, the menu, the question, "nice crash"
        hydrus_core::crash_log::install(std::path::Path::new(&dir), "hydrus-gui-test");
        let d = start();
        let recording = recorded();
        let recorded_nice = &recording["cases"][0];
        assert_eq!(recorded_nice["press"], "nice crash");
        let (_, _, buttons) = asked(&d);
        assert_eq!(buttons[0], "nice crash");
        message_window().unwrap().invoke_chosen(0);
        unreachable!("the process should have panicked");
    }
    let dir = tempfile::tempdir().unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", THIS, "--nocapture", "--test-threads=1"])
        .env(CHILD, dir.path())
        .output()
        .unwrap();
    assert!(
        !output.status.success(),
        "a nice crash ends the process badly: {output:?}"
    );
    let log = std::fs::read_to_string(dir.path().join(hydrus_core::crash_log::LOG_NAME))
        .unwrap_or_else(|e| {
            panic!(
                "no crash log: {e}\n{}",
                String::from_utf8_lossy(&output.stderr)
            )
        });
    assert!(log.contains("==== panic ===="), "{log}");
    assert!(log.contains(model::CRASH_PANIC), "{log}");
    assert!(log.contains("program: hydrus-gui-test"), "{log}");
    assert!(log.contains("backtrace:"), "{log}");
    assert_eq!(log.matches("==== panic ====").count(), 1, "{log}");
}
