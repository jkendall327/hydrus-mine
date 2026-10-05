//! Real Options list/editor actions, saved calls, owner cancellation and exchange.
use hydrus_core::external_calls::{ActualCall, Callable, Manager, Parameter, Process, Rule};
use hydrus_gui::{Bound, MainWindow, OptionsWindow, Pages, bind, headless};
use hydrus_store::{Store, import::import_legacy};
use slint::{ComponentHandle as _, Model as _};
use std::sync::Arc;
fn store() -> ([tempfile::TempDir; 2], Arc<Store>) {
    let legacy = hydrus_testkit::legacy_fixture("basic");
    let native = tempfile::tempdir().unwrap();
    import_legacy(
        legacy.path(),
        &native.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store = Store::open(native.path()).unwrap();
    ([legacy, native], store)
}
fn open(ui: &MainWindow, bound: &Bound) -> OptionsWindow {
    ui.invoke_menu_title_pressed(0, 20.0, 22.0);
    let lines = ui.get_menu_panes().row_data(0).unwrap().lines;
    let at = (0..lines.row_count())
        .find(|&i| lines.row_data(i).unwrap().label == "options…")
        .unwrap();
    ui.invoke_menu_line_clicked(0, i32::try_from(at).unwrap(), 0.0, 0.0, 0.0);
    let w = bound.options.borrow().as_ref().unwrap().clone_strong();
    let pages = w.get_pages();
    let at = (0..pages.row_count())
        .find(|&i| pages.row_data(i).unwrap().text == "external programs")
        .unwrap();
    w.invoke_page_chosen(i32::try_from(at).unwrap());
    w
}
fn named(w: &OptionsWindow, name: &str) -> i32 {
    let rows = w.get_external_call_rows();
    i32::try_from(
        (0..rows.row_count())
            .find(|&i| rows.row_data(i).unwrap().cells.row_data(0).unwrap() == name)
            .unwrap(),
    )
    .unwrap()
}
fn call(bound: &Bound) -> hydrus_gui::ExternalCallWindow {
    bound
        .options_external_calls
        .editor
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong()
}
fn saved(store: &Store) -> Manager {
    store.read(hydrus_store::settings::get).unwrap()
}
fn seed(store: &Store) -> Callable {
    let mut c = Callable::new("synthetic call");
    c.call = ActualCall::Process(Process {
        executable: std::env::current_exe()
            .unwrap()
            .to_string_lossy()
            .into_owned(),
        arguments: vec!["--list".into(), "%path%".into()],
        rules: vec![Rule::new(Parameter::Path)],
        ..Process::default()
    });
    let manager = Manager {
        calls: vec![c.clone()],
    };
    store
        .write(move |ctx| hydrus_store::settings::set(ctx.conn(), &manager))
        .unwrap();
    c
}

#[test]
fn callable_command_child_apply_cancel_parent_staging_reopen_and_retired_owner() {
    let (_dirs, store) = store();
    let original = seed(&store);
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    let w = open(&ui, &bound);
    assert_eq!(w.get_rows().row_data(1).unwrap().kind, 30);
    w.invoke_external_call_clicked(named(&w, "synthetic call"), false, false);
    w.invoke_external_call_action("edit".into());
    let child = call(&bound);
    child.set_name("edited 日本".into());
    child.invoke_command_edit();
    let command = bound
        .options_external_calls
        .command
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    command.set_executable("cancelled-command".into());
    command.invoke_cancel();
    assert!(bound.options_external_calls.command.borrow().is_none());
    assert!(
        child
            .get_command_template()
            .contains(original.call.description().trim_start_matches("CALL: "))
    );
    child.invoke_command_edit();
    let command = bound
        .options_external_calls
        .command
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    command.invoke_action("add".into());
    let parameter = bound
        .options_external_calls
        .question
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    parameter.invoke_name_entered("  --literal\t value  \nignored".into());
    assert!(command.get_full_template().contains("--literal value"));
    command.invoke_apply();
    assert!(bound.options_external_calls.command.borrow().is_none());
    assert!(child.get_command_template().contains("--literal value"));
    child.invoke_apply();
    assert!(bound.options_external_calls.editor.borrow().is_none());
    assert_eq!(
        saved(&store).calls.as_slice(),
        std::slice::from_ref(&original)
    );
    w.invoke_cancel();
    assert_eq!(
        saved(&store).calls.as_slice(),
        std::slice::from_ref(&original)
    );
    child.invoke_apply();
    command.invoke_apply();
    parameter.invoke_name_entered("stale".into());
    assert_eq!(
        saved(&store).calls.as_slice(),
        std::slice::from_ref(&original)
    );
    let w = open(&ui, &bound);
    w.invoke_external_call_clicked(named(&w, "synthetic call"), false, false);
    w.invoke_external_call_action("edit".into());
    let child = call(&bound);
    child.set_name("saved 日本".into());
    child.invoke_apply();
    w.invoke_apply();
    let persisted = saved(&store);
    assert_eq!(persisted.calls[0].name, "saved 日本");
    assert_eq!(persisted.calls[0].key, original.key);
    assert_eq!(persisted.calls[0].call, original.call);
    let w = open(&ui, &bound);
    w.invoke_external_call_action("add".into());
    let child = call(&bound);
    child.set_name("discarded".into());
    child.invoke_command_edit();
    w.invoke_cancel();
    assert!(!bound.options_external_calls.has_open());
    child.invoke_apply();
    assert_eq!(saved(&store), persisted);
    let w = open(&ui, &bound);
    w.invoke_external_call_clicked(named(&w, "saved 日本"), false, false);
    w.invoke_external_call_action("edit".into());
    let child = call(&bound);
    let pixels = headless::render(&windows.get(windows.count() - 1).unwrap(), 1050, 760);
    headless::save_png(
        &std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("external_callable.png"),
        &pixels,
        1050,
        760,
    )
    .unwrap();
    child.invoke_cancel();
    w.invoke_cancel();
}
#[test]
fn list_duplicate_defaults_delete_capture_and_options_persistence() {
    let (_dirs, store) = store();
    let original = seed(&store);
    headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    let w = open(&ui, &bound);
    w.invoke_external_call_clicked(named(&w, "synthetic call"), false, false);
    w.invoke_external_call_action("duplicate".into());
    assert_eq!(w.get_external_call_rows().row_count(), 2);
    assert!((0..2).all(|i| w.get_external_call_rows().row_data(i).unwrap().selected));
    w.invoke_external_call_action("delete".into());
    let q = bound
        .options_external_calls
        .question
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    assert_eq!(q.get_message(), "Remove all selected?");
    w.invoke_external_call_clicked(0, false, false);
    q.invoke_answered(false);
    assert_eq!(w.get_external_call_rows().row_count(), 2);
    w.invoke_external_call_clicked(0, false, false);
    w.invoke_external_call_action("delete".into());
    let q = bound
        .options_external_calls
        .question
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    w.invoke_external_call_clicked(1, false, false);
    q.invoke_answered(true);
    assert_eq!(w.get_external_call_rows().row_count(), 1);
    assert_eq!(
        w.get_external_call_rows()
            .row_data(0)
            .unwrap()
            .cells
            .row_data(0)
            .unwrap(),
        "synthetic call (1)"
    );
    w.invoke_external_call_action("defaults".into());
    let q = bound
        .options_external_calls
        .question
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    assert_eq!(q.get_yes_label(), "just for my platform");
    q.invoke_answered(false);
    let defaults = bound
        .options_external_calls
        .defaults
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    assert_eq!(defaults.get_rows().row_count(), 15);
    defaults.invoke_toggled(0);
    defaults.invoke_toggled(9);
    defaults.invoke_apply();
    assert_eq!(w.get_external_call_rows().row_count(), 3);
    assert_eq!(
        saved(&store).calls.as_slice(),
        std::slice::from_ref(&original)
    );
    w.invoke_apply();
    let persisted = saved(&store);
    assert_eq!(persisted.calls.len(), 3);
    assert!(persisted.calls.iter().all(|c| c.key != original.key));
    assert_eq!(
        persisted
            .calls
            .iter()
            .map(|c| c.key)
            .collect::<std::collections::BTreeSet<_>>()
            .len(),
        3
    );
    let w = open(&ui, &bound);
    w.invoke_external_call_action("defaults-all".into());
    let q = bound
        .options_external_calls
        .question
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    q.invoke_answered(false);
    assert!(bound.options_external_calls.defaults.borrow().is_none());
    assert_eq!(w.get_external_call_rows().row_count(), 18);
    assert_eq!(
        (0..18)
            .filter(|&i| w.get_external_call_rows().row_data(i).unwrap().selected)
            .count(),
        15
    );
    w.invoke_cancel();
    assert_eq!(saved(&store), persisted);
    let w = open(&ui, &bound);
    w.invoke_external_call_action("defaults-all".into());
    let q = bound
        .options_external_calls
        .question
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    q.invoke_answered(true);
    let platform_count = hydrus_downloader_exchange::external_calls::defaults(true)
        .unwrap()
        .len();
    assert_eq!(w.get_external_call_rows().row_count(), 3 + platform_count);
    w.invoke_cancel();
    assert_eq!(saved(&store), persisted);
    let w = open(&ui, &bound);
    w.invoke_external_call_action("defaults-all".into());
    let q = bound
        .options_external_calls
        .question
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    bound.options_external_calls.cancel();
    q.invoke_answered(false);
    assert_eq!(w.get_external_call_rows().row_count(), 3);
    w.invoke_cancel();
    assert_eq!(saved(&store), persisted);
    let w = open(&ui, &bound);
    w.invoke_external_call_action("defaults".into());
    let q = bound
        .options_external_calls
        .question
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    w.invoke_cancel();
    q.invoke_answered(true);
    assert!(!bound.options_external_calls.has_open());
    assert_eq!(saved(&store), persisted);
}
#[test]
fn imported_reference_calls_review_cancel_and_saved_process_test_consumer() {
    let (_dirs, store) = store();
    headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    let w = open(&ui, &bound);
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
    assert!(exchange.get_ready());
    assert!(saved(&store).calls.is_empty());
    exchange.invoke_action("accept".into());
    assert_eq!(w.get_external_call_rows().row_count(), 2);
    w.invoke_cancel();
    assert!(saved(&store).calls.is_empty());
    exchange.invoke_action("accept".into());
    assert!(saved(&store).calls.is_empty());
    let w = open(&ui, &bound);
    w.invoke_external_call_action("import".into());
    let exchange = bound
        .options_external_calls
        .exchange
        .0
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    exchange.set_text("[157,1,[]]".into());
    exchange.invoke_action("review".into());
    assert!(!exchange.get_error().is_empty());
    assert!(!exchange.get_ready());
    exchange.set_text(reference["defaults"][0].to_string().into());
    exchange.invoke_action("review".into());
    exchange.invoke_action("accept".into());
    w.invoke_apply();
    let persisted = saved(&store);
    assert_eq!(persisted.calls.len(), 1);
    assert_eq!(persisted.calls[0].call, ActualCall::DefaultFile);
}

#[test]
fn reopened_saved_process_uses_real_editor_inputs_and_owned_test_call_worker() {
    let (_dirs, store) = store();
    let owned = tempfile::tempdir().unwrap();
    let output = owned.path().join("owned Unicode argument.txt");
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
        let script = owned.path().join("owned-argument-fixture.cmd");
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
    let mut c = Callable::new("saved local fixture");
    c.call = ActualCall::Process(process);
    let before = Manager { calls: vec![c] };
    let write = before.clone();
    store
        .write(move |ctx| hydrus_store::settings::set(ctx.conn(), &write))
        .unwrap();
    headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    let w = open(&ui, &bound);
    w.invoke_external_call_clicked(named(&w, "saved local fixture"), false, false);
    w.invoke_external_call_action("edit".into());
    let child = call(&bound);
    let value = "synthetic 日本; $(this must stay text)";
    child.invoke_test_input(0, value.into());
    assert!(child.get_preview().contains(value));
    child.invoke_test(false);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(8);
    while child.get_testing() {
        assert!(
            std::time::Instant::now() < deadline,
            "local test call timed out: {}",
            child.get_test_status()
        );
        slint::platform::update_timers_and_animations();
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    assert_eq!(child.get_test_status(), "Looks good!");
    #[cfg(not(windows))]
    assert_eq!(std::fs::read_to_string(output).unwrap(), value);
    #[cfg(windows)]
    assert_eq!(
        std::fs::read_to_string(output).unwrap().trim(),
        format!("\"{value}\"")
    );
    child.invoke_test(true);
    while child.get_testing() {
        assert!(std::time::Instant::now() < deadline);
        slint::platform::update_timers_and_animations();
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    assert_eq!(child.get_test_status(), "Availability test worked!");
    child.invoke_cancel();
    w.invoke_cancel();
    assert_eq!(saved(&store), before);
}

#[test]
fn duplicate_warning_decline_keeps_unsorted_unselected_prefix_accept_finishes_and_retired_question_cannot_append()
 {
    let (_dirs, store) = store();
    let calls = [
        ("a valid", "owned-program".to_owned()),
        ("b unusual", "x".repeat(257)),
        ("c tail", "owned-program".to_owned()),
    ]
    .into_iter()
    .map(|(name, executable)| {
        let mut call = Callable::new(name);
        call.call = ActualCall::Process(Process {
            executable,
            ..Process::default()
        });
        call
    })
    .collect::<Vec<_>>();
    let original = Manager { calls };
    let manager = original.clone();
    store
        .write(move |ctx| hydrus_store::settings::set(ctx.conn(), &manager))
        .unwrap();
    headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    let oracle: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../oracle/fixtures/external_calls.json"
    ))
    .unwrap();
    for (case, accept) in [false, true].into_iter().enumerate() {
        let w = open(&ui, &bound);
        w.invoke_external_call_sort(0, true);
        w.invoke_external_call_clicked(0, false, false);
        w.invoke_external_call_clicked(2, false, true);
        w.invoke_external_call_action("duplicate".into());
        let q = bound
            .options_external_calls
            .question
            .borrow()
            .as_ref()
            .unwrap()
            .clone_strong();
        assert_eq!(
            q.get_message().as_str(),
            oracle["duplicate_warnings"][case]["questions"][0]["message"]
                .as_str()
                .unwrap()
        );
        assert_eq!(w.get_external_call_rows().row_count(), 4);
        assert!(!w.get_external_call_rows().row_data(3).unwrap().selected);
        // The selected call snapshot cannot change while its warning is pending.
        w.invoke_external_call_clicked(0, false, false);
        q.invoke_answered(accept);
        let names = (0..w.get_external_call_rows().row_count())
            .map(|i| {
                w.get_external_call_rows()
                    .row_data(i)
                    .unwrap()
                    .cells
                    .row_data(0)
                    .unwrap()
                    .to_string()
            })
            .collect::<Vec<_>>();
        let selected = (0..w.get_external_call_rows().row_count())
            .filter_map(|i| {
                let row = w.get_external_call_rows().row_data(i).unwrap();
                row.selected
                    .then(|| row.cells.row_data(0).unwrap().to_string())
            })
            .collect::<Vec<_>>();
        assert_eq!(
            serde_json::to_value(names).unwrap(),
            oracle["duplicate_warnings"][case]["names"]
        );
        assert_eq!(
            serde_json::to_value(selected).unwrap(),
            oracle["duplicate_warnings"][case]["selected"]
        );
        assert_eq!(saved(&store), original);
        if accept {
            w.invoke_apply();
            let saved = saved(&store);
            assert_eq!(saved.calls.len(), 6);
            assert_eq!(
                saved
                    .calls
                    .iter()
                    .map(|c| c.key)
                    .collect::<std::collections::BTreeSet<_>>()
                    .len(),
                6
            );
        } else {
            w.invoke_cancel();
            q.invoke_answered(true);
            assert_eq!(saved(&store), original);
        }
    }
}

#[test]
fn simple_delete_includes_os_launch_rows_but_cancel_and_closed_owner_do_not_change_saved_calls() {
    let (_dirs, store) = store();
    let mut file = Callable::new("OS file");
    file.call = ActualCall::DefaultFile;
    let mut url = Callable::new("OS URL");
    url.call = ActualCall::DefaultUrl;
    let manager = Manager {
        calls: vec![file, url],
    };
    let original = manager.clone();
    store
        .write(move |ctx| hydrus_store::settings::set(ctx.conn(), &manager))
        .unwrap();
    headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    let w = open(&ui, &bound);
    w.invoke_external_call_clicked(0, false, false);
    w.invoke_external_call_clicked(1, false, true);
    w.invoke_external_call_action("delete".into());
    let q = bound
        .options_external_calls
        .question
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    assert_eq!(q.get_message(), "Remove all selected?");
    q.invoke_answered(false);
    assert_eq!(w.get_external_call_rows().row_count(), 2);
    w.invoke_external_call_action("delete".into());
    let q = bound
        .options_external_calls
        .question
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    q.invoke_answered(true);
    assert_eq!(w.get_external_call_rows().row_count(), 0);
    assert_eq!(saved(&store), original);
    w.invoke_apply();
    assert!(saved(&store).calls.is_empty());
    let w = open(&ui, &bound);
    w.invoke_external_call_action("defaults-all".into());
    let q = bound
        .options_external_calls
        .question
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    w.invoke_cancel();
    q.invoke_answered(false);
    assert!(saved(&store).calls.is_empty());
}
