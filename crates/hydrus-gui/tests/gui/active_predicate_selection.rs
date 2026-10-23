//! The search page's active predicate list after an edit: which rows it
//! selects, and the mixed edit dialog's OR predicates, each edited in the OR
//! editor from its row, compared with the reference's real list and dialog
//! (`oracle/record_active_predicate_selection.py`).

use serde_json::{Value as Json, json};
use slint::{ComponentHandle as _, Model as _};

use hydrus_gui::{MainWindow, Pages, SearchPage, bind, headless};
use hydrus_search::{FileSearchContext, LocationContext, Predicate};

/// The predicate a recorded row's text is.
fn predicate(text: &str) -> Predicate {
    if let Some((a, b)) = text.split_once(" OR ") {
        return Predicate::Or(vec![predicate(a), predicate(b)]);
    }
    hydrus_search::parse_api_search(&json!([text]))
        .unwrap()
        .remove(0)
}

/// The rows, and the selected rows, as sorted texts (the recorder's search
/// was set with `SetFileSearchContext`, which sorts it, where this page is
/// restored as given).
fn rows(ui: &MainWindow) -> (Vec<String>, Vec<String>) {
    let texts: Vec<String> = ui
        .get_predicates()
        .iter()
        .map(|r| r.text.to_string())
        .collect();
    let mask: Vec<bool> = ui.get_active_predicate_selected().iter().collect();
    let mut selected: Vec<String> = texts
        .iter()
        .zip(&mask)
        .filter(|(_, on)| **on)
        .map(|(t, _)| t.clone())
        .collect();
    let mut shown = texts;
    shown.sort();
    selected.sort();
    (shown, selected)
}

// leaf: audit-options-search-active-edit
#[test]
fn edits_select_what_they_made_and_mixed_edits_edit_their_ors_as_the_references_do() {
    let fixture = hydrus_testkit::fixture_json("active_predicate_selection.json");
    let (_dirs, store) = super::subscriptions::store();
    let _windows = headless::init();
    let mut differences = Vec::new();
    for case in fixture["cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let mut differ = |key: &str, ours: Json, want: &Json| {
            if ours != *want {
                differences.push(format!("{name}: {key} {ours} not {want}"));
            }
        };
        let before = &case["before"];
        let values: Vec<Predicate> = before["shown"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| predicate(t.as_str().unwrap()))
            .collect();
        let context = FileSearchContext {
            location: LocationContext::single(hydrus_core::ServiceKey::new(
                b"local files".to_vec(),
            )),
            predicates: values,
            ..Default::default()
        };
        let ui = MainWindow::new().unwrap();
        ui.show().unwrap();
        let bound = bind(
            &ui,
            Pages::single(SearchPage::restored(
                store.clone(),
                context,
                false,
                None,
                Vec::new(),
            )),
        );
        // the recorded selection, clicked (Ctrl after the first)
        let texts: Vec<String> = ui
            .get_predicates()
            .iter()
            .map(|r| r.text.to_string())
            .collect();
        for (i, text) in before["selected"].as_array().unwrap().iter().enumerate() {
            let at = texts.iter().position(|t| json!(t) == *text).unwrap();
            ui.invoke_active_predicate_clicked(i32::try_from(at).unwrap(), i > 0, false);
        }
        let (shown, selected) = rows(&ui);
        differ("before shown", json!(shown), &before["shown"]);
        differ("before selected", json!(selected), &before["selected"]);
        if case["edit"].as_bool() == Some(true) {
            // Shift+Return on the selection edits it
            ui.invoke_active_predicate_activated(false, true);
            let editor = bound
                .predicate_editor
                .borrow()
                .as_ref()
                .map(slint::ComponentHandle::clone_strong)
                .expect("the edit dialog open");
            let title = editor.get_composed_title();
            let title = title
                .strip_suffix(editor.get_title_suffix().as_str())
                .unwrap_or(&title)
                .to_owned();
            differ("title", json!(title), &case["title"]);
            let labels = |e: &hydrus_gui::PredicateEditorWindow| -> Json {
                json!(
                    e.get_invertible_labels()
                        .iter()
                        .map(|l| l.to_string())
                        .collect::<Vec<_>>()
                )
            };
            differ(
                "invertible before",
                labels(&editor),
                &case["invertible_before"],
            );
            if let Some(simple) = case["simple"].as_array() {
                for (i, text) in simple.iter().enumerate() {
                    editor.invoke_simple_edited(
                        i32::try_from(i).unwrap(),
                        text.as_str().unwrap().into(),
                    );
                }
            }
            if case["flip"].as_bool() == Some(true) {
                for i in 0..editor.get_invertible_labels().row_count() {
                    editor.invoke_invertible_clicked(i32::try_from(i).unwrap());
                }
            }
            if case.get("or_before").is_some() {
                editor.invoke_or_edit(0);
                let or = bound
                    .search_or
                    .borrow()
                    .as_ref()
                    .map(slint::ComponentHandle::clone_strong)
                    .expect("the OR editor open");
                let terms = |or: &hydrus_gui::SearchOrWindow| -> Json {
                    json!(
                        or.get_predicates()
                            .iter()
                            .map(|p| p.to_string())
                            .collect::<Vec<_>>()
                    )
                };
                differ("OR before", terms(&or), &case["or_before"]);
                for text in case["or_type"].as_array().into_iter().flatten() {
                    let text = text.as_str().unwrap();
                    or.set_input(text.into());
                    or.invoke_edited(text.into());
                    or.invoke_enter(false);
                }
                for text in case["or_remove"].as_array().into_iter().flatten() {
                    let at = or
                        .get_predicates()
                        .iter()
                        .position(|p| json!(p.to_string()) == *text)
                        .unwrap();
                    or.invoke_remove(i32::try_from(at).unwrap());
                }
                differ("OR after", terms(&or), &case["or_after"]);
                or.invoke_apply();
                assert!(
                    bound.search_or.borrow().is_none(),
                    "{name}: the OR editor stays open"
                );
            }
            differ(
                "invertible after",
                labels(&editor),
                &case["invertible_after"],
            );
            if case["accepted"].as_bool() == Some(true) {
                editor.invoke_ok(0);
            } else {
                editor.invoke_cancel();
            }
            assert!(
                bound.predicate_editor.borrow().is_none(),
                "{name}: the dialog stays open"
            );
        } else {
            // typed into the search box and entered, as the autocomplete does
            for text in case["enter"].as_array().unwrap() {
                ui.invoke_search_edited(text.as_str().unwrap().into());
                ui.invoke_search_accepted();
            }
        }
        let (shown, selected) = rows(&ui);
        differ("shown", json!(shown), &case["after"]["shown"]);
        differ("selected", json!(selected), &case["after"]["selected"]);
    }
    assert!(differences.is_empty(), "{}", differences.join("\n"));
}
