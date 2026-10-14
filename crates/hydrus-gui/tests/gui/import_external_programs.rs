//! The import options editor's external programs page, against the
//! reference's `EditExternalProgramsImportOptionsPanel` and its entry editor
//! (`oracle/fixtures/import_external_programs.json`,
//! `record_import_external_programs.py`): the list, adding, editing and
//! deleting entries with the reference's dialogs and notices, what Apply
//! saves, and the programs then run by an import using those options.
#![cfg(unix)]

use hydrus_core::external_calls::{
    ActualCall, Callable, Manager, Parameter, Pipeline, Process, Rule,
};
use hydrus_core::import_options::{
    CallerType, ExternalProgramsOptions, ImportOptionsManager, ImportOptionsSlice,
};
use hydrus_gui::{
    ExternalProgramEntryWindow, ImportOptionsWindow, MainWindow, Pages, SessionDialog, bind,
    headless,
};
use hydrus_store::queues;
use serde_json::Value;
use slint::{ComponentHandle as _, Model as _};

const EXTERNAL_PROGRAMS: i32 = 6;

fn recorded() -> Value {
    hydrus_testkit::fixture_json("import_external_programs.json")
}

fn key(n: u8) -> [u8; 32] {
    [n; 32]
}

fn logger(log: &std::path::Path, first: &str, tokens: &[(Parameter, &str)]) -> Process {
    let mut arguments = vec![
        "-c".to_owned(),
        format!(
            "for a in \"$@\"; do printf '%s\\n' \"$a\" >> '{}'; done; echo --- >> '{}'",
            log.display(),
            log.display()
        ),
        "sh".to_owned(),
        first.to_owned(),
    ];
    arguments.extend(tokens.iter().map(|(_, t)| (*t).to_owned()));
    Process {
        executable: "/bin/sh".into(),
        arguments,
        rules: tokens.iter().map(|(p, _)| Rule::new(*p)).collect(),
        ..Process::default()
    }
}

fn manager(log: &std::path::Path, with_broken: bool) -> Manager {
    let call = |n: u8, name: &str, pipeline: Pipeline, call: ActualCall| Callable {
        key: key(n),
        name: name.into(),
        pipeline,
        call,
    };
    let mut calls = vec![
        call(
            1,
            "Zulu URL",
            Pipeline::Url,
            ActualCall::Process(Process::default()),
        ),
        call(
            2,
            "beta log",
            Pipeline::File,
            ActualCall::Process(logger(
                log,
                "beta",
                &[
                    (Parameter::Path, "%path%"),
                    (Parameter::Uri, "%path_uri%"),
                    (Parameter::Hash, "%hash%"),
                ],
            )),
        ),
        call(
            3,
            "Alpha 日本 log",
            Pipeline::File,
            ActualCall::Process(logger(log, "alpha", &[(Parameter::Path, "%path%")])),
        ),
        call(
            5,
            "Default OS File Launch",
            Pipeline::File,
            ActualCall::DefaultFile,
        ),
        call(
            6,
            "gamma file id",
            Pipeline::File,
            ActualCall::Process(logger(log, "gamma", &[(Parameter::FileId, "%file_id%")])),
        ),
    ];
    if with_broken {
        calls.push(call(
            4,
            "broken",
            Pipeline::File,
            ActualCall::Process(Process {
                executable: "/nonexistent/no-such-program".into(),
                arguments: vec!["%path%".into()],
                rules: vec![Rule::new(Parameter::Path)],
                ..Process::default()
            }),
        ));
    }
    Manager { calls }
}

struct Setup {
    ui: MainWindow,
    bound: hydrus_gui::Bound,
    store: std::sync::Arc<hydrus_store::Store>,
    queue: i64,
    log: std::path::PathBuf,
    _dirs: [tempfile::TempDir; 2],
}

