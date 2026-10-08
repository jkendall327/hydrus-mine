use hydrus_core::external_calls::{
    ActualCall, Callable, Inputs, Manager, Parameter, Process, Rule, clean_arguments,
};
use hydrus_gui_model::external_calls::{Table, available, test_call};

#[test]
fn actual_reference_substitution_validation_and_argument_cleaning() {
    let reference = hydrus_testkit::fixture_json("external_calls.json");
    for case in reference["clean_cases"].as_array().unwrap() {
        let input: Vec<String> = serde_json::from_value(case[0].clone()).unwrap();
        assert_eq!(
            serde_json::to_value(clean_arguments(&input)).unwrap(),
            case[1]
        );
    }
    let p = Process {
        executable: "owned-program".into(),
        arguments: vec!["pre:%path%:%path%".into(), "%path%".into()],
        rules: vec![Rule::new(Parameter::Path)],
        ..Process::default()
    };
    let inputs = Inputs::from([(Parameter::Path, vec!["/synthetic folder/漢😀.png".into()])]);
    assert_eq!(
        ActualCall::Process(p.clone()).preview(&inputs),
        reference["preview"].as_str().unwrap()
    );
    assert_eq!(
        ActualCall::Process(p.clone()).preview(&Inputs::new()),
        reference["missing_preview"].as_str().unwrap()
    );
    let unused = Process {
        arguments: vec!["literal".into()],
        ..p
    };
    assert_eq!(
        ActualCall::Process(unused).preview(&inputs),
        reference["unused_preview"].as_str().unwrap()
    );
}
#[test]
fn list_acceptance_retains_edit_key_and_selection_but_regenerates_import_and_duplicate_keys() {
    let original = Callable::new("synthetic call");
    let original_key = original.key;
    let mut table = Table::new(Manager::default());
    table.add(original.clone());
    table.add(original);
    assert_eq!(
        table
            .manager
            .calls
            .iter()
            .map(|c| c.name.as_str())
            .collect::<Vec<_>>(),
        ["synthetic call", "synthetic call (1)"]
    );
    assert!(table.manager.calls.iter().all(|c| c.key != original_key));
    assert_ne!(table.manager.calls[0].key, table.manager.calls[1].key);
    table.click(0, false, false);
    let key = table.selected()[0].key;
    let edited = Callable::new("edited 日本");
    assert!(table.replace(key, edited));
    assert_eq!(table.selected()[0].key, key);
    table.duplicate();
    assert_eq!(table.manager.calls.len(), 3);
    assert_eq!(table.selected().len(), 2);
    assert!(table.selected().iter().any(|c| c.key == key));
    table.click(2, false, false);
    table.delete(&[key]);
    assert_eq!(table.manager.calls.len(), 2);
    assert!(!table.manager.calls.iter().any(|c| c.key == key));
}
#[test]
fn saved_call_executes_owned_harmless_process_with_exact_single_argument_and_no_shell_interpretation()
 {
    let dir = tempfile::tempdir().unwrap();
    let output = dir.path().join("owned synthetic output.txt");
    #[cfg(not(windows))]
    let process = Process {
        executable: "/bin/sh".into(),
        arguments: vec![
            "-c".into(),
            "printf '%s' \"$1\" > \"$2\"".into(),
            "owned-fixture".into(),
            "%path%".into(),
            output.to_string_lossy().into_owned(),
        ],
        rules: vec![Rule::new(Parameter::Path)],
        ..Process::default()
    };
    #[cfg(windows)]
    let process = {
        let script = dir.path().join("owned-argument-fixture.cmd");
        std::fs::write(&script, "@chcp 65001 >nul\r\n@echo \"%~1\">\"%~2\"\r\n").unwrap();
        Process {
            executable: "cmd.exe".into(),
            arguments: vec![
                "/D".into(),
                "/C".into(),
                script.to_string_lossy().into_owned(),
                "%path%".into(),
                output.to_string_lossy().into_owned(),
            ],
            rules: vec![Rule::new(Parameter::Path)],
            ..Process::default()
        }
    };
    let native = tempfile::tempdir().unwrap();
    let store = hydrus_store::Store::open(native.path()).unwrap();
    let mut callable = Callable::new("saved harmless fixture");
    callable.call = ActualCall::Process(process);
    let manager = Manager {
        calls: vec![callable],
    };
    store
        .write(move |ctx| hydrus_store::settings::set(ctx.conn(), &manager))
        .unwrap();
    let reopened = hydrus_store::Store::open(native.path()).unwrap();
    let saved: Manager = reopened.read(hydrus_store::settings::get).unwrap();
    let call = saved.calls[0].call.clone();
    assert!(available(&call));
    let value = "synthetic 日本; $(this must stay text)";
    test_call(
        &call,
        &Inputs::from([(Parameter::Path, vec![value.into()])]),
    )
    .unwrap();
    #[cfg(not(windows))]
    assert_eq!(std::fs::read_to_string(output).unwrap(), value);
    #[cfg(windows)]
    assert_eq!(
        std::fs::read_to_string(output).unwrap().trim(),
        format!("\"{value}\"")
    );
}

