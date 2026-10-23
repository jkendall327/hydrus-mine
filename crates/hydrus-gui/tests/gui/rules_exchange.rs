//! The auto-resolution rules list's and comparator lists' export, import and
//! duplicate, replayed through the real windows against the reference's own
//! (`oracle/fixtures/rules_exchange.json`, from
//! `oracle/record_rules_exchange.py`, on the `basic` fixture whose services
//! the store is imported from): rules and comparators export as the
//! reference's serialised text, and pasting and duplicating add, select and
//! say what the reference's lists do.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use serde_json::Value;
use slint::{ComponentHandle as _, Model as _};

use hydrus_core::duplicates::{DuplicatesSearch, PairSearchKind, PixelDuplicates};
use hydrus_core::pages::{DuplicatesPage, Page, PageContent, PageKey, Session};
use hydrus_gui::{AutoResolutionRulesWindow, Bound, MainWindow, Pages, bind, headless};
use hydrus_search::{FileSearchContext, LocationContext};
use hydrus_store::Store;
use hydrus_store::sessions::{self, LAST_SESSION};
use hydrus_testkit::serialised::normalised_rule_export as normal;

struct Opened {
    _windows: headless::Windows,
    _dir: tempfile::TempDir,
    _ui: MainWindow,
    bound: Bound,
    copied: Rc<RefCell<Vec<String>>>,
}

fn recorded() -> Value {
    hydrus_testkit::fixture_json("rules_exchange.json")
}

/// The `basic` fixture imported (its rating services are the recording's),
/// with a duplicates page open, and "edit rules" opened from it.
fn opened() -> (Opened, AutoResolutionRulesWindow) {
    let windows = headless::init();
    let legacy = hydrus_testkit::legacy_fixture("basic");
    let dir = tempfile::tempdir().unwrap();
    hydrus_store::import::import_legacy(
        legacy.path(),
        &dir.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store = Store::open(dir.path()).unwrap();
    let search = FileSearchContext {
        location: LocationContext::single(hydrus_core::ServiceKey::new(b"local files".to_vec())),
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
    let copied: Rc<RefCell<Vec<String>>> = Rc::default();
    hydrus_gui::set_clipper({
        let copied = copied.clone();
        move |clip| {
            if let hydrus_gui::Clip::Text(text) = clip {
                copied.borrow_mut().push(text.clone());
            }
        }
    });
    ui.invoke_duplicates_action("edit rules".into(), 0, false, false);
    let list = bound
        .auto_resolution
        .list
        .borrow()
        .as_ref()
        .expect("it opens")
        .clone_strong();
    assert_eq!(list.get_rows().row_count(), 0, "the fixture has no rules");
    (
        Opened {
            _windows: windows,
            _dir: dir,
            _ui: ui,
            bound,
            copied,
        },
        list,
    )
}

pub(crate) fn paste(text: Option<&str>) {
    let text = text.map(str::to_owned);
    hydrus_gui::set_clipboard_reader(move || Ok(text.clone()));
}

/// Every message shown, oldest first, each dismissed in turn (the next
/// comes once the one before is), as the recording writes them.
pub(crate) fn said() -> Vec<Value> {
    let mut out = Vec::new();
    while let Some(window) = hydrus_gui::message_window() {
        if !window.window().is_visible() {
            break;
        }
        let title = window.get_window_title().to_string();
        let text = window.get_message().to_string();
        out.push(match title.as_str() {
            "Information" => serde_json::json!({"kind": "information", "text": text}),
            "Warning" => serde_json::json!({"kind": "warning", "text": text}),
            _ => serde_json::json!({"kind": "critical", "title": title, "text": text}),
        });
        window.invoke_cancelled();
        assert!(out.len() < 10, "messages keep coming: {out:?}");
    }
    out
}

/// The rules list: each row's name, and the selected ones.
fn rules(list: &AutoResolutionRulesWindow) -> (Vec<String>, Vec<String>) {
    let rows = list.get_rows();
    let mut names = Vec::new();
    let mut selected = Vec::new();
    for r in 0..rows.row_count() {
        let row = rows.row_data(r).unwrap();
        let name = row.cells.row_data(0).unwrap().to_string();
        if row.selected {
            selected.push(name.clone());
        }
        names.push(name);
    }
    (names, selected)
}

fn select_rules(list: &AutoResolutionRulesWindow, wanted: &[Value]) {
    let (names, selected) = rules(list);
    // (a control-click on each selected row clears the selection)
    for name in &selected {
        let row = names.iter().position(|n| n == name).unwrap();
        list.invoke_row_clicked(i32::try_from(row).unwrap(), true, false);
    }
    for name in wanted {
        let row = names
            .iter()
            .position(|n| n == name.as_str().unwrap())
            .unwrap_or_else(|| panic!("{name} is listed"));
        list.invoke_row_clicked(i32::try_from(row).unwrap(), true, false);
    }
    assert_eq!(serde_json::json!(rules(list).1.len()), wanted.len());
}

fn strings(model: &slint::ModelRc<slint::SharedString>) -> Vec<String> {
    model.iter().map(|s| s.to_string()).collect()
}

fn picked(model: &slint::ModelRc<bool>) -> Vec<usize> {
    model
        .iter()
        .enumerate()
        .filter_map(|(i, p)| p.then_some(i))
        .collect()
}

/// A comparator list, as a test drives it: its rows, selected rows, a click
/// and the exchange buttons.
struct ComparatorList<'a> {
    rows: &'a dyn Fn() -> Vec<String>,
    picked: &'a dyn Fn() -> Vec<usize>,
    click: &'a dyn Fn(usize, bool),
    exchange: &'a dyn Fn(i32),
}

impl ComparatorList<'_> {
    fn select(&self, wanted: &[usize]) {
        for row in (self.picked)() {
            (self.click)(row, true);
        }
        for &row in wanted {
            (self.click)(row, true);
        }
        assert_eq!((self.picked)(), wanted);
    }

    fn state(&self) -> Value {
        serde_json::json!({"rows": (self.rows)(), "selected": (self.picked)()})
    }
}

