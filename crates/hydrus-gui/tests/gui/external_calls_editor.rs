//! The external programs page and the callable editor's strings, input rules,
//! process flags and executable checks, against the reference panels in
//! `hydrus/client/gui/panels/options/ExternalProgramsPanel.py`.
use super::external_calls::{call, named, open, question, saved, seed, store};
use hydrus_core::external_calls::{ActualCall, Parameter, Pipeline};
use hydrus_core::url::strings::{Conversion, ProcessingStep, StringConverter};
use hydrus_gui::{MainWindow, Pages, bind, headless};
use slint::{ComponentHandle as _, Model as _};
use std::{cell::RefCell, rc::Rc};

fn edit_seeded(
    store: &Arc<hydrus_store::Store>,
) -> (
    MainWindow,
    hydrus_gui::Bound,
    hydrus_gui::OptionsWindow,
    hydrus_gui::ExternalCallWindow,
) {
    seed(store);
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    let w = open(&ui, &bound);
    w.invoke_external_call_clicked(named(&w, "synthetic call"), false, false);
    w.invoke_external_call_action("edit".into());
    let child = call(&bound);
    (ui, bound, w, child)
}
use std::sync::Arc;

fn wait(child: &hydrus_gui::ExternalCallWindow) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(8);
    while child.get_testing() {
        assert!(std::time::Instant::now() < deadline);
        slint::platform::update_timers_and_animations();
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
}

// leaf: audit-options-external-programs-help-for-this-panel
#[test]
fn help_button_opens_the_external_programs_documentation() {
    let (_dirs, store) = store();
    let _windows = headless::init();
    let launched = Rc::new(RefCell::new(Vec::new()));
    hydrus_gui::set_launcher({
        let launched = launched.clone();
        move |target| launched.borrow_mut().push(target.to_owned())
    });
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store).unwrap());
    let w = open(&ui, &bound);
    // HC.DOCUMENTATION_EXTERNAL_PROGRAMS = 'external_programs.html'
    w.invoke_external_call_action("help".into());
    assert_eq!(
        launched.borrow().as_slice(),
        ["https://hydrusnetwork.github.io/hydrus/external_programs.html"]
    );
    // Help is not a list edit: nothing was opened or changed.
    assert!(!bound.options_external_calls.has_open());
}

// leaf: audit-options-nested-external-call-input-rules
#[test]
fn input_rules_enable_token_and_string_processor_reach_the_saved_call() {
    let (_dirs, store) = store();
    let _windows = headless::init();
    let (_ui, bound, w, child) = edit_seeded(&store);
    // One rule row per parameter of the "send single file" job, in reference order.
    let labels = |c: &hydrus_gui::ExternalCallWindow| {
        c.get_rules()
            .iter()
            .map(|r| (r.label.to_string(), r.enabled, r.token.to_string()))
            .collect::<Vec<_>>()
    };
    assert_eq!(
        labels(&child),
        [
            ("file path".to_owned(), true, "%path%".to_owned()),
            ("file URI".to_owned(), false, "%path_uri%".to_owned()),
            ("file hash (sha256)".to_owned(), false, "%hash%".to_owned()),
            ("file id".to_owned(), false, "%file_id%".to_owned()),
        ]
    );
    assert_eq!(
        child.get_pipeline_description(),
        "Summary: This tells the client how to open a file in another program.\n\nAvailable input parameters: file path, file URI, file hash (sha256), file id\n\nExpected output parameters: none"
    );
    // Enabling a parameter whose token is not in the template is vetoed.
    child.invoke_rule_enabled(1, true);
    assert_eq!(
        child.get_validity(),
        "The replacement string \"%path_uri%\" for input parameter \"file URI\" is not in the command template!"
    );
    child.invoke_rule_enabled(1, false);
    assert_eq!(child.get_validity(), "Everything looks good!");
    // Renaming the token means it is no longer in the template either.
    child.invoke_rule_token(0, "%renamed%".into());
    assert!(child.get_validity().contains("\"%renamed%\""));
    child.invoke_rule_token(0, "%path%".into());
    // A string processor sits between the input and its token.
    child.invoke_rule_process(0);
    assert!(
        bound
            .options_external_calls
            .strings
            .processor
            .borrow()
            .is_some()
    );
    let editor = bound
        .options_external_calls
        .strings
        .processor
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    editor.invoke_cancel();
    assert!(
        bound
            .options_external_calls
            .strings
            .processor
            .borrow()
            .is_none()
    );
    // The saved call carries each rule's token and processor.
    let mut seeded = saved(&store);
    let ActualCall::Process(p) = &mut seeded.calls[0].call else {
        unreachable!()
    };
    p.rules[0]
        .processor
        .steps
        .push(ProcessingStep::Convert(StringConverter {
            conversions: vec![Conversion::Prepend("pre-".into())],
            ..StringConverter::default()
        }));
    p.rules[0].token = "%path%".into();
    let with_processor = seeded.clone();
    child.invoke_cancel();
    w.invoke_cancel();
    store
        .write(move |ctx| hydrus_store::settings::set(ctx.conn(), &with_processor))
        .unwrap();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    let w = open(&ui, &bound);
    w.invoke_external_call_clicked(named(&w, "synthetic call"), false, false);
    w.invoke_external_call_action("edit".into());
    let child = call(&bound);
    assert_eq!(
        child.get_rules().row_data(0).unwrap().summary,
        "some conversion"
    );
    child.invoke_test_input(0, "x.png".into());
    assert!(
        child.get_preview().ends_with("pre-x.png"),
        "{}",
        child.get_preview()
    );
    child.invoke_rule_enabled(0, false);
    child.invoke_rule_enabled(3, true);
    child.invoke_rule_token(3, "%path%".into());
    child.invoke_apply();
    w.invoke_apply();
    let ActualCall::Process(p) = &saved(&store).calls[0].call else {
        unreachable!()
    };
    assert_eq!(p.rules.len(), 1);
    assert_eq!(p.rules[0].parameter, Parameter::FileId);
    assert_eq!(p.rules[0].token, "%path%");
}

