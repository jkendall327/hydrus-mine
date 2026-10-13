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
    let mut line = String::new();
    BufReader::new(child.stdout.take().unwrap())
        .read_line(&mut line)
        .unwrap();
    Some((Server(child), line.trim().to_owned()))
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
    std::thread::sleep(Duration::from_millis(1500));
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