// leaf: audit-media-rules-exchange
#[test]
fn rules_and_comparators_export_import_and_duplicate_as_the_reference_s_lists_do() {
    let rec = recorded();
    let (opened, list) = opened();
    let copied = opened.copied.clone();

    // the reference's suggestions, added as a user adds them, and the
    // recorded rules pasted in (the reference's own export of them)
    for i in 0..8 {
        list.invoke_add_suggested();
        list.invoke_suggested_chosen(i);
    }
    let exports = rec["exports"].as_array().unwrap();
    let custom = exports.last().unwrap();
    paste(custom["text"].as_str());
    list.invoke_exchange(3);
    let pasted = custom["names"].as_array().unwrap().len();
    assert_eq!(
        serde_json::json!(said()),
        serde_json::json!([{"kind": "information", "text": format!("{pasted} objects added!")}])
    );
    assert_eq!(rules(&list).0.len(), 8 + pasted);

    // export: one rule, the suggestions together, the recorded ones
    // together; in the list's order, as the reference's text
    for export in exports {
        select_rules(&list, export["names"].as_array().unwrap());
        assert_eq!(
            serde_json::json!(rules(&list).1),
            export["selected_in_order"]
        );
        let before = copied.borrow().len();
        list.invoke_exchange(0);
        let text = copied.borrow().get(before).cloned().expect("copied");
        assert_eq!(
            normal(&text),
            normal(export["text"].as_str().unwrap()),
            "{}",
            export["names"]
        );
    }

    // the rule editor's comparator list
    select_rules(&list, &[Value::from("wide")]);
    list.invoke_edit();
    let rule = opened
        .bound
        .auto_resolution
        .rule
        .borrow()
        .as_ref()
        .expect("the rule editor opens")
        .clone_strong();
    let comparators = ComparatorList {
        rows: &|| strings(&rule.get_comparators()),
        picked: &|| picked(&rule.get_comparator_picked()),
        click: &|row, ctrl| {
            rule.invoke_comparator_clicked(i32::try_from(row).unwrap(), ctrl, false);
        },
        exchange: &|mode| rule.invoke_comparator_exchange(mode),
    };
    assert_eq!(
        serde_json::json!((comparators.rows)()),
        rec["comparator_imports"]["start"]["rows"]
    );
    for export in rec["comparators"].as_array().unwrap() {
        let rows: Vec<usize> = serde_json::from_value(export["rows"].clone()).unwrap();
        comparators.select(&rows);
        let before = copied.borrow().len();
        (comparators.exchange)(0);
        let text = copied.borrow().get(before).cloned().expect("copied");
        assert_eq!(
            normal(&text),
            normal(export["text"].as_str().unwrap()),
            "{rows:?}"
        );
    }
    replay_steps(
        &comparators,
        &copied,
        &rec["comparator_imports"],
        "the rule's comparators",
    );

    // an OR's list (the rule's tenth comparator), and the AND within it
    let innermost = || {
        opened
            .bound
            .auto_resolution
            .comparators
            .borrow()
            .last()
            .expect("a comparator editor opens")
            .1
            .clone_strong()
    };
    comparators.select(&[9]);
    rule.invoke_comparator_edit();
    let or = innermost();
    let sub_list = |window: &hydrus_gui::ComparatorWindow, run: &Value, what: &str| {
        let list = ComparatorList {
            rows: &|| strings(&window.get_subs()),
            picked: &|| picked(&window.get_sub_picked()),
            click: &|row, ctrl| window.invoke_sub_clicked(i32::try_from(row).unwrap(), ctrl, false),
            exchange: &|mode| window.invoke_sub_exchange(mode),
        };
        replay_steps(&list, &copied, run, what);
    };
    sub_list(&or, &rec["or_list"], "an OR's comparators");
    // (the AND, the OR's first)
    or.invoke_sub_clicked(0, false, false);
    or.invoke_sub_edit();
    let and = innermost();
    sub_list(&and, &rec["and_list"], "an AND's comparators");
    hydrus_gui::clear_clipboard_reader();
}