// leaf: audit-options-nested-external-call-process-timeout
#[test]
fn timeout_long_lived_terminal_and_text_flags_are_saved_on_the_process() {
    let (_dirs, store) = store();
    let _windows = headless::init();
    let (ui, bound, w, child) = edit_seeded(&store);
    // The reference defaults.
    assert_eq!(child.get_timeout(), 15);
    assert!(!child.get_long_lived() && child.get_hide_terminal() && child.get_output_text());
    child.set_timeout(90);
    child.invoke_changed();
    child.set_hide_terminal(false);
    child.set_output_text(false);
    child.invoke_apply();
    w.invoke_apply();
    let ActualCall::Process(p) = &saved(&store).calls[0].call else {
        unreachable!()
    };
    assert_eq!(
        (p.timeout_seconds, p.long_lived, p.hide_terminal, p.text),
        (90, false, false, false)
    );
    // "this can live for a very long time" (a None timeout) saves the long-lived marker
    // with the reference's fixed 15 second timeout.
    let w = open(&ui, &bound);
    w.invoke_external_call_clicked(named(&w, "synthetic call"), false, false);
    w.invoke_external_call_action("edit".into());
    let child = call(&bound);
    assert_eq!(child.get_timeout(), 90);
    assert!(!child.get_hide_terminal() && !child.get_output_text());
    child.set_long_lived(true);
    child.invoke_changed();
    child.invoke_apply();
    w.invoke_apply();
    let ActualCall::Process(p) = &saved(&store).calls[0].call else {
        unreachable!()
    };
    assert_eq!((p.timeout_seconds, p.long_lived), (15, true));
}

// leaf: audit-options-nested-external-call-command-path
#[test]
fn executable_path_checks_and_show_path_help_match_the_reference() {
    let (_dirs, store) = store();
    let _windows = headless::init();
    let (_ui, bound, w, child) = edit_seeded(&store);
    child.invoke_show_path();
    let q = question(&bound);
    let message = q.get_message().to_string();
    assert!(message.starts_with("As hydrus sees it, your PATH is as follows. Any executable you specify with just a name, rather than a full path, needs to exist in one of these locations. You should be very very careful in ever editing your PATH. Ask a chatbot if you need to learn more. Recall that if you ever do change it, you need to restart hydrus (in a new terminal if needed) to see the changes here.\n\n"));
    let path = std::env::var("PATH").unwrap();
    for part in std::env::split_paths(&path) {
        assert!(message.contains(part.to_string_lossy().as_ref()));
    }
    q.invoke_answered(true);
    // The command window asks about an empty path, then about a path `which` cannot find.
    child.invoke_command_edit();
    let command = bound
        .options_external_calls
        .command
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    command.set_executable("".into());
    command.invoke_apply();
    assert_eq!(
        question(&bound).get_message(),
        "Hey, you really need to put an exe name/path in the path box. Are you sure you want to save this?"
    );
    question(&bound).invoke_answered(true);
    assert_eq!(
        question(&bound).get_message(),
        "Hey, I looked for the executable path \"\" but did not see it with a \"which\" call. You sure you are good?"
    );
    question(&bound).invoke_answered(false);
    command.set_executable("no-such-program-anywhere".into());
    command.invoke_apply();
    assert_eq!(
        question(&bound).get_message(),
        "Hey, I looked for the executable path \"no-such-program-anywhere\" but did not see it with a \"which\" call. You sure you are good?"
    );
    question(&bound).invoke_answered(true);
    assert!(bound.options_external_calls.command.borrow().is_none());
    assert!(
        child
            .get_command_template()
            .starts_with("no-such-program-anywhere ")
    );
    // Typing a bare name that is on the PATH asks nothing.
    child.invoke_command_edit();
    let command = bound
        .options_external_calls
        .command
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    command.set_executable("sh".into());
    command.invoke_apply();
    assert!(bound.options_external_calls.command.borrow().is_none());
    assert!(child.get_command_template().starts_with("sh "));
    child.invoke_cancel();
    w.invoke_cancel();
}

