//! Help > debug > what is this object?, clicked in the real menu and fed as
//! the reference's window was (`oracle/fixtures/what_is_this_object.json`,
//! recorded by `oracle/record_what_is_this_object.py`): the window, paste
//! from the clipboard as text, files or a bitmap, the file chooser and drops.
use super::debug_menu_actions::start;
use regex::Regex;
use serde_json::Value;
use std::path::PathBuf;
use std::rc::Rc;

fn recording() -> Value {
    hydrus_testkit::fixture_json("what_is_this_object.json")
}

fn strings(value: &Value) -> Vec<String> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_owned())
        .collect()
}

/// The text the clipboard holds, for the window's paste.
fn clipboard_text(text: &str) {
    let text = text.to_owned();
    hydrus_gui::set_clipboard_reader(move || Ok(Some(text.clone())));
    hydrus_gui::set_clipboard_paths_reader(Vec::new);
    hydrus_gui::set_clipboard_image_reader(|| None);
}

fn clipboard_paths(paths: &[PathBuf]) {
    let paths = paths.to_vec();
    hydrus_gui::set_clipboard_reader(|| Ok(None));
    hydrus_gui::set_clipboard_paths_reader(move || paths.clone());
    hydrus_gui::set_clipboard_image_reader(|| None);
}

fn dismiss_problem() -> (String, String) {
    let window = hydrus_gui::message_window().expect("a problem is shown");
    let said = (
        window.get_window_title().to_string(),
        window.get_message().to_string(),
    );
    window.invoke_cancelled();
    said
}

// leaf: audit-options-help-debug-action-what-is-this-object
#[test]
fn what_is_this_object_opens_the_reference_s_window_and_describes_what_is_pasted() {
    let d = start();
    let recording = recording();
    assert_eq!(
        strings(&recording["menu_path"]),
        ["help", "debug", "what is this object?"]
    );
    d.click(&["what is this object?"]);
    let windows = hydrus_gui::what_is_this_window::windows();
    assert_eq!(windows.len(), 1);
    let window = &windows[0];
    // the recorded frame: its title, its two boxes, empty, and the paste button's tooltip
    assert_eq!(
        window.get_window_title(),
        recording["window"]["title"].as_str().unwrap()
    );
    assert_eq!(recording["window"]["edits"], 2);
    assert_eq!(window.get_description(), "");
    assert_eq!(window.get_json_content(), "");
    assert_eq!(
        window.get_paste_tooltip(),
        recording["window"]["paste_tooltip"].as_str().unwrap()
    );
    // the constructed objects, pasted as text
    for (name, case) in recording["objects"].as_object().unwrap() {
        let payload = case["payloads"][0].as_str().unwrap();
        clipboard_text(payload);
        window.invoke_paste();
        assert_eq!(
            window.get_description(),
            case["description"].as_str().unwrap(),
            "{name}"
        );
        assert_eq!(window.get_json_content(), payload, "{name}");
        assert_eq!(window.get_notice(), "Pasted 1 items!", "{name}");
    }
    // and what is not one
    for (name, case) in recording["errors"].as_object().unwrap() {
        let payloads = strings(&case["payloads"]);
        let Some(text) = payloads.first().filter(|_| payloads.len() == 1) else {
            continue;
        };
        clipboard_text(text);
        window.invoke_paste();
        assert_eq!(
            window.get_description(),
            case["description"].as_str().unwrap(),
            "{name}"
        );
    }
}

// leaf: audit-options-help-debug-action-what-is-this-object
#[test]
fn pasting_files_and_dropping_them_go_through_the_same_ingest_as_the_reference() {
    let d = start();
    let recording = recording();
    d.click(&["what is this object?"]);
    let window = hydrus_gui::what_is_this_window::windows().remove(0);
    let dir = tempfile::tempdir().unwrap();
    let list_text = recording["paste_payload"].as_str().unwrap().to_owned();
    let json_path = dir.path().join("object.json");
    std::fs::write(&json_path, &list_text).unwrap();
    let png_path = dir.path().join("object.png");
    std::fs::write(
        &png_path,
        hydrus_downloader_exchange::text_png::encode(&list_text, 64, &[255; 64]).unwrap(),
    )
    .unwrap();
    let pasted = |case: &str| -> Value {
        recording["pastes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|p| p["case"] == case)
            .unwrap()
            .clone()
    };
    let check = |case: &str, window: &hydrus_gui::WhatIsThisWindow| {
        let recorded = pasted(case);
        assert_eq!(
            window.get_description(),
            recorded["description"].as_str().unwrap(),
            "{case}"
        );
        let micro = strings(&recorded["micro"]);
        assert_eq!(
            window.get_notice().to_string(),
            micro.first().cloned().unwrap_or_default(),
            "{case}"
        );
    };
    for (case, paths) in [
        ("url to a json file", vec![json_path.clone()]),
        ("url to a png", vec![png_path.clone()]),
        ("two urls", vec![json_path.clone(), png_path.clone()]),
    ] {
        window.set_description("".into());
        window.set_notice("".into());
        clipboard_paths(&paths);
        window.invoke_paste();
        check(case, &window);
    }
    // a path as text is only text, and garbage is an error
    for (case, text) in [
        ("path to a json file", json_path.to_string_lossy().into_owned()),
        ("garbage text", "garbage".to_owned()),
        ("json text", list_text.clone()),
    ] {
        clipboard_text(&text);
        window.invoke_paste();
        check(case, &window);
    }
    // a path that is not there is a problem, not an import
    window.set_description("".into());
    window.set_notice("".into());
    clipboard_paths(&[dir.path().join("missing.json")]);
    window.invoke_paste();
    let (title, message) = dismiss_problem();
    let recorded = &pasted("missing path")["critical"][0];
    assert_eq!(title, recorded["title"].as_str().unwrap());
    let anywhere = Regex::new(r"'[^']*missing\.json'").unwrap();
    assert_eq!(
        anywhere.replace(&message, "'<path>'"),
        anywhere.replace(recorded["message"].as_str().unwrap(), "'<path>'")
    );
    assert_eq!(window.get_description(), "");
    assert_eq!(window.get_notice(), "");
    // dropped files and the png chooser are the paths' ingest, with their errors
    hydrus_gui::what_is_this_window::drop_paths(&window, &[json_path]);
    assert_eq!(
        window.get_description(),
        recording["import_paths_json"]["description"].as_str().unwrap()
    );
    let asked = Rc::new(std::cell::RefCell::new(Vec::new()));
    hydrus_gui::set_picker({
        let asked = asked.clone();
        let png = png_path.clone();
        move |kind, title| {
            asked.borrow_mut().push((kind, title.to_owned()));
            vec![png.clone()]
        }
    });
    window.set_description("".into());
    window.invoke_picture_clicked();
    assert_eq!(
        *asked.borrow(),
        [(hydrus_gui::Pick::Files, "Select the pngs to add.".to_owned())]
    );
    assert_eq!(
        window.get_description(),
        recording["import_paths_png"]["description"].as_str().unwrap()
    );
    let binary = dir.path().join("binary.json");
    std::fs::write(&binary, b"\xff\xfe\x00bad").unwrap();
    hydrus_gui::what_is_this_window::drop_paths(&window, &[binary]);
    let (_, message) = dismiss_problem();
    let quoted = Regex::new(r#""[^"]*binary\.json""#).unwrap();
    assert_eq!(
        quoted.replace(&message, "\"<path>\""),
        quoted.replace(
            recording["import_paths_not_utf8"]["critical"][0]["message"].as_str().unwrap(),
            "\"<path>\""
        )
    );
}
