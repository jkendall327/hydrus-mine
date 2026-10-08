//! The manage notes window put through the reference's recording
//! (`oracle/fixtures/manage_notes.json`) as a user would: the file's notes
//! seeded, the dialog opened from the thumbnail's "manage" menu, tabs chosen,
//! text typed, "add", "rename" and "delete" answered, "copy" and "copy URLs"
//! pressed; what the dialog shows (tab names, the current tab and its text,
//! whether it can be edited), what it said, and what it put on the clipboard,
//! read back at every step. The case that pastes is not replayed here (the
//! window's paste reads the system clipboard).

use std::{cell::RefCell, rc::Rc};

use hydrus_gui::{Clip, MainWindow, Pages, SearchPage, bind, headless};
use hydrus_store::import::import_legacy;
use serde_json::Value;
use slint::{ComponentHandle as _, Model as _};

fn names(dialog: &hydrus_gui::ManageNotesWindow) -> Vec<String> {
    dialog.get_names().iter().map(|n| n.to_string()).collect()
}

// leaf: audit-media-notes-urls
#[test]
fn the_manage_notes_window_follows_the_reference_s_steps() {
    let recorded = hydrus_testkit::fixture_json("manage_notes.json");
    let windows = headless::init();
    let mut copies_seen = 0;
    for (c, case) in recorded["cases"].as_array().unwrap().iter().enumerate() {
        let steps: Vec<&Value> = case["states"].as_array().unwrap().iter().collect();
        if steps.iter().any(|s| s["step"][0] == "paste") {
            continue;
        }
        let legacy = hydrus_testkit::legacy_fixture("basic");
        let native = tempfile::tempdir().unwrap();
        import_legacy(
            legacy.path(),
            &native.path().join(hydrus_store::store::DB_FILE_NAME),
        )
        .unwrap();
        let store = hydrus_store::Store::open(native.path()).unwrap();
        let ui = MainWindow::new().unwrap();
        ui.show().unwrap();
        let bound = bind(&ui, Pages::single(SearchPage::new(store.clone())));
        ui.invoke_search_edited("system:everything".into());
        ui.invoke_search_accepted();
        let file = bound.current.borrow().borrow().results()[0];
        for (name, text) in case["notes"].as_object().unwrap() {
            let (name, text) = (name.clone(), text.as_str().unwrap().to_owned());
            store
                .write_content(move |w| w.set_note(file, &name, &text))
                .unwrap();
        }
        let copies = Rc::new(RefCell::new(Vec::new()));
        hydrus_gui::set_clipper({
            let copies = copies.clone();
            move |clip| copies.borrow_mut().push(clip.clone())
        });
        ui.invoke_thumbnail_clicked(0, false, false);
        ui.invoke_thumbnail_menu_requested(0);
        let manage = ui.get_thumbnail_menu().manage;
        let id = manage
            .iter()
            .find(|row| row.label.starts_with("notes"))
            .map(|row| row.id)
            .unwrap();
        ui.invoke_menu_chosen(id);
        let dialog = bound.manage_notes.borrow().as_ref().unwrap().clone_strong();

        let check = |state: &Value, at: &str| {
            let tabs: Vec<(String, String)> = state["tabs"]
                .as_array()
                .unwrap()
                .iter()
                .map(|t| {
                    (
                        t[0].as_str().unwrap().to_owned(),
                        t[1].as_str().unwrap().to_owned(),
                    )
                })
                .collect();
            assert_eq!(
                names(&dialog),
                tabs.iter().map(|t| t.0.clone()).collect::<Vec<_>>(),
                "{at}"
            );
            let current = state["current"].as_i64().unwrap();
            if current >= 0 {
                assert_eq!(i64::from(dialog.get_current()), current, "{at}");
                // (the box gives back what was typed, "\r\n" and all)
                assert_eq!(
                    dialog.get_text().replace("\r\n", "\n"),
                    tabs[usize::try_from(current).unwrap()].1,
                    "{at}"
                );
            }
            assert_eq!(
                dialog.get_can_edit(),
                state["can_edit"].as_bool().unwrap(),
                "{at}"
            );
        };
        // (the dialog opens on its first tab; the recording started on a named one)
        let first = &steps[0]["state"];
        let start = i32::try_from(first["current"].as_i64().unwrap()).unwrap();
        if start >= 0 && dialog.get_current() != start {
            dialog.invoke_tab_chosen(start);
        }
        check(first, &format!("case {c} start"));
        for recorded in &steps[1..] {
            let step = recorded["step"].as_array().unwrap();
            let at = format!("case {c}: {step:?}");
            let said = recorded["said"].as_array().unwrap();
            let before = copies.borrow().len();
            let tab = |i: &Value| i32::try_from(i.as_u64().unwrap()).unwrap();
            let answer = |name: &str| {
                dialog.set_asking_text(name.into());
                dialog.invoke_chosen(0);
            };
            match step[0].as_str().unwrap() {
                "type" => {
                    dialog.invoke_tab_chosen(tab(&step[1]));
                    dialog.set_text(step[2].as_str().unwrap().into());
                    dialog.invoke_text_edited();
                }
                "select" => dialog.invoke_tab_chosen(tab(&step[1])),
                "add" => {
                    dialog.invoke_add();
                    assert_eq!(
                        dialog.get_asking_message(),
                        said[0]["asked"].as_str().unwrap(),
                        "{at}"
                    );
                    answer(step[1].as_str().unwrap());
                }
                "rename" => {
                    dialog.invoke_tab_renamed(tab(&step[1]));
                    assert_eq!(
                        dialog.get_asking_message(),
                        said[0]["asked"].as_str().unwrap(),
                        "{at}"
                    );
                    assert_eq!(
                        dialog.get_asking_text(),
                        said[0]["default"].as_str().unwrap(),
                        "{at}"
                    );
                    answer(step[2].as_str().unwrap());
                }
                "delete" => {
                    dialog.invoke_delete();
                    match said.first() {
                        Some(said) => {
                            assert_eq!(
                                dialog.get_asking_message(),
                                said["asked"].as_str().unwrap(),
                                "{at}"
                            );
                            dialog.invoke_chosen(0);
                        }
                        None => assert!(!dialog.get_asking(), "{at}"),
                    }
                }
                "copy" => dialog.invoke_copy(),
                "urls" => dialog.invoke_copy_urls(),
                other => panic!("{other}"),
            }
            if let Some(notified) = said.iter().find_map(|s| s.get("notified")) {
                assert_eq!(dialog.get_notice(), notified.as_str().unwrap(), "{at}");
            }
            let copied: Vec<String> = copies.borrow()[before..]
                .iter()
                .map(|clip| match clip {
                    Clip::Text(text) => text.clone(),
                    other @ Clip::Files(_) => panic!("{other:?}"),
                })
                .collect();
            let wanted: Vec<String> = recorded["clipboard"]
                .as_array()
                .unwrap()
                .iter()
                .map(|t| t.as_str().unwrap().to_owned())
                .collect();
            if step[0] == "copy" {
                // (the JSON is Python's: compared by what it holds)
                let theirs: Value = serde_json::from_str(&wanted[0]).unwrap();
                let ours: Value = serde_json::from_str(&copied[0]).unwrap();
                assert_eq!(ours, theirs, "{at}");
            } else {
                assert_eq!(copied, wanted, "{at}");
            }
            copies_seen += copied.len();
            check(&recorded["state"], &at);
        }
        dialog.invoke_cancel();
        if dialog.get_asking() {
            dialog.invoke_chosen(1);
        }
        let _ = &windows;
    }
    assert!(copies_seen >= 2, "the URLs and the notes were copied");
}
