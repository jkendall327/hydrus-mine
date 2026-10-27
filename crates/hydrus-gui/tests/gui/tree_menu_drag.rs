//! The pages tree's right-click menu and its drag and drop, replayed through
//! the real main window with real pointer events against the reference's
//! tree (`oracle/fixtures/tab_strip_interactions.json`'s "tree" steps, from
//! `oracle/record_tab_strip_interactions.py`): a right-click on a row shows
//! its notebook's tab menu for that page (none on empty space), whose
//! actions reach that notebook without showing it, and a row dragged and let
//! go above, on or below another row, or past the rows, moves its page
//! where the reference's tree model puts it.

use std::sync::Arc;

use serde_json::{Value as Json, json};
use slint::platform::{PointerEventButton, WindowEvent};
use slint::{ComponentHandle as _, LogicalPosition, Model as _};

use hydrus_store::settings::{self, TabAlignment, TabPresentationSettings};
use hydrus_store::{Store, sessions};

use crate::common::widgets;
use crate::tab_strip_clicks::{Strip, click, double_click, press, recorded_pages, release, settle};

/// The recording's pages, with the tree on the left and the tabs hidden
/// (so a page's name labels only its row).
fn open() -> ([tempfile::TempDir; 2], Arc<Store>, Strip) {
    let (dirs, store) = crate::subscriptions::store();
    recorded_pages(&store);
    store
        .write(|ctx| {
            settings::set(
                ctx.conn(),
                &TabPresentationSettings {
                    tree_alignment: Some(TabAlignment::Left),
                    hide_navigation_tabs: true,
                    ..TabPresentationSettings::default()
                },
            )
        })
        .unwrap();
    let strip = Strip::open(&store);
    (dirs, store, strip)
}

/// The tree's rows (from the top of the first), each 26 high, below the
/// tree's 28-high button bar.
fn row_top(strip: &Strip, name: &str) -> LogicalPosition {
    settle(&strip.native);
    let rows = strip.ui.get_page_tree();
    let index = rows
        .iter()
        .position(|row| row.name == name)
        .unwrap_or_else(|| panic!("no tree row {name:?}"));
    #[allow(clippy::cast_precision_loss)]
    let y = strip.ui.get_page_tree_y() + 28.0 + index as f32 * 26.0;
    LogicalPosition::new(strip.ui.get_page_tree_x() + 60.0, y)
}

fn row(strip: &Strip, name: &str) -> LogicalPosition {
    let top = row_top(strip, name);
    LogicalPosition::new(top.x, top.y + 13.0)
}

/// Empty space in the tree, below its rows.
fn below_rows(strip: &Strip) -> LogicalPosition {
    #[allow(clippy::cast_precision_loss)]
    let rows = strip.ui.get_page_tree().row_count() as f32;
    LogicalPosition::new(
        strip.ui.get_page_tree_x() + 60.0,
        strip.ui.get_page_tree_y() + 28.0 + rows * 26.0 + 40.0,
    )
}

/// Point at each submenu on `path` and click its last entry, as a user
/// does (the open menus are drawn over everything: the last match).
fn choose(strip: &Strip, path: &[&str]) {
    let window = strip.ui.window();
    for (i, label) in path.iter().enumerate() {
        let at = widgets::position(window, label, widgets::count(window, label) - 1);
        if i + 1 < path.len() {
            strip
                .native
                .dispatch_event(WindowEvent::PointerMoved { position: at });
        } else {
            click(&strip.native, at, PointerEventButton::Left);
        }
        settle(&strip.native);
    }
    assert_eq!(strip.ui.get_menu_panes().row_count(), 0, "chosen");
}

fn step<'a>(recorded: &'a Json, action: &str) -> &'a Json {
    recorded["tree"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["action"] == action)
        .unwrap_or_else(|| panic!("no recorded step {action:?}"))
}

