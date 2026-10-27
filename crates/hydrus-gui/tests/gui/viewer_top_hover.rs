//! The media viewer's top hover frame, as the reference's
//! (`oracle/record_viewer_top_hover.py`: a real `CanvasMediaListBrowser`
//! and its `CanvasHoverFrameTopNavigableList` over the basic fixture's files
//! in the inbox, archived, with a note, with sound and in the trash). The
//! real viewer is opened on the same files in the same order, its hover
//! raised by the pointer, and for each file its widgets are found where
//! they are laid out: in the recorded order left to right, shown or not as
//! recorded, with the recorded tooltips; then each recorded click (left
//! and right) is made with real pointer events at the button's place, and
//! what followed is compared.

use std::cell::RefCell;
use std::rc::Rc;

use hydrus_core::HashId;
use serde_json::Value;
use slint::ComponentHandle as _;
use slint::platform::{PointerEventButton, WindowEvent};
use slint::{LogicalPosition, Model as _};

use super::common::widgets;
use super::media_support::*;

/// The accessible label hydrus-rs gives the widget with this reference icon.
fn label_of(icon: &str) -> &'static str {
    match icon {
        "position_first" => "first",
        "position_previous" => "previous",
        "position_next" => "next",
        "position_last" => "last",
        "position_random" => "random",
        "slideshow" => "slideshow",
        "archive" => "archive",
        "to_inbox" => "return to inbox",
        "delete" => "send to trash",
        "trash_delete" => "delete completely",
        "undelete" => "undelete",
        "page_with_text" => "show detailed file metadata",
        "zoom_in" => "zoom in",
        "zoom_out" => "zoom out",
        "zoom_switch" => "zoom switch",
        "zoom_cog" => "advanced zoom",
        "sound" | "mute" => "Global mute/unmute",
        "keyboard" => "shortcuts",
        "eye" => "view options",
        "move" => "click and drag from here to move the window",
        "fullscreen_switch" => "fullscreen switch",
        "open_externally" => "open externally",
        "drag" => "drag from here to export file",
        "stop" => "close",
        other => panic!("no native widget for the reference's {other}"),
    }
}

fn hash_of(name: &str) -> hydrus_core::Sha256 {
    let manifest = hydrus_testkit::fixture_json("legacy_db/basic.manifest.json");
    let hex = manifest["files"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["name"] == name)
        .unwrap()["hash"]
        .as_str()
        .unwrap()
        .to_owned();
    hex.parse().unwrap()
}

fn id_of(fixture: &Fixture, hash: hydrus_core::Sha256) -> HashId {
    fixture
        .store
        .read(move |c| hydrus_store::master::hash_id(c, &hash))
        .unwrap()
        .unwrap()
}

/// The pointer near the top middle, where it raises the hover.
fn raise(viewer: &hydrus_gui::MediaViewerWindow) {
    let position = LogicalPosition::new(500.0, 4.0);
    viewer
        .window()
        .dispatch_event(WindowEvent::PointerMoved { position });
    widgets::settle();
}

/// How far down the window the hover's row of buttons reaches.
const ROW: f32 = 40.0;

/// Where the widget labelled `label` is in the hover's row (a text such as
/// "1/5" may show elsewhere in the window too).
fn in_row(viewer: &hydrus_gui::MediaViewerWindow, label: &str) -> LogicalPosition {
    (0..widgets::count(viewer.window(), label))
        .map(|n| widgets::position(viewer.window(), label, n))
        .find(|at| at.y <= ROW)
        .unwrap_or_else(|| {
            panic!(
                "no {label:?} in the top hover's row: {:?}",
                (0..widgets::count(viewer.window(), label))
                    .map(|n| widgets::position(viewer.window(), label, n))
                    .collect::<Vec<_>>()
            )
        })
}

/// A widget's label: a button's, or a text's own text.
fn label(widget: &Value) -> String {
    match widget["label"].as_str() {
        Some(_) => widget["text"].as_str().unwrap().to_owned(),
        None => label_of(widget["icon"].as_str().unwrap()).to_owned(),
    }
}

