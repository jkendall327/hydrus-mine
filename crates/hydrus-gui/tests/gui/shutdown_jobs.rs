//! The shutdown box of Options > maintenance and processing against the
//! reference's own exit (`TryToExit`, then `DoIdleShutdownWork`), replaying
//! `oracle/fixtures/shutdown_work.json` (`oracle/record_shutdown_work.py`):
//! each recorded case sets the box through the real Options window and the
//! last shutdown work's time, exits the real window (its close button, or
//! File > exit/force maintenance), answers "Maintenance is due" as the
//! recording did, and compares the question, whether the client exited,
//! whether the work ran, when it was told to stop, and what was registered.

use hydrus_gui::{OptionRow, OptionsWindow};
use hydrus_store::settings::ShutdownWork;
use serde_json::Value;
use slint::{ComponentHandle as _, Model as _};

use super::options_system_consumers::{Client, client, row_in};

fn shutdown_row(options: &OptionsWindow, label: &str) -> (i32, OptionRow) {
    row_in(options, "shutdown", label)
}

const ACTION: &str = "Run jobs on shutdown: ";
const PERIOD: &str = "Only run shutdown jobs once per: ";
const MINUTES: &str = "Max number of minutes to run shutdown jobs: ";

/// Whether the database has been analysed (planner statistics exist).
fn analysed(client: &Client) -> bool {
    client
        .store
        .read(|conn| {
            Ok(conn
                .query_row(
                    "SELECT 1 FROM sqlite_master WHERE name = 'sqlite_stat1'",
                    [],
                    |_| Ok(()),
                )
                .is_ok())
        })
        .unwrap()
}

fn seconds_now() -> i64 {
    hydrus_core::TimestampMs::now().secs()
}

/// File > `label`.
fn file_menu(client: &Client, label: &str) {
    let ui = &client.ui;
    ui.invoke_menu_title_pressed(0, 10.0, 22.0);
    let lines = ui.get_menu_panes().row_data(0).unwrap().lines;
    let at = (0..lines.row_count())
        .position(|i| lines.row_data(i).unwrap().label == label)
        .unwrap_or_else(|| panic!("file > {label}"));
    ui.invoke_menu_line_clicked(0, at as i32, 0.0, 0.0, 0.0);
}