fn setup() -> Setup {
    let (dirs, store) = crate::subscriptions::store();
    let log = dirs[1].path().join("programs.log");
    let calls = manager(&log, true);
    store
        .write(move |ctx| hydrus_store::settings::set(ctx.conn(), &calls))
        .unwrap();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    ui.invoke_new_page();
    ui.invoke_chooser_pressed(4);
    ui.invoke_chooser_pressed(8);
    let queue = bound.current.borrow().borrow().importer().unwrap().queue;
    Setup {
        ui,
        bound,
        store,
        queue,
        log,
        _dirs: dirs,
    }
}

impl Setup {
    fn editor(&self) -> ImportOptionsWindow {
        self.ui.invoke_importer_import_options();
        let editor = self
            .bound
            .folders
            .import_options
            .borrow()
            .as_ref()
            .unwrap()
            .clone_strong();
        editor.invoke_kind_clicked(EXTERNAL_PROGRAMS);
        editor.set_custom_index(1);
        editor.invoke_changed();
        editor
    }

    fn saved(&self) -> ImportOptionsSlice {
        let queue = self.queue;
        self.store
            .read(move |conn| queues::queue(conn, queue))
            .unwrap()
            .unwrap()
            .options
    }
}

fn list(editor: &ImportOptionsWindow) -> Vec<String> {
    editor
        .get_program_rows()
        .iter()
        .map(|row| row.cells.row_data(0).unwrap().to_string())
        .collect()
}

fn recorded_list(event: &Value) -> Vec<String> {
    event["list"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t.as_str().unwrap().to_owned())
        .collect()
}

fn entry_window() -> ExternalProgramEntryWindow {
    hydrus_gui::external_program_entry_window().expect("the entry editor is open")
}

fn dialog() -> SessionDialog {
    hydrus_gui::external_program_dialog().expect("a notice or question is open")
}

/// What the reference's entry dialog showed, against ours.
fn check_dialog(window: &ExternalProgramEntryWindow, seen: &Value) {
    assert_eq!(window.get_window_title(), seen["title"].as_str().unwrap());
    let programs: Vec<String> = window
        .get_programs()
        .iter()
        .map(|p| p.to_string())
        .collect();
    let expected: Vec<&str> = seen["programs"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p[0].as_str().unwrap())
        .collect();
    assert_eq!(programs, expected);
    let whens: Vec<String> = window.get_whens().iter().map(|p| p.to_string()).collect();
    let expected: Vec<&str> = seen["whens"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p[0].as_str().unwrap())
        .collect();
    assert_eq!(whens, expected);
    // which are chosen when it opens
    let initial_program = seen["initial"]["program"].as_str().unwrap();
    let position = seen["programs"]
        .as_array()
        .unwrap()
        .iter()
        .position(|p| p[1] == initial_program)
        .unwrap();
    assert_eq!(
        usize::try_from(window.get_program_index()).unwrap(),
        position
    );
    let initial_when = &seen["initial"]["when"];
    let position = seen["whens"]
        .as_array()
        .unwrap()
        .iter()
        .position(|w| &w[1] == initial_when)
        .unwrap();
    assert_eq!(usize::try_from(window.get_when_index()).unwrap(), position);
}

fn choose(window: &ExternalProgramEntryWindow, program: &str, when: usize) {
    let index = window
        .get_programs()
        .iter()
        .position(|p| p == program)
        .unwrap();
    window.set_program_index(i32::try_from(index).unwrap());
    window.set_when_index(i32::try_from(when).unwrap());
}