// leaf: audit-media-rules-exchange
#[test]
fn rules_paste_and_duplicate_add_select_and_say_as_the_reference_s_list_does() {
    let rec = recorded();
    let run = &rec["rule_imports"];
    let (_opened, list) = opened();
    let text_of = |name: &str| {
        rec["exports"]
            .as_array()
            .unwrap()
            .iter()
            .find(|e| e["names"] == serde_json::json!([name]))
            .unwrap()["text"]
            .as_str()
            .unwrap()
            .to_owned()
    };
    // the recording's starting rules: a suggestion, and "worse"
    list.invoke_add_suggested();
    list.invoke_suggested_chosen(0);
    paste(Some(&text_of("worse")));
    list.invoke_exchange(3);
    said();
    select_rules(&list, &[]);
    let (names, selected) = rules(&list);
    assert_eq!(
        serde_json::json!({"names": names, "selected": selected}),
        run["start"]
    );

    for step in run["steps"].as_array().unwrap() {
        let name = step["name"].as_str().unwrap();
        if step["action"] == "paste" {
            select_rules(&list, &[]);
            paste(step["text"].as_str());
            list.invoke_exchange(3);
        } else {
            select_rules(
                &list,
                &[Value::from("pixel-perfect pairs"), Value::from("worse")],
            );
            list.invoke_exchange(6);
        }
        assert_eq!(serde_json::json!(said()), step["messages"], "{name}");
        let (names, selected) = rules(&list);
        assert_eq!(serde_json::json!(names), step["names"], "{name}");
        assert_eq!(serde_json::json!(selected), step["selected"], "{name}");
    }
    hydrus_gui::clear_clipboard_reader();
}

/// Replay a recorded run of a comparator list: each step's selection and
/// action, then the messages and the list as the reference had them.
fn replay_steps(
    list: &ComparatorList<'_>,
    copied: &Rc<RefCell<Vec<String>>>,
    run: &Value,
    what: &str,
) {
    // (the reference's list as it opens: nothing selected)
    list.select(&[]);
    assert_eq!(list.state(), run["start"], "{what}: the list as it starts");
    for step in run["steps"].as_array().unwrap() {
        let name = format!("{what}: {}", step["name"]);
        let rows: Vec<usize> = serde_json::from_value(step["select"].clone()).unwrap();
        list.select(&rows);
        match step["action"].as_str().unwrap() {
            "export" => {
                let before = copied.borrow().len();
                (list.exchange)(0);
                let text = copied.borrow().get(before).cloned().expect("copied");
                assert_eq!(
                    normal(&text),
                    normal(step["text"].as_str().unwrap()),
                    "{name}"
                );
            }
            "duplicate" => (list.exchange)(4),
            _ => {
                paste(step["text"].as_str());
                (list.exchange)(2);
            }
        }
        assert_eq!(serde_json::json!(said()), step["messages"], "{name}");
        let after = list.state();
        assert_eq!(after["rows"], step["rows"], "{name}");
        assert_eq!(after["selected"], step["selected"], "{name}");
    }
}

