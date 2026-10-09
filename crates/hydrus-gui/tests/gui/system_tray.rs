//! The system tray, through the real main window and Options window. The
//! headless platform has no tray host, so a recording one stands in for
//! Slint's (which the last tests drive on its own); what is asserted is what
//! the client asked the tray to show, and what it did with its window.
//! Each rule is the reference's (`oracle/fixtures/system_tray.json`, from
//! `oracle/record_system_tray.py`), checked in
//! `hydrus-gui-model/tests/model/system_tray.rs` case by case.

use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};
use std::time::{Duration, Instant};

use hydrus_gui::system_tray::{Controller, Host, OptionEntry, SlintHost, View};
use hydrus_gui::{Bound, MainWindow, OptionsWindow, Pages, SearchPage, bind, headless};
use hydrus_store::Store;
use hydrus_store::settings::{self, AdvancedMode, GuiSettings};
use slint::{ComponentHandle as _, Model as _};

#[derive(Debug, Clone, PartialEq)]
enum Asked {
    Show(View),
    Hide,
}

/// A tray that records what it was asked.
#[derive(Default)]
struct Recorder {
    available: Cell<bool>,
    /// The icon cannot be made.
    broken: Cell<bool>,
    asked: RefCell<Vec<Asked>>,
    /// The main window, to see whether it was still shown as the icon came.
    window: RefCell<Option<slint::Weak<MainWindow>>>,
    window_shown_at_show: RefCell<Vec<bool>>,
}

impl Recorder {
    fn available() -> Rc<Self> {
        let host = Self::default();
        host.available.set(true);
        Rc::new(host)
    }

    fn last(&self) -> Option<Asked> {
        self.asked.borrow().last().cloned()
    }

    fn shown(&self) -> bool {
        matches!(self.last(), Some(Asked::Show(_)))
    }

    fn view(&self) -> View {
        match self.last() {
            Some(Asked::Show(view)) => view,
            other => panic!("the icon is not shown: {other:?}"),
        }
    }
}

impl Host for Recorder {
    fn available(&self) -> bool {
        self.available.get()
    }

    fn show(&self, view: &View) -> bool {
        let shown = self
            .window
            .borrow()
            .as_ref()
            .and_then(slint::Weak::upgrade)
            .is_some_and(|window| window.window().is_visible());
        self.window_shown_at_show.borrow_mut().push(shown);
        if self.broken.get() {
            return false;
        }
        self.asked.borrow_mut().push(Asked::Show(view.clone()));
        true
    }

    fn hide(&self) {
        self.asked.borrow_mut().push(Asked::Hide);
    }
}

struct Client {
    _dir: tempfile::TempDir,
    store: std::sync::Arc<Store>,
    ui: MainWindow,
    bound: Bound,
    host: Rc<Recorder>,
}

fn client(available: bool) -> Client {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    // exits ask nothing, so a close that exits is seen at once
    store
        .write(|ctx| {
            let mut gui: GuiSettings = settings::get(ctx.conn())?;
            gui.confirm_exit = true;
            settings::set(ctx.conn(), &gui)
        })
        .unwrap();
    open(dir, store, available)
}

fn open(dir: tempfile::TempDir, store: std::sync::Arc<Store>, available: bool) -> Client {
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(&ui, Pages::single(SearchPage::new(store.clone())));
    let host = Recorder::available();
    host.available.set(available);
    *host.window.borrow_mut() = Some(ui.as_weak());
    bound.tray.set_host(host.clone());
    // the window is in front
    bound.tray.set_active(Rc::new(|| true));
    Client {
        _dir: dir,
        store,
        ui,
        bound,
        host,
    }
}

fn pump(millis: u64) {
    let end = Instant::now() + Duration::from_millis(millis);
    while Instant::now() < end {
        slint::platform::update_timers_and_animations();
        std::thread::sleep(Duration::from_millis(8));
    }
}

