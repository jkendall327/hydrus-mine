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
    assert_eq!(table.selected()[0].key, key);
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
    let process = Process {
        executable: "cmd.exe".into(),
        arguments: vec![
            "/C".into(),
            format!("echo synthetic 日本>\"{}\"", output.display()),
        ],
        ..Process::default()
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
    assert!(output.is_file());
}
