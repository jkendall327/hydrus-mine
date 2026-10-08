//! The system tray icon: when it exists, and what closing, minimising and
//! clicking do with the main window, as `ClientGUI` and
//! `ClientSystemTrayIcon` do (`oracle/fixtures/system_tray.json`).
//!
//! The decisions are `hydrus_gui_model::system_tray`'s. The [`Controller`]
//! applies them to the main window and tells a [`Host`] what to show; the
//! real host is Slint's own `SystemTrayIcon` (a StatusNotifierItem on
//! Linux), and tests put a recording one in its place.

use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};
use std::sync::Arc;
use std::time::Duration;

use slint::ComponentHandle as _;

use hydrus_gui_model::system_tray::{self as rules, Click, Close, Options};
use hydrus_store::Store;
use hydrus_store::reference_options::ReferenceOptions;
use hydrus_store::settings::{self, GuiSettings, Pauses};

use crate::{HydrusTray, MainWindow};

/// What the icon and its menu show (`RegenOptionsCheckboxes`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct View {
    /// Whether the main window is shown (the first entry says which it does).
    pub ui_shown: bool,
    pub tooltip: String,
    pub network_paused: bool,
    pub subscriptions_paused: bool,
    pub options: Options,
}

/// Where the icon is shown.
pub trait Host {
    /// Whether the desktop has a tray to show it in
    /// (`QSystemTrayIcon.isSystemTrayAvailable`).
    fn available(&self) -> bool;
    /// Make the icon exist, showing this.
    fn show(&self, view: &View);
    /// Take the icon away.
    fn hide(&self);
    /// Told where its events go; the controller calls this once.
    fn connect(&self, _controller: Weak<Controller>) {}
}

/// The entries of the icon's menu that change an option.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OptionEntry {
    Minimise,
    Close,
    Start,
}

impl OptionEntry {
    const fn name(self) -> &'static str {
        match self {
            Self::Minimise => rules::MINIMISE,
            Self::Close => rules::CLOSE,
            Self::Start => rules::START,
        }
    }
}

/// Applies the tray's rules to the main window.
pub struct Controller {
    store: Arc<Store>,
    window: slint::Weak<MainWindow>,
    host: RefCell<Rc<dyn Host>>,
    /// Whether the main window is in front (the reference's `isActiveWindow`).
    active: RefCell<Rc<dyn Fn() -> bool>>,
    /// The client is hidden to the tray (`_currently_hidden_to_system_tray`).
    hidden: Cell<bool>,
    /// The host has been asked to show the icon (`_have_system_tray_icon`).
    icon: Cell<bool>,
    /// The window's state before it was hidden, to restore it on show.
    maximised: Cell<Option<bool>>,
    /// The window was minimised at the last look.
    minimised: Cell<bool>,
    me: Weak<Self>,
    watch: slint::Timer,
}

impl std::fmt::Debug for Controller {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Controller")
            .field("hidden", &self.hidden.get())
            .field("icon", &self.icon.get())
            .finish_non_exhaustive()
    }
}

/// How often the main window is looked at for minimising (winit gives no
/// event for it).
const WATCH: Duration = Duration::from_millis(250);

impl Controller {
    /// A controller using Slint's tray.
    pub fn new(store: Arc<Store>, window: &MainWindow) -> Rc<Self> {
        Self::with_host(
            store,
            window,
            &(Rc::new(SlintHost::default()) as Rc<dyn Host>),
        )
    }

    pub fn with_host(store: Arc<Store>, window: &MainWindow, host: &Rc<dyn Host>) -> Rc<Self> {
        let controller = Rc::new_cyclic(|me| Self {
            store,
            window: window.as_weak(),
            host: RefCell::new(host.clone()),
            active: RefCell::new(Rc::new({
                let weak = window.as_weak();
                move || weak.upgrade().is_some_and(|window| focused(&window))
            })),
            hidden: Cell::new(false),
            icon: Cell::new(false),
            maximised: Cell::new(None),
            minimised: Cell::new(false),
            me: me.clone(),
            watch: slint::Timer::default(),
        });
        host.connect(Rc::downgrade(&controller));
        // a window close request first asks whether to hide to the tray
        let me = Rc::downgrade(&controller);
        crate::client_exit::intercept_close(Rc::new(move || {
            me.upgrade().is_some_and(|controller| controller.close())
        }));
        controller
    }

