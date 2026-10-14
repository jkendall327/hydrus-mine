//! The preview window's top-right hover against the reference's
//! `CanvasPanel._DrawTopRight` and `CanvasHoverFrameTopRight`: ratings at the
//! preview window's icon sizes, and the two switches for drawing it in the
//! background and for popping it in on mouseover. The first two tests read
//! the reference's source; the third replays `oracle/record_preview_ratings.py`.

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
    preview_at(client, None)
}

/// Show a file in the preview (the first, or the one with this hash).
fn preview_at(client: &Client, wanted: Option<&str>) -> HashId {
    let first = first_file(client);
    let results = client.bound.current.borrow().borrow().results().to_vec();
    let (file, index) = match wanted {
        None => (first, 0),
        Some(hash) => {
            let hash: hydrus_core::Sha256 = hash.parse().unwrap();
            let id = client
                .store
                .read(|c| hydrus_store::master::hash_id(c, &hash))
                .unwrap()
                .unwrap();
            (id, results.iter().position(|r| *r == id).unwrap())
        }
    };
    client.ui.show().unwrap();
    let native = client.native();
    for _ in 0..3 {
        headless::render(&native, 1000, 900);
    }
    client.bound.preview.refresh();
    client
        .ui
        .invoke_thumbnail_clicked(i32::try_from(index).unwrap(), false, false);
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

/// The ratings' sizes the options say, the two switches, and where the
/// hover pops in, against the reference's real preview canvas
/// (oracle/record_preview_ratings.py).
// leaf: audit-options-ratings-preview-window-preview-window-like-dislike-and-numerical-rating-icon-size
// leaf: audit-options-ratings-preview-window-preview-window-inc-dec-rating-icon-height
#[test]
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::float_cmp
)] // (sizes of whole pixels)
fn the_preview_ratings_and_hover_are_the_references_under_the_options() {
    use hydrus_gui_model::ratings::{Kind, controls};

    let recorded: serde_json::Value = hydrus_testkit::fixture_json("preview_ratings.json");
    let client = Client::basic();
    let file = preview_first(&client);
    let set_ratings = |incdec: i64| {
        for control in controls(&client.store, file) {
            let service = control.service;
            match control.kind {
                Kind::Like { .. } => client
                    .store
                    .write_content(move |w| w.set_rating(service, &[file], Some(1.0)))
                    .unwrap(),
                Kind::Numerical { .. } => client
                    .store
                    .write_content(move |w| w.set_rating(service, &[file], Some(0.6)))
                    .unwrap(),
                Kind::IncDec { .. } => client
                    .store
                    .write_content(move |w| w.set_incdec(service, &[file], incdec))
                    .unwrap(),
            }
        }
    };
    let num = |v: &serde_json::Value| v.as_f64().unwrap() as f32;
    let mut last_incdec = -1;
    for result in recorded["results"].as_array().unwrap() {
        let incdec = result["incdec_value"].as_i64().unwrap();
        if incdec != last_incdec {
            set_ratings(incdec);
            last_incdec = incdec;
        }
        // the sizes, set in the Options window
        set_options(
            &client,
            &[
                (SIZE, &format!("{}", result["icon"].as_f64().unwrap())),
                (
                    HEIGHT,
                    &format!("{}", result["incdec_height"].as_f64().unwrap()),
                ),
            ],
            &[],
        );
        client.bound.preview.refresh();
        headless::render(&client.native(), 1000, 900);
        let what = format!(
            "icon {}, incdec {}, value {incdec}",
            result["icon"], result["incdec_height"]
        );
        let rows: Vec<_> = client.ui.get_preview_ratings().iter().collect();
        assert_eq!(rows.len(), 3, "{what}: a like, the stars and a counter");
        let calls = result["background"]["calls"].as_array().unwrap();
        let call = |kind: &str| calls.iter().find(|c| c["kind"] == kind).unwrap();
        let widget = |kind: &str| {
            result["popped"]
                .as_array()
                .unwrap()
                .iter()
                .find(|c| c["kind"] == kind)
                .unwrap()["size"]
                .clone()
        };
        for row in &rows {
            let (kind, pad) = (row.graphic.kind, row.graphic.pad);
            let (width, height) = (row.width, row.height);
            let (popup_width, popup_height) = (row.popup_width, row.popup_height);
            match kind {
                0 => {
                    let size = &call("like")["size"];
                    assert_eq!(
                        (width, height),
                        (num(&size[0]), num(&size[1])),
                        "{what}: like drawn"
                    );
                    let w = widget("like");
                    assert_eq!(
                        (popup_width + 4.0, popup_height + 4.0),
                        (num(&w[0]), num(&w[1])),
                        "{what}: like popped in"
                    );
                }
                1 => {
                    let numerical = call("numerical");
                    let reserved = calls
                        .iter()
                        .find(|c| c["kind"] == "numerical_width")
                        .unwrap();
                    let size = &numerical["size"];
                    assert_eq!(pad, num(&numerical["pad"]), "{what}: the numerical pad");
                    assert_eq!(
                        (width + pad, height),
                        (num(&reserved["width"]), num(&size[1])),
                        "{what}: numerical drawn"
                    );
                    let w = widget("numerical");
                    assert_eq!(
                        (popup_width + pad, popup_height + 4.0),
                        (num(&w[0]), num(&w[1])),
                        "{what}: numerical popped in"
                    );
                }
                _ => {
                    let size = &call("incdec")["size"];
                    assert_eq!(
                        (width, height),
                        (num(&size[0]), num(&size[1])),
                        "{what}: incdec drawn"
                    );
                    let w = widget("incdec");
                    assert_eq!(
                        (popup_width + 1.0, popup_height + 4.0),
                        (num(&w[0]), num(&w[1])),
                        "{what}: incdec popped in"
                    );
                }
            }
        }
    }
    // the options go back to the defaults for what follows
    set_options(&client, &[(SIZE, "12"), (HEIGHT, "12")], &[]);

    // the draw switch: nothing is drawn with it off
    for switch in recorded["switches"].as_array().unwrap() {
        set_options(
            &client,
            &[],
            &[
                (DRAW, switch["draw"].as_bool().unwrap()),
                (POP_IN, switch["pop_in"].as_bool().unwrap()),
            ],
        );
        assert_eq!(
            client.ui.get_preview_draw_top_right(),
            switch["calls"].as_u64().unwrap() > 0,
            "{switch}"
        );
    }

    // the pop-in switch: the points around the hover's edges that pop it in
    // are the reference's rectangle's
    let mut popped_somewhere = 0;
    for popin in recorded["popins"].as_array().unwrap() {
        let on = popin["pop_in"].as_bool().unwrap();
        set_options(&client, &[], &[(DRAW, true), (POP_IN, on)]);
        let ui = &client.ui;
        pointer_to(&client, 1.0, 1.0);
        let (x, y) = (ui.get_preview_hover_x(), ui.get_preview_hover_y());
        let (w, h) = (ui.get_preview_hover_width(), ui.get_preview_hover_height());
        let map = |d: f32, size: f32, theirs: f32| -> f32 {
            if (d - (theirs - 1.0)).abs() < f32::EPSILON {
                size - 1.0
            } else if (d - theirs).abs() < f32::EPSILON {
                size
            } else if (d - (theirs / 2.0).floor()).abs() < f32::EPSILON {
                (size / 2.0).floor()
            } else if d > theirs {
                d - theirs + size
            } else {
                d
            }
        };
        let mut points = popin["points"].as_array().unwrap().clone();
        points.dedup();
        let (ideal_w, ideal_h) = (num(&popin["ideal_size"][0]), num(&popin["ideal_size"][1]));
        for point in &points {
            let (dx, dy) = (num(&point["from_left"]), num(&point["from_top"]));
            let (px, py) = if on {
                (x + map(dx, w, ideal_w), y + map(dy, h, ideal_h))
            } else {
                (x + dx, y + dy)
            };
            pointer_to(&client, px + 0.5, py + 0.5);
            assert_eq!(
                ui.get_preview_hover_popped(),
                point["contains"].as_bool().unwrap() && on,
                "pop-in {on}: the point {dx}, {dy} from the hover's corner"
            );
            popped_somewhere += usize::from(ui.get_preview_hover_popped());
            pointer_to(&client, 1.0, 1.0);
        }
    }
    assert!(popped_somewhere >= 3, "the points inside pop it in");
    set_options(&client, &[], &[(POP_IN, true)]);
}

