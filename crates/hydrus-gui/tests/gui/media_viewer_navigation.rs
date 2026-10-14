//! The media viewer's navigation, through the real viewer window: first,
//! previous, next and last (round the ends, as `MediaList` does), a shuffled
//! slideshow's random moves, and removing the file shown from the view.

use std::time::{Duration, Instant};

use slint::ComponentHandle;

use super::common::widgets;
use super::media_support::*;

/// Press a key in the viewer's window.
fn press(viewer: &hydrus_gui::MediaViewerWindow, text: &str) {
    match text {
        "ctrl+r" => widgets::ctrl_key(viewer.window(), "r"),
        "home" => widgets::key(viewer.window(), slint::platform::Key::Home),
        "end" => widgets::key(viewer.window(), slint::platform::Key::End),
        "right" => widgets::key(viewer.window(), slint::platform::Key::RightArrow),
        "left" => widgets::key(viewer.window(), slint::platform::Key::LeftArrow),
        other => widgets::key(viewer.window(), other),
    }
}

// leaf: audit-media-viewer-navigation
#[test]
fn the_viewer_goes_first_previous_next_last_random_and_removes_from_view() {
    // the shortcut-only commands: random on y, undo random on u
    let fixture = start();
    fixture
        .store
        .write(|ctx| {
            let mut settings: hydrus_core::shortcuts::Settings =
                hydrus_store::settings::get(ctx.conn())?;
            let set = settings.sets.entry("media_viewer".into()).or_default();
            for (key, action) in [('y', 186), ('u', 187)] {
                set.push(hydrus_core::shortcuts::Binding {
                    gesture: hydrus_core::shortcuts::Gesture::new(0, key as u32, 0, 0),
                    action,
                    text: None,
                    content: None,
                });
            }
            hydrus_store::settings::set(ctx.conn(), &settings)
        })
        .unwrap();
    fixture.ui.invoke_search_edited("system:filetype is jpeg".into());
    fixture.ui.invoke_search_accepted();
    let n = fixture.results().len();

    // the reference's real viewer, recorded by
    // oracle/record_viewer_navigation.py: each command a shortcut sends,
    // with random draws scripted; after each, the index string shown and
    // where in the list the file shown is
    let recorded = hydrus_testkit::fixture_json("viewer_navigation.json");
    assert_eq!(n as u64, recorded["count"].as_u64().unwrap());
    let steps = recorded["steps"].as_array().unwrap();
    let start = &steps[0];
    assert_eq!(start["do"], "open");
    // (the reference opened on the file at 5/14 of its list; this is the same
    // place in ours)
    let opened_at: usize = start["index"]
        .as_str()
        .unwrap()
        .split('/')
        .next()
        .unwrap()
        .parse()
        .unwrap();
    fixture.ui.invoke_thumbnail_activated(opened_at as i32 - 1);
    let viewer = fixture
        .bound
        .viewer
        .borrow()
        .as_ref()
        .map(ComponentHandle::clone_strong)
        .expect("the viewer opens");
    widgets::lay_out(viewer.window(), 1000.0, 700.0);
    hydrus_gui::hold_viewer_random(
        &recorded["draws"]
            .as_array()
            .unwrap()
            .iter()
            .map(|d| d.as_u64().unwrap() as usize)
            .collect::<Vec<_>>(),
    );
    let mut closed = false;
    assert_eq!(viewer.get_caption(), start["index"].as_str().unwrap());
    for step in &steps[1..] {
        let command = step["do"].as_str().unwrap();
        press(
            &viewer,
            match command {
                "first" => "home",
                "last" => "end",
                "next" => "right",
                "previous" => "left",
                "random" => "y",
                "undo random" => "u",
                "remove" => "ctrl+r",
                other => panic!("{other}"),
            },
        );
        slint::platform::update_timers_and_animations();
        if step["closed"].as_bool().unwrap() {
            closed = fixture.bound.viewer.borrow().is_none();
            break;
        }
        assert_eq!(
            viewer.get_caption(),
            step["index"].as_str().unwrap(),
            "after {command}: {step}"
        );
    }
    assert!(closed, "removing the last file closes the viewer");
}

// the slideshow's shuffle (a native-only check: its draws are not scripted)
#[test]
fn a_shuffled_slideshow_moves_to_other_files() {
    let fixture = start();
    let n = fixture.results().len();
    assert!(n > 10);
    fixture.ui.invoke_thumbnail_activated(4);
    let viewer = fixture
        .bound
        .viewer
        .borrow()
        .as_ref()
        .map(ComponentHandle::clone_strong)
        .expect("the viewer opens");
    // random moves (a shuffled slideshow's): never to the file already shown,
    // and not just on to the next one
    let slideshow = |viewer: &hydrus_gui::MediaViewerWindow| {
        viewer.invoke_context_menu_requested();
        group_rows(&viewer.get_context_menu().slideshow)
    };
    let all = slideshow(&viewer);
    viewer.invoke_menu_chosen(find(&all, "shuffle this slideshow"));
    let all = slideshow(&viewer);
    viewer.invoke_menu_chosen(find(&all, "very fast"));
    let mut shown = vec![viewer.get_caption().to_string()];
    let started = Instant::now();
    while started.elapsed() < Duration::from_secs(2) {
        slint::platform::update_timers_and_animations();
        let caption = viewer.get_caption().to_string();
        if shown.last() != Some(&caption) {
            shown.push(caption);
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    let numbers: Vec<usize> = shown
        .iter()
        .map(|c: &String| c.split('/').next().unwrap().parse().unwrap())
        .collect();
    assert!(numbers.len() > 5, "{shown:?}");
    assert!(
        numbers.windows(2).any(|w| w[1] != w[0] % n + 1),
        "not merely the next file each time: {numbers:?}"
    );
    // stopped
    let all = slideshow(&viewer);
    viewer.invoke_menu_chosen(find(&all, "stop"));
    let held = viewer.get_caption().to_string();
    std::thread::sleep(Duration::from_millis(300));
    slint::platform::update_timers_and_animations();
    assert_eq!(viewer.get_caption(), held);

    // removing the file shown: the next takes its place, one fewer in all;
    // at the end, the first
    viewer.invoke_first();
    viewer.invoke_next();
    viewer.invoke_remove_from_view();
    assert_eq!(viewer.get_caption(), format!("2/{}", n - 1));
    viewer.invoke_last();
    assert_eq!(viewer.get_caption(), format!("{}/{}", n - 1, n - 1));
    viewer.invoke_remove_from_view();
    assert_eq!(viewer.get_caption(), format!("1/{}", n - 2));
    viewer.invoke_close_requested();
}
