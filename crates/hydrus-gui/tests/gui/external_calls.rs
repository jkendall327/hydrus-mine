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
    let reopened_index = windows.count() - 1;
    assert_eq!(
        list_rows(&w)
            .into_iter()
            .map(|row| row.0)
            .collect::<Vec<_>>(),
        persisted
            .calls
            .iter()
            .map(|call| vec![
                call.name.clone(),
                call.pipeline.label().to_owned(),
                call.call.description(),
            ])
            .collect::<Vec<_>>()
    );
    assert_eq!(saved(&store), persisted);
    list_capture(
        &windows,
        reopened_index,
        w.window(),
        "external-calls-reopened-saved.png",
        (1100, 800),
    );
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
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    let w = open(&ui, &bound);
    w.invoke_external_call_clicked(named(&w, "synthetic call"), false, false);
    w.invoke_external_call_action("duplicate".into());
    assert_eq!(w.get_external_call_rows().row_count(), 2);
    assert!((0..2).all(|i| w.get_external_call_rows().row_data(i).unwrap().selected));
    assert!(w.get_external_call_selected());
    assert!(!w.get_external_call_child_open());
    list_capture(
        &windows,
        windows.count() - 1,
        w.window(),
        "external-calls-duplicate-selected.png",
        (1100, 800),
    );
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
    let _headless_windows = headless::init();
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
    let _headless_windows = headless::init();
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
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    let oracle: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../oracle/fixtures/external_calls.json"
    ))
    .unwrap();
    for (case, accept) in [false, true].into_iter().enumerate() {
        let w = open(&ui, &bound);
        let list_index = windows.count() - 1;
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
        if !accept {
            list_capture(
                &windows,
                windows.count() - 1,
                q.window(),
                "external-calls-duplicate-warning.png",
                (900, 420),
            );
        }
        // The selected call snapshot cannot change while its warning is pending.
        w.invoke_external_call_clicked(0, false, false);
        q.invoke_answered(accept);
        assert!(bound.options_external_calls.question.borrow().is_none());
        assert!(!q.window().is_visible());
        // The closed question's display flag follows the real 30 ms owner timer.
        // Preserve the non-modal assertion below without forcing widget state.
        let closed_at = std::time::Instant::now();
        while w.get_external_call_child_open() {
            headless::render(&windows.get(list_index).unwrap(), 1100, 800);
            assert!(
                closed_at.elapsed() < std::time::Duration::from_secs(2),
                "closed duplicate warning did not release the list controls"
            );
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
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
        assert!(w.get_external_call_selected());
        assert!(!w.get_external_call_child_open());
        list_capture(
            &windows,
            list_index,
            w.window(),
            if accept {
                "external-calls-duplicate-complete.png"
            } else {
                "external-calls-duplicate-declined-prefix.png"
            },
            (1100, 800),
        );
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
            assert_duplicate_manager(&saved, &original);
            let reopened = open(&ui, &bound);
            assert_eq!(
                store.read(hydrus_store::settings::get::<Manager>).unwrap(),
                saved
            );
            assert_eq!(
                list_rows(&reopened)
                    .into_iter()
                    .map(|row| row.0)
                    .collect::<Vec<_>>(),
                saved
                    .calls
                    .iter()
                    .map(|call| vec![
                        call.name.clone(),
                        call.pipeline.label().to_owned(),
                        call.call.description(),
                    ])
                    .collect::<Vec<_>>()
            );
            reopened.invoke_cancel();
        } else {
            w.invoke_cancel();
            q.invoke_answered(true);
            assert_eq!(saved(&store), original);
            retired_duplicate_question_preserves_successor(&ui, &bound, &store, &original, &oracle);
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
    url.pipeline = hydrus_core::external_calls::Pipeline::Url;
    let manager = Manager {
        calls: vec![file, url],
    };
    let original = manager.clone();
    store
        .write(move |ctx| hydrus_store::settings::set(ctx.conn(), &manager))
        .unwrap();
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    retired_delete_question_preserves_successor(&ui, &bound, &store);
    let w = open(&ui, &bound);
    let list_index = windows.count() - 1;
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
    assert_eq!(
        list_rows(&w)
            .into_iter()
            .map(|row| row.0[0].clone())
            .collect::<Vec<_>>(),
        original
            .calls
            .iter()
            .map(|call| call.name.clone())
            .collect::<Vec<_>>()
    );
    assert!(w.get_external_call_rows().iter().all(|row| row.selected));
    list_capture(
        &windows,
        windows.count() - 1,
        q.window(),
        "external-calls-delete-review.png",
        (520, 200),
    );
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
    assert!(!w.get_external_call_selected());
    assert!(!w.get_external_call_child_open());
    list_capture(
        &windows,
        list_index,
        w.window(),
        "external-calls-delete-empty.png",
        (1100, 800),
    );
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

fn command(bound: &Bound) -> hydrus_gui::ExternalCommandWindow {
    bound
        .options_external_calls
        .command
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong()
}
fn question(bound: &Bound) -> hydrus_gui::SessionDialog {
    bound
        .options_external_calls
        .question
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong()
}
fn command_rows(w: &hydrus_gui::ExternalCommandWindow) -> Vec<String> {
    w.get_rows()
        .iter()
        .map(|r| r.cells.row_data(0).unwrap().to_string())
        .collect()
}
fn command_capture(windows: &headless::Windows, index: usize, name: &str, size: (u32, u32)) {
    let native = windows.get(index).unwrap();
    headless::render(&native, size.0, size.1);
    let pixels = headless::render(&native, size.0, size.1);
    headless::save_png(
        &std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join(name),
        &pixels,
        size.0,
        size.1,
    )
    .unwrap();
}
fn command_feedback_capture(
    windows: &headless::Windows,
    index: usize,
    w: &hydrus_gui::ExternalCommandWindow,
    expected: &str,
    name: &str,
) {
    use std::time::{Duration, Instant};
    let native = windows.get(index).unwrap();
    let started = Instant::now();
    // The real overlay is admitted by a 50 ms Timer and expires after 3 s.
    // Observe its painted opaque fill, not merely the feedback string.
    let pixels = loop {
        let pixels = headless::render(&native, 760, 590);
        if started.elapsed() >= Duration::from_millis(50)
            && pixels
                .chunks_exact(4)
                .any(|pixel| pixel == [255, 255, 220, 255])
        {
            break pixels;
        }
        assert!(
            started.elapsed() < Duration::from_secs(2),
            "the actual command feedback overlay must paint before expiry"
        );
        std::thread::sleep(Duration::from_millis(5));
    };
    assert_eq!(w.get_feedback(), expected);
    assert!(w.window().is_visible());
    headless::save_png(
        &std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join(name),
        &pixels,
        760,
        590,
    )
    .unwrap();
}
fn command_key(
    w: &hydrus_gui::ExternalCommandWindow,
    key: slint::SharedString,
    control: bool,
    shift: bool,
) {
    use slint::platform::{Key, WindowEvent};
    for (held, on) in [(Key::Control, control), (Key::Shift, shift)] {
        if on {
            w.window()
                .dispatch_event(WindowEvent::KeyPressed { text: held.into() });
        }
    }
    w.window()
        .dispatch_event(WindowEvent::KeyPressed { text: key.clone() });
    w.window()
        .dispatch_event(WindowEvent::KeyReleased { text: key });
    for (held, on) in [(Key::Shift, shift), (Key::Control, control)] {
        if on {
            w.window()
                .dispatch_event(WindowEvent::KeyReleased { text: held.into() });
        }
    }
}
#[test]
fn actual_command_parameter_queue_buttons_keys_cancel_and_saved_argument_consumer() {
    use slint::platform::{Key, PointerEventButton, WindowEvent};

    hydrus_gui::set_clipper(|clip| {
        if let hydrus_gui::Clip::Text(text) = clip {
            headless::set_clipboard_text(text);
        }
    });
    let (_dirs, store) = store();
    let mut original = seed(&store);
    let ActualCall::Process(ref mut process) = original.call else {
        panic!("seed process");
    };
    process.arguments = ["zero", "one", "two", "three"].map(str::to_owned).to_vec();
    let manager = Manager {
        calls: vec![original.clone()],
    };
    store
        .write(move |writer| hydrus_store::settings::set(writer.conn(), &manager))
        .unwrap();
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    let options = open(&ui, &bound);
    options.invoke_external_call_clicked(0, false, false);
    options.invoke_external_call_action("edit".into());
    let child = call(&bound);
    child.invoke_command_edit();
    let w = command(&bound);
    let command_native = windows.get(windows.count() - 1).unwrap();
    let fixture = hydrus_testkit::fixture_json("external_command.json");
    for event in fixture["queue"].as_array().unwrap().iter().take(11) {
        match event["action"].as_str().unwrap() {
            "initial" => {}
            "select_top_pair" => {
                w.invoke_clicked(0, false, false);
                w.invoke_clicked(1, true, false);
            }
            "up_at_top" | "up_after_bottom" => {
                w.invoke_action("up".into());
            }
            "down_after_top" => {
                w.invoke_action("down".into());
            }
            "down_at_bottom" => {
                w.invoke_clicked(2, false, false);
                w.invoke_clicked(3, true, false);
                w.invoke_action("down".into());
            }
            "edit_first_of_multiple" => {
                w.invoke_clicked(1, false, false);
                w.invoke_clicked(3, true, false);
                w.invoke_action("edit".into());
                let q = question(&bound);
                let expected = &event["entries"][0];
                assert_eq!(q.get_window_title(), expected["title"].as_str().unwrap());
                assert_eq!(q.get_message(), expected["message"].as_str().unwrap());
                assert_eq!(q.get_text(), expected["default"].as_str().unwrap());
                assert_eq!(
                    q.get_placeholder(),
                    expected["placeholder"].as_str().unwrap()
                );
                assert!(q.window().is_visible());
                command_capture(
                    &windows,
                    windows.count() - 1,
                    "external-command-parameter-edit.png",
                    (760, 640),
                );
                w.invoke_action("delete".into());
                assert!(
                    q.get_asking_name(),
                    "the nested editor blocks other queue actions"
                );
                q.invoke_name_entered("  edited\t日本😀  \nignored".into());
            }
            "cancel_edit" => {
                w.invoke_action("edit".into());
                question(&bound).invoke_cancelled();
            }
            "add_unselected" => {
                w.invoke_action("add".into());
                let q = question(&bound);
                let expected = &event["entries"][0];
                assert_eq!(q.get_window_title(), expected["title"].as_str().unwrap());
                assert_eq!(q.get_message(), expected["message"].as_str().unwrap());
                assert_eq!(q.get_text(), expected["default"].as_str().unwrap());
                assert_eq!(
                    q.get_placeholder(),
                    expected["placeholder"].as_str().unwrap()
                );
                assert!(q.window().is_visible());
                command_capture(
                    &windows,
                    windows.count() - 1,
                    "external-command-parameter-add.png",
                    (760, 640),
                );
                question(&bound).invoke_name_entered("  added  value  ".into());
            }
            "decline_delete" | "accept_delete" => {
                w.invoke_action("delete".into());
                let q = question(&bound);
                assert_eq!(
                    q.get_message(),
                    event["questions"][0]["message"].as_str().unwrap()
                );
                q.invoke_answered(event["action"] == "accept_delete");
            }
            unexpected => panic!("unexpected queue step {unexpected}"),
        }
        assert_eq!(
            serde_json::to_value(command_rows(&w)).unwrap(),
            event["rows"],
            "{}",
            event["action"]
        );
        let selected: Vec<usize> = w
            .get_rows()
            .iter()
            .enumerate()
            .filter(|(_, r)| r.selected)
            .map(|(i, _)| i)
            .collect();
        let mut expected: Vec<usize> = serde_json::from_value(event["selected"].clone()).unwrap();
        expected.sort_unstable();
        assert_eq!(selected, expected);
    }
    let pixels = headless::render(&command_native, 760, 590);
    headless::save_png(
        &std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("external-command-parameters.png"),
        &pixels,
        760,
        590,
    )
    .unwrap();
    w.invoke_cancel();
    assert!(bound.options_external_calls.command.borrow().is_none());
    child.invoke_command_edit();
    let w = command(&bound);
    // Fresh clicked row; genuine Slint key dispatch must retain list focus through row refreshes.
    headless::render(&windows.get(windows.count() - 1).unwrap(), 760, 590);
    let position = slint::LogicalPosition::new(
        w.get_parameter_list_x() + 10.0,
        w.get_parameter_list_y() + 24.0 + 33.0,
    );
    w.window().dispatch_event(WindowEvent::PointerPressed {
        position,
        button: PointerEventButton::Left,
    });
    w.window().dispatch_event(WindowEvent::PointerReleased {
        position,
        button: PointerEventButton::Left,
    });
    assert!(w.get_rows().row_data(1).unwrap().selected);
    command_key(&w, Key::DownArrow.into(), false, true);
    assert!(w.get_rows().row_data(2).unwrap().selected);
    command_key(&w, Key::Home.into(), true, false);
    command_key(&w, " ".into(), true, false);
    assert!((0..3).all(|i| w.get_rows().row_data(i).unwrap().selected));
    command_key(&w, "c".into(), true, false);
    assert_eq!(
        headless::clipboard_text().as_deref(),
        Some("one\ntwo\nzero")
    );
    command_key(&w, "a".into(), true, false);
    command_key(&w, "c".into(), true, false);
    assert_eq!(
        headless::clipboard_text().as_deref(),
        Some("zero\none\ntwo\nthree")
    );
    command_key(&w, Key::End.into(), false, false);
    command_key(&w, Key::UpArrow.into(), false, true);
    command_key(&w, Key::Delete.into(), false, false);
    let q = question(&bound);
    assert_eq!(q.get_message(), "Remove 2 selected?");
    assert!(q.window().is_visible());
    command_capture(
        &windows,
        windows.count() - 1,
        "external-command-delete-review.png",
        (520, 200),
    );
    q.invoke_answered(true);
    assert_eq!(command_rows(&w), ["zero", "one"]);
    // Accepted parameter editing reaches the existing saved substitution consumer.
    w.invoke_clicked(1, false, false);
    w.invoke_action("edit".into());
    question(&bound).invoke_name_entered(" pre:%path%:日本😀 ".into());
    w.invoke_apply();
    child.invoke_apply();
    options.invoke_apply();
    let persisted = saved(&store);
    let ActualCall::Process(process) = &persisted.calls[0].call else {
        panic!("saved process");
    };
    let inputs = hydrus_core::external_calls::Inputs::from([(
        Parameter::Path,
        vec!["/synthetic/漢😀.png".into()],
    )]);
    assert_eq!(
        process.command(&inputs).unwrap(),
        [
            process.executable.clone(),
            "zero".into(),
            "pre:/synthetic/漢😀.png:日本😀".into()
        ]
    );
    let options = open(&ui, &bound);
    options.invoke_external_call_clicked(0, false, false);
    options.invoke_external_call_action("edit".into());
    let child = call(&bound);
    child.invoke_command_edit();
    let reopened = command(&bound);
    assert_eq!(command_rows(&reopened), ["zero", "pre:%path%:日本😀"]);
    reopened.invoke_action("add".into());
    let retired = question(&bound);
    options.invoke_cancel();
    retired.invoke_name_entered("retired".into());
    reopened.invoke_apply();
    assert_eq!(saved(&store), persisted);
    assert!(!bound.options_external_calls.has_open());
}

#[test]
fn command_clipboard_exact_review_raw_rows_clean_copy_errors_and_owner_retirement() {
    hydrus_gui::set_clipper(|clip| {
        if let hydrus_gui::Clip::Text(text) = clip {
            headless::set_clipboard_text(text);
        }
    });
    let (_dirs, store) = store();
    let windows = headless::init();
    let original = seed(&store);
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    let options = open(&ui, &bound);
    options.invoke_external_call_clicked(0, false, false);
    options.invoke_external_call_action("edit".into());
    let child = call(&bound);
    let reference = hydrus_testkit::fixture_json("external_command.json");
    for event in reference["clipboard"].as_array().unwrap() {
        child.invoke_command_edit();
        let w = command(&bound);
        let command_index = windows.count() - 1;
        // Reproduce the recorded before-state through the real paste route.
        hydrus_gui::set_paster(|| "before before".into());
        w.invoke_action("paste".into());
        question(&bound).invoke_answered(true);
        let raw = event["raw"].as_str().unwrap().to_owned();
        hydrus_gui::set_paster(move || raw.clone());
        w.invoke_action("paste".into());
        let q = question(&bound);
        assert_eq!(
            q.get_message(),
            event["questions"][0]["message"].as_str().unwrap()
        );
        w.invoke_action("add".into());
        assert!(!q.get_asking_name(), "paste review blocks queue mutation");
        if event["raw"].as_str().unwrap().ends_with("arg29") && event["accepted"].as_bool().unwrap()
        {
            assert!(q.window().is_visible());
            command_capture(
                &windows,
                windows.count() - 1,
                "external-command-paste-review.png",
                (760, 590),
            );
        }
        q.invoke_answered(event["accepted"].as_bool().unwrap());
        assert_eq!(
            serde_json::to_value(command_rows(&w)).unwrap(),
            event["raw_arguments"]
        );
        assert_eq!(w.get_full_template(), event["example"].as_str().unwrap());
        if event["raw"] == "owned-program profile=\"My Profile\" 日本😀"
            && event["accepted"].as_bool().unwrap()
        {
            command_feedback_capture(
                &windows,
                command_index,
                &w,
                "Pasted!",
                "external-command-paste-feedback.png",
            );
        }
        w.invoke_action("copy".into());
        assert_eq!(w.get_feedback(), "Copied!");
        assert_eq!(
            headless::clipboard_text().as_deref(),
            event["copies"][0][1].as_str()
        );
        if event["raw"] == "owned-program profile=\"My Profile\" 日本😀"
            && event["accepted"].as_bool().unwrap()
        {
            command_feedback_capture(
                &windows,
                command_index,
                &w,
                "Copied!",
                "external-command-copy-feedback.png",
            );
        }
        w.invoke_cancel();
        assert_eq!(saved(&store).calls, std::slice::from_ref(&original));
    }
    child.invoke_command_edit();
    let w = command(&bound);
    let before_rows = command_rows(&w);
    let before_example = w.get_full_template();
    hydrus_gui::set_clipboard_reader(|| Ok(None));
    w.invoke_action("paste".into());
    let q = question(&bound);
    assert!(q.get_notice_only());
    assert_eq!(
        q.get_window_title(),
        reference["unavailable"]["errors"][0]["title"]
            .as_str()
            .unwrap()
    );
    assert_eq!(
        q.get_message(),
        reference["unavailable"]["errors"][0]["message"]
            .as_str()
            .unwrap()
    );
    assert_eq!(command_rows(&w), before_rows);
    assert_eq!(w.get_full_template(), before_example);
    assert!(q.window().is_visible());
    command_capture(
        &windows,
        windows.count() - 1,
        "external-command-clipboard-error.png",
        (520, 200),
    );
    q.invoke_cancelled();
    hydrus_gui::set_clipboard_reader(|| Err("synthetic clipboard access failure".into()));
    w.invoke_action("paste".into());
    let error = question(&bound);
    assert_eq!(error.get_message(), "synthetic clipboard access failure");
    error.invoke_cancelled();
    hydrus_gui::clear_clipboard_reader();
    hydrus_gui::set_paster(|| {
        "owned-program --literal 日本😀 prefix:%path% profile=\"My Profile\"".into()
    });
    w.invoke_action("paste".into());
    question(&bound).invoke_answered(true);
    assert_eq!(w.get_feedback(), "Pasted!");
    w.invoke_apply();
    let availability = question(&bound);
    assert!(availability.get_message().contains("with a \"which\" call"));
    availability.invoke_answered(true);
    child.invoke_apply();
    options.invoke_apply();
    let persisted = saved(&store);
    let ActualCall::Process(process) = &persisted.calls[0].call else {
        panic!("saved pasted process");
    };
    let inputs = hydrus_core::external_calls::Inputs::from([(
        Parameter::Path,
        vec!["/synthetic/漢😀.png".into()],
    )]);
    assert_eq!(
        process.command(&inputs).unwrap(),
        [
            "owned-program",
            "--literal",
            "日本😀",
            "prefix:/synthetic/漢😀.png",
            "profile=\"My",
            "Profile\""
        ]
    );
    let options = open(&ui, &bound);
    options.invoke_external_call_clicked(0, false, false);
    options.invoke_external_call_action("edit".into());
    let child = call(&bound);
    child.invoke_command_edit();
    let w = command(&bound);
    assert_eq!(command_rows(&w), process.arguments);
    hydrus_gui::set_paster(|| "stale accepted-paste 日本😀".into());
    w.invoke_action("paste".into());
    let retired = question(&bound);
    options.invoke_cancel();
    retired.invoke_answered(true);
    w.invoke_apply();
    child.invoke_apply();
    assert_eq!(saved(&store), persisted);
    assert!(!bound.options_external_calls.has_open());

    // Keep the cancelled predecessor handles while a successor owns both
    // slots. Old actions must not publish clipboard text or disturb its draft.
    let successor_options = open(&ui, &bound);
    successor_options.invoke_external_call_clicked(0, false, false);
    successor_options.invoke_external_call_action("edit".into());
    let successor_child = call(&bound);
    successor_child.invoke_command_edit();
    let successor = command(&bound);
    hydrus_gui::set_paster(|| "successor-program --kept 日本😀".into());
    successor.invoke_action("paste".into());
    let successor_review = question(&bound);
    let draft = (
        successor.get_executable(),
        command_rows(&successor),
        successor
            .get_rows()
            .iter()
            .map(|row| row.selected)
            .collect::<Vec<_>>(),
        successor.get_full_template(),
        successor.get_feedback(),
    );
    let review_message = successor_review.get_message();
    headless::set_clipboard_text("successor clipboard sentinel 日本😀");
    let clipboard = headless::clipboard_text();
    let manager = saved(&store);
    for action in ["copy", "paste", "add", "delete"] {
        w.invoke_action(action.into());
    }
    w.invoke_apply();
    w.invoke_cancel();
    retired.invoke_answered(true);
    retired.invoke_answered(false);
    retired.invoke_cancelled();
    assert_eq!(headless::clipboard_text(), clipboard);
    assert_eq!(saved(&store), manager);
    assert_eq!(manager, persisted);
    assert_eq!(
        (
            successor.get_executable(),
            command_rows(&successor),
            successor
                .get_rows()
                .iter()
                .map(|row| row.selected)
                .collect::<Vec<_>>(),
            successor.get_full_template(),
            successor.get_feedback(),
        ),
        draft
    );
    assert_eq!(successor_review.get_message(), review_message);
    assert!(std::ptr::eq(command(&bound).window(), successor.window()));
    assert!(std::ptr::eq(
        question(&bound).window(),
        successor_review.window()
    ));
    assert!(std::ptr::eq(
        call(&bound).window(),
        successor_child.window()
    ));
    assert!(std::ptr::eq(
        bound.options.borrow().as_ref().unwrap().window(),
        successor_options.window()
    ));
    assert!(successor.window().is_visible());
    assert!(successor_review.window().is_visible());
    assert!(successor_child.window().is_visible());
    assert!(successor_options.window().is_visible());
    assert!(!w.window().is_visible());
    assert!(!retired.window().is_visible());
    assert!(!child.window().is_visible());
    assert!(!options.window().is_visible());

    // Current acceptance still updates only this draft; current Cancel closes
    // its slot normally and the outer Cancel preserves the complete manager.
    successor_review.invoke_answered(true);
    assert_eq!(successor.get_executable(), "successor-program");
    assert_eq!(command_rows(&successor), ["--kept", "日本😀"]);
    assert_eq!(
        successor.get_full_template(),
        "successor-program --kept 日本😀"
    );
    assert_eq!(successor.get_feedback(), "Pasted!");
    assert!(bound.options_external_calls.question.borrow().is_none());
    assert!(!successor_review.window().is_visible());
    assert!(std::ptr::eq(command(&bound).window(), successor.window()));
    successor.invoke_cancel();
    assert!(bound.options_external_calls.command.borrow().is_none());
    assert!(!successor.window().is_visible());
    assert!(std::ptr::eq(
        call(&bound).window(),
        successor_child.window()
    ));
    successor_options.invoke_cancel();
    assert!(!bound.options_external_calls.has_open());
    assert!(!successor_child.window().is_visible());
    assert!(!successor_options.window().is_visible());
    assert_eq!(saved(&store), persisted);
    assert_eq!(headless::clipboard_text(), clipboard);
}

#[test]
fn parameter_queue_reverse_edit_and_real_key_origin_histories_match_actual_qt() {
    use slint::platform::{Key, PointerEventButton, WindowEvent};
    let (_dirs, store) = store();
    let original = seed(&store);
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    let options = open(&ui, &bound);
    options.invoke_external_call_clicked(0, false, false);
    options.invoke_external_call_action("edit".into());
    let child = call(&bound);
    let reference = hydrus_testkit::fixture_json("external_command.json");
    for history in reference["queue_edges"].as_array().unwrap() {
        child.invoke_command_edit();
        let w = command(&bound);
        let command_index = windows.count() - 1;
        let native = windows.get(windows.count() - 1).unwrap();
        hydrus_gui::set_paster(|| "owned-program alpha beta gamma delta".into());
        w.invoke_action("paste".into());
        question(&bound).invoke_answered(true);
        headless::render(&native, 760, 590);
        for event in history["steps"].as_array().unwrap() {
            match event["action"].as_str().unwrap() {
                "initial" => {}
                action @ ("click_1" | "click_3" | "ctrl_click_1") => {
                    let control = action == "ctrl_click_1";
                    let row = if action == "click_3" { 3.0 } else { 1.0 };
                    if control {
                        w.window().dispatch_event(WindowEvent::KeyPressed {
                            text: Key::Control.into(),
                        });
                    }
                    let position = slint::LogicalPosition::new(
                        w.get_parameter_list_x() + 10.0,
                        w.get_parameter_list_y() + 24.0 + row * 22.0 + 11.0,
                    );
                    w.window().dispatch_event(WindowEvent::PointerPressed {
                        position,
                        button: PointerEventButton::Left,
                    });
                    w.window().dispatch_event(WindowEvent::PointerReleased {
                        position,
                        button: PointerEventButton::Left,
                    });
                    if control {
                        w.window().dispatch_event(WindowEvent::KeyReleased {
                            text: Key::Control.into(),
                        });
                    }
                }
                "edit_selection_first" => {
                    if history["name"] == "reverse_edit" {
                        assert!(w.window().is_visible());
                        assert!(w.get_selected());
                        assert!(!w.get_child_open());
                        // Child closure and selection update the actual
                        // controls; let their enabled/color animations finish.
                        let started = std::time::Instant::now();
                        loop {
                            headless::render(&native, 760, 590);
                            if started.elapsed() >= std::time::Duration::from_millis(35)
                                && !w.window().has_active_animations()
                            {
                                break;
                            }
                            assert!(
                                started.elapsed() < std::time::Duration::from_secs(2),
                                "selected command controls must settle before capture"
                            );
                            std::thread::sleep(std::time::Duration::from_millis(5));
                        }
                        assert!(w.get_selected());
                        assert!(!w.get_child_open());
                        command_capture(
                            &windows,
                            command_index,
                            "external-command-parameters-selected.png",
                            (760, 590),
                        );
                    }
                    w.invoke_action("edit".into());
                    let q = question(&bound);
                    assert_eq!(
                        q.get_text(),
                        event["entries"][0]["default"].as_str().unwrap()
                    );
                    q.invoke_name_entered("first-added 日本😀".into());
                }
                "shift_down" => {
                    command_key(&w, Key::DownArrow.into(), false, true);
                }
                "ctrl_home" => {
                    command_key(&w, Key::Home.into(), true, false);
                }
                "select_all" => {
                    command_key(&w, "a".into(), true, false);
                }
                "delete" => {
                    w.invoke_action("delete".into());
                    let q = question(&bound);
                    assert_eq!(
                        q.get_message(),
                        event["questions"][0]["message"].as_str().unwrap()
                    );
                    q.invoke_answered(true);
                }
                unexpected => panic!("unexpected edge {unexpected}"),
            }
            assert_eq!(
                serde_json::to_value(command_rows(&w)).unwrap(),
                event["rows"],
                "{} / {}",
                history["name"],
                event["action"]
            );
            let selected: Vec<usize> = w
                .get_rows()
                .iter()
                .enumerate()
                .filter(|(_, r)| r.selected)
                .map(|(i, _)| i)
                .collect();
            let mut expected: Vec<usize> =
                serde_json::from_value(event["selected"].clone()).unwrap();
            expected.sort_unstable();
            assert_eq!(
                selected, expected,
                "{} / {}",
                history["name"], event["action"]
            );
        }
        w.invoke_cancel();
        assert_eq!(saved(&store).calls, std::slice::from_ref(&original));
    }
    options.invoke_cancel();
    assert!(!bound.options_external_calls.has_open());
}

// Capture the owned adapter only after real timers and widget animations settle.
// Larger supported views make the long import warning readable; this is not a
// universal/default-size geometry assertion or a physical list-button test.
fn list_capture(
    windows: &headless::Windows,
    index: usize,
    window: &slint::Window,
    name: &str,
    size: (u32, u32),
) {
    use std::time::{Duration, Instant};
    let adapter = windows.get(index).unwrap();
    let started = Instant::now();
    let mut previous = None;
    let pixels = loop {
        let pixels = headless::render(&adapter, size.0, size.1);
        if started.elapsed() >= Duration::from_millis(35)
            && !window.has_active_animations()
            && previous.as_ref() == Some(&pixels)
        {
            break pixels;
        }
        assert!(
            started.elapsed() < Duration::from_secs(2),
            "{name}: live list/question did not settle; visible={}, animations={}",
            window.is_visible(),
            window.has_active_animations(),
        );
        previous = Some(pixels);
        std::thread::sleep(Duration::from_millis(5));
    };
    assert!(
        window.is_visible(),
        "{name}: capture must be a live owned window"
    );
    headless::save_png(
        &std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join(name),
        &pixels,
        size.0,
        size.1,
    )
    .unwrap();
}
fn list_rows(w: &OptionsWindow) -> Vec<(Vec<String>, bool)> {
    w.get_external_call_rows()
        .iter()
        .map(|row| {
            (
                row.cells.iter().map(|cell| cell.to_string()).collect(),
                row.selected,
            )
        })
        .collect()
}
fn assert_duplicate_manager(manager: &Manager, original: &Manager) {
    for call in &original.calls {
        assert_eq!(
            manager
                .calls
                .iter()
                .find(|candidate| candidate.key == call.key),
            Some(call)
        );
        let duplicate = manager
            .calls
            .iter()
            .find(|candidate| candidate.name == format!("{} (1)", call.name))
            .unwrap();
        assert!(
            !original
                .calls
                .iter()
                .any(|candidate| candidate.key == duplicate.key)
        );
        let mut expected = call.clone();
        expected.key = duplicate.key;
        expected.name.clone_from(&duplicate.name);
        assert_eq!(duplicate, &expected);
    }
}
fn retired_duplicate_question_preserves_successor(
    ui: &MainWindow,
    bound: &Bound,
    store: &Store,
    original: &Manager,
    oracle: &serde_json::Value,
) {
    let retired_options = open(ui, bound);
    retired_options.invoke_external_call_sort(0, true);
    retired_options.invoke_external_call_clicked(0, false, false);
    retired_options.invoke_external_call_clicked(2, false, true);
    retired_options.invoke_external_call_action("duplicate".into());
    let retired = question(bound);
    assert_eq!(
        retired.get_message().as_str(),
        oracle["duplicate_warnings"][0]["questions"][0]["message"]
            .as_str()
            .unwrap()
    );
    retired_options.invoke_cancel();
    assert!(bound.options_external_calls.question.borrow().is_none());
    assert!(!retired.window().is_visible());
    assert!(!retired_options.window().is_visible());

    let successor = open(ui, bound);
    successor.invoke_external_call_sort(0, true);
    successor.invoke_external_call_clicked(0, false, false);
    successor.invoke_external_call_clicked(2, false, true);
    successor.invoke_external_call_action("duplicate".into());
    let current = question(bound);
    let rows = list_rows(&successor);
    let flags = (
        successor.get_external_call_selected(),
        successor.get_external_call_single(),
        successor.get_external_call_child_open(),
        successor.get_external_call_sort_column(),
        successor.get_external_call_ascending(),
        successor.get_external_call_error(),
    );
    let message = current.get_message();
    for answer in [true, false] {
        retired.invoke_answered(answer);
    }
    retired.invoke_cancelled();
    retired.invoke_force_close();
    retired_options.invoke_external_call_action("duplicate".into());
    retired_options.invoke_external_call_action("delete".into());
    retired_options.invoke_apply();
    retired_options.invoke_cancel();
    assert_eq!(saved(store), *original);
    assert_eq!(list_rows(&successor), rows);
    assert_eq!(
        (
            successor.get_external_call_selected(),
            successor.get_external_call_single(),
            successor.get_external_call_child_open(),
            successor.get_external_call_sort_column(),
            successor.get_external_call_ascending(),
            successor.get_external_call_error(),
        ),
        flags
    );
    assert_eq!(current.get_message(), message);
    assert!(std::ptr::eq(question(bound).window(), current.window()));
    assert!(std::ptr::eq(
        bound.options.borrow().as_ref().unwrap().window(),
        successor.window()
    ));
    assert!(current.window().is_visible());
    assert!(successor.window().is_visible());
    assert!(!retired.window().is_visible());
    assert!(!retired_options.window().is_visible());
    assert!(bound.options_external_calls.editor.borrow().is_none());
    assert!(bound.options_external_calls.command.borrow().is_none());
    assert!(bound.options_external_calls.defaults.borrow().is_none());
    assert!(bound.options_external_calls.exchange.0.borrow().is_none());

    // Only the current answer completes the remaining duplicate queue.
    current.invoke_answered(true);
    assert!(bound.options_external_calls.question.borrow().is_none());
    assert!(!current.window().is_visible());
    let complete = list_rows(&successor);
    assert_eq!(
        serde_json::to_value(complete.iter().map(|row| &row.0[0]).collect::<Vec<_>>()).unwrap(),
        oracle["duplicate_warnings"][1]["names"]
    );
    assert!(complete.iter().all(|row| row.1));
    assert_eq!(saved(store), *original);
    successor.invoke_cancel();
    assert!(bound.options.borrow().is_none());
    assert!(!bound.options_external_calls.has_open());
    assert_eq!(saved(store), *original);
}

fn retired_delete_question_preserves_successor(ui: &MainWindow, bound: &Bound, store: &Store) {
    // Use the existing OS file/URL fixture; do not rewrite the Store.
    let persisted = saved(store);
    assert_eq!(persisted.calls.len(), 2);
    let retired_options = open(ui, bound);
    retired_options.invoke_external_call_clicked(0, false, false);
    retired_options.invoke_external_call_action("delete".into());
    let retired = question(bound);
    assert_eq!(retired.get_message(), "Remove all selected?");
    retired_options.invoke_cancel();
    assert!(bound.options_external_calls.question.borrow().is_none());
    assert!(!retired.window().is_visible());
    assert!(!retired_options.window().is_visible());

    let successor = open(ui, bound);
    let last = successor.get_external_call_rows().row_count() - 1;
    successor.invoke_external_call_clicked(i32::try_from(last).unwrap(), false, false);
    successor.invoke_external_call_action("delete".into());
    let current = question(bound);
    let rows = list_rows(&successor);
    assert_eq!(rows.len(), 2);
    assert!(rows[last].1);
    assert!(!rows[0].1);
    let flags = list_flags(&successor);
    let message = current.get_message();
    retired.invoke_answered(true);
    retired.invoke_answered(false);
    retired.invoke_cancelled();
    retired.invoke_force_close();
    retired_options.invoke_external_call_action("delete".into());
    retired_options.invoke_external_call_action("duplicate".into());
    retired_options.invoke_apply();
    retired_options.invoke_cancel();
    assert_eq!(saved(store), persisted);
    assert_eq!(list_rows(&successor), rows);
    assert_eq!(list_flags(&successor), flags);
    assert_eq!(current.get_message(), message);
    assert!(std::ptr::eq(question(bound).window(), current.window()));
    assert!(std::ptr::eq(
        bound.options.borrow().as_ref().unwrap().window(),
        successor.window()
    ));
    assert!(current.window().is_visible());
    assert!(successor.window().is_visible());
    assert!(!retired.window().is_visible());
    assert!(!retired_options.window().is_visible());
    assert!(bound.options_external_calls.editor.borrow().is_none());
    assert!(bound.options_external_calls.command.borrow().is_none());
    assert!(bound.options_external_calls.defaults.borrow().is_none());
    assert!(bound.options_external_calls.exchange.0.borrow().is_none());

    // Current Yes removes exactly its captured last row, never the old row0.
    current.invoke_answered(true);
    let expected = rows
        .into_iter()
        .enumerate()
        .filter(|(i, _)| *i != last)
        .map(|(_, row)| row)
        .collect::<Vec<_>>();
    assert_eq!(list_rows(&successor), expected);
    assert!(!successor.get_external_call_selected());
    assert!(bound.options_external_calls.question.borrow().is_none());
    assert!(!current.window().is_visible());
    assert!(successor.window().is_visible());
    assert_eq!(saved(store), persisted);
    successor.invoke_cancel();
    assert!(bound.options.borrow().is_none());
    assert!(!bound.options_external_calls.has_open());
    assert_eq!(saved(store), persisted);
}
fn list_flags(w: &OptionsWindow) -> (bool, bool, bool, i32, bool, slint::SharedString) {
    (
        w.get_external_call_selected(),
        w.get_external_call_single(),
        w.get_external_call_child_open(),
        w.get_external_call_sort_column(),
        w.get_external_call_ascending(),
        w.get_external_call_error(),
    )
}