/// The open menu (closed after), the pages and the page shown, compared
/// with the recorded step's.
fn compare(strip: &Strip, recorded: &Json, action: &str, keep_open: bool) {
    let step = step(recorded, action);
    settle(&strip.native);
    let menu = (strip.ui.get_menu_panes().row_count() > 0).then(|| strip.menu(0));
    assert_eq!(
        menu.as_ref(),
        step["menus"]
            .as_array()
            .unwrap()
            .first()
            .map(|m| &m["entries"]),
        "{action}: menu"
    );
    if !keep_open {
        strip.dismiss();
    }
    assert_eq!(
        strip.state(),
        json!({ "pages": step["pages"], "shown": step["shown"] }),
        "{action}"
    );
}

/// A right-click on a row: the button pressed and let go there.
fn right_click(strip: &Strip, at: LogicalPosition) {
    click(&strip.native, at, PointerEventButton::Right);
}

// leaf: interaction-pages-tree-menu-drag
#[test]
fn the_trees_right_click_menu_acts_on_a_rows_notebook_as_the_references_does() {
    let recorded = hydrus_testkit::fixture_json("tab_strip_interactions.json");
    let (_dirs, _store, strip) = open();
    compare(&strip, &recorded, "initial", false);
    for (action, name) in [
        ("right click tree row alpha", "alpha"),
        ("right click tree row nested", "nested"),
        ("right click tree row inner one", "inner one"),
    ] {
        right_click(&strip, row(&strip, name));
        compare(&strip, &recorded, action, false);
    }
    right_click(&strip, below_rows(&strip));
    compare(&strip, &recorded, "right click tree empty space", false);

    // alpha shown: the nested notebook is no longer on the way to it, and
    // its menu's actions act on it without showing it
    double_click(
        &strip.native,
        row(&strip, "alpha"),
        PointerEventButton::Left,
    );
    settle(&strip.native);
    assert_eq!(strip.state()["shown"], "alpha");
    let action = "right click tree row inner two, alpha shown";
    right_click(&strip, row(&strip, "inner two"));
    compare(&strip, &recorded, action, true);
    choose(&strip, &["select", "page to the right"]);
    compare(&strip, &recorded, "its select > page to the right", false);
    right_click(&strip, row(&strip, "inner two"));
    compare(
        &strip,
        &recorded,
        "right click tree row inner two again",
        true,
    );
    choose(&strip, &["move page", "left"]);
    compare(&strip, &recorded, "its move page > left", false);
    right_click(&strip, row(&strip, "inner one"));
    compare(
        &strip,
        &recorded,
        "right click tree row inner one, alpha shown",
        true,
    );
    choose(&strip, &["close page"]);
    compare(&strip, &recorded, "its close page", false);
    // (the notebook kept what its menu chose: shown, it shows inner two)
    double_click(
        &strip.native,
        row(&strip, "nested"),
        PointerEventButton::Left,
    );
    settle(&strip.native);
    assert_eq!(strip.state()["shown"], "inner two");
}

// leaf: interaction-pages-tree-menu-drag
#[test]
fn the_trees_rename_from_its_menu_renames_a_page_in_a_notebook_not_shown() {
    let (_dirs, store, strip) = open();
    double_click(
        &strip.native,
        row(&strip, "alpha"),
        PointerEventButton::Left,
    );
    right_click(&strip, row(&strip, "inner two"));
    choose(&strip, &["rename page"]);
    let asked = strip.rename(Some("renamed two"));
    assert_eq!(asked[0]["message"], "Enter the new name.");
    assert_eq!(asked[0]["default"], "inner two");
    assert_eq!(
        strip.state(),
        json!({
            "pages": [
                { "name": "alpha" },
                { "name": "nested", "children": [{ "name": "inner one" }, { "name": "renamed two" }] },
                { "name": "omega" },
            ],
            "shown": "alpha",
        })
    );
    // and it is kept
    strip.bound.pages.borrow_mut().sync(5).unwrap();
    let saved = store
        .read(|conn| sessions::load(conn, sessions::LAST_SESSION))
        .unwrap()
        .unwrap();
    assert_eq!(
        crate::tab_strip_clicks::tree(&saved.pages)[1]["children"][1]["name"],
        "renamed two"
    );
}

