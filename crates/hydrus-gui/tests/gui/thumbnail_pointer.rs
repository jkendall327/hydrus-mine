//! The thumbnail grid under real pointer events, replayed against the
//! reference's default grid (`oracle/fixtures/thumbnail_pointer.json`, from
//! `oracle/record_thumbnail_pointer.py`, v688's
//! `MediaResultsPanelThumbnailsGraphicsViewTest`): left clicks with ctrl
//! and shift and on the space between thumbnails, a double-click, a middle
//! press (the viewer opened on the thumbnail, the selection untouched;
//! nothing on empty space) and right presses (the menu, after selecting the
//! thumbnail pressed, or nothing changed on empty space). The page holds the
//! recording's files in its order, as the recorder's page did.

use std::sync::Arc;

use i_slint_core::window::WindowInner;
use slint::platform::PointerEventButton;
use slint::{ComponentHandle as _, LogicalPosition};

use hydrus_core::HashId;
use hydrus_gui::{MainWindow, Pages, SearchPage, bind, headless};
use hydrus_store::Store;
use hydrus_store::import::import_legacy;

use crate::common::widgets::{self, Mods};

// leaf: interaction-thumbnail-middle-click
// leaf: interaction-thumbnail-click-select
// leaf: interaction-thumbnail-double-click-open
// leaf: interaction-thumbnail-context-menu
#[test]
fn presses_on_the_grid_select_open_and_show_the_menu_as_the_reference_s() {
    let recorded = hydrus_testkit::fixture_json("thumbnail_pointer.json");
    assert_eq!(
        recorded["graphics_view"],
        "MediaResultsPanelThumbnailsGraphicsViewTest"
    );
    let legacy = hydrus_testkit::legacy_fixture("basic");
    let native = tempfile::tempdir().unwrap();
    import_legacy(
        legacy.path(),
        &native.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store: Arc<Store> = Store::open(native.path()).unwrap();
    // (the recorder notes the viewer's launch without opening one, so no
    // viewer closes there to select the file it showed; the viewers this
    // test opens and closes are told not to, by the reference's option)
    store
        .write(|tx| {
            hydrus_store::settings::set(
                tx.conn(),
                &hydrus_store::settings::ViewerClosingSettings {
                    select_exit_media: false,
                    ..Default::default()
                },
            )
        })
        .unwrap();
    // the recording's files, by hash, in its page's order
    let theirs: Vec<HashId> = recorded["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|h| {
            store
                .read(|c| hydrus_store::master::hash_id(c, &h.as_str().unwrap().parse().unwrap()))
                .unwrap()
                .unwrap()
        })
        .collect();
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    // a page of them on "my files", as the recorder opened (the files
    // given, in their order)
    let mut pages = Pages::single(SearchPage::new(store.clone()));
    pages.open_files(
        hydrus_search::LocationContext::single(hydrus_core::ServiceKey::new(
            hydrus_core::service::builtin_keys::MY_FILES.to_vec(),
        )),
        theirs.clone(),
        None,
        None,
    );
    let bound = bind(&ui, pages);
    let page = bound.current.borrow().clone();
    assert_eq!(page.borrow().files(), theirs);
    let main_window = windows.get(0).unwrap();
    // (tall enough for every thumbnail)
    headless::render(&main_window, 1100, 1600);
    let columns = usize::try_from(ui.get_grid_columns()).unwrap();
    let win = ui.window();

    let files = page.borrow().files();
    let ours = |recorded_index: &serde_json::Value| -> Option<usize> {
        recorded_index.as_u64().map(|i| {
            let id = theirs[usize::try_from(i).unwrap()];
            files
                .iter()
                .position(|&f| f == id)
                .unwrap_or_else(|| panic!("file {i} is in the page ({} files)", files.len()))
        })
    };
    let selected = || -> Vec<HashId> {
        let mut s: Vec<HashId> = page
            .borrow()
            .selected_indices()
            .into_iter()
            .map(|i| files[i])
            .collect();
        s.sort_unstable();
        s
    };
    // where the file at `index` is (its middle), or the gap after the
    // first thumbnail of the first row
    let at = |index: Option<usize>| -> LogicalPosition {
        let (x0, y0) = (ui.get_grid_origin_x(), ui.get_grid_origin_y());
        match index {
            Some(index) => {
                let (row, column) = (index / columns, index % columns);
                LogicalPosition::new(
                    x0 + 4.0 + 156.0 * column as f32 + 76.0,
                    y0 + 4.0 + 131.0 * row as f32 + 63.0,
                )
            }
            None => LogicalPosition::new(x0 + 156.0 + 1.0, y0 + 63.0),
        }
    };
    for (i, step) in recorded["steps"].as_array().unwrap().iter().enumerate() {
        let context = format!("step {i}: {step}");
        let position = at(ours(&step["file"]));
        let button = match step["button"].as_str().unwrap() {
            "left" => PointerEventButton::Left,
            "middle" => PointerEventButton::Middle,
            "right" => PointerEventButton::Right,
            other => panic!("{other}"),
        };
        let mods = Mods {
            ctrl: step["ctrl"].as_bool().unwrap(),
            shift: step["shift"].as_bool().unwrap(),
            ..Mods::NONE
        };
        if step["double"].as_bool().unwrap() {
            widgets::double_click_at(win, position, button, mods);
        } else {
            widgets::click_at_with(win, position, button, mods);
        }

        let mut want: Vec<HashId> = step["selected"]
            .as_array()
            .unwrap()
            .iter()
            .map(|i| theirs[usize::try_from(i.as_u64().unwrap()).unwrap()])
            .collect();
        want.sort_unstable();
        assert_eq!(selected(), want, "{context}: selected");
        assert_eq!(
            page.borrow().focused(),
            ours(&step["focused"]),
            "{context}: focused"
        );

        // the viewer, opened on the file pressed, or not
        let viewer = bound
            .viewer
            .borrow()
            .as_ref()
            .map(slint::ComponentHandle::clone_strong);
        let launched = step["launched"].as_array().unwrap();
        match launched.first() {
            Some(file) => {
                let viewer = viewer.unwrap_or_else(|| panic!("{context}: no viewer"));
                let index = ours(file).unwrap();
                assert_eq!(
                    viewer.get_caption().split('/').next().unwrap(),
                    (index + 1).to_string(),
                    "{context}: the viewer's file"
                );
                viewer.invoke_close_requested();
                widgets::settle();
                assert!(bound.viewer.borrow().is_none());
            }
            None => assert!(viewer.is_none(), "{context}: no viewer"),
        }

        // the menu, shown or not
        let inner = WindowInner::from_pub(win);
        let shown = !inner.active_popups().is_empty();
        assert_eq!(
            shown,
            !step["menus"].as_array().unwrap().is_empty(),
            "{context}: menu"
        );
        inner.close_all_popups();
    }
}
