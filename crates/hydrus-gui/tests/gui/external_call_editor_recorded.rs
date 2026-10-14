//! The external call editor in the options window against the reference's own
//! panels (`oracle/fixtures/external_call_editor.json`, from
//! `oracle/record_external_call_editor.py`): the executable path questions and
//! the PATH message, the input parameter rows and the vetoes, the timeout and
//! the other process flags, and Add.
use super::external_calls::{call, named, open, question, saved, seed, store};
use hydrus_core::external_calls::{ActualCall, Callable};
use hydrus_gui::{MainWindow, Pages, bind, headless};
use serde_json::Value;
use slint::{ComponentHandle as _, Model as _};

fn recording() -> Value {
    hydrus_testkit::fixture_json("external_call_editor.json")
}

fn rule_rows(c: &hydrus_gui::ExternalCallWindow) -> Value {
    serde_json::json!(
        c.get_rules()
            .iter()
            .map(|r| serde_json::json!([r.label.as_str(), r.enabled, r.token.as_str()]))
            .collect::<Vec<_>>()
    )
}

fn validity(c: &hydrus_gui::ExternalCallWindow) -> Value {
    let text = c.get_validity().to_string();
    if text == "Everything looks good!" {
        Value::Null
    } else {
        Value::String(text)
    }
}

