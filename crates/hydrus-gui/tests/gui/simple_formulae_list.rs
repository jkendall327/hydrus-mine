//! The saved simple downloader formula list (`_EditFormulae`): defaults join
//! the draft with unique names, named simple formulae export and import as
//! text and PNG (anything else is refused), and the draft reaches the saved
//! setting only on Apply.
use crate::subscriptions::store;
use hydrus_gui::{headless, simple_formulae_window};
use hydrus_gui_model::downloader_interchange::{self as exchange, Definition, Native};
use hydrus_gui_model::formula_editors::new_formula;
use hydrus_parse::simple::SimpleFormula;
use hydrus_store::settings::{self, SimpleDownloaderFormulae};
use serde_json::Value;
use slint::{ComponentHandle as _, Model as _};
use std::rc::Rc;

fn names(w: &hydrus_gui::SimpleFormulaeWindow) -> Vec<String> {
    w.get_rows()
        .iter()
        .map(|r| r.cells.row_data(0).unwrap().to_string())
        .collect()
}

// leaf: audit-network-simple-formula-actions
// leaf: audit-network-simple-formula-commit
// leaf: audit-network-simple-formula-exchange
#[test]
fn simple_formula_list_defaults_exchange_and_apply() {
    let (_dirs, store) = store();
    let _windows = headless::init();
    let slots = simple_formulae_window::Slots::default();
    let applied = Rc::new(std::cell::Cell::new(0));
    let open = || {
        let w = simple_formulae_window::open(
            &store,
            &slots,
            Rc::new({
                let applied = applied.clone();
                move || applied.set(applied.get() + 1)
            }),
        )
        .unwrap();
        *slots.list.borrow_mut() = Some(w.clone_strong());
        w
    };
    let saved = || {
        store
            .read(settings::get::<SimpleDownloaderFormulae>)
            .unwrap()
    };
    let before = saved();
    let list = open();
    let start = names(&list);
    assert_eq!(start.len(), before.formulae.len());

    // Defaults join the draft; their names are made unique, as the reference's
    // unique-named list does.
    list.invoke_defaults();
    list.invoke_defaults_chosen(0);
    let with_defaults = names(&list);
    assert_eq!(with_defaults.len(), start.len() * 2);
    let mut unique = with_defaults.clone();
    unique.sort();
    unique.dedup();
    assert_eq!(unique.len(), with_defaults.len());
    assert_eq!(saved(), before, "the draft is not saved before Apply");

    // Export the selected row as text and as a PNG.
    let picked = with_defaults
        .iter()
        .position(|n| n.ends_with("(1)"))
        .unwrap();
    list.invoke_row_clicked(i32::try_from(picked).unwrap(), false, false);
    list.invoke_exchange(false);
    let export = slots.exchange.0.borrow().as_ref().unwrap().clone_strong();
    let exported = exchange::decode_text(export.get_text().as_str()).unwrap();
    assert_eq!(exported.len(), 1);
    assert!(matches!(&exported[0].native, Native::Simple(f) if f.name == with_defaults[picked]));
    export.invoke_action("cancel".into());

    // Import: a non-simple definition is refused, a named simple formula joins
    // the draft (renamed if the name is taken).
    list.invoke_exchange(true);
    let import = slots.exchange.0.borrow().as_ref().unwrap().clone_strong();
    import.set_text(
        exchange::encode_text(&[Definition::new(Native::Page(
            hydrus_gui_model::parser_editors::new_page(),
        ))])
        .unwrap()
        .into(),
    );
    import.invoke_action("review".into());
    assert!(!import.get_ready());
    assert!(!import.get_error().is_empty());
    import.invoke_action("accept".into());
    assert_eq!(names(&list), with_defaults);
    let mut f = new_formula(false);
    f.name = "imported".into();
    let simple = SimpleFormula {
        name: "imported".into(),
        formula: f,
    };
    import.set_text(
        exchange::encode_text(&[Definition::new(Native::Simple(simple.clone()))])
            .unwrap()
            .into(),
    );
    import.invoke_action("review".into());
    assert!(import.get_ready(), "{}", import.get_error());
    import.invoke_action("accept".into());
    assert!(names(&list).contains(&"imported".to_string()));
    assert_eq!(saved(), before);

    // Cancel discards the whole draft.
    list.invoke_cancel();
    assert!(slots.list.borrow().is_none());
    assert_eq!(saved(), before);
    assert_eq!(applied.get(), 0);

    // Apply saves the draft and tells the caller to refresh its chooser.
    let list = open();
    list.invoke_defaults();
    list.invoke_defaults_chosen(0);
    list.invoke_exchange(true);
    let import = slots.exchange.0.borrow().as_ref().unwrap().clone_strong();
    import.set_text(
        exchange::encode_text(&[Definition::new(Native::Simple(simple))])
            .unwrap()
            .into(),
    );
    import.invoke_action("review".into());
    import.invoke_action("accept".into());
    let draft = names(&list);
    list.invoke_apply();
    assert_eq!(applied.get(), 1);
    let mut got: Vec<_> = saved().formulae.into_iter().map(|f| f.name).collect();
    let mut want = draft;
    got.sort();
    want.sort();
    assert_eq!(got, want);
    assert_eq!(saved().favourite, before.favourite);
}