/// What the preview window's background draws of the top-right hover besides
/// the ratings, for the file in each of its states (inbox, archived,
/// trashed), and with the draw option turned off, against the reference's
/// `_DrawTopRight` (oracle/record_preview_background_details.py): the
/// inbox or trash icon and the location strings, in order.
// leaf: audit-options-media-viewer-hovers-preview-window-hovers-draw-ratings-and-locations-top-right-in-preview-window-background
#[test]
#[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
fn the_preview_background_draws_the_icons_and_locations_the_reference_does() {
    let recorded = hydrus_testkit::fixture_json("preview_background_details.json");
    let client = Client::basic();
    let file = preview_at(&client, recorded["file"].as_str());
    let ui = &client.ui;
    // (the file is in "my files" and in "art": it is trashed only once it is
    // deleted from both)
    let domains: Vec<_> = ["my files", "art"]
        .iter()
        .map(|name| client.store.snapshot().services.by_name(name).unwrap().id)
        .collect();
    for state in recorded["states"].as_array().unwrap() {
        let name = state["state"].as_str().unwrap();
        match name {
            "inbox" => client
                .store
                .write_content(move |w| w.inbox(&[file]))
                .unwrap(),
            "archive" => client
                .store
                .write_content(move |w| w.archive(&[file]))
                .unwrap(),
            "trash" => {
                for domain in domains.clone() {
                    client
                        .store
                        .write_content(move |w| w.delete_files(domain, &[file], Some("test")))
                        .unwrap();
                }
            }
            other => panic!("{other}"),
        }
        client.bound.preview.refresh();
        headless::render(&client.native(), 1000, 900);
        let drawn: Vec<(String, String)> = state["draws"]
            .as_array()
            .unwrap()
            .iter()
            .map(|d| {
                (
                    d["kind"].as_str().unwrap().to_owned(),
                    d["which"]
                        .as_str()
                        .or_else(|| d["text"].as_str())
                        .unwrap()
                        .to_owned(),
                )
            })
            .collect();
        let icons: Vec<&str> = drawn
            .iter()
            .filter(|(kind, _)| kind == "icon")
            .map(|(_, which)| which.as_str())
            .collect();
        // the icons: the trash, then the inbox, as the reference orders them
        let mut ours = Vec::new();
        if ui.get_preview_trashed() {
            ours.push("trash");
        }
        if ui.get_preview_inbox() {
            ours.push("inbox");
        }
        assert_eq!(ours, icons, "{name}: the icons drawn");
        // the location strings come first, then the URLs (which hydrus-rs
        // lists by its own default, DIFFERENCES.md: every class, unmatched
        // too), where the reference's default shows none of this file's
        let lines: Vec<String> = ui
            .get_preview_lines()
            .iter()
            .map(|l| l.to_string())
            .collect();
        let locations: Vec<String> = state["draws"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|d| d["kind"] == "text")
            .map(|d| d["text"].as_str().unwrap().to_owned())
            .collect();
        assert!(lines.len() >= locations.len(), "{name}: {lines:?}");
        assert_eq!(
            lines[..locations.len()],
            locations[..],
            "{name}: the locations drawn"
        );
        assert!(ui.get_preview_draw_top_right(), "{name}");
    }
    // the option off: nothing of it is drawn (the reference's draws are none)
    let off = recorded["switches"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["draw"] == false)
        .unwrap();
    assert_eq!(off["drawn"], 0);
    // what is painted in the hover's corner of the pane, drawn and not
    let painted = |client: &Client| -> Vec<u8> {
        let ui = &client.ui;
        let pixels = headless::render(&client.native(), 1000, 900);
        let (x, y) = (
            ui.get_preview_hover_x() as usize,
            ui.get_preview_hover_y() as usize,
        );
        let (w, h) = (
            ui.get_preview_hover_width() as usize,
            ui.get_preview_hover_height() as usize,
        );
        let mut out = Vec::new();
        for row in y..y + h {
            out.extend_from_slice(&pixels[(row * 1000 + x) * 4..(row * 1000 + x + w) * 4]);
        }
        out
    };
    let drawn = painted(&client);
    set_options(&client, &[], &[(DRAW, false)]);
    assert!(!ui.get_preview_draw_top_right());
    let blank = painted(&client);
    assert_ne!(drawn, blank, "the corner is painted differently with it on");
    set_options(&client, &[], &[(DRAW, true)]);
    assert!(ui.get_preview_draw_top_right());
    assert_ne!(painted(&client), blank, "and on again it is painted");
}

