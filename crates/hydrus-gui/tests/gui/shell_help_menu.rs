//! Help menu entries driven through the real main menu bar: each link
//! reaches the OS launcher with the reference's URL, and advanced mode
//! toggles in the store (hydrus `_InitialiseMenuInfoHelp`).

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use slint::Model as _;

use hydrus_gui::{MainWindow, Pages, bind, headless};
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

fn labels(ui: &MainWindow, pane: usize) -> Vec<String> {
    let lines = ui.get_menu_panes().row_data(pane).unwrap().lines;
    (0..lines.row_count())
        .map(|i| lines.row_data(i).unwrap().label.to_string())
        .collect()
}

fn open_help(ui: &MainWindow) {
    let titles = ui.get_menu_titles();
    let help = (0..titles.row_count())
        .position(|i| titles.row_data(i).unwrap().label == "help")
        .unwrap();
    ui.invoke_menu_title_pressed(i32::try_from(help).unwrap(), 300.0, 22.0);
}

fn index(ui: &MainWindow, pane: usize, label: &str) -> i32 {
    labels(ui, pane)
        .iter()
        .position(|l| l == label)
        .unwrap_or_else(|| panic!("{label} in {:?}", labels(ui, pane))) as i32
}

fn hover(ui: &MainWindow, pane: usize, label: &str) {
    let i = index(ui, pane, label);
    ui.invoke_menu_line_hovered(pane as i32, i, 300.0, 40.0 + 22.0 * i as f32, 150.0);
}

fn choose(ui: &MainWindow, pane: usize, label: &str) {
    ui.invoke_menu_line_clicked(pane as i32, index(ui, pane, label), 0.0, 0.0, 0.0);
}

fn launched_by(ui: &MainWindow, path: &[&str]) -> Vec<String> {
    let launched: Rc<RefCell<Vec<String>>> = Rc::default();
    hydrus_gui::set_launcher({
        let launched = launched.clone();
        move |target| launched.borrow_mut().push(target.to_owned())
    });
    open_help(ui);
    for (depth, label) in path.iter().enumerate() {
        if depth + 1 < path.len() {
            hover(ui, depth, label);
        } else {
            choose(ui, depth, label);
        }
    }
    launched.borrow().clone()
}

fn bound_window() -> (
    [tempfile::TempDir; 2],
    Arc<Store>,
    MainWindow,
    hydrus_gui::Bound,
) {
    let (dirs, store) = store();
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    (dirs, store, ui, bound)
}

// leaf: audit-options-menu-menu-help-site
// leaf: audit-options-menu-menu-help-github-repository
// leaf: audit-options-menu-menu-help-latest-build
// leaf: audit-options-menu-menu-help-issue-tracker
// leaf: audit-options-menu-menu-help-8chan-moe-t-hydrus-network-general
// leaf: audit-options-menu-menu-help-x
// leaf: audit-options-menu-menu-help-tumblr
// leaf: audit-options-menu-menu-help-discord
// leaf: audit-options-menu-menu-help-patreon
#[test]
fn every_help_link_opens_the_references_url() {
    let (_dirs, _store, ui, _bound) = bound_window();
    open_help(&ui);
    // the reference's order inside help > links
    hover(&ui, 0, "links");
    assert_eq!(
        labels(&ui, 1),
        [
            "site",
            "github repository",
            "latest build",
            "issue tracker",
            "8chan.moe /t/ (Hydrus Network General)",
            "x",
            "tumblr",
            "discord",
            "patreon",
        ]
    );
    ui.invoke_menu_dismissed();
    for (label, url) in [
        ("site", "https://hydrusnetwork.github.io/hydrus/"),
        (
            "github repository",
            "https://github.com/hydrusnetwork/hydrus",
        ),
        (
            "latest build",
            "https://github.com/hydrusnetwork/hydrus/releases/latest",
        ),
        (
            "issue tracker",
            "https://github.com/hydrusnetwork/hydrus/issues",
        ),
        (
            "8chan.moe /t/ (Hydrus Network General)",
            "https://8chan.moe/t/catalog.html",
        ),
        ("x", "https://x.com/hydrusnetwork"),
        ("tumblr", "https://hydrus.tumblr.com/"),
        ("discord", "https://discord.gg/wPHPCUZ"),
        ("patreon", "https://www.patreon.com/hydrus_dev"),
    ] {
        assert_eq!(launched_by(&ui, &["links", label]), [url], "{label}");
        assert!(
            ui.get_menu_panes().row_count() == 0,
            "{label} closes the menu"
        );
    }
}

