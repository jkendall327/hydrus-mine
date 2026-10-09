//! The tag filter editor's window, step by step as the reference's
//! (`oracle/fixtures/tag_filter_editor.json`, from
//! `oracle/record_tag_filter_editor.py`): opened on each recorded filter,
//! with the recorded namespaces offered (a parser's), its tabs chosen and
//! its global and namespace boxes clicked, slices typed into its lists,
//! selected and removed (the question answered with its button), "block
//! everything" pressed and the test box typed in; after each step its tabs,
//! both simple lists (ticks, entries, enabled, errors), the advanced lists,
//! whether "except for these" takes input, what it said of entries already
//! covered, the current filter, the test's result, and the filter it gives.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use serde_json::{Value, json};
use slint::{ComponentHandle as _, Model as _};

use hydrus_core::tag_filter::{FilterRule, TagFilter};
use hydrus_core::url::strings::{StringConverter, StringProcessor};
use hydrus_gui::{TagFilterWindow, headless, tag_filter_window};
use hydrus_gui_model::tag_filter_editor::pretty_slice;
use hydrus_parse::content::{ContentKind, ContentParser, PageParser};
use hydrus_parse::formula::{Formula, FormulaKind};
use hydrus_store::Store;
use hydrus_store::import::import_legacy;

use crate::common::widgets;

const WHITELIST: i32 = 0;
const BLACKLIST: i32 = 1;
const EXCLUDE: i32 = 2;
const EXCEPT: i32 = 3;

fn strings(value: &Value) -> Vec<String> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_owned())
        .collect()
}

fn rules(value: &Value) -> TagFilter {
    let mut filter = TagFilter::new();
    for rule in value.as_array().unwrap() {
        let rule = strings(rule);
        let kind = if rule[1] == "black" {
            FilterRule::Blacklist
        } else {
            FilterRule::Whitelist
        };
        filter.set_rule(rule[0].clone(), kind);
    }
    filter
}

