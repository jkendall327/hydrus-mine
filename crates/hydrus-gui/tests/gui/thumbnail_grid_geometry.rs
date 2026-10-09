//! The thumbnail grid under the thumbnail size, border and margin options,
//! against the reference's real panel (oracle/record_thumbnail_grid.py): the
//! options set in the Options window, the grid sized to the reference's
//! viewport, its spans, columns and rows, which file a click at each of a set
//! of points around the margins hits, and the size each file's thumbnail is
//! shown at.

use std::sync::Arc;

use slint::platform::{PointerEventButton, WindowEvent};
use slint::{ComponentHandle as _, Model as _};

use hydrus_gui::{MainWindow, OptionsWindow, Pages, bind, headless};
use hydrus_store::Store;

fn open_options(ui: &MainWindow, bound: &hydrus_gui::Bound) -> OptionsWindow {
    ui.invoke_menu_title_pressed(0, 20.0, 22.0);
    let lines = ui.get_menu_panes().row_data(0).unwrap().lines;
    let at = (0..lines.row_count())
        .position(|i| lines.row_data(i).unwrap().label == "options\u{2026}")
        .expect("file > options");
    ui.invoke_menu_line_clicked(0, at as i32, 0.0, 0.0, 0.0);
    let window = bound.options.borrow().as_ref().unwrap().clone_strong();
    let pages = window.get_pages();
    let i = (0..pages.row_count())
        .position(|i| pages.row_data(i).unwrap().text == "thumbnails")
        .unwrap() as i32;
    window.set_page(i);
    window.invoke_page_chosen(i);
    window
}

/// A number row on the thumbnails page, set; its limits are the reference's.
fn set(window: &OptionsWindow, label: &str, limits: (i32, i32), n: i64) {
    let rows = window.get_rows();
    let (i, found) = (0..rows.row_count())
        .map(|i| (i as i32, rows.row_data(i).unwrap()))
        .find(|(_, r)| r.label == label)
        .unwrap_or_else(|| panic!("{label:?}"));
    assert_eq!(
        (found.kind, found.minimum, found.maximum),
        (2, limits.0, limits.1),
        "{label:?}"
    );
    window.invoke_number_edited(i, i32::try_from(n).unwrap());
}

fn int(v: &serde_json::Value) -> i64 {
    v.as_f64().map_or_else(|| v.as_i64().unwrap(), |f| f as i64)
}

