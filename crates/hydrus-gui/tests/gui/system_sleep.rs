//! Options > system > system sleep, from the two rows to the network engine
//! the daemon runs, replaying `oracle/fixtures/system_sleep_options.json`
//! (`oracle/record_system_sleep_options.py`: the reference's
//! `Controller.SleepCheck` run for each case).
use super::options_window::{open, row, show_page, store};
use hydrus_gui::{MainWindow, Pages, bind, headless};
use hydrus_net::{Job, NetEngine, NetOptions, Request};
use hydrus_store::network::NetworkSettings;
use hydrus_store::settings::get;
use slint::ComponentHandle as _;
use std::sync::Arc;

const DETECT: &str = "Allow wake-from-system-sleep detection:";
const DELAY: &str =
    "After a wake from system sleep, wait this many seconds before allowing new network access:";

/// Tick and type the two rows, as a user would, and apply.
fn set(ui: &MainWindow, bound: &hydrus_gui::Bound, enabled: bool, delay: i64) {
    open(ui);
    let window = bound.options.borrow().as_ref().unwrap().clone_strong();
    show_page(&window, "system");
    window.invoke_check_toggled(row(&window, DETECT).0, enabled);
    window.invoke_number_edited(row(&window, DELAY).0, i32::try_from(delay).unwrap());
    window.invoke_apply();
}

/// The engine the daemon makes from the saved options (`hydrus serve`).
fn engine(store: &Arc<hydrus_store::Store>) -> NetEngine {
    let network = store.read(get::<NetworkSettings>).unwrap();
    NetEngine::new(store.clone(), NetOptions::from_settings(&network)).unwrap()
}

fn now_ms() -> i64 {
    i64::try_from(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis(),
    )
    .unwrap()
}

/// Whether a request is held for the wake (the reference's job status), or
/// goes (and fails at once: nothing listens there).
fn held(engine: &NetEngine) -> bool {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let job = Job::new();
    let request = Request::get("http://127.0.0.1:9/");
    let waited = runtime.block_on(async {
        tokio::time::timeout(
            std::time::Duration::from_millis(300),
            engine.fetch(&request, &job),
        )
        .await
        .is_err()
    });
    waited && job.state().status == "looks like computer just woke up, waiting a bit"
}

// leaf: audit-options-system-system-sleep-allow-wake-from-system-sleep-detection
// leaf: audit-options-system-system-sleep-after-a-wake-from-system-sleep-wait-this-many-seconds-before-allowing-new-network-access
#[test]
fn sleep_rows_reach_the_daemon_s_engine_and_replay_the_reference_s_checks() {
    let recorded = hydrus_testkit::fixture_json("system_sleep_options.json");
    let (_dirs, store) = store();
    // (the fixture's client has new network traffic paused, which holds
    // requests before any wake would)
    store
        .write(|ctx| {
            let mut pauses: hydrus_store::settings::Pauses = get(ctx.conn())?;
            pauses.network_traffic = false;
            hydrus_store::settings::set(ctx.conn(), &pauses)
        })
        .unwrap();
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    // (the recorder's checks ran at T, its last check `gap_ms` before)
    let recorded_t = 1_700_000_000_000_i64;
    for case in recorded["cases"].as_array().unwrap() {
        let enabled = case["enabled"].as_bool().unwrap();
        let delay = case["delay"].as_i64().unwrap();
        let gap = case["gap_ms"].as_i64().unwrap();
        set(&ui, &bound, enabled, delay);
        // at the recorder's own times
        let net = engine(&store);
        net.sleep_check_at(recorded_t - gap);
        net.sleep_check_at(recorded_t);
        let (last, awake_at) = net.wake_state();
        assert_eq!(awake_at.is_some(), case["detected"], "{case}");
        assert_eq!(awake_at.unwrap_or(0), case["deadline_ms"], "{case}");
        assert_eq!(last == Some(recorded_t), case["last_check_touched"], "{case}");
        // the daemon checks every 15 s; once the delay is over, requests go
        let delay_ms = delay * 1000;
        for elapsed in (15_000..=delay_ms).step_by(15_000) {
            net.sleep_check_at(recorded_t + elapsed);
        }
        net.sleep_check_at(recorded_t + delay_ms + 1);
        assert_eq!(net.wake_state().1.is_some(), case["after_delay"], "{case}");

        // and now, a request is held for the delay after the wake
        let net = engine(&store);
        let now = now_ms();
        net.sleep_check_at(now - gap);
        net.sleep_check_at(now);
        let deadline = net.wake_state().1;
        assert_eq!(
            deadline.map(|at| at - now),
            case["detected"]
                .as_bool()
                .unwrap()
                .then(|| case["deadline_ms"].as_i64().unwrap() - recorded_t),
            "{case}"
        );
        assert_eq!(held(&net), deadline.is_some_and(|at| at > now), "{case}");
    }

    // a wait still pending when the row is unticked ends at the next check,
    // in the engine already running (it reloads what the window saved)
    let pending = &recorded["disabled_pending"];
    set(&ui, &bound, true, 60);
    let net = engine(&store);
    let now = now_ms();
    net.sleep_check_at(now - 61_000);
    net.sleep_check_at(now);
    assert_eq!(net.wake_state().1.is_some(), pending["before"]);
    assert!(held(&net));
    set(&ui, &bound, false, 60);
    assert!(net.reload_settings().unwrap());
    net.sleep_check_at(now + 1);
    assert_eq!(net.wake_state().1.is_some(), pending["after"]);
    assert!(!held(&net));

    // out-of-range delays are clamped as the reference's spin box clamps them
    for clamp in recorded["clamps"].as_array().unwrap() {
        set(&ui, &bound, true, clamp["typed"].as_i64().unwrap());
        assert_eq!(
            store.read(get::<NetworkSettings>).unwrap().wake_delay_period,
            clamp["saved"].as_u64().unwrap()
        );
    }
}
