//! The media viewer's navigation, through the real viewer window: first,
//! previous, next and last (round the ends, as `MediaList` does), a shuffled
//! slideshow's random moves, and removing the file shown from the view.

use std::time::{Duration, Instant};

use slint::ComponentHandle;

use super::media_support::*;

// leaf: audit-media-viewer-navigation
#[test]
fn the_viewer_goes_first_previous_next_last_random_and_removes_from_view() {
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
    let at = |i: usize| format!("{i}/{n}");
    assert_eq!(viewer.get_caption(), at(5));
    viewer.invoke_next();
    assert_eq!(viewer.get_caption(), at(6));
    viewer.invoke_previous();
    viewer.invoke_previous();
    assert_eq!(viewer.get_caption(), at(4));
    viewer.invoke_first();
    assert_eq!(viewer.get_caption(), at(1));
    // round the ends
    viewer.invoke_previous();
    assert_eq!(viewer.get_caption(), at(n));
    viewer.invoke_next();
    assert_eq!(viewer.get_caption(), at(1));
    viewer.invoke_last();
    assert_eq!(viewer.get_caption(), at(n));
    viewer.invoke_first();
    viewer.invoke_next();
    viewer.invoke_next();

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