#[test]
fn actual_qt_sort_tie_break_casefold_and_duplicate_selection_routes() {
    let reference = hydrus_testkit::fixture_json("external_calls.json");
    let calls = reference["sort_input"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|v| {
            hydrus_downloader_exchange::external_calls::decode_text(&v.to_string()).unwrap()
        })
        .collect::<Vec<_>>();
    let mut table = Table::new(Manager { calls });
    for case in reference["sort_cases"].as_array().unwrap() {
        table.sort(
            usize::try_from(case["column"].as_u64().unwrap()).unwrap(),
            case["ascending"].as_bool().unwrap(),
        );
        assert_eq!(
            serde_json::to_value(
                table
                    .manager
                    .calls
                    .iter()
                    .map(|c| c.name.as_str())
                    .collect::<Vec<_>>()
            )
            .unwrap(),
            case["names"]
        );
    }
    let row = table
        .manager
        .calls
        .iter()
        .position(|c| c.name == "alpha")
        .unwrap();
    table.click(row, false, false);
    table.duplicate();
    assert_eq!(
        serde_json::to_value(
            table
                .selected()
                .iter()
                .map(|c| c.name.as_str())
                .collect::<Vec<_>>()
        )
        .unwrap(),
        reference["duplicate_selected"]
    );
    let defaults = hydrus_downloader_exchange::external_calls::defaults(true).unwrap();
    assert_eq!(
        serde_json::to_value(defaults.iter().map(|c| c.name.as_str()).collect::<Vec<_>>()).unwrap(),
        reference["defaults_by_platform"][if cfg!(windows) {
            "Windows"
        } else if cfg!(target_os = "macos") {
            "macOS"
        } else {
            "Linux"
        }]
    );
}

#[test]
fn cancelling_the_owner_terminates_and_reaps_its_started_direct_child() {
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };
    let owned = tempfile::tempdir().unwrap();
    let marker = owned.path().join("started.txt");
    #[cfg(not(windows))]
    let process = Process {
        executable: "/bin/sh".into(),
        arguments: vec![
            "-c".into(),
            "printf started > \"$1\"; while :; do :; done".into(),
            "owned-fixture".into(),
            marker.to_string_lossy().into_owned(),
        ],
        ..Process::default()
    };
    #[cfg(windows)]
    let process = {
        let script = owned.path().join("owned-cancellable-fixture.cmd");
        std::fs::write(
            &script,
            format!(
                "@echo started>\"{}\"\r\n:ownedloop\r\n@goto ownedloop\r\n",
                marker.display()
            ),
        )
        .unwrap();
        Process {
            executable: "cmd.exe".into(),
            arguments: vec![
                "/D".into(),
                "/C".into(),
                script.to_string_lossy().into_owned(),
            ],
            ..Process::default()
        }
    };
    let cancel = Arc::new(AtomicBool::new(false));
    let request = cancel.clone();
    let worker = std::thread::spawn(move || {
        hydrus_gui_model::external_calls::test_call_cancellable(
            &ActualCall::Process(process),
            &Inputs::new(),
            &request,
        )
    });
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(8);
    while !marker.exists() {
        assert!(std::time::Instant::now() < deadline);
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    cancel.store(true, Ordering::Release);
    assert_eq!(
        worker.join().unwrap().unwrap_err(),
        "External call cancelled."
    );
}