/// The list's buttons and dialogs, replayed from the reference's real
/// `AddEditDeleteListBoxUniqueNamedObjects` inside `_EditFormulae`
/// (`oracle/record_simple_formula_list.py`): add and edit ask for the name ("Edit name.",
/// with the formula's current name), then open the formula editor; a name clash gets
/// "(1)"; deleting asks "Remove N selected?"; "add defaults" appends the reference's
/// defaults, renamed to stay unique. Rows keep the order they were added in.
// leaf: audit-network-simple-formula-actions
#[test]
fn the_list_buttons_replay_the_reference_s_add_edit_delete_and_defaults() {
    let recorded = hydrus_testkit::fixture_json("simple_formula_list.json");
    let (_dirs, store) = store();
    let _windows = headless::init();
    let slots = simple_formulae_window::Slots::default();
    let list = simple_formulae_window::open(&store, &slots, Rc::new(|| {})).unwrap();
    *slots.list.borrow_mut() = Some(list.clone_strong());
    let enabled = |w: &hydrus_gui::SimpleFormulaeWindow| -> Vec<(&'static str, bool)> {
        vec![
            ("add", true),
            ("edit", w.get_selected()),
            ("delete", w.get_selected()),
            ("add defaults", true),
        ]
    };
    let asked_name = |w: &hydrus_gui::SimpleFormulaeWindow, wanted: &Value| {
        assert!(w.get_naming(), "the name is asked for");
        assert_eq!(w.get_name(), wanted[2].as_str().unwrap(), "default name");
        if let Some(name) = wanted[3].as_str() {
            w.set_name(name.into());
            w.invoke_named();
            true
        } else {
            w.invoke_name_cancelled();
            false
        }
    };
    let mut defaulted = false;
    for step in recorded["actions"].as_array().unwrap() {
        let what = step["step"].as_str().unwrap();
        defaulted |= what.starts_with("add them all");
        let dialogs = step["dialogs"].as_array().unwrap();
        match what {
            "opened" | "select a row" | "select two rows" => {
                if what == "select a row" {
                    let at = names(&list).iter().position(|n| n == "my formula").unwrap();
                    list.invoke_row_clicked(i32::try_from(at).unwrap(), false, false);
                } else if what == "select two rows" {
                    list.invoke_row_clicked(0, false, false);
                    list.invoke_row_clicked(1, true, false);
                }
            }
            "add them all" | "add them all again" => {
                list.invoke_defaults();
                list.invoke_defaults_chosen(0);
            }
            "delete with nothing selected" => list.invoke_delete(),
            w if w.starts_with("select from a list") => {
                list.invoke_defaults();
                list.invoke_defaults_chosen(1);
                let chooser = &dialogs[0];
                assert_eq!(list.get_defaults_step(), 2, "{what}: the chooser");
                let mut labels: Vec<String> = list
                    .get_default_ticks()
                    .iter()
                    .map(|t| t.label.to_string())
                    .collect();
                let mut wanted_labels = strings(&chooser[2]);
                labels.sort();
                wanted_labels.sort();
                assert_eq!(labels, wanted_labels, "{what}: the defaults offered");
                assert!(
                    list.get_default_ticks().iter().all(|t| !t.on),
                    "{what}: none ticked"
                );
                match chooser[4].as_array() {
                    None => list.invoke_defaults_cancelled(),
                    Some(ticked) => {
                        for (i, tick) in list.get_default_ticks().iter().enumerate() {
                            if ticked.iter().any(|t| *t == tick.label.as_str()) {
                                list.invoke_default_ticked(i32::try_from(i).unwrap(), true);
                            }
                        }
                        list.invoke_defaults_picked(0);
                    }
                }
                assert_eq!(list.get_defaults_step(), 0, "{what}: closed");
            }
            w if w.starts_with("delete") => {
                list.invoke_delete();
                if let Some(q) = dialogs.first() {
                    assert_eq!(list.get_question(), q[1].as_str().unwrap(), "{what}");
                    list.invoke_chosen(i32::from(!q[2].as_bool().unwrap()));
                }
            }
            w if w.starts_with("add") || w.starts_with("edit") || w.starts_with("cancel") => {
                if w.starts_with("edit") {
                    list.invoke_edit();
                } else {
                    list.invoke_add();
                }
                if asked_name(&list, &dialogs[0]) {
                    let editor = dialogs[1][1].as_bool().unwrap();
                    let formula = slots
                        .formula
                        .formula
                        .borrow()
                        .as_ref()
                        .expect("the formula editor opens")
                        .clone_strong();
                    if editor {
                        formula.invoke_apply();
                    } else {
                        formula.invoke_cancel();
                    }
                }
            }
            other => panic!("{other}"),
        }
        // (the reference lists its default formulae in the order the filesystem gives its
        // static folder, so once defaults are in, the rows are compared as a set)
        let (mut ours, mut theirs) = (names(&list), strings(&step["rows"]));
        if defaulted {
            ours.sort();
            theirs.sort();
        }
        assert_eq!(ours, theirs, "{what}: rows");
        let selected: Vec<usize> = list
            .get_rows()
            .iter()
            .enumerate()
            .filter(|(_, r)| r.selected)
            .map(|(i, _)| i)
            .collect();
        let wanted: Vec<usize> = step["selected"]
            .as_array()
            .unwrap()
            .iter()
            .map(|i| usize::try_from(i.as_u64().unwrap()).unwrap())
            .collect();
        assert_eq!(selected, wanted, "{what}: selection");
        for (button, state) in enabled(&list) {
            assert_eq!(
                state,
                step["buttons"][button].as_bool().unwrap(),
                "{what}: {button}"
            );
        }
    }
}

