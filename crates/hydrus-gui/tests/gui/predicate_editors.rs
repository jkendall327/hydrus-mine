//! The editors of system predicates offered without a value, against the
//! reference's own (`oracle/record_system_predicate_editors.py`, on the
//! `basic` fixture): what the empty search box offers, each editor's pages,
//! ready-made buttons and panels, what each panel makes at first and after
//! each of its fields is changed; and the editor window adding what it
//! makes to the page's search.

use std::sync::Arc;

use serde_json::Value as Json;
use slint::{ComponentHandle as _, Model as _};

use hydrus_core::search::context::{LocationContext, TagContext};
use hydrus_core::service::builtin_keys;
use hydrus_core::{ServiceKey, ServiceType};
use hydrus_gui::autocomplete::Autocomplete;
use hydrus_gui::predicate_editor_window::button_label;
use hydrus_gui::predicate_editors::{Blank, Context, Editor, Field, Panel, Pressed};
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
            "system:hash",
            "system:limit",
            "system:number of tags",
            "system:rating",
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
    for editor in recorded["editors"].as_array().unwrap() {
        let name = editor["text"].as_str().unwrap();
        let blank = Blank::from_text(name).unwrap_or_else(|| panic!("no {name}"));
        let ours = Editor::new(blank, &context);
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
                // a tree's groups and their filetypes, in order
                for widget in theirs["widgets"].as_array().unwrap() {
                    if widget["kind"] != "tree" {
                        continue;
                    }
                    let Some(Field::Tree { groups }) = panel
                        .fields
                        .iter()
                        .find(|f| matches!(f, Field::Tree { .. }))
                    else {
                        panic!("{class} has no tree");
                    };
                    let ours: Vec<(String, Vec<String>)> = groups
                        .iter()
                        .map(|g| (g.name.clone(), g.options.clone()))
                        .collect();
                    let theirs: Vec<(String, Vec<String>)> = widget["groups"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|g| {
                            (
                                g["text"].as_str().unwrap().to_owned(),
                                strings(&g["children"]),
                            )
                        })
                        .collect();
                    assert_eq!(ours, theirs);
                }
                let made = panel.predicates(&context).map(|p| texts(&p, &text));
                match theirs["predicates"].as_array() {
                    Some(_) => assert_eq!(made, Ok(strings(&theirs["predicates"])), "{class}"),
                    // (no URL class to choose)
                    None => assert!(made.is_err(), "{class}"),
                }
            }
        }
    }
}