/// One recorded case, replayed; `now` is the second the exit started in.
fn replay(client: &mut Client, case: &Value) {
    let name = case["name"].as_str().unwrap();
    // a client started afresh, its database due an analysis unless the
    // recording says the last work left nothing to do
    client.ui.show().unwrap();
    client.bound = hydrus_gui::bind(
        &client.ui,
        hydrus_gui::Pages::open(client.store.clone()).unwrap(),
    );
    let nothing_left =
        case["work_due_now"].as_array().unwrap().is_empty() && case["do_shutdown_work"] == false;
    if !nothing_left {
        client
            .store
            .write(|ctx| {
                ctx.conn()
                    .execute_batch("DROP TABLE IF EXISTS sqlite_stat1")?;
                hydrus_store::settings::set(
                    ctx.conn(),
                    &hydrus_store::db_maintenance::AnalyzeTimestamps::default(),
                )
            })
            .unwrap();
    }
    // the box, through File > options
    let options = client.options("maintenance and processing");
    // (the period and minutes first: they are disabled under "do not run")
    let (i, _) = shutdown_row(&options, ACTION);
    options.invoke_choice_chosen(i, 2);
    let period = case["period"].as_i64().unwrap();
    let (i, _) = shutdown_row(&options, PERIOD);
    for (field, value) in [period / 86_400, period % 86_400 / 3600, period % 3600 / 60]
        .into_iter()
        .enumerate()
    {
        options.invoke_field_edited(i, field as i32, value as i32);
    }
    let (i, _) = shutdown_row(&options, MINUTES);
    options.invoke_number_edited(i, case["minutes"].as_i64().unwrap() as i32);
    let (i, _) = shutdown_row(&options, ACTION);
    options.invoke_choice_chosen(i, case["action"].as_i64().unwrap() as i32);
    options.invoke_apply();
    let saved: ShutdownWork = client.get();
    assert_eq!(
        (
            i64::from(saved.action),
            saved.period_seconds as i64,
            i64::from(saved.max_minutes)
        ),
        (
            case["action"].as_i64().unwrap(),
            period,
            case["minutes"].as_i64().unwrap()
        ),
        "{name}"
    );
    // the last shutdown work "ago" seconds before an exit early in a second
    // (so a boundary of a whole period does not slip by a second)
    while hydrus_core::TimestampMs::now().0 % 1000 >= 200 {
        std::thread::yield_now();
    }
    let now = seconds_now();
    let last = case["ago"].as_i64().map_or(0, |ago| now - ago);
    client
        .store
        .write(move |ctx| {
            let mut work: ShutdownWork = hydrus_store::settings::get(ctx.conn())?;
            work.last_done = last;
            hydrus_store::settings::set(ctx.conn(), &work)
        })
        .unwrap();
    let ran_before = hydrus_gui_model::shutdown_work::last_run();
    match case["how"].as_str().unwrap() {
        "force" => file_menu(client, "exit/force maintenance"),
        _ => client
            .ui
            .window()
            .dispatch_event(slint::platform::WindowEvent::CloseRequested),
    }
    let questions = case["questions"].as_array().unwrap();
    match questions.as_slice() {
        [] => assert!(
            hydrus_gui::client_exit::maintenance_question().is_none(),
            "{name}: not asked"
        ),
        [question] => {
            let shown = hydrus_gui::client_exit::maintenance_question()
                .unwrap_or_else(|| panic!("{name}: asked"));
            assert_eq!(
                shown.get_window_title(),
                question["title"].as_str().unwrap()
            );
            assert_eq!(question["kwargs"]["auto_no_time"], 15);
            // the outstanding work is this database's own (its tables differ
            // from the reference's), worded the same
            let message = shown.get_message().to_string();
            let recorded = question["message"].as_str().unwrap();
            let (head, tail) = recorded.split_at(recorded.find("analyze ").unwrap());
            assert!(tail.ends_with(" table_names"), "{tail}");
            let (shown_head, shown_tail) = message.split_at(message.find("analyze ").unwrap());
            assert_eq!(shown_head, head, "{name}");
            let count = shown_tail
                .strip_prefix("analyze ")
                .and_then(|t| t.strip_suffix(" table_names"))
                .unwrap_or_else(|| panic!("{name}: {shown_tail}"));
            assert!(count.replace(',', "").parse::<u64>().unwrap() > 0);
            assert!(client.ui.window().is_visible(), "{name}: the exit waits");
            match case["answer"].as_str().unwrap() {
                "yes" => shown.invoke_answered(true),
                "no" => shown.invoke_answered(false),
                "cancel" => shown.invoke_cancelled(),
                other => panic!("{other}"),
            }
        }
        _ => panic!("{name}: one question at most"),
    }
    assert_eq!(
        !client.ui.window().is_visible(),
        case["exited"].as_bool().unwrap(),
        "{name}: exited"
    );
    let worked = case["do_shutdown_work"].as_bool().unwrap() && case["exited"] == true;
    let ran = hydrus_gui_model::shutdown_work::last_run();
    let done = client.get::<ShutdownWork>().last_done;
    if worked {
        assert_ne!(ran, ran_before, "{name}: the work ran");
        let (started, stop) = ran.unwrap();
        assert_eq!(started, now, "{name}");
        assert_eq!(
            stop - started,
            case["maintain"][0]["stop_time"].as_i64().unwrap(),
            "{name}: told to stop after its minutes"
        );
        assert!(analysed(client), "{name}: the analysis was run");
        // registered when the work finished, not when it started
        assert!((now..=seconds_now()).contains(&done), "{name}: {done}");
    } else {
        assert_eq!(ran, ran_before, "{name}: no work");
        let registered = case["last_after_exit_question"].as_i64().unwrap();
        // "no" registers at once, so the question is not asked again; anything
        // else leaves the time alone
        assert_eq!(
            done - now,
            if registered == 0 { 0 } else { last - now },
            "{name}"
        );
        assert_eq!(registered, if registered == 0 { 0 } else { last - now });
    }
    if !nothing_left && !worked {
        assert!(!analysed(client), "{name}: nothing analysed");
    }
}

// leaf: audit-options-maintenance-and-processing-when-to-run-high-cpu-jobs-shutdown-run-jobs-on-shutdown
// leaf: audit-options-maintenance-and-processing-when-to-run-high-cpu-jobs-shutdown-only-run-shutdown-jobs-once-per
// leaf: audit-options-maintenance-and-processing-when-to-run-high-cpu-jobs-shutdown-max-number-of-minutes-to-run-shutdown-jobs
#[test]
fn exiting_does_the_shutdown_work_the_reference_does() {
    let recorded = hydrus_testkit::fixture_json("shutdown_work.json");
    let cases = recorded["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 18);
    let mut client = client();
    for case in cases {
        replay(&mut client, case);
    }
}

#[test]
fn the_shutdown_box_rows_are_the_references() {
    use hydrus_gui_model::shutdown_work::ACTIONS;
    let client = client();
    let options = client.options("maintenance and processing");
    let (_, row) = shutdown_row(&options, ACTION);
    let items: Vec<String> = row.items.iter().map(|s| s.to_string()).collect();
    assert_eq!(items, ACTIONS, "the reference's three choices");
    assert_eq!(row.index, 2, "ask first is the default");
    let (_, row) = shutdown_row(&options, PERIOD);
    assert_eq!(row.fields.row_count(), 3, "days, hours and minutes");
    assert_eq!(row.fields.row_data(0).unwrap().value, 1, "one day");
    let (_, row) = shutdown_row(&options, MINUTES);
    assert_eq!(
        (row.kind, row.minimum, row.maximum, row.number),
        (2, 1, 1440, 5)
    );
    // neither is any use while jobs are not run on shutdown
    let (i, _) = shutdown_row(&options, ACTION);
    options.invoke_choice_chosen(i, 0);
    options.invoke_apply();
    let options = client.options("maintenance and processing");
    assert!(!shutdown_row(&options, PERIOD).1.enabled);
    assert!(!shutdown_row(&options, MINUTES).1.enabled);
    options.invoke_cancel();
}
