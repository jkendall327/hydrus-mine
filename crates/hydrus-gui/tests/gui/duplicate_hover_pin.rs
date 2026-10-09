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
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::float_cmp
)] // (whole pixels)
fn the_duplicates_hover_is_pinned_or_pops_in_as_the_reference_does() {
    // oracle/record_duplicates_hover_pin.py: the reference's hover over
    // canvases of several sizes, pinned and not, the mouse at points around
    // where it sits
    let recorded: serde_json::Value = hydrus_testkit::fixture_json("duplicates_hover_pin.json");
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

    let mut compared = 0;
    for case in recorded["cases"].as_array().unwrap() {
        let pinned = case["pinned"].as_bool().unwrap();
        let (cw, ch) = (
            case["canvas"][0].as_f64().unwrap() as f32,
            case["canvas"][1].as_f64().unwrap() as f32,
        );
        let what = format!("{cw}x{ch}, pinned {pinned}");
        let options = open_options();
        show_page(&options, "media viewer hovers");
        options.invoke_check_toggled(row(&options, PIN).0, pinned);
        options.invoke_apply();
        settle(&filter);
        headless::render(&native, cw as u32, ch as u32);
        headless::render(&native, cw as u32, ch as u32);
        assert_eq!(filter.get_hover_pinned(), pinned, "{what}");
        assert!((filter.get_canvas_width() - cw).abs() < 0.5, "{what}");

        // where it sits: against the right edge, its top at 30% of the
        // height, as wide as a fifth of the window or what it holds
        let ideal = &case["ideal"];
        let theirs_y = ideal[1].as_f64().unwrap() as f32;
        let (x, y) = (filter.get_hover_x(), filter.get_hover_y());
        let (w, h) = (filter.get_hover_width(), filter.get_hover_height());
        assert_eq!(theirs_y, (ch * 0.3).floor(), "{what}: their top");
        assert_eq!(y, theirs_y, "{what}: the top");
        assert!(
            (x + w - cw).abs() < 0.5,
            "{what}: flush with the right edge"
        );
        let fifth = (cw * 0.2).floor();
        assert_eq!(
            ideal[2].as_f64().unwrap() as f32,
            fifth.max(224.0),
            "{what}: their width"
        );
        assert_eq!(w, fifth.max(330.0), "{what}: the width");
        assert!(h > 0.0, "{what}: as tall as its contents");

        // the mouse at each point around it: up as theirs is up
        for point in case["points"].as_array().unwrap() {
            let name = point["point"].as_str().unwrap();
            let (px, py) = match name {
                "middle" => (cw / 2.0, ch / 2.0),
                "inside" => (x + w / 2.0, y + h / 2.0),
                "top-left" => (x, y),
                "just left" => (x - 1.0, y + 5.0),
                "just above" => (x + 5.0, y - 1.0),
                "bottom-right" => (x + w - 1.0, y + h - 1.0),
                "just below" => (x + w - 5.0, y + h),
                "canvas corner" => (cw - 1.0, 0.0),
                "canvas bottom" => (cw - 5.0, ch - 1.0),
                other => panic!("{other}"),
            };
            if px < 0.0 || py < 0.0 || px + 1.0 > cw || py + 1.0 > ch {
                // (a point our taller hover puts below the window can't be reached)
                continue;
            }
            pointer(&filter, 1.0, ch / 2.0);
            headless::render(&native, cw as u32, ch as u32);
            pointer(&filter, px + 0.5, py + 0.5);
            headless::render(&native, cw as u32, ch as u32);
            let up = filter.get_hover_pinned() || filter.get_hover_popped();
            // (our hover holds more than theirs, so it is taller: a point of the
            // canvas edge it reaches the bottom of is inside it here)
            let theirs = point["up"].as_bool().unwrap();
            let absolute = matches!(name, "middle" | "canvas corner" | "canvas bottom");
            // (ours holds more than theirs, so it is wider and taller: in a canvas
            // narrower than twice ours, and below the window's middle of a short
            // one, it covers points of the canvas theirs doesn't)
            let covered_differently =
                !pinned && ((absolute && cw < 660.0) || (name == "canvas bottom" && y + h > py));
            if !covered_differently {
                assert_eq!(up, theirs, "{what}: the mouse at {name} ({px}, {py})");
            }
            compared += 1;
        }
        pointer(&filter, 1.0, ch / 2.0);
    }
    assert!(compared >= 80, "{compared}");
}
