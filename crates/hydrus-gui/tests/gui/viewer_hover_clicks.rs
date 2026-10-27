//! Clicks on the media viewer's notes hover and on its top-right hover's
//! inbox icon, as the reference's (`oracle/record_viewer_hover_clicks.py`:
//! a real `CanvasMediaListBrowser` over two inbox files with notes, and real
//! mouse presses on its `NotePanel`s and its inbox icon). The real viewer is
//! opened on the same files; each recorded press is made with real pointer
//! events where the note or icon is laid out, and what followed compared:
//! Manage Notes opened on that note, the note's text folded or unfolded
//! (kept while the next file has a note of that name), the note copied,
//! and the file archived.

use std::cell::RefCell;
use std::rc::Rc;

use hydrus_core::HashId;
use serde_json::Value;
use slint::platform::{PointerEventButton, WindowEvent};
use slint::{ComponentHandle as _, LogicalPosition, Model as _};

use super::common::widgets;
use super::media_support::*;

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

fn id_of(fixture: &Fixture, hash: hydrus_core::Sha256) -> HashId {
    fixture
        .store
        .read(move |c| hydrus_store::master::hash_id(c, &hash))
        .unwrap()
        .unwrap()
}

/// The recording's files, in the inbox with their notes, and the viewer
/// opened on the first.
fn open(recorded: &Value) -> (Fixture, hydrus_gui::MediaViewerWindow) {
    let fixture = start();
    let mut writes = Vec::new();
    for file in recorded["files"].as_array().unwrap() {
        let name = file["name"].as_str().unwrap();
        let id = id_of(&fixture, hash_of(name));
        let notes: Vec<String> = recorded["steps"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|s| s["file"] == name)
            .flat_map(|s| s["notes"].as_array().unwrap().iter())
            .map(|n| n["name"].as_str().unwrap().to_owned())
            .collect();
        writes.push((id, name.to_owned(), notes));
    }
    fixture
        .store
        .write_content(move |w| {
            for (id, name, notes) in &writes {
                w.inbox(&[*id])?;
                for note in notes {
                    w.set_note(*id, note, &format!("the {note} note of {name}"))?;
                }
            }
            Ok(())
        })
        .unwrap();
    let hashes: Vec<String> = recorded["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["hash"].as_str().unwrap().to_owned())
        .collect();
    fixture
        .ui
        .invoke_search_edited(format!("system:hash = {}", hashes.join(" ")).into());
    fixture.ui.invoke_search_accepted();
    let order: Vec<HashId> = recorded["order"]
        .as_array()
        .unwrap()
        .iter()
        .map(|h| id_of(&fixture, h.as_str().unwrap().parse().unwrap()))
        .collect();
    assert_eq!(
        fixture.results(),
        order,
        "the files in the reference's order"
    );
    fixture.ui.invoke_thumbnail_activated(0);
    let viewer = fixture
        .bound
        .viewer
        .borrow()
        .as_ref()
        .map(slint::ComponentHandle::clone_strong)
        .expect("the viewer opens");
    viewer
        .window()
        .set_size(slint::LogicalSize::new(1000.0, 750.0));
    widgets::settle();
    (fixture, viewer)
}

/// The pointer over the notes hover, raising it.
fn raise_notes(viewer: &hydrus_gui::MediaViewerWindow) {
    let position = LogicalPosition::new(950.0, viewer.get_notes_hover_y() + 4.0);
    viewer
        .window()
        .dispatch_event(WindowEvent::PointerMoved { position });
    widgets::settle();
    assert!(viewer.get_notes_showing(), "the notes hover is raised");
}

/// The notes as the hover shows them: name, text and whether it shows.
fn notes(viewer: &hydrus_gui::MediaViewerWindow) -> Vec<(String, String, bool)> {
    viewer
        .get_notes()
        .iter()
        .map(|n| (n.name.to_string(), n.text.to_string(), n.shown))
        .collect()
}

fn recorded_notes(step: &Value) -> Vec<(String, String, bool)> {
    step["notes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| {
            (
                n["name"].as_str().unwrap().to_owned(),
                n["text"].as_str().unwrap().to_owned(),
                n["text_shown"].as_bool().unwrap(),
            )
        })
        .collect()
}

/// Show the recorded file called `name` (by its place in the order).
fn go_to(viewer: &hydrus_gui::MediaViewerWindow, recorded: &Value, name: &str) {
    let hash = recorded["files"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["name"] == name)
        .unwrap()["hash"]
        .clone();
    let place = recorded["order"]
        .as_array()
        .unwrap()
        .iter()
        .position(|h| *h == hash)
        .unwrap();
    viewer.invoke_first();
    for _ in 0..place {
        viewer.invoke_next();
    }
    widgets::settle();
}

fn press(viewer: &hydrus_gui::MediaViewerWindow, at: LogicalPosition, button: &str) {
    let button = match button {
        "left" => PointerEventButton::Left,
        "right" => PointerEventButton::Right,
        _ => PointerEventButton::Middle,
    };
    widgets::click_at(viewer.window(), at, button);
}