// leaf: audit-options-thumbnails-appearance-thumbnail-width
// leaf: audit-options-thumbnails-appearance-thumbnail-height
// leaf: audit-options-thumbnails-appearance-thumbnail-border
// leaf: audit-options-thumbnails-appearance-thumbnail-margin
#[test]
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss
)]
fn the_thumbnail_options_lay_out_the_grid_as_the_reference_does() {
    let recorded: serde_json::Value = hydrus_testkit::fixture_json("thumbnail_grid.json");
    let legacy = hydrus_testkit::legacy_fixture("basic");
    let native = tempfile::tempdir().unwrap();
    hydrus_store::import::import_legacy(
        legacy.path(),
        &native.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store: Arc<Store> = Store::open(native.path()).unwrap();
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(
        &ui,
        Pages::single(super::common::all_local_page(store.clone())),
    );
    ui.invoke_search_edited("system:everything".into());
    ui.invoke_search_accepted();
    let main_window = windows.get(0).unwrap();
    let page = bound.current.borrow().clone();
    let files = page.borrow().results().to_vec();
    let theirs: Vec<&str> = recorded["media"]
        .as_array()
        .unwrap()
        .iter()
        .map(|h| h.as_str().unwrap())
        .collect();
    assert_eq!(files.len(), theirs.len(), "the same files");
    let hash_of = |id| {
        store
            .read(|c| hydrus_store::master::hash(c, id))
            .unwrap()
            .unwrap()
            .to_hex()
    };
    let hashes: Vec<String> = files.iter().map(|&id| hash_of(id)).collect();

    // the main window sized so the grid's viewport is the reference's
    let (mut width, mut height) = (1100_i64, 700_i64);
    let mut fit = |viewport: (i64, i64)| {
        for _ in 0..6 {
            headless::render(&main_window, width as u32, height as u32);
            let got = (
                ui.get_grid_visible_width().round() as i64,
                ui.get_grid_visible_height().round() as i64,
            );
            if got == viewport {
                return;
            }
            width += viewport.0 - got.0;
            height += viewport.1 - got.1;
        }
        panic!("the grid can't be made {viewport:?}");
    };
    let click = |x: f32, y: f32| {
        // (a middle press elsewhere first, so clicks aren't counted as one
        // double-click)
        let away = slint::LogicalPosition::new(1.0, 1.0);
        main_window.dispatch_event(WindowEvent::PointerPressed {
            position: away,
            button: PointerEventButton::Middle,
        });
        main_window.dispatch_event(WindowEvent::PointerReleased {
            position: away,
            button: PointerEventButton::Middle,
        });
        let position =
            slint::LogicalPosition::new(ui.get_grid_origin_x() + x, ui.get_grid_origin_y() + y);
        main_window.dispatch_event(WindowEvent::PointerMoved { position });
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
            main_window.dispatch_event(event);
        }
    };
    let shown_sizes = || -> Vec<(u32, u32)> {
        (0..bound.rows.row_count())
            .flat_map(|r| {
                let row = bound.rows.row_data(r).unwrap().thumbnails;
                (0..row.row_count())
                    .map(|t| {
                        let size = row.row_data(t).unwrap().image.size();
                        (size.width, size.height)
                    })
                    .collect::<Vec<_>>()
            })
            .collect()
    };

    let mut checked_sizes = Vec::new();
    for config in recorded["configs"].as_array().unwrap() {
        let (w, h) = (int(&config["width"]), int(&config["height"]));
        let (border, margin) = (int(&config["border"]), int(&config["margin"]));
        let options = open_options(&ui, &bound);
        set(&options, "Thumbnail width: ", (20, 2048), w);
        set(&options, "Thumbnail height: ", (20, 2048), h);
        set(&options, "Thumbnail border: ", (0, 20), border);
        set(&options, "Thumbnail margin: ", (0, 20), margin);
        options.invoke_apply();
        let what = format!("{w}x{h}, border {border}, margin {margin}");

        // the layout at each of the reference's viewports
        for layout in config["layouts"].as_array().unwrap() {
            let viewport = (int(&layout["viewport"][0]), int(&layout["viewport"][1]));
            fit(viewport);
            let span = (
                (ui.get_thumbnail_width() + 2.0 * ui.get_thumbnail_margin()).round() as i64,
                (ui.get_thumbnail_height() + 2.0 * ui.get_thumbnail_margin()).round() as i64,
            );
            assert_eq!(
                span,
                (int(&layout["span"][0]), int(&layout["span"][1])),
                "{what} at {viewport:?}"
            );
            assert_eq!(
                i64::from(ui.get_grid_columns()),
                int(&layout["columns"]),
                "{what} at {viewport:?}"
            );
            // the first thumbnails: where they are and how big
            for (i, rect) in layout["first_thumbnails"]
                .as_array()
                .unwrap()
                .iter()
                .enumerate()
            {
                let columns = int(&layout["columns"]);
                let (row, column) = (i as i64 / columns, i as i64 % columns);
                let ours = (
                    column * span.0 + margin,
                    row * span.1 + margin,
                    ui.get_thumbnail_width().round() as i64,
                    ui.get_thumbnail_height().round() as i64,
                );
                let theirs = (int(&rect[0]), int(&rect[1]), int(&rect[2]), int(&rect[3]));
                assert_eq!(ours, theirs, "{what} at {viewport:?}, file {i}");
            }
            // every row, and the grid as tall as they are
            assert_eq!(
                ui.get_grid_content_height().round() as i64,
                int(&layout["scene"][1]),
                "{what} at {viewport:?}"
            );
        }

        // what a click at each point hits, at the viewport the reference
        // tried them at
        let at = &config["layouts"][4];
        fit((int(&at["viewport"][0]), int(&at["viewport"][1])));
        for hit in config["hits"].as_array().unwrap() {
            let (x, y) = (int(&hit[0]), int(&hit[1]));
            click(x as f32 + 0.5, y as f32 + 0.5);
            let selected: Vec<usize> = page.borrow().selected_indices().into_iter().collect();
            let expected: Vec<usize> = hit[2].as_u64().map(|i| i as usize).into_iter().collect();
            assert_eq!(selected, expected, "{what}: a click at ({x}, {y})");
        }

        // each file's thumbnail, at the size the reference's cache gives it
        if checked_sizes.contains(&(w, h)) {
            continue;
        }
        checked_sizes.push((w, h));
        let sizes = recorded["thumbnail_sizes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| int(&s["width"]) == w && int(&s["height"]) == h)
            .unwrap();
        shown_sizes();
        bound.rows.wait();
        let ours = shown_sizes();
        for (i, hash) in hashes.iter().enumerate() {
            let at = theirs.iter().position(|h| h == hash).unwrap();
            let size = &sizes["sizes"][at];
            assert_eq!(
                ours[i],
                (int(&size[0]) as u32, int(&size[1]) as u32),
                "{what}: {hash}"
            );
        }
    }
}
