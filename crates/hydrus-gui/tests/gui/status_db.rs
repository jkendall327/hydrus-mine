//! The status bar's database field and its tooltip against the reference's
//! (oracle/fixtures/status_db.json, from oracle/record_status_db.py): what
//! the database is doing (reading, writing, committing, locked, nothing),
//! and "current db job: <the job>" while it has one. The reference names a
//! job by its action ("read status_test"); hydrus-rs's jobs are closures,
//! named by the function they are written in.

use serde_json::Value;

use crate::options_gui_support::Client;

/// The recorded tooltip with the reference's action replaced by ours.
fn tip(recorded: &Value, action: &str, ours: &str) -> String {
    let tooltip = recorded["tooltip"].as_str().unwrap();
    assert!(tooltip.ends_with(action), "{tooltip}");
    format!("{}{ours}", tooltip.strip_suffix(action).unwrap())
}

// leaf: audit-options-status-activity
#[test]
fn the_database_field_says_what_the_database_does_and_its_job_as_the_reference_does() {
    let recorded = hydrus_testkit::fixture_json("status_db.json");
    let client = Client::basic();
    // (time held still: after the first poll no maintenance pass falls due)
    let now = hydrus_core::TimestampMs::now().0;
    let shown = || {
        client.bound.maintenance.poll_at(now).unwrap();
        (
            client.ui.get_status_db().to_string(),
            client.ui.get_status_db_tip().to_string(),
        )
    };
    let wanted =
        |case: &Value, tooltip: String| (case["text"].as_str().unwrap().to_owned(), tooltip);
    let until = |what: &str, want: &(String, String)| {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        let mut seen = shown();
        while &seen != want {
            assert!(
                std::time::Instant::now() < deadline,
                "{what}: {seen:?}, not {want:?}"
            );
            std::thread::sleep(std::time::Duration::from_millis(20));
            seen = shown();
        }
    };
    // (a job is named by the function it is written in: this test)
    let here = "the_database_field_says_what_the_database_does_and_its_job_as_the_reference_does";

    // nothing
    let idle = &recorded["idle"];
    until(
        "idle",
        &wanted(idle, idle["tooltip"].as_str().unwrap().into()),
    );

    // a read held in its work
    let (started, started_rx) = std::sync::mpsc::channel();
    let (release, released) = std::sync::mpsc::channel::<()>();
    let reader = {
        let store = client.store.clone();
        std::thread::spawn(move || {
            store
                .read(move |_| {
                    started.send(()).unwrap();
                    released.recv().unwrap();
                    Ok(())
                })
                .unwrap();
        })
    };
    started_rx.recv().unwrap();
    let reading = &recorded["reading"];
    until(
        "reading",
        &wanted(
            reading,
            tip(reading, "read status_test", &format!("read {here}")),
        ),
    );
    release.send(()).unwrap();
    reader.join().unwrap();

    // a write held in its work
    let (started, started_rx) = std::sync::mpsc::channel();
    let (release, released) = std::sync::mpsc::channel::<()>();
    let writer = {
        let store = client.store.clone();
        std::thread::spawn(move || {
            store
                .write(move |_| {
                    started.send(()).unwrap();
                    released.recv().unwrap();
                    Ok(())
                })
                .unwrap();
        })
    };
    started_rx.recv().unwrap();
    let writing = &recorded["writing"];
    until(
        "writing",
        &wanted(
            writing,
            tip(writing, "write status_test", &format!("write {here}")),
        ),
    );

    // its commit held: committing, the job still named. (The tooltip names
    // the batch's last job: a write of the client's own joining this batch
    // would be named instead, so the comparison waits until ours is shown;
    // the client's timers are not pumped here, so none is expected.)
    let hold = client.store.hold_next_commit();
    release.send(()).unwrap();
    let committing = &recorded["committing"];
    until(
        "committing",
        &wanted(
            committing,
            tip(committing, "write status_commit", &format!("write {here}")),
        ),
    );
    drop(hold);
    writer.join().unwrap();

    // paused (as for a backup): locked, the tooltip its text
    let paused = client.store.pause().unwrap();
    let locked = &recorded["locked"];
    until(
        "locked",
        &wanted(locked, locked["tooltip"].as_str().unwrap().into()),
    );
    drop(paused);
    let after = &recorded["after"];
    until(
        "after",
        &wanted(after, after["tooltip"].as_str().unwrap().into()),
    );
}