    /// Show the icon's events somewhere else (tests).
    pub fn set_host(&self, host: Rc<dyn Host>) {
        host.connect(self.me.clone());
        if self.icon.replace(false) {
            self.host.borrow().hide();
        }
        *self.host.borrow_mut() = host;
        self.update_icon();
    }

    /// Tell it whether the window is in front (tests; the default asks the
    /// window system).
    pub fn set_active(&self, active: Rc<dyn Fn() -> bool>) {
        *self.active.borrow_mut() = active;
    }

    pub fn available(&self) -> bool {
        self.host.borrow().available()
    }

    /// The client is hidden to the tray.
    pub fn hidden(&self) -> bool {
        self.hidden.get()
    }

    /// The icon is being shown.
    pub fn has_icon(&self) -> bool {
        self.icon.get()
    }

    fn options(&self) -> Options {
        Options::read(&self.reference_options())
    }

    fn reference_options(&self) -> ReferenceOptions {
        self.store
            .read(settings::get::<ReferenceOptions>)
            .unwrap_or_default()
    }

    fn view(&self) -> View {
        let pauses: Pauses = self.store.read(settings::get).unwrap_or_default();
        let name = self
            .store
            .read(settings::get::<GuiSettings>)
            .unwrap_or_default()
            .application_display_name;
        View {
            ui_shown: !self.hidden.get(),
            tooltip: rules::tooltip(&name, pauses.network_traffic, pauses.subscriptions),
            network_paused: pauses.network_traffic,
            subscriptions_paused: pauses.subscriptions,
            options: self.options(),
        }
    }

    /// `_UpdateSystemTrayIcon`: the icon exists while it is always shown or
    /// the client is hidden to it. Also the way to show it new settings.
    pub fn update_icon(&self) {
        let options = self.options();
        let available = self.available();
        let need = rules::needs_icon(available, options.always_show, self.hidden.get());
        if need {
            self.host.borrow().show(&self.view());
        } else if self.icon.get() {
            self.host.borrow().hide();
        }
        self.icon.set(need);
        // looking for a minimise is only worth the wake-ups when it would hide
        if rules::minimise_hides(available, options.minimise) {
            let me = self.me.clone();
            self.watch
                .start(slint::TimerMode::Repeated, WATCH, move || {
                    if let Some(controller) = me.upgrade() {
                        controller.look();
                    }
                });
        } else {
            self.watch.stop();
            self.minimised.set(false);
        }
    }

    /// The options or the pauses changed (Options applied, the Network menu):
    /// the icon follows.
    pub fn refresh(&self) {
        self.update_icon();
    }

    /// Start the client: showing the window, or leaving it hidden in the
    /// tray (the option, with a tray to hide in).
    pub fn start(&self, window: &MainWindow) -> Result<(), slint::PlatformError> {
        if rules::starts_hidden(self.available(), self.options().start) {
            self.hidden.set(true);
            self.update_icon();
            Ok(())
        } else {
            self.update_icon();
            window.show()
        }
    }

    /// `_SystemTrayHide`.
    pub fn hide_to_tray(&self) {
        if !self.available() {
            eprintln!("Was called to hide to system tray, but system tray is not available!");
            return;
        }
        if self.hidden.get() {
            return;
        }
        let Some(window) = self.window.upgrade() else {
            return;
        };
        // (restored from a minimise, so that showing it brings it back)
        if window.window().is_minimized() {
            window.window().set_minimized(false);
        }
        self.maximised.set(Some(window.window().is_maximized()));
        let _ = window.hide();
        self.hidden.set(true);
        self.minimised.set(false);
        self.update_icon();
    }

