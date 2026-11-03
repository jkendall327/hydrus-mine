//! The media viewer top hover's zoom options (cog) menu and its move
//! button, as the reference's (`oracle/record_viewer_zoom_locks.py`: a real
//! `CanvasMediaListBrowser`, 1000x750, over four of the basic fixture's
//! files). The real viewer is opened on the same files in the same order;
//! the menus are opened and their entries chosen with real pointer events,
//! and the options they leave, the menu as it then shows, and the file's
//! zoom and place are compared with the recording; then the locks are seen
//! at work going to the next file and resizing the window, and each entry
//! of the resize menu, chosen with either button, sizes the window as the
//! reference's did.

use std::cell::RefCell;
use std::rc::Rc;

use hydrus_core::HashId;
use hydrus_core::media_viewer::{MediaViewerSettings, ZoomType};
use hydrus_store::settings::ViewerCanvasSettings;
use serde_json::Value;
use slint::ComponentHandle as _;
use slint::platform::{PointerEventButton, WindowEvent};
use slint::{LogicalPosition, Model as _};

use super::common::widgets;
use super::media_support::*;

fn recording() -> Value {
    hydrus_testkit::fixture_json("viewer_zoom_locks.json")
}

fn id_of(fixture: &Fixture, hash: &str) -> HashId {
    let hash: hydrus_core::Sha256 = hash.parse().unwrap();
    fixture
        .store
        .read(move |c| hydrus_store::master::hash_id(c, &hash))
        .unwrap()
        .unwrap()
}