// leaf: audit-options-nested-external-call-test-availability
#[test]
fn availability_test_is_a_which_call_with_the_reference_messages() {
    let (_dirs, store) = store();
    let _windows = headless::init();
    let (_ui, bound, w, child) = edit_seeded(&store);
    // The seeded executable is this test binary, a full path that exists.
    child.invoke_test(true);
    wait(&child);
    assert_eq!(child.get_test_status(), "Availability test worked!");
    child.invoke_command_edit();
    let command = bound
        .options_external_calls
        .command
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    command.set_executable("no-such-program-anywhere".into());
    command.invoke_apply();
    question(&bound).invoke_answered(true);
    child.invoke_test(true);
    wait(&child);
    assert_eq!(child.get_test_status(), "Availability test failed!");
    child.invoke_cancel();
    w.invoke_cancel();
    let _ = Pipeline::File;
}

/// Reference export text with the generated keys blanked, for comparison.
fn without_keys(value: &serde_json::Value) -> serde_json::Value {
    let mut value = value.clone();
    fn walk(v: &mut serde_json::Value) {
        if let serde_json::Value::Array(a) = v {
            // [serialisable type, version, [name, key, ...]]: the key is a 64 hex string.
            for item in a.iter_mut() {
                if let serde_json::Value::String(s) = item
                    && s.len() == 64
                    && s.chars().all(|c| c.is_ascii_hexdigit())
                {
                    *s = String::new();
                }
                walk(item);
            }
        }
    }
    walk(&mut value);
    value
}

// leaf: audit-options-external-programs-external-calls-export
// leaf: audit-options-external-programs-external-calls-add
#[test]
fn add_creates_a_new_call_and_export_writes_the_selected_calls_as_the_reference_does() {
    let (_dirs, store) = store();
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    let w = open(&ui, &bound);
    // Export is off with nothing selected, then follows the selection.
    assert!(!w.get_external_call_selected());
    w.invoke_external_call_action("import".into());
    let exchange = bound
        .options_external_calls
        .exchange
        .0
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    let reference = hydrus_testkit::fixture_json("external_calls.json");
    exchange.set_text(reference["export"].as_str().unwrap().into());
    exchange.invoke_action("review".into());
    exchange.invoke_action("accept".into());
    bound.options_external_calls.exchange.cancel();
    let rows = w.get_external_call_rows().row_count();
    assert_eq!(rows, 2);
    // Imported calls arrive selected, as in the reference.
    assert!(w.get_external_call_selected());
    w.invoke_external_call_action("export".into());
    let exchange = bound
        .options_external_calls
        .exchange
        .0
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    let exported: serde_json::Value = serde_json::from_str(exchange.get_text().as_str()).unwrap();
    let expected: serde_json::Value =
        serde_json::from_str(reference["export"].as_str().unwrap()).unwrap();
    // Same list in the same shape; only the generated keys differ.
    assert_eq!(without_keys(&exported), without_keys(&expected));
    bound.options_external_calls.exchange.cancel();
    // Add opens the editor on a fresh "new call" and Apply appends it to the list.
    w.invoke_external_call_action("add".into());
    let child = call(&bound);
    assert_eq!(child.get_name(), "new call");
    child.set_name("brand new".into());
    child.invoke_apply();
    // The fresh call has no executable yet, so the reference asks first.
    assert!(
        question(&bound)
            .get_message()
            .starts_with("Hey, it looks like something is not quite right here.")
    );
    question(&bound).invoke_answered(true);
    assert_eq!(w.get_external_call_rows().row_count(), 3);
    w.invoke_apply();
    assert!(saved(&store).calls.iter().any(|c| c.name == "brand new"));
}