// leaf: import-external-program-entry
#[test]
fn programs_are_authored_in_the_import_options_and_run_by_imports_like_the_reference() {
    let _windows = headless::init();
    let recorded = recorded();
    let s = setup();
    let editor = s.editor();
    assert!(list(&editor).is_empty());
    let events = recorded["add_events"].as_array().unwrap();
    let event = |label: &str| events.iter().find(|e| e["label"] == label).unwrap();

    // add: cancelled, then the default, then one of each way
    editor.invoke_program_action("add".into());
    assert!(editor.get_program_child_open());
    let window = entry_window();
    check_dialog(&window, &event("add-cancel")["seen"]);
    window.invoke_cancelled();
    assert!(!editor.get_program_child_open());
    assert_eq!(list(&editor), recorded_list(event("add-cancel")));

    for (label, program, when) in [
        ("add-default", None, 0),
        ("add-all-new-beta", Some("beta log"), 1),
        ("add-already-in-alpha", Some("Alpha 日本 log"), 2),
        ("add-all-os", Some("Default OS File Launch"), 0),
        ("add-none-broken", Some("broken"), 3),
    ] {
        editor.invoke_program_action("add".into());
        let window = entry_window();
        check_dialog(&window, &event(label)["seen"]);
        if let Some(program) = program {
            choose(&window, program, when);
        }
        window.invoke_accepted();
        assert_eq!(list(&editor), recorded_list(event(label)), "{label}");
    }
    // nothing reaches the importer until Apply
    assert!(s.saved().external_programs.is_none());
    // the last list's own summary, as the editor's list names the kind
    let reference =
        ExternalProgramsOptions::from_serialised(&event("add-none-broken")["value"][2].to_string())
            .unwrap();
    assert!(
        editor
            .get_labels()
            .iter()
            .any(|l| l.contains(&reference.summary())),
        "{:?} should say {}",
        editor.get_labels().iter().collect::<Vec<_>>(),
        reference.summary()
    );

    // edit the first: another call, for all imports; then a cancelled edit
    editor.invoke_program_clicked(0, false, false);
    editor.invoke_program_action("edit".into());
    let window = entry_window();
    check_dialog(&window, &recorded["edit"]["seen"]);
    choose(&window, "Alpha 日本 log", 0);
    window.invoke_accepted();
    assert_eq!(
        list(&editor)[0],
        recorded["edit"]["after"].as_str().unwrap()
    );
    editor.invoke_program_clicked(0, false, false);
    editor.invoke_program_action("edit".into());
    entry_window().invoke_cancelled();
    assert_eq!(
        list(&editor)[0],
        recorded["edit"]["after"].as_str().unwrap()
    );
    assert_eq!(recorded["edit_cancel"], "CancelledException");

    // delete the OS default (and one more) after the reference's question
    editor.invoke_program_clicked(3, false, false);
    editor.invoke_program_clicked(4, true, false);
    editor.invoke_program_action("delete".into());
    assert_eq!(dialog().get_message(), "Remove 2 selected?");
    dialog().invoke_answered(false);
    assert_eq!(list(&editor).len(), 5, "answering no deletes nothing");
    editor.invoke_program_action("delete".into());
    dialog().invoke_answered(true);
    assert_eq!(list(&editor).len(), 3);
    // (only the OS default, which is not run on an import here, goes after
    // all: put the broken one back)
    editor.invoke_program_action("add".into());
    choose(&entry_window(), "broken", 3);
    entry_window().invoke_accepted();
    assert_eq!(list(&editor).len(), 4);

    // Apply: what the importer is saved with
    editor.invoke_apply();
    let saved = s
        .saved()
        .external_programs
        .expect("custom external programs");
    assert_eq!(saved.entries.len(), 4);
    assert_eq!(saved.entries[0].name, "Alpha 日本 log");
    assert_eq!(
        (saved.entries[0].on_new, saved.entries[0].on_already_in),
        (true, true)
    );
    assert_eq!(
        saved.entries[1].summary(),
        "call beta log on all new file imports"
    );
    assert_eq!(
        saved.entries[2].summary(),
        "call Alpha 日本 log on all \"already in db\" imports"
    );
    assert_eq!(
        saved.entries[3].summary(),
        "call broken on no imports (will do nothing!)"
    );

    // the importer's programs are then run by an import using them, as the
    // reference runs them: for a new file, in the order added
    let services = s.store.snapshot();
    let full =
        ImportOptionsManager::default().full(CallerType::SpecificImporter, Some(&s.saved()), &[]);
    let options = hydrus_import::FileImportOptions::from_full(&full, &services.services);
    assert_eq!(options.external_programs, saved.entries);
    let source = s.log.with_file_name("source.bmp");
    std::fs::write(&source, hydrus_testkit::bmp(1, 20000)).unwrap();
    let importer =
        hydrus_import::FileImporter::new(s.store.clone(), hydrus_media::MediaTools::new());
    let result = importer.import_path(&source, &options).unwrap();
    assert_eq!(result.status, hydrus_import::ImportStatus::SuccessfulAndNew);
    let hash = result.hash.unwrap();
    let path = services
        .storage
        .file_path(&hash, hydrus_core::Mime::ImageBmp)
        .unwrap();
    let log = std::fs::read_to_string(&s.log).unwrap();
    let ran: Vec<Vec<&str>> = log
        .split("---\n")
        .filter(|c| !c.is_empty())
        .map(|c| c.lines().collect())
        .collect();
    // (the recorded "two-in-order": alpha with the path, then beta with
    // path, URI and hash; here the first entry is alpha for all imports)
    let two = recorded["runs"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["label"] == "two-in-order")
        .unwrap();
    let shape = |line: &Value| line[0].as_str().unwrap().to_owned();
    let expected: Vec<String> = two["lines"].as_array().unwrap().iter().map(shape).collect();
    assert_eq!(
        ran.iter().map(|c| c[0].to_owned()).collect::<Vec<_>>(),
        expected
    );
    assert_eq!(ran[0][1], path.display().to_string());
    assert_eq!(ran[1][1], path.display().to_string());
    assert_eq!(ran[1][2], format!("file://{}", path.display()));
    assert_eq!(ran[1][3], hash.to_hex());
}

