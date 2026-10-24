//! The desktop's idle time, read from a real X server (Xvfb, with input made
//! through the XTEST extension) and from a session bus (a private
//! `dbus-daemon`) serving GNOME's idle monitor. In a file of its own: it starts
//! servers. CI installs them and sets `HYDRUS_REQUIRE_DESKTOP=1`, which turns
//! a missing one into a failure.
#![cfg(all(unix, not(target_vendor = "apple"), not(target_os = "android")))]

use std::io::{BufRead as _, BufReader};
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use hydrus_gui::system_idle::{Desktop, Platform, SystemIdle as _};

/// A server process that prints one line (its address) and is killed on drop.
struct Server(Child);

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn start(program: &str, args: &[&str]) -> Option<(Server, String)> {
    let child = Command::new(program)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn();
    let mut child = match child {
        Ok(child) => child,
        Err(error) => {
            assert!(
                std::env::var_os("HYDRUS_REQUIRE_DESKTOP").is_none(),
                "{program} is needed: {error}"
            );
            eprintln!("skipping: no {program} ({error})");
            return None;
        }
    };
    // (read on a thread: a server that never says its address must fail the
    // test, not hang it)
    let stdout = child.stdout.take().unwrap();
    let server = Server(child);
    let (sender, receiver) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut line = String::new();
        BufReader::new(stdout).read_line(&mut line).unwrap();
        let _ = sender.send(line);
    });
    let line = receiver
        .recv_timeout(Duration::from_secs(10))
        .unwrap_or_else(|_| panic!("{program} did not say its address"));
    Some((server, line.trim().to_owned()))
}

/// The platform's idle time once it has been sampled.
fn sampled(platform: &Platform) -> u64 {
    let started = Instant::now();
    loop {
        if let Some(idle) = platform.idle_ms() {
            return idle;
        }
        assert!(
            started.elapsed() < Duration::from_secs(10),
            "no idle time came"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
}

#[test]
fn the_x_servers_idle_time_grows_and_input_resets_it() {
    use x11rb::connection::Connection as _;
    use x11rb::protocol::xproto::ConnectionExt as _;
    use x11rb::protocol::xtest::ConnectionExt as _;

    let Some((_xvfb, number)) = start(
        "Xvfb",
        &[
            "-displayfd",
            "1",
            "-screen",
            "0",
            "64x64x8",
            "-nolisten",
            "tcp",
        ],
    ) else {
        return;
    };
    let display = format!(":{number}");
    let platform = Platform::start_at(Desktop {
        // (no session bus here: only the X server can answer)
        bus: Some("unix:path=/nonexistent/bus".into()),
        display: Some(display.clone()),
        wayland: false,
    });
    let first = sampled(&platform);
    std::thread::sleep(Duration::from_secs(2));
    let later = platform.idle_ms().unwrap();
    assert!(later >= first + 1400, "grew from {first} to {later}");

    // a key pressed on the desktop, outside any of our windows
    let (connection, _) = x11rb::connect(Some(&display)).unwrap();
    let setup = connection.setup();
    let keycode = setup.min_keycode.max(10);
    for kind in [2u8, 3u8] {
        connection
            .xtest_fake_input(kind, keycode, 0, 0, 0, 0, 0)
            .unwrap();
    }
    // (a reply to a request after them says the server has taken them)
    connection.get_input_focus().unwrap().reply().unwrap();
    let started = Instant::now();
    loop {
        let idle = platform.idle_ms().unwrap();
        if idle < later {
            break;
        }
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "input did not reset the idle time: {idle}"
        );
        std::thread::sleep(Duration::from_millis(100));
    }
}

struct Monitor(Arc<AtomicU64>);

#[zbus::interface(name = "org.gnome.Mutter.IdleMonitor")]
impl Monitor {
    fn get_idletime(&self) -> u64 {
        self.0.load(Ordering::Relaxed)
    }
}

#[test]
fn gnomes_idle_monitor_on_the_session_bus_is_asked_when_there_is_no_x_server() {
    let Some((_bus, address)) = start(
        "dbus-daemon",
        &["--session", "--nofork", "--print-address=1"],
    ) else {
        return;
    };
    let told = Arc::new(AtomicU64::new(120_000));
    let _service = zbus::blocking::connection::Builder::address(address.as_str())
        .unwrap()
        .name("org.gnome.Mutter.IdleMonitor")
        .unwrap()
        .serve_at("/org/gnome/Mutter/IdleMonitor/Core", Monitor(told.clone()))
        .unwrap()
        .build()
        .unwrap();
    let platform = Platform::start_at(Desktop {
        bus: Some(address),
        display: None,
        // (a Wayland session: the X server is not asked)
        wayland: true,
    });
    let idle = sampled(&platform);
    assert!((120_000..125_000).contains(&idle), "{idle}");
    // the person types: the monitor says so at the next sample
    told.store(500, Ordering::Relaxed);
    let started = Instant::now();
    while platform.idle_ms().unwrap() > 5_000 {
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "the monitor's answer did not come"
        );
        std::thread::sleep(Duration::from_millis(100));
    }
}

