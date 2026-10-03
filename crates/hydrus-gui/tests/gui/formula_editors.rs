//! Formula child windows, test parsing, cancel/apply, and persistence in
//! simple downloader settings and JSON sidecar sources.
use crate::subscriptions::store;
use hydrus_core::url::strings::{StringConverter, StringProcessor};
use hydrus_gui::formula_window::FormulaTestData;
use hydrus_gui::{
    MainWindow, Pages, bind, formula_editors::new_formula, formula_window, headless,
    sidecars_window,
};
use hydrus_parse::formula::{FormulaKind, HtmlContent, JsonRule};
use hydrus_store::settings::{self, SimpleDownloaderFormulae};
use slint::{ComponentHandle as _, Model as _};
use std::{cell::RefCell, rc::Rc};
fn labels(rows: &slint::ModelRc<hydrus_gui::TableRow>) -> Vec<String> {
    (0..rows.row_count())
        .map(|i| {
            rows.row_data(i)
                .unwrap()
                .cells
                .row_data(0)
                .unwrap()
                .to_string()
        })
        .collect()
}
#[test]
fn formula_editors_html_rules_validation_processing_and_screenshot() {
    let (_dirs, store) = store();
    let windows = headless::init();
    let slots = formula_window::Slots::default();
    let applied = Rc::new(RefCell::new(None));
    let w = formula_window::open(
        &store,
        &new_formula(false),
        FormulaTestData::default(),
        &slots,
        Rc::new({
            let applied = applied.clone();
            move |f| *applied.borrow_mut() = Some(f)
        }),
    )
    .unwrap();
    *slots.formula.borrow_mut() = Some(w.clone_strong());
    w.set_attribute("".into());
    w.invoke_apply();
    assert_eq!(w.get_veto(), "Please enter an attribute to fetch!");
    assert!(applied.borrow().is_none());
    w.set_attribute("href".into());
    w.set_document("<html><a href=\"one\">first</a><a href=\"two\">second</a></html>".into());
    w.invoke_test();
    assert_eq!(labels(&w.get_results()), ["one", "two"]);
    w.invoke_row_clicked(0, false, false);
    w.invoke_edit();
    let rule = slots.rule.borrow().as_ref().unwrap().clone_strong();
    rule.set_index_on(true);
    rule.set_index(-1);
    rule.invoke_changed();
    assert!(rule.get_description().contains("last <a>"));
    rule.invoke_apply();
    w.invoke_test();
    assert_eq!(labels(&w.get_results()), ["two"]);
    w.invoke_edit();
    let rule = slots.rule.borrow().as_ref().unwrap().clone_strong();
    rule.set_index_on(false);
    rule.set_match_on(true);
    rule.invoke_changed();
    rule.invoke_edit_match();
    let matcher = slots.strings.step.borrow().as_ref().unwrap().clone_strong();
    matcher.set_match_type(1);
    matcher.set_fixed("second".into());
    matcher.invoke_changed();
    matcher.invoke_apply();
    assert!(rule.get_match_label().contains("second"));
    rule.invoke_apply();
    w.invoke_test();
    assert_eq!(labels(&w.get_results()), ["two"]);
    w.invoke_edit_processing();
    let p = slots
        .strings
        .processor
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    assert_eq!(labels(&p.get_starting()), ["two"]);
    p.invoke_cancel();
    w.invoke_add();
    let rule = slots.rule.borrow().as_ref().unwrap().clone_strong();
    rule.set_rule_type(1);
    rule.set_tag("".into());
    rule.set_depth(1);
    rule.invoke_changed();
    rule.invoke_apply();
    w.set_content(2);
    w.invoke_changed();
    w.invoke_test();
    assert!(labels(&w.get_results())[0].contains("<body>"));
    w.invoke_row_clicked(1, false, false);
    w.invoke_delete();
    assert_eq!(w.get_question(), "Remove 1 selected?");
    w.invoke_cancelled();
    assert_eq!(w.get_rules().row_count(), 2);
    w.invoke_delete();
    w.invoke_chosen(0);
    assert_eq!(w.get_rules().row_count(), 1);
    w.set_content(0);
    w.invoke_changed();
    w.invoke_test();
    let pixels = headless::render(&windows.get(0).unwrap(), 1040, 660);
    let shot = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("formula_html.png");
    headless::save_png(&shot, &pixels, 1040, 660).unwrap();
    w.set_name("links".into());
    w.invoke_apply();
    let f = applied.borrow().clone().unwrap();
    assert_eq!(f.name, "links");
    let FormulaKind::Html { content, .. } = f.kind else {
        panic!()
    };
    assert_eq!(content, HtmlContent::Attribute("href".into()));
    assert!(slots.formula.borrow().is_none());
}
#[test]
fn formula_editors_simple_downloader_list_saves_cancel_and_unique_names() {
    let (_dirs, store) = store();
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    (bound.open_page)(&hydrus_gui::page_chooser::NewPage::SimpleDownloader);
    let before = store
        .read(settings::get::<SimpleDownloaderFormulae>)
        .unwrap();
    ui.invoke_simple_edit_formulae();
    let slots = &bound.simple_formulae;
    let list = slots.list.borrow().as_ref().unwrap().clone_strong();
    let count = list.get_rows().row_count();
    list.invoke_add();
    list.set_name("json links".into());
    list.invoke_named();
    let formula = slots
        .formula
        .formula
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    formula.set_kind(1);
    formula.invoke_type_chosen();
    formula.set_document("{\"posts\":[\"one\",\"two\"]}".into());
    formula.invoke_add();
    let r = slots.formula.rule.borrow().as_ref().unwrap().clone_strong();
    r.set_rule_type(1);
    r.invoke_changed();
    r.invoke_apply();
    formula.invoke_test();
    assert_eq!(labels(&formula.get_results()), ["one", "two"]);
    formula.invoke_apply();
    assert_eq!(list.get_rows().row_count(), count + 1);
    // Saving the list changes formulae only; a chooser change made while
    // the list is open must remain the favourite for subsequent pages.
    ui.invoke_simple_formula_chosen(1);
    let favourite = store
        .read(settings::get::<SimpleDownloaderFormulae>)
        .unwrap()
        .favourite;
    list.invoke_apply();
    let written = store
        .read(settings::get::<SimpleDownloaderFormulae>)
        .unwrap();
    assert_eq!(written.favourite, favourite);
    assert_eq!(written.formulae.len(), before.formulae.len() + 1);
    assert!(written.formulae.iter().any(|f| f.name == "json links"));
    assert!(
        (0..ui.get_simple_formulae().row_count()).any(|i| ui
            .get_simple_formulae()
            .row_data(i)
            .unwrap()
            == "json links")
    );
    ui.invoke_simple_edit_formulae();
    let list = slots.list.borrow().as_ref().unwrap().clone_strong();
    let i = labels(&list.get_rows())
        .iter()
        .position(|n| n == "json links")
        .unwrap();
    list.invoke_row_clicked(i32::try_from(i).unwrap(), false, false);
    list.invoke_edit();
    list.invoke_named();
    let formula = slots
        .formula
        .formula
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    formula.invoke_apply();
    assert!(labels(&list.get_rows()).contains(&"json links (1)".into()));
    list.invoke_cancel();
    assert_eq!(
        store
            .read(settings::get::<SimpleDownloaderFormulae>)
            .unwrap(),
        written
    );
    ui.invoke_simple_edit_formulae();
    let list = slots.list.borrow().as_ref().unwrap().clone_strong();
    list.invoke_row_clicked(0, false, false);
    list.invoke_delete();
    assert_eq!(list.get_question(), "Remove 1 selected?");
    list.invoke_chosen(1);
    assert_eq!(list.get_rows().row_count(), written.formulae.len());
    list.invoke_delete();
    list.invoke_chosen(0);
    list.invoke_apply();
    assert_eq!(
        store
            .read(settings::get::<SimpleDownloaderFormulae>)
            .unwrap()
            .formulae
            .len(),
        written.formulae.len() - 1
    );
}
#[test]
fn formula_editors_sidecar_json_child_applies_to_source_only_on_acceptance() {
    let (_dirs, store) = store();
    let windows = headless::init();
    let slots = sidecars_window::Slots::default();
    let node = sidecars_window::Node::Source(hydrus_parse::sidecar::Importer {
        source: hydrus_parse::sidecar::Source::Json {
            naming: hydrus_parse::sidecar::SidecarNaming {
                remove_actual_filename_ext: false,
                suffix: String::new(),
                filename_converter: StringConverter::default(),
            },
            formula: Box::new(new_formula(true)),
        },
        processor: StringProcessor::default(),
    });
    let applied = Rc::new(RefCell::new(None));
    let w = sidecars_window::open_node(
        &store,
        hydrus_gui::sidecar_editors::Context::Import,
        &node,
        &slots,
        Rc::new({
            let applied = applied.clone();
            move |n| *applied.borrow_mut() = Some(n)
        }),
    )
    .unwrap();
    *slots.node.borrow_mut() = Some(w.clone_strong());
    w.invoke_edit_formula();
    let f = slots
        .formula
        .formula
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    assert!(!f.get_allow_type_change());
    assert!(f.get_newline_note().contains("not collapsed"));
    f.invoke_row_clicked(0, false, false);
    f.invoke_edit();
    let r = slots.formula.rule.borrow().as_ref().unwrap().clone_strong();
    r.set_rule_type(1);
    r.invoke_changed();
    r.invoke_apply();
    f.set_document("[\"tag:one\",\"tag:two\"]".into());
    f.invoke_test();
    assert_eq!(labels(&f.get_results()), ["tag:one", "tag:two"]);
    f.set_document("[\" first\\n second \"]".into());
    f.invoke_test();
    assert_eq!(labels(&f.get_results()), ["first\n second"]);
    f.set_document("[\"tag:one\",\"tag:two\"]".into());
    f.invoke_test();
    let pixels = headless::render(&windows.get(1).unwrap(), 1040, 660);
    headless::save_png(
        &std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("formula_json.png"),
        &pixels,
        1040,
        660,
    )
    .unwrap();
    f.invoke_apply();
    w.invoke_apply();
    let Some(sidecars_window::Node::Source(importer)) = &*applied.borrow() else {
        panic!()
    };
    let hydrus_parse::sidecar::Source::Json { formula, .. } = &importer.source else {
        panic!()
    };
    let FormulaKind::Json { rules, .. } = &formula.kind else {
        panic!()
    };
    assert_eq!(rules, &[JsonRule::AllItems]);
}