#[test]
fn actual_defaults_menu_routes_preserve_prior_selection_and_generate_unique_named_keys() {
    let oracle = hydrus_testkit::fixture_json("external_calls.json");
    for case in &oracle["default_routes"].as_array().unwrap()[..2] {
        let mut prior = Callable::new("prior selection");
        prior.call = ActualCall::Process(Process {
            executable: "owned-program".into(),
            ..Process::default()
        });
        let key = prior.key;
        let mut table = Table::new(Manager { calls: vec![prior] });
        table.click(0, false, false);
        let mut calls = hydrus_downloader_exchange::external_calls::defaults(false).unwrap();
        if case["route"] == "some" {
            calls = vec![calls[0].clone(), calls[9].clone()];
        }
        table.add_selected(calls);
        assert_eq!(
            serde_json::to_value(
                table
                    .manager
                    .calls
                    .iter()
                    .map(|c| c.name.as_str())
                    .collect::<Vec<_>>()
            )
            .unwrap(),
            case["names"]
        );
        assert_eq!(
            serde_json::to_value(
                table
                    .selected()
                    .iter()
                    .map(|c| c.name.as_str())
                    .collect::<Vec<_>>()
            )
            .unwrap(),
            case["selected"]
        );
        assert!(table.selection.is_selected(key));
        assert_eq!(
            table
                .manager
                .calls
                .iter()
                .map(|c| c.key)
                .collect::<std::collections::BTreeSet<_>>()
                .len(),
            table.manager.calls.len()
        );
    }
}

#[test]
fn duplicate_import_warnings_match_python_repr_and_all_size_boundaries() {
    let oracle = hydrus_testkit::fixture_json("external_calls.json");
    for case in oracle["warning_cases"].as_array().unwrap() {
        let calls =
            hydrus_downloader_exchange::external_calls::decode_text(&case["call"].to_string())
                .unwrap();
        let ActualCall::Process(process) = &calls[0].call else {
            panic!("recorded process call");
        };
        assert_eq!(
            serde_json::to_value(process.import_warning()).unwrap(),
            case["warning"]
        );
    }
}

// leaf: audit-options-nested-external-call-command-copy
#[test]
fn command_clipboard_review_and_cleaned_example_match_actual_qt() {
    use hydrus_gui_model::external_command::{Paste, Queue};
    let reference = hydrus_testkit::fixture_json("external_command.json");
    for event in reference["clipboard"].as_array().unwrap() {
        let paste = Paste::parse(event["raw"].as_str().unwrap());
        assert_eq!(
            paste.question(),
            event["questions"][0]["message"].as_str().unwrap()
        );
        let (executable, queue) = if event["accepted"].as_bool().unwrap() {
            (paste.executable, Queue::new(paste.arguments))
        } else {
            ("before".into(), Queue::new(vec!["before".into()]))
        };
        assert_eq!(
            serde_json::to_value(&queue.arguments).unwrap(),
            event["raw_arguments"]
        );
        assert_eq!(
            serde_json::json!([executable, clean_arguments(&queue.arguments)]),
            event["value"]
        );
        assert_eq!(
            queue.full_template(&executable),
            event["example"].as_str().unwrap()
        );
        assert_eq!(
            queue.full_template(&executable),
            event["copies"][0][1].as_str().unwrap()
        );
    }
}