/// Options > system tray, the switches ticked as given (by label), applied.
fn set_options(client: &Client, ticks: &[(&str, bool)]) {
    client.ui.invoke_menu_title_pressed(0, 20., 22.);
    let menu = client.ui.get_menu_panes().row_data(0).unwrap();
    let at = menu
        .lines
        .iter()
        .position(|line| line.label == "options…")
        .unwrap();
    client
        .ui
        .invoke_menu_line_clicked(0, i32::try_from(at).unwrap(), 0., 0., 0.);
    let options: OptionsWindow = client
        .bound
        .options
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    let page = options
        .get_pages()
        .iter()
        .position(|page| page.text == "system tray")
        .unwrap();
    options.invoke_page_chosen(i32::try_from(page).unwrap());
    for (label, on) in ticks {
        let row = options
            .get_rows()
            .iter()
            .position(|row| row.label == *label)
            .unwrap_or_else(|| panic!("{label:?}"));
        options.invoke_check_toggled(i32::try_from(row).unwrap(), *on);
    }
    options.invoke_apply();
}

const ALWAYS: &str = "Always show the hydrus system tray icon: ";
const MINIMISE: &str = "Minimise the main window to system tray: ";
const CLOSE: &str = "Close the main window to system tray: ";
const START: &str = "Start the client minimised to system tray: ";

fn request_close(ui: &MainWindow) {
    ui.window()
        .dispatch_event(slint::platform::WindowEvent::CloseRequested);
}

/// The File menu's lines, opened fresh.
fn file_menu(ui: &MainWindow) -> Vec<String> {
    ui.invoke_menu_title_pressed(0, 20., 22.);
    let menu = ui.get_menu_panes().row_data(0).unwrap();
    let lines = menu
        .lines
        .iter()
        .map(|line| {
            if line.kind == 2 {
                "---".to_owned()
            } else {
                line.label.to_string()
            }
        })
        .collect();
    ui.invoke_menu_dismissed();
    lines
}

fn choose_in_file_menu(ui: &MainWindow, label: &str) {
    ui.invoke_menu_title_pressed(0, 20., 22.);
    let menu = ui.get_menu_panes().row_data(0).unwrap();
    let at = menu
        .lines
        .iter()
        .position(|line| line.label == label)
        .unwrap_or_else(|| panic!("{label}"));
    ui.invoke_menu_line_clicked(0, i32::try_from(at).unwrap(), 0., 0., 0.);
}

// leaf: audit-options-system-tray-always-show-the-hydrus-system-tray-icon
#[test]
fn the_icon_is_made_and_taken_away_as_always_show_is_applied_where_there_is_a_tray() {
    let _windows = headless::init();
    let client = client(true);
    assert!(
        client.host.asked.borrow().is_empty(),
        "no icon to begin with"
    );
    set_options(&client, &[(ALWAYS, true)]);
    assert!(client.host.shown());
    let view = client.host.view();
    assert!(view.ui_shown, "it says the window is shown");
    assert_eq!(view.tooltip, "hydrus client");
    assert!(view.options.always_show);
    // the tooltip follows what is paused, and the name
    client.bound.tray.flip_pause(true);
    client.bound.tray.flip_pause(false);
    assert_eq!(
        client.host.view().tooltip,
        "hydrus client - network traffic paused - subscriptions paused"
    );
    client.bound.tray.flip_pause(true);
    assert_eq!(
        client.host.view().tooltip,
        "hydrus client - subscriptions paused"
    );
    // taken away when it is no longer always shown
    set_options(&client, &[(ALWAYS, false)]);
    assert_eq!(client.host.last(), Some(Asked::Hide));
    assert!(!client.bound.tray.has_icon());
    // and kept, on reopening (the setting is read by whatever looks)
    let Client {
        _dir: dir,
        store,
        ui,
        bound,
        ..
    } = client;
    drop((ui, bound));
    set_without_window(&store, ALWAYS, true);
    let again = open(dir, store, true);
    again.bound.tray.start(&again.ui).unwrap();
    assert!(again.host.shown(), "a client opened with it on shows it");
}

