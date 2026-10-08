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

// leaf: audit-options-help-debug-action-subprocess-report-mode
#[test]
fn subprocess_report_mode_reports_the_call_before_it_is_made() {
    let _one = ONE_AT_A_TIME
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let seen = capture();
    let process = Process {
        executable: "true".into(),
        arguments: vec!["--probe".into()],
        ..Process::default()
    };
    let call = ActualCall::Process(process);
    test_call(&call, &Inputs::new()).unwrap();
    assert!(
        seen.lock().unwrap().is_empty(),
        "silent while the mode is off"
    );
    Flag::SubprocessReport.set(true);
    test_call(&call, &Inputs::new()).unwrap();
    Flag::SubprocessReport.set(false);
    let seen = seen.lock().unwrap();
    assert_eq!(seen.len(), 1);
    assert!(seen[0].starts_with("KWargs are: "), "{}", seen[0]);
    assert!(seen[0].contains("\"true\", \"--probe\""), "{}", seen[0]);
    debug_flags::set_sink(None);
}
