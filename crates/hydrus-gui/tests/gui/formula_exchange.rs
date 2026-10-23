//! The formula editor's own "export" and "import" (the reference's formula
//! panel, `EditFormulaPanel`: `ExportToClipboard`, `ExportToPNG`,
//! `ImportFromClipboard`, `ImportFromPNG`, `_ImportObject`), replayed
//! against `oracle/fixtures/formula_exchange.json` (from
//! `oracle/record_formula_exchange.py`): a formula of each kind goes out as
//! the reference's text and png, and what is pasted or opened from a png
//! replaces the formula at once, or is refused with the reference's words.
use hydrus_gui::{formula_window, headless};
use hydrus_gui_model::downloader_interchange::{self as exchange, Native};
use hydrus_gui_model::formula_editors::FormulaTestData;
use hydrus_parse::formula::Formula;
use hydrus_store::Store;
use serde_json::Value;
use slint::ComponentHandle as _;
use std::{cell::RefCell, rc::Rc};

use crate::rules_exchange::{paste, said};

fn recorded() -> Value {
    hydrus_testkit::fixture_json("formula_exchange.json")
}

fn formula(value: &Value) -> Formula {
    let mut definitions = exchange::decode_text(value.as_str().unwrap()).unwrap();
    match definitions.pop().unwrap().native {
        Native::Formula(f) => f,
        other => panic!("not a formula: {other:?}"),
    }
}

struct Editing {
    _dir: tempfile::TempDir,
    window: hydrus_gui::FormulaWindow,
    slots: formula_window::Slots,
    copied: Rc<RefCell<Vec<String>>>,
    applied: Rc<RefCell<Option<Formula>>>,
}

fn editing(start: &Formula) -> Editing {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let slots = formula_window::Slots::default();
    let applied = Rc::new(RefCell::new(None));
    let window = formula_window::open(
        &store,
        start,
        FormulaTestData::default(),
        &slots,
        Rc::new({
            let applied = applied.clone();
            move |f| *applied.borrow_mut() = Some(f)
        }),
    )
    .unwrap();
    *slots.formula.borrow_mut() = Some(window.clone_strong());
    let copied: Rc<RefCell<Vec<String>>> = Rc::default();
    hydrus_gui::set_clipper({
        let copied = copied.clone();
        move |clip| {
            if let hydrus_gui::Clip::Text(text) = clip {
                copied.borrow_mut().push(text.clone());
            }
        }
    });
    Editing {
        _dir: dir,
        window,
        slots,
        copied,
        applied,
    }
}

impl Editing {
    /// "export > to clipboard": the text copied.
    fn exported(&self) -> String {
        let before = self.copied.borrow().len();
        self.window.invoke_exchange(0);
        self.copied.borrow().get(before).cloned().expect("copied")
    }
}

// leaf: formula-exchange
#[test]
fn a_formula_goes_out_and_comes_in_as_the_reference_s_formula_panel_does() {
    let rec = recorded();
    let _windows = headless::init();
    let exports = rec["exports"].as_array().unwrap();

    // to the clipboard: each kind, as the reference's text
    for export in exports {
        let e = editing(&formula(&export["formula"]));
        assert_eq!(e.exported(), export["clipboard"].as_str().unwrap());
    }

    // from the clipboard, into an editor holding the last kind: a formula
    // replaces it at once; anything else is refused and leaves it
    let start = formula(&exports.last().unwrap()["formula"]);
    for import in rec["imports"].as_array().unwrap() {
        let name = import["name"].as_str().unwrap();
        let e = editing(&start);
        paste(import["text"].as_str());
        e.window.invoke_exchange(2);
        assert_eq!(Value::from(said()), import["messages"], "{name}");
        assert_eq!(e.exported(), import["formula"].as_str().unwrap(), "{name}");
        // what it holds is what applying hands on
        e.window.invoke_apply();
        assert_eq!(
            e.applied.borrow().as_ref().unwrap(),
            &formula(&import["formula"]),
            "{name}"
        );
    }
    hydrus_gui::clear_clipboard_reader();
}

// leaf: formula-exchange
#[test]
fn a_formula_goes_to_and_comes_from_the_reference_s_pngs() {
    let rec = recorded();
    let _windows = headless::init();
    let exports = rec["exports"].as_array().unwrap();
    let temp = tempfile::tempdir().unwrap();

    // to a png: the panel starts as the reference's, and the png carries
    // the reference's text
    let png = &rec["png"];
    let e = editing(&formula(&exports[0]["formula"]));
    e.window.invoke_exchange(1);
    let panel = e.slots.png.window().expect("the png export opens");
    assert_eq!(panel.get_png_title(), png["title"].as_str().unwrap());
    assert_eq!(
        panel.get_payload_description(),
        png["payload_description"].as_str().unwrap()
    );
    assert_eq!(panel.get_png_width(), png["width"].as_i64().unwrap() as i32);
    let path = temp.path().join("formula.png");
    panel.set_path(path.to_string_lossy().as_ref().into());
    panel.invoke_action("update".into());
    panel.invoke_action("export".into());
    assert!(panel.get_done(), "{}", panel.get_error());
    panel.invoke_action("close".into());
    assert_eq!(
        hydrus_downloader_exchange::text_png::decode(&std::fs::read(&path).unwrap()).unwrap(),
        png["text"].as_str().unwrap()
    );

    // from a png: the reference's own, one of a string match, and a file
    // that is no png
    let titles: Rc<RefCell<Vec<String>>> = Rc::default();
    let files: Rc<RefCell<Vec<std::path::PathBuf>>> = Rc::default();
    hydrus_gui::set_picker({
        let titles = titles.clone();
        let files = files.clone();
        move |_, title| {
            titles.borrow_mut().push(title.to_owned());
            files.borrow().clone()
        }
    });
    let start = formula(&exports.last().unwrap()["formula"]);
    let bad = temp.path().join("bad.png");
    std::fs::write(&bad, b"not a png").unwrap();
    for import in rec["png_imports"].as_array().unwrap() {
        let name = import["name"].as_str().unwrap();
        let path = import["file"]
            .as_str()
            .map_or_else(|| bad.clone(), hydrus_testkit::fixture_path);
        *files.borrow_mut() = vec![path.clone()];
        let e = editing(&start);
        e.window.invoke_exchange(3);
        // (the reference's message names its own temporary file)
        let mut expected = import["messages"].clone();
        if let Some(text) = expected[0]["text"].as_str()
            && text.ends_with("did not appear to be a valid image!")
        {
            expected[0]["text"] =
                format!("\"{}\" did not appear to be a valid image!", path.display()).into();
        }
        assert_eq!(Value::from(said()), expected, "{name}");
        assert_eq!(e.exported(), import["formula"].as_str().unwrap(), "{name}");
    }
    assert!(
        titles
            .borrow()
            .iter()
            .all(|t| t == rec["png_picker_title"].as_str().unwrap()),
        "{:?}",
        titles.borrow()
    );
}
