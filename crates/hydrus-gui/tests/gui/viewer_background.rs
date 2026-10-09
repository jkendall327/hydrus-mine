//! The media viewer's passive background copies of its hovers (Options >
//! media viewer hovers > "Draw ... in the viewer background"), replaying
//! `oracle/fixtures/viewer_background_options.json`
//! (`oracle/record_viewer_background_options.py`: the reference's
//! `_DrawBackgroundDetails` painted for every combination of the four rows,
//! with each `drawText` call's text and rectangle recorded). The real viewer
//! paints on a 1000x750 canvas; what each copy says is compared with what the
//! reference wrote, and where its ink lands with the reference's rectangles.
use super::options_window::{open, row, show_page, store};
use hydrus_gui::{MainWindow, Pages, bind, headless};
use hydrus_store::settings::{self, ViewerBackgroundSettings};
use serde_json::Value;
use slint::{ComponentHandle as _, Model as _};

const ROWS: [&str; 4] = [
    "Draw tags (left) in the viewer background:",
    "Draw file information (top) in the viewer background:",
    "Draw ratings and locations (top-right) in the viewer background:",
    "Draw notes (right) in the viewer background:",
];
const HOVERS: [&str; 3] = [
    "Pop-in tags (left) hover window on mouseover:",
    "Pop-in ratings and locations (top-right) hover window on mouseover:",
    "Pop-in notes (right) hover window on mouseover:",
];
const KINDS: [&str; 4] = ["Tags", "TopMiddle", "TopRight", "Notes"];

/// How far (px) native ink may sit from the reference's text rectangles: the
/// fonts differ (Qt's and Slint's metrics), the places do not.
const SLACK: f64 = 8.0;

#[derive(Debug, Clone, Copy)]
struct Box {
    x0: f64,
    y0: f64,
    x1: f64,
    y1: f64,
}

/// The reference's text rectangles of one kind, together.
fn recorded_box(draws: &[Value], kind: &str) -> Option<Box> {
    draws
        .iter()
        .filter(|d| d["kind"] == kind)
        .map(|d| {
            let r: Vec<f64> = d["rect"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_f64().unwrap())
                .collect();
            Box {
                x0: r[0],
                y0: r[1],
                x1: r[0] + r[2],
                y1: r[1] + r[3],
            }
        })
        .reduce(|a, b| Box {
            x0: a.x0.min(b.x0),
            y0: a.y0.min(b.y0),
            x1: a.x1.max(b.x1),
            y1: a.y1.max(b.y1),
        })
}

/// Where a render has ink (anything but the canvas colour).
fn ink(pixels: &[u8], width: usize) -> Option<Box> {
    let mut found: Option<Box> = None;
    for (i, pixel) in pixels.chunks_exact(4).enumerate() {
        if pixel[..3] == [32, 32, 32] {
            continue;
        }
        #[allow(clippy::cast_precision_loss)]
        let (x, y) = ((i % width) as f64, (i / width) as f64);
        found = Some(match found {
            None => Box {
                x0: x,
                y0: y,
                x1: x + 1.0,
                y1: y + 1.0,
            },
            Some(b) => Box {
                x0: b.x0.min(x),
                y0: b.y0.min(y),
                x1: b.x1.max(x + 1.0),
                y1: b.y1.max(y + 1.0),
            },
        });
    }
    found
}

fn texts(draws: &[Value], kind: &str) -> Vec<String> {
    draws
        .iter()
        .filter(|d| d["kind"] == kind)
        .map(|d| d["text"].as_str().unwrap().to_owned())
        .collect()
}

/// The file information line without its age ("imported: 4 days 18 hours
/// ago" was the recorder's clock).
fn without_age(line: &str) -> String {
    line.split(" | ")
        .map(|part| {
            part.find(": ")
                .filter(|_| part.ends_with(" ago"))
                .map_or(part, |at| &part[..at])
        })
        .collect::<Vec<_>>()
        .join(" | ")
}

fn near(a: f64, b: f64) -> bool {
    (a - b).abs() <= SLACK
}

