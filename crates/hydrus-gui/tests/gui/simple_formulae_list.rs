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

    // Export the selected row; import refuses what is not a simple formula
    // and adds a named one to the draft (renamed if the name is taken).
    let picked = with_defaults
        .iter()
        .position(|n| n.ends_with("(1)"))
        .unwrap();
    list.invoke_row_clicked(i32::try_from(picked).unwrap(), false, false);
    let copied: std::rc::Rc<std::cell::RefCell<Vec<String>>> = std::rc::Rc::default();
    hydrus_gui::set_clipper({
        let copied = copied.clone();
        move |clip| {
            if let hydrus_gui::Clip::Text(text) = clip {
                copied.borrow_mut().push(text.clone());
            }
        }
    });
    list.invoke_exchange(0);
    let exported = exchange::decode_text(copied.borrow().last().unwrap()).unwrap();
    assert_eq!(exported.len(), 1);
    assert!(matches!(&exported[0].native, Native::Simple(f) if f.name == with_defaults[picked]));
    let page = exchange::encode_text(&[Definition::new(Native::Page(
        hydrus_gui_model::parser_editors::new_page(),
    ))])
    .unwrap();
    crate::rules_exchange::paste(Some(&page));
    list.invoke_exchange(3);
    assert_eq!(crate::rules_exchange::said().len(), 1);
    assert_eq!(names(&list), with_defaults);
    let mut f = new_formula(false);
    f.name = "imported".into();
    let simple = SimpleFormula {
        name: "imported".into(),
        formula: f,
    };
    let simple_text =
        exchange::encode_text(&[Definition::new(Native::Simple(simple.clone()))]).unwrap();
    crate::rules_exchange::paste(Some(&simple_text));
    list.invoke_exchange(3);
    crate::rules_exchange::said();
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
    crate::rules_exchange::paste(Some(&simple_text));
    list.invoke_exchange(3);
    crate::rules_exchange::said();
    hydrus_gui::clear_clipboard_reader();
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