fn set_without_window(store: &Store, label: &str, on: bool) {
    let name = match label {
        ALWAYS => "always_show_system_tray_icon",
        MINIMISE => "minimise_client_to_system_tray",
        CLOSE => "close_client_to_system_tray",
        _ => "start_client_in_system_tray",
    };
    store
        .write(move |ctx| {
            let mut options: hydrus_store::reference_options::ReferenceOptions =
                settings::get(ctx.conn())?;
            options.set_boolean(name, on);
            settings::set(ctx.conn(), &options)
        })
        .unwrap();
}

#[test]
fn there_is_no_icon_where_there_is_no_tray() {
    let _windows = headless::init();
    let client = client(false);
    set_options(&client, &[(ALWAYS, true), (CLOSE, true)]);
    assert!(client.host.asked.borrow().is_empty());
    assert!(!client.bound.tray.has_icon());
    // closing is not turned into hiding
    request_close(&client.ui);
    assert!(!client.bound.tray.hidden());
    assert!(client.ui.window().is_visible());
    assert!(
        client
            .ui
            .get_question()
            .starts_with("Are you sure you want to exit")
    );
}

// leaf: audit-options-system-tray-close-the-main-window-to-system-tray
#[test]
fn closing_hides_the_window_to_the_icon_instead_of_exiting() {
    let _windows = headless::init();
    let client = client(true);
    // by default, the close button asks to exit
    request_close(&client.ui);
    let question = client.ui.get_question();
    assert!(question.starts_with("Are you sure you want to exit the client?"));
    client.ui.invoke_answer(false);
    assert!(client.ui.window().is_visible());

    set_options(&client, &[(CLOSE, true)]);
    assert!(
        !client.host.shown(),
        "the option alone does not show the icon"
    );
    request_close(&client.ui);
    pump(500);
    assert!(!client.ui.window().is_visible(), "the window is hidden");
    assert!(
        client.ui.get_question().is_empty(),
        "and nothing asks to exit"
    );
    assert!(client.bound.tray.hidden());
    let view = client.host.view();
    assert!(!view.ui_shown, "the icon's first entry now says show");
    assert!(view.options.close);

    // the icon's show entry brings it back, still running
    client.bound.tray.flip_show_hide();
    assert!(client.ui.window().is_visible());
    assert_eq!(client.host.last(), Some(Asked::Hide), "no longer needed");

    // File > exit is not the close button: it still exits
    choose_in_file_menu(&client.ui, "exit");
    assert!(client.ui.window().is_visible());
    assert!(
        client
            .ui
            .get_question()
            .starts_with("Are you sure you want to exit")
    );
    assert!(!client.bound.tray.hidden());
    client.ui.invoke_answer(false);

    // the icon's exit entry asks to exit too, hidden or not
    request_close(&client.ui);
    pump(500);
    assert!(client.bound.tray.hidden());
    client.bound.tray.exit();
    assert!(client.ui.window().is_visible());
    assert!(
        client
            .ui
            .get_question()
            .starts_with("Are you sure you want to exit")
    );
    client.ui.invoke_answer(false);
}