/// A store whose downloaders' parsers parse tags of the recorded
/// namespaces (the namespaces the editor offers are its parsers', as the
/// reference's `GetParserNamespaces`).
fn store(namespaces: &[String]) -> ([tempfile::TempDir; 2], Arc<Store>) {
    let legacy = hydrus_testkit::legacy_fixture("basic");
    let native = tempfile::tempdir().unwrap();
    import_legacy(
        legacy.path(),
        &native.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store = Store::open(native.path()).unwrap();
    let mut downloaders: hydrus_parse::Downloaders =
        store.read(hydrus_store::settings::get).unwrap();
    let tags = |namespace: &String| ContentParser {
        name: format!("{namespace} tags"),
        kind: ContentKind::Tag {
            namespace: (!namespace.is_empty()).then(|| namespace.clone()),
        },
        formula: Formula {
            reference_auxiliary: None,
            name: String::new(),
            kind: FormulaKind::Static {
                text: "x".into(),
                count: 1,
            },
            processor: StringProcessor::default(),
        },
    };
    let parser = PageParser {
        reference_auxiliary: None,
        name: "tags".into(),
        key: "00".repeat(32),
        converter: StringConverter::default(),
        subsidiary: Vec::new(),
        content_parsers: namespaces.iter().map(tags).collect(),
        example_urls: Vec::new(),
    };
    downloaders.parsers = vec![parser];
    assert_eq!(downloaders.parser_namespaces(), namespaces);
    store
        .write(move |ctx| hydrus_store::settings::set(ctx.conn(), &downloaders))
        .unwrap();
    ([legacy, native], store)
}

fn rows(model: &slint::ModelRc<hydrus_gui::TableRow>) -> Vec<String> {
    model
        .iter()
        .map(|row| row.cells.row_data(0).unwrap().to_string())
        .collect()
}

fn ticks(model: &slint::ModelRc<hydrus_gui::Tick>) -> Vec<bool> {
    model.iter().map(|t| t.on).collect()
}

/// What the window shows, as the recording has it (its lists' entries are
/// the reference's slices, shown as the reference shows them).
fn state(w: &TagFilterWindow) -> Value {
    json!({
        "tabs": w.get_tabs().iter().map(|t| t.to_string()).collect::<Vec<_>>(),
        "whitelist": {
            "enabled": w.get_white_enabled(),
            "error": w.get_white_error().as_str(),
            "list": rows(&w.get_white_rows()),
            "global": ticks(&w.get_white_global()),
            "namespaces": ticks(&w.get_white_namespaces()),
            "namespaces_enabled": w.get_white_namespaces_enabled(),
        },
        "blacklist": {
            "enabled": w.get_black_enabled(),
            "error": w.get_black_error().as_str(),
            "list": rows(&w.get_black_rows()),
            "global": ticks(&w.get_black_global()),
            "namespaces": ticks(&w.get_black_namespaces()),
            "namespaces_enabled": w.get_black_namespaces_enabled(),
        },
        "advanced_blacklist": rows(&w.get_exclude_rows()),
        "advanced_whitelist": rows(&w.get_except_rows()),
        "except_input_enabled": w.get_except_input_enabled(),
        "redundant": w.get_redundant().as_str(),
        "current": w.get_current().as_str(),
        "test": w.get_test_result().as_str(),
        "test_colour": match w.get_test_good() {
            0 => "",
            1 => "HydrusValid",
            _ => "HydrusInvalid",
        },
    })
}

/// The recorded state, its slices shown as the window shows them.
fn theirs(state: &Value) -> Value {
    let pretty =
        |v: &Value| -> Vec<String> { strings(v).iter().map(|s| pretty_slice(s)).collect() };
    let simple = |s: &Value| {
        json!({
            "enabled": s["enabled"],
            "error": s["error"],
            "list": pretty(&s["list"]),
            "global": s["global"],
            "namespaces": s["namespaces"],
            "namespaces_enabled": s["namespaces_enabled"],
        })
    };
    json!({
        "tabs": state["tabs"],
        "whitelist": simple(&state["whitelist"]),
        "blacklist": simple(&state["blacklist"]),
        "advanced_blacklist": pretty(&state["advanced_blacklist"]),
        "advanced_whitelist": pretty(&state["advanced_whitelist"]),
        "except_input_enabled": state["except_input_enabled"],
        "redundant": state["redundant"],
        "current": state["current"],
        "test": state["test"],
        "test_colour": state["test_colour"],
    })
}

/// Show a tab, clicking it.
fn show_tab(w: &TagFilterWindow, name: &str) {
    let tabs: Vec<String> = w.get_tabs().iter().map(|t| t.to_string()).collect();
    let i = tabs.iter().position(|t| t == name).unwrap();
    if w.get_tab() != i32::try_from(i).unwrap() {
        widgets::click(w.window(), name);
    }
    assert_eq!(w.get_tab(), i32::try_from(i).unwrap(), "{name} shown");
}

/// Click a box of a simple list (its label), on that list's tab; whether
/// it could be clicked (it was enabled).
fn click_tick(w: &TagFilterWindow, list: i32, global: bool, index: usize) -> bool {
    show_tab(
        w,
        if list == WHITELIST {
            "whitelist"
        } else {
            "blacklist"
        },
    );
    let model = match (list, global) {
        (WHITELIST, true) => w.get_white_global(),
        (WHITELIST, false) => w.get_white_namespaces(),
        (_, true) => w.get_black_global(),
        (_, false) => w.get_black_namespaces(),
    };
    let label = model.row_data(index).unwrap().label.to_string();
    let enabled = widgets::enabled(w.window(), &label);
    widgets::click(w.window(), &label);
    enabled
}

/// Select these slices of a list (a click, then ctrl-clicks) and remove
/// them, answering the question "yes" with its button.
fn remove(w: &TagFilterWindow, list: i32, slices: &[String]) {
    show_tab(
        w,
        if list == WHITELIST {
            "whitelist"
        } else if list == BLACKLIST {
            "blacklist"
        } else {
            "advanced"
        },
    );
    let shown = rows(&match list {
        WHITELIST => w.get_white_rows(),
        BLACKLIST => w.get_black_rows(),
        EXCLUDE => w.get_exclude_rows(),
        _ => w.get_except_rows(),
    });
    for (n, slice) in slices.iter().enumerate() {
        let row = shown
            .iter()
            .position(|r| *r == pretty_slice(slice))
            .unwrap_or_else(|| panic!("{slice} in {shown:?}"));
        w.invoke_row_clicked(list, i32::try_from(row).unwrap(), n > 0, false);
    }
    w.invoke_remove(list);
    assert_eq!(w.get_asking_message(), "Remove all selected?");
    widgets::click(w.window(), "yes");
    assert!(!w.get_asking());
}

// leaf: audit-shared-tag-blacklist
// leaf: audit-shared-tag-advanced
#[test]
fn the_tag_filter_window_shows_and_edits_a_filter_as_the_reference_does() {
    let recorded = hydrus_testkit::fixture_json("tag_filter_editor.json");
    let namespaces = strings(&recorded["namespaces"]);
    let (_dirs, store) = store(&namespaces);
    let _windows = headless::init();
    let mut skipped = Vec::new();
    for (n, case) in recorded["cases"].as_array().unwrap().iter().enumerate() {
        let slot = Rc::new(RefCell::new(None));
        let given: Rc<RefCell<Option<TagFilter>>> = Rc::default();
        let w = tag_filter_window::open(
            &store,
            &rules(&case["rules"]),
            case["blacklist_only"].as_bool().unwrap(),
            "filter",
            "",
            &slot,
            Rc::new({
                let given = given.clone();
                move |f| *given.borrow_mut() = Some(f)
            }),
        )
        .unwrap();
        widgets::lay_out(w.window(), 1000.0, 900.0);
        let mut diverged = false;
        let mut reachable: Option<Value> = None;
        let mut prior = Value::Null;
        for recorded_state in case["states"].as_array().unwrap() {
            let step = &recorded_state["step"];
            let at = format!("case {n}, {step}");
            let mut clickable = true;
            if let Some(step) = step.as_array() {
                let index = || usize::try_from(step[1].as_u64().unwrap()).unwrap();
                let slices = || strings(&step[1]);
                match step[0].as_str().unwrap() {
                    "white_global" => clickable = click_tick(&w, WHITELIST, true, index()),
                    "white_ns" => clickable = click_tick(&w, WHITELIST, false, index()),
                    "black_global" => clickable = click_tick(&w, BLACKLIST, true, index()),
                    "black_ns" => clickable = click_tick(&w, BLACKLIST, false, index()),
                    "white_add" => w.invoke_typed(WHITELIST, slices().join("\n").into()),
                    "black_add" => w.invoke_typed(BLACKLIST, slices().join("\n").into()),
                    "adv_black_add" => w.invoke_typed(EXCLUDE, slices().join("\n").into()),
                    "adv_white_add" => w.invoke_typed(EXCEPT, slices().join("\n").into()),
                    "white_remove" => remove(&w, WHITELIST, &slices()),
                    "black_remove" => remove(&w, BLACKLIST, &slices()),
                    "adv_black_delete" => remove(&w, EXCLUDE, &slices()),
                    "adv_white_delete" => remove(&w, EXCEPT, &slices()),
                    "block_everything" => {
                        show_tab(&w, "advanced");
                        widgets::click(w.window(), "block everything");
                    }
                    "test" => {
                        w.set_test_input(step[1].as_str().unwrap().into());
                        w.invoke_test_edited();
                    }
                    other => panic!("{other}"),
                }
            } else {
                // the tab chosen as it opens
                let tabs: Vec<String> = w.get_tabs().iter().map(|t| t.to_string()).collect();
                let shown = usize::try_from(w.get_tab()).unwrap();
                assert_eq!(tabs[shown], recorded_state["state"]["tab"], "{at}");
            }
            if !clickable {
                reachable = Some(prior.clone());
                // (the recorder set a disabled box in the panel behind it,
                // which no user can: the window ignores the click, and the
                // case goes no further)
                diverged = true;
                break;
            }
            // (what it says of entries already covered stays up for a moment,
            // as the reference's message does: it is compared when the
            // recording has one, not when the recording has cleared it)
            let mut ours = state(&w);
            if recorded_state["state"]["redundant"] == "" {
                ours["redundant"] = json!("");
            }
            assert_eq!(ours, theirs(&recorded_state["state"]), "{at}");
            prior = recorded_state["state"]["rules"].clone();
        }
        // the filter it gives (at the last state the window reached)
        w.invoke_apply();
        let given = given.borrow().clone().expect("applied");
        let mut ours: Vec<Vec<String>> = given
            .rules()
            .map(|(s, r)| {
                vec![
                    s.to_owned(),
                    if r == FilterRule::Blacklist {
                        "black".to_owned()
                    } else {
                        "white".to_owned()
                    },
                ]
            })
            .collect();
        ours.sort();
        if diverged {
            skipped.push(n);
        }
        let last = &case["states"].as_array().unwrap().last().unwrap()["state"];
        let rules_json = reachable.unwrap_or_else(|| last["rules"].clone());
        let mut expected: Vec<Vec<String>> =
            rules_json.as_array().unwrap().iter().map(strings).collect();
        expected.sort();
        assert_eq!(ours, expected, "case {n}: the filter given");
    }
    // (only case 1 ends on a box its window had disabled)
    assert_eq!(skipped, [1]);
}
