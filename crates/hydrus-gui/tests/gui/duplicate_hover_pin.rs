//! The duplicates filter's right hover against the reference's
//! `hover_window_duplicates_always_on_top`: pinned (the default) it is always
//! there; unpinned it pops in over the canvas' right edge on mouseover.

use std::sync::Arc;

use slint::platform::WindowEvent;
use slint::{ComponentHandle as _, Model as _};

use hydrus_core::duplicates::DuplicatesSearch;
use hydrus_core::pages::{DuplicatesPage, Page, PageContent, PageKey, Session};
use hydrus_gui::{MainWindow, Pages, bind, headless};
use hydrus_search::{FileSearchContext, LocationContext};
use hydrus_store::duplicates::{PairSearchKind, PixelDuplicates};
use hydrus_store::sessions::{self, LAST_SESSION};

use crate::options_gui_support::{row, show_page};

const PIN: &str =
    "Pin the duplicates (right, duplicates filter) hover window so it is always visible:";

fn pointer(filter: &hydrus_gui::DuplicateFilterWindow, x: f32, y: f32) {
    filter.window().dispatch_event(WindowEvent::PointerMoved {
        position: slint::LogicalPosition::new(x, y),
    });
}

fn settle(filter: &hydrus_gui::DuplicateFilterWindow) {
    for _ in 0..5 {
        slint::platform::update_timers_and_animations();
        std::thread::sleep(std::time::Duration::from_millis(40));
    }
    let _ = filter;
}

// leaf: audit-options-media-viewer-hovers-hover-windows-pin-the-duplicates-right-duplicates-filter-hover-window-so-it-is-always-visible
#[test]
fn the_duplicates_hover_is_pinned_or_pops_in_as_the_option_says() {
    let windows = headless::init();
    let (_dir, store) = super::duplicate_filter::store_with_pairs();
    let (_, key) = super::duplicate_filter::my_files(&store);
    let search = FileSearchContext {
        location: LocationContext::single(key),
        ..FileSearchContext::default()
    };
    let duplicates = DuplicatesPage::new(DuplicatesSearch {
        search_1: search.clone(),
        search_2: search,
        kind: PairSearchKind::OneFileMatchesOneSearch,
        pixel_duplicates: PixelDuplicates::Allowed,
        max_hamming_distance: 4,
    });
    let session = Session {
        name: LAST_SESSION.into(),
        pages: vec![Page {
            key: PageKey::random(),
            name: "duplicates".into(),
            content: PageContent::Duplicates {
                duplicates,
                sort: None,
            },
        }],
    };
    store
        .write(move |ctx| sessions::save(ctx.conn(), &session, 0))
        .unwrap();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(Arc::clone(&store)).unwrap());
    ui.invoke_launch_filter();
    let filter = bound.filter.borrow().as_ref().unwrap().clone_strong();
    let native = windows.get(1).expect("the filter's window");
    headless::render(&native, 1200, 800);

    // the options page: the reference's label and its default, on
    let open_options = || {
        ui.invoke_menu_title_pressed(0, 20.0, 22.0);
        let lines = ui.get_menu_panes().row_data(0).unwrap().lines;
        let at = (0..lines.row_count())
            .position(|i| lines.row_data(i).unwrap().label == "options\u{2026}")
            .unwrap();
        ui.invoke_menu_line_clicked(0, i32::try_from(at).unwrap(), 0.0, 0.0, 0.0);
        bound.options.borrow().as_ref().unwrap().clone_strong()
    };
    let options = open_options();
    show_page(&options, "media viewer hovers");
    let (_, shown) = row(&options, PIN);
    assert_eq!((shown.kind, shown.checked), (1, true));
    options.invoke_cancel();

    // pinned: the comparison is always up, floating over the canvas' right
    // edge (the canvas keeps the whole window)
    settle(&filter);
    headless::render(&native, 1200, 800);
    assert!(filter.get_hover_pinned());
    assert!(!filter.get_hover_popped());
    assert!((filter.get_canvas_width() - 1200.0).abs() < 0.5);
    assert!(
        (filter.get_hover_x() - 870.0).abs() < 0.5,
        "{}",
        filter.get_hover_x()
    );

    // unpinned: the canvas has the room, and nothing shows until the mouse
    // is at its right edge
    let options = open_options();
    show_page(&options, "media viewer hovers");
    options.invoke_check_toggled(row(&options, PIN).0, false);
    options.invoke_apply();
    settle(&filter);
    headless::render(&native, 1200, 800);
    assert!(!filter.get_hover_pinned());
    assert!(!filter.get_hover_popped());
    assert!((filter.get_canvas_width() - 1200.0).abs() < 0.5);
    pointer(&filter, 300.0, 400.0);
    headless::render(&native, 1200, 800);
    assert!(!filter.get_hover_popped(), "mouse in the middle");
    pointer(&filter, 1100.0, 400.0);
    headless::render(&native, 1200, 800);
    assert!(filter.get_hover_popped(), "mouse at the right edge");
    // it stays up while the mouse is over it, and goes with the mouse
    pointer(&filter, 1190.0, 300.0);
    headless::render(&native, 1200, 800);
    assert!(filter.get_hover_popped());
    pointer(&filter, 300.0, 400.0);
    headless::render(&native, 1200, 800);
    assert!(!filter.get_hover_popped());

    // pinned again
    let options = open_options();
    show_page(&options, "media viewer hovers");
    options.invoke_check_toggled(row(&options, PIN).0, true);
    options.invoke_apply();
    settle(&filter);
    headless::render(&native, 1200, 800);
    assert!(filter.get_hover_pinned());
    assert!((filter.get_canvas_width() - 1200.0).abs() < 0.5);
    assert!((filter.get_hover_x() - 870.0).abs() < 0.5);
}