#[test]
fn a_desktop_with_neither_says_nothing() {
    let platform = Platform::start_at(Desktop {
        bus: Some("unix:path=/nonexistent/bus".into()),
        display: None,
        wayland: true,
    });
    std::thread::sleep(Duration::from_millis(1500));
    assert_eq!(platform.idle_ms(), None);
}

/// The running client's idle-only "last session" autosave, end to end: the
/// main window bound to a store, watching a real X server's idle time as the
/// client does as it opens (`watch_the_desktop`). A key pressed on the
/// desktop, outside any of the client's windows, holds the save back, to be
/// tried again after the reference's wait (`oracle/fixtures/session_autosave.json`,
/// its "auto session save wait loop"); once the desktop has been left alone
/// for longer than the user idle time, the save goes ahead.
// leaf: audit-options-gui-sessions-sessions-if-last-session-above-only-autosave-during-idle-time
#[test]
fn input_on_the_desktop_holds_back_the_running_client_s_idle_only_autosave() {
    use hydrus_gui::{MainWindow, Pages, bind, headless};
    use hydrus_store::settings::{self, GuiIdleSettings, GuiSessionSettings};
    use hydrus_store::{Store, sessions};
    use x11rb::connection::Connection as _;
    use x11rb::protocol::xproto::ConnectionExt as _;
    use x11rb::protocol::xtest::ConnectionExt as _;

    let recorded = hydrus_testkit::fixture_json("session_autosave.json");
    let wait = recorded["steps"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["only_idle"] == true && s["idle"] == false)
        .unwrap()["calls"][0]
        .clone();
    assert_eq!(wait["label"], "auto session save wait loop");
    let wait_ms = wait["delay"].as_i64().unwrap() * 1000;

    let Some((_xvfb, number)) = start(
        "Xvfb",
        &[
            "-displayfd",
            "1",
            "-screen",
            "0",
            "64x64x8",
            "-nolisten",
            "tcp",
        ],
    ) else {
        return;
    };
    let display = format!(":{number}");

    let _windows = headless::init();
    let legacy = hydrus_testkit::legacy_fixture("basic");
    let native = tempfile::tempdir().unwrap();
    hydrus_store::import::import_legacy(
        legacy.path(),
        &native.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store = Store::open(native.path()).unwrap();
    store
        .write(move |ctx| {
            settings::set(
                ctx.conn(),
                &GuiSessionSettings {
                    autosave_minutes: 2,
                    only_during_idle: true,
                    ..GuiSessionSettings::default()
                },
            )?;
            settings::set(
                ctx.conn(),
                &GuiIdleSettings {
                    enabled: true,
                    // (two seconds of the desktop left alone is idle)
                    user_seconds: Some(2),
                    mouse_seconds: None,
                    api_seconds: None,
                    busy_cpu_percent: 50,
                    busy_cpu_count: None,
                },
            )
        })
        .unwrap();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    ui.invoke_search_edited("system:everything".into());
    ui.invoke_search_accepted();
    bound.session_autosave.watch_the_desktop(Desktop {
        // (no session bus here: only the X server can answer)
        bus: Some("unix:path=/nonexistent/bus".into()),
        display: Some(display.clone()),
        wayland: false,
    });
    let autosave = &bound.session_autosave;
    let saved = || {
        store
            .read(|conn| hydrus_store::session_backups::latest(conn, sessions::LAST_SESSION))
            .unwrap()
            .is_some()
    };
    // booted, and our own windows quiet, since long ago (past the reference's
    // two minutes after boot that are never idle)
    autosave.boot_at(0);
    autosave.user_at(0);
    autosave.mouse_at(0);
    let due = autosave.next().unwrap();
    // (the client's clock runs on from the save's due time, with the desktop's)
    let started = Instant::now();
    let clock = |offset: i64| due + offset + i64::try_from(started.elapsed().as_millis()).unwrap();
    let wait_until = |offset: i64, idle: bool, what: &str| loop {
        let now = clock(offset);
        if autosave.idle_at(now) == idle {
            return now;
        }
        assert!(started.elapsed() < Duration::from_secs(30), "{what}");
        std::thread::sleep(Duration::from_millis(50));
    };
    // (the new desktop left alone long enough to be idle first)
    wait_until(0, true, "the desktop never went idle");

    // a key pressed on the desktop, outside any of our windows
    let (connection, _) = x11rb::connect(Some(&display)).unwrap();
    let keycode = connection.setup().min_keycode.max(10);
    for kind in [2u8, 3u8] {
        connection
            .xtest_fake_input(kind, keycode, 0, 0, 0, 0, 0)
            .unwrap();
    }
    connection.get_input_focus().unwrap().reply().unwrap();
    let now = wait_until(0, false, "the key press never reached the client");
    assert!(!autosave.poll_at(now).unwrap());
    assert!(!saved());
    assert_eq!(autosave.next(), Some(now + wait_ms));

    // the desktop left alone past the user idle time: idle, and saved
    let now = wait_until(wait_ms, true, "the desktop never went idle again");
    assert!(autosave.poll_at(now).unwrap());
    assert!(saved());
}
