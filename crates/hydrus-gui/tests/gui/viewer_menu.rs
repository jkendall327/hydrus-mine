//! The media viewer's right-click menu is the reference's
//! (`oracle/record_viewer_menu.py`: its own `ShowMenuFromSignal` in a
//! viewer of the `basic` fixture's files), for the entries hydrus-rs has
//! so far: the reference's menu is compared with the others left out,
//! built from what the reference's viewer read (its zoom, its canvas's,
//! whether it was at its largest, whether it was fullscreen).

use std::sync::Arc;

use hydrus_core::HashId;
use hydrus_core::media_viewer::{AudioSettings, InfoLineSettings, SlideshowSettings};
use hydrus_gui::slideshow::Slideshow;
use hydrus_gui::thumbnail_menu::Slots;
use hydrus_gui::viewer_menu::{Player, ViewerState, ZoomState, player, viewer_menu};
use hydrus_store::Store;
use hydrus_store::import::import_legacy;
use serde_json::{Value, json};

use crate::common::menus::{as_recorded, described, pruned, tidy, unescaped};

#[test]
fn the_viewer_s_menu_is_the_reference_s() {
    let fixture = hydrus_testkit::fixture_json("viewer_menu.json");
    let legacy = hydrus_testkit::legacy_fixture("basic");
    let native = tempfile::tempdir().unwrap();
    import_legacy(
        legacy.path(),
        &native.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store: Arc<Store> = Store::open(native.path()).unwrap();
    let now_ms = fixture["now"].as_i64().unwrap() * 1000;
    let id = |hash: &Value| -> HashId {
        store
            .read(|c| hydrus_store::master::hash_id(c, &hash.as_str().unwrap().parse().unwrap()))
            .unwrap()
            .unwrap()
    };
    let mut checked = 0;
    for viewer in fixture["viewers"].as_array().unwrap() {
        for case in viewer["menus"].as_array().unwrap() {
            let file = id(&case["hash"]);
            let facts = &case["viewer"];
            // what plays it is what plays it in the reference
            let ours = player(&store, file);
            assert_eq!(
                ours.label(),
                facts["player"].as_str().unwrap(),
                "{}",
                case["file"]
            );
            let state = ViewerState {
                zoom: facts["zoomable"].as_bool().unwrap().then(|| ZoomState {
                    current: facts["zoom"].as_f64().unwrap(),
                    canvas: facts["canvas_zoom"].as_f64().unwrap(),
                    at_max: facts["at_max_zoom"].as_bool().unwrap(),
                }),
                fullscreen: facts["fullscreen"].as_bool().unwrap(),
                audio: AudioSettings::default(),
                forced_mute: None,
                player: ours,
                // (no slideshow yet, and the options' defaults)
                slideshow: Slideshow::new(&SlideshowSettings::default()),
                slideshow_settings: SlideshowSettings::default(),
            };
            let entries = viewer_menu(&store, file, &state, &InfoLineSettings::default(), now_ms);
            let ours = described(&entries);
            let recorded_ours = as_recorded(&ours);
            // (and the window's template shows it as it is)
            assert_eq!(described(&Slots::new(&entries).entries()), ours);
            // the reference's, less what hydrus-rs doesn't have (its info's
            // detailed metadata included)
            let mut theirs = unescaped(case["menu"].as_array().unwrap());
            let mut info = theirs.remove(0);
            let mut theirs = pruned(&theirs);
            if let Some(entries) = info.get_mut("entries") {
                let kept: Vec<Value> = entries.as_array().unwrap().clone();
                *entries = Value::Array(tidy(kept));
            }
            theirs.insert(0, info);
            theirs.insert(1, json!("---"));
            let theirs = tidy(theirs);
            assert!(
                recorded_ours == theirs,
                "{} {}\n{}\n!=\n{}",
                viewer["viewer"],
                case["file"],
                serde_json::to_string_pretty(&recorded_ours).unwrap(),
                serde_json::to_string_pretty(&theirs).unwrap(),
            );
            checked += 1;
        }
    }
    // five files in my files, four in all local files
    assert_eq!(checked, 9);
    // (an animation the client plays itself, which the recordings don't
    // have: the fixture's animated webp was deleted)
    assert_eq!(Player::Animation.label(), "Hydrus Native Animation Player");
}

/// Every row of the template's menu, by label.
fn rows(menu: &hydrus_gui::ThumbnailMenu) -> Vec<(String, i32)> {
    use slint::Model as _;
    let mut out = Vec::new();
    let mut add = |rows: &slint::ModelRc<hydrus_gui::MenuRow>| {
        out.extend(rows.iter().map(|r| (r.label.to_string(), r.id)));
    };
    for rows in [
        &menu.info_before,
        &menu.head,
        &menu.zoom,
        &menu.volume.g1,
        &menu.volume.g2,
        &menu.volume.g3,
        &menu.dismiss,
        &menu.filter,
        &menu.delete,
        &menu.delete_menu,
        &menu.trash,
        &menu.manage,
        &menu.open_a,
        &menu.open_b,
        &menu.share_a,
        &menu.share_c,
        &menu.share_d,
        &menu.player,
    ] {
        add(rows);
    }
    out
}

#[test]
fn a_right_click_shows_the_viewer_s_menu_and_its_entries_act() {
    use std::cell::RefCell;
    use std::rc::Rc;

    use hydrus_gui::{Clip, MainWindow, Pages, SearchPage, bind, headless};
    use slint::ComponentHandle as _;
    use slint::platform::{PointerEventButton, WindowEvent};

    let legacy = hydrus_testkit::legacy_fixture("basic");
    let native = tempfile::tempdir().unwrap();
    import_legacy(
        legacy.path(),
        &native.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store: Arc<Store> = Store::open(native.path()).unwrap();
    super::common::remove_trashed_from_view(&store);
    let copied: Rc<RefCell<Vec<Clip>>> = Rc::default();
    hydrus_gui::set_clipper({
        let copied = copied.clone();
        move |clip| copied.borrow_mut().push(clip.clone())
    });
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::single(SearchPage::new(store.clone())));
    ui.invoke_search_edited("system:everything".into());
    ui.invoke_search_accepted();
    let page = bound.current.borrow().clone();
    let files = page.borrow().results().to_vec();
    let inboxed = store
        .read(|c| hydrus_store::media::inboxed(c, &files))
        .unwrap();
    // a still image in the inbox
    let index = files
        .iter()
        .position(|&f| inboxed.contains(&f) && player(&store, f) == Player::StaticImage)
        .unwrap();
    let file = files[index];
    ui.invoke_thumbnail_activated(i32::try_from(index).unwrap());
    let viewer = bound
        .viewer
        .borrow()
        .as_ref()
        .map(slint::ComponentHandle::clone_strong)
        .unwrap();
    // a right-click on the window asks for the menu (and shows it, which
    // in a headless window stays open: the menu is asked for again
    // directly from then on)
    let size = viewer
        .window()
        .size()
        .to_logical(viewer.window().scale_factor());
    let at = slint::LogicalPosition::new(size.width / 2.0, size.height / 2.0);
    viewer.window().dispatch_event(WindowEvent::PointerPressed {
        position: at,
        button: PointerEventButton::Right,
    });
    viewer
        .window()
        .dispatch_event(WindowEvent::PointerReleased {
            position: at,
            button: PointerEventButton::Right,
        });
    let right_click = || {
        viewer.invoke_context_menu_requested();
        rows(&viewer.get_context_menu())
    };
    let find = |rows: &[(String, i32)], label: &str| {
        rows.iter()
            .find(|(l, _)| l.starts_with(label))
            .unwrap_or_else(|| panic!("{label} in {rows:?}"))
            .1
    };
    let menu = rows(&viewer.get_context_menu());
    assert!(viewer.get_context_menu().info_is_menu);
    assert!(viewer.get_context_menu().zoom_title.starts_with("zoom: "));
    assert_eq!(viewer.get_context_menu().player_title, "player", "{menu:?}");
    // the volume, shown, can't be chosen
    assert_eq!(find(&menu, "volume: 70"), -1);

    // zoom to max: the menu then has no "zoom to max"
    viewer.invoke_menu_chosen(find(&menu, "zoom to max"));
    let menu = right_click();
    assert_eq!(viewer.get_context_menu().zoom_title, "zoom: 2000%");
    assert!(menu.iter().all(|(l, _)| l != "zoom to max"), "{menu:?}");
    // fullscreen: out of it, and the entry says go back
    let fullscreen = viewer.window().is_fullscreen();
    let label = if fullscreen {
        "exit fullscreen"
    } else {
        "go fullscreen"
    };
    viewer.invoke_menu_chosen(find(&menu, label));
    assert_ne!(
        viewer.window().is_fullscreen(),
        fullscreen || cfg!(target_os = "macos")
    );
    // force mute just here: then force unmute, or stop forcing
    let menu = right_click();
    viewer.invoke_menu_chosen(find(&menu, "force mute just here"));
    let menu = right_click();
    assert!(
        menu.iter().any(|(l, _)| l == "stop forcing mute"),
        "{menu:?}"
    );
    // mute media viewer: kept in the options
    viewer.invoke_menu_chosen(find(&menu, "mute media viewer"));
    let kept: AudioSettings = store.read(hydrus_store::settings::get).unwrap();
    assert!(kept.viewer_mute);
    // copy file
    let menu = right_click();
    viewer.invoke_menu_chosen(find(&menu, "copy file"));
    assert_eq!(copied.borrow().len(), 1);
    // archive: archived, and the menu offers to return it
    viewer.invoke_menu_chosen(find(&menu, "archive"));
    let inboxed = store
        .read(|c| hydrus_store::media::inboxed(c, &[file]))
        .unwrap();
    assert!(inboxed.is_empty());
    let menu = right_click();
    assert!(menu.iter().any(|(l, _)| l == "return to inbox"), "{menu:?}");
    // open → in a new page: a page of it, locked to it
    let pages_before = bound.pages.borrow().tabs()[0].names.len();
    viewer.invoke_menu_chosen(find(&menu, "in a new page"));
    assert_eq!(bound.pages.borrow().tabs()[0].names.len(), pages_before + 1);
    let opened = bound.pages.borrow_mut().current();
    assert_eq!(opened.borrow().results(), [file]);
    // delete from my files: asked, then gone from the viewer
    let caption = viewer.get_caption().to_string();
    let menu = right_click();
    let delete = menu
        .iter()
        .find(|(l, _)| l.starts_with("delete from "))
        .unwrap_or_else(|| panic!("{menu:?}"))
        .1;
    viewer.invoke_menu_chosen(delete);
    assert!(!viewer.get_question().is_empty());
    viewer.invoke_answer(true);
    assert_ne!(viewer.get_caption().to_string(), caption);
    // remove from view: the next file shown, one fewer
    let menu = right_click();
    let before = viewer.get_caption().to_string();
    viewer.invoke_menu_chosen(find(&menu, "remove from view"));
    assert_ne!(viewer.get_caption().to_string(), before);
    viewer.invoke_close_requested();
}
