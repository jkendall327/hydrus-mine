//! "Include the file system in this wait": after the computer wakes from
//! sleep, asking for a file's path waits out the wake delay when the option
//! is on, as `oracle/fixtures/file_system_wake_wait.json` recorded from the
//! reference's files manager.

use std::time::Instant;

use hydrus_core::time::TimestampMs;
use hydrus_core::{Mime, Sha256};
use hydrus_store::Store;
use hydrus_store::network::NetworkSettings;
use hydrus_store::reference_options::ReferenceOptions;

// leaf: audit-options-system-system-sleep-include-the-file-system-in-this-wait
#[test]
fn file_paths_wait_out_the_wake_delay_when_asked_as_recorded() {
    let recording = hydrus_testkit::fixture_json("file_system_wake_wait.json");
    let delay = recording["delay"].as_u64().unwrap();
    for case in recording["cases"].as_array().unwrap() {
        let waits = case["file_system_waits"].as_bool().unwrap();
        let detect = case["detect"].as_bool().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path()).unwrap();
        // the options, as the Options window saves them
        store
            .write(move |ctx| {
                let mut network: NetworkSettings = hydrus_store::settings::get(ctx.conn())?;
                network.detect_sleep = detect;
                network.wake_delay_period = delay;
                hydrus_store::settings::set(ctx.conn(), &network)?;
                let mut options: ReferenceOptions = hydrus_store::settings::get(ctx.conn())?;
                options.set_boolean("file_system_waits_on_wakeup", waits);
                hydrus_store::settings::set(ctx.conn(), &options)
            })
            .unwrap();
        // (the poll that notices changed options)
        assert!(store.refresh_if_changed().unwrap(), "{case}");
        // a wake from sleep: the last check was an hour ago
        let snapshot = store.snapshot();
        snapshot
            .wake_gate
            .check_at(TimestampMs::now().0 - 3_600_000, snapshot.wake);
        store.sleep_check();

        let hash = Sha256([7; 32]);
        let started = Instant::now();
        let path = store.snapshot().storage.file_path(&hash, Mime::ImagePng);
        let blocked = started.elapsed().as_secs_f64().round() as u64;
        assert!(path.is_some());
        assert_eq!(blocked, case["blocked_seconds"].as_u64().unwrap(), "{case}");
        let just_woke = snapshot
            .wake_gate
            .just_woke_at(TimestampMs::now().0, snapshot.wake);
        assert_eq!(just_woke, case["just_woke_after"], "{case}");
    }
}
