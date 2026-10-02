//! The editors of system predicates offered without a value, against the
//! reference's own (`oracle/record_system_predicate_editors.py`, on the
//! `basic` fixture): what the empty search box offers, each editor's pages,
//! ready-made buttons and panels, what each panel makes at first and after
//! each of its fields is changed; and the editor window adding what it
//! makes to the page's search.

use std::sync::Arc;

use serde_json::Value as Json;
use slint::Model as _;

use hydrus_core::ServiceKey;
use hydrus_core::search::context::{LocationContext, TagContext};
use hydrus_core::service::builtin_keys;
use hydrus_gui::autocomplete::Autocomplete;
use hydrus_gui::predicate_editor_window::button_label;
use hydrus_gui::predicate_editors::{Blank, Context, Editor, Field, Panel};
use hydrus_gui::{MainWindow, Pages, SearchPage, bind, headless};
use hydrus_search::{CivilDateTime, Predicate, TextContext, predicate_text};
use hydrus_store::Store;
use hydrus_store::import::import_legacy;

fn store() -> ([tempfile::TempDir; 2], Arc<Store>) {
    let legacy = hydrus_testkit::legacy_fixture("basic");
    let native = tempfile::tempdir().unwrap();
    import_legacy(
        legacy.path(),
        &native.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store = Store::open(native.path()).unwrap();
    ([legacy, native], store)
}

fn recorded() -> Json {
    hydrus_testkit::fixture_json("system_predicate_editors.json")
}

/// The context as the reference had it, on the day it was recorded.
fn context(store: &Store, recorded: &Json) -> Context {
    let today = recorded["today"].as_str().unwrap();
    let mut parts = today.split('-').map(|p| p.parse::<u16>().unwrap());
    let (year, month, day) = (
        parts.next().unwrap(),
        parts.next().unwrap(),
        parts.next().unwrap(),
    );
    Context::new(
        &store.snapshot().services,
        Vec::new(),
        CivilDateTime::new(year, month as u8, day as u8, 0, 0).unwrap(),
    )
}

fn text_context(store: &Store) -> TextContext {
    let viewing = store.read(hydrus_store::settings::get).unwrap_or_default();
    TextContext::from_store(&store.snapshot().services, &viewing)
}

fn texts(predicates: &[Predicate], text: &TextContext) -> Vec<String> {
    predicates.iter().map(|p| predicate_text(p, text)).collect()
}

fn strings(json: &Json) -> Vec<String> {
    json.as_array()
        .unwrap()
        .iter()
        .map(|s| s.as_str().unwrap().to_owned())
        .collect()
}

/// The blank predicates hydrus-rs has no editor for yet.
const UNPORTED: [&str; 4] = [
    "system:filetype",
    "system:hash",
    "system:rating",
    "system:similar files",
];

#[test]
fn the_empty_search_box_offers_the_reference_s_system_predicates() {
    let (_dirs, store) = store();
    let recorded = recorded();
    let mut autocomplete = Autocomplete::new(store.clone());
    autocomplete.set_context(
        &LocationContext::single(ServiceKey::new(builtin_keys::MY_FILES.to_vec())),
        &TagContext::default(),
    );
    autocomplete.set_text("");
    let ours: Vec<(String, bool)> = autocomplete
        .suggestions()
        .iter()
        .map(|s| (s.label.clone(), s.editor.is_some()))
        .collect();
    let theirs: Vec<(String, bool)> = recorded["offered"]
        .as_array()
        .unwrap()
        .iter()
        .map(|o| {
            (
                o["text"].as_str().unwrap().to_owned(),
                o["opens_editor"].as_bool().unwrap(),
            )
        })
        .filter(|(text, _)| !UNPORTED.contains(&text.as_str()))
        .collect();
    assert_eq!(ours, theirs);
    // and searching all known files, those needing no file's metadata
    autocomplete.set_context(
        &LocationContext::single(ServiceKey::new(builtin_keys::COMBINED_FILE.to_vec())),
        &TagContext::default(),
    );
    let everywhere: Vec<&str> = autocomplete
        .suggestions()
        .iter()
        .map(|s| s.predicate.as_str())
        .collect();
    assert_eq!(
        everywhere,
        [
            "system:everything",
            "system:file relationships",
            "system:file service",
            "system:file viewing statistics",
            "system:limit",
            "system:number of tags",
            "system:tag (advanced)",
            "system:tag as number",
        ]
    );
}

#[test]
fn each_editor_is_the_reference_s() {
    let (_dirs, store) = store();
    let recorded = recorded();
    let context = context(&store, &recorded);
    let text = text_context(&store);
    let mut skipped = Vec::new();
    for editor in recorded["editors"].as_array().unwrap() {
        let name = editor["text"].as_str().unwrap();
        let blank = Blank::from_text(name).unwrap_or_else(|| panic!("no {name}"));
        let Some(ours) = Editor::new(blank, &context) else {
            skipped.push(name);
            continue;
        };
        let pages = editor["pages"].as_array().unwrap();
        assert_eq!(ours.pages.len(), pages.len(), "{name}");
        for (page, theirs) in ours.pages.iter().zip(pages) {
            assert_eq!(page.name, theirs["name"].as_str().unwrap(), "{name}");
            let buttons: Vec<(String, Vec<String>)> = page
                .buttons
                .iter()
                .map(|b| {
                    let predicates: Vec<Predicate> = b
                        .predicates
                        .iter()
                        .cloned()
                        .map(Predicate::System)
                        .collect();
                    (button_label(b, &text), texts(&predicates, &text))
                })
                .collect();
            let their_buttons: Vec<(String, Vec<String>)> = theirs["buttons"]
                .as_array()
                .unwrap()
                .iter()
                .map(|b| {
                    (
                        b["label"].as_str().unwrap().to_owned(),
                        strings(&b["predicates"]),
                    )
                })
                .collect();
            assert_eq!(buttons, their_buttons, "{name}");
            let panels = theirs["panels"].as_array().unwrap();
            assert_eq!(page.panels.len(), panels.len(), "{name}");
            for (panel, theirs) in page.panels.iter().zip(panels) {
                let class = theirs["class"].as_str().unwrap();
                assert_eq!(panel.kind.class_name(), class, "{name}");
                let made = panel.predicates(&context).map(|p| texts(&p, &text));
                match theirs["predicates"].as_array() {
                    Some(_) => assert_eq!(made, Ok(strings(&theirs["predicates"])), "{class}"),
                    // (no URL class to choose)
                    None => assert!(made.is_err(), "{class}"),
                }
            }
        }
    }
    assert_eq!(skipped, UNPORTED);
}

/// Change our panel as the recording changed the reference's widget
/// `widget`: a radio button or drop-down by the choice offering the same
/// options, a number or text by its place among those shown. `false` if
/// ours has no such field.
fn change(panel: &mut Panel, widgets: &[Json], widget: usize, set: &Json) -> bool {
    let fact = &widgets[widget];
    let kind = fact["kind"].as_str().unwrap();
    let choice_with = |panel: &Panel, options: &[String]| {
        panel
            .fields
            .iter()
            .position(|f| matches!(f, Field::Choice { options: o, .. } if o == options))
    };
    let nth_shown = |panel: &Panel, k: usize, number: bool| {
        (0..panel.fields.len())
            .filter(|&i| panel.shown(i))
            .filter(|&i| match &panel.fields[i] {
                Field::Number { .. } => number,
                Field::Text { .. } => !number,
                _ => false,
            })
            .nth(k)
    };
    let before = |what: &str| {
        widgets[..widget]
            .iter()
            .filter(|w| w["kind"] == what && (what != "text" || w["class"] == "QLineEdit"))
            .count()
    };
    match kind {
        "radio" | "choice" => {
            let options = strings(if kind == "radio" {
                &fact["group"]
            } else {
                &fact["choices"]
            });
            let Some(i) = choice_with(panel, &options) else {
                return false;
            };
            let wanted = set.as_str().or(fact["text"].as_str()).unwrap();
            let option = options.iter().position(|o| o == wanted).unwrap();
            panel.choose(i, option);
            true
        }
        "number" => {
            let Some(i) = nth_shown(panel, before("number"), true) else {
                return false;
            };
            panel.set_number(i, set.as_i64().unwrap());
            true
        }
        "text" => {
            let Some(i) = nth_shown(panel, before("text"), false) else {
                return false;
            };
            panel.set_text(i, set.as_str().unwrap());
            true
        }
        // (a calendar and a time box in the reference; typed here)
        "date" | "time" => {
            let Some(i) = nth_shown(panel, usize::from(kind == "time"), false) else {
                return false;
            };
            panel.set_text(i, set.as_str().unwrap());
            true
        }
        other => panic!("can't change a {other}"),
    }
}

/// The number fields a panel shows (value, least and most) are the
/// reference's spin boxes shown, in order.
fn numbers_match(panel: &Panel, widgets: &[Json], what: &str) {
    let ours: Vec<(i64, i64, i64)> = (0..panel.fields.len())
        .filter(|&i| panel.shown(i))
        .filter_map(|i| match &panel.fields[i] {
            Field::Number {
                value, min, max, ..
            } => Some((*value, *min, *max)),
            _ => None,
        })
        .collect();
    let theirs: Vec<(i64, i64, i64)> = widgets
        .iter()
        .filter(|w| w["kind"] == "number")
        .map(|w| {
            (
                w["value"].as_i64().unwrap(),
                w["min"].as_i64().unwrap(),
                w["max"].as_i64().unwrap(),
            )
        })
        .collect();
    assert_eq!(ours, theirs, "{what}");
}

#[test]
fn each_change_to_a_panel_makes_what_the_reference_s_makes() {
    let (_dirs, store) = store();
    let recorded = recorded();
    let context = context(&store, &recorded);
    let text = text_context(&store);
    let mut checked = 0;
    for editor in recorded["editors"].as_array().unwrap() {
        let blank = Blank::from_text(editor["text"].as_str().unwrap()).unwrap();
        let Some(ours) = Editor::new(blank, &context) else {
            continue;
        };
        for (page, theirs) in ours.pages.iter().zip(editor["pages"].as_array().unwrap()) {
            for (panel, theirs) in page.panels.iter().zip(theirs["panels"].as_array().unwrap()) {
                let class = theirs["class"].as_str().unwrap();
                let widgets = theirs["widgets"].as_array().unwrap();
                numbers_match(panel, widgets, class);
                let at_first = panel.predicates(&context).map(|p| texts(&p, &text));
                for change_made in theirs["changes"].as_array().unwrap() {
                    let widget = usize::try_from(change_made["widget"].as_u64().unwrap()).unwrap();
                    let set = &change_made["set"];
                    let expected = change_made["predicates"]
                        .as_array()
                        .map(|_| strings(&change_made["predicates"]));
                    let mut changed = panel.clone();
                    if !change(&mut changed, widgets, widget, set) {
                        // a widget of the reference's we don't have (tag
                        // advanced's autocomplete) changes nothing
                        assert_eq!(expected, at_first.clone().ok(), "{class} {change_made}");
                        continue;
                    }
                    let made = changed.predicates(&context).map(|p| texts(&p, &text));
                    // (the reference offers terabytes, but can't write them)
                    if set == "TB" {
                        assert!(
                            expected
                                .as_ref()
                                .is_some_and(|e| e[0].starts_with("error:")),
                            "{expected:?}"
                        );
                        assert_eq!(made, Ok(vec!["system:filesize < 200TB".to_owned()]));
                        checked += 1;
                        continue;
                    }
                    match expected {
                        Some(expected) => {
                            assert_eq!(made, Ok(expected), "{class} {widget} {set}");
                        }
                        None => assert!(made.is_err(), "{class} {widget} {set}"),
                    }
                    checked += 1;
                }
            }
        }
    }
    assert!(checked > 150, "{checked}");
}

/// Several fields set in turn make what the reference's make: the number of
/// tags swapped for a namespace's own predicate, typed dates and times,
/// tags cleaned, and the amounts either side of "≈".
#[test]
fn panels_set_in_several_ways_make_what_the_reference_s_make() {
    let (_dirs, store) = store();
    let recorded = recorded();
    let context = context(&store, &recorded);
    let text = text_context(&store);
    for scenario in recorded["scenarios"].as_array().unwrap() {
        let blank = Blank::from_text(scenario["editor"].as_str().unwrap()).unwrap();
        let editor = Editor::new(blank, &context).unwrap();
        let page = usize::try_from(scenario["page"].as_u64().unwrap()).unwrap();
        let panel = usize::try_from(scenario["panel"].as_u64().unwrap()).unwrap();
        let mut panel = editor.pages[page].panels[panel].clone();
        for step in scenario["steps"].as_array().unwrap() {
            let widgets = step["widgets"].as_array().unwrap();
            let widget = usize::try_from(step["widget"].as_u64().unwrap()).unwrap();
            numbers_match(&panel, widgets, &scenario.to_string());
            assert!(
                change(&mut panel, widgets, widget, &step["set"]),
                "{scenario}"
            );
        }
        let made = panel.predicates(&context).map(|p| texts(&p, &text));
        assert_eq!(
            made,
            Ok(strings(&scenario["predicates"])),
            "{}",
            scenario["steps"]
                .as_array()
                .unwrap()
                .iter()
                .map(|s| format!("{} {}", s["widget"], s["set"]))
                .collect::<Vec<_>>()
                .join(", ")
        );
    }
}

/// The page's predicates, as shown.
fn shown_predicates(ui: &MainWindow) -> Vec<String> {
    ui.get_predicates()
        .iter()
        .map(|p| p.text.to_string())
        .collect()
}

fn suggestion(ui: &MainWindow, text: &str) -> i32 {
    let suggestions = ui.get_suggestions();
    let i = suggestions
        .iter()
        .position(|s| s.text == text)
        .unwrap_or_else(|| panic!("no {text}"));
    i32::try_from(i).unwrap()
}

#[test]
fn the_editor_window_adds_what_it_makes_to_the_search() {
    let (_dirs, store) = store();
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::single(SearchPage::new(store.clone())));
    let editor = || {
        bound
            .predicate_editor
            .borrow()
            .as_ref()
            .map(slint::ComponentHandle::clone_strong)
            .expect("an editor")
    };
    ui.invoke_search_edited("".into());

    // a ready-made button adds its predicate, and closes the editor
    ui.invoke_suggestion_chosen(suggestion(&ui, "system:limit"));
    let window = editor();
    assert!(window.get_note().starts_with("system:limit clips"));
    let buttons: Vec<String> = window
        .get_buttons()
        .iter()
        .map(|b: slint::SharedString| b.to_string())
        .collect();
    assert_eq!(
        buttons,
        [
            "system:limit is 64",
            "system:limit is 256",
            "system:limit is 1,024"
        ]
    );
    window.invoke_button_clicked(0);
    assert!(bound.predicate_editor.borrow().is_none());
    assert_eq!(shown_predicates(&ui), ["system:limit is 64"]);

    // enter on the highlighted one opens it too; a page of several, and a
    // panel's "ok"
    ui.invoke_search_edited("".into());
    let time = suggestion(&ui, "system:time");
    ui.invoke_move_highlight(time);
    ui.invoke_search_accepted();
    let window = editor();
    let pages: Vec<String> = window
        .get_pages()
        .iter()
        .map(|p: slint::SharedString| p.to_string())
        .collect();
    assert_eq!(pages, ["import", "modified", "last viewed", "archived"]);
    window.invoke_page_chosen(1);
    assert_eq!(window.get_buttons().row_count(), 0);
    window.invoke_ok(0);
    assert_eq!(
        shown_predicates(&ui),
        [
            "system:limit is 64",
            "system:modified time: since 7 days ago"
        ]
    );

    // what can't be made says why, and cancelling adds nothing
    ui.invoke_suggestion_chosen(suggestion(&ui, "system:time"));
    let window = editor();
    window.invoke_text_edited(1, 2, "June".into());
    window.invoke_ok(1);
    assert!(window.get_error().contains("year-month-day"));
    assert!(bound.predicate_editor.borrow().is_some());
    window.invoke_cancel();
    assert!(bound.predicate_editor.borrow().is_none());
    assert_eq!(shown_predicates(&ui).len(), 2);

    // choosing "≈" shows the amount either side, which is then set
    ui.invoke_suggestion_chosen(suggestion(&ui, "system:dimensions"));
    let window = editor();
    let width = window.get_panels().row_data(0).unwrap().fields;
    assert!(!width.row_data(3).unwrap().shown);
    window.invoke_chose(0, 1, 2);
    assert!(width.row_data(3).unwrap().shown);
    assert!(!width.row_data(4).unwrap().shown);
    window.invoke_number_edited(0, 3, 50);
    window.invoke_ok(0);
    assert_eq!(
        shown_predicates(&ui).last().unwrap(),
        "system:width \u{2248} 1,920 \u{b1}50"
    );
}

