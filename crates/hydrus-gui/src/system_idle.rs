//! How long the person has been away from the keyboard and mouse, asked of
//! the desktop. The reference polls the global cursor position to learn
//! that the mouse has moved outside its windows; winit cannot read the
//! pointer there, so hydrus-rs asks the desktop how long ago it last saw
//! any keyboard or mouse input instead, and counts that against the idle
//! timers.

use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};
use std::sync::{Arc, Weak};

/// A source of the desktop's idle time.
pub trait SystemIdle {
    /// Milliseconds since the last keyboard or mouse input anywhere on the
    /// desktop, or none where the desktop can't say.
    fn idle_ms(&self) -> Option<u64>;
}

/// No idle source: only input over hydrus-rs's own windows counts.
#[derive(Debug)]
pub struct Unavailable;

impl SystemIdle for Unavailable {
    fn idle_ms(&self) -> Option<u64> {
        None
    }
}

/// Where to ask: the session bus and X display of the desktop, and whether
/// it is a Wayland session (where the X server sees only X clients' input).
#[derive(Debug, Clone, Default)]
pub struct Desktop {
    /// The session bus's address (`DBUS_SESSION_BUS_ADDRESS`), else the
    /// usual one.
    pub bus: Option<String>,
    /// The X display (`DISPLAY`).
    pub display: Option<String>,
    pub wayland: bool,
}

impl Desktop {
    pub fn from_env() -> Self {
        Self {
            bus: std::env::var("DBUS_SESSION_BUS_ADDRESS").ok(),
            display: std::env::var("DISPLAY").ok().filter(|d| !d.is_empty()),
            wayland: std::env::var_os("WAYLAND_DISPLAY").is_some()
                || std::env::var("XDG_SESSION_TYPE").is_ok_and(|kind| kind == "wayland"),
        }
    }
}

const UNKNOWN: u64 = u64::MAX;

#[derive(Debug, Default)]
struct Sample {
    /// The idle time the desktop gave, or [`UNKNOWN`].
    idle_ms: AtomicU64,
    /// When it gave it (milliseconds since the Unix epoch).
    at_ms: AtomicI64,
}

/// The platform's idle time, sampled once a second on a thread of its own
/// so a slow bus or X server never stalls the interface.
#[derive(Debug)]
pub struct Platform {
    sample: Arc<Sample>,
}

impl Platform {
    /// Ask the desktop this session runs on.
    pub fn start() -> Self {
        Self::start_at(Desktop::from_env())
    }

    /// Ask this desktop.
    pub fn start_at(desktop: Desktop) -> Self {
        let sample = Arc::new(Sample {
            idle_ms: AtomicU64::new(UNKNOWN),
            at_ms: AtomicI64::new(0),
        });
        let weak = Arc::downgrade(&sample);
        let spawned = std::thread::Builder::new()
            .name("system-idle".into())
            .spawn(move || sample_loop(&weak, desktop));
        if let Err(error) = spawned {
            eprintln!("could not watch the desktop's idle time: {error}");
        }
        Self { sample }
    }
}

impl SystemIdle for Platform {
    fn idle_ms(&self) -> Option<u64> {
        let idle = self.sample.idle_ms.load(Ordering::Relaxed);
        if idle == UNKNOWN {
            return None;
        }
        let age = hydrus_core::TimestampMs::now()
            .0
            .saturating_sub(self.sample.at_ms.load(Ordering::Relaxed));
        Some(idle.saturating_add(u64::try_from(age).unwrap_or(0)))
    }
}

fn sample_loop(weak: &Weak<Sample>, desktop: Desktop) {
    let mut backend = backend::Backend::new(desktop);
    while let Some(sample) = weak.upgrade() {
        let idle = backend.idle_ms();
        sample
            .at_ms
            .store(hydrus_core::TimestampMs::now().0, Ordering::Relaxed);
        sample
            .idle_ms
            .store(idle.unwrap_or(UNKNOWN), Ordering::Relaxed);
        drop(sample);
        std::thread::sleep(std::time::Duration::from_secs(1));
    }
}