    /// `_SystemTrayShow`.
    pub fn show_from_tray(&self) {
        if !self.hidden.get() {
            return;
        }
        if let Some(window) = self.window.upgrade() {
            let _ = window.show();
            if let Some(maximised) = self.maximised.take() {
                window.window().set_maximized(maximised);
            }
        }
        self.hidden.set(false);
        self.update_icon();
    }

    /// The icon's show/hide entry (`_SystemTrayFlipShowHide`).
    pub fn flip_show_hide(&self) {
        if self.hidden.get() {
            self.show_from_tray();
        } else {
            self.hide_to_tray();
        }
    }

    /// A click on the icon (`_SystemTrayActivation`).
    pub fn clicked(&self) {
        let active = (self.active.borrow())();
        match rules::click(self.hidden.get(), active, self.options().minimise) {
            Click::ShowAndRaise => {
                self.show_from_tray();
                self.raise();
            }
            Click::HideToTray => self.hide_to_tray(),
            Click::Minimise => {
                if let Some(window) = self.window.upgrade() {
                    window.window().set_minimized(true);
                }
            }
            Click::Raise => self.raise(),
        }
    }

    fn raise(&self) {
        if let Some(window) = self.window.upgrade() {
            if window.window().is_minimized() {
                window.window().set_minimized(false);
            }
            crate::main_identity::activate_if_inactive(&window);
        }
    }

    /// The window's close request (`closeEvent`): hides to the tray when it
    /// is told to, and says so; else the client asks to exit.
    pub fn close(&self) -> bool {
        if rules::close(self.available(), self.options().close) == Close::HideToTray {
            self.hide_to_tray();
            true
        } else {
            false
        }
    }

    /// The icon's exit entry.
    pub fn exit(&self) {
        // (the confirmation and the work of exiting need the window)
        self.show_from_tray();
        crate::client_exit::set_mode(hydrus_gui_model::shutdown_work::ExitMode::Exit);
        if let Some(window) = self.window.upgrade() {
            let _ = window
                .window()
                .dispatch_event_with_result(slint::platform::WindowEvent::CloseRequested);
        }
    }

    /// One of the icon's pause entries.
    pub fn flip_pause(&self, network: bool) {
        let done = self.store.write(move |ctx| {
            let conn = ctx.conn();
            let mut pauses: Pauses = settings::get(conn)?;
            let field = if network {
                &mut pauses.network_traffic
            } else {
                &mut pauses.subscriptions
            };
            *field = !*field;
            settings::set(conn, &pauses)
        });
        if let Err(error) = done {
            eprintln!("could not pause: {error}");
        }
        self.refresh();
    }

    /// One of the icon's option entries (`_FlipSimpleBool`).
    pub fn flip_option(&self, entry: OptionEntry) {
        let done = self.store.write(move |ctx| {
            let conn = ctx.conn();
            let mut options: ReferenceOptions = settings::get(conn)?;
            let on = options.boolean(entry.name());
            options.set_boolean(entry.name(), !on);
            settings::set(conn, &options)
        });
        if let Err(error) = done {
            eprintln!("could not save the option: {error}");
        }
        self.refresh();
    }

    /// The client is exiting: the icon goes with it (`SaveAndHide`).
    pub fn retire(&self) {
        self.watch.stop();
        if self.icon.replace(false) {
            self.host.borrow().hide();
        }
        self.hidden.set(false);
    }

    /// Look for the main window being minimised (`changeEvent`).
    fn look(&self) {
        let Some(window) = self.window.upgrade() else {
            return;
        };
        if self.hidden.get() || !window.window().is_visible() {
            self.minimised.set(false);
            return;
        }
        match crate::popup_freeze::minimized(window.window()) {
            Some(true) => {
                if !self.minimised.replace(true)
                    && rules::minimise_hides(self.available(), self.options().minimise)
                {
                    self.hide_to_tray();
                }
            }
            Some(false) => self.minimised.set(false),
            // (Wayland: no way to know)
            None => {}
        }
    }
}