/// A window whose pages search "all local files" (the trash included),
/// where the reference's canvas showed the trashed file.
fn start_all_local_files() -> Fixture {
    let (legacy, dir, store) = store();
    let windows = hydrus_gui::headless::init();
    store
        .write_and_refresh(|ctx| {
            let mut defaults: hydrus_store::settings::SearchDefaults =
                hydrus_store::settings::get(ctx.conn())?;
            defaults.local_location =
                hydrus_search::LocationContext::single(hydrus_core::ServiceKey::new(
                    hydrus_core::service::builtin_keys::HYDRUS_LOCAL_FILE_STORAGE.to_vec(),
                ));
            hydrus_store::settings::set(ctx.conn(), &defaults)
        })
        .unwrap();
    let ui = hydrus_gui::MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = hydrus_gui::bind(
        &ui,
        hydrus_gui::Pages::single(hydrus_gui::SearchPage::new(store.clone())),
    );
    Fixture {
        _legacy: legacy,
        _dir: dir,
        windows_: windows,
        store,
        ui,
        bound,
    }
}

/// The browser's files as the recording set them up, on a page of all
/// local files (the trash included), and the viewer opened on the first.
fn open(recorded: &Value) -> (Fixture, hydrus_gui::MediaViewerWindow) {
    let fixture = start_all_local_files();
    let names: Vec<(String, String)> = recorded["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| {
            (
                f["name"].as_str().unwrap().to_owned(),
                f["state"].as_str().unwrap().to_owned(),
            )
        })
        .collect();
    let ids: Vec<(HashId, String)> = names
        .iter()
        .map(|(name, state)| (id_of(&fixture, hash_of(name)), state.clone()))
        .collect();
    let my_files = fixture
        .store
        .snapshot()
        .services
        .builtin(hydrus_core::service::builtin_keys::MY_FILES)
        .unwrap()
        .id;
    fixture
        .store
        .write_content(move |w| {
            let (trash, storage) = (w.roles().trash, w.roles().local_file_storage);
            let mut q = w
                .conn()
                .prepare("SELECT hash_id FROM file_domain_current WHERE service_id = ?1")?;
            let trashed: Vec<HashId> = q
                .query_map([trash], |r| r.get(0))?
                .collect::<Result<_, _>>()?;
            drop(q);
            w.delete_files(storage, &trashed, None)?;
            for (id, state) in &ids {
                w.inbox(&[*id])?;
                match state.as_str() {
                    "archived" => w.archive(&[*id])?,
                    "note" => w.set_note(*id, "note", "a note on the file")?,
                    "trashed" => w.delete_files(my_files, &[*id], None)?,
                    _ => {}
                }
            }
            Ok(())
        })
        .unwrap();
    let hashes: Vec<String> = names
        .iter()
        .map(|(name, _)| hash_of(name).to_hex())
        .collect();
    fixture
        .ui
        .invoke_search_edited(format!("system:hash = {}", hashes.join(" ")).into());
    fixture.ui.invoke_search_accepted();
    // the reference's order: the same files, sorted the same way
    let order: Vec<HashId> = recorded["order"]
        .as_array()
        .unwrap()
        .iter()
        .map(|h| {
            id_of(
                &fixture,
                h.as_str().unwrap().parse::<hydrus_core::Sha256>().unwrap(),
            )
        })
        .collect();
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
    // (the reference's frame was 1000x750)
    viewer
        .window()
        .set_size(slint::LogicalSize::new(1000.0, 750.0));
    widgets::settle();
    (fixture, viewer)
}

/// Show the file `hash` (its place in the recorded order), by the last and
/// next buttons' own callbacks.
fn show(viewer: &hydrus_gui::MediaViewerWindow, recorded: &Value, hash: &str) {
    let place = recorded["order"]
        .as_array()
        .unwrap()
        .iter()
        .position(|h| h == hash)
        .unwrap();
    viewer.invoke_first();
    for _ in 0..place {
        viewer.invoke_next();
    }
    widgets::settle();
}

