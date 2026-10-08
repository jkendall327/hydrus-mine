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
