//! Shared helpers of the options-gui lane's tests: a main window on the
//! `basic` store (or another), its options window, and rows by label.

use std::sync::Arc;

use slint::{ComponentHandle as _, Model as _};

use hydrus_gui::{Bound, MainWindow, OptionRow, OptionsWindow, Pages, SearchPage, bind, headless};
use hydrus_store::Store;
use hydrus_store::import::import_legacy;

/// A main window bound to a store, with the windows' collector alive.
pub struct Client {
    pub store: Arc<Store>,
    pub ui: MainWindow,
    pub bound: Bound,
    _windows: headless::Windows,
    _dirs: Vec<tempfile::TempDir>,
}

/// The `basic` fixture, migrated into a new store.
pub fn basic_store() -> (Vec<tempfile::TempDir>, Arc<Store>) {
    let legacy = hydrus_testkit::legacy_fixture("basic");
    let native = tempfile::tempdir().unwrap();
    import_legacy(
        legacy.path(),
        &native.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store = Store::open(native.path()).unwrap();
    (vec![legacy, native], store)
}

impl Client {
    pub fn basic() -> Self {
        let (dirs, store) = basic_store();
        Self::with(dirs, store)
    }

    pub fn with(dirs: Vec<tempfile::TempDir>, store: Arc<Store>) -> Self {
        let windows = headless::init();
        let ui = MainWindow::new().unwrap();
        ui.show().unwrap();
        let bound = bind(&ui, Pages::single(SearchPage::new(store.clone())));
        Self {
            store,
            ui,
            bound,
            _windows: windows,
            _dirs: dirs,
        }
    }

    /// file > options…
    pub fn open_options(&self) -> OptionsWindow {
        self.ui.invoke_menu_title_pressed(0, 20.0, 22.0);
        let lines = self.ui.get_menu_panes().row_data(0).unwrap().lines;
        let at = (0..lines.row_count())
            .position(|i| lines.row_data(i).unwrap().label == "options\u{2026}")
            .expect("file > options");
        self.ui
            .invoke_menu_line_clicked(0, at as i32, 0.0, 0.0, 0.0);
        self.bound
            .options
            .borrow()
            .as_ref()
            .expect("the options window")
            .clone_strong()
    }

    /// A saved setting, as the consumers read it.
    pub fn setting<T: hydrus_store::settings::Setting + Default>(&self) -> T {
        self.store
            .read(hydrus_store::settings::get::<T>)
            .unwrap_or_default()
    }
}

pub fn page_names(options: &OptionsWindow) -> Vec<String> {
    let pages = options.get_pages();
    (0..pages.row_count())
        .map(|i| pages.row_data(i).unwrap().text.to_string())
        .collect()
}

pub fn show_page(options: &OptionsWindow, name: &str) {
    let i = page_names(options)
        .iter()
        .position(|n| n == name)
        .unwrap_or_else(|| panic!("no page {name:?}")) as i32;
    options.set_page(i);
    options.invoke_page_chosen(i);
}

/// The row with this label (the reference's, trailing space and all) and
/// its index.
pub fn row(options: &OptionsWindow, label: &str) -> (i32, OptionRow) {
    let rows = options.get_rows();
    (0..rows.row_count())
        .map(|i| (i as i32, rows.row_data(i).unwrap()))
        .find(|(_, r)| r.label == label)
        .unwrap_or_else(|| panic!("{label:?}"))
}

/// The labels of the boxes (kind 0) above a row, outermost first: the
/// reference's static boxes it sits in.
pub fn box_of(options: &OptionsWindow, label: &str) -> String {
    let rows = options.get_rows();
    let (at, _) = row(options, label);
    (0..at)
        .rev()
        .map(|i| rows.row_data(i as usize).unwrap())
        .find(|r| r.kind == 0)
        .map(|r| r.label.to_string())
        .unwrap_or_default()
}

/// The labels of the open menus' lines, a pane at a time.
pub fn menu_lines(ui: &MainWindow) -> Vec<Vec<String>> {
    let panes = ui.get_menu_panes();
    (0..panes.row_count())
        .map(|p| {
            let lines = panes.row_data(p).unwrap().lines;
            (0..lines.row_count())
                .map(|i| lines.row_data(i).unwrap().label.to_string())
                .collect()
        })
        .collect()
}

/// Press a title of the menu bar ("network").
pub fn press_title(ui: &MainWindow, label: &str) {
    let titles = ui.get_menu_titles();
    let at = (0..titles.row_count())
        .position(|i| titles.row_data(i).unwrap().label == label)
        .unwrap_or_else(|| panic!("menu {label:?}"));
    ui.invoke_menu_title_pressed(at as i32, 200.0, 22.0);
}

/// Point at a line of the last open menu (its submenu opens).
pub fn hover_line(ui: &MainWindow, label: &str) {
    let panes = menu_lines(ui);
    let p = panes.len() - 1;
    let i = panes[p]
        .iter()
        .position(|l| l == label)
        .unwrap_or_else(|| panic!("{label} in {:?}", panes[p]));
    ui.invoke_menu_line_hovered(
        p as i32,
        i as i32,
        300.0 + 150.0 * p as f32,
        40.0 + 22.0 * i as f32,
        150.0 * p as f32,
    );
}

/// A dropdown row's choices.
pub fn items(row: &OptionRow) -> Vec<String> {
    (0..row.items.row_count())
        .map(|i| row.items.row_data(i).unwrap().to_string())
        .collect()
}