#[test]
fn formula_editors_cancel_children_and_preserve_unsupported_formulae() {
    let (_dirs, store) = store();
    let _windows = headless::init();
    let slots = formula_window::Slots::default();
    let mut original = new_formula(false);
    original.kind = FormulaKind::ContextVariable {
        variable: "url".into(),
    };
    original.name = "kept".into();
    let accepted = Rc::new(RefCell::new(None));
    let w = formula_window::open(
        &store,
        &original,
        FormulaTestData::default(),
        &slots,
        Rc::new({
            let accepted = accepted.clone();
            move |f| *accepted.borrow_mut() = Some(f)
        }),
    )
    .unwrap();
    *slots.formula.borrow_mut() = Some(w.clone_strong());
    assert!(!w.get_supported());
    w.set_context("url=https://example.com/post".into());
    w.invoke_test();
    assert_eq!(labels(&w.get_results()), ["https://example.com/post"]);
    w.set_name("must not change".into());
    w.invoke_apply();
    assert_eq!(*accepted.borrow(), Some(original));
    let w = formula_window::open(
        &store,
        &new_formula(false),
        FormulaTestData::default(),
        &slots,
        Rc::new(|_| panic!("cancel applied")),
    )
    .unwrap();
    *slots.formula.borrow_mut() = Some(w.clone_strong());
    w.invoke_add();
    let rule = slots.rule.borrow().as_ref().unwrap().clone_strong();
    rule.set_match_on(true);
    rule.invoke_changed();
    rule.invoke_edit_match();
    assert!(slots.strings.step.borrow().is_some());
    slots.cancel();
    assert!(slots.formula.borrow().is_none());
    assert!(slots.rule.borrow().is_none());
    assert!(slots.strings.step.borrow().is_none());
}