// leaf: audit-options-system-tray-start-the-client-minimised-to-system-tray
#[test]
fn a_client_starts_hidden_in_the_icon_when_told_to_and_a_tray_is_there() {
    let _windows = headless::init();
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    // set in the Options window, as a user does, and the client restarted
    let first = open(dir, store.clone(), true);
    set_options(&first, &[(START, true)]);
    let Client {
        _dir: dir,
        ui,
        bound,
        ..
    } = first;
    drop((ui, bound));

    // a tray: the window is not shown, and the icon is
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::single(SearchPage::new(store.clone())));
    let host = Recorder::available();
    bound.tray.set_host(host.clone());
    bound.tray.start(&ui).unwrap();
    assert!(!ui.window().is_visible());
    assert!(bound.tray.hidden());
    assert!(!host.view().ui_shown);
    // the icon brings it up
    bound.tray.clicked();
    assert!(ui.window().is_visible());
    assert!(!bound.tray.hidden());
    drop((ui, bound));

    // no tray: shown as ever
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::single(SearchPage::new(store.clone())));
    let host = Recorder::available();
    host.available.set(false);
    bound.tray.set_host(host.clone());
    bound.tray.start(&ui).unwrap();
    assert!(ui.window().is_visible());
    assert!(host.asked.borrow().is_empty());
    drop((ui, bound, dir));

    // option off: shown
    set_without_window(&store, START, false);
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::single(SearchPage::new(store)));
    bound.tray.set_host(Recorder::available());
    bound.tray.start(&ui).unwrap();
    assert!(ui.window().is_visible());
}

// (this drives Slint's own minimised state in the headless platform, not
// winit's `is_minimized`, which Wayland does not answer: DIFFERENCES.md)
// leaf: audit-options-system-tray-minimise-the-main-window-to-system-tray
#[test]
fn minimising_hides_the_window_to_the_icon_where_the_window_system_reports_it() {
    let _windows = headless::init();
    let client = client(true);
    // without the option, a minimised window stays on the taskbar
    client.ui.window().set_minimized(true);
    pump(600);
    assert!(client.ui.window().is_minimized());
    assert!(!client.bound.tray.hidden());
    client.ui.window().set_minimized(false);

    set_options(&client, &[(MINIMISE, true)]);
    assert!(
        !client.host.shown(),
        "the option alone does not show the icon"
    );
    client.ui.window().set_maximized(true);
    client.ui.window().set_minimized(true);
    pump(600);
    assert!(client.bound.tray.hidden(), "minimising hid it");
    pump(500);
    assert!(!client.ui.window().is_visible());
    assert!(!client.host.view().ui_shown);
    // showing it again restores what it was before the minimise
    client.bound.tray.flip_show_hide();
    assert!(client.ui.window().is_visible());
    assert!(!client.ui.window().is_minimized());
    assert!(client.ui.window().is_maximized());
    pump(600);
    assert!(!client.bound.tray.hidden(), "and it is not hidden again");

    // and with no tray, nothing is hidden
    client.host.available.set(false);
    client.ui.window().set_minimized(true);
    pump(600);
    assert!(!client.bound.tray.hidden());
    assert!(client.ui.window().is_minimized());
}

#[test]
fn a_click_on_the_icon_does_what_the_reference_does() {
    let _windows = headless::init();
    let client = client(true);
    set_options(&client, &[(ALWAYS, true)]);
    // active, no minimise-to-tray: it goes to the taskbar
    client.bound.tray.clicked();
    assert!(client.ui.window().is_minimized());
    client.ui.window().set_minimized(false);
    // active, with it: it hides to the icon
    set_options(&client, &[(MINIMISE, true)]);
    client.bound.tray.clicked();
    assert!(client.bound.tray.hidden());
    // hidden: it shows
    client.bound.tray.clicked();
    assert!(!client.bound.tray.hidden());
    assert!(client.ui.window().is_visible());
    // not in front: it only comes forward
    client.bound.tray.set_active(Rc::new(|| false));
    client.bound.tray.clicked();
    assert!(!client.bound.tray.hidden());
    assert!(client.ui.window().is_visible());
    assert!(!client.ui.window().is_minimized());
}

