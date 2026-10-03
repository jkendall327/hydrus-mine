//! The main window's menu bar in the client: its titles, menus opened from
//! them and moved through by the pointer and keys, and what its entries do
//! (tests/main_menu.rs has the menus themselves against the reference's).

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use slint::{ComponentHandle as _, Model as _, SharedString};

use hydrus_gui::{Clip, MainWindow, Pages, bind, headless};
use hydrus_store::Store;
use hydrus_store::import::import_legacy;

fn store() -> ([tempfile::TempDir; 2], Arc<Store>) {
    let legacy = hydrus_testkit::legacy_fixture("basic");
    let native = tempfile::tempdir().unwrap();
    import_legacy(
        legacy.path(),
        &native.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store = Store::open(native.path()).unwrap();
    ([legacy, native], store)
}

fn titles(ui: &MainWindow) -> Vec<(String, bool)> {
    let titles = ui.get_menu_titles();
    (0..titles.row_count())
        .map(|i| {
            let t = titles.row_data(i).unwrap();
            (t.label.to_string(), t.usable)
        })
        .collect()
}

/// The open menus' lines: (label, usable, ticked), separators as "---".
fn panes(ui: &MainWindow) -> Vec<Vec<(String, bool, bool)>> {
    let panes = ui.get_menu_panes();
    (0..panes.row_count())
        .map(|p| {
            let lines = panes.row_data(p).unwrap().lines;
            (0..lines.row_count())
                .map(|i| {
                    let line = lines.row_data(i).unwrap();
                    let label = if line.kind == 2 {
                        "---".to_owned()
                    } else {
                        line.label.to_string()
                    };
                    (label, line.usable, line.checked)
                })
                .collect()
        })
        .collect()
}

/// The line of the last menu open with this label.
fn line(ui: &MainWindow, label: &str) -> (i32, i32) {
    let panes = panes(ui);
    let p = panes.len() - 1;
    let i = panes[p]
        .iter()
        .position(|l| l.0 == label)
        .unwrap_or_else(|| panic!("{label} in {:?}", panes[p]));
    (p as i32, i as i32)
}

/// Point at a line (its submenu opens), or choose it.
fn hover(ui: &MainWindow, label: &str) {
    let (p, i) = line(ui, label);
    ui.invoke_menu_line_hovered(
        p,
        i,
        300.0 + 150.0 * p as f32,
        40.0 + 22.0 * i as f32,
        150.0 * p as f32,
    );
}

fn choose(ui: &MainWindow, label: &str) {
    let (p, i) = line(ui, label);
    ui.invoke_menu_line_clicked(p, i, 0.0, 0.0, 0.0);
}

fn tabs(ui: &MainWindow) -> Vec<String> {
    let row = ui.get_tab_rows().row_data(0).unwrap();
    (0..row.names.row_count())
        .map(|i| row.names.row_data(i).unwrap().to_string())
        .collect()
}

#[test]
fn the_bar_s_menus_open_and_do_what_they_say() {
    let (_dirs, store) = store();
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    let launched: Rc<RefCell<Vec<String>>> = Rc::default();
    hydrus_gui::set_launcher({
        let launched = launched.clone();
        move |target| launched.borrow_mut().push(target.to_owned())
    });
    let copied: Rc<RefCell<Vec<String>>> = Rc::default();
    hydrus_gui::set_clipper({
        let copied = copied.clone();
        move |clip| {
            if let Clip::Text(text) = clip {
                copied.borrow_mut().push(text.clone());
            }
        }
    });
    let title =
        |label: &str| -> i32 { titles(&ui).iter().position(|(t, _)| t == label).unwrap() as i32 };
    // the reference's titles; nothing to undo
    assert_eq!(
        titles(&ui),
        [
            ("file".to_owned(), true),
            ("undo".to_owned(), false),
            ("pages".to_owned(), true),
            ("database".to_owned(), true),
            ("network".to_owned(), true),
            ("services".to_owned(), true),
            ("tags".to_owned(), true),
            ("help".to_owned(), true),
        ]
    );
    ui.invoke_menu_title_pressed(title("undo"), 40.0, 22.0);
    assert!(panes(&ui).is_empty(), "a disabled menu doesn't open");

    // pages > download > new watcher page: a watcher page, shown
    let pages = title("pages");
    ui.invoke_menu_title_pressed(pages, 80.0, 22.0);
    assert_eq!(ui.get_menu_open(), pages);
    assert_eq!(panes(&ui)[0][0].0, "weight");
    hover(&ui, "download");
    assert_eq!(
        panes(&ui)[1],
        [
            ("new url download page".to_owned(), true, false),
            ("new watcher page".to_owned(), true, false),
            ("new gallery page".to_owned(), true, false),
            ("new simple downloader page".to_owned(), true, false),
        ]
    );
    let before = tabs(&ui);
    choose(&ui, "new watcher page");
    assert!(panes(&ui).is_empty());
    assert_eq!(ui.get_menu_open(), -1);
    assert_eq!(tabs(&ui).len(), before.len() + 1);
    assert_eq!(bound.pages.borrow().shown().name, "watcher");

    // the weight menu counts the pages; its label copies itself
    ui.invoke_menu_title_pressed(pages, 80.0, 22.0);
    hover(&ui, "weight");
    let count = format!("{} pages open", before.len() + 1);
    assert_eq!(panes(&ui)[1][0], (count.clone(), true, false));
    choose(&ui, &count);
    assert_eq!(*copied.borrow(), [count]);
    // the history: the latest first; choosing one shows it
    ui.invoke_menu_title_pressed(pages, 80.0, 22.0);
    hover(&ui, "history");
    let history: Vec<String> = panes(&ui)[1].iter().map(|l| l.0.clone()).collect();
    assert_eq!(history[0], "1: watcher");
    let first = before[0].split(" (").next().unwrap().to_owned();
    assert!(
        history[1].starts_with(&format!("2: {first}")),
        "{history:?}"
    );
    assert_eq!(&history[history.len() - 2..], ["---", "Clear History"]);
    let earlier = history[1].clone();
    choose(&ui, &earlier);
    assert_eq!(bound.pages.borrow().shown().name, first);

    // a page closed: undo offers it back
    ui.invoke_tab_chosen(0, i32::try_from(before.len()).unwrap());
    ui.invoke_close_page();
    assert_eq!(titles(&ui)[1], ("undo".to_owned(), true));
    ui.invoke_menu_title_pressed(title("undo"), 40.0, 22.0);
    assert_eq!(panes(&ui)[0], [("closed pages".to_owned(), true, false)]);
    hover(&ui, "closed pages");
    assert_eq!(
        panes(&ui)[1],
        [
            ("clear all\u{2026}".to_owned(), true, false),
            ("---".to_owned(), false, false),
            ("watcher".to_owned(), true, false),
        ]
    );
    choose(&ui, "watcher");
    assert_eq!(tabs(&ui).len(), before.len() + 1, "reopened");
    assert_eq!(bound.pages.borrow().shown().name, "watcher");
    // closed again, and the closed pages cleared, asking first
    ui.invoke_close_page();
    ui.invoke_menu_title_pressed(title("undo"), 40.0, 22.0);
    hover(&ui, "closed pages");
    choose(&ui, "clear all\u{2026}");
    assert_eq!(ui.get_question(), "Clear the 1 closed pages?");
    ui.invoke_answer(true);
    ui.invoke_menu_title_pressed(title("undo"), 40.0, 22.0);
    assert!(panes(&ui).is_empty(), "nothing left to undo");

    // network > pause: switches the store's pauses, and shows them ticked
    let network = title("network");
    ui.invoke_menu_title_pressed(network, 200.0, 22.0);
    hover(&ui, "pause");
    choose(&ui, "paged watcher checking");
    let pauses: hydrus_store::settings::Pauses = store.read(hydrus_store::settings::get).unwrap();
    assert!(pauses.watcher_checkers);
    ui.invoke_menu_title_pressed(network, 200.0, 22.0);
    hover(&ui, "pause");
    let (_, i) = line(&ui, "paged watcher checking");
    assert!(panes(&ui)[1][i as usize].2, "ticked");
    choose(&ui, "paged watcher checking");
    let pauses: hydrus_store::settings::Pauses = store.read(hydrus_store::settings::get).unwrap();
    assert!(!pauses.watcher_checkers);

    // help > links open in the browser; advanced mode adds entries
    ui.invoke_menu_title_pressed(title("help"), 300.0, 22.0);
    hover(&ui, "links");
    choose(&ui, "github repository");
    assert_eq!(
        *launched.borrow(),
        ["https://github.com/hydrusnetwork/hydrus"]
    );
    ui.invoke_menu_title_pressed(title("help"), 300.0, 22.0);
    choose(&ui, "advanced mode");
    let advanced: hydrus_store::settings::AdvancedMode =
        store.read(hydrus_store::settings::get).unwrap();
    assert!(advanced.0);
    ui.invoke_menu_title_pressed(network, 200.0, 22.0);
    hover(&ui, "pause");
    line(&ui, "nudge subscriptions awake");

    // the database directory, from file > open
    ui.invoke_menu_title_pressed(title("file"), 0.0, 22.0);
    hover(&ui, "open");
    choose(&ui, "database directory");
    assert_eq!(
        launched.borrow().last().unwrap(),
        &store.dir().to_string_lossy()
    );

    // a press elsewhere closes the menus; so does pressing the title again
    ui.invoke_menu_title_pressed(pages, 80.0, 22.0);
    ui.invoke_menu_dismissed();
    assert!(panes(&ui).is_empty());
    ui.invoke_menu_title_pressed(pages, 80.0, 22.0);
    ui.invoke_menu_title_pressed(pages, 80.0, 22.0);
    assert!(panes(&ui).is_empty());
    // with one open, pointing at another title opens that one
    ui.invoke_menu_title_pressed(pages, 80.0, 22.0);
    ui.invoke_menu_title_hovered(network, 200.0, 22.0);
    assert_eq!(ui.get_menu_open(), network);
    ui.invoke_menu_dismissed();
}

#[test]
fn keys_open_and_move_through_the_menus() {
    use slint::platform::Key;
    let (_dirs, store) = store();
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store).unwrap());
    let key = |k: Key| ui.invoke_menu_key(SharedString::from(k), false);
    // alt and a title's letter opens its menu, on its first entry
    assert!(!ui.invoke_menu_key("x".into(), true), "no menu's letter");
    assert!(!key(Key::DownArrow), "with no menu open, keys go on");
    assert!(ui.invoke_menu_key("p".into(), true));
    assert_eq!(ui.get_menu_open(), 2);
    let current = |ui: &MainWindow| {
        let panes = ui.get_menu_panes();
        panes.row_data(panes.row_count() - 1).unwrap().current
    };
    assert_eq!(current(&ui), 0);
    // down past separators; right into a submenu, on its first entry
    assert!(key(Key::DownArrow));
    assert_eq!(current(&ui), 2, "history, past the separator");
    let (_, special) = line(&ui, "special");
    while current(&ui) != special {
        key(Key::DownArrow);
    }
    key(Key::RightArrow);
    assert_eq!(panes(&ui).len(), 2);
    assert_eq!(current(&ui), 0);
    key(Key::DownArrow);
    // enter chooses: a duplicates page
    key(Key::Return);
    assert!(panes(&ui).is_empty());
    assert_eq!(bound.pages.borrow().shown().name, "duplicates");
    // escape closes the last menu opened; left from the bar's menu goes
    // to the one before (past the disabled undo menu)
    ui.invoke_menu_key("f".into(), true);
    assert_eq!(ui.get_menu_open(), 0);
    key(Key::LeftArrow);
    assert_eq!(ui.get_menu_open(), 7, "help");
    key(Key::RightArrow);
    assert_eq!(ui.get_menu_open(), 0, "and round to file");
    key(Key::Escape);
    assert!(panes(&ui).is_empty());
    assert_eq!(ui.get_menu_open(), -1);
}

