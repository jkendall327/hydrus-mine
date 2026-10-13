//! A bitmap on the clipboard, and PNG files dropped, in the downloader
//! import window and the subscriptions list, against
//! `oracle/record_exchange_bitmap_drop.py` (the reference's
//! `ReviewDownloaderImport` and subscription list panel, driven with a real
//! bitmap on the Qt clipboard and real dropped paths).

use std::path::PathBuf;

use hydrus_gui::{
    ClipboardImage, DownloaderExchangeWindow, MainWindow, Pages, bind, deliver_dropped_files,
    downloader_interchange_window as windows, headless,
};
use hydrus_parse::Downloaders;
use serde_json::Value;
use slint::ComponentHandle as _;

use crate::subscriptions::{asked, open_dialog, rows, store};

fn reference() -> Value {
    hydrus_testkit::fixture_json("exchange_bitmap_drop.json")
}

fn fixtures() -> PathBuf {
    hydrus_testkit::fixtures_dir()
}

/// The text the reference said, with the paths it named made ours.
fn said(text: &Value) -> String {
    text.as_str()
        .unwrap()
        .replace("{dir}", fixtures().to_str().unwrap())
}

fn path_of(name: &str) -> String {
    fixtures().join(name).to_string_lossy().into_owned()
}

/// The pixels of a fixture PNG as the clipboard holds an image: RGBA, grey
/// values in every colour channel, as `kind` says.
fn bitmap(name: &str, kind: &str) -> ClipboardImage {
    let file = std::fs::File::open(fixtures().join(name)).unwrap();
    let mut decoder = png::Decoder::new(std::io::BufReader::new(file));
    decoder.set_transformations(png::Transformations::EXPAND);
    let mut reader = decoder.read_info().unwrap();
    let mut pixels = vec![0; reader.output_buffer_size().unwrap()];
    let info = reader.next_frame(&mut pixels).unwrap();
    let channels = info.color_type.samples();
    let rgba: Vec<u8> = pixels[..info.buffer_size()]
        .chunks_exact(channels)
        .flat_map(|p| match (channels, kind) {
            (1, _) => [p[0], p[0], p[0], 255],
            (3, _) => [p[0], p[1], p[2], 255],
            _ => [p[0], p[1], p[2], p[3]],
        })
        .collect();
    ClipboardImage {
        width: info.width as usize,
        height: info.height as usize,
        rgba,
    }
}

fn gug_names(store: &hydrus_store::Store) -> Vec<String> {
    let mut names: Vec<String> = store
        .read(hydrus_store::settings::get::<Downloaders>)
        .unwrap()
        .gugs
        .gugs
        .iter()
        .map(|g| g.name().to_owned())
        .collect();
    names.sort();
    names
}

/// Accept the reviewed package, answering the reference's question yes.
fn import(window: &DownloaderExchangeWindow) -> (String, String) {
    window.invoke_action("accept".into());
    assert!(window.get_question_asks(), "{}", window.get_error());
    let question = window.get_question().to_string();
    window.invoke_action("yes".into());
    let notice = window.get_question().to_string();
    window.invoke_action("ok".into());
    (question, notice)
}