/// Whether the hover pops in with the mouse at points around where it pops
/// in, the switch on and off, against the reference's `DoRegularHideShow`
/// run with the mouse put there (oracle/record_preview_background_details.py).
// leaf: audit-options-media-viewer-hovers-preview-window-hovers-pop-in-this-hover-on-mouseover
#[test]
#[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
fn the_hover_pops_in_with_the_mouse_where_the_reference_does_and_never_with_the_switch_off() {
    let recorded = hydrus_testkit::fixture_json("preview_background_details.json");
    let client = Client::basic();
    preview_first(&client);
    let ui = &client.ui;
    let rect = recorded["hover_rect"].as_array().unwrap();
    let (their_w, their_h) = (
        rect[2].as_f64().unwrap() as f32,
        rect[3].as_f64().unwrap() as f32,
    );
    // a point d from the hover's corner in the reference's hover of `theirs`
    // size, in ours of `size`: the same edge, or the same distance past it
    let map = |d: f32, size: f32, theirs: f32| -> f32 {
        if (d - (theirs - 1.0)).abs() < f32::EPSILON {
            size - 1.0
        } else if (d - theirs).abs() < f32::EPSILON {
            size
        } else if (d - (theirs / 2.0).floor()).abs() < f32::EPSILON {
            (size / 2.0).floor()
        } else if d > theirs {
            d - theirs + size
        } else {
            d
        }
    };
    let mut popped_somewhere = 0;
    for decision in recorded["pop_in_decisions"].as_array().unwrap() {
        let on = decision["pop_in"].as_bool().unwrap();
        set_options(&client, &[], &[(DRAW, true), (POP_IN, on)]);
        pointer_to(&client, 1.0, 1.0);
        let (x, y) = (ui.get_preview_hover_x(), ui.get_preview_hover_y());
        let (w, h) = (ui.get_preview_hover_width(), ui.get_preview_hover_height());
        let dx = decision["from_left"].as_f64().unwrap() as f32;
        let dy = decision["from_top"].as_f64().unwrap() as f32;
        let (px, py) = (x + map(dx, w, their_w), y + map(dy, h, their_h));
        pointer_to(&client, px + 0.5, py + 0.5);
        assert_eq!(
            ui.get_preview_hover_popped(),
            decision["pops_in"].as_bool().unwrap(),
            "pop-in {on}: the mouse {dx}, {dy} from the hover's corner"
        );
        popped_somewhere += usize::from(ui.get_preview_hover_popped());
        pointer_to(&client, 1.0, 1.0);
    }
    assert_eq!(popped_somewhere, 3, "the three points inside, with it on");
    set_options(&client, &[], &[(POP_IN, true)]);
}