#[test]
fn more_suggestions_than_fit_scroll_rather_than_spill_over() {
    let (_dirs, store) = store();
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let _bound = bind(&ui, Pages::single(SearchPage::new(store.clone())));
    let main_window = windows.get(0).unwrap();
    let (width, height) = (1100_usize, 900_usize);
    // (twice: the first frame lays the window out)
    let draw = || {
        headless::render(&main_window, width as u32, height as u32);
        headless::render(&main_window, width as u32, height as u32)
    };
    // the sidebar's rows of pixels that differ (the grid's thumbnails load
    // as they will)
    let sidebar = 290;
    let differing = |a: &[u8], b: &[u8]| -> Vec<usize> {
        (0..height)
            .filter(|y| {
                a[y * width * 4..(y * width + sidebar) * 4]
                    != b[y * width * 4..(y * width + sidebar) * 4]
            })
            .collect()
    };
    ui.invoke_search_edited("".into());
    let all: Vec<_> = ui.get_suggestions().iter().collect();
    assert!(all.len() > 14, "{} suggestions", all.len());

    // the first row, where highlighting it changes the window
    ui.set_highlighted(-1);
    let plain = draw();
    ui.set_highlighted(0);
    let first = differing(&plain, &draw());
    let top = *first.first().expect("the highlight drawn");
    // twelve rows of 22 pixels show, under which the window is as it is
    // with no more than twelve suggestions
    let bottom = top + 12 * 22 + 2;
    ui.set_highlighted(-1);
    let everything = draw();
    ui.set_suggestions(slint::ModelRc::new(slint::VecModel::from(
        all[..12].to_vec(),
    )));
    let twelve = draw();
    let below: Vec<usize> = differing(&everything, &twelve)
        .into_iter()
        .filter(|y| *y >= bottom)
        .collect();
    assert_eq!(
        below,
        Vec::<usize>::new(),
        "suggestions drawn under the dropdown"
    );

    // the last highlighted is scrolled into view
    ui.set_suggestions(slint::ModelRc::new(slint::VecModel::from(all.clone())));
    draw();
    ui.set_highlighted(i32::try_from(all.len() - 1).unwrap());
    let scrolled = differing(&everything, &draw());
    assert!(!scrolled.is_empty(), "the last suggestion not shown");
    assert!(
        scrolled.iter().all(|y| (top - 2..bottom).contains(y)),
        "{scrolled:?} outside {top}..{bottom}"
    );
}
