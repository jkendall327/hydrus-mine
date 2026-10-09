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
    poll(9);
    assert_eq!(shown().0, "");
    poll(10);
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
}