fn strings(value: &Value) -> Vec<String> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_owned())
        .collect()
}

// (a tag rule with no attributes is written `null` by the reference and `{}` here)
fn plain(value: &Value) -> Value {
    match value {
        Value::Object(map) if map.is_empty() => Value::Null,
        Value::Array(items) => Value::Array(items.iter().map(plain).collect()),
        other => other.clone(),
    }
}

/// What "export" writes for one selected formula is the reference's own serialisation of it:
/// the clipboard text `_ExportToClipboard` wrote for the first default (the recording), compared
/// as JSON.
// (not tagged: the reference's import and export menus, clipboard messages ("1 objects added!",
// the types it refuses) and PNG dialogs differ from this window's shared import window, see
// DIFFERENCES.md)
#[test]
fn exporting_a_formula_writes_the_reference_s_serialisation() {
    let recorded = hydrus_testkit::fixture_json("simple_formula_list.json");
    let step = recorded["exchange"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["step"] == "export to clipboard")
        .unwrap();
    let wanted: Value = serde_json::from_str(step["clipboard"][0].as_str().unwrap()).unwrap();
    let name = wanted[1].as_str().unwrap().to_owned();
    let (_dirs, store) = store();
    let _windows = headless::init();
    let slots = simple_formulae_window::Slots::default();
    let list = simple_formulae_window::open(&store, &slots, Rc::new(|| {})).unwrap();
    *slots.list.borrow_mut() = Some(list.clone_strong());
    let at = names(&list).iter().position(|n| *n == name).unwrap();
    list.invoke_row_clicked(i32::try_from(at).unwrap(), false, false);
    list.invoke_exchange(false);
    let export = slots.exchange.0.borrow().as_ref().unwrap().clone_strong();
    let ours: Value = serde_json::from_str(export.get_text().as_str()).unwrap();
    assert_eq!(plain(&ours), plain(&wanted));
}