// leaf: audit-media-viewer-top-hover-layout
// leaf: audit-media-viewer-top-hover-visibility
// leaf: audit-media-viewer-top-hover-tooltips
#[test]
fn the_top_hover_s_widgets_are_the_reference_s_in_its_order() {
    let recorded = hydrus_testkit::fixture_json("viewer_top_hover.json");
    let recorded = &recorded["browser"];
    assert!(recorded["mpv_available"].as_bool().unwrap());
    let (fixture, viewer) = open(recorded);
    // (where libmpv is missing, as the reference without mpv, no volume
    // control: CI has it)
    let mpv = viewer.get_volume_available();
    assert!(
        mpv || std::env::var_os("HYDRUS_REQUIRE_MPV").is_none(),
        "libmpv is needed for the volume control"
    );
    for file in recorded["files"].as_array().unwrap() {
        show(&viewer, recorded, file["hash"].as_str().unwrap());
        raise(&viewer);
        assert!(viewer.get_info_showing(), "the hover is raised");
        // (for review: the hover over each file)
        let window = fixture.windows_.get(fixture.windows_.count() - 1).unwrap();
        let pixels = hydrus_gui::headless::render_snapshot(&window, 1000, 750);
        hydrus_gui::headless::save_png(
            &std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!(
                "viewer_top_hover_{}.png",
                file["name"].as_str().unwrap().replace('.', "_")
            )),
            &pixels,
            1000,
            750,
        )
        .unwrap();
        let mut left = f32::MIN;
        for widget in file["widgets"].as_array().unwrap() {
            let label = label(widget);
            let what = format!("{} on {}", label, file["name"]);
            if widget["class"] == "VolumeControl" && !mpv {
                continue;
            }
            if !widget["shown"].as_bool().unwrap() {
                assert!(
                    (0..widgets::count(viewer.window(), &label)).all(|n| widgets::position(
                        viewer.window(),
                        &label,
                        n
                    )
                    .y > ROW),
                    "{what} is hidden"
                );
                continue;
            }
            // (the index and zoom texts: as recorded, between the buttons
            // either side of them)
            if !widget["label"].is_null() {
                let shown = match widget["label"].as_str().unwrap() {
                    "index" => viewer.get_caption(),
                    _ => viewer.get_zoom_text(),
                };
                assert_eq!(shown, label, "{what}");
                continue;
            }
            let at = in_row(&viewer, &label);
            assert!(
                at.x > left,
                "{what} ({at:?}) is right of the one before ({left}): {:?}",
                (0..widgets::count(viewer.window(), &label))
                    .map(|n| widgets::position(viewer.window(), &label, n))
                    .collect::<Vec<_>>()
            );
            left = at.x;
            assert_eq!(
                widgets::description(viewer.window(), &label).as_deref(),
                widget["tooltip"].as_str(),
                "{what}'s tooltip"
            );
        }
    }
}

/// What the file shown is, as the reference's state after a press reads.
fn state(viewer: &hydrus_gui::MediaViewerWindow) -> (String, String, bool) {
    (
        viewer.get_caption().to_string(),
        viewer.get_zoom_text().to_string(),
        viewer.get_file_inbox(),
    )
}

fn menu_texts(menu: &Value) -> Vec<String> {
    menu.as_array()
        .unwrap()
        .iter()
        .filter_map(|item| item["text"].as_str().map(str::to_owned))
        .collect()
}