/// The menus as drawn: a real pointer opens one from the bar, points into
/// a submenu and chooses from it.
#[test]
#[allow(clippy::float_cmp)] // (whole pixels)
fn the_pointer_opens_menus_and_chooses_from_them() {
    let (_dirs, store) = store();
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let _bound = bind(&ui, Pages::open(store.clone()).unwrap());
    let launched: Rc<RefCell<Vec<String>>> = Rc::default();
    hydrus_gui::set_launcher({
        let launched = launched.clone();
        move |target| launched.borrow_mut().push(target.to_owned())
    });
    ui.show().unwrap();
    let window = windows.get(0).unwrap();
    headless::render(&window, 1100, 700);
    let at = |x: f32, y: f32, press: bool| {
        use slint::platform::{PointerEventButton, WindowEvent};
        let position = slint::LogicalPosition::new(x, y);
        ui.window()
            .dispatch_event(WindowEvent::PointerMoved { position });
        if press {
            for event in [
                WindowEvent::PointerPressed {
                    position,
                    button: PointerEventButton::Left,
                },
                WindowEvent::PointerReleased {
                    position,
                    button: PointerEventButton::Left,
                },
            ] {
                ui.window().dispatch_event(event);
            }
        }
        headless::render(&window, 1100, 700);
    };
    // "file", the first title
    at(8.0, 11.0, true);
    assert_eq!(ui.get_menu_open(), 0);
    let menu = ui.get_menu_panes().row_data(0).unwrap();
    assert_eq!(menu.y, 22.0, "under the bar");
    // its fifth line, "open" (lines 22 high, separators 7, 2 around)
    let (_, open) = line(&ui, "open");
    assert_eq!(open, 4);
    let open_y = 22.0 + 2.0 + 22.0 + 7.0 + 22.0 + 7.0 + 11.0;
    at(30.0, open_y, false);
    assert_eq!(panes(&ui).len(), 2, "its submenu opened");
    let submenu = ui.get_menu_panes().row_data(1).unwrap();
    assert!(submenu.x > 60.0, "beside it: {}", submenu.x);
    assert_eq!(submenu.y, open_y - 11.0 - 2.0, "level with it");
    // along to it, and down to "database directory", its second line
    at(submenu.x + 20.0, open_y, false);
    let directory_y = submenu.y + 2.0 + 22.0 + 11.0;
    at(submenu.x + 20.0, directory_y, true);
    assert!(panes(&ui).is_empty());
    assert_eq!(
        *launched.borrow(),
        [store.dir().to_string_lossy().to_string()]
    );
    // a press away from the menus closes them
    at(8.0, 11.0, true);
    assert_eq!(ui.get_menu_open(), 0);
    at(600.0, 400.0, true);
    assert_eq!(ui.get_menu_open(), -1);
}

