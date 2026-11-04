//! The filters' top hover frames, as the reference's
//! (`oracle/record_viewer_top_hover.py`: a real
//! `CanvasMediaListFilterArchiveDelete` over four of the basic fixture's
//! files in the inbox, and a real `CanvasFilterDuplicates` over the
//! auto-resolution fixture's potential pairs). Each filter is opened on the
//! same files, its hover raised by the pointer, and its widgets found where
//! they are laid out: in the recorded order left to right, shown or not as
//! recorded, with the recorded tooltips; then each recorded click (left and
//! right) is made with real pointer events at the button's place, and what
//! followed is compared, including that the presses the reference ignores
//! open nothing and change nothing.

use std::cell::RefCell;
use std::rc::Rc;

use hydrus_core::HashId;
use serde_json::Value;
use slint::ComponentHandle as _;
use slint::platform::{PointerEventButton, WindowEvent};
use slint::{LogicalPosition, Model as _};

use super::common::widgets;

/// What the tests read of a filter window.
trait Filter {
    fn win(&self) -> &slint::Window;
    fn index(&self) -> String;
    fn zoom(&self) -> String;
    fn inbox(&self) -> bool;
    fn raised(&self) -> bool;
    fn volume_available(&self) -> bool;
    fn global_muted(&self) -> bool;
    fn zoom_menu(&self) -> hydrus_gui::MenuGroups;
    fn eye_menu(&self) -> hydrus_gui::ViewerEyeMenu;
    fn shortcut_names(&self) -> usize;
}

macro_rules! filter {
    ($type:ty, $index:ident) => {
        impl Filter for $type {
            fn win(&self) -> &slint::Window {
                self.window()
            }
            fn index(&self) -> String {
                self.$index().to_string()
            }
            fn zoom(&self) -> String {
                self.get_zoom_text().to_string()
            }
            fn inbox(&self) -> bool {
                self.get_file_inbox()
            }
            fn raised(&self) -> bool {
                self.get_info_showing()
            }
            fn volume_available(&self) -> bool {
                self.get_volume_available()
            }
            fn global_muted(&self) -> bool {
                self.get_global_muted()
            }
            fn zoom_menu(&self) -> hydrus_gui::MenuGroups {
                self.get_top_zoom_menu()
            }
            fn eye_menu(&self) -> hydrus_gui::ViewerEyeMenu {
                self.get_eye_menu()
            }
            fn shortcut_names(&self) -> usize {
                self.get_shortcut_names().row_count()
            }
        }
    };
}
filter!(hydrus_gui::ArchiveDeleteWindow, get_caption);
filter!(hydrus_gui::DuplicateFilterWindow, get_index_text);

/// The accessible label hydrus-rs gives the widget with this reference
/// icon, in a filter's hover (`duplicates`: the duplicate filter's).
fn label_of(icon: &str, duplicates: bool) -> &'static str {
    match (icon, duplicates) {
        ("position_first", true) => "go back a pair",
        ("position_previous", false) => "back",
        ("position_previous", true) => "previous",
        ("position_next", false) => "skip",
        ("position_next", true) => "next",
        ("position_last", true) => "show a different pair",
        ("archive", _) => "archive",
        ("to_inbox", _) => "return to inbox",
        ("delete", _) => "send to trash",
        ("trash_delete", _) => "delete completely",
        ("undelete", _) => "undelete",
        ("page_with_text", _) => "show detailed file metadata",
        ("zoom_in", _) => "zoom in",
        ("zoom_out", _) => "zoom out",
        ("zoom_switch", _) => "zoom switch",
        ("zoom_cog", _) => "advanced zoom",
        ("sound" | "mute", _) => "Global mute/unmute",
        ("keyboard", _) => "shortcuts",
        ("eye", _) => "view options",
        ("move", _) => "click and drag from here to move the window",
        ("fullscreen_switch", _) => "fullscreen switch",
        ("open_externally", _) => "open externally",
        ("drag", _) => "drag from here to export file",
        ("stop", _) => "close",
        (other, _) => panic!("no native widget for the reference's {other}"),
    }
}