/// Change our panel as the recording changed the reference's widget
/// `widget`: a radio button or drop-down by the choice offering the same
/// options, a number or text by its place among those shown. `false` if
/// ours has no such field.
fn change(
    panel: &mut Panel,
    widgets: &[Json],
    widget: usize,
    set: &Json,
    warnings: &mut Vec<String>,
) -> bool {
    let fact = &widgets[widget];
    let kind = fact["kind"].as_str().unwrap();
    let choice_with = |panel: &Panel, wanted: &dyn Fn(&[String]) -> bool| {
        panel
            .fields
            .iter()
            .position(|f| matches!(f, Field::Choice { options, .. } if wanted(options)))
    };
    let nth_shown = |panel: &Panel, k: usize, number: bool| {
        (0..panel.fields.len())
            .filter(|&i| panel.shown(i))
            .filter(|&i| match &panel.fields[i] {
                Field::Number { .. } => number,
                Field::Text { .. } | Field::Lines { .. } => !number,
                _ => false,
            })
            .nth(k)
    };
    let before = |what: &str| {
        widgets[..widget]
            .iter()
            .filter(|w| {
                w["kind"] == what
                    && (what != "text"
                        || w["class"] == "QLineEdit"
                        || w["class"] == "QPlainTextEdit")
            })
            .count()
    };
    match kind {
        "radio" | "choice" => {
            let options = strings(if kind == "radio" {
                &fact["group"]
            } else {
                &fact["choices"]
            });
            let Some(i) = choice_with(panel, &|o| o == options.as_slice()) else {
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
        "ticks" => {
            let options = strings(&fact["options"]);
            let Some(i) = panel
                .fields
                .iter()
                .position(|f| matches!(f, Field::Ticks { options: o, .. } if *o == options))
            else {
                return false;
            };
            let option = options.iter().position(|o| o == set).unwrap();
            let on = match &panel.fields[i] {
                Field::Ticks { ticked, .. } => !ticked[option],
                _ => unreachable!(),
            };
            panel.tick(i, option, on);
            true
        }
        "tree" => {
            let i = panel
                .fields
                .iter()
                .position(|f| matches!(f, Field::Tree { .. }))
                .unwrap();
            let Field::Tree { groups } = &panel.fields[i] else {
                unreachable!()
            };
            let (group, child) = set
                .as_str()
                .unwrap()
                .split_once('/')
                .map_or((set.as_str().unwrap(), None), |(g, c)| (g, Some(c)));
            let g = groups.iter().position(|x| x.name == group).unwrap();
            let option = child.map(|c| groups[g].options.iter().position(|o| o == c).unwrap());
            let on = match option {
                Some(o) => !groups[g].ticked[o],
                None => !groups[g].ticked.iter().all(|t| *t),
            };
            panel.tick_tree(i, g, option, on);
            true
        }
        // (the reference's like/dislike and star controls, drop-downs here)
        "like" => {
            let i = choice_with(panel, &|o| o.iter().any(|x| x == "like")).unwrap();
            let wanted = match set.as_str().unwrap() {
                "none" => "(not set)",
                other => other,
            };
            let option = match &panel.fields[i] {
                Field::Choice { options, .. } => options.iter().position(|o| o == wanted).unwrap(),
                _ => unreachable!(),
            };
            panel.choose(i, option);
            true
        }
        "stars" => {
            let stars = fact["num_stars"].as_u64().unwrap();
            let wanted = format!("{}/{stars}", (set.as_f64().unwrap() * stars as f64).round());
            let i = choice_with(panel, &|o| {
                o.iter().any(|x| x.ends_with(&format!("/{stars}")))
            })
            .unwrap();
            let option = match &panel.fields[i] {
                Field::Choice { options, .. } => options.iter().position(|o| *o == wanted).unwrap(),
                _ => unreachable!(),
            };
            panel.choose(i, option);
            true
        }
        // (the reference's service specifier button, its choices in place)
        "services" => {
            let modes: Vec<usize> = (0..panel.fields.len())
                .filter(|&i| {
                    matches!(&panel.fields[i], Field::Choice { options, .. }
                        if options == &["service type", "service"])
                })
                .collect();
            let mode = modes[before("services")];
            if let Some(types) = set.get("types") {
                panel.choose(mode, 0);
                let wanted: Vec<&str> = types
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|t| {
                        ServiceType::from_code(u8::try_from(t.as_u64().unwrap()).unwrap())
                            .unwrap()
                            .short_name()
                    })
                    .collect();
                let Field::Ticks { options, .. } = panel.fields[mode + 1].clone() else {
                    unreachable!()
                };
                for (o, option) in options.iter().enumerate() {
                    panel.tick(mode + 1, o, wanted.contains(&option.as_str()));
                }
            } else {
                panel.choose(mode, 1);
                let wanted = strings(&set["services"]);
                let Field::Ticks { options, .. } = panel.fields[mode + 2].clone() else {
                    unreachable!()
                };
                for (o, option) in options.iter().enumerate() {
                    let name = option.split_once(": ").unwrap().1;
                    panel.tick(mode + 2, o, wanted.iter().any(|w| w == name));
                }
            }
            true
        }
        "button" => {
            let text = fact["text"].as_str().unwrap();
            let i = panel
                .fields
                .iter()
                .position(|f| matches!(f, Field::Button(label) if label == text))
                .unwrap();
            let pressed = panel.press(i);
            let pressed = if matches!(pressed, Pressed::Confirm(_)) {
                // The original recording answers the cleanup question yes.
                panel.press_confirmed(i)
            } else {
                pressed
            };
            if let Pressed::Warning(said) = pressed {
                warnings.push(said);
            }
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
        let ours = Editor::new(blank, &context);
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
                    if !change(&mut changed, widgets, widget, set, &mut Vec::new()) {
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
        let editor = Editor::new(blank, &context);
        let page = usize::try_from(scenario["page"].as_u64().unwrap()).unwrap();
        let panel = usize::try_from(scenario["panel"].as_u64().unwrap()).unwrap();
        let mut panel = editor.pages[page].panels[panel].clone();
        let mut warnings = Vec::new();
        for step in scenario["steps"].as_array().unwrap() {
            let widgets = step["widgets"].as_array().unwrap();
            let widget = usize::try_from(step["widget"].as_u64().unwrap()).unwrap();
            numbers_match(&panel, widgets, &scenario.to_string());
            assert!(
                change(&mut panel, widgets, widget, &step["set"], &mut warnings),
                "{scenario}"
            );
        }
        let steps = scenario["steps"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| format!("{} {}", s["widget"], s["set"]))
            .collect::<Vec<_>>()
            .join(", ");
        let made = panel.predicates(&context).map(|p| texts(&p, &text));
        // what it makes, or why it can't, word for word; and what it warned
        match scenario["predicates"].as_array() {
            Some(_) => assert_eq!(made, Ok(strings(&scenario["predicates"])), "{steps}"),
            None => assert_eq!(
                made,
                Err(scenario["error"].as_str().unwrap().to_owned()),
                "{steps}"
            ),
        }
        assert_eq!(warnings, strings(&scenario["warnings"]), "{steps}");
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
    ui.show().unwrap();
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
fn native_viewtime_milliseconds_survive_accept_recent_reopen_and_cancel() {
    use hydrus_core::search::{predicate::ViewingStat, recent::RecentPredicates};
    let fixture = hydrus_testkit::fixture_json("viewtime_milliseconds.json");
    let (_dirs, store) = store();
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(&ui, Pages::single(SearchPage::new(store.clone())));
    ui.invoke_search_edited("".into());
    for milliseconds in [345, 1001] {
        let case = fixture["cases"]
            .as_array()
            .unwrap()
            .iter()
            .find(|case| {
                case["milliseconds"] == milliseconds
                    && case["operator"] == "="
                    && case["locations"] == serde_json::json!(["media"])
            })
            .unwrap();
        ui.invoke_suggestion_chosen(suggestion(&ui, "system:file viewing statistics"));
        let window = bound
            .predicate_editor
            .borrow()
            .as_ref()
            .unwrap()
            .clone_strong();
        window.invoke_chose(1, 2, 2);
        for (field, value) in [
            (3, 0),
            (4, 0),
            (5, 0),
            (6, milliseconds / 1000),
            (7, milliseconds % 1000),
        ] {
            window.invoke_number_edited(1, field, value);
        }
        if milliseconds == 345 {
            let pixels = headless::render(&windows.get(windows.count() - 1).unwrap(), 1040, 400);
            headless::save_png(
                &std::path::Path::new(env!("CARGO_TARGET_TMPDIR"))
                    .join("viewtime-milliseconds.png"),
                &pixels,
                1040,
                400,
            )
            .unwrap();
        }
        window.invoke_ok(1);
        assert!(bound.predicate_editor.borrow().is_none());
        assert!(shown_predicates(&ui).contains(&case["text"].as_str().unwrap().to_owned()));
        let recent: RecentPredicates = store.read(hydrus_store::settings::get).unwrap();
        let hydrus_search::SystemPredicate::FileViewingStats { stat, value, .. } =
            recent.by_type[&29][0]
        else {
            panic!("missing viewing predicate");
        };
        assert_eq!(stat, ViewingStat::ViewTimeMilliseconds);
        assert_eq!(value, u64::try_from(milliseconds).unwrap());
    }
    let before: RecentPredicates = store.read(hydrus_store::settings::get).unwrap();
    ui.invoke_suggestion_chosen(suggestion(&ui, "system:file viewing statistics"));
    let window = bound
        .predicate_editor
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    assert_eq!(
        window.get_recent().row_data(0).unwrap(),
        "system:viewtime in media = 1.0 seconds"
    );
    window.invoke_number_edited(1, 7, 999);
    window.invoke_cancel();
    window.invoke_ok(1);
    assert_eq!(
        store
            .read(hydrus_store::settings::get::<RecentPredicates>)
            .unwrap(),
        before
    );
    assert_eq!(shown_predicates(&ui).len(), 2);
}

#[test]
fn the_editor_window_shows_what_trees_and_buttons_change() {
    let boundaries = hydrus_testkit::fixture_json("predicate_boundaries.json");
    let (_dirs, store) = store();
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(&ui, Pages::single(SearchPage::new(store.clone())));
    let editor = || {
        bound
            .predicate_editor
            .borrow()
            .as_ref()
            .map(slint::ComponentHandle::clone_strong)
            .expect("an editor")
    };
    let field = |window: &hydrus_gui::PredicateEditorWindow, p: usize, f: usize| {
        window
            .get_panels()
            .row_data(p)
            .unwrap()
            .fields
            .row_data(f)
            .unwrap()
    };
    let last = |ui: &MainWindow| shown_predicates(ui).last().cloned().unwrap_or_default();
    ui.invoke_search_edited("".into());

    // the filetype tree: a group's filetypes show once it is expanded, and
    // a group ticked ticks them all
    ui.invoke_suggestion_chosen(suggestion(&ui, "system:filetype"));
    let window = editor();
    let shown_rows = |window: &hydrus_gui::PredicateEditorWindow| -> Vec<(String, bool)> {
        field(window, 0, 2)
            .rows
            .iter()
            .filter(|r| r.shown)
            .map(|r| (r.text.to_string(), r.ticked))
            .collect()
    };
    let groups = shown_rows(&window);
    assert_eq!(groups.len(), 7);
    assert_eq!(groups[0], ("image".to_owned(), false));
    window.invoke_expanded(0, 2, 0, true);
    let rows = shown_rows(&window);
    assert_eq!(rows.len(), 7 + 12);
    assert_eq!(rows[1], ("jpeg".to_owned(), false));
    assert_eq!(rows[13], ("animation".to_owned(), false));
    let check_tree = |step: usize| {
        let rows = field(&window, 0, 2).rows;
        let group = rows.row_data(0).unwrap();
        let state = if group.partial {
            1
        } else if group.ticked {
            2
        } else {
            0
        };
        assert_eq!(state, boundaries["mime"][step]["state"].as_u64().unwrap());
        let selected: Vec<u64> = rows
            .iter()
            .skip(1)
            .take(12)
            .map(|row| if row.ticked { 2 } else { 0 })
            .collect();
        let recorded: Vec<u64> = boundaries["mime"][step]["children"]
            .as_array()
            .unwrap()
            .iter()
            .map(|value| value.as_u64().unwrap())
            .collect();
        assert_eq!(selected, recorded);
    };
    check_tree(0);
    window.invoke_tree_ticked(0, 2, 0, 1, true);
    check_tree(1);
    window.invoke_tree_ticked(0, 2, 0, -1, true);
    check_tree(2);
    window.invoke_tree_ticked(0, 2, 0, 0, false);
    check_tree(3);
    window.invoke_tree_ticked(0, 2, 0, -1, false);
    check_tree(4);
    window.invoke_tree_ticked(0, 2, 0, 1, true);
    window.invoke_tree_ticked(0, 2, 1, -1, true);
    let rows = shown_rows(&window);
    assert!(rows[2].1, "png ticked");
    assert!(rows[13].1, "animation ticked");
    let tree = field(&window, 0, 2).rows;
    assert!(tree.row_data(0).unwrap().partial);
    assert!(!tree.row_data(13).unwrap().partial);
    window.invoke_expanded(0, 2, 0, false);
    assert_eq!(shown_rows(&window).len(), 7);
    window.invoke_ok(0);
    assert_eq!(last(&ui), "system:filetype is animation, png");

    // "system:hash": a hash of another type is said to be; the clean-up
    // button says which lines aren't hashes, its forced one drops them and
    // chooses the type the rest are, which then makes them
    ui.invoke_suggestion_chosen(suggestion(&ui, "system:hash"));
    let window = editor();
    let md5 = "d41d8cd98f00b204e9800998ecf8427e";
    window.invoke_text_edited(0, 2, format!("{md5}\nnot a hash").into());
    window.invoke_pressed(0, 3);
    assert!(
        window
            .get_error()
            .starts_with("Unfortunately, some hashes did not parse correctly."),
        "{}",
        window.get_error()
    );
    assert!(window.get_error().contains("\"not a hash\""));
    assert_eq!(
        field(&window, 0, 2).text,
        boundaries["hash"][0]["text"].as_str().unwrap()
    );
    window.invoke_pressed(0, 4);
    assert_eq!(
        window.get_question(),
        boundaries["hash"][0]["questions"][0].as_str().unwrap()
    );
    window.invoke_text_edited(0, 2, "changed while question pending".into());
    let frozen_error = window.get_error();
    window.invoke_ok(0);
    assert_eq!(window.get_error(), frozen_error);
    assert!(bound.predicate_editor.borrow().is_some());
    assert!(field(&window, 0, 2).text.contains("not a hash"));
    window.invoke_answer(false);
    assert!(window.get_question().is_empty());
    assert_eq!(
        field(&window, 0, 2).text,
        boundaries["hash"][0]["text"].as_str().unwrap()
    );
    window.invoke_pressed(0, 4);
    window.invoke_answer(true);
    assert_eq!(window.get_error(), "");
    assert_eq!(
        field(&window, 0, 2).text,
        boundaries["hash"][1]["text"].as_str().unwrap()
    );
    let types = field(&window, 0, 5);
    assert_eq!(
        types
            .options
            .row_data(usize::try_from(types.chosen).unwrap()),
        Some("md5".into())
    );
    window.invoke_ok(0);
    assert_eq!(last(&ui), format!("system:hash (md5) is {md5}"));

    // "system:rating": a like chosen chooses "is" with it
    ui.invoke_suggestion_chosen(suggestion(&ui, "system:rating"));
    let window = editor();
    assert_eq!(field(&window, 1, 0).text, "favourites");
    window.invoke_chose(1, 2, 1);
    assert_eq!(field(&window, 1, 1).chosen, 2);
    window.invoke_ok(1);
    assert_eq!(last(&ui), "system:rating for favourites is like");

    // "system:similar files": "clear" empties both hashes
    ui.invoke_suggestion_chosen(suggestion(&ui, "system:similar files"));
    let window = editor();
    window.invoke_text_edited(0, 4, "ab".repeat(32).into());
    window.invoke_text_edited(0, 5, "cd".repeat(8).into());
    window.invoke_pressed(0, 1);
    assert_eq!(field(&window, 0, 4).text, "");
    assert_eq!(field(&window, 0, 5).text, "");
}

#[test]
fn more_suggestions_than_fit_scroll_rather_than_spill_over() {
    let (_dirs, store) = store();
    // Exercise the embedded twelve-row boundary explicitly: the current
    // reference defaults are a floating, twenty-two-row dropdown.
    store
        .write(|ctx| {
            hydrus_store::settings::set(
                ctx.conn(),
                &hydrus_store::settings::FileSearchSettings {
                    float_autocomplete: false,
                    autocomplete_rows: 12,
                    ..Default::default()
                },
            )
        })
        .unwrap();
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
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

    // Read autocomplete paints its actual dense selection mask, as Qt paints
    // selected terms. Mutating only `highlighted` leaves that mask unchanged.
    ui.invoke_suggestion_selection_clicked(0, false, false);
    assert!(ui.invoke_suggestions_deselected());
    assert_eq!(ui.get_suggestion_selected().row_count(), all.len());
    assert!(
        ui.get_suggestion_selected()
            .iter()
            .all(|selected| !selected)
    );
    let plain = draw();
    ui.invoke_suggestion_selection_clicked(0, false, false);
    assert_eq!(ui.get_highlighted(), 0);
    let selected: Vec<_> = ui.get_suggestion_selected().iter().collect();
    assert_eq!(selected.len(), all.len());
    assert!(selected[0]);
    assert!(selected[1..].iter().all(|selected| !selected));
    let first = differing(&plain, &draw());
    let top = *first.first().expect("the highlight drawn");
    // twelve rows of 22 pixels show, under which the window is as it is
    // with no more than twelve suggestions
    let bottom = top + 12 * 22 + 2;
    assert!(ui.invoke_suggestions_deselected());
    assert!(
        ui.get_suggestion_selected()
            .iter()
            .all(|selected| !selected)
    );
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
    let last = i32::try_from(all.len() - 1).unwrap();
    ui.invoke_suggestion_selection_clicked(last, false, false);
    assert_eq!(ui.get_highlighted(), last);
    let selected: Vec<_> = ui.get_suggestion_selected().iter().collect();
    assert_eq!(selected.len(), all.len());
    assert!(selected[all.len() - 1]);
    assert!(selected[..all.len() - 1].iter().all(|selected| !selected));
    let scrolled = differing(&everything, &draw());
    let scroll_y = ui.get_read_scroll_y();
    let view_height = ui.get_read_view_height();
    let content_height = ui.get_read_content_height();
    let last_top = last as f32 * 22.0;
    let last_bottom = last_top + 22.0;
    assert!(scroll_y < 0.0, "last row did not move the scroll viewport");
    assert!(view_height > 0.0);
    assert!(content_height >= last_bottom);
    assert!(scroll_y >= view_height - content_height - 1.0);
    assert!(last_top + scroll_y >= -1.0);
    assert!(last_bottom + scroll_y <= view_height + 1.0);
    assert!(!scrolled.is_empty(), "the last suggestion not shown");
    assert!(
        scrolled.iter().all(|y| (top - 2..bottom).contains(y)),
        "{scrolled:?} outside {top}..{bottom}"
    );

    // Moving back to the first row uses the same real selection callback and
    // scrolls upward, rather than leaving an off-screen dense selection mask.
    ui.invoke_suggestion_selection_clicked(0, false, false);
    let first_again = differing(&plain, &draw());
    assert_eq!(ui.get_read_scroll_y(), 0.0);
    assert!(!first_again.is_empty(), "the first suggestion not restored");
    assert!(first_again.iter().all(|y| (top - 2..bottom).contains(y)));
}

#[test]
fn predicate_star_save_and_reset_survive_cancel_and_reach_future_searches() {
    use hydrus_gui::predicate_editors::defaults::CustomDefaults;
    use slint::ComponentHandle as _;
    let recording = hydrus_testkit::fixture_json("predicate_custom_defaults.json");
    let (dirs, store) = store();
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(&ui, Pages::single(SearchPage::new(store.clone())));
    let editor = || {
        bound
            .predicate_editor
            .borrow()
            .as_ref()
            .unwrap()
            .clone_strong()
    };
    let open = || {
        ui.invoke_search_edited("".into());
        ui.invoke_suggestion_chosen(suggestion(&ui, "system:limit"));
        editor()
    };
    let actions = |window: &hydrus_gui::PredicateEditorWindow| {
        window
            .get_defaults_actions()
            .iter()
            .map(|a| a.to_string())
            .collect::<Vec<_>>()
    };
    let case = recording["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["class"] == "PanelPredicateSystemLimit")
        .unwrap();
    let window = open();
    window.invoke_defaults_menu(0);
    assert_eq!(actions(&window), strings(&case["menu_before"]));
    window.invoke_number_edited(0, 1, 731);
    window.invoke_defaults_action(0, "set this as new default".into());
    assert!(window.get_error().is_empty());
    window.invoke_defaults_menu(0);
    assert_eq!(actions(&window), strings(&case["menu_after_save"]));
    let kept = store
        .read(hydrus_store::settings::get::<CustomDefaults>)
        .unwrap();
    assert_eq!(
        kept.predicates,
        vec![Predicate::System(hydrus_search::SystemPredicate::Limit(
            731
        ))]
    );
    window.invoke_cancel();
    assert!(shown_predicates(&ui).is_empty());
    let window = open();
    assert_eq!(
        window
            .get_panels()
            .row_data(0)
            .unwrap()
            .fields
            .row_data(1)
            .unwrap()
            .value,
        731
    );
    window.invoke_defaults_action(0, "reset to original default".into());
    assert!(
        store
            .read(hydrus_store::settings::get::<CustomDefaults>)
            .unwrap()
            .predicates
            .is_empty()
    );
    assert_eq!(
        window
            .get_panels()
            .row_data(0)
            .unwrap()
            .fields
            .row_data(1)
            .unwrap()
            .value,
        731
    );
    window.invoke_defaults_menu(0);
    assert_eq!(actions(&window), strings(&case["menu_before"]));
    window.invoke_cancel();
    window.invoke_defaults_action(0, "set this as new default".into());
    assert!(
        store
            .read(hydrus_store::settings::get::<CustomDefaults>)
            .unwrap()
            .predicates
            .is_empty()
    );
    let window = open();
    assert_eq!(
        window
            .get_panels()
            .row_data(0)
            .unwrap()
            .fields
            .row_data(1)
            .unwrap()
            .value,
        256
    );
    window.invoke_number_edited(0, 1, 731);
    window.invoke_defaults_action(0, "set this as new default".into());
    window.invoke_cancel();
    let window = open();
    window.invoke_defaults_menu(0);
    let pixels = headless::render(&windows.get(windows.count() - 1).unwrap(), 1020, 540);
    headless::save_png(
        &std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("predicate_custom_defaults.png"),
        &pixels,
        1020,
        540,
    )
    .unwrap();
    window.invoke_ok(0);
    assert_eq!(shown_predicates(&ui), ["system:limit is 731"]);
    drop(bound);
    drop(ui);
    drop(store);
    let reopened = Store::open(dirs[1].path()).unwrap();
    assert_eq!(
        reopened
            .read(hydrus_store::settings::get::<CustomDefaults>)
            .unwrap()
            .predicates,
        kept.predicates
    );
}

#[test]
fn regex_star_save_is_immediate_but_acceptance_checks_and_viewtime_keeps_milliseconds() {
    use hydrus_gui::predicate_editors::defaults::CustomDefaults;
    use slint::ComponentHandle as _;
    let (_dirs, store) = store();
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(&ui, Pages::single(SearchPage::new(store.clone())));
    ui.invoke_search_edited("".into());
    ui.invoke_suggestion_chosen(suggestion(&ui, "system:urls"));
    let window = bound
        .predicate_editor
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    window.invoke_text_edited(2, 3, "predicate-defaults\\.example".into());
    window.invoke_defaults_action(2, "set this as new default".into());
    window.invoke_text_edited(2, 3, "[".into());
    window.invoke_defaults_action(2, "set this as new default".into());
    assert!(window.get_error().is_empty());
    let kept = store
        .read(hydrus_store::settings::get::<CustomDefaults>)
        .unwrap();
    assert_eq!(
        kept.predicates,
        vec![Predicate::System(
            hydrus_search::SystemPredicate::KnownUrl {
                rule: hydrus_core::search::predicate::UrlRule::Regex("[".into()),
                has: true,
            }
        )]
    );
    window.invoke_ok(2);
    assert!(window.get_error().contains("regex"));
    assert!(shown_predicates(&ui).is_empty());
    window.invoke_cancel();
    assert_eq!(
        store
            .read(hydrus_store::settings::get::<CustomDefaults>)
            .unwrap(),
        kept
    );
    ui.invoke_search_edited("".into());
    ui.invoke_suggestion_chosen(suggestion(&ui, "system:urls"));
    let window = bound
        .predicate_editor
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    assert_eq!(
        window
            .get_panels()
            .row_data(2)
            .unwrap()
            .fields
            .row_data(3)
            .unwrap()
            .text,
        "["
    );
    window.invoke_defaults_menu(2);
    let recording = hydrus_testkit::fixture_json("predicate_custom_defaults.json");
    assert_eq!(
        window
            .get_defaults_actions()
            .iter()
            .map(|a| a.to_string())
            .collect::<Vec<_>>(),
        strings(&recording["invalid_regex"]["menu_after_save"])
    );
    window.invoke_cancel();
    ui.invoke_search_edited("".into());
    ui.invoke_suggestion_chosen(suggestion(&ui, "system:file viewing statistics"));
    let window = bound
        .predicate_editor
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    window.invoke_number_edited(1, 7, 37);
    window.invoke_defaults_action(1, "set this as new default".into());
    window.invoke_cancel();
    ui.invoke_search_edited("".into());
    ui.invoke_suggestion_chosen(suggestion(&ui, "system:file viewing statistics"));
    let window = bound
        .predicate_editor
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    assert_eq!(
        window
            .get_panels()
            .row_data(1)
            .unwrap()
            .fields
            .row_data(7)
            .unwrap()
            .value,
        37
    );
    window.invoke_ok(1);
    let defaults = store
        .read(hydrus_store::settings::get::<CustomDefaults>)
        .unwrap();
    let recent = store
        .read(hydrus_store::settings::get::<hydrus_core::search::recent::RecentPredicates>)
        .unwrap();
    let value = defaults
        .predicates
        .iter()
        .find(|p| {
            matches!(
                p,
                Predicate::System(hydrus_search::SystemPredicate::FileViewingStats { .. })
            )
        })
        .unwrap();
    assert!(matches!(
        value,
        Predicate::System(hydrus_search::SystemPredicate::FileViewingStats {
            stat: hydrus_core::search::predicate::ViewingStat::ViewTimeMilliseconds,
            value: 600_037,
            ..
        })
    ));
    assert!(
        recent.by_type[&29]
            .iter()
            .any(|p| Predicate::System(p.clone()) == *value)
    );
}

#[test]
fn imported_predicate_defaults_reach_panels_and_reset_never_resurrects_legacy_values() {
    use hydrus_gui::predicate_editors::defaults::CustomDefaults;
    use slint::ComponentHandle as _;
    let recording = hydrus_testkit::fixture_json("predicate_custom_defaults.json");
    let source = hydrus_testkit::legacy_fixture("basic");
    let destination = tempfile::tempdir().unwrap();
    let conn = rusqlite::Connection::open(source.path().join("client.db")).unwrap();
    conn.execute(
        "UPDATE json_dumps SET dump = ?1 WHERE dump_type = 22",
        [recording["options_saved_families"][2]
            .to_string()
            .into_bytes()],
    )
    .unwrap();
    drop(conn);
    import_legacy(
        source.path(),
        &destination.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store = Store::open(destination.path()).unwrap();
    assert_eq!(
        store
            .read(hydrus_store::settings::get::<CustomDefaults>)
            .unwrap()
            .predicates
            .len(),
        38
    );
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(&ui, Pages::single(SearchPage::new(store.clone())));
    let open = || {
        ui.invoke_search_edited("".into());
        ui.invoke_suggestion_chosen(suggestion(&ui, "system:limit"));
        bound
            .predicate_editor
            .borrow()
            .as_ref()
            .unwrap()
            .clone_strong()
    };
    let window = open();
    assert_eq!(
        window
            .get_panels()
            .row_data(0)
            .unwrap()
            .fields
            .row_data(1)
            .unwrap()
            .value,
        259
    );
    window.invoke_defaults_menu(0);
    assert_eq!(window.get_defaults_actions().row_count(), 2);
    window.invoke_defaults_action(0, "reset to original default".into());
    assert_eq!(
        window
            .get_panels()
            .row_data(0)
            .unwrap()
            .fields
            .row_data(1)
            .unwrap()
            .value,
        259
    );
    window.invoke_cancel();
    assert!(shown_predicates(&ui).is_empty());
    let window = open();
    assert_eq!(
        window
            .get_panels()
            .row_data(0)
            .unwrap()
            .fields
            .row_data(1)
            .unwrap()
            .value,
        256
    );
    window.invoke_defaults_menu(0);
    assert_eq!(window.get_defaults_actions().row_count(), 1);
    window.invoke_cancel();
    drop(bound);
    drop(ui);
    drop(store);
    let reopened = Store::open(destination.path()).unwrap();
    let defaults = reopened
        .read(hydrus_store::settings::get::<CustomDefaults>)
        .unwrap();
    assert_eq!(defaults.predicates.len(), 37);
    assert!(!defaults.predicates.iter().any(|p| matches!(
        p,
        Predicate::System(hydrus_search::SystemPredicate::Limit(_))
    )));
}
