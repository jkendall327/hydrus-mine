//! Exiting with shutdown maintenance: when it runs, what it asks, and the work.
use hydrus_gui_model::shutdown_work::{Decision, ExitMode, decide, run, work_due};
use hydrus_store::settings::ShutdownWork;

#[test]
fn shutdown_work_runs_asks_or_skips_as_the_reference_decides() {
    let work = vec!["analyze 3 table_names".to_owned()];
    let due = ShutdownWork {
        last_done: 0,
        ..ShutdownWork::default()
    };
    let now = 100_000;
    assert_eq!(
        decide(&due, ExitMode::Exit, now, &work),
        Decision::Ask("Is now a good time for the client to do up to 5 minutes' maintenance work? (Will auto-no in 15 seconds)\n\nThe outstanding jobs appear to be:\n\nanalyze 3 table_names".into())
    );
    assert_eq!(decide(&due, ExitMode::Exit, now, &[]), Decision::Skip);
    let recent = ShutdownWork {
        last_done: now - 10,
        ..ShutdownWork::default()
    };
    assert_eq!(
        decide(&recent, ExitMode::Restart, now, &work),
        Decision::Skip
    );
    assert_eq!(
        decide(&recent, ExitMode::ForceMaintenance, now, &[]),
        Decision::Run
    );
    let always = ShutdownWork {
        action: 1,
        ..due.clone()
    };
    assert_eq!(decide(&always, ExitMode::Exit, now, &[]), Decision::Run);
    let never = ShutdownWork { action: 0, ..due };
    assert_eq!(decide(&never, ExitMode::Exit, now, &work), Decision::Skip);
    assert_eq!(
        ExitMode::Restart.question(),
        "Are you sure you want to restart the client? (Will auto-yes in 15 seconds)"
    );
}

#[test]
fn the_work_analyzes_the_due_tables_and_registers_itself() {
    let dir = tempfile::tempdir().unwrap();
    let store = hydrus_store::Store::open(dir.path()).unwrap();
    let before = work_due(&store);
    assert_eq!(before.len(), 1);
    assert!(before[0].starts_with("analyze "));
    run(&store, 12_345).unwrap();
    let saved: ShutdownWork = store.read(hydrus_store::settings::get).unwrap();
    assert_eq!(saved.last_done, 12_345);
}