const ROW: f32 = 40.0;

/// The pointer near the top middle, where it raises the hover.
fn raise(filter: &dyn Filter) {
    filter.win().dispatch_event(WindowEvent::PointerMoved {
        position: LogicalPosition::new(500.0, 4.0),
    });
    widgets::settle();
}

fn in_row(filter: &dyn Filter, label: &str) -> LogicalPosition {
    let window = filter.win();
    (0..widgets::count(window, label))
        .map(|n| widgets::position(window, label, n))
        .find(|at| at.y <= ROW)
        .unwrap_or_else(|| panic!("no {label:?} in the top hover's row"))
}

/// The hover's widgets are the recorded ones, in their order, shown or
/// not, with their tooltips; the index and zoom texts as recorded.
fn check_widgets(filter: &dyn Filter, recorded: &Value, duplicates: bool) {
    raise(filter);
    assert!(filter.raised(), "the hover is raised");
    let window = filter.win();
    let mut left = f32::MIN;
    for widget in recorded["widgets"].as_array().unwrap() {
        if widget["class"] == "VolumeControl" && !filter.volume_available() {
            continue;
        }
        if let Some(which) = widget["label"].as_str() {
            let shown = if which == "index" {
                filter.index()
            } else {
                filter.zoom()
            };
            assert_eq!(shown, widget["text"].as_str().unwrap(), "the {which} text");
            continue;
        }
        let label = label_of(widget["icon"].as_str().unwrap(), duplicates);
        if !widget["shown"].as_bool().unwrap() {
            assert!(
                (0..widgets::count(window, label))
                    .all(|n| widgets::position(window, label, n).y > ROW),
                "{label} is hidden"
            );
            continue;
        }
        let at = in_row(filter, label);
        assert!(
            at.x > left,
            "{label} ({at:?}) is right of the one before ({left})"
        );
        left = at.x;
        assert_eq!(
            widgets::description(window, label).as_deref(),
            widget["tooltip"].as_str(),
            "{label}'s tooltip"
        );
    }
}

/// A recorded menu's entries, as (text, checked), without separators,
/// submenus and the Linux warning label.
fn checks(items: &[Value]) -> Vec<(String, bool)> {
    items
        .iter()
        .filter(|i| i["separator"].is_null() && i["submenu"].is_null())
        .filter(|i| i["text"] != "MAY BE BUGGY/CRASHY ON LINUX")
        .map(|i| {
            (
                i["text"].as_str().unwrap().to_owned(),
                i["checked"].as_bool().unwrap_or(false),
            )
        })
        .collect()
}

fn group_rows(groups: &[&slint::ModelRc<hydrus_gui::MenuRow>]) -> Vec<(String, bool)> {
    groups
        .iter()
        .flat_map(|g| {
            g.iter()
                .map(|r| (r.label.to_string(), r.checked))
                .collect::<Vec<_>>()
        })
        .collect()
}

/// What a filter's presses reach outside its window.
struct Watching {
    launched: Rc<RefCell<Vec<String>>>,
    browsed: Rc<RefCell<Vec<String>>>,
    moves: Rc<RefCell<Vec<hydrus_gui::WindowMove>>>,
}

fn watch() -> Watching {
    let watching = Watching {
        launched: Rc::default(),
        browsed: Rc::default(),
        moves: Rc::default(),
    };
    hydrus_gui::set_launcher({
        let launched = watching.launched.clone();
        move |target| launched.borrow_mut().push(target.to_owned())
    });
    hydrus_gui::set_file_browser({
        let browsed = watching.browsed.clone();
        move |path| browsed.borrow_mut().push(path.to_owned())
    });
    hydrus_gui::watch_window_moves({
        let moves = watching.moves.clone();
        move |m| moves.borrow_mut().push(m)
    });
    watching
}

