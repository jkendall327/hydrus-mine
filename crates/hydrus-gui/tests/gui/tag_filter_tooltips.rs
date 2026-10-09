//! The tooltip of the tag filter editor's "show other panels" button: the
//! reference's text, drawn by Slint's `Tooltip` once the pointer has rested on
//! the button. The other tag filter buttons' tooltips are tested where their
//! windows are: tag migration, tag display, the import options' tag filtering,
//! and the model's `tag_filter_tooltips` replay of the reference's button text.
use hydrus_core::tag_filter::{FilterRule, TagFilter};
use hydrus_gui::{headless, tag_filter_window};
use hydrus_store::Store;
use slint::ComponentHandle as _;
use slint::LogicalPosition;
use slint::platform::WindowEvent;
use std::{cell::RefCell, rc::Rc, sync::Arc, time::Duration};

/// The window's pixels near `at` once the pointer, moved there once, has rested
/// past the tooltip's delay. (Each move restarts that delay, as a real hand's
/// last move does not, so there is exactly one.) Only the neighbourhood of the
/// pointer is kept, where the tooltip would be: a text cursor blinks elsewhere.
pub fn picture_after_resting(
    ui: &impl slint::ComponentHandle,
    adapter: &slint::platform::software_renderer::MinimalSoftwareWindow,
    size: (u32, u32),
    at: (f32, f32),
) -> Vec<u8> {
    ui.window().dispatch_event(WindowEvent::PointerExited);
    headless::render(adapter, size.0, size.1);
    ui.window().dispatch_event(WindowEvent::PointerMoved {
        position: LogicalPosition::new(at.0, at.1),
    });
    for _ in 0..8 {
        std::thread::sleep(Duration::from_millis(100));
        slint::platform::update_timers_and_animations();
    }
    let picture = headless::render(adapter, size.0, size.1);
    ui.window().dispatch_event(WindowEvent::PointerExited);
    near(&picture, size, at)
}

/// The rows of `picture` within 60 lines and 300 columns of `at`.
pub fn near(picture: &[u8], size: (u32, u32), at: (f32, f32)) -> Vec<u8> {
    let width = size.0 as usize;
    let (px, py) = (at.0 as usize, at.1 as usize);
    let mut out = Vec::new();
    for row in py.saturating_sub(30)..(py + 90).min(size.1 as usize) {
        out.extend_from_slice(
            &picture[(row * width + px.saturating_sub(300)) * 4
                ..(row * width + (px + 300).min(width)) * 4],
        );
    }
    out
}

// leaf: audit-shared-tag-tooltips
#[test]
fn show_other_panels_shows_the_references_tooltip_once_the_pointer_rests_on_it() {
    let f = hydrus_testkit::fixture_json("tag_filter_tooltips.json");
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(Store::open(dir.path()).unwrap());
    let windows = headless::init();
    store
        .write(|ctx| {
            hydrus_store::settings::set(ctx.conn(), &hydrus_store::settings::AdvancedMode(true))
        })
        .unwrap();
    let slot = Rc::new(RefCell::new(None));
    let w = tag_filter_window::open(
        &store,
        &TagFilter::new().with_rule("goblin", FilterRule::Blacklist),
        true,
        "blacklist",
        "",
        &slot,
        Rc::new(|_| {}),
    )
    .unwrap();
    w.show().unwrap();
    assert!(w.get_show_other_panels());
    // (Qt wraps its tooltips at 80 characters; Slint wraps its own)
    assert_eq!(
        w.get_show_other_panels_tooltip().replace('\n', " "),
        f["show_other_panels"].as_str().unwrap().replace('\n', " ")
    );
    let adapter = (0..100)
        .take_while(|&n| windows.get(n).is_some())
        .last()
        .map(|n| windows.get(n).unwrap())
        .unwrap();
    let size = (760, 720);
    // where the button is: what appears when it is offered
    let with = headless::render(&adapter, size.0, size.1);
    w.set_show_other_panels(false);
    let without = headless::render(&adapter, size.0, size.1);
    w.set_show_other_panels(true);
    let first = with
        .chunks(4)
        .zip(without.chunks(4))
        .position(|(a, b)| a != b)
        .unwrap();
    let at = (
        (first % size.0 as usize) as f32 + 40.0,
        (first / size.0 as usize) as f32 + 10.0,
    );
    // the tooltip is drawn, and what is drawn is what it says
    let shown = picture_after_resting(&w, &adapter, size, at);
    w.set_show_other_panels_tooltip("another text".into());
    let other = picture_after_resting(&w, &adapter, size, at);
    assert_ne!(shown, other);
    w.set_show_other_panels_tooltip("".into());
    let none = picture_after_resting(&w, &adapter, size, at);
    assert_ne!(shown, none);
    // and away from the button there is none
    w.set_show_other_panels_tooltip(f["show_other_panels"].as_str().unwrap().into());
    let elsewhere = picture_after_resting(&w, &adapter, size, (6.0, 700.0));
    w.set_show_other_panels_tooltip("another text".into());
    assert_eq!(
        elsewhere,
        picture_after_resting(&w, &adapter, size, (6.0, 700.0))
    );
}