/// The viewer on the recorded files, in the recorded order, 1000x750.
fn open(recorded: &Value) -> (Fixture, hydrus_gui::MediaViewerWindow) {
    let fixture = start();
    let hashes: Vec<&str> = recorded["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["hash"].as_str().unwrap())
        .collect();
    fixture
        .ui
        .invoke_search_edited(format!("system:hash = {}", hashes.join(" ")).into());
    fixture.ui.invoke_search_accepted();
    let order: Vec<HashId> = hashes.iter().map(|h| id_of(&fixture, h)).collect();
    assert_eq!(
        fixture.results(),
        order,
        "the files in the reference's order"
    );
    fixture.ui.invoke_thumbnail_activated(0);
    let viewer = fixture
        .bound
        .viewer
        .borrow()
        .as_ref()
        .map(slint::ComponentHandle::clone_strong)
        .expect("the viewer opens");
    viewer
        .window()
        .set_size(slint::LogicalSize::new(1000.0, 750.0));
    widgets::settle();
    (fixture, viewer)
}

/// The pointer near the top middle, where it raises the hover.
fn raise(viewer: &hydrus_gui::MediaViewerWindow) {
    viewer.window().dispatch_event(WindowEvent::PointerMoved {
        position: LogicalPosition::new(500.0, 4.0),
    });
    widgets::settle();
}

/// Where the hover's button labelled `label` is (in its row).
fn in_row(viewer: &hydrus_gui::MediaViewerWindow, label: &str) -> LogicalPosition {
    (0..widgets::count(viewer.window(), label))
        .map(|n| widgets::position(viewer.window(), label, n))
        .find(|at| at.y <= 40.0)
        .unwrap_or_else(|| panic!("no {label:?} in the top hover's row"))
}

/// Open the zoom options menu with a click on its button.
fn open_zoom_menu(viewer: &hydrus_gui::MediaViewerWindow) {
    raise(viewer);
    widgets::click_at(
        viewer.window(),
        in_row(viewer, "advanced zoom"),
        PointerEventButton::Left,
    );
    widgets::settle();
    assert!(widgets::popup_open(viewer.window()), "the zoom menu opens");
}

/// Choose the zoom options menu's entry `text`, as a user does.
fn choose(viewer: &hydrus_gui::MediaViewerWindow, text: &str) {
    open_zoom_menu(viewer);
    widgets::click(viewer.window(), text);
    widgets::settle();
    widgets::close_popups(viewer.window());
}

/// The zoom menu's rows as shown: (text, checked).
fn menu_rows(viewer: &hydrus_gui::MediaViewerWindow) -> Vec<(String, bool)> {
    let menu = viewer.get_top_zoom_menu();
    [&menu.g1, &menu.g2, &menu.g3, &menu.g4, &menu.g5]
        .into_iter()
        .flat_map(|g| {
            g.iter()
                .map(|r| (r.label.to_string(), r.checked))
                .collect::<Vec<_>>()
        })
        .collect()
}

fn recorded_rows(menu: &Value) -> Vec<(String, bool)> {
    menu.as_array()
        .unwrap()
        .iter()
        .filter(|i| i["separator"].is_null())
        .map(|i| {
            (
                i["text"].as_str().unwrap().to_owned(),
                i["checked"].as_bool().unwrap_or(false),
            )
        })
        .collect()
}

/// The recorded menu's separators split it as the native menu's groups.
fn recorded_groups(menu: &Value) -> Vec<usize> {
    let mut groups = vec![0];
    for item in menu.as_array().unwrap() {
        if item["separator"].is_null() {
            *groups.last_mut().unwrap() += 1;
        } else {
            groups.push(0);
        }
    }
    groups
}

fn shown_groups(viewer: &hydrus_gui::MediaViewerWindow) -> Vec<usize> {
    let menu = viewer.get_top_zoom_menu();
    [&menu.g1, &menu.g2, &menu.g3, &menu.g4, &menu.g5]
        .into_iter()
        .map(slint::Model::row_count)
        .filter(|n| *n > 0)
        .collect()
}

/// The options the menu changes, as the recording names them.
fn options(fixture: &Fixture) -> Value {
    let canvas: ViewerCanvasSettings = fixture
        .store
        .read(hydrus_store::settings::get)
        .unwrap_or_default();
    let viewer: MediaViewerSettings = fixture
        .store
        .read(hydrus_store::settings::get)
        .unwrap_or_default();
    serde_json::json!({
        "size": canvas.lock_size,
        "type": canvas.lock_zoom_type,
        "pan": canvas.lock_pan,
        "from_menu": canvas.default_zoom_type_from_menu,
        "recenter_on_resize": canvas.recenter_on_resize,
        "default_zoom_type": viewer.default_zoom_type as i64,
    })
}

/// The zoom type checked in the menu, as the reference's code for it.
fn zoom_type(viewer: &hydrus_gui::MediaViewerWindow) -> Option<i64> {
    let codes = [
        ZoomType::DefaultForFiletype,
        ZoomType::Full,
        ZoomType::Canvas,
        ZoomType::FillX,
        ZoomType::FillY,
        ZoomType::FillAuto,
    ];
    let types = viewer.get_top_zoom_menu().g2;
    (0..types.row_count())
        .find(|&i| types.row_data(i).unwrap().checked)
        .map(|i| codes[i] as i64)
}

/// The file shown, its zoom and its box, against a recorded state.
fn assert_state(
    fixture: &Fixture,
    viewer: &hydrus_gui::MediaViewerWindow,
    recorded: &Value,
    what: &str,
) {
    // (the zoom menu checks the zoom type it is asked for)
    viewer.invoke_top_zoom_menu_requested();
    // (the file shown, by its place in the list)
    let order: Vec<HashId> = fixture.results();
    let place = order
        .iter()
        .position(|f| *f == id_of(fixture, recorded["shown"].as_str().unwrap()))
        .unwrap();
    assert_eq!(
        viewer.get_caption(),
        format!("{}/{}", place + 1, order.len()),
        "{what}: the file shown"
    );
    assert_eq!(
        viewer.get_zoom_text(),
        recorded["zoom_text"].as_str().unwrap(),
        "{what}: the zoom"
    );
    assert_eq!(
        zoom_type(viewer),
        recorded["zoom_type"].as_i64(),
        "{what}: the zoom type"
    );
    let at = |v: &Value| (v[0].as_i64().unwrap(), v[1].as_i64().unwrap());
    assert_eq!(
        (viewer.get_media_x() as i64, viewer.get_media_y() as i64),
        at(&recorded["position"]),
        "{what}: where the file is"
    );
    assert_eq!(
        (
            viewer.get_media_width() as i64,
            viewer.get_media_height() as i64
        ),
        at(&recorded["size"]),
        "{what}: its size"
    );
}

fn set_locks(fixture: &Fixture, locks: &[&str], recenter_on_resize: bool) {
    let locks: Vec<String> = locks.iter().map(|l| (*l).to_owned()).collect();
    fixture
        .store
        .write(move |ctx| {
            let mut canvas: ViewerCanvasSettings = hydrus_store::settings::get(ctx.conn())?;
            canvas.lock_size = locks.iter().any(|l| l == "size");
            canvas.lock_zoom_type = locks.iter().any(|l| l == "type");
            canvas.lock_pan = locks.iter().any(|l| l == "pan");
            canvas.recenter_on_resize = recenter_on_resize;
            hydrus_store::settings::set(ctx.conn(), &canvas)
        })
        .unwrap();
}

fn lock_names(locks: &Value) -> Vec<&str> {
    locks
        .as_array()
        .unwrap()
        .iter()
        .map(|l| l.as_str().unwrap())
        .collect()
}

/// As the recorder works the first file: no locks, the second file then
/// the first (at its default zoom), filling the window's width, zoomed in
/// twice about the middle, and panned right and down.
fn worked(fixture: &Fixture, viewer: &hydrus_gui::MediaViewerWindow) {
    set_locks(fixture, &[], true);
    viewer.invoke_first();
    viewer.invoke_next();
    viewer.invoke_previous();
    widgets::settle();
    choose(viewer, "fill horizontally");
    viewer.invoke_zoom(1, false, 0.0, 0.0);
    viewer.invoke_zoom(1, false, 0.0, 0.0);
    viewer.invoke_pan(1, 0);
    viewer.invoke_pan(0, 1);
    widgets::settle();
}

// leaf: audit-media-viewer-top-hover-zoom-options
#[test]
fn the_zoom_options_menu_s_entries_do_what_the_reference_s_do() {
    let recorded = recording();
    let (fixture, viewer) = open(&recorded);
    for step in recorded["menu"].as_array().unwrap() {
        let what = match (step["chose"].as_str(), step["panned"].as_bool()) {
            (Some(text), _) => {
                choose(&viewer, text);
                format!("after {text:?}")
            }
            (None, Some(true)) => {
                viewer.invoke_pan(1, 0);
                widgets::settle();
                "panned".to_owned()
            }
            _ => "as it opens".to_owned(),
        };
        assert_state(&fixture, &viewer, &step["state"], &what);
        if !step["options"].is_null() {
            assert_eq!(options(&fixture), step["options"], "{what}: the options");
        }
        if !step["menu"].is_null() {
            open_zoom_menu(&viewer);
            assert_eq!(
                menu_rows(&viewer),
                recorded_rows(&step["menu"]),
                "{what}: the menu"
            );
            assert_eq!(
                shown_groups(&viewer),
                recorded_groups(&step["menu"]),
                "{what}: its separators"
            );
            // (each entry shown)
            for (text, _) in recorded_rows(&step["menu"]) {
                assert!(
                    widgets::shows(viewer.window(), &text),
                    "{what}: {text} shows"
                );
            }
            widgets::close_popups(viewer.window());
        }
    }
    // a right click on the cog opens nothing, as the reference's
    raise(&viewer);
    widgets::click_at(
        viewer.window(),
        in_row(&viewer, "advanced zoom"),
        PointerEventButton::Right,
    );
    widgets::settle();
    assert!(
        !widgets::popup_open(viewer.window()),
        "no menu on a right click"
    );
    // a lock chosen stays chosen in the next viewer opened
    choose(&viewer, "lock current pan");
    viewer.invoke_close_requested();
    widgets::settle();
    assert!(fixture.bound.viewer.borrow().is_none(), "the viewer closed");
    fixture.ui.invoke_thumbnail_activated(0);
    let viewer = fixture
        .bound
        .viewer
        .borrow()
        .as_ref()
        .map(slint::ComponentHandle::clone_strong)
        .expect("the viewer opens again");
    viewer
        .window()
        .set_size(slint::LogicalSize::new(1000.0, 750.0));
    widgets::settle();
    open_zoom_menu(&viewer);
    assert!(
        menu_rows(&viewer).contains(&("lock current pan".to_owned(), true)),
        "the pan lock checked after reopening"
    );
}

// leaf: audit-media-viewer-top-hover-zoom-options
#[test]
fn the_zoom_locks_keep_the_zoom_and_pan_going_to_the_next_file_as_the_reference_s_do() {
    let recorded = recording();
    let (fixture, viewer) = open(&recorded);
    for case in recorded["next"].as_array().unwrap() {
        let locks = lock_names(&case["locks"]);
        let what = format!("locked {locks:?}");
        worked(&fixture, &viewer);
        assert_state(
            &fixture,
            &viewer,
            &case["before"],
            &format!("{what}, before"),
        );
        set_locks(&fixture, &locks, true);
        viewer.invoke_next();
        widgets::settle();
        assert_state(
            &fixture,
            &viewer,
            &case["after"],
            &format!("{what}, the next file"),
        );
        viewer.invoke_next();
        widgets::settle();
        assert_state(
            &fixture,
            &viewer,
            &case["then"],
            &format!("{what}, the one after"),
        );
    }
}

// leaf: audit-media-viewer-top-hover-zoom-options
#[test]
fn the_zoom_locks_and_recentering_keep_the_zoom_as_the_window_resizes_as_the_reference_s_do() {
    let recorded = recording();
    let (fixture, viewer) = open(&recorded);
    for case in recorded["resize"].as_array().unwrap() {
        let locks = lock_names(&case["locks"]);
        let recenter = case["recenter_on_resize"].as_bool().unwrap();
        let what = format!("locked {locks:?}, recentering {recenter}");
        set_locks(&fixture, &[], true);
        viewer
            .window()
            .set_size(slint::LogicalSize::new(1000.0, 750.0));
        widgets::settle();
        worked(&fixture, &viewer);
        assert_state(
            &fixture,
            &viewer,
            &case["before"],
            &format!("{what}, before"),
        );
        set_locks(&fixture, &locks, recenter);
        viewer
            .window()
            .set_size(slint::LogicalSize::new(800.0, 600.0));
        widgets::settle();
        assert_state(
            &fixture,
            &viewer,
            &case["after"],
            &format!("{what}, resized"),
        );
    }
}

// leaf: audit-media-viewer-top-hover-move-button
#[test]
fn the_move_button_moves_the_window_and_its_right_click_resizes_it_as_the_reference_s_does() {
    let recorded = recording();
    let recorded = &recorded;
    let (fixture, viewer) = open(recorded);
    let moves: Rc<RefCell<Vec<hydrus_gui::WindowMove>>> = Rc::default();
    hydrus_gui::watch_window_moves({
        let moves = moves.clone();
        move |m| moves.borrow_mut().push(m)
    });
    let button = "click and drag from here to move the window";
    // a left press: the platform's own move, no menu
    raise(&viewer);
    widgets::click_at(
        viewer.window(),
        in_row(&viewer, button),
        PointerEventButton::Left,
    );
    widgets::settle();
    assert_eq!(
        *moves.borrow(),
        [hydrus_gui::WindowMove::System],
        "a system move"
    );
    assert!(
        !widgets::popup_open(viewer.window()),
        "no menu on a left press"
    );
    // a middle click: nothing, as the reference's
    moves.borrow_mut().clear();
    raise(&viewer);
    widgets::click_at(
        viewer.window(),
        in_row(&viewer, button),
        PointerEventButton::Middle,
    );
    widgets::settle();
    assert!(moves.borrow().is_empty(), "no move on a middle click");
    assert!(
        !widgets::popup_open(viewer.window()),
        "no menu on a middle click"
    );
    // a right click: the resize menu, its entries as the reference's
    let menu = &recorded["resize_menu"];
    let entries: Vec<&str> = menu["menu"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|i| i["text"].as_str())
        .collect();
    raise(&viewer);
    widgets::click_at(
        viewer.window(),
        in_row(&viewer, button),
        PointerEventButton::Right,
    );
    widgets::settle();
    assert!(
        widgets::popup_open(viewer.window()),
        "the resize menu opens"
    );
    let mut last_y = f32::MIN;
    for entry in &entries {
        let at = widgets::position(viewer.window(), entry, 0);
        assert!(at.y > last_y, "{entry} under the one before");
        last_y = at.y;
    }
    // (its note, chosen, is copied, as a menu label is)
    let copied: Rc<RefCell<Vec<hydrus_gui::Clip>>> = Rc::default();
    hydrus_gui::set_clipper({
        let copied = copied.clone();
        move |clip| copied.borrow_mut().push(clip.clone())
    });
    widgets::click(viewer.window(), entries.last().unwrap());
    widgets::settle();
    assert_eq!(
        *copied.borrow(),
        [hydrus_gui::Clip::Text(
            (*entries.last().unwrap()).to_owned()
        )],
        "the note copied"
    );
    assert!(!widgets::popup_open(viewer.window()), "the menu closes");
    // each entry, chosen with either button, from the first file worked
    for choice in menu["choices"].as_array().unwrap() {
        let text = choice["chose"].as_str().unwrap();
        let pointer = match choice["button"].as_str().unwrap() {
            "left" => PointerEventButton::Left,
            _ => PointerEventButton::Right,
        };
        let what = format!("{text} ({pointer:?})");
        viewer
            .window()
            .set_size(slint::LogicalSize::new(1000.0, 750.0));
        widgets::settle();
        worked(&fixture, &viewer);
        assert_state(
            &fixture,
            &viewer,
            &choice["before"]["state"],
            &format!("{what}, before"),
        );
        moves.borrow_mut().clear();
        raise(&viewer);
        widgets::click_at(
            viewer.window(),
            in_row(&viewer, button),
            PointerEventButton::Right,
        );
        widgets::settle();
        widgets::click_at(
            viewer.window(),
            widgets::position(viewer.window(), text, 0),
            pointer,
        );
        widgets::settle();
        assert!(
            !widgets::popup_open(viewer.window()),
            "{what}: the menu closes"
        );
        let geometry = |v: &Value| -> Vec<i64> {
            v.as_array()
                .unwrap()
                .iter()
                .map(|n| n.as_i64().unwrap())
                .collect()
        };
        let (before, after) = (
            geometry(&choice["before"]["geometry"]),
            geometry(&choice["geometry"]),
        );
        let size = viewer
            .window()
            .size()
            .to_logical(viewer.window().scale_factor());
        assert_eq!(
            [size.width as i64, size.height as i64],
            [after[2], after[3]],
            "{what}: the window's size"
        );
        assert_state(&fixture, &viewer, &choice["state"], &what);
        // moved to where the file was: by its place in the window (the
        // reference moves its frame, whose border's 2px it adds across;
        // "resize to fit" was rescued back on screen, which the headless
        // platform, with no screens, does not do)
        if text != "resize to fit" {
            let border = before[0] - 100;
            assert_eq!(
                *moves.borrow(),
                [hydrus_gui::WindowMove::To(
                    (after[0] - before[0] - border) as i32,
                    (after[1] - before[1]) as i32
                )],
                "{what}: moved"
            );
        }
    }
}