#[derive(Clone, Copy)]
enum Place {
    Above(&'static str),
    On(&'static str),
    Below(&'static str),
    Space,
}

/// Press on the row `source`, move past the drag distance, then to `place`
/// and let go there.
fn drag(strip: &Strip, source: &str, place: Place) {
    let start = row(strip, source);
    let to = match place {
        Place::Above(name) => {
            let top = row_top(strip, name);
            LogicalPosition::new(top.x, top.y + 1.0)
        }
        Place::On(name) => row(strip, name),
        Place::Below(name) => {
            let top = row_top(strip, name);
            LogicalPosition::new(top.x, top.y + 24.0)
        }
        Place::Space => below_rows(strip),
    };
    press(&strip.native, start, PointerEventButton::Left);
    for position in [
        LogicalPosition::new(start.x, start.y + 12.0),
        LogicalPosition::new(to.x, to.y + 1.0),
        to,
    ] {
        strip
            .native
            .dispatch_event(WindowEvent::PointerMoved { position });
    }
    release(&strip.native, to, PointerEventButton::Left);
    settle(&strip.native);
}

// leaf: interaction-pages-tree-menu-drag
#[test]
fn dragging_tree_rows_moves_pages_where_the_references_tree_drops_them() {
    let recorded = hydrus_testkit::fixture_json("tab_strip_interactions.json");
    let (_dirs, _store, strip) = open();
    for (action, source, place, indicator) in [
        (
            "drag omega above alpha",
            "omega",
            Place::Above("alpha"),
            "AboveItem",
        ),
        (
            "drag omega onto nested",
            "omega",
            Place::On("nested"),
            "OnItem",
        ),
        (
            "drag inner one below alpha",
            "inner one",
            Place::Below("alpha"),
            "BelowItem",
        ),
        (
            "drag alpha onto inner two (a page)",
            "alpha",
            Place::On("inner two"),
            "OnItem",
        ),
        (
            "drag inner two above inner one",
            "inner two",
            Place::Above("inner one"),
            "AboveItem",
        ),
        (
            "drag omega below the last row",
            "omega",
            Place::Below("inner one"),
            "BelowItem",
        ),
        (
            "drag inner one to empty space",
            "inner one",
            Place::Space,
            "OnViewport",
        ),
    ] {
        assert_eq!(step(&recorded, action)["indicator"], indicator, "{action}");
        drag(&strip, source, place);
        compare(&strip, &recorded, action, false);
    }
}

/// What Qt refuses: a row dropped on itself or on a row inside it
/// (`droppingOnItself`), a press that moves less than the drag distance,
/// and a release away from the rows.
#[test]
fn tree_drags_onto_themselves_short_or_away_from_the_rows_move_nothing() {
    let (_dirs, _store, strip) = open();
    let before = strip.state();
    drag(&strip, "nested", Place::On("inner two"));
    drag(&strip, "nested", Place::Below("nested"));
    assert_eq!(strip.state(), before);
    // a short move: no drag (the press and release a click on the row)
    let start = row(&strip, "omega");
    press(&strip.native, start, PointerEventButton::Left);
    let near = LogicalPosition::new(start.x, start.y + 4.0);
    strip
        .native
        .dispatch_event(WindowEvent::PointerMoved { position: near });
    release(&strip.native, near, PointerEventButton::Left);
    settle(&strip.native);
    assert_eq!(strip.state(), before);
    // let go over the page, right of the tree
    press(
        &strip.native,
        row(&strip, "omega"),
        PointerEventButton::Left,
    );
    let away = LogicalPosition::new(
        strip.ui.get_page_tree_x() + strip.ui.get_page_tree_width() + 200.0,
        start.y,
    );
    for position in [LogicalPosition::new(start.x, start.y + 12.0), away] {
        strip
            .native
            .dispatch_event(WindowEvent::PointerMoved { position });
    }
    release(&strip.native, away, PointerEventButton::Left);
    settle(&strip.native);
    assert_eq!(strip.state(), before);
}