// leaf: import-external-program-entry
#[test]
fn the_entry_editor_gives_the_notices_the_reference_gives() {
    let _windows = headless::init();
    let recorded = recorded();
    let s = setup();
    // an entry whose call is not registered (any more): a call is registered,
    // chosen, applied, and then taken from the registered ones
    let mut with_temporary = manager(&s.log, true);
    with_temporary.calls.push(Callable {
        key: key(9),
        name: "gone program".into(),
        pipeline: Pipeline::File,
        call: ActualCall::Process(Process::default()),
    });
    s.store
        .write(move |ctx| hydrus_store::settings::set(ctx.conn(), &with_temporary))
        .unwrap();
    let editor = s.editor();
    editor.invoke_program_action("add".into());
    choose(&entry_window(), "gone program", 1);
    entry_window().invoke_accepted();
    editor.invoke_apply();
    let all = manager(&s.log, true);
    s.store
        .write(move |ctx| hydrus_store::settings::set(ctx.conn(), &all))
        .unwrap();
    let editor = s.editor();
    assert_eq!(
        list(&editor),
        [recorded["edit_missing"]["summary"].as_str().unwrap()]
    );
    editor.invoke_program_clicked(0, false, false);
    editor.invoke_program_action("edit".into());
    let information = &recorded["edit_missing"]["messages"][0];
    assert_eq!(dialog().get_message(), information[1].as_str().unwrap());
    assert!(hydrus_gui::external_program_entry_window().is_none());
    dialog().invoke_cancelled();
    // then the editor, the first call chosen
    check_dialog(&entry_window(), &recorded["edit_missing"]["seen"]);
    entry_window().invoke_cancelled();
    assert_eq!(list(&editor).len(), 1);

    // with no "send single file" calls registered at all
    s.store
        .write(|ctx| hydrus_store::settings::set(ctx.conn(), &Manager::default()))
        .unwrap();
    editor.invoke_program_action("add".into());
    let critical = &recorded["no_calls"]["messages"][0];
    assert_eq!(dialog().get_window_title(), critical[1].as_str().unwrap());
    assert_eq!(dialog().get_message(), critical[2].as_str().unwrap());
    dialog().invoke_cancelled();
    assert!(hydrus_gui::external_program_entry_window().is_none());
    assert_eq!(list(&editor).len(), 1);
    assert!(!editor.get_program_child_open());
}
