//! Shared set-up for the media lane's tests: a copy of the `basic` fixture
//! in a headless main window showing `system:everything`, and menu helpers.

use std::sync::Arc;

use hydrus_core::HashId;
use hydrus_gui::{Bound, MainWindow, MenuGroups, MenuRow, Pages, SearchPage, bind, headless};
use hydrus_store::Store;
use hydrus_store::import::import_legacy;
use slint::{ComponentHandle as _, Model as _};

pub struct Fixture {
    pub _dir: tempfile::TempDir,
    pub _legacy: Box<dyn std::any::Any>,
    pub windows_: headless::Windows,
    pub store: Arc<Store>,
    pub ui: MainWindow,
    pub bound: Bound,
}

pub fn store() -> (Box<dyn std::any::Any>, tempfile::TempDir, Arc<Store>) {
    let legacy = hydrus_testkit::legacy_fixture("basic");
    let dir = tempfile::tempdir().unwrap();
    import_legacy(
        legacy.path(),
        &dir.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store = Store::open(dir.path()).unwrap();
    (Box::new(legacy), dir, store)
}

/// A window on `system:everything` over the fixture's files.
pub fn start() -> Fixture {
    start_in(&headless::init())
}

/// A further window and store in a test that has already started one
/// (the platform is set once per thread).
pub fn start_in(windows: &headless::Windows) -> Fixture {
    let windows = windows.clone();
    let (legacy, dir, store) = store();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(&ui, Pages::single(SearchPage::new(store.clone())));
    ui.invoke_search_edited("system:everything".into());
    ui.invoke_search_accepted();
    Fixture {
        _legacy: legacy,
        _dir: dir,
        windows_: windows,
        store,
        ui,
        bound,
    }
}

impl Fixture {
    pub fn another(&self) -> Fixture {
        start_in(&self.windows_)
    }

    pub fn results(&self) -> Vec<HashId> {
        self.bound.current.borrow().borrow().results().to_vec()
    }

    pub fn selected(&self) -> Vec<HashId> {
        self.bound.current.borrow().borrow().selected_files()
    }

    /// Click the thumbnail at `first`, then ctrl+click the `others`.
    pub fn select(&self, first: i32, others: &[i32]) {
        self.ui.invoke_thumbnail_clicked(first, false, false);
        for &other in others {
            self.ui.invoke_thumbnail_clicked(other, true, false);
        }
    }

    /// Select exactly `files`.
    pub fn select_files(&self, files: &[HashId]) {
        self.bound.current.borrow().borrow_mut().select_files(files);
    }

    /// The thumbnail menu as asked for the selection (-1) or a thumbnail.
    pub fn menu(&self, at: i32) -> hydrus_gui::ThumbnailMenu {
        self.ui.invoke_thumbnail_menu_requested(at);
        self.ui.get_thumbnail_menu()
    }
}

pub fn rows(rows: &slint::ModelRc<MenuRow>) -> Vec<(String, i32)> {
    rows.iter().map(|r| (r.label.to_string(), r.id)).collect()
}

pub fn group_rows(groups: &MenuGroups) -> Vec<(String, i32)> {
    [
        &groups.g1, &groups.g2, &groups.g3, &groups.g4, &groups.g5, &groups.g6,
    ]
    .into_iter()
    .flat_map(self::rows)
    .collect()
}

/// The id of the row whose label starts with `prefix`.
pub fn find(rows: &[(String, i32)], prefix: &str) -> i32 {
    rows.iter()
        .find(|(label, _)| label.starts_with(prefix))
        .unwrap_or_else(|| panic!("{prefix} in {rows:?}"))
        .1
}

/// The files of the fixture currently in the domain called `name`.
pub fn in_domain(store: &Store, name: &str) -> Vec<HashId> {
    let name = name.to_owned();
    store
        .read(move |c| {
            let mut stmt = c.prepare(
                "SELECT hash_id FROM file_domain_current WHERE service_id =
                 (SELECT service_id FROM services WHERE name = ?1)",
            )?;
            Ok(stmt
                .query_map([name], |r| r.get::<_, HashId>(0))?
                .collect::<rusqlite::Result<Vec<_>>>()?)
        })
        .unwrap()
}

/// Like `start`, on the page of every local file domain (trash included).
pub fn start_local() -> Fixture {
    let (legacy, dir, store) = store();
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(
        &ui,
        Pages::single(super::common::all_local_page(store.clone())),
    );
    ui.invoke_search_edited("system:everything".into());
    ui.invoke_search_accepted();
    Fixture {
        _legacy: legacy,
        _dir: dir,
        windows_: windows,
        store,
        ui,
        bound,
    }
}
