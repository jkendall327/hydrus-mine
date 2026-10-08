//! The preview window's top-right hover against the reference's
//! `CanvasPanel._DrawTopRight` and `CanvasHoverFrameTopRight`: ratings at the
//! preview window's icon sizes, and the two switches for drawing it in the
//! background and for popping it in on mouseover. Read from the reference's
//! source (there is no Qt in the sandbox to record with).

use slint::platform::{PointerEventButton, WindowEvent};
use slint::{ComponentHandle as _, Model as _};

use hydrus_core::HashId;
use hydrus_gui::headless;
use hydrus_store::settings::RatingContextSizes;

use crate::options_gui_support::{Client, box_of, row, show_page};

fn first_file(client: &Client) -> HashId {
    client.ui.invoke_search_edited("system:everything".into());
    client.ui.invoke_search_accepted();
    client.bound.current.borrow().borrow().results()[0]
}

/// Show the first file in the preview, with its pane laid out.
fn preview_first(client: &Client) -> HashId {
    let file = first_file(client);
    client.ui.show().unwrap();
    let native = client.native();
    for _ in 0..3 {
        headless::render(&native, 1000, 900);
    }
    client.bound.preview.refresh();
    client.ui.invoke_thumbnail_clicked(0, false, false);
    let started = std::time::Instant::now();
    while client.ui.get_preview_media().size().width == 0 {
        client.bound.preview.refresh();
        assert!(started.elapsed() < std::time::Duration::from_secs(5));
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    client.bound.preview.refresh();
    headless::render(&native, 1000, 900);
    file
}

fn set_options(client: &Client, edits: &[(&str, &str)], checks: &[(&str, bool)]) {
    let options = client.open_options();
    for (page, label, text) in edits.iter().map(|(label, text)| ("ratings", *label, *text)) {
        show_page(&options, page);
        let (at, _) = row(&options, label);
        options.invoke_text_edited(at, text.into());
    }
    for (label, on) in checks {
        show_page(&options, "media viewer hovers");
        let (at, _) = row(&options, label);
        options.invoke_check_toggled(at, *on);
    }
    options.invoke_apply();
    client.bound.preview.refresh();
    headless::render(&client.native(), 1000, 900);
}

fn pointer_to(client: &Client, x: f32, y: f32) {
    client
        .ui
        .window()
        .dispatch_event(WindowEvent::PointerMoved {
            position: slint::LogicalPosition::new(x, y),
        });
    headless::render(&client.native(), 1000, 900);
}

const SIZE: &str = "Preview window like/dislike and numerical rating icon size:";
const HEIGHT: &str = "Preview window inc/dec rating icon height:";
const DRAW: &str = "Draw ratings and locations (top-right) in preview window background: ";
const POP_IN: &str = "Pop-in this hover on mouseover: ";

// leaf: audit-options-ratings-preview-window-preview-window-like-dislike-and-numerical-rating-icon-size
// leaf: audit-options-ratings-preview-window-preview-window-inc-dec-rating-icon-height
#[test]
#[allow(clippy::float_cmp)] // (sizes set, not computed)
fn the_preview_window_draws_its_ratings_at_the_sizes_the_options_say() {
    let client = Client::basic();
    // the "preview window" box of the ratings page: the reference's labels,
    // positions, ranges (1 to 255 and 2 to 255) and defaults
    let options = client.open_options();
    show_page(&options, "ratings");
    for label in [SIZE, HEIGHT] {
        let (_, r) = row(&options, label);
        assert_eq!((r.kind, r.text.as_str()), (4, "12.0"), "{label}");
        assert_eq!(box_of(&options, label), "preview window");
    }
    options.invoke_cancel();

    preview_first(&client);
    let ui = &client.ui;
    let kinds: Vec<i32> = ui
        .get_preview_ratings()
        .iter()
        .map(|r| r.graphic.kind)
        .collect();
    assert!(kinds.len() >= 3, "the likes, the stars and the counter");
    let likes = kinds.iter().filter(|k| **k == 0).count() as f32;
    assert!(likes >= 1.0);
    // what is drawn: the likes along one row (each with the 2 pixels after
    // it), the counter a rectangle of twice its height (and 1 pixel after)
    let drawn = |ui: &hydrus_gui::MainWindow| {
        headless::render(&client.native(), 1000, 900);
        (
            ui.get_preview_likes_width(),
            ui.get_preview_incdecs_width(),
            ui.get_preview_incdecs_height(),
        )
    };
    let (likes_width, incdec_width, incdec_height) = drawn(ui);
    assert_eq!(
        (likes_width, incdec_width, incdec_height),
        (likes * 14.0, 25.0, 12.0),
        "the reference's defaults"
    );

    // the background draw rounds both sizes (`round( GetFloat(...) )`, half
    // to even)...
    set_options(&client, &[(SIZE, "30"), (HEIGHT, "17.5")], &[]);
    let saved = client.setting::<RatingContextSizes>();
    assert_eq!(
        (saved.preview_icon_size, saved.preview_incdec_height),
        (30.0, 17.5)
    );
    assert_eq!(drawn(ui), (likes * 32.0, 37.0, 18.0));
    // ...and the popped-in hover cuts them off (`GetIconSize`'s `int()`):
    // 17.5 is 18 drawn and 17 popped
    let (x, y) = (ui.get_preview_hover_x(), ui.get_preview_hover_y());
    let w = ui.get_preview_hover_width();
    pointer_to(&client, x + w - 3.0, y + 3.0);
    assert!(ui.get_preview_hover_popped());
    assert_eq!(drawn(ui), (likes * 32.0, 35.0, 17.0));
    pointer_to(&client, x - 100.0, y + ui.get_preview_hover_height() + 40.0);
    // half to even: 16.5 draws as 16
    set_options(&client, &[(HEIGHT, "16.5")], &[]);
    assert_eq!(drawn(ui).2, 16.0);

    // a value outside the spin box's range is taken at its edge (as the
    // dialogs' rating size spin boxes are)
    set_options(&client, &[(SIZE, "0.5")], &[]);
    assert_eq!(
        client.setting::<RatingContextSizes>().preview_icon_size,
        1.0
    );
    set_options(&client, &[(SIZE, "300"), (HEIGHT, "1")], &[]);
    let saved = client.setting::<RatingContextSizes>();
    assert_eq!(
        (saved.preview_icon_size, saved.preview_incdec_height),
        (255.0, 2.0)
    );
}

// leaf: audit-options-media-viewer-hovers-preview-window-hovers-draw-ratings-and-locations-top-right-in-preview-window-background
// leaf: audit-options-media-viewer-hovers-preview-window-hovers-pop-in-this-hover-on-mouseover
#[test]
fn the_top_right_hover_draws_in_the_background_and_pops_in_as_the_options_say() {
    let client = Client::basic();
    let options = client.open_options();
    show_page(&options, "media viewer hovers");
    for label in [DRAW, POP_IN] {
        let (_, r) = row(&options, label);
        assert_eq!((r.kind, r.checked), (1, true), "on by default: {label}");
        assert_eq!(box_of(&options, label), "preview window hovers");
    }
    options.invoke_cancel();

    let file = preview_first(&client);
    let ui = &client.ui;
    assert!(ui.get_preview_draw_top_right());
    assert!(ui.get_preview_pop_in());
    // the file's locations are drawn with its ratings
    assert!(ui.get_preview_lines().row_count() > 0, "its locations");
    let (x, y) = (ui.get_preview_hover_x(), ui.get_preview_hover_y());
    let (w, h) = (ui.get_preview_hover_width(), ui.get_preview_hover_height());
    assert!(w > 0.0 && h > 0.0);

    // away from the corner it is only drawn; over it, the hover pops in
    pointer_to(&client, x - 100.0, y + h + 20.0);
    assert!(!ui.get_preview_hover_popped());
    pointer_to(&client, x + w / 2.0, y + 3.0);
    assert!(ui.get_preview_hover_popped());

    // clicking a rating in the popped-in hover sets it, as in the viewer
    let like = hydrus_gui_model::ratings::controls(&client.store, file)
        .into_iter()
        .find(|c| matches!(c.kind, hydrus_gui_model::ratings::Kind::Like { .. }))
        .expect("a like/dislike service");
    let hydrus_gui_model::ratings::Kind::Like { state: before, .. } = like.kind else {
        unreachable!()
    };
    let like_row = i32::try_from(
        ui.get_preview_ratings()
            .iter()
            .position(|r| r.graphic.kind == 0)
            .unwrap(),
    )
    .unwrap();
    ui.invoke_preview_rating_clicked(like_row, true, 0.5);
    let after = hydrus_gui_model::ratings::controls(&client.store, file)
        .into_iter()
        .find(|c| c.service == like.service)
        .unwrap();
    let hydrus_gui_model::ratings::Kind::Like { state: after, .. } = after.kind else {
        unreachable!()
    };
    assert_ne!(before, after, "the click set the like");

    // pop-in off: over the corner nothing pops in; drawn still
    set_options(&client, &[], &[(POP_IN, false)]);
    pointer_to(&client, x - 100.0, y + h + 20.0);
    pointer_to(&client, x + w / 2.0, y + 3.0);
    assert!(!ui.get_preview_pop_in());
    assert!(!ui.get_preview_hover_popped());
    assert!(ui.get_preview_draw_top_right());
    // drawing off as well: nothing of it is drawn
    set_options(&client, &[], &[(DRAW, false)]);
    assert!(!ui.get_preview_draw_top_right());
    // and on again: pop-in alone shows it
    set_options(&client, &[], &[(POP_IN, true)]);
    pointer_to(&client, x - 100.0, y + h + 20.0);
    pointer_to(&client, x + w / 2.0, y + 3.0);
    assert!(ui.get_preview_hover_popped());
    let _ = PointerEventButton::Left;
}
