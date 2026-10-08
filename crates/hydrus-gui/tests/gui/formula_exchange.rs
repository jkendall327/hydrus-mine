//! The formula editor's own import/export (`ClientGUIParsingFormulae`
//! `_ExportToClipboard`, `ExportToPNG`, `ImportFromClipboard`,
//! `ImportFromPNG`, `_ImportObject`): one formula goes out as serialised
//! text or a PNG, one formula comes in and replaces the draft, and anything
//! else is refused without touching it.
use hydrus_gui::{formula_window, headless};
use hydrus_gui_model::downloader_interchange::{self as exchange, Definition, Native};
use hydrus_gui_model::formula_editors::{FormulaTestData, new_formula};
use hydrus_store::Store;
use slint::ComponentHandle as _;
use std::{cell::RefCell, rc::Rc};

// leaf: formula-exchange
#[test]
fn a_formula_exports_as_text_and_png_and_imports_exactly_one_formula() {
    let _windows = headless::init();
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let slots = formula_window::Slots::default();
    let applied = Rc::new(RefCell::new(None));
    let mut starting = new_formula(false);
    starting.name = "exported".into();
    let w = formula_window::open(
        &store,
        &starting,
        FormulaTestData::default(),
        &slots,
        Rc::new({
            let applied = applied.clone();
            move |f| *applied.borrow_mut() = Some(f)
        }),
    )
    .unwrap();
    *slots.formula.borrow_mut() = Some(w.clone_strong());

    // Export: the draft as serialised text, and as a PNG.
    w.invoke_exchange(false);
    let export = slots.exchange.0.borrow().as_ref().unwrap().clone_strong();
    let exported = exchange::decode_text(export.get_text().as_str()).unwrap();
    assert_eq!(exported.len(), 1);
    assert!(matches!(&exported[0].native, Native::Formula(f) if f.name == "exported"));
    let png = dir.path().join("formula.png");
    export.set_path(png.to_string_lossy().as_ref().into());
    export.invoke_action("save".into());
    assert!(export.get_error().is_empty(), "{}", export.get_error());
    assert_eq!(
        exchange::decode_png(&std::fs::read(&png).unwrap()).unwrap(),
        exported
    );
    export.invoke_action("cancel".into());
    assert!(!slots.exchange.has_open());

    // Import refuses things that are not one formula, and leaves the draft.
    w.invoke_exchange(true);
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
    let mut two = exported.clone();
    two.extend(exported.clone());
    import.set_text(exchange::encode_text(&two).unwrap().into());
    import.invoke_action("review".into());
    assert!(!import.get_ready());
    assert!(!import.get_error().is_empty());
    import.invoke_action("accept".into());
    assert_eq!(w.get_name(), "exported");

    // A formula from a PNG replaces the draft; applying hands it to the owner.
    let mut other = new_formula(true);
    other.name = "from png".into();
    let other_png = dir.path().join("other.png");
    std::fs::write(
        &other_png,
        exchange::encode_png(&[Definition::new(Native::Formula(other.clone()))]).unwrap(),
    )
    .unwrap();
    import.set_text("".into());
    import.set_path(other_png.to_string_lossy().as_ref().into());
    import.invoke_action("open".into());
    assert!(import.get_error().is_empty(), "{}", import.get_error());
    assert!(import.get_ready());
    import.invoke_action("accept".into());
    assert_eq!(w.get_name(), "from png");
    w.invoke_apply();
    let got = applied.borrow().clone().unwrap();
    assert_eq!(got.name, "from png");
    assert_eq!(got.kind, other.kind);
}