/// What the test does about a press beyond comparing the index, zoom and
/// archive state and the menus: the window's own consequences.
struct Press<'a> {
    icon: &'a str,
    button: &'a str,
    did: &'a [Value],
}

/// Make each recorded press with real pointer events and compare what
/// followed; `extra` checks what a press reaches outside the hover, and
/// returns whether the press was handled there (the rest is compared here).
fn replay_presses(
    filter: &dyn Filter,
    recorded: &Value,
    duplicates: bool,
    store: &hydrus_store::Store,
    watching: &Watching,
    extra: &mut dyn FnMut(&Press<'_>, &Value),
) {
    let global_mute = || {
        store
            .read(hydrus_store::settings::get::<hydrus_core::media_viewer::AudioSettings>)
            .unwrap_or_default()
            .global_mute
    };
    for press in recorded["presses"].as_array().unwrap() {
        let icon = press["icon"].as_str().unwrap();
        let button = press["button"].as_str().unwrap();
        let (before, after) = (&press["before"], &press["after"]);
        let what = format!("{icon} ({button})");
        assert_eq!(
            filter.index(),
            before["index"].as_str().unwrap(),
            "{what}: before"
        );
        assert_eq!(
            filter.zoom(),
            before["zoom"].as_str().unwrap(),
            "{what}: zoom before"
        );
        assert_eq!(
            filter.inbox(),
            before["inbox"].as_bool().unwrap(),
            "{what}: inbox before"
        );
        if (icon == "sound" || icon == "mute") && !filter.volume_available() {
            continue;
        }
        if icon == "stop" && button == "left" {
            // (closing is checked last, by the caller)
            continue;
        }
        raise(filter);
        assert!(filter.raised(), "{what}: the hover is raised");
        let label = match icon {
            "archive" | "to_inbox" if filter.inbox() || !duplicates => "archive",
            "archive" | "to_inbox" => "return to inbox",
            other => label_of(other, duplicates),
        };
        let at = in_row(filter, label);
        let mute_before = global_mute();
        let fullscreen_before = filter.win().is_fullscreen();
        let (launched, browsed, moves) = (
            watching.launched.borrow().len(),
            watching.browsed.borrow().len(),
            watching.moves.borrow().len(),
        );
        let pointer = if button == "left" {
            PointerEventButton::Left
        } else {
            PointerEventButton::Right
        };
        widgets::click_at(filter.win(), at, pointer);
        widgets::settle();
        let did = press["did"].as_array().unwrap();
        let has = |key: &str| did.iter().any(|d| !d[key].is_null());
        // (a delete the reference held is the filter's own: the caller's)
        if !did.iter().any(|d| d["pub"] == "canvas_delete") {
            assert_eq!(
                filter.index(),
                after["index"].as_str().unwrap(),
                "{what}: index after"
            );
            assert_eq!(
                filter.zoom(),
                after["zoom"].as_str().unwrap(),
                "{what}: zoom after"
            );
            assert_eq!(
                filter.inbox(),
                after["inbox"].as_bool().unwrap(),
                "{what}: inbox after"
            );
        }
        if let Some(muted) = press["global_mute"].as_bool() {
            assert_eq!(global_mute(), muted, "{what}: global mute");
            assert_eq!(filter.global_muted(), muted, "{what}: its icon");
        } else {
            assert_eq!(global_mute(), mute_before, "{what}: no mute change");
        }
        if let Some(menu) = did.iter().find_map(|d| d.get("menu")) {
            let items = menu.as_array().unwrap();
            match icon {
                "zoom_cog" => {
                    let m = filter.zoom_menu();
                    assert_eq!(
                        group_rows(&[&m.g1, &m.g2, &m.g3, &m.g4]),
                        checks(items),
                        "{what}: the zoom menu"
                    );
                }
                "keyboard" => {
                    assert_eq!(checks(items), [("edit shortcuts".to_owned(), false)]);
                    assert_eq!(filter.shortcut_names(), 0, "{what}: no custom sets");
                    assert!(widgets::shows(filter.win(), "edit shortcuts"), "{what}");
                }
                "eye" => {
                    let eye = filter.eye_menu();
                    let mut rows = Vec::new();
                    for group in [&eye.window, &eye.hovers, &eye.rendering] {
                        rows.extend(group_rows(&[&group.g1, &group.g2]));
                    }
                    // (every recorded entry, the Linux warning label among
                    // them, as an entry that is not checked)
                    let mut expected = Vec::new();
                    for submenu in items {
                        expected.extend(
                            submenu["submenu"]
                                .as_array()
                                .unwrap()
                                .iter()
                                .filter(|i| i["separator"].is_null())
                                .map(|i| {
                                    (
                                        i["text"].as_str().unwrap().to_owned(),
                                        i["checked"].as_bool().unwrap_or(false),
                                    )
                                }),
                        );
                    }
                    assert_eq!(rows, expected, "{what}: the view options menu");
                }
                "move" => {
                    for (text, _) in checks(items) {
                        assert!(widgets::shows(filter.win(), &text), "{what}: {text}");
                    }
                }
                other => panic!("{other}'s menu"),
            }
            assert!(widgets::popup_open(filter.win()), "{what}: a menu opened");
            widgets::close_popups(filter.win());
        } else {
            assert!(!widgets::popup_open(filter.win()), "{what}: no menu");
        }
        if icon == "move" && button == "left" {
            assert_eq!(
                watching.moves.borrow()[moves..],
                [hydrus_gui::WindowMove::System],
                "{what}: the window's system move"
            );
        } else {
            assert_eq!(watching.moves.borrow().len(), moves, "{what}: no move");
        }
        if did.iter().any(|d| d["pub"] == "canvas_fullscreen_switch") {
            assert_eq!(
                filter.win().is_fullscreen(),
                !fullscreen_before,
                "{what}: fullscreen"
            );
            raise(filter);
            widgets::click_at(
                filter.win(),
                in_row(filter, label),
                PointerEventButton::Left,
            );
            widgets::settle();
            assert_eq!(
                filter.win().is_fullscreen(),
                fullscreen_before,
                "{what}: and back"
            );
        } else {
            assert_eq!(
                filter.win().is_fullscreen(),
                fullscreen_before,
                "{what}: not fullscreen"
            );
        }
        assert_eq!(
            watching.launched.borrow().len(),
            launched + usize::from(has("open externally")),
            "{what}: opened externally"
        );
        assert_eq!(
            watching.browsed.borrow().len(),
            browsed + usize::from(has("drag")),
            "{what}: shown in the file browser"
        );
        extra(&Press { icon, button, did }, press);
    }
}

fn hash_of(name: &str) -> hydrus_core::Sha256 {
    let manifest = hydrus_testkit::fixture_json("legacy_db/basic.manifest.json");
    manifest["files"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["name"] == name)
        .unwrap()["hash"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap()
}

fn id_in(store: &hydrus_store::Store, hash: hydrus_core::Sha256) -> HashId {
    store
        .read(move |c| hydrus_store::master::hash_id(c, &hash))
        .unwrap()
        .unwrap()
}

// leaf: audit-media-archive-delete-top-hover
#[test]
fn the_archive_delete_filter_s_top_hover_is_the_reference_s() {
    let recorded = hydrus_testkit::fixture_json("viewer_top_hover.json");
    let recorded = &recorded["archive_delete"];
    let watching = watch();
    let fixture = super::media_support::start();
    let store = fixture.store.clone();
    let names = ["jpeg_04.jpg", "jpeg_05.jpg", "jpeg_06.jpg", "jpeg_07.jpg"];
    let ids: Vec<HashId> = names.iter().map(|n| id_in(&store, hash_of(n))).collect();
    store
        .write_content({
            let ids = ids.clone();
            move |w| w.inbox(&ids)
        })
        .unwrap();
    let hexes: Vec<String> = names.iter().map(|n| hash_of(n).to_hex()).collect();
    fixture
        .ui
        .invoke_search_edited(format!("system:hash = {}", hexes.join(" ")).into());
    fixture.ui.invoke_search_accepted();
    // (in the reference's order: as imported)
    {
        let page = fixture.bound.current.borrow().clone();
        let mut page = page.borrow_mut();
        page.set_sort_by(hydrus_search::SortBy::ImportTime);
        page.set_sort_order(hydrus_search::SortOrder::Ascending);
    }
    widgets::settle();
    let order: Vec<HashId> = recorded["order"]
        .as_array()
        .unwrap()
        .iter()
        .map(|h| id_in(&store, h.as_str().unwrap().parse().unwrap()))
        .collect();
    assert_eq!(fixture.results(), order, "the reference's order");
    fixture.ui.invoke_archive_delete_filter();
    let window = fixture
        .bound
        .archive_delete
        .borrow()
        .as_ref()
        .map(slint::ComponentHandle::clone_strong)
        .expect("the filter opens");
    window
        .window()
        .set_size(slint::LogicalSize::new(1000.0, 750.0));
    widgets::settle();
    check_widgets(&window, recorded, false);
    // (for review: the hover raised)
    let shot = fixture.windows_.get(fixture.windows_.count() - 1).unwrap();
    let pixels = hydrus_gui::headless::render_snapshot(&shot, 1000, 750);
    hydrus_gui::headless::save_png(
        &std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("archive_delete_top_hover.png"),
        &pixels,
        1000,
        750,
    )
    .unwrap();
    let path_of = |hash: &str| {
        let id = id_in(&store, hash.parse().unwrap());
        hydrus_gui::thumbnail_menu::paths(&store, &[id])
            .pop()
            .unwrap()
    };
    replay_presses(
        &window,
        recorded,
        false,
        &store,
        &watching,
        &mut |press, recorded| {
            let what = format!("{} ({})", press.icon, press.button);
            let shown = recorded["before"]["shown"].as_str().unwrap();
            // the trash button deletes, as the delete key does: the next file
            // (the reference's message was held), then back to this one
            if press.did.iter().any(|d| d["pub"] == "canvas_delete") {
                assert_eq!(
                    window.get_caption(),
                    "4/4",
                    "{what}: deleted, the next file"
                );
                window.invoke_back();
                widgets::settle();
            }
            if press.did.iter().any(|d| !d["dialog"].is_null()) {
                assert!(
                    fixture.bound.embedded_metadata.borrow().is_some(),
                    "{what}: the embedded metadata window"
                );
                if let Some(w) = fixture.bound.embedded_metadata.borrow_mut().take() {
                    let _ = w.hide();
                }
            }
            if press.did.iter().any(|d| !d["open externally"].is_null()) {
                assert_eq!(
                    watching.launched.borrow().last(),
                    Some(&path_of(shown)),
                    "{what}"
                );
            }
            if press.did.iter().any(|d| !d["drag"].is_null()) {
                assert_eq!(
                    watching.browsed.borrow().last(),
                    Some(&path_of(shown)),
                    "{what}"
                );
            }
            if press
                .did
                .iter()
                .any(|d| d["command"] == "SIMPLE_FOCUS_TAB_AND_MEDIA")
            {
                assert_eq!(
                    fixture.selected(),
                    [id_in(&store, shown.parse().unwrap())],
                    "{what}: the file selected in its page"
                );
            }
        },
    );
    // close, last: with files decided, it asks, as the reference's
    raise(&window);
    widgets::click_at(
        window.window(),
        in_row(&window, "close"),
        PointerEventButton::Left,
    );
    widgets::settle();
    assert!(!window.get_question().is_empty(), "closing asks to commit");
}

/// The duplicate filter open on the auto-resolution fixture's pairs, in
/// the batch the reference made (six pairs, the largest files first).
fn open_duplicates(o: &crate::auto_resolution_review::Opened) -> hydrus_gui::DuplicateFilterWindow {
    o.store
        .write(|ctx| {
            let mut settings: hydrus_store::duplicates::DuplicateFilterSettings =
                hydrus_store::settings::get(ctx.conn())?;
            settings.max_batch_size = 6;
            hydrus_store::settings::set(ctx.conn(), &settings)
        })
        .unwrap();
    o.ui.invoke_launch_filter();
    let window = o
        .bound
        .filter
        .borrow()
        .as_ref()
        .expect("the filter opens")
        .clone_strong();
    window
        .window()
        .set_size(slint::LogicalSize::new(1000.0, 750.0));
    widgets::settle();
    window
}

// leaf: audit-media-duplicate-filter-top-hover
#[test]
fn the_duplicate_filter_s_top_hover_is_the_reference_s() {
    let recorded = hydrus_testkit::fixture_json("viewer_top_hover.json");
    let recorded = &recorded["duplicates"];
    let watching = watch();
    let _windows = hydrus_gui::headless::init();
    let o = crate::auto_resolution_review::opened();
    let window = open_duplicates(&o);
    let shown = hydrus_gui::shown_pair().map(|(a, _)| o.hex[&a].clone());
    assert_eq!(
        shown.as_deref(),
        recorded["now"]["shown"].as_str(),
        "the reference's first file"
    );
    check_widgets(&window, recorded, true);
    replay_presses(
        &window,
        recorded,
        true,
        &o.store,
        &watching,
        &mut |press, recorded| {
            let what = format!("{} ({})", press.icon, press.button);
            let shown = recorded["before"]["shown"].as_str().unwrap();
            // the trash button asks, as the reference's `_Delete` (whose
            // answers duplicate_filter_delete.rs replays): forgotten here
            if press.button == "left" && press.did.iter().any(|d| d["pub"] == "canvas_delete") {
                assert_eq!(
                    window.get_question(),
                    "Delete just this file, or both?",
                    "{what}"
                );
                widgets::click(window.window(), "forget it");
                widgets::settle();
                assert!(window.get_question().is_empty(), "{what}: forgotten");
            }
            assert_eq!(
                hydrus_gui::shown_pair()
                    .map(|(a, _)| o.hex[&a].clone())
                    .as_deref(),
                recorded["after"]["shown"].as_str(),
                "{what}: the file shown after"
            );
            if press.did.iter().any(|d| !d["dialog"].is_null()) {
                assert!(
                    o.bound.embedded_metadata.borrow().is_some(),
                    "{what}: the embedded metadata window"
                );
                if let Some(w) = o.bound.embedded_metadata.borrow_mut().take() {
                    let _ = w.hide();
                }
            }
            if press.did.iter().any(|d| !d["open externally"].is_null()) {
                let id = o.ids[shown];
                assert_eq!(
                    watching.launched.borrow().last(),
                    hydrus_gui::thumbnail_menu::paths(&o.store, &[id]).last(),
                    "{what}"
                );
            }
        },
    );
    raise(&window);
    widgets::click_at(
        window.window(),
        in_row(&window, "close"),
        PointerEventButton::Left,
    );
    widgets::settle();
    assert!(o.bound.filter.borrow().is_none(), "the filter closed");
}

// leaf: audit-media-archive-delete-top-hover
#[test]
fn the_archive_delete_filter_s_undelete_button_undeletes_and_its_frame_stays_in_a_narrow_window() {
    let fixture = super::media_support::start_local();
    let store = fixture.store.clone();
    let id = id_in(&store, hash_of("jpeg_04.jpg"));
    // (deleted from my files, still in the second local domain: the filter
    // takes it, and its undelete button shows)
    store
        .write_content(move |w| {
            let art = w.snapshot().services.by_name("art").unwrap().id;
            let my_files = w
                .snapshot()
                .services
                .builtin(hydrus_core::service::builtin_keys::MY_FILES)
                .unwrap()
                .id;
            w.add_files(art, &[(id, Some(1_234_567_890_000))])?;
            w.delete_files(my_files, &[id], None)
        })
        .unwrap();
    fixture
        .ui
        .invoke_search_edited(format!("system:hash = {}", hash_of("jpeg_04.jpg").to_hex()).into());
    fixture.ui.invoke_search_accepted();
    fixture.ui.invoke_archive_delete_filter();
    let window = fixture
        .bound
        .archive_delete
        .borrow()
        .as_ref()
        .map(slint::ComponentHandle::clone_strong)
        .expect("the filter opens");
    window
        .window()
        .set_size(slint::LogicalSize::new(1000.0, 750.0));
    widgets::settle();
    raise(&window);
    assert!(window.get_file_undeletable(), "undeletable");
    widgets::click_at(
        window.window(),
        in_row(&window, "undelete"),
        PointerEventButton::Left,
    );
    widgets::settle();
    // (asked, as the reference's `UndeleteMedia` asks, while trashing
    // asks to confirm)
    if let Some(chooser) = hydrus_gui::undelete::last_chooser() {
        widgets::lay_out(chooser.window(), 400.0, 200.0);
        widgets::settle();
        widgets::click(chooser.window(), "yes");
        widgets::settle();
    }
    let my_files = store
        .snapshot()
        .services
        .builtin(hydrus_core::service::builtin_keys::MY_FILES)
        .unwrap()
        .id;
    let in_my_files = store
        .read(move |c| {
            Ok(c.query_row(
                "SELECT COUNT(*) FROM file_domain_current WHERE service_id = ?1 AND hash_id = ?2",
                (my_files, id),
                |r| r.get::<_, i64>(0),
            )?)
        })
        .unwrap();
    assert_eq!(in_my_files, 1, "back in my files");
    raise(&window);
    assert!(!window.get_file_undeletable(), "its button gone");
    // a narrow window: the frame, as wide as its row needs, stays inside it
    window
        .window()
        .set_size(slint::LogicalSize::new(420.0, 400.0));
    widgets::settle();
    window.window().dispatch_event(WindowEvent::PointerMoved {
        position: LogicalPosition::new(210.0, 4.0),
    });
    widgets::settle();
    let (x, _, width, _) = widgets::bounds(window.window(), "close").expect("close shows");
    assert!(
        x + width <= 420.0,
        "close ({x} + {width}) inside the window"
    );
    let (x, _, _, _) = widgets::bounds(window.window(), "back").expect("back shows");
    assert!(x >= 0.0, "back ({x}) inside the window");
}

// (the duplicate filter's resize policy: recentering on resize, as the
// reference's duplicate canvas honours it)
#[test]
fn the_duplicate_filter_keeps_its_zoom_as_it_resizes_unless_recentering() {
    let _windows = hydrus_gui::headless::init();
    let o = crate::auto_resolution_review::opened();
    let window = open_duplicates(&o);
    let rect = |w: &hydrus_gui::DuplicateFilterWindow| {
        (
            w.get_media_x(),
            w.get_media_y(),
            w.get_media_width(),
            w.get_media_height(),
        )
    };
    for recenter in [false, true] {
        o.store
            .write(move |ctx| {
                let mut canvas: hydrus_store::settings::ViewerCanvasSettings =
                    hydrus_store::settings::get(ctx.conn())?;
                canvas.recenter_on_resize = recenter;
                hydrus_store::settings::set(ctx.conn(), &canvas)
            })
            .unwrap();
        window
            .window()
            .set_size(slint::LogicalSize::new(1000.0, 750.0));
        widgets::settle();
        window.invoke_zoom(1, false, 0.0, 0.0);
        window.invoke_pan(1, 0);
        widgets::settle();
        let before = rect(&window);
        window
            .window()
            .set_size(slint::LogicalSize::new(900.0, 700.0));
        widgets::settle();
        if recenter {
            assert_ne!(rect(&window), before, "recentred at its default zoom");
        } else {
            assert_eq!(rect(&window), before, "kept where it was");
        }
    }
}