/// The list's "export", "import" and "duplicate", replayed against the
/// reference's `AddImportExportButtons` on it (`oracle/record_simple_formula_list.py`):
/// exports are the reference's text, pastes and duplicates append under unique names
/// and are selected with the rows already selected, with the reference's messages;
/// "to png" and "to pngs" write the reference's pngs' texts, titled by name, and
/// "from pngs" brings them back.
// leaf: audit-network-simple-formula-exchange
#[test]
fn the_list_exchange_replays_the_reference_s_clipboard_and_png_menus() {
    let recorded = hydrus_testkit::fixture_json("simple_formula_list.json");
    let (_dirs, store) = store();
    let _windows = headless::init();
    let copied: std::rc::Rc<std::cell::RefCell<Vec<String>>> = std::rc::Rc::default();
    hydrus_gui::set_clipper({
        let copied = copied.clone();
        move |clip| {
            if let hydrus_gui::Clip::Text(text) = clip {
                copied.borrow_mut().push(text.clone());
            }
        }
    });
    let temp = tempfile::tempdir().unwrap();
    let files: std::rc::Rc<std::cell::RefCell<Vec<std::path::PathBuf>>> = std::rc::Rc::default();
    let picked_titles: std::rc::Rc<std::cell::RefCell<Vec<String>>> = std::rc::Rc::default();
    hydrus_gui::set_picker({
        let files = files.clone();
        let titles = picked_titles.clone();
        move |_, title| {
            titles.borrow_mut().push(title.to_owned());
            files.borrow().clone()
        }
    });
    let selected = |w: &hydrus_gui::SimpleFormulaeWindow| -> Vec<usize> {
        w.get_rows()
            .iter()
            .enumerate()
            .filter(|(_, r)| r.selected)
            .map(|(i, _)| i)
            .collect()
    };
    let select = |w: &hydrus_gui::SimpleFormulaeWindow, rows: &[i32]| {
        for (n, row) in rows.iter().enumerate() {
            w.invoke_row_clicked(*row, n > 0, false);
        }
    };
    let said = || -> Vec<Value> {
        crate::rules_exchange::said()
            .into_iter()
            .map(|m| match m["kind"].as_str().unwrap() {
                "critical" => serde_json::json!(["critical", m["title"], m["text"]]),
                kind => serde_json::json!([kind, m["text"]]),
            })
            .collect()
    };
    for run in ["exchange", "pngs"] {
        let slots = simple_formulae_window::Slots::default();
        let list = simple_formulae_window::open(&store, &slots, Rc::new(|| {})).unwrap();
        *slots.list.borrow_mut() = Some(list.clone_strong());
        for step in recorded[run].as_array().unwrap() {
            let what = step["step"].as_str().unwrap();
            let mut dialogs: Vec<Value> = step["dialogs"].as_array().unwrap().clone();
            let before = copied.borrow().len();
            match what {
                "opened" => {}
                "select the first row" => select(&list, &[0]),
                "select two rows" => select(&list, &[0, 1]),
                "export to clipboard" | "export two to clipboard" => list.invoke_exchange(0),
                "duplicate" => list.invoke_exchange(5),
                "export to png" | "export to pngs" => {
                    list.invoke_exchange(if what == "export to png" { 1 } else { 2 });
                    let (started, written) = (&dialogs[0][1], dialogs[0][2].as_array().unwrap());
                    let folder = temp.path().join(run).join(what.replace(' ', "_"));
                    std::fs::create_dir_all(&folder).unwrap();
                    if what == "export to png" {
                        let panel = slots.png.window().expect("the png export opens");
                        assert_eq!(panel.get_png_title(), started["title"].as_str().unwrap());
                        assert_eq!(
                            panel.get_payload_description(),
                            started["payload_description"].as_str().unwrap()
                        );
                        assert_eq!(
                            i64::from(panel.get_png_width()),
                            started["width"].as_i64().unwrap()
                        );
                        panel.set_path(folder.join("single").to_string_lossy().as_ref().into());
                        panel.invoke_action("update".into());
                        panel.invoke_action("export".into());
                        assert!(panel.get_done(), "{}", panel.get_error());
                        panel.invoke_action("close".into());
                    } else {
                        let panel = slots.pngs.window().expect("the pngs export opens");
                        assert_eq!(
                            i64::from(panel.get_png_width()),
                            started["width"].as_i64().unwrap()
                        );
                        // (it starts in the folder the last png went to, as the reference's)
                        assert_eq!(
                            panel.get_path().as_str(),
                            temp.path()
                                .join(run)
                                .join("export_to_png")
                                .to_string_lossy()
                        );
                        panel.set_path(folder.to_string_lossy().as_ref().into());
                        panel.invoke_action("update".into());
                        panel.invoke_action("export".into());
                        assert!(panel.get_done(), "{}", panel.get_error());
                        panel.invoke_action("close".into());
                    }
                    let mut ours: Vec<String> = std::fs::read_dir(&folder)
                        .unwrap()
                        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
                        .collect();
                    ours.sort();
                    let theirs: Vec<String> = written
                        .iter()
                        .map(|w| w["written"].as_str().unwrap().to_owned())
                        .collect();
                    assert_eq!(ours, theirs, "{what}: the files written");
                    for w in written {
                        let ours =
                            std::fs::read(folder.join(w["written"].as_str().unwrap())).unwrap();
                        let theirs = std::fs::read(hydrus_testkit::fixture_path(
                            w["fixture"].as_str().unwrap(),
                        ))
                        .unwrap();
                        let text = w["text"].as_str().unwrap();
                        assert_eq!(
                            hydrus_downloader_exchange::text_png::decode(&ours).unwrap(),
                            text,
                            "{what}"
                        );
                        assert_eq!(
                            hydrus_downloader_exchange::text_png::decode(&theirs).unwrap(),
                            text,
                            "{what}"
                        );
                    }
                    dialogs.remove(0);
                }
                w if w.starts_with("import") && w.contains("png") => {
                    let pngs: Vec<std::path::PathBuf> = if w.contains("no png") {
                        let bad = temp.path().join("bad.png");
                        std::fs::write(&bad, b"not a png").unwrap();
                        vec![bad]
                    } else {
                        let folder = temp.path().join(run).join("export_to_pngs");
                        let mut found: Vec<_> = std::fs::read_dir(&folder)
                            .unwrap()
                            .map(|e| e.unwrap().path())
                            .collect();
                        found.sort();
                        found
                    };
                    *files.borrow_mut() = pngs.clone();
                    list.invoke_exchange(4);
                    // the picker, with the reference's title
                    assert_eq!(
                        picked_titles.borrow().last().unwrap(),
                        dialogs[0][1].as_str().unwrap()
                    );
                    dialogs.remove(0);
                    // (the reference's message names its own temporary file)
                    for d in &mut dialogs {
                        if d[2]
                            .as_str()
                            .is_some_and(|t| t.ends_with("did not appear to be a valid image!"))
                        {
                            d[2] = format!(
                                "\"{}\" did not appear to be a valid image!",
                                pngs[0].display()
                            )
                            .into();
                        }
                    }
                }
                w if w.starts_with("import") => {
                    crate::rules_exchange::paste(step["pasted"].as_str());
                    list.invoke_exchange(3);
                }
                other => panic!("{other}"),
            }
            assert_eq!(
                Value::from(said()),
                Value::from(dialogs),
                "{what}: messages"
            );
            let copies: Vec<String> = copied.borrow()[before..].to_vec();
            assert_eq!(Value::from(copies), step["clipboard"], "{what}: clipboard");
            assert_eq!(Value::from(names(&list)), step["rows"], "{what}: rows");
            assert_eq!(
                Value::from(selected(&list)),
                step["selected"],
                "{what}: selection"
            );
        }
        list.invoke_cancel();
    }
    hydrus_gui::clear_clipboard_reader();
}