#[test]
fn formula_editors_json_sidecar_formula_is_saved_with_import_folder() {
    let (_dirs, store) = store();
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    let watched = tempfile::tempdir().unwrap();
    crate::folders::open(&ui, "manage import folders\u{2026}");
    let list = bound
        .folders
        .import_list
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    list.invoke_add();
    let folder = bound
        .folders
        .import_edit
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    folder.set_path(watched.path().to_string_lossy().into_owned().into());
    folder.invoke_edit_sidecars();
    let slots = &bound.folders.sidecars;
    let routers = slots.routers.borrow().as_ref().unwrap().clone_strong();
    routers.invoke_add();
    let router = slots.router.borrow().as_ref().unwrap().clone_strong();
    router.invoke_add();
    let choices = router.get_asking_choices();
    let json = (0..choices.row_count())
        .find(|i| choices.row_data(*i).unwrap() == "a .json sidecar")
        .unwrap();
    router.invoke_chosen(i32::try_from(json).unwrap());
    let node = slots.node.borrow().as_ref().unwrap().clone_strong();
    node.invoke_edit_formula();
    let formula = slots
        .formula
        .formula
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    formula.invoke_row_clicked(0, false, false);
    formula.invoke_edit();
    let rule = slots.formula.rule.borrow().as_ref().unwrap().clone_strong();
    rule.set_rule_type(2);
    rule.set_index(-1);
    rule.invoke_changed();
    rule.invoke_apply();
    formula.invoke_apply();
    node.invoke_apply();
    router.invoke_apply();
    routers.invoke_apply();
    folder.invoke_apply();
    list.invoke_apply();
    let saved = store
        .read(hydrus_store::import_folders::import_folders)
        .unwrap();
    let hydrus_parse::sidecar::Source::Json { formula, .. } =
        &saved[0].settings.routers[0].importers[0].source
    else {
        panic!()
    };
    let FormulaKind::Json { rules, .. } = &formula.kind else {
        panic!()
    };
    assert_eq!(rules, &[JsonRule::Index(-1)]);
}