/// Export to a png through the export panel open now: what it starts with
/// (its title and payload description), and the text the png carries.
fn png_exported(path: &std::path::Path) -> (String, String, String) {
    let window = hydrus_gui::png_export_window::last().expect("the png export opens");
    let started = (
        window.get_png_title().to_string(),
        window.get_payload_description().to_string(),
    );
    window.set_path(path.to_string_lossy().as_ref().into());
    window.invoke_action("update".into());
    window.invoke_action("export".into());
    assert!(window.get_done(), "{}", window.get_error());
    window.invoke_action("close".into());
    let text = hydrus_downloader_exchange::text_png::decode(&std::fs::read(path).unwrap()).unwrap();
    (started.0, started.1, text)
}

// leaf: audit-media-rules-exchange
#[test]
fn rules_and_comparators_go_to_and_come_from_the_reference_s_pngs_and_json_files() {
    let rec = recorded();
    let pngs = rec["pngs"].as_array().unwrap();
    let (opened, list) = opened();
    let temp = tempfile::tempdir().unwrap();
    let files: Rc<RefCell<Vec<std::path::PathBuf>>> = Rc::default();
    hydrus_gui::set_picker({
        let files = files.clone();
        move |_, _| files.borrow().clone()
    });

    // the reference's png of the rule "wide" comes in
    *files.borrow_mut() = vec![hydrus_testkit::fixture_path(
        pngs[0]["file"].as_str().unwrap(),
    )];
    list.invoke_exchange(5);
    assert_eq!(
        serde_json::json!(said()),
        serde_json::json!([{"kind": "information", "text": "1 objects added!"}])
    );
    assert_eq!(rules(&list).0, ["wide"]);

    // and goes out as a png carrying the reference's text, the panel titled
    // and described as the reference's
    select_rules(&list, &[Value::from("wide")]);
    list.invoke_exchange(2);
    let (title, description, text) = png_exported(&temp.path().join("rule.png"));
    assert_eq!(normal(&text), normal(pngs[0]["text"].as_str().unwrap()));
    assert_eq!(title, pngs[0]["title"].as_str().unwrap());
    // (the text is a byte longer than the reference's, ours a new rule's id
    // "-1" where its was a database id "9", which "5.78 KB" doesn't show)
    assert_eq!(text.len(), pngs[0]["text"].as_str().unwrap().len() + 1);
    assert_eq!(
        description,
        pngs[0]["payload_description"].as_str().unwrap()
    );

    // a json file carries the same text (`_ExportToJSON` writes
    // `DumpToString`), and comes in again
    let json = temp.path().join("export.json");
    *files.borrow_mut() = vec![json.clone()];
    list.invoke_exchange(1);
    assert_eq!(
        normal(&std::fs::read_to_string(&json).unwrap()),
        normal(pngs[0]["text"].as_str().unwrap())
    );
    list.invoke_exchange(4);
    assert_eq!(
        serde_json::json!(said()),
        serde_json::json!([{"kind": "information", "text": "1 objects added!"}])
    );
    assert_eq!(rules(&list).0, ["wide", "wide (1)"]);
    select_rules(&list, &[Value::from("wide")]);

    // the reference's png of a comparator comes into the rule's comparators
    list.invoke_edit();
    let rule = opened
        .bound
        .auto_resolution
        .rule
        .borrow()
        .as_ref()
        .expect("the rule editor opens")
        .clone_strong();
    let before = rule.get_comparators().row_count();
    *files.borrow_mut() = vec![hydrus_testkit::fixture_path(
        pngs[1]["file"].as_str().unwrap(),
    )];
    rule.invoke_comparator_exchange(3);
    assert_eq!(
        serde_json::json!(said()),
        serde_json::json!([{"kind": "information", "text": "1 objects added!"}])
    );
    assert_eq!(rule.get_comparators().row_count(), before + 1);
    assert_eq!(picked(&rule.get_comparator_picked()), [before]);
    // and, selected as it came, goes out again as the reference's png of it
    rule.invoke_comparator_exchange(1);
    let (title, description, text) = png_exported(&temp.path().join("comparator.png"));
    assert_eq!(text, pngs[1]["text"].as_str().unwrap());
    assert_eq!(title, pngs[1]["title"].as_str().unwrap());
    assert_eq!(description, pngs[1]["title"].as_str().unwrap());

    // a file that is no png: the reference's wording
    let bad = temp.path().join("bad.png");
    std::fs::write(&bad, b"not a png").unwrap();
    *files.borrow_mut() = vec![bad];
    rule.invoke_comparator_exchange(3);
    let said = said();
    assert_eq!(said.len(), 1);
    assert_eq!(said[0]["title"], "Problem importing!");
    assert_eq!(rule.get_comparators().row_count(), before + 1);
}