// leaf: audit-options-file-tray
#[test]
fn file_minimise_to_system_tray_is_offered_with_a_tray_in_advanced_mode_and_hides_the_window() {
    const ENTRY: &str = "minimise to system tray";
    let _windows = headless::init();
    let client = client(true);
    // linux, simple mode: not offered
    assert!(!file_menu(&client.ui).contains(&ENTRY.to_owned()));
    client
        .store
        .write(|ctx| settings::set(ctx.conn(), &AdvancedMode(true)))
        .unwrap();
    let lines = file_menu(&client.ui);
    let at = lines
        .iter()
        .position(|line| line == ENTRY)
        .expect("offered");
    // after options, between separators, before restart
    assert_eq!(lines[at - 1], "---");
    assert_eq!(lines[at - 2], "options…");
    assert_eq!(lines[at + 1], "---");
    assert_eq!(lines[at + 2], "restart");
    // with no tray: not offered
    client.host.available.set(false);
    assert!(!file_menu(&client.ui).contains(&ENTRY.to_owned()));
    client.host.available.set(true);

    choose_in_file_menu(&client.ui, ENTRY);
    assert!(client.bound.tray.hidden());
    pump(500);
    assert!(!client.ui.window().is_visible());
    assert!(!client.host.view().ui_shown, "the icon was made");
    // the icon's show entry brings it back
    client.bound.tray.flip_show_hide();
    assert!(client.ui.window().is_visible());
}

#[test]
fn the_icons_option_entries_flip_the_options() {
    let _windows = headless::init();
    let client = client(true);
    set_options(&client, &[(ALWAYS, true)]);
    for (entry, option) in [
        (OptionEntry::Minimise, "minimise_client_to_system_tray"),
        (OptionEntry::Close, "close_client_to_system_tray"),
        (OptionEntry::Start, "start_client_in_system_tray"),
    ] {
        client.bound.tray.flip_option(entry);
        let options: hydrus_store::reference_options::ReferenceOptions =
            client.store.read(settings::get).unwrap();
        assert!(options.boolean(option), "{option}");
        client.bound.tray.flip_option(entry);
        let options: hydrus_store::reference_options::ReferenceOptions =
            client.store.read(settings::get).unwrap();
        assert!(!options.boolean(option), "{option}");
    }
    let view = client.host.view();
    assert!(!view.options.minimise && !view.options.close && !view.options.start);
}

struct Both(Rc<SlintHost>, Rc<Recorder>);
impl Host for Both {
    fn available(&self) -> bool {
        self.1.available()
    }
    fn show(&self, view: &View) -> bool {
        self.0.show(view)
    }
    fn hide(&self) {
        self.0.hide();
    }
    fn connect(&self, controller: Weak<Controller>) {
        self.0.connect(controller);
    }
}

// Slint's own tray: its component, its menu's state and its events.
// leaf: audit-options-system-tray-always-show-the-hydrus-system-tray-icon
#[test]
fn slint_s_tray_shows_the_view_and_sends_its_events_to_the_controller() {
    let _windows = headless::init();
    let client = client(true);
    let host = Rc::new(SlintHost::default());
    // (headless has no tray host; availability is the recorder's)
    client
        .bound
        .tray
        .set_host(Rc::new(Both(host.clone(), client.host.clone())));
    set_options(&client, &[(ALWAYS, true)]);
    let tray = host.component().expect("the component was made");
    assert!(tray.get_icon_shown());
    assert!(tray.get_ui_shown());
    assert_eq!(tray.get_tooltip_text(), "hydrus client");
    assert!(!tray.get_network_paused());

    // its menu entries reach the client
    tray.invoke_pause_network();
    assert!(tray.get_network_paused());
    assert_eq!(
        tray.get_tooltip_text(),
        "hydrus client - network traffic paused"
    );
    tray.invoke_flip_option("close".into());
    assert!(tray.get_close_to_tray());
    tray.invoke_flip_show_hide();
    assert!(client.bound.tray.hidden());
    assert!(!tray.get_ui_shown());
    tray.invoke_activated();
    assert!(!client.bound.tray.hidden(), "a click shows it");
    tray.invoke_exit_client();
    assert!(
        client
            .ui
            .get_question()
            .starts_with("Are you sure you want to exit")
    );
    client.ui.invoke_answer(false);

    // not always shown any more: gone
    set_options(&client, &[(ALWAYS, false)]);
    assert!(!tray.get_icon_shown());
}