#[test]
fn formula_editors_bulk_edit_sequences_and_cancelled_children_unlock_parent() {
    let (_dirs, store) = store();
    let _windows = headless::init();
    let slots = hydrus_gui::simple_formulae_window::Slots::default();
    let before = store
        .read(settings::get::<SimpleDownloaderFormulae>)
        .unwrap();
    let applied = Rc::new(std::cell::Cell::new(false));
    let list = hydrus_gui::simple_formulae_window::open(
        &store,
        &slots,
        Rc::new({
            let applied = applied.clone();
            move || applied.set(true)
        }),
    )
    .unwrap();
    *slots.list.borrow_mut() = Some(list.clone_strong());
    let names = labels(&list.get_rows());
    assert!(names.len() >= 2);
    list.invoke_add();
    list.set_name("cancelled".into());
    list.invoke_named();
    assert!(list.get_editing_formula());
    let f = slots
        .formula
        .formula
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    f.invoke_cancel();
    assert!(!list.get_editing_formula());
    assert_eq!(labels(&list.get_rows()), names);
    list.invoke_row_clicked(0, false, false);
    list.invoke_row_clicked(1, true, false);
    list.invoke_edit();
    assert!(list.get_naming());
    assert_eq!(list.get_name(), names[0]);
    list.set_name("bulk first".into());
    list.invoke_named();
    let f = slots
        .formula
        .formula
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    f.invoke_apply();
    assert!(list.get_naming());
    assert!(!list.get_editing_formula());
    assert_eq!(list.get_name(), names[1]);
    list.set_name("bulk second".into());
    list.invoke_named();
    let f = slots
        .formula
        .formula
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    f.invoke_cancel();
    assert!(!list.get_naming());
    assert!(!list.get_editing_formula());
    let mut expected = names;
    expected[0] = "bulk first".into();
    assert_eq!(labels(&list.get_rows()), expected);
    assert_eq!(
        store
            .read(settings::get::<SimpleDownloaderFormulae>)
            .unwrap(),
        before
    );
    list.invoke_apply();
    assert!(applied.get());
    let saved = store
        .read(settings::get::<SimpleDownloaderFormulae>)
        .unwrap();
    assert_eq!(
        saved
            .formulae
            .iter()
            .map(|f| f.name.clone())
            .collect::<Vec<_>>(),
        expected
    );
    assert_eq!(saved.favourite, before.favourite);
}