// A rule written in the editor: its search's lines and its metadata
// comparator's search entries typed as the recording's lines (the reference
// parsed the same lines), then OK, export as the reference's. The lines name rating and file
// services and leave view canvases to the options, as typed predicates do.
// leaf: audit-media-rules-exchange
#[test]
fn a_rule_typed_in_the_editor_exports_as_the_reference_s() {
    let rec = recorded();
    let (opened, list) = opened();
    let copied = opened.copied.clone();
    let recorded_text = rec["exports"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["names"] == serde_json::json!(["typed"]))
        .unwrap()["text"]
        .as_str()
        .unwrap()
        .to_owned();
    let lines = |which: &str| -> String {
        rec["typed_lines"][which]
            .as_array()
            .unwrap()
            .iter()
            .map(|l| l.as_str().unwrap())
            .collect::<Vec<_>>()
            .join("\n")
    };
    // the rule's other settings, as the reference's (pasted), then its
    // predicates typed over in the editor
    paste(Some(&recorded_text));
    list.invoke_exchange(3);
    said();
    select_rules(&list, &[Value::from("typed")]);
    list.invoke_edit();
    let rule = opened
        .bound
        .auto_resolution
        .rule
        .borrow()
        .as_ref()
        .expect("the rule editor opens")
        .clone_strong();
    rule.set_search_1(lines("search").into());
    rule.invoke_changed();
    rule.invoke_comparator_clicked(0, false, false);
    rule.invoke_comparator_edit();
    let editor = opened
        .bound
        .auto_resolution
        .comparators
        .borrow()
        .last()
        .expect("the comparator editor opens")
        .1
        .clone_strong();
    assert_eq!(editor.get_kind(), "search");
    // the comparator's search (the reference's metadata conditional search):
    // what the paste brought taken out, then each line typed and entered
    while editor.get_search_predicates().row_count() > 0 {
        editor.invoke_search_removed(0);
    }
    let mut not_entered = Vec::new();
    for line in rec["typed_lines"]["comparator"].as_array().unwrap() {
        let line = line.as_str().unwrap();
        let before = editor.get_search_predicates().row_count();
        editor.set_search_typed(line.into());
        editor.invoke_search_edited();
        editor.invoke_search_chosen(0, false);
        if editor.get_search_predicates().row_count() != before + 1 {
            not_entered.push(line);
        }
    }
    assert!(not_entered.is_empty(), "not entered: {not_entered:?}");
    editor.invoke_apply();
    assert!(
        opened.bound.auto_resolution.comparators.borrow().is_empty(),
        "{}",
        editor.get_errors()
    );
    rule.invoke_apply();
    assert!(
        opened.bound.auto_resolution.rule.borrow().is_none(),
        "{}",
        rule.get_errors()
    );
    select_rules(&list, &[Value::from("typed")]);
    let before = copied.borrow().len();
    list.invoke_exchange(0);
    assert_eq!(said(), Vec::<Value>::new(), "exported without a problem");
    let text = copied.borrow().get(before).cloned().expect("copied");
    let (mut ours, mut theirs) = (normal(&text), normal(&recorded_text));
    // (the comparator's predicates in the order its search keeps them, as
    // the reference's editor's list does; the recording set them directly)
    for value in [&mut ours, &mut theirs] {
        if let Some(predicates) = value[3][5][2][2][0][1][2][1][2][2][3].as_array_mut() {
            predicates.sort_by_key(ToString::to_string);
        }
    }
    assert!(
        ours == theirs,
        "{}",
        hydrus_testkit::serialised::first_difference(&ours, &theirs).unwrap_or_default()
    );
    hydrus_gui::clear_clipboard_reader();
}