fn titles(entries: &serde_json::Value, out: &mut Vec<String>) {
    for entry in entries.as_array().unwrap() {
        if let Some(text) = entry["text"].as_str() {
            out.push(text.to_owned());
        }
        if entry.get("submenu").is_some() {
            titles(&entry["submenu"], out);
        }
    }
}

/// The menu's words are the reference's (`menu` in the recording).
#[test]
fn slint_s_tray_menu_has_the_reference_s_entries() {
    let recorded = hydrus_testkit::fixture_json("system_tray.json");
    let source = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("ui/system_tray.slint"),
    )
    .unwrap();
    let mut wanted = Vec::new();
    titles(&recorded["menu"]["shown"], &mut wanted);
    titles(&recorded["menu"]["hidden"], &mut wanted);
    assert_eq!(wanted.len(), 16);
    for title in wanted {
        assert!(source.contains(&format!("\"{title}\"")), "{title}");
    }
}

#[test]
fn the_icon_is_up_before_the_window_goes_so_the_event_loop_is_never_left_empty() {
    let _windows = headless::init();
    let client = client(true);
    set_options(&client, &[(CLOSE, true)]);
    request_close(&client.ui);
    // as the icon was made the window was still shown (a tray icon and a
    // window each keep Slint's event loop alive; hiding the last of them
    // ends it, and the icon takes hold a turn after it is made)
    assert_eq!(*client.host.window_shown_at_show.borrow(), [true]);
    assert!(client.ui.window().is_visible(), "not yet");
    assert!(client.bound.tray.hidden());
    pump(500);
    assert!(!client.ui.window().is_visible(), "and then it is");

    // shown again before the window went: it simply stays
    client.bound.tray.flip_show_hide();
    request_close(&client.ui);
    client.bound.tray.flip_show_hide();
    pump(500);
    assert!(client.ui.window().is_visible());
    assert!(!client.bound.tray.hidden());
}

#[test]
fn a_window_is_not_hidden_when_there_is_no_icon_to_hide_it_to() {
    let _windows = headless::init();
    let client = client(true);
    set_options(&client, &[(CLOSE, true)]);
    client.host.broken.set(true);
    request_close(&client.ui);
    pump(500);
    assert!(client.ui.window().is_visible());
    assert!(!client.bound.tray.hidden());
    assert!(!client.bound.tray.has_icon());
}

#[test]
fn a_window_hidden_to_a_tray_that_goes_away_comes_back() {
    let _windows = headless::init();
    let client = client(true);
    set_options(&client, &[(CLOSE, true)]);
    request_close(&client.ui);
    pump(500);
    assert!(client.bound.tray.hidden());
    client.host.available.set(false);
    pump(600);
    assert!(!client.bound.tray.hidden());
    assert!(client.ui.window().is_visible());
}

#[test]
fn an_exit_that_was_vetoed_does_not_make_the_next_close_button_exit() {
    let _windows = headless::init();
    let client = client(true);
    set_options(&client, &[(CLOSE, true)]);
    // File > exit while the window cannot take it (hidden): vetoed
    client.ui.hide().unwrap();
    hydrus_gui::client_exit::set_mode(hydrus_gui_model::shutdown_work::ExitMode::Exit);
    request_close(&client.ui);
    client.ui.show().unwrap();
    assert!(client.ui.get_question().is_empty());
    // the close button still hides to the tray, as the option says
    request_close(&client.ui);
    assert!(client.ui.get_question().is_empty(), "nothing asked to exit");
    assert!(client.bound.tray.hidden());
}
