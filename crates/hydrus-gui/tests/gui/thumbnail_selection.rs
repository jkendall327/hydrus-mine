//! Selecting several thumbnails in the window (ctrl and shift clicks,
//! ctrl+a, escape, the arrows) and acting on them all: their tags listed,
//! F3, F7 and shift+F7 (asking first), delete and F12.

use std::sync::Arc;

use hydrus_core::HashId;
use hydrus_gui::{MainWindow, Pages, SearchPage, bind, headless};
use hydrus_store::Store;
use hydrus_store::import::import_legacy;
use slint::Model as _;
use slint::platform::{Key, PointerEventButton, WindowEvent};

fn inbox(store: &Store, files: &[HashId]) -> Vec<bool> {
    let snapshot = store.snapshot();
    store
        .read(|c| hydrus_store::media::load(c, &snapshot.services, None, files))
        .unwrap()
        .results
        .iter()
        .map(|m| m.inbox)
        .collect()
}

// leaf: audit-options-files-and-trash-confirm-sending-more-than-one-file-to-archive-or-inbox
#[test]
fn several_thumbnails_are_selected_and_acted_on() {
    let legacy = hydrus_testkit::legacy_fixture("basic");
    let native = tempfile::tempdir().unwrap();
    import_legacy(
        legacy.path(),
        &native.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store: Arc<Store> = Store::open(native.path()).unwrap();
    super::common::remove_trashed_from_view(&store);
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    slint::ComponentHandle::show(&ui).unwrap();
    let bound = bind(
        &ui,
        Pages::single(super::common::all_local_page(store.clone())),
    );
    ui.invoke_search_edited("system:inbox".into());
    ui.invoke_search_accepted();
    let page = bound.current.borrow().clone();
    let files = page.borrow().results().to_vec();
    assert!(files.len() > 12);
    let main_window = windows.get(0).unwrap();
    headless::render(&main_window, 1100, 700);
    // Keep the original grid viewport, independently of the sidebar width.
    let width = ui.get_grid_origin_x().round() as u32 + 800;
    headless::render(&main_window, width, 700);
    let columns = usize::try_from(ui.get_grid_columns()).unwrap();
    assert_eq!(columns, 5);

    let type_text = |text: slint::SharedString| {
        main_window.dispatch_event(WindowEvent::KeyPressed { text: text.clone() });
        main_window.dispatch_event(WindowEvent::KeyReleased { text });
    };
    let key = |key: Key| type_text(key.into());
    let with = |modifier: Option<Key>, then: &dyn Fn()| {
        let text: Option<slint::SharedString> = modifier.map(Into::into);
        if let Some(text) = &text {
            main_window.dispatch_event(WindowEvent::KeyPressed { text: text.clone() });
        }
        then();
        if let Some(text) = text {
            main_window.dispatch_event(WindowEvent::KeyReleased { text });
        }
    };
    // a click on the file at `index` (in the rows in view)
    let click = |index: usize, modifier: Option<Key>| {
        let (row, column) = (index / columns, index % columns);
        let position = slint::LogicalPosition::new(
            ui.get_grid_origin_x() + 4.0 + 156.0 * column as f32 + 76.0,
            ui.get_grid_origin_y() + 4.0 + 131.0 * row as f32 + 63.0,
        );
        with(modifier, &|| {
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
        });
    };
    let selected = || {
        page.borrow()
            .selected_indices()
            .into_iter()
            .collect::<Vec<_>>()
    };
    // the grid shows which are selected
    let drawn = || {
        let rows = ui.get_thumbnail_rows();
        (0..rows.row_count())
            .flat_map(|r| {
                let row = rows.row_data(r).unwrap();
                let first = usize::try_from(row.first).unwrap();
                (0..row.thumbnails.row_count())
                    .filter(move |&i| row.thumbnails.row_data(i).unwrap().selected)
                    .map(move |i| first + i)
            })
            .collect::<Vec<_>>()
    };

    // a click, a ctrl+click, a shift+click from there
    click(0, None);
    assert_eq!(selected(), [0]);
    let one_file_tags = ui.get_tags().row_count();
    click(2, Some(Key::Control));
    assert_eq!(selected(), [0, 2]);
    click(4, Some(Key::Shift));
    assert_eq!(selected(), [0, 2, 3, 4]);
    assert_eq!(drawn(), [0, 2, 3, 4]);
    // the tags listed are the selection's, counted
    assert!(ui.get_tags().row_count() >= one_file_tags);
    let pixels = headless::render(&main_window, width, 700);
    let shots = std::path::Path::new(env!("CARGO_TARGET_TMPDIR"));
    headless::save_png(&shots.join("thumbnail_selection.png"), &pixels, width, 700).unwrap();
    // ctrl+a selects all, escape none; the arrows move (shift selecting)
    with(Some(Key::Control), &|| type_text("a".into()));
    assert_eq!(selected().len(), files.len());
    key(Key::Escape);
    assert!(selected().is_empty());
    assert!(drawn().is_empty());
    click(1, None);
    key(Key::DownArrow);
    assert_eq!(selected(), [1 + columns]);
    with(Some(Key::Shift), &|| key(Key::RightArrow));
    with(Some(Key::Shift), &|| key(Key::RightArrow));
    assert_eq!(selected(), [6, 7, 8]);
    assert_eq!(page.borrow().focused(), Some(6));
    // a click on a gap between thumbnails selects nothing
    let gap =
        slint::LogicalPosition::new(ui.get_grid_origin_x() + 2.0, ui.get_grid_origin_y() + 80.0);
    for event in [
        WindowEvent::PointerPressed {
            position: gap,
            button: PointerEventButton::Left,
        },
        WindowEvent::PointerReleased {
            position: gap,
            button: PointerEventButton::Left,
        },
    ] {
        main_window.dispatch_event(event);
    }
    assert!(selected().is_empty());

    // F3 manages every selected file's tags
    click(0, None);
    click(2, Some(Key::Shift));
    key(Key::F3);
    let manage = bound
        .manage_tags
        .borrow()
        .as_ref()
        .map(slint::ComponentHandle::clone_strong)
        .expect("manage tags opened");
    assert_eq!(manage.get_window_title(), "manage tags for 3 files");
    manage.invoke_cancel();
    // F7 asks before archiving several; escape leaves them be, enter
    // archives them all
    let chosen = files[..3].to_vec();
    key(Key::F7);
    assert_eq!(ui.get_question(), "Archive 3 files?");
    key(Key::Escape);
    assert_eq!(inbox(&store, &chosen), [true, true, true]);
    let status = ui.get_status().to_string();
    assert!(
        status.contains(" selected, all in inbox, totalling "),
        "{status}"
    );
    key(Key::F7);
    key(Key::Return);
    assert_eq!(ui.get_question(), "");
    assert_eq!(inbox(&store, &chosen), [false, false, false]);
    // (the status bar follows)
    let status = ui.get_status().to_string();
    assert!(
        status.contains(" selected, all archived, totalling "),
        "{status}"
    );
    // shift+F7 asks too; one file isn't asked about
    with(Some(Key::Shift), &|| key(Key::F7));
    assert_eq!(ui.get_question(), "Send 3 files to inbox?");
    key(Key::Return);
    assert_eq!(inbox(&store, &chosen), [true, true, true]);
    click(1, Some(Key::Control));
    click(2, Some(Key::Control));
    key(Key::F7);
    assert_eq!(ui.get_question(), "");
    assert_eq!(inbox(&store, &chosen), [false, true, true]);
    // delete asks about them all, and they leave the page
    click(1, Some(Key::Control));
    click(2, Some(Key::Control));
    assert_eq!(selected(), [0, 1, 2]);
    key(Key::Delete);
    assert_eq!(ui.get_question(), "Send these 3 files to the trash?");
    key(Key::Return);
    let after = page.borrow().results().to_vec();
    assert_eq!(after.len(), files.len() - 3);
    assert!(chosen.iter().all(|f| !after.contains(f)));
    assert!(selected().is_empty());
    // F12 filters the files selected
    click(0, None);
    with(Some(Key::Shift), &|| key(Key::End));
    assert_eq!(selected().len(), after.len());
    click(3, Some(Key::Control));
    key(Key::F12);
    let filter = bound
        .archive_delete
        .borrow()
        .as_ref()
        .map(slint::ComponentHandle::clone_strong)
        .expect("the filter opened");
    assert_eq!(filter.get_caption(), format!("1/{}", after.len() - 1));
}

/// The grid as the reference lays it out from the thumbnail border and
/// margin options: a cell is the bounding box and its border, with the
/// margin all round it, so neighbours are two margins apart; a click in a
/// margin is on no file (the reference's `_GetThumbnailUnderMouse`, its
/// last pixel before a thumbnail counting as the margin).
#[test]
#[allow(clippy::float_cmp)] // (sizes set, not computed)
fn borders_and_margins_lay_the_grid_out() {
    let legacy = hydrus_testkit::legacy_fixture("basic");
    let native = tempfile::tempdir().unwrap();
    import_legacy(
        legacy.path(),
        &native.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store: Arc<Store> = Store::open(native.path()).unwrap();
    let layout = hydrus_store::settings::ThumbnailLayout {
        border: 3,
        margin: 10,
    };
    store
        .write(move |ctx| hydrus_store::settings::set(ctx.conn(), &layout))
        .unwrap();
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    slint::ComponentHandle::show(&ui).unwrap();
    let bound = bind(&ui, Pages::single(SearchPage::new(store.clone())));
    ui.invoke_search_edited("system:inbox".into());
    ui.invoke_search_accepted();
    let page = bound.current.borrow().clone();
    let main_window = windows.get(0).unwrap();
    headless::render(&main_window, 1100, 700);
    // Preserve four custom cells in the original 800px grid viewport.
    let width = ui.get_grid_origin_x().round() as u32 + 800;
    headless::render(&main_window, width, 700);
    let grid_left = ui.get_grid_origin_x().round() as i32;
    let grid_top = ui.get_grid_origin_y().round() as i32;
    // (the default box, 150 by 125, and its border)
    let (cell_width, cell_height) = (150 + 2 * 3, 125 + 2 * 3);
    assert_eq!(ui.get_thumbnail_width(), cell_width as f32);
    assert_eq!(ui.get_thumbnail_height(), cell_height as f32);

    // which file a click at (x, y) selects, if any
    let at = |x: f32, y: f32| {
        let position = slint::LogicalPosition::new(x, y);
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
        let selected = page.borrow().selected_indices();
        assert!(selected.len() <= 1);
        selected.into_iter().next()
    };
    // the runs of pixels on one file (or none) along a line
    let runs = |along: &dyn Fn(f32) -> Option<usize>, from: i32, to: i32| {
        let mut runs: Vec<(Option<usize>, i32)> = Vec::new();
        for p in from..to {
            let file = along(p as f32);
            match runs.last_mut() {
                Some((last, length)) if *last == file => *length += 1,
                _ => runs.push((file, 1)),
            }
        }
        runs
    };
    // Across the first row, relative to the measured grid origin.
    let scan_y = grid_top as f32 + cell_height as f32 / 2.0;
    let across = runs(&|x| at(x, scan_y), grid_left, width as i32);
    let columns = usize::try_from(ui.get_grid_columns()).unwrap();
    // (the window's 800 or so across fit four spans of 176, not five)
    assert_eq!(columns, 4);
    let files: Vec<(Option<usize>, i32)> = across
        .iter()
        .copied()
        .skip_while(|(file, _)| file.is_none())
        .collect();
    assert_eq!(
        &files[..2 * columns - 1],
        &[
            (Some(0), cell_width),
            (None, 2 * 10),
            (Some(1), cell_width),
            (None, 2 * 10),
            (Some(2), cell_width),
            (None, 2 * 10),
            (Some(3), cell_width),
        ],
        "{across:?}"
    );
    // the first file's left edge: its margin, then the file from the
    // margin's next pixel
    let (none, before) = across[0];
    assert_eq!(none, None);
    assert_eq!(before, 10 + 1, "{across:?}");
    // and down the first column
    let middle = grid_left as f32 + 10.0 + cell_width as f32 / 2.0;
    let down = runs(&|y| at(middle, y), grid_top, 700);
    let files: Vec<(Option<usize>, i32)> = down
        .iter()
        .copied()
        .skip_while(|(file, _)| file.is_none())
        .collect();
    assert_eq!(
        &files[..3],
        &[
            (Some(0), cell_height),
            (None, 2 * 10),
            (Some(columns), cell_height)
        ],
        "{down:?}"
    );

    // drawn so: each cell's border 3 pixels wide, from its margin on
    // (nothing selected, so every border is the same)
    assert_eq!(at(grid_left as f32 + 5.0, scan_y), None);
    let pixels = headless::render(&main_window, width, 700);
    let pixel = |x: usize| &pixels[(scan_y as usize * width as usize + x) * 4..][..4];
    let border = pixel(grid_left as usize + 10);
    for column in 0..columns {
        let left = grid_left as usize + 176 * column + 10;
        assert_ne!(pixel(left - 1), border, "column {column}'s margin");
        for x in left..left + 3 {
            assert_eq!(pixel(x), border, "column {column}'s border at {x}");
        }
        assert_ne!(pixel(left + 3), border, "column {column}'s inside");
        let right = left + usize::try_from(cell_width).unwrap() - 1;
        assert_eq!(pixel(right), border, "column {column}'s right");
    }
}