// leaf: audit-media-viewer-top-hover-navigation-buttons
// leaf: audit-media-viewer-top-hover-random-slideshow-buttons
// leaf: audit-media-viewer-top-hover-file-buttons
// leaf: audit-media-viewer-top-hover-zoom-buttons
// leaf: audit-media-viewer-top-hover-window-buttons
#[test]
fn the_top_hover_s_buttons_do_what_the_reference_s_do() {
    let recorded = hydrus_testkit::fixture_json("viewer_top_hover.json");
    let recorded = &recorded["browser"];
    // (nothing is really opened or shown: what would be is noted)
    let launched: Rc<RefCell<Vec<String>>> = Rc::default();
    hydrus_gui::set_launcher({
        let launched = launched.clone();
        move |target| launched.borrow_mut().push(target.to_owned())
    });
    let browsed: Rc<RefCell<Vec<String>>> = Rc::default();
    hydrus_gui::set_file_browser({
        let browsed = browsed.clone();
        move |path| browsed.borrow_mut().push(path.to_owned())
    });
    let (fixture, viewer) = open(recorded);
    hydrus_gui::hold_viewer_random(&[recorded["random_draw"].as_u64().unwrap() as usize]);
    let path_of = |hash: &str| {
        let id = id_of(&fixture, hash.parse::<hydrus_core::Sha256>().unwrap());
        hydrus_gui::thumbnail_menu::paths(&fixture.store, &[id])
            .pop()
            .unwrap()
    };
    for press in recorded["presses"].as_array().unwrap() {
        let icon = press["icon"].as_str().unwrap();
        let button = press["button"].as_str().unwrap();
        let before = &press["before"];
        let after = &press["after"];
        let what = format!("{icon} ({button}) on {}", before["shown"]);
        // (to the file pressed on, unless shown: the zoom stays as the
        // presses before left it, as the reference's container kept it)
        if viewer.get_caption() != before["index"].as_str().unwrap() {
            show(&viewer, recorded, before["shown"].as_str().unwrap());
        }
        assert_eq!(
            viewer.get_caption(),
            before["index"].as_str().unwrap(),
            "{what}: before"
        );
        assert_eq!(
            viewer.get_zoom_text(),
            before["zoom"].as_str().unwrap(),
            "{what}: zoom before"
        );
        assert_eq!(
            viewer.get_file_inbox(),
            before["inbox"].as_bool().unwrap(),
            "{what}: inbox before"
        );
        raise(&viewer);
        assert!(viewer.get_info_showing(), "{what}: the hover is raised");
        if (icon == "sound" || icon == "mute") && !viewer.get_volume_available() {
            continue;
        }
        // (the archive button as it is now: the recording names it by the
        // icon it had when the presses began)
        let label = match icon {
            "archive" | "to_inbox" if viewer.get_file_inbox() => "archive",
            "archive" | "to_inbox" => "return to inbox",
            other => label_of(other),
        };
        let at = in_row(&viewer, label);
        let mute_before = global_mute(&fixture);
        let fullscreen_before = viewer.window().is_fullscreen();
        let launched_before = launched.borrow().len();
        let browsed_before = browsed.borrow().len();
        // (the press is the canvas's for a moment; the archive's write lands)
        let pointer = match button {
            "left" => PointerEventButton::Left,
            _ => PointerEventButton::Right,
        };
        if icon == "stop" && button == "left" {
            // close: the viewer closes (checked last, below)
            continue;
        }
        widgets::click_at(viewer.window(), at, pointer);
        widgets::settle();
        let did = press["did"].as_array().unwrap();
        let has = |key: &str| did.iter().any(|d| !d[key].is_null());
        let (index, zoom, inbox) = state(&viewer);
        assert_eq!(
            index,
            after["index"].as_str().unwrap(),
            "{what}: index after"
        );
        assert_eq!(zoom, after["zoom"].as_str().unwrap(), "{what}: zoom after");
        assert_eq!(
            inbox,
            after["inbox"].as_bool().unwrap(),
            "{what}: inbox after"
        );
        // the global mute, flipped by the volume control
        if let Some(muted) = press["global_mute"].as_bool() {
            assert_eq!(global_mute(&fixture), muted, "{what}: global mute");
            assert!(viewer.get_global_muted() == muted, "{what}: its icon");
        } else {
            assert_eq!(global_mute(&fixture), mute_before, "{what}: no mute change");
        }
        // a menu: its entries
        if let Some(menu) = did.iter().find_map(|d| d.get("menu")) {
            let texts = menu_texts(menu);
            match icon {
                "slideshow" => {
                    viewer.invoke_context_menu_requested();
                    let rows: Vec<String> = group_rows(&viewer.get_context_menu().slideshow)
                        .into_iter()
                        .map(|r| r.0)
                        .collect();
                    assert_eq!(rows, texts, "{what}: the slideshow menu");
                }
                "keyboard" => {
                    assert_eq!(texts, ["edit shortcuts"], "{what}");
                    assert_eq!(viewer.get_shortcut_names().row_count(), 0, "{what}");
                }
                "zoom_cog" => {
                    let rows: Vec<(String, bool)> = [
                        &viewer.get_top_zoom_menu().g1,
                        &viewer.get_top_zoom_menu().g2,
                    ]
                    .into_iter()
                    .flat_map(|g| {
                        g.iter()
                            .map(|r| (r.label.to_string(), r.checked))
                            .collect::<Vec<_>>()
                    })
                    .collect();
                    let expected: Vec<(String, bool)> = menu.as_array().unwrap()[..8]
                        .iter()
                        .filter(|i| i["separator"].is_null())
                        .map(|i| {
                            (
                                i["text"].as_str().unwrap().to_owned(),
                                i["checked"].as_bool().unwrap_or(false),
                            )
                        })
                        .collect();
                    assert_eq!(rows, expected, "{what}: recenter and the zoom types");
                }
                "eye" => assert!(
                    viewer.get_eye_menu().window.g1.row_count() > 0,
                    "{what}: the view options menu"
                ),
                // (the move button's right-click resize menu is not ported:
                // DIFFERENCES.md)
                "move" => {}
                other => panic!("{other}'s menu"),
            }
            assert!(
                widgets::popup_open(viewer.window()) || icon == "move",
                "{what}: a menu opened"
            );
            widgets::close_popups(viewer.window());
        }
        // the trash and delete buttons ask as the delete key does; undelete
        // asks, and returns the file
        if did.iter().any(|d| d["pub"] == "canvas_delete") {
            let dialog = fixture.bound.viewer_deletion.borrow().as_ref().map_or_else(
                || panic!("{what}: asks to delete"),
                slint::ComponentHandle::clone_strong,
            );
            assert!(
                dialog.get_composed_title().starts_with("Delete files?"),
                "{what}"
            );
            dialog.invoke_cancel();
        }
        if did.iter().any(|d| d["pub"] == "canvas_undelete") {
            assert!(
                !viewer.get_question().is_empty(),
                "{what}: asks to undelete"
            );
            viewer.invoke_answer(true);
            widgets::settle();
            assert!(!viewer.get_file_trashed(), "{what}: undeleted");
        }
        if did.iter().any(|d| !d["dialog"].is_null()) {
            let shown = fixture.bound.embedded_metadata.borrow().is_some();
            assert!(shown, "{what}: the embedded metadata window");
            if let Some(window) = fixture.bound.embedded_metadata.borrow_mut().take() {
                let _ = window.hide();
            }
        }
        if has("open externally") {
            assert_eq!(
                launched.borrow()[launched_before..],
                [path_of(before["shown"].as_str().unwrap())],
                "{what}: opened"
            );
        }
        if has("drag") {
            // (the stand-in for dragging it out: shown in the file browser)
            assert_eq!(
                browsed.borrow()[browsed_before..],
                [path_of(before["shown"].as_str().unwrap())],
                "{what}"
            );
        }
        if did.iter().any(|d| d["pub"] == "canvas_fullscreen_switch") {
            assert_eq!(
                viewer.window().is_fullscreen(),
                !fullscreen_before,
                "{what}: fullscreen switched"
            );
            viewer.invoke_toggle_fullscreen();
        }
        if did
            .iter()
            .any(|d| d["command"] == "SIMPLE_FOCUS_TAB_AND_MEDIA")
        {
            let shown = id_of(
                &fixture,
                before["shown"]
                    .as_str()
                    .unwrap()
                    .parse::<hydrus_core::Sha256>()
                    .unwrap(),
            );
            assert_eq!(
                fixture.selected(),
                [shown],
                "{what}: the file selected in its page"
            );
        }
    }
    // close, last
    raise(&viewer);
    widgets::click_at(
        viewer.window(),
        widgets::position(viewer.window(), "close", 0),
        PointerEventButton::Left,
    );
    assert!(fixture.bound.viewer.borrow().is_none(), "the viewer closed");
    hydrus_gui::hold_viewer_random(&[]);
}

