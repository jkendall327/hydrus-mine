//! The current network jobs list shows every request the engine holds, one
//! row per job with its typed wait, status, speed and bytes as the reference's
//! columns do (`ConvertValueRangeToBytes`, speed per second); the selected
//! job's detail names its network contexts and whether it obeys bandwidth.
use hydrus_core::network::NetworkContext;
use hydrus_core::time::TimestampMs;
use hydrus_gui::{headless, network_data_window as windows};
use hydrus_store::Store;
use hydrus_store::network_runtime::{NetworkJob, Snapshot, WaitReason};
use hydrus_store::settings;
use slint::Model as _;

fn until(mut condition: impl FnMut() -> bool) {
    let start = std::time::Instant::now();
    while !condition() {
        assert!(start.elapsed() < std::time::Duration::from_secs(8));
        slint::platform::update_timers_and_animations();
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}

// leaf: audit-network-jobs-waits
#[test]
fn every_request_is_a_row_with_its_typed_wait_speed_bytes_and_contexts() {
    let _headless = headless::init();
    let slots = windows::Slots::default();
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let now = TimestampMs::now().millis() / 1000;
    let waits = [
        WaitReason::Engine,
        WaitReason::Paused,
        WaitReason::Headers,
        WaitReason::Wake,
        WaitReason::Bandwidth,
        WaitReason::Domain,
        WaitReason::Gallery,
        WaitReason::Connection,
        WaitReason::ServerBandwidth,
        WaitReason::Downloading,
        WaitReason::Login,
    ];
    let jobs: Vec<NetworkJob> = waits
        .iter()
        .enumerate()
        .map(|(i, wait)| NetworkJob {
            id: i as u64 + 1,
            url: format!("https://site{i}.example/file"),
            status: format!("status {i}"),
            wait: *wait,
            speed: 1024 * (i as u64),
            bytes_read: 2048,
            bytes_total: (i % 2 == 0).then_some(4096),
            contexts: vec![
                NetworkContext::global(),
                NetworkContext::domain(format!("site{i}.example")),
            ],
            obeys_bandwidth: i % 3 != 0,
        })
        .collect();
    {
        let jobs = jobs.clone();
        store
            .write(move |ctx| {
                settings::set(
                    ctx.conn(),
                    &Snapshot {
                        epoch: "daemon".into(),
                        at: now,
                        usage: Vec::new(),
                        jobs,
                        ..Snapshot::default()
                    },
                )
            })
            .unwrap();
    }
    let window = windows::open_jobs(store.clone(), &slots).unwrap();
    until(|| window.get_rows().row_count() == waits.len());
    let cells = |r: usize| -> Vec<String> {
        let row = window.get_rows().row_data(r).unwrap();
        (0..row.cells.row_count())
            .map(|c| row.cells.row_data(c).unwrap().to_string())
            .collect()
    };
    let columns: Vec<String> = window
        .get_columns()
        .iter()
        .map(|c| c.title.to_string())
        .collect();
    assert_eq!(columns, ["position", "url", "status", "speed", "progress"]);
    for (i, wait) in waits.iter().enumerate() {
        // (rows may be in any order: find ours by URL)
        let url = format!("https://site{i}.example/file");
        let row = (0..waits.len()).map(cells).find(|c| c[1] == url).unwrap();
        assert_eq!(row[0], wait.label());
        assert_eq!(row[2], format!("status {i}"));
        let speed = jobs[i].speed;
        assert_eq!(
            row[3],
            format!(
                "{}/s",
                hydrus_gui_model::gui_format::bytes(
                    &hydrus_store::settings::GuiFormatting::default(),
                    speed
                )
            )
        );
        assert_eq!(
            row[4],
            if i % 2 == 0 { "2 KB/4 KB" } else { "2 KB" },
            "{wait:?}"
        );
    }
    // (every wait is its own phase)
    let mut labels: Vec<_> = waits.iter().map(|w| w.label()).collect();
    labels.sort_unstable();
    labels.dedup();
    assert_eq!(labels.len(), waits.len());
    // the selected job's detail: its contexts and whether it obeys bandwidth
    for i in [0, 1, 4] {
        let url = format!("https://site{i}.example/file");
        let at = (0..waits.len()).find(|&r| cells(r)[1] == url).unwrap();
        window.invoke_row_clicked(i32::try_from(at).unwrap(), false, false);
        let detail = window.get_detail().to_string();
        assert!(detail.contains(&url), "{detail}");
        assert!(
            detail.contains(&NetworkContext::global().to_human_string()),
            "{detail}"
        );
        assert!(
            detail.contains(&NetworkContext::domain(format!("site{i}.example")).to_human_string()),
            "{detail}"
        );
        assert!(
            detail.contains(&format!("obeys bandwidth: {}", i % 3 != 0)),
            "{detail}"
        );
    }
}