/// pages > sessions > save (the reference's flow, model-checked in
/// hydrus-gui-model's session_saving): "as new session…" asks a name,
/// refuses a reserved one with a warning, saves a new one (copies of the
/// pages, with their own keys), asks before overwriting an existing one
/// ("no, choose another name" asks again); a session's own entry asks
/// whether to overwrite it, and yes saves the pages as they are now.
#[test]
fn sessions_are_saved_from_the_pages_menu() {
    use hydrus_gui::session_saving::{NAME_MESSAGE, RESERVED_WARNING};
    use hydrus_store::sessions;
    let (_dirs, store) = store();
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    let title =
        |label: &str| -> i32 { titles(&ui).iter().position(|(t, _)| t == label).unwrap() as i32 };
    let save = |entry: &str| {
        ui.invoke_menu_title_pressed(title("pages"), 80.0, 22.0);
        hover(&ui, "sessions");
        hover(&ui, "save");
        choose(&ui, entry);
    };
    let dialog = || {
        bound
            .session_dialog
            .borrow()
            .as_ref()
            .map(slint::ComponentHandle::clone_strong)
    };
    let saved = |name: &str| {
        let name = name.to_owned();
        store.read(move |c| sessions::load(c, &name)).unwrap()
    };
    // (the page shown with files)
    ui.invoke_search_edited("system:everything".into());
    ui.invoke_search_accepted();

    save("as new session\u{2026}");
    let d = dialog().expect("a name is asked");
    assert!(d.get_asking_name());
    assert_eq!(
        (d.get_window_title().as_str(), d.get_message().as_str()),
        ("Enter Text", NAME_MESSAGE)
    );
    d.invoke_name_entered("last session".into());
    assert!(d.get_asking_name());
    assert_eq!(d.get_warning().as_str(), RESERVED_WARNING);
    d.invoke_name_entered("my session".into());
    assert!(dialog().is_none(), "saved");
    let live = bound.pages.borrow().session().clone();
    let mine = saved("my session").expect("saved");
    // (saved now, in seconds)
    let now = hydrus_core::time::TimestampMs::now().millis() / 1000;
    let when = store
        .read(sessions::names)
        .unwrap()
        .into_iter()
        .find(|(name, _)| name == "my session")
        .unwrap()
        .1;
    assert!((now - 5..=now).contains(&when), "{when} at {now}");
    let names = |s: &hydrus_core::pages::Session| -> Vec<String> {
        s.pages.iter().map(|p| p.name.clone()).collect()
    };
    assert_eq!(names(&mine), names(&live));
    assert_ne!(
        mine.pages[0].key, live.pages[0].key,
        "copies, not the open pages"
    );
    // (with their files)
    let files = |key: hydrus_core::pages::PageKey| {
        store.read(move |c| sessions::page_files(c, &key)).unwrap()
    };
    let shown = live
        .pages
        .iter()
        .position(|p| p.key == bound.pages.borrow().shown().key)
        .unwrap();
    assert!(!files(live.pages[shown].key).is_empty());
    assert_eq!(files(mine.pages[shown].key), files(live.pages[shown].key));

    // an existing name: asked; no asks another; cancelled, nothing more
    save("as new session\u{2026}");
    let d = dialog().unwrap();
    d.invoke_name_entered("my session".into());
    assert!(!d.get_asking_name());
    assert_eq!(
        (
            d.get_message().as_str(),
            d.get_window_title().as_str(),
            d.get_yes_label().as_str(),
            d.get_no_label().as_str()
        ),
        (
            "Session \"my session\" already exists! Do you want to overwrite it?",
            "Overwrite existing session?",
            "yes, overwrite",
            "no, choose another name"
        )
    );
    d.invoke_answered(false);
    assert!(d.get_asking_name());
    d.invoke_cancelled();
    assert!(dialog().is_none());

    // a session's own entry: asked; yes saves the pages as they are now
    ui.invoke_new_page();
    ui.invoke_chooser_pressed(4);
    ui.invoke_chooser_pressed(4);
    save("my session");
    let d = dialog().unwrap();
    assert_eq!(
        (d.get_message().as_str(), d.get_no_label().as_str()),
        ("Overwrite \"my session\" session?", "no")
    );
    d.invoke_answered(false);
    assert!(dialog().is_none());
    assert_eq!(saved("my session").unwrap().pages.len(), live.pages.len());
    save("my session");
    dialog().unwrap().invoke_answered(true);
    assert_eq!(
        saved("my session").unwrap().pages.len(),
        live.pages.len() + 1
    );
}