// leaf: audit-options-menu-menu-help-open-help
// leaf: audit-options-menu-menu-help-changelog
#[test]
fn open_help_and_changelog_launch_their_pages() {
    let (_dirs, _store, ui, _bound) = bound_window();
    assert_eq!(
        launched_by(&ui, &["open help"]),
        ["https://hydrusnetwork.github.io/hydrus/"]
    );
    assert_eq!(
        launched_by(&ui, &["changelog"]),
        ["https://hydrusnetwork.github.io/hydrus/changelog.html"]
    );
}

// leaf: audit-options-menu-menu-help-advanced-mode
#[test]
fn advanced_mode_toggles_persists_and_ticks() {
    let (_dirs, store, ui, _bound) = bound_window();
    let read = || {
        let mode: hydrus_store::settings::AdvancedMode =
            store.read(hydrus_store::settings::get).unwrap();
        mode.0
    };
    let ticked = |ui: &MainWindow| {
        let lines = ui.get_menu_panes().row_data(0).unwrap().lines;
        let i = index(ui, 0, "advanced mode") as usize;
        lines.row_data(i).unwrap().checked
    };
    assert!(!read());
    open_help(&ui);
    assert!(!ticked(&ui));
    choose(&ui, 0, "advanced mode");
    assert!(read());
    open_help(&ui);
    assert!(ticked(&ui), "reopened menu shows it ticked");
    choose(&ui, 0, "advanced mode");
    assert!(!read(), "and off again");
}

// leaf: audit-options-about-header
// leaf: audit-options-about-credits
// leaf: audit-options-about-license
#[test]
fn about_window_header_credits_and_license_match_the_recording() {
    use slint::ComponentHandle as _;
    let (_dirs, _store, ui, bound) = bound_window();
    let recorded = hydrus_testkit::fixture_json("about_window.json");
    let launched: Rc<RefCell<Vec<String>>> = Rc::default();
    hydrus_gui::set_launcher({
        let launched = launched.clone();
        move |target| launched.borrow_mut().push(target.to_owned())
    });
    open_help(&ui);
    choose(&ui, 0, "about");
    let window = bound.about.borrow().as_ref().unwrap().clone_strong();
    assert_eq!(
        window.get_window_title(),
        recorded["title"].as_str().unwrap()
    );
    // the reference's second label is "v688, using network version 20"
    let theirs = recorded["labels"][1].as_str().unwrap();
    assert!(
        window
            .get_version()
            .ends_with(&format!("porting hydrus {theirs}"))
    );
    // the site label is a hyperlink to the reference's site, and opens it
    let site = window.get_site().to_string();
    assert!(recorded["labels"][2].as_str().unwrap().contains(&site));
    window.invoke_open_site();
    assert_eq!(*launched.borrow(), [site]);
    let tabs = recorded["tabs"].as_array().unwrap();
    let texts = window.get_texts();
    assert!(
        texts
            .row_data(2)
            .unwrap()
            .starts_with(tabs[2][1].as_str().unwrap()),
        "credits begin as the reference's"
    );
    assert_eq!(texts.row_data(3).unwrap(), tabs[3][1].as_str().unwrap());
    window.invoke_close_clicked();
}