fn global_mute(fixture: &Fixture) -> bool {
    let audio: hydrus_core::media_viewer::AudioSettings = fixture
        .store
        .read(hydrus_store::settings::get)
        .unwrap_or_default();
    audio.global_mute
}

// leaf: audit-media-viewer-top-hover-volume
#[test]
fn over_the_volume_control_the_viewer_s_mute_shows_and_mutes() {
    let recorded = hydrus_testkit::fixture_json("viewer_top_hover.json");
    let recorded = &recorded["browser"];
    let (fixture, viewer) = open(recorded);
    if !viewer.get_volume_available() {
        assert!(
            std::env::var_os("HYDRUS_REQUIRE_MPV").is_none(),
            "libmpv is needed for the volume control"
        );
        return;
    }
    let control = recorded["files"][0]["widgets"]
        .as_array()
        .unwrap()
        .iter()
        .find(|w| w["class"] == "VolumeControl")
        .unwrap();
    // the reference's popup: the viewer's own mute, with its tooltip
    let mute = control["popup"]["mute_tooltip"].as_str().unwrap();
    raise(&viewer);
    assert!(
        !widgets::shows(viewer.window(), mute),
        "shut until pointed at"
    );
    let at = in_row(&viewer, "Global mute/unmute");
    viewer
        .window()
        .dispatch_event(WindowEvent::PointerMoved { position: at });
    widgets::settle();
    assert!(
        widgets::shows(viewer.window(), mute),
        "open over the control"
    );
    let before = fixture
        .store
        .read(hydrus_store::settings::get::<hydrus_core::media_viewer::AudioSettings>)
        .unwrap_or_default();
    let on = widgets::position(viewer.window(), mute, 0);
    for event in [
        WindowEvent::PointerMoved { position: on },
        WindowEvent::PointerPressed {
            position: on,
            button: PointerEventButton::Left,
        },
        WindowEvent::PointerReleased {
            position: on,
            button: PointerEventButton::Left,
        },
    ] {
        viewer.window().dispatch_event(event);
    }
    widgets::settle();
    let after = fixture
        .store
        .read(hydrus_store::settings::get::<hydrus_core::media_viewer::AudioSettings>)
        .unwrap_or_default();
    assert_eq!(
        after.viewer_mute, !before.viewer_mute,
        "the viewer's mute flips"
    );
    assert_eq!(after.global_mute, before.global_mute, "not the global one");
    assert_eq!(viewer.get_viewer_muted(), after.viewer_mute, "its icon");
}