// leaf: exchange-bitmap-drop
#[test]
fn downloader_import_takes_a_clipboard_bitmap_and_dropped_pngs_like_the_reference() {
    let _windows = headless::init();
    let reference = reference();
    let cases = &reference["downloader_import"];
    for (kind, case) in [
        ("grey", "paste_grey"),
        ("rgb", "paste_rgb"),
        ("rgba", "paste_rgba"),
    ] {
        let (_dirs, store) = store();
        let slots = windows::Slots::default();
        let window = windows::package(&store, &slots, true).unwrap();
        let image = bitmap("exchange_bitmap_package.png", kind);
        hydrus_gui::set_clipboard_image_reader(move || Some(image.clone()));
        window.invoke_action("paste".into());
        assert!(
            window.get_error().is_empty(),
            "{kind}: {}",
            window.get_error()
        );
        assert!(window.get_ready(), "{kind}");
        assert!(
            gug_names(&store).is_empty(),
            "nothing is added before the question"
        );
        let (question, notice) = import(&window);
        assert_eq!(question, cases[case]["questions"][0]["message"], "{kind}");
        assert_eq!(notice, cases[case]["messages"][0][1], "{kind}");
        assert_eq!(
            gug_names(&store),
            cases[case]["gugs"]
                .as_array()
                .unwrap()
                .iter()
                .map(|n| n.as_str().unwrap().to_owned())
                .collect::<Vec<_>>()
        );
    }
    // an image that is not a hydrus PNG says why, and adds nothing
    {
        let (_dirs, store) = store();
        let slots = windows::Slots::default();
        let window = windows::package(&store, &slots, true).unwrap();
        let image = bitmap("exchange_bitmap_plain.png", "rgb");
        hydrus_gui::set_clipboard_image_reader(move || Some(image.clone()));
        window.invoke_action("paste".into());
        assert_eq!(
            window.get_error(),
            cases["paste_plain"]["messages"][0][2].as_str().unwrap()
        );
        assert!(!window.get_ready());
        assert!(gug_names(&store).is_empty());
        // no bitmap: the clipboard's text, as before
        hydrus_gui::set_clipboard_image_reader(|| None);
        hydrus_gui::set_paster(|| "not a package".into());
        window.invoke_action("paste".into());
        assert_eq!(window.get_text(), "not a package");
    }
    // dropped files
    let drops = [
        ("drop_png", vec![path_of("exchange_bitmap_package.png")]),
        ("drop_json", vec![path_of("exchange_bitmap_package.json")]),
        ("drop_plain_png", vec![path_of("exchange_bitmap_plain.png")]),
        (
            "drop_missing_png",
            vec![path_of("exchange_bitmap_missing.png")],
        ),
        (
            "drop_bad_then_good",
            vec![
                path_of("exchange_bitmap_plain.png"),
                path_of("exchange_bitmap_package.png"),
            ],
        ),
    ];
    hydrus_gui::set_clipboard_image_reader(|| None);
    for (case, paths) in drops {
        let (_dirs, store) = store();
        let slots = windows::Slots::default();
        let window = windows::package(&store, &slots, true).unwrap();
        assert!(
            deliver_dropped_files(window.window(), paths),
            "the window takes drops"
        );
        let expected = &cases[case];
        let messages = expected["messages"].as_array().unwrap();
        if messages[0][0] == "critical" {
            let text = said(&messages[0][2]);
            if case == "drop_missing_png" {
                // (the operating system words a missing file its own way)
                assert!(
                    window.get_error().starts_with(
                        "Sorry, there was a problem importing those paths! Error was: "
                    ),
                    "{}",
                    window.get_error()
                );
                assert!(window.get_error().contains("No such file or directory"));
            } else {
                assert_eq!(window.get_error(), text, "{case}");
            }
            assert!(!window.get_ready(), "{case}");
            assert!(gug_names(&store).is_empty(), "{case}");
        } else {
            assert!(
                window.get_error().is_empty(),
                "{case}: {}",
                window.get_error()
            );
            let (question, notice) = import(&window);
            assert_eq!(question, expected["questions"][0]["message"], "{case}");
            assert_eq!(notice, messages[0][1], "{case}");
            assert_eq!(gug_names(&store), ["bitmap gallery"], "{case}");
        }
    }
    // a PNG is told by its bytes, not its extension
    let (_dirs, store) = store();
    let slots = windows::Slots::default();
    let window = windows::package(&store, &slots, true).unwrap();
    let directory = tempfile::tempdir().unwrap();
    let upper = directory.path().join("UPPER.PNG");
    std::fs::copy(fixtures().join("exchange_bitmap_package.png"), &upper).unwrap();
    assert!(deliver_dropped_files(
        window.window(),
        vec![upper.to_string_lossy().into_owned()]
    ));
    let (question, _) = import(&window);
    assert_eq!(
        question,
        cases["drop_upper_extension"]["questions"][0]["message"]
    );
    assert_eq!(gug_names(&store), ["bitmap gallery"]);
    hydrus_gui::set_clipboard_image_reader(|| None);
}

fn names(dialog: &hydrus_gui::SubscriptionsWindow) -> Vec<String> {
    let mut names: Vec<String> = rows(dialog)
        .into_iter()
        .map(|(cells, _)| cells[0].clone())
        .collect();
    names.sort();
    names
}

fn expected_names(case: &Value) -> Vec<String> {
    case["names"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| n.as_str().unwrap().to_owned())
        .collect()
}

/// Answer each notice in turn, noting what it said.
fn notices(dialog: &hydrus_gui::SubscriptionsWindow) -> Vec<(String, String)> {
    let mut seen = Vec::new();
    while dialog.get_asking() {
        let (title, message, _) = asked(dialog);
        seen.push((title, message));
        dialog.invoke_chosen(0);
    }
    seen
}

