//! The embed button, as the reference's (`oracle/record_embed_button.py`:
//! JPEGs and animated GIFs set to start with the embed button in the media
//! viewer and the preview, PNGs not; a real `CanvasMediaListBrowser` and a
//! real page's preview canvas over the three, and real presses on the
//! button). A file of a kind that starts with it waits behind it, in the
//! place the file takes: a right press there opens the viewer's menu and the
//! file still waits; a left press starts the file (drawn, or played). A file
//! of another kind shows at once, and coming back to a file that waits shows
//! the button again.

use std::time::{Duration, Instant};

use hydrus_core::media_viewer::MediaViewerSettings;
use hydrus_core::{HashId, Mime};
use serde_json::Value;
use slint::ComponentHandle as _;
use slint::platform::PointerEventButton;

use super::common::widgets;
use super::media_support::*;

const LABEL: &str = "embed button";

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

fn id_of(store: &hydrus_store::Store, hash: hydrus_core::Sha256) -> HashId {
    store
        .read(move |c| hydrus_store::master::hash_id(c, &hash))
        .unwrap()
        .unwrap()
}

/// JPEGs and animated GIFs start with the embed button, in the media viewer
/// and in the preview, as the recording set them.
fn start_with_embed(store: &hydrus_store::Store) {
    store
        .write_and_refresh(|ctx| {
            let mut settings: MediaViewerSettings = hydrus_store::settings::get(ctx.conn())?;
            for mime in [Mime::ImageJpeg, Mime::AnimationGif] {
                let mut view = settings.view(mime);
                view.media_start_with_embed = true;
                view.preview_start_with_embed = true;
                settings.media_view.insert(mime.code(), view);
            }
            hydrus_store::settings::set(ctx.conn(), &settings)
        })
        .unwrap();
}

fn until(done: impl Fn() -> bool) {
    let started = Instant::now();
    while !done() {
        slint::platform::update_timers_and_animations();
        assert!(started.elapsed() < Duration::from_secs(10), "timed out");
        std::thread::sleep(Duration::from_millis(5));
    }
}

/// The recording's files on a page in the reference's order.
fn search(fixture: &Fixture, recorded: &Value) -> Vec<HashId> {
    let hashes: Vec<&str> = recorded["order"]
        .as_array()
        .unwrap()
        .iter()
        .map(|h| h.as_str().unwrap())
        .collect();
    fixture
        .ui
        .invoke_search_edited(format!("system:hash = {}", hashes.join(" ")).into());
    fixture.ui.invoke_search_accepted();
    let order: Vec<HashId> = hashes
        .iter()
        .map(|h| id_of(&fixture.store, h.parse().unwrap()))
        .collect();
    assert_eq!(
        fixture.results(),
        order,
        "the files in the reference's order"
    );
    order
}

// leaf: interaction-embed-button
#[test]
fn a_file_that_starts_with_the_embed_button_waits_behind_it_in_the_viewer() {
    let recorded = hydrus_testkit::fixture_json("embed_button.json");
    let fixture = start();
    start_with_embed(&fixture.store);
    let order = search(&fixture, &recorded);
    fixture.ui.invoke_thumbnail_activated(0);
    let viewer = fixture
        .bound
        .viewer
        .borrow()
        .as_ref()
        .map(slint::ComponentHandle::clone_strong)
        .expect("the viewer opens");
    let (width, height) = (1000.0, 750.0);
    viewer
        .window()
        .set_size(slint::LogicalSize::new(width, height));
    widgets::settle();
    for step in recorded["viewer"].as_array().unwrap() {
        let what = format!("{step}");
        let name = step["file"].as_str().unwrap();
        let file = id_of(&fixture.store, hash_of(name));
        match step["do"].as_str().unwrap() {
            "show" | "show again" => {
                // (as the viewer's own navigation goes to it)
                viewer.invoke_first();
                for _ in 0..order.iter().position(|&f| f == file).unwrap() {
                    viewer.invoke_next();
                }
                widgets::settle();
            }
            "press" => {
                let at = widgets::position(viewer.window(), LABEL, 0);
                let button = match step["button"].as_str().unwrap() {
                    "right" => PointerEventButton::Right,
                    _ => PointerEventButton::Left,
                };
                widgets::click_at(viewer.window(), at, button);
                widgets::settle();
                let menu = step["menu"].as_array().unwrap();
                assert_eq!(
                    widgets::popup_open(viewer.window()),
                    !menu.is_empty(),
                    "{what}: the menu opens"
                );
                if let Some(menu) = menu.first() {
                    assert_eq!(
                        viewer.get_context_menu().info_title,
                        menu[0].as_str().unwrap(),
                        "{what}"
                    );
                    widgets::close_popups(viewer.window());
                }
            }
            other => panic!("{other}"),
        }
        let embed = step["embed"].as_bool().unwrap();
        assert_eq!(viewer.get_embed_shown(), embed, "{what}");
        assert_eq!(
            widgets::bounds(viewer.window(), LABEL).is_some(),
            embed,
            "{what}"
        );
        if let Some(rect) = step["embed_rect"].as_array() {
            // (in the place the file takes, which here is the whole canvas)
            let rect: Vec<f32> = rect.iter().map(|v| v.as_f64().unwrap() as f32).collect();
            let (x, y, w, h) = widgets::bounds(viewer.window(), LABEL).unwrap();
            assert_eq!(
                (x.round(), y.round(), w.round(), h.round()),
                (rect[0], rect[1], rect[2], rect[3]),
                "{what}"
            );
        }
        // the media window: none while it waits, a still drawn or a file
        // played when it starts
        let media = viewer.get_media();
        match step["media_window"].as_str() {
            None => {
                assert_eq!(media.size().width, 0, "{what}: nothing drawn");
                assert!(!viewer.get_scanbar_shown(), "{what}: nothing played");
            }
            Some("StaticImage") => {
                assert!(media.size().width > 0, "{what}: drawn");
                assert!(!viewer.get_scanbar_shown(), "{what}");
            }
            Some("MPVWidget") => {
                assert!(viewer.get_scanbar_shown(), "{what}: played");
            }
            Some(other) => panic!("{other}"),
        }
    }
}