#[cfg(all(unix, not(target_vendor = "apple"), not(target_os = "android")))]
mod backend {
    use super::Desktop;
    use x11rb::connection::Connection as _;
    use x11rb::protocol::screensaver::ConnectionExt as _;

    /// GNOME's idle monitor on the session bus: it answers on Wayland and
    /// X11 alike, and counts input to every window.
    struct Mutter(zbus::blocking::Connection);

    impl Mutter {
        fn connect(bus: Option<&str>) -> Option<Self> {
            let connection = match bus {
                Some(address) => zbus::blocking::connection::Builder::address(address)
                    .ok()?
                    .build()
                    .ok()?,
                None => zbus::blocking::Connection::session().ok()?,
            };
            let this = Self(connection);
            this.idle_ms()?;
            Some(this)
        }

        fn idle_ms(&self) -> Option<u64> {
            let reply = self
                .0
                .call_method(
                    Some("org.gnome.Mutter.IdleMonitor"),
                    "/org/gnome/Mutter/IdleMonitor/Core",
                    Some("org.gnome.Mutter.IdleMonitor"),
                    "GetIdletime",
                    &(),
                )
                .ok()?;
            reply.body().deserialize::<u64>().ok()
        }
    }

    /// The X server's screensaver extension (`XScreenSaverQueryInfo`).
    struct X11 {
        connection: x11rb::rust_connection::RustConnection,
        root: u32,
    }

    impl X11 {
        fn connect(display: Option<&str>) -> Option<Self> {
            let (connection, screen) = x11rb::connect(display).ok()?;
            let root = connection.setup().roots.get(screen)?.root;
            let this = Self { connection, root };
            this.idle_ms()?;
            Some(this)
        }

        fn idle_ms(&self) -> Option<u64> {
            let reply = self.connection.screensaver_query_info(self.root).ok()?;
            Some(u64::from(reply.reply().ok()?.ms_since_user_input))
        }
    }

    /// Whichever source the desktop has, found at the first sample that
    /// works and asked again if it stops answering.
    pub struct Backend {
        desktop: Desktop,
        mutter: Option<Mutter>,
        x11: Option<X11>,
        tried_at: Option<std::time::Instant>,
    }

    impl Backend {
        pub fn new(desktop: Desktop) -> Self {
            Self {
                desktop,
                mutter: None,
                x11: None,
                tried_at: None,
            }
        }

        pub fn idle_ms(&mut self) -> Option<u64> {
            if let Some(idle) = self.mutter.as_ref().and_then(Mutter::idle_ms) {
                return Some(idle);
            }
            if let Some(idle) = self.x11.as_ref().and_then(X11::idle_ms) {
                return Some(idle);
            }
            self.mutter = None;
            self.x11 = None;
            // (looking again costs a connection: not more than every 30s)
            if self
                .tried_at
                .is_some_and(|at| at.elapsed() < std::time::Duration::from_secs(30))
            {
                return None;
            }
            self.tried_at = Some(std::time::Instant::now());
            self.mutter = Mutter::connect(self.desktop.bus.as_deref());
            if let Some(idle) = self.mutter.as_ref().and_then(Mutter::idle_ms) {
                return Some(idle);
            }
            // (under Wayland, the X server of XWayland sees only input to
            // X clients, which says nothing of the rest of the desktop)
            if !self.desktop.wayland {
                self.x11 = X11::connect(self.desktop.display.as_deref());
            }
            self.x11.as_ref().and_then(X11::idle_ms)
        }
    }
}

#[cfg(not(all(unix, not(target_vendor = "apple"), not(target_os = "android"))))]
mod backend {
    use super::Desktop;

    pub struct Backend;

    impl Backend {
        pub fn new(_: Desktop) -> Self {
            Self
        }

        pub fn idle_ms(&mut self) -> Option<u64> {
            None
        }
    }
}