fn focused(window: &MainWindow) -> bool {
    use slint::winit_030::WinitWindowAccessor as _;
    window
        .window()
        .with_winit_window(slint::winit_030::winit::window::Window::has_focus)
        .unwrap_or(false)
}

/// Whether a tray is there to show an icon in: on Linux, a
/// StatusNotifierWatcher on the session bus.
#[cfg(all(unix, not(target_vendor = "apple"), not(target_os = "android")))]
fn tray_available() -> bool {
    let check = || -> zbus::Result<bool> {
        let connection = zbus::blocking::Connection::session()?;
        let bus = zbus::blocking::fdo::DBusProxy::new(&connection)?;
        let watcher = zbus::names::BusName::try_from("org.kde.StatusNotifierWatcher")?;
        Ok(bus.name_has_owner(watcher)?)
    };
    check().unwrap_or(false)
}

#[cfg(not(all(unix, not(target_vendor = "apple"), not(target_os = "android"))))]
fn tray_available() -> bool {
    true
}

/// Slint's `SystemTrayIcon`.
#[derive(Default)]
pub struct SlintHost {
    tray: RefCell<Option<HydrusTray>>,
    controller: RefCell<Weak<Controller>>,
}

impl std::fmt::Debug for SlintHost {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SlintHost")
            .field("made", &self.tray.borrow().is_some())
            .finish_non_exhaustive()
    }
}

impl SlintHost {
    /// The component, once an icon has been shown.
    pub fn component(&self) -> Option<HydrusTray> {
        self.tray.borrow().as_ref().map(HydrusTray::clone_strong)
    }

    fn make(&self) -> Result<HydrusTray, slint::PlatformError> {
        let tray = HydrusTray::new()?;
        let controller = self.controller.borrow().clone();
        macro_rules! on {
            ($register:ident, |$controller:ident| $body:expr) => {{
                let me = controller.clone();
                tray.$register(move || {
                    if let Some($controller) = me.upgrade() {
                        $body;
                    }
                });
            }};
        }
        on!(on_activated, |c| c.clicked());
        on!(on_flip_show_hide, |c| c.flip_show_hide());
        on!(on_pause_network, |c| c.flip_pause(true));
        on!(on_pause_subscriptions, |c| c.flip_pause(false));
        on!(on_exit_client, |c| c.exit());
        let me = controller;
        tray.on_flip_option(move |name| {
            let Some(controller) = me.upgrade() else {
                return;
            };
            match name.as_str() {
                "minimise" => controller.flip_option(OptionEntry::Minimise),
                "close" => controller.flip_option(OptionEntry::Close),
                "start" => controller.flip_option(OptionEntry::Start),
                other => eprintln!("the tray has no option {other}"),
            }
        });
        Ok(tray)
    }
}

impl Host for SlintHost {
    fn available(&self) -> bool {
        tray_available()
    }

    fn show(&self, view: &View) {
        let mut slot = self.tray.borrow_mut();
        if slot.is_none() {
            match self.make() {
                Ok(tray) => *slot = Some(tray),
                Err(error) => {
                    eprintln!("could not make the system tray icon: {error}");
                    return;
                }
            }
        }
        let Some(tray) = slot.as_ref() else {
            return;
        };
        tray.set_ui_shown(view.ui_shown);
        tray.set_tooltip_text(view.tooltip.as_str().into());
        tray.set_network_paused(view.network_paused);
        tray.set_subscriptions_paused(view.subscriptions_paused);
        tray.set_minimise_to_tray(view.options.minimise);
        tray.set_close_to_tray(view.options.close);
        tray.set_start_in_tray(view.options.start);
        let _ = tray.show();
    }

    fn hide(&self) {
        // (kept, not dropped: this can be a menu entry of the icon itself)
        if let Some(tray) = self.tray.borrow().as_ref() {
            let _ = tray.hide();
        }
    }

    fn connect(&self, controller: Weak<Controller>) {
        *self.controller.borrow_mut() = controller;
    }
}