/// The preview of a page over the recording's files.
fn preview_page() -> (Fixture, Vec<HashId>) {
    let recorded = hydrus_testkit::fixture_json("embed_button.json");
    let fixture = start();
    std::fs::write(fixture.store.dir().join("mpv.conf"), "ao=null\n").unwrap();
    start_with_embed(&fixture.store);
    let order = search(&fixture, &recorded);
    for _ in 0..3 {
        let window = fixture.windows_.get(0).unwrap();
        hydrus_gui::headless::render(&window, 1000, 900);
    }
    fixture.bound.preview.refresh();
    assert!(fixture.ui.get_preview_actual_height() > 0.0);
    (fixture, order)
}

// leaf: interaction-embed-button
#[test]
fn a_file_that_starts_with_the_embed_button_waits_behind_it_in_the_preview() {
    let recorded = hydrus_testkit::fixture_json("embed_button.json");
    let (fixture, order) = preview_page();
    let ui = &fixture.ui;
    let refresh_until = |done: &dyn Fn() -> bool| {
        until(|| {
            fixture.bound.preview.refresh();
            done()
        });
    };
    let playing = || fixture.bound.preview.playing().0;
    for step in recorded["preview"].as_array().unwrap() {
        let what = format!("{step}");
        let name = step["file"].as_str().unwrap();
        let file = id_of(&fixture.store, hash_of(name));
        match step["do"].as_str().unwrap() {
            "show" => {
                let index = order.iter().position(|&f| f == file).unwrap();
                ui.invoke_thumbnail_clicked(i32::try_from(index).unwrap(), false, false);
                refresh_until(&|| fixture.bound.preview.displayed_file() == Some(file));
            }
            "press" => {
                assert_eq!(step["button"], "left");
                let at = widgets::position(ui.window(), LABEL, 0);
                widgets::click_at(ui.window(), at, PointerEventButton::Left);
                refresh_until(&|| !ui.get_preview_embed());
            }
            other => panic!("{other}"),
        }
        let embed = step["embed"].as_bool().unwrap();
        refresh_until(&|| ui.get_preview_embed() == embed);
        assert_eq!(widgets::bounds(ui.window(), LABEL).is_some(), embed, "{what}");
        if embed {
            // (in the place the file takes: the reference's button fills its
            // media container)
            let (x, y, w, h) = widgets::bounds(ui.window(), LABEL).unwrap();
            let media = widgets::bounds(ui.window(), "preview media").unwrap();
            assert_eq!((x, y, w, h), media, "{what}");
            let container = step["container"].as_array().unwrap();
            let rect = step["embed_rect"].as_array().unwrap();
            assert_eq!(rect[2], container[2], "{what}");
            assert_eq!(rect[3], container[3], "{what}");
        }
        match step["media_window"].as_str() {
            None => {
                assert_eq!(ui.get_preview_media().size().width, 0, "{what}");
                assert_eq!(playing(), None, "{what}: nothing played");
            }
            Some("StaticImage") => {
                refresh_until(&|| ui.get_preview_media().size().width > 0);
                assert_eq!(playing(), None, "{what}");
            }
            Some("MPVWidget") => {
                refresh_until(&|| playing().is_some());
            }
            Some(other) => panic!("{other}"),
        }
    }
}