// leaf: subscriptions-exchange-bitmap-drop
#[test]
fn subscriptions_list_takes_a_clipboard_bitmap_and_dropped_pngs_like_the_reference() {
    let _windows = headless::init();
    let reference = reference();
    let cases = &reference["subscription_list"];
    // the clipboard's bitmap, whatever its channels
    for (kind, case) in [
        ("grey", "clipboard_grey"),
        ("rgb", "clipboard_rgb"),
        ("rgba", "clipboard_rgba"),
    ] {
        let (_dirs, store) = store();
        let ui = MainWindow::new().unwrap();
        let bound = bind(&ui, Pages::open(store.clone()).unwrap());
        let dialog = open_dialog(&ui, &bound);
        let image = bitmap("exchange_bitmap_subscriptions.png", kind);
        hydrus_gui::set_clipboard_image_reader(move || Some(image.clone()));
        dialog.invoke_exchange_mode(3);
        let seen = notices(&dialog);
        assert_eq!(seen.len(), 1, "{kind}: {seen:?}");
        assert_eq!(seen[0].1, cases[case]["messages"][0][1], "{kind}");
        assert_eq!(names(&dialog), expected_names(&cases[case]), "{kind}");
        dialog.invoke_cancel();
    }
    // an image that is not a hydrus PNG
    {
        let (_dirs, store) = store();
        let ui = MainWindow::new().unwrap();
        let bound = bind(&ui, Pages::open(store.clone()).unwrap());
        let dialog = open_dialog(&ui, &bound);
        let image = bitmap("exchange_bitmap_plain.png", "rgb");
        hydrus_gui::set_clipboard_image_reader(move || Some(image.clone()));
        dialog.invoke_exchange_mode(3);
        let expected = &cases["clipboard_plain"]["messages"][0];
        assert_eq!(
            notices(&dialog),
            [(
                expected[1].as_str().unwrap().to_owned(),
                expected[2].as_str().unwrap().to_owned()
            )]
        );
        assert!(names(&dialog).is_empty());
        dialog.invoke_cancel();
    }
    hydrus_gui::set_clipboard_image_reader(|| None);
    // dropped files: asked first, PNGs before JSON, the reference's messages
    let drops = [
        (
            "drop_png_declined",
            vec!["exchange_bitmap_subscriptions.png"],
            false,
        ),
        ("drop_png", vec!["exchange_bitmap_subscriptions.png"], true),
        (
            "drop_json",
            vec!["exchange_bitmap_subscriptions.json"],
            true,
        ),
        (
            "drop_png_and_json",
            vec![
                "exchange_bitmap_subscriptions.json",
                "exchange_bitmap_subscriptions.png",
            ],
            true,
        ),
        ("drop_plain_png", vec!["exchange_bitmap_plain.png"], true),
        (
            "drop_bad_then_good",
            vec![
                "exchange_bitmap_plain.png",
                "exchange_bitmap_subscriptions.png",
            ],
            true,
        ),
        (
            "drop_missing_png",
            vec!["exchange_bitmap_missing.png"],
            true,
        ),
    ];
    for (case, files, yes) in drops {
        let (_dirs, store) = store();
        let ui = MainWindow::new().unwrap();
        let bound = bind(&ui, Pages::open(store.clone()).unwrap());
        let dialog = open_dialog(&ui, &bound);
        let expected = &cases[case];
        assert!(deliver_dropped_files(
            dialog.window(),
            files.iter().map(|f| path_of(f)).collect()
        ));
        let (title, message, choices) = asked(&dialog);
        assert_eq!(title, expected["questions"][0]["title"], "{case}");
        assert_eq!(message, expected["questions"][0]["message"], "{case}");
        assert_eq!(choices, ["yes", "no"], "{case}");
        assert!(
            names(&dialog).is_empty(),
            "nothing is added before the answer"
        );
        dialog.invoke_chosen(if yes { 0 } else { 1 });
        let seen = notices(&dialog);
        let messages: Vec<&Value> = expected["messages"].as_array().unwrap().iter().collect();
        assert_eq!(seen.len(), messages.len(), "{case}: {seen:?}");
        for (seen, message) in seen.iter().zip(&messages) {
            if case == "drop_missing_png" {
                // (the operating system words a missing file its own way)
                assert_eq!(seen.0, message[1].as_str().unwrap());
                assert!(seen.1.contains("No such file or directory"), "{}", seen.1);
            } else if message[0] == "critical" {
                assert_eq!(
                    (seen.0.as_str(), seen.1.as_str()),
                    (message[1].as_str().unwrap(), message[2].as_str().unwrap()),
                    "{case}"
                );
            } else {
                assert_eq!(seen.1, message[1].as_str().unwrap(), "{case}");
            }
        }
        assert_eq!(names(&dialog), expected_names(expected), "{case}");
        dialog.invoke_cancel();
    }
    // a drop is not taken while the list asks something else
    let (_dirs, store) = store();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    let dialog = open_dialog(&ui, &bound);
    let image = bitmap("exchange_bitmap_plain.png", "rgb");
    hydrus_gui::set_clipboard_image_reader(move || Some(image.clone()));
    dialog.invoke_exchange_mode(3);
    let before = asked(&dialog);
    deliver_dropped_files(
        dialog.window(),
        vec![path_of("exchange_bitmap_subscriptions.png")],
    );
    assert_eq!(asked(&dialog), before);
    hydrus_gui::set_clipboard_image_reader(|| None);
}