/// pages > sessions > "clear and load" > a session: asked first, then the
/// pages are closed for good and the session's pages are at the top.
#[test]
fn clear_and_load_replaces_the_pages_with_a_session() {
    use hydrus_gui::session_saving::clear_and_load_question;
    let (_dirs, store) = store();
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    let title =
        |label: &str| -> i32 { titles(&ui).iter().position(|(t, _)| t == label).unwrap() as i32 };
    let session_menu = |submenu: &str, entry: &str| {
        ui.invoke_menu_title_pressed(title("pages"), 80.0, 22.0);
        hover(&ui, "sessions");
        hover(&ui, submenu);
        choose(&ui, entry);
    };
    // a session of the one page, saved; then two more pages
    bound
        .pages
        .borrow_mut()
        .save_session("one page", 0)
        .unwrap();
    (bound.open_page)(&hydrus_gui::page_chooser::NewPage::Duplicates);
    (bound.open_page)(&hydrus_gui::page_chooser::NewPage::Duplicates);
    let before = tabs(&ui).len();
    assert!(before >= 3, "{:?}", tabs(&ui));

    // no: nothing changes
    session_menu("clear and load", "one page");
    assert_eq!(ui.get_question(), clear_and_load_question("one page"));
    ui.invoke_answer(false);
    assert_eq!(tabs(&ui).len(), before);
    // yes: the session's page alone, none to reopen
    session_menu("clear and load", "one page");
    ui.invoke_answer(true);
    assert_eq!(tabs(&ui).len(), 1);
    assert!(!bound.pages.borrow_mut().unclose());
}