// leaf: interaction-viewer-note-hover-clicks
#[test]
fn a_note_s_clicks_edit_fold_and_copy_it_as_the_reference_s_do() {
    let recorded = hydrus_testkit::fixture_json("viewer_hover_clicks.json");
    let copied: Rc<RefCell<Vec<String>>> = Rc::default();
    hydrus_gui::set_clipper({
        let copied = copied.clone();
        move |clip| {
            if let hydrus_gui::Clip::Text(text) = clip {
                copied.borrow_mut().push(text.clone());
            }
        }
    });
    let (fixture, viewer) = open(&recorded);
    for step in recorded["steps"].as_array().unwrap() {
        let what = format!("{step}");
        match step["do"].as_str().unwrap() {
            "open" => {
                go_to(&viewer, &recorded, step["file"].as_str().unwrap());
                assert_eq!(notes(&viewer), recorded_notes(step), "{what}");
                // the panel's tooltip
                raise_notes(&viewer);
                let note = step["notes"][0]["name"].as_str().unwrap();
                assert!(
                    widgets::descriptions(viewer.window(), note)
                        .iter()
                        .any(|d| d.as_deref() == step["notes"][0]["tooltip"].as_str()),
                    "{what}"
                );
            }
            "press" => {
                let note = step["note"].as_str().unwrap();
                raise_notes(&viewer);
                let panel = widgets::descriptions(viewer.window(), note)
                    .iter()
                    .position(Option::is_some)
                    .expect("the note's panel");
                let at = widgets::position(viewer.window(), note, panel);
                let copies = copied.borrow().len();
                press(&viewer, at, step["button"].as_str().unwrap());
                let did = step["did"].as_array().unwrap();
                if let Some(edit) = did.iter().find(|d| !d["edit notes"].is_null()) {
                    let window = fixture.bound.manage_notes.borrow().as_ref().map_or_else(
                        || panic!("{what}: Manage Notes opens"),
                        slint::ComponentHandle::clone_strong,
                    );
                    let names: Vec<String> =
                        window.get_names().iter().map(|n| n.to_string()).collect();
                    let current = usize::try_from(window.get_current()).unwrap();
                    assert_eq!(names[current], edit["start on"].as_str().unwrap(), "{what}");
                    window.invoke_cancel();
                    widgets::settle();
                    assert!(fixture.bound.manage_notes.borrow().is_none(), "{what}");
                } else {
                    assert!(fixture.bound.manage_notes.borrow().is_none(), "{what}");
                }
                if let Some(clip) = did.iter().find(|d| !d["clipboard"].is_null()) {
                    assert_eq!(
                        copied.borrow()[copies..],
                        [clip["clipboard"][1].as_str().unwrap()],
                        "{what}"
                    );
                } else {
                    assert_eq!(copied.borrow().len(), copies, "{what}: nothing copied");
                }
                assert_eq!(notes(&viewer), recorded_notes(step), "{what}");
            }
            "show" => {
                go_to(&viewer, &recorded, step["file"].as_str().unwrap());
                assert_eq!(notes(&viewer), recorded_notes(step), "{what}");
            }
            _ => {}
        }
    }
}

// leaf: interaction-viewer-inbox-icon-click
#[test]
fn a_left_press_on_the_inbox_icon_archives_the_file() {
    let recorded = hydrus_testkit::fixture_json("viewer_hover_clicks.json");
    let (fixture, viewer) = open(&recorded);
    let steps = recorded["steps"].as_array().unwrap();
    // (on the file the recording pressed on: the one shown last)
    let file = steps.iter().rev().find(|s| s["do"] == "show").unwrap()["file"]
        .as_str()
        .unwrap();
    go_to(&viewer, &recorded, file);
    let opened = &steps[0]["top_right"];
    assert!(opened["inbox"].as_bool().unwrap());
    // the top-right hover shows the icon and where the file is
    let raise = |viewer: &hydrus_gui::MediaViewerWindow| {
        viewer.window().dispatch_event(WindowEvent::PointerMoved {
            position: LogicalPosition::new(990.0, 4.0),
        });
        widgets::settle();
    };
    raise(&viewer);
    assert!(
        viewer.get_ratings_showing(),
        "the top-right hover is raised"
    );
    assert!(widgets::shows(viewer.window(), "inbox"));
    assert!(widgets::shows(
        viewer.window(),
        opened["locations"].as_str().unwrap()
    ));
    for step in steps.iter().filter(|s| s["do"] == "inbox icon") {
        let what = format!("{step}");
        raise(&viewer);
        let at = widgets::position(viewer.window(), "inbox", 0);
        press(&viewer, at, step["button"].as_str().unwrap());
        let after = &step["top_right"];
        assert_eq!(
            viewer.get_file_inbox(),
            after["inbox"].as_bool().unwrap(),
            "{what}"
        );
        raise(&viewer);
        assert_eq!(
            widgets::shows(viewer.window(), "inbox"),
            after["icons"].as_bool().unwrap(),
            "{what}: the icon"
        );
    }
    // the store has it archived
    let pressed = id_of(&fixture, hash_of(file));
    let inboxed = fixture
        .store
        .read(move |c| hydrus_store::media::inboxed(c, &[pressed]))
        .unwrap();
    assert!(inboxed.is_empty(), "archived");
}
