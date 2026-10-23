//! The auto-resolution "test A or B using search terms" comparator's editor
//! and its embedded search, driven as a user would and compared step by step
//! with the reference's real panel (`oracle/record_metadata_conditional_editor.py`).

use std::sync::Arc;

use serde_json::{Value as Json, json};
use slint::{ComponentHandle as _, Model as _};

use hydrus_core::duplicates::{DuplicatesSearch, PairSearchKind, PixelDuplicates};
use hydrus_core::pages::{DuplicatesPage, Page, PageContent, PageKey, Session};
use hydrus_gui::{Bound, ComparatorWindow, MainWindow, Pages, bind, headless};
use hydrus_search::{FileSearchContext, LocationContext};
use hydrus_store::sessions::{self, LAST_SESSION};

use crate::duplicates_lane_page::{kind_index, list_of, newest_comparator, rule_window};

/// The basic fixture's store, with a duplicates page open on its files.
fn opened() -> (
    headless::Windows,
    [tempfile::TempDir; 2],
    Arc<hydrus_store::Store>,
    MainWindow,
    Bound,
) {
    let windows = headless::init();
    let (dirs, store) = super::subscriptions::store();
    let (_, key) = super::duplicate_filter::my_files(&store);
    let search = FileSearchContext {
        location: LocationContext::single(key),
        ..FileSearchContext::default()
    };
    let session = Session {
        name: LAST_SESSION.into(),
        pages: vec![Page {
            key: PageKey::random(),
            name: "duplicates".into(),
            content: PageContent::Duplicates {
                duplicates: DuplicatesPage::new(DuplicatesSearch {
                    search_1: search.clone(),
                    search_2: search,
                    kind: PairSearchKind::OneFileMatchesOneSearch,
                    pixel_duplicates: PixelDuplicates::Allowed,
                    max_hamming_distance: 4,
                }),
                sort: None,
            },
        }],
    };
    store
        .write(move |ctx| sessions::save(ctx.conn(), &session, 0))
        .unwrap();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(Arc::clone(&store)).unwrap());
    (windows, dirs, store, ui, bound)
}

/// Presses `key` in the comparator's editor, with Shift held if `shift`.
fn press(window: &ComparatorWindow, key: slint::platform::Key, shift: bool) {
    use slint::platform::{Key, WindowEvent};
    let native = window.window();
    if shift {
        native.dispatch_event(WindowEvent::KeyPressed {
            text: Key::Shift.into(),
        });
    }
    native.dispatch_event(WindowEvent::KeyPressed { text: key.into() });
    native.dispatch_event(WindowEvent::KeyReleased { text: key.into() });
    if shift {
        native.dispatch_event(WindowEvent::KeyReleased {
            text: Key::Shift.into(),
        });
    }
}

/// The results as drawn, each a row of lines.
fn rows_drawn(window: &ComparatorWindow) -> Json {
    json!(
        window
            .get_search_suggestions()
            .iter()
            .map(|s| s.split('\n').map(str::to_owned).collect::<Vec<_>>())
            .collect::<Vec<_>>()
    )
}

fn highlighted(window: &ComparatorWindow) -> Json {
    let at = window.get_search_highlighted();
    json!(if at < 0 { vec![] } else { vec![at] })
}

/// Where hydrus-rs lists otherwise than the reference, on purpose
/// (DIFFERENCES.md; DECISIONS.md, "where the reference has a bug, ours
/// stands"): the reference's `ResultsFetch` lists nothing for typed system
/// text while no OR is being built, and nothing for typed tags while one is;
/// hydrus-rs lists a system predicate that can be tested on one file, and
/// searches tags, either way. (the typed text, whether an OR is being built,
/// the rows hydrus-rs lists, its highlighted row)
const OURS: &[(&str, bool, &[&[&str]], i64)] = &[
    ("system:inbox", false, &[&["system:inbox"]], 0),
    ("system:width > 100", false, &[&["system:width > 100"]], 0),
    (
        "system:filetype is image",
        false,
        &[&["system:filetype is image"]],
        0,
    ),
    ("system:archive", false, &[&["system:archive"]], 0),
    ("system:has audio", false, &[&["system:has audio"]], 0),
    ("system:height < 50", false, &[&["system:height < 50"]], 0),
    (
        "blue",
        true,
        &[&["OR:", "    system:inbox"], &["blue eyes (11-19)"]],
        1,
    ),
];

