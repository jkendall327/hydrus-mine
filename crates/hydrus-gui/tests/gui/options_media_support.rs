//! Shared helpers of the options-media lane: a main window on the `basic`
//! store whose headless windows can be drawn.

use std::sync::Arc;

use slint::{ComponentHandle as _, Model as _};

use hydrus_gui::{Bound, MainWindow, OptionsWindow, Pages, SearchPage, bind, headless};
use hydrus_store::Store;

pub struct Media {
    pub store: Arc<Store>,
    pub ui: MainWindow,
    pub bound: Bound,
    pub windows: headless::Windows,
    _dirs: Vec<tempfile::TempDir>,
}

impl Media {
    pub fn basic() -> Self {
        let (dirs, store) = crate::options_gui_support::basic_store();
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
            windows,
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
        let options = self
            .bound
            .options
            .borrow()
            .as_ref()
            .expect("the options window")
            .clone_strong();
        options.show().unwrap();
        // (drawn once, as a shown window is, before it is used)
        let native = self.windows.get(self.windows.count() - 1).unwrap();
        for _ in 0..2 {
            headless::render(&native, 950, 700);
        }
        options
    }

    /// A saved setting, as the consumers read it.
    pub fn setting<T: hydrus_store::settings::Setting + Default>(&self) -> T {
        self.store
            .read(hydrus_store::settings::get::<T>)
            .unwrap_or_default()
    }

    pub fn search(&self, query: &str) {
        self.ui.invoke_search_edited(query.into());
        self.ui.invoke_search_accepted();
    }

    pub fn results(&self) -> Vec<hydrus_core::HashId> {
        self.bound.current.borrow().borrow().results().to_vec()
    }
}