// leaf: audit-options-media-viewer-hovers-background-draw-tags-left-in-the-viewer-background
// leaf: audit-options-media-viewer-hovers-background-draw-file-information-top-in-the-viewer-background
// leaf: audit-options-media-viewer-hovers-background-draw-ratings-and-locations-top-right-in-the-viewer-background
// leaf: audit-options-media-viewer-hovers-background-draw-notes-right-in-the-viewer-background
#[test]
#[allow(clippy::too_many_lines)]
fn background_copies_say_and_place_what_the_reference_draws() {
    let fixture = hydrus_testkit::fixture_json("viewer_background_options.json");
    let (_dirs, store) = store();
    let id = store
        .read(|conn| {
            hydrus_store::master::hash_id(conn, &fixture["hash"].as_str().unwrap().parse().unwrap())
        })
        .unwrap()
        .unwrap();
    for (name, text) in fixture["notes"].as_object().unwrap() {
        let (name, text) = (name.clone(), text.as_str().unwrap().to_owned());
        store
            .write_content(move |writer| writer.set_note(id, &name, &text))
            .unwrap();
    }
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let mut pages = Pages::single(hydrus_gui::SearchPage::new(store.clone()));
    pages.open_files(
        hydrus_search::LocationContext::single(hydrus_core::ServiceKey::new(
            hydrus_core::service::builtin_keys::HYDRUS_LOCAL_FILE_STORAGE.to_vec(),
        )),
        vec![id],
        None,
        None,
    );
    let bound = bind(&ui, pages);
    ui.invoke_thumbnail_activated(0);
    let viewer = bound.viewer.borrow().as_ref().unwrap().clone_strong();
    let drawn = windows.get(windows.count() - 1).unwrap();
    let (width, height) = (
        u32::try_from(fixture["canvas"][0].as_u64().unwrap()).unwrap(),
        u32::try_from(fixture["canvas"][1].as_u64().unwrap()).unwrap(),
    );
    headless::render(&drawn, width, height); // settle the canvas size
    // the defaults are the reference's
    open(&ui);
    let options = bound.options.borrow().as_ref().unwrap().clone_strong();
    show_page(&options, "media viewer hovers");
    for (label, initial) in ROWS.iter().zip(fixture["initial"].as_array().unwrap()) {
        assert_eq!(row(&options, label).1.checked, initial.as_bool().unwrap());
    }
    options.invoke_cancel();

    for event in fixture["events"].as_array().unwrap() {
        let values: Vec<bool> = event["values"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_bool().unwrap())
            .collect();
        open(&ui);
        let options = bound.options.borrow().as_ref().unwrap().clone_strong();
        show_page(&options, "media viewer hovers");
        for (label, value) in ROWS.iter().zip(&values) {
            options.invoke_check_toggled(row(&options, label).0, *value);
        }
        for label in HOVERS {
            options.invoke_check_toggled(
                row(&options, label).0,
                event["hovers_enabled"].as_bool().unwrap(),
            );
        }
        options.invoke_check_toggled(
            row(
                &options,
                "Draw index text (bottom-right) in the viewer background:",
            )
            .0,
            false,
        );
        options.invoke_apply();
        let saved = store
            .read(settings::get::<ViewerBackgroundSettings>)
            .unwrap();
        assert_eq!(
            vec![saved.tags, saved.information, saved.ratings, saved.notes],
            values
        );
        viewer
            .window()
            .dispatch_event(slint::platform::WindowEvent::PointerExited);
        viewer.set_media(slint::Image::default());
        viewer.set_sharp_shown(false);
        viewer.set_sharp(slint::Image::default());
        viewer.set_media_x(0.0);
        viewer.set_media_y(0.0);
        viewer.set_media_width(1000.0);
        viewer.set_media_height(750.0);
        let draws = event["draws"].as_array().unwrap();

        // what each copy says: the viewer's own rows, drawn as the reference
        // drew them
        let drawn_kinds: Vec<&str> = event["calls"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| c["kind"].as_str().unwrap())
            .collect();
        let shown = [
            viewer.get_draw_tags_background(),
            viewer.get_draw_information_background(),
            viewer.get_draw_ratings_background(),
            viewer.get_draw_notes_background(),
        ];
        for (kind, shown) in KINDS.iter().zip(shown) {
            assert_eq!(drawn_kinds.contains(kind), shown, "{kind}: {event}");
        }
        if shown[0] {
            let tags: Vec<String> = viewer.get_tags().iter().map(|t| t.text.to_string()).collect();
            assert_eq!(tags, texts(draws, "Tags"), "tags");
        }
        if shown[1] {
            let top = texts(draws, "TopMiddle");
            assert_eq!(viewer.get_tag_banner().as_str(), top[0]);
            assert_eq!(without_age(&viewer.get_background_info_line()), without_age(&top[1]));
            assert!(viewer.get_background_info_line().ends_with(" ago"));
        }
        if shown[2] {
            // the reference writes an inc/dec rating's value and the
            // locations; its stars are shapes
            let mut ours: Vec<String> = viewer
                .get_ratings()
                .iter()
                .filter(|r| r.kind == 2)
                .map(|r| r.text.to_string())
                .collect();
            ours.extend(viewer.get_location_strings().iter().map(|s| s.to_string()));
            assert_eq!(ours, texts(draws, "TopRight"), "top right");
        }
        if shown[3] {
            let notes: Vec<String> = viewer
                .get_notes()
                .iter()
                .flat_map(|n| [n.name.to_string(), n.text.to_string()])
                .collect();
            assert_eq!(notes, texts(draws, "Notes"), "notes");
        }

        // where each copy lands: with only its own row on, its ink against
        // the reference's rectangles (and the notes below the ratings)
        let pixels = headless::render_snapshot(&drawn, width, height);
        let all = ink(&pixels, width as usize);
        assert_eq!(all.is_some(), event["visible_pixels"].as_u64().unwrap() > 0, "{event}");
        if values.iter().filter(|v| **v).count() == 1 {
            let kind = KINDS[values.iter().position(|v| *v).unwrap()];
            let ours = all.unwrap();
            let theirs = recorded_box(draws, kind).unwrap();
            let fits = match kind {
                // left-aligned rows from the left edge
                "Tags" => near(ours.x0, theirs.x0) && near(ours.y0, theirs.y0),
                // centred lines at the top
                "TopMiddle" => {
                    near((ours.x0 + ours.x1) / 2.0, (theirs.x0 + theirs.x1) / 2.0)
                        && near(ours.y0, theirs.y0)
                }
                // a column at the right: the reference's rectangles are the
                // column (its texts wrap in it), ours is ink inside it
                "Notes" => {
                    near(ours.x0, theirs.x0) && ours.x1 <= theirs.x1 + SLACK && near(ours.y0, theirs.y0)
                }
                // right-aligned from the right edge, ending where the
                // reference's locations end (its rectangles are its texts:
                // the inc/dec value and the locations; the stars above them
                // are shapes it does not record)
                _ => near(ours.x1, theirs.x1) && ours.y0 <= theirs.y0 && near(ours.y1, theirs.y1),
            };
            assert!(fits, "{kind}: ours {ours:?}, the reference's {theirs:?}");
        }
        if shown[3] && shown[2] && !shown[0] && !shown[1] {
            // the notes start under the ratings, as `_DrawNotes(input_y)` does
            let calls = event["calls"].as_array().unwrap();
            let input_y = calls.iter().find(|c| c["kind"] == "Notes").unwrap()["input_y"]
                .as_f64()
                .unwrap();
            let ratings_end = calls.iter().find(|c| c["kind"] == "TopRight").unwrap()
                ["output_y"]
                .as_f64()
                .unwrap();
            assert!((input_y - ratings_end).abs() < f64::EPSILON);
            assert!(
                near(f64::from(viewer.get_background_notes_y()), input_y),
                "notes start at {} where the reference's start at {input_y}",
                viewer.get_background_notes_y()
            );
        }

        // opaque media painted after the passive text covers all of it
        let mut cover = slint::SharedPixelBuffer::<slint::Rgba8Pixel>::new(4, 3);
        cover.make_mut_slice().fill(slint::Rgba8Pixel {
            r: 32,
            g: 32,
            b: 32,
            a: 255,
        });
        viewer.set_media(slint::Image::from_rgba8(cover));
        let pixels = headless::render_snapshot(&drawn, width, height);
        assert_eq!(
            ink(&pixels, width as usize).is_some(),
            event["opaque_cover_pixels"].as_u64().unwrap() > 0,
            "media covers passive copies: {event}"
        );
    }
    viewer.invoke_close_requested();
}
