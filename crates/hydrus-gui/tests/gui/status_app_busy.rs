//! The status bar's application-busy field: the reference's thread-pool
//! wording and thresholds ("working" above 3 jobs, "busy" above 8, looked
//! at most every ten seconds, with the tooltip "There were N threads doing
//! jobs at last check."), counting the daemon's running downloader queues.

use hydrus_store::live::DaemonLive;

use crate::options_gui_support::Client;

fn daemon_says(client: &Client, jobs: u32, at_ms: i64) {
    client
        .store
        .write(move |ctx| {
            hydrus_store::settings::set(
                ctx.conn(),
                &DaemonLive {
                    started: 1,
                    bytes: 0,
                    speed: 0,
                    at: at_ms / 1000,
                    jobs,
                },
            )
        })
        .unwrap();
}

// leaf: audit-options-status-activity
#[test]
fn the_application_busy_field_says_how_many_jobs_run_as_the_reference_does() {
    let client = Client::basic();
    // (no maintenance pass of the client's own to count with the daemon's queues)
    client
        .store
        .write(|ctx| {
            hydrus_store::settings::set(
                ctx.conn(),
                &hydrus_store::maintenance_gates::Preferences {
                    trash_normal: false,
                    deferred_normal: false,
                },
            )
        })
        .unwrap();
    // (and it is not idle time either, when they would run regardless)
    client
        .store
        .write(|ctx| {
            hydrus_store::settings::set(
                ctx.conn(),
                &hydrus_store::settings::GuiIdleSettings {
                    enabled: false,
                    ..Default::default()
                },
            )
        })
        .unwrap();
    // (and any pass admitted as the client opened is done)
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    while [
        hydrus_store::maintenance_gates::Worker::Trash,
        hydrus_store::maintenance_gates::Worker::Deferred,
    ]
    .into_iter()
    .any(|worker| client.bound.maintenance.running(worker))
    {
        assert!(
            std::time::Instant::now() < deadline,
            "a pass never finished"
        );
        client
            .bound
            .maintenance
            .poll_at(hydrus_core::TimestampMs::now().0)
            .unwrap();
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    let base = hydrus_core::TimestampMs::now().0 + 10_000_000;
    let at = |seconds: i64| base + seconds * 1000;
    let poll = |seconds: i64| client.bound.maintenance.poll_at(at(seconds)).unwrap();
    let shown = || {
        (
            client.ui.get_status_app_busy().to_string(),
            client.ui.get_status_app_busy_tip().to_string(),
        )
    };

    // quiet: nothing shown, but the tooltip has the count
    daemon_says(&client, 2, at(0));
    poll(0);
    assert_eq!(
        shown(),
        (
            String::new(),
            "There were 2 threads doing jobs at last check.".into()
        )
    );
    // (looked at once in ten seconds: busy now, but not yet seen)
    daemon_says(&client, 9, at(5));
    poll(5);
    assert_eq!(shown().0, "");
    poll(10);
    assert_eq!(shown().0, "", "ten seconds is not more than ten");
    poll(11);
    assert_eq!(
        shown(),
        (
            "busy".into(),
            "There were 9 threads doing jobs at last check.".into()
        )
    );
    // the thresholds: 3 is quiet, 4 to 8 working, 9 busy
    for (i, (jobs, wanted)) in [
        (3, ""),
        (4, "working"),
        (8, "working"),
        (9, "busy"),
        (1, ""),
    ]
    .into_iter()
    .enumerate()
    {
        let step = 100 + i64::try_from(i).unwrap() * 20;
        daemon_says(&client, jobs, at(step));
        poll(step);
        assert_eq!(shown().0, wanted, "{jobs} jobs");
        assert_eq!(
            shown().1,
            format!("There were {jobs} threads doing jobs at last check.")
        );
    }
    // a daemon that has not said anything lately (it has stopped) runs none
    daemon_says(&client, 12, at(1000));
    poll(1100);
    assert_eq!(
        shown(),
        (
            String::new(),
            "There were 0 threads doing jobs at last check.".into()
        )
    );

    // and the database field says what this client's connection is doing
    // while it does it
    let (started_tx, started_rx) = std::sync::mpsc::channel();
    let (release_tx, release_rx) = std::sync::mpsc::channel::<()>();
    let writer = {
        let store = client.store.clone();
        std::thread::spawn(move || {
            store
                .write(move |_| {
                    started_tx.send(()).unwrap();
                    release_rx.recv().unwrap();
                    Ok(())
                })
                .unwrap();
        })
    };
    started_rx.recv().unwrap();
    poll(1200);
    assert_eq!(client.ui.get_status_db(), "db writing");
    release_tx.send(()).unwrap();
    writer.join().unwrap();
    poll(1201);
    assert_eq!(client.ui.get_status_db(), "");
    let stats = client.bound.maintenance.statistics();
    assert_eq!(
        (stats.trash_passes, stats.deferred_passes),
        (0, 0),
        "no pass of the client's own ran"
    );
}

// leaf: audit-options-status-activity
#[test]
fn a_running_maintenance_pass_of_the_client_counts_as_a_job() {
    use hydrus_store::maintenance_gates::{Preferences, Worker};

    let client = Client::basic();
    // only the trash pass may be admitted, at normal time
    client
        .store
        .write(|ctx| {
            hydrus_store::settings::set(
                ctx.conn(),
                &Preferences {
                    trash_normal: true,
                    deferred_normal: false,
                },
            )?;
            hydrus_store::settings::set(
                ctx.conn(),
                &hydrus_store::settings::GuiIdleSettings {
                    enabled: false,
                    ..Default::default()
                },
            )
        })
        .unwrap();
    // wait out anything admitted as the client opened
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    while client.bound.maintenance.running(Worker::Trash)
        || client.bound.maintenance.running(Worker::Deferred)
    {
        assert!(
            std::time::Instant::now() < deadline,
            "a pass never finished"
        );
        client
            .bound
            .maintenance
            .poll_at(hydrus_core::TimestampMs::now().0)
            .unwrap();
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    // hold the writer, so the pass the next poll starts waits in its first
    // write: a genuinely running job
    let (held_tx, held_rx) = std::sync::mpsc::channel();
    let (release_tx, release_rx) = std::sync::mpsc::channel::<()>();
    let holder = {
        let store = client.store.clone();
        std::thread::spawn(move || {
            store
                .write(move |_| {
                    held_tx.send(()).unwrap();
                    release_rx.recv().unwrap();
                    Ok(())
                })
                .unwrap();
        })
    };
    held_rx.recv().unwrap();
    let base = hydrus_core::TimestampMs::now().0 + 100_000_000;
    let at = |seconds: i64| base + seconds * 1000;
    let poll = |seconds: i64| client.bound.maintenance.poll_at(at(seconds)).unwrap();
    // (no daemon has said anything: the pass is the only job)
    poll(0);
    assert!(
        client.bound.maintenance.running(Worker::Trash),
        "the pass started"
    );
    poll(11);
    assert_eq!(
        client.ui.get_status_app_busy_tip(),
        "There were 1 threads doing jobs at last check."
    );
    // it finishes when the writer is let go, and is no longer counted
    release_tx.send(()).unwrap();
    holder.join().unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    let mut second = 22;
    while client.bound.maintenance.running(Worker::Trash) {
        assert!(
            std::time::Instant::now() < deadline,
            "the pass never finished"
        );
        poll(second);
        second += 11;
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    poll(second + 11);
    assert_eq!(
        client.ui.get_status_app_busy_tip(),
        "There were 0 threads doing jobs at last check."
    );
}