/// The seeded call opened in the call editor, from a fresh window.
fn edit_seeded(
    store: &std::sync::Arc<hydrus_store::Store>,
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

// leaf: audit-options-nested-external-call-command-path
#[test]
fn the_executable_path_questions_and_the_path_message_are_the_recorded_ones() {
    let rec = recording();
    let (_dirs, store) = store();
    let _windows = headless::init();
    let (_ui, bound, w, child) = edit_seeded(&store);

    // show PATH: the recorded wording (verified as wording only: the PATH itself
    // differs by machine), then one line per PATH entry
    let shown = rec["show_path"]["messages"][0].as_str().unwrap();
    let (words, recorded_entries) = shown.split_once("\n\n").unwrap();
    assert_eq!(
        recorded_entries,
        rec["show_path"]["path"]
            .as_str()
            .unwrap()
            .replace(':', "\n"),
        "the recording lists its PATH one entry a line"
    );
    child.invoke_show_path();
    let q = question(&bound);
    let message = q.get_message().to_string();
    let entries = std::env::split_paths(&std::env::var_os("PATH").unwrap())
        .map(|p| p.to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join("\n");
    assert_eq!(message, format!("{words}\n\n{entries}"));
    q.invoke_answered(true);

    // the questions asked on Apply, for each path and scripted answers
    for case in rec["command"].as_array().unwrap() {
        let path = case["path"].as_str().unwrap();
        child.invoke_command_edit();
        let command = bound
            .options_external_calls
            .command
            .borrow()
            .as_ref()
            .unwrap()
            .clone_strong();
        command.set_executable(path.into());
        command.invoke_apply();
        let answers = case["answers"].as_array().unwrap();
        for (asked, answer) in case["asked"].as_array().unwrap().iter().zip(answers) {
            assert_eq!(
                question(&bound).get_message(),
                asked.as_str().unwrap(),
                "{case}"
            );
            question(&bound).invoke_answered(answer.as_bool().unwrap());
        }
        let closed = bound.options_external_calls.command.borrow().is_none();
        assert_eq!(closed, case["ok"].as_bool().unwrap(), "{case}");
        if closed {
            assert!(
                child
                    .get_command_template()
                    .starts_with(&format!("{path} ")),
                "{}",
                child.get_command_template()
            );
        } else {
            command.invoke_cancel();
        }
    }
    child.invoke_cancel();
    w.invoke_cancel();
}

// (Not tagged for audit-options-nested-external-call-process-timeout: the control matches the
// recording, but nothing tests the runtime enforcing the timeout or text mode; DIFFERENCES.md.)
#[test]
fn the_timeout_and_process_flags_save_and_reopen_as_the_reference_does() {
    let rec = recording();
    let flags = &rec["timeouts"];
    let (_dirs, store) = store();
    let _windows = headless::init();
    let (ui, bound, w, child) = edit_seeded(&store);
    let defaults = &flags["defaults"];
    assert_eq!(
        i64::from(child.get_timeout()),
        defaults["timeout"].as_i64().unwrap()
    );
    assert_eq!(
        (
            child.get_long_lived(),
            child.get_hide_terminal(),
            child.get_output_text()
        ),
        (
            defaults["long_lived"].as_bool().unwrap(),
            defaults["hide_terminal"].as_bool().unwrap(),
            defaults["text"].as_bool().unwrap()
        ),
        "the reference's defaults"
    );
    child.invoke_cancel();
    w.invoke_cancel();

    // what each entered timeout (whole seconds a user can type) saves as
    let state = (ui, bound);
    for entered in flags["entered"].as_array().unwrap() {
        let seconds = &entered["entered"];
        // none (long-lived) is below; a half second cannot be typed into a whole-second
        // box, and the recorded clamp for 0 and 0.5 (both save 1) is shown by 0
        let Some(seconds) = seconds.as_i64() else {
            continue;
        };
        if seconds >= 3600 {
            continue; // not enterable: minutes stop at 59 (loaded values, below)
        }
        let w = open(&state.0, &state.1);
        w.invoke_external_call_clicked(named(&w, "synthetic call"), false, false);
        w.invoke_external_call_action("edit".into());
        let child = call(&state.1);
        // (set directly, so a 0 here shows the clamp on save, not the box's own minimum)
        child.set_timeout(i32::try_from(seconds).unwrap());
        child.invoke_changed();
        child.invoke_apply();
        w.invoke_apply();
        let ActualCall::Process(p) = &saved(&store).calls[0].call else {
            unreachable!()
        };
        assert_eq!(
            (p.timeout_seconds, p.long_lived),
            (
                entered["timeout"].as_u64().unwrap(),
                entered["long_lived"].as_bool().unwrap()
            ),
            "{entered}"
        );
        assert_eq!(
            i64::try_from(p.timeout_seconds).unwrap(),
            entered["widget"].as_i64().unwrap()
        );
    }
    // "this can live for a very long time": the reference saves 15 seconds and the marker
    let none = flags["entered"][0].clone();
    assert!(none["entered"].is_null());
    let w = open(&state.0, &state.1);
    w.invoke_external_call_clicked(named(&w, "synthetic call"), false, false);
    w.invoke_external_call_action("edit".into());
    let child = call(&state.1);
    child.set_long_lived(true);
    child.invoke_changed();
    child.invoke_apply();
    w.invoke_apply();
    let ActualCall::Process(p) = &saved(&store).calls[0].call else {
        unreachable!()
    };
    assert_eq!(
        (p.timeout_seconds, p.long_lived),
        (
            none["timeout"].as_u64().unwrap(),
            none["long_lived"].as_bool().unwrap()
        )
    );

    // the flags toggled, saved and reopened
    let w = open(&state.0, &state.1);
    w.invoke_external_call_clicked(named(&w, "synthetic call"), false, false);
    w.invoke_external_call_action("edit".into());
    let child = call(&state.1);
    child.set_hide_terminal(!child.get_hide_terminal());
    child.set_output_text(!child.get_output_text());
    child.invoke_changed();
    child.invoke_apply();
    w.invoke_apply();
    let ActualCall::Process(p) = &saved(&store).calls[0].call else {
        unreachable!()
    };
    let toggled = &flags["flags_toggled"];
    assert_eq!(
        (p.hide_terminal, p.text),
        (
            toggled["hide_terminal"].as_bool().unwrap(),
            toggled["text"].as_bool().unwrap()
        )
    );

    // a stored timeout the control cannot hold reopens as the reference's does
    for entered in flags["entered"].as_array().unwrap() {
        let Some(seconds) = entered["entered"].as_u64().filter(|s| *s >= 3600) else {
            continue;
        };
        let mut manager = saved(&store);
        let ActualCall::Process(p) = &mut manager.calls[0].call else {
            unreachable!()
        };
        p.timeout_seconds = seconds;
        p.long_lived = false;
        store
            .write(move |ctx| hydrus_store::settings::set(ctx.conn(), &manager))
            .unwrap();
        let w = open(&state.0, &state.1);
        w.invoke_external_call_clicked(named(&w, "synthetic call"), false, false);
        w.invoke_external_call_action("edit".into());
        let child = call(&state.1);
        assert_eq!(
            i64::from(child.get_timeout()),
            entered["widget"].as_i64().unwrap(),
            "{entered}"
        );
        child.invoke_cancel();
        w.invoke_cancel();
    }
}

// leaf: audit-options-external-programs-external-calls-add
#[test]
fn add_opens_a_new_call_that_is_named_keyed_and_asked_about_as_recorded() {
    let rec = recording();
    let add = &rec["add"];
    let (_dirs, store) = store();
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    let w = open(&ui, &bound);
    let rows = |w: &hydrus_gui::OptionsWindow| -> Vec<Vec<String>> {
        w.get_external_call_rows()
            .iter()
            .map(|r| r.cells.iter().map(|c| c.to_string()).collect())
            .collect()
    };
    let names = |w: &hydrus_gui::OptionsWindow| -> Vec<String> {
        rows(w).into_iter().map(|r| r[0].clone()).collect()
    };
    for (i, expected) in add["names_after_each_add"]
        .as_array()
        .unwrap()
        .iter()
        .enumerate()
    {
        w.invoke_external_call_action("add".into());
        let child = call(&bound);
        if i == 0 {
            assert_eq!(child.get_name(), add["name"].as_str().unwrap());
            assert_eq!(
                child.get_pipeline_description(),
                add["description"].as_str().unwrap()
            );
            assert_eq!(validity(&child), add["validity"]);
        }
        // Apply is asked about, as the reference asks (declined, then accepted)
        child.invoke_apply();
        assert_eq!(
            question(&bound).get_message(),
            add["ok_declined"][1][0].as_str().unwrap()
        );
        if i == 0 {
            question(&bound).invoke_answered(false);
            assert!(bound.options_external_calls.editor.borrow().is_some());
            assert!(!add["ok_declined"][0].as_bool().unwrap());
            child.invoke_apply();
            assert_eq!(
                question(&bound).get_message(),
                add["ok_accepted"][1][0].as_str().unwrap()
            );
        }
        question(&bound).invoke_answered(true);
        assert!(bound.options_external_calls.editor.borrow().is_none());
        let mut now: Vec<String> = names(&w)
            .into_iter()
            .filter(|n| n.starts_with("new call"))
            .collect();
        now.sort();
        let expected: Vec<String> = expected
            .as_array()
            .unwrap()
            .iter()
            .map(|n| n.as_str().unwrap().to_owned())
            .collect();
        assert_eq!(now, expected);
    }
    // the new call's row, and each one's own key
    let row = rows(&w).into_iter().find(|r| r[0] == "new call").unwrap();
    assert_eq!(
        row,
        add["row"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| c.as_str().unwrap().to_owned())
            .collect::<Vec<_>>()
    );
    w.invoke_apply();
    let calls: Vec<Callable> = saved(&store).calls;
    let keys: std::collections::BTreeSet<_> = calls
        .iter()
        .filter(|c| c.name.starts_with("new call"))
        .map(|c| c.key)
        .collect();
    assert_eq!(keys.len(), 2, "{}", add["keys_unique"]);
    assert!(add["keys_unique"].as_bool().unwrap());
}

/// Not tagged (audit-options-nested-external-call-input-rules): the rows, the
/// pipeline description, the vetoes and the string processor buttons' labels
/// are the recorded ones, but a processor is not edited through the child
/// editor here (it is put in the store and the editor reopened), so the leaf's
/// String Processor part is only half shown.
#[test]
fn the_input_rows_vetoes_and_processor_button_labels_are_the_recorded_ones() {
    use hydrus_core::url::strings::{Conversion, ProcessingStep, StringConverter};
    let rec = recording();
    let rules = &rec["rules"];
    let (_dirs, store) = store();
    let _windows = headless::init();
    let (ui, bound, w, child) = edit_seeded(&store);
    assert_eq!(rule_rows(&child), rules["rows"]);
    assert_eq!(validity(&child), rules["veto"]);
    assert_eq!(
        child.get_pipeline_description(),
        rec["add"]["description"].as_str().unwrap()
    );
    for step in rules["steps"].as_array().unwrap() {
        match step["do"].as_str().unwrap() {
            "enable file URI" => child.invoke_rule_enabled(1, true),
            "disable file URI" => child.invoke_rule_enabled(1, false),
            "rename the file path token" => child.invoke_rule_token(0, "%renamed%".into()),
            "disable every input" => {
                child.invoke_rule_token(0, "%path%".into());
                child.invoke_rule_enabled(0, false);
            }
            "no executable" => {
                child.invoke_rule_enabled(0, true);
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
                question(&bound).invoke_answered(true);
                question(&bound).invoke_answered(true);
            }
            other => panic!("{other}"),
        }
        // (the token box keeps what was typed; its row model is not re-read)
        if step["do"] != "rename the file path token" {
            assert_eq!(rule_rows(&child), step["rows"], "{step}");
        }
        assert_eq!(validity(&child), step["veto"], "{step}");
    }
    child.invoke_cancel();
    w.invoke_cancel();

    // the buttons: no processing says so; a conversion lists its step
    let labels = |c: &hydrus_gui::ExternalCallWindow| -> Vec<String> {
        c.get_rules()
            .iter()
            .map(|r| r.summary.to_string())
            .collect()
    };
    let buttons = rules["processor_buttons"].as_array().unwrap();
    let w = open(&ui, &bound);
    w.invoke_external_call_clicked(named(&w, "synthetic call"), false, false);
    w.invoke_external_call_action("edit".into());
    let child = call(&bound);
    assert_eq!(
        labels(&child),
        buttons
            .iter()
            .map(|b| b["default_label"].as_str().unwrap().to_owned())
            .collect::<Vec<_>>()
    );
    child.invoke_cancel();
    w.invoke_cancel();
    let mut manager: hydrus_core::external_calls::Manager = saved(&store);
    let ActualCall::Process(p) = &mut manager.calls[0].call else {
        unreachable!()
    };
    for rule in &mut p.rules {
        rule.processor
            .steps
            .push(ProcessingStep::Convert(StringConverter {
                conversions: vec![Conversion::Prepend("pre-".into())],
                ..StringConverter::default()
            }));
    }
    store
        .write(move |ctx| hydrus_store::settings::set(ctx.conn(), &manager))
        .unwrap();
    let w = open(&ui, &bound);
    w.invoke_external_call_clicked(named(&w, "synthetic call"), false, false);
    w.invoke_external_call_action("edit".into());
    let child = call(&bound);
    assert_eq!(
        labels(&child)[0],
        buttons[0]["label_with_a_conversion"].as_str().unwrap()
    );
    child.invoke_cancel();
    w.invoke_cancel();
}