/// [`OURS`]' listing for `text`, if it is one of them.
fn ours(text: &str, or_active: bool) -> Option<(Json, Json)> {
    OURS.iter()
        .find(|(typed, or, _, _)| *typed == text && *or == or_active)
        .map(|(_, _, rows, at)| (json!(rows), json!([at])))
}

// leaf: audit-media-comparator-conditional
#[test]
fn the_metadata_conditional_editor_searches_as_the_references_does() {
    use slint::platform::Key;
    let fixture = hydrus_testkit::fixture_json("metadata_conditional_editor.json");
    let (_windows, _dirs, store, ui, bound) = opened();
    let clipboard = std::rc::Rc::new(std::cell::RefCell::new(String::new()));
    hydrus_gui::set_paster({
        let clipboard = clipboard.clone();
        move || clipboard.borrow().clone()
    });
    ui.invoke_duplicates_action("edit rules".into(), 0, false, false);
    let list = list_of(&bound);
    list.invoke_add();
    let rule = rule_window(&bound);
    rule.set_comparator_kind(kind_index(&rule, "test A or B using search terms"));
    rule.invoke_comparator_add();
    let c = newest_comparator(&bound);
    // (the user clicks into the search box)
    c.invoke_focus_search();
    let editor = || {
        bound
            .auto_resolution
            .conditional
            .predicate_editor
            .borrow()
            .as_ref()
            .map(slint::ComponentHandle::clone_strong)
    };
    let mut differences = Vec::new();
    let mut differ = |what: &Json, key: &str, ours: Json, want: &Json| {
        if ours != *want {
            differences.push(format!("{what}: {key} {ours} not {want}"));
        }
    };
    // our listing for text a deliberate difference left in the box
    let mut left: Option<(String, Json, Json)> = None;
    for step in fixture["steps"].as_array().unwrap() {
        let what = &step["step"];
        let listed = &step["listed"];
        match what[0].as_str().unwrap() {
            "initial" => {}
            kind @ ("type" | "shift type") => {
                let text = what[1].as_str().unwrap();
                let or_active = c.get_search_or_active();
                c.set_search_typed(text.into());
                c.invoke_search_edited();
                // (the list as it is when Return is pressed)
                left = None;
                if let Some((rows, at)) = ours(text, or_active) {
                    // a deliberate difference: what hydrus-rs lists, then
                    // what the reference's Return did (nothing with nothing
                    // listed; with only its OR listed, it entered the OR)
                    differ(what, "our listed rows", rows_drawn(&c), &rows);
                    differ(what, "our listed highlight", highlighted(&c), &at);
                    if or_active {
                        c.invoke_search_chosen(0, kind == "shift type");
                    } else {
                        // (the text stays, and so does our listing for it)
                        left = Some((text.to_owned(), rows, at));
                    }
                } else {
                    differ(what, "listed rows", rows_drawn(&c), &listed["rows_drawn"]);
                    differ(
                        what,
                        "listed highlight",
                        highlighted(&c),
                        &listed["highlighted"],
                    );
                    if !text.is_empty() {
                        press(&c, Key::Return, kind == "shift type");
                    }
                }
            }
            "double click" => {
                c.invoke_search_removed(i32::try_from(what[1].as_i64().unwrap()).unwrap());
            }
            "looking at" => {
                c.set_looking(i32::try_from(what[1].as_i64().unwrap()).unwrap());
                c.invoke_changed();
            }
            "paste" => {
                what[1]
                    .as_str()
                    .unwrap()
                    .clone_into(&mut clipboard.borrow_mut());
                c.invoke_search_paste();
            }
            "empty search" => c.invoke_search_empty(),
            "escape" => press(&c, Key::Escape, false),
            "or button" => {
                c.invoke_search_or_action(3);
                let child = bound
                    .auto_resolution
                    .conditional
                    .search_or
                    .borrow()
                    .as_ref()
                    .map(slint::ComponentHandle::clone_strong)
                    .expect("the OR editor open");
                differ(
                    what,
                    "OR editor title",
                    json!(child.get_window_title().to_string()),
                    &listed["title"],
                );
                if let Some(typed) = listed["typed"].as_array() {
                    for typing in typed {
                        let text = typing["text"].as_str().unwrap();
                        child.set_input(text.into());
                        child.invoke_edited(text.into());
                        let rows: Vec<Vec<String>> = child
                            .get_suggestions()
                            .iter()
                            .map(|row| vec![row.text.to_string()])
                            .collect();
                        if let Some((ours, _)) = ours(text, false) {
                            // (a deliberate difference, as above: the
                            // reference's Return did nothing)
                            differ(what, "OR editor our rows", json!(rows), &ours);
                        } else {
                            differ(what, "OR editor rows", json!(rows), &typing["rows_drawn"]);
                            child.invoke_enter(false);
                        }
                        let shown: Vec<String> = child
                            .get_predicates()
                            .iter()
                            .map(|p| p.to_string())
                            .collect();
                        differ(what, "OR editor shown", json!(shown), &typing["shown"]);
                    }
                    child.invoke_apply();
                } else {
                    child.invoke_cancel();
                }
                assert!(
                    bound
                        .auto_resolution
                        .conditional
                        .search_or
                        .borrow()
                        .is_none()
                );
            }
            kind @ ("activate row" | "shift activate row") => {
                // the row highlighted with the keys, then Return
                let row = what[1].as_i64().unwrap();
                press(&c, Key::Home, false);
                for _ in 0..row {
                    press(&c, Key::DownArrow, false);
                }
                differ(what, "highlight", highlighted(&c), &json!([row]));
                let activated = c
                    .get_search_suggestions()
                    .row_data(usize::try_from(row).unwrap())
                    .map(|s| s.split('\n').next().unwrap_or_default().to_owned());
                differ(what, "activated", json!(activated), &listed["activated"]);
                press(&c, Key::Return, kind == "shift activate row");
                let editors = listed["editors"].as_array().unwrap();
                match (editor(), editors.first()) {
                    (None, None) => {}
                    (Some(child), Some(want)) => {
                        let title = child.get_composed_title();
                        let title = title
                            .strip_suffix(child.get_title_suffix().as_str())
                            .unwrap_or(&title);
                        differ(what, "editor title", json!(title), &want["title"]);
                        let buttons: Vec<String> =
                            child.get_buttons().iter().map(|b| b.to_string()).collect();
                        // (the recording lists the buttons of fixed predicates,
                        // not the dimension presets)
                        let fixed: Vec<&String> = buttons
                            .iter()
                            .filter(|b| b.starts_with("system:"))
                            .collect();
                        differ(
                            what,
                            "editor buttons",
                            json!(fixed),
                            &want["static_buttons"],
                        );
                        match &what[2] {
                            Json::String(answer) if answer == "cancel" => child.invoke_cancel(),
                            Json::String(button) => {
                                let at = buttons.iter().position(|b| b == button).unwrap();
                                child.invoke_button_clicked(i32::try_from(at).unwrap());
                            }
                            panel => {
                                child.invoke_ok(i32::try_from(panel.as_i64().unwrap()).unwrap());
                            }
                        }
                        assert!(editor().is_none(), "{what}: the editor stays open");
                    }
                    (ours, want) => differ(
                        what,
                        "editor open",
                        json!(ours.is_some()),
                        &json!(want.is_some()),
                    ),
                }
            }
            other => panic!("unknown step {other}"),
        }
        let want = &step["state"];
        let strs = |model: slint::ModelRc<slint::SharedString>| -> Vec<String> {
            model.iter().map(|s| s.to_string()).collect()
        };
        differ(
            what,
            "looking_at_choices",
            json!(strs(c.get_looking_choices())),
            &want["looking_at_choices"],
        );
        differ(
            what,
            "looking_at",
            json!(c.get_looking()),
            &want["looking_at"],
        );
        match &left {
            Some((typed, rows, at)) if c.get_search_typed() == typed.as_str() => {
                differ(what, "our rows_drawn", rows_drawn(&c), rows);
                differ(what, "our highlighted", highlighted(&c), at);
            }
            _ => {
                differ(what, "rows_drawn", rows_drawn(&c), &want["rows_drawn"]);
                differ(what, "highlighted", highlighted(&c), &want["highlighted"]);
            }
        }
        differ(
            what,
            "shown",
            json!(strs(c.get_search_predicates())),
            &want["shown"],
        );
        differ(
            what,
            "cancel_shown",
            json!(c.get_search_or_active()),
            &want["cancel_shown"],
        );
        differ(
            what,
            "rewind_shown",
            json!(c.get_search_or_rewind_visible()),
            &want["rewind_shown"],
        );
        differ(
            what,
            "typed_left",
            json!(c.get_search_typed().to_string()),
            &want["typed_left"],
        );
        differ(
            what,
            "summary",
            json!(c.get_summary().to_string()),
            &want["summary"],
        );
    }
    // what the comparator gives back, kept and opened again
    let last = &fixture["steps"].as_array().unwrap().last().unwrap()["state"];
    c.invoke_apply();
    rule.invoke_apply();
    list.invoke_apply();
    let written = store.read(hydrus_store::duplicates::auto::rules).unwrap();
    let hydrus_store::duplicates::auto::Comparator::OneFileMetadata { predicates, .. } =
        &written[0].1.comparators[0]
    else {
        panic!("{:?}", written[0].1.comparators)
    };
    let text = hydrus_search::TextContext::from_store(
        &store.snapshot().services,
        &store.read(hydrus_store::settings::get).unwrap(),
    );
    let mut saved: Vec<String> = predicates
        .iter()
        .map(|p| hydrus_search::predicate_text(p, &text))
        .collect();
    saved.sort();
    differ(&json!("applied"), "saved", json!(saved), &last["saved"]);
    ui.invoke_duplicates_action("edit rules".into(), 0, false, false);
    let list = list_of(&bound);
    list.invoke_row_clicked(0, false, false);
    list.invoke_edit();
    let rule = rule_window(&bound);
    rule.invoke_comparator_clicked(0);
    rule.invoke_comparator_edit();
    let c = newest_comparator(&bound);
    differ(
        &json!("reopened"),
        "shown",
        json!(
            c.get_search_predicates()
                .iter()
                .map(|s| s.to_string())
                .collect::<Vec<_>>()
        ),
        &last["shown"],
    );
    differ(
        &json!("reopened"),
        "summary",
        json!(c.get_summary().to_string()),
        &last["summary"],
    );
    // and Return on typed system text enters it (where the reference's
    // listed nothing)
    c.invoke_focus_search();
    c.set_search_typed("system:width > 100".into());
    c.invoke_search_edited();
    press(&c, Key::Return, false);
    assert!(
        c.get_search_predicates()
            .iter()
            .any(|p| p == "system:width > 100"),
        "{:?}",
        c.get_search_predicates().iter().collect::<Vec<_>>()
    );
    assert!(differences.is_empty(), "{}", differences.join("\n"));
}