// leaf: audit-options-nested-external-call-command-arguments
#[test]
fn command_parameter_queue_selection_reorder_and_keyboard_match_actual_qt() {
    use hydrus_gui_model::external_command::Queue;
    let reference = hydrus_testkit::fixture_json("external_command.json");
    let mut queue = Queue::new(["zero", "one", "two", "three"].map(str::to_owned).to_vec());
    for event in reference["queue"].as_array().unwrap() {
        match event["action"].as_str().unwrap() {
            "initial" | "cancel_edit" | "decline_delete" => {}
            "select_top_pair" => {
                queue.selection.select_many(&[0, 1]);
            }
            "up_at_top" | "up_after_bottom" | "clicked_up" => {
                queue.reorder(false);
            }
            "down_after_top" => {
                queue.reorder(true);
            }
            "clicked_down" => {
                queue = Queue::new(
                    ["alpha", "beta", "gamma", "delta"]
                        .map(str::to_owned)
                        .to_vec(),
                );
                queue.click(1, false, false);
                queue.reorder(true);
            }
            "down_at_bottom" => {
                queue.selection.select_many(&[2, 3]);
                queue.reorder(true);
            }
            "edit_first_of_multiple" => {
                queue.selection.select_many(&[1, 3]);
                let first = queue.selected()[0];
                queue.arguments[first] = "edited 日本😀".into();
            }
            "add_unselected" => {
                queue.arguments.push("added value".into());
            }
            "accept_delete" | "delete_key" => {
                queue.delete(&queue.selected());
            }
            "mouse_beta" | "click_before_reorder" => {
                queue = Queue::new(
                    ["alpha", "beta", "gamma", "delta"]
                        .map(str::to_owned)
                        .to_vec(),
                );
                queue.click(1, false, false);
            }
            "shift_down" | "shift_after_reorder" => {
                queue.navigate("next", false, true);
            }
            "ctrl_home" => {
                queue.navigate("home", true, false);
            }
            "ctrl_space" => {
                queue.toggle_current();
            }
            "select_all" => {
                queue.select_all();
            }
            "copy_selected" | "copy_insert" | "copy_after_ctrl_space" => {
                assert_eq!(
                    queue.copy_selected().unwrap(),
                    event["copies"][0][1].as_str().unwrap()
                );
            }
            "plain_end" => {
                queue.navigate("end", false, false);
            }
            "shift_up" => {
                queue.navigate("previous", false, true);
            }
            _ => panic!("unexpected queue step {event}"),
        }
        assert_eq!(
            serde_json::to_value(&queue.arguments).unwrap(),
            event["rows"],
            "{}",
            event["action"]
        );
        let mut expected: Vec<usize> = serde_json::from_value(event["selected"].clone()).unwrap();
        assert_eq!(queue.selection.selected_order(), expected);
        expected.sort_unstable();
        assert_eq!(queue.selected(), expected, "{}", event["action"]);
        assert_eq!(
            queue.current.map_or(-1, |i| i64::try_from(i).unwrap()),
            event["current"].as_i64().unwrap(),
            "{}",
            event["action"]
        );
    }
}

// leaf: audit-options-nested-external-call-command-arguments
#[test]
fn command_queue_reverse_edit_and_keyboard_origins_match_actual_qt() {
    use hydrus_gui_model::external_command::Queue;
    let reference = hydrus_testkit::fixture_json("external_command.json");
    for history in reference["queue_edges"].as_array().unwrap() {
        let mut queue = Queue::new(
            ["alpha", "beta", "gamma", "delta"]
                .map(str::to_owned)
                .to_vec(),
        );
        for event in history["steps"].as_array().unwrap() {
            match event["action"].as_str().unwrap() {
                "initial" => {}
                "click_1" => {
                    queue.click(1, false, false);
                }
                "click_3" => {
                    queue.click(3, false, false);
                }
                "ctrl_click_1" => {
                    queue.click(1, true, false);
                }
                "edit_selection_first" => {
                    let index = queue.selection.selected_order()[0];
                    assert_eq!(
                        queue.arguments[index],
                        event["entries"][0]["default"].as_str().unwrap()
                    );
                    queue.arguments[index] = "first-added 日本😀".into();
                }
                "shift_down" => {
                    queue.navigate("next", false, true);
                }
                "ctrl_home" => {
                    queue.navigate("home", true, false);
                }
                "select_all" => {
                    queue.select_all();
                }
                "delete" => {
                    queue.delete(&queue.selected());
                }
                unexpected => panic!("unexpected edge {unexpected}"),
            }
            assert_eq!(
                serde_json::to_value(&queue.arguments).unwrap(),
                event["rows"],
                "{} / {}",
                history["name"],
                event["action"]
            );
            let expected: Vec<usize> = serde_json::from_value(event["selected"].clone()).unwrap();
            assert_eq!(
                queue.selection.selected_order(),
                expected,
                "{} / {}",
                history["name"],
                event["action"]
            );
            assert_eq!(
                queue.current.map_or(-1, |i| i64::try_from(i).unwrap()),
                event["current"].as_i64().unwrap(),
                "{} / {}",
                history["name"],
                event["action"]
            );
        }
    }
}
