//! Formula child windows, test parsing, cancel/apply, and persistence in
//! simple downloader settings and JSON sidecar sources.
use crate::subscriptions::store;
use hydrus_core::url::strings::{Conversion, ProcessingStep, StringConverter, StringProcessor};
use hydrus_gui::formula_window::FormulaTestData;
use hydrus_gui::{
    MainWindow, Pages, bind, formula_editors::new_formula, formula_window, headless,
    sidecars_window,
};
use hydrus_parse::formula::{FormulaKind, HtmlContent, JsonRule, ParsingContext};
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
fn formula_fetch_decodes_real_documents_updates_preview_and_discards_closed_owner_results() {
    use crate::parser_editors::{TestDocuments, until_fetch};
    let server = TestDocuments::start();
    let (_dirs, store) = store();
    let rendered = headless::init();
    let slots = formula_window::Slots::default();
    let mut formula = new_formula(false);
    if let FormulaKind::Html { rules, content } = &mut formula.kind {
        rules[0].tag_name = Some("p".into());
        *content = HtmlContent::Text;
    }
    let w = formula_window::open(
        &store,
        &formula,
        FormulaTestData {
            text: "<p>previous</p>".into(),
            context: [
                ("url".into(), "https://test-docs.example/pasted".into()),
                ("post_index".into(), "12".into()),
                ("token".into(), "preserved".into()),
            ]
            .into(),
            ..FormulaTestData::default()
        },
        &slots,
        Rc::new(|_| panic!("fetching must not apply formula changes")),
    )
    .unwrap();
    *slots.formula.borrow_mut() = Some(w.clone_strong());
    w.set_fetch_url(format!(" {}/document ", server.base).into());
    w.invoke_fetch();
    assert!(w.get_fetching());
    w.invoke_apply();
    // The imported basic client starts with network traffic paused. Fetching
    // must respect this setting; resume it through the store before expecting HTTP.
    assert!(
        store
            .read(settings::get::<settings::Pauses>)
            .unwrap()
            .network_traffic
    );
    until_fetch(|| w.get_fetch_status().contains("network traffic is paused"));
    assert!(server.requests.lock().unwrap().is_empty());
    store
        .write_and_refresh(|ctx| {
            let mut pauses: settings::Pauses = settings::get(ctx.conn())?;
            pauses.network_traffic = false;
            settings::set(ctx.conn(), &pauses)
        })
        .unwrap();
    until_fetch(|| !w.get_fetching());
    assert_eq!(w.get_document(), "<p>fetched café</p>");
    assert_eq!(labels(&w.get_results()), ["fetched café"]);
    assert!(w.get_context().contains("post_index=0"));
    assert!(w.get_context().contains("token=preserved"));
    assert!(
        w.get_context()
            .contains(&format!("url={}/document", server.base))
    );
    assert_eq!(w.get_examples().row_count(), 2);
    let pixels = headless::render(&rendered.get(0).unwrap(), 1040, 660);
    assert!(pixels.chunks_exact(4).any(|pixel| pixel != &pixels[..4]));
    headless::save_png(
        &std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("formula-fetch.png"),
        &pixels,
        1040,
        660,
    )
    .unwrap();
    assert!(
        !server.requests.lock().unwrap()[0]
            .to_lowercase()
            .contains("referer:")
    );
    w.set_example(0);
    w.invoke_example_chosen();
    assert_eq!(labels(&w.get_results()), ["previous"]);
    assert!(
        w.get_context()
            .contains("url=https://test-docs.example/pasted")
    );
    w.set_example(1);
    w.invoke_example_chosen();
    assert_eq!(labels(&w.get_results()), ["fetched café"]);
    w.set_fetch_url(format!("{}/error", server.base).into());
    w.invoke_fetch();
    until_fetch(|| !w.get_fetching());
    assert_eq!(w.get_document(), "fetch failed:\n\n404: missing");
    w.set_fetch_url(format!("{}/hold", server.base).into());
    w.invoke_fetch();
    until_fetch(|| server.requests.lock().unwrap().len() == 3);
    w.invoke_cancel_fetch();
    until_fetch(|| !w.get_fetching());
    assert_eq!(w.get_document(), "fetch cancelled");
    w.set_fetch_url(format!("{}/hold", server.base).into());
    w.invoke_fetch();
    until_fetch(|| server.requests.lock().unwrap().len() == 4);
    slots.cancel();
    assert!(slots.formula.borrow().is_none());
    let reopened = formula_window::open(
        &store,
        &formula,
        FormulaTestData::default(),
        &slots,
        Rc::new(|_| {}),
    )
    .unwrap();
    *slots.formula.borrow_mut() = Some(reopened.clone_strong());
    slint::platform::update_timers_and_animations();
    assert_eq!(reopened.get_document(), "");
    assert!(!reopened.get_fetching());
    w.invoke_fetch();
    assert!(!w.get_fetching() || !w.window().is_visible());
    reopened.invoke_cancel();
}
// leaf: formula-html-rule
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
fn formula_editors_cancel_children_and_edit_context_formulae() {
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
    assert!(w.get_supported());
    w.set_context("url=https://example.com/post".into());
    w.invoke_test();
    assert_eq!(labels(&w.get_results()), ["https://example.com/post"]);
    w.set_name("edited name".into());
    original.name = "edited name".into();
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

#[test]
fn lifecycle_formula_parent_keeps_rule_address_stable() {
    let (_dirs, store) = store();
    let _windows = headless::init();
    let slots = formula_window::Slots::default();
    let mut formula = new_formula(false);
    let FormulaKind::Html { rules, .. } = &mut formula.kind else {
        panic!()
    };
    let mut second = rules[0].clone();
    second.tag_name = Some("b".into());
    rules.push(second);
    let accepted = Rc::new(RefCell::new(None));
    let w = formula_window::open(
        &store,
        &formula,
        FormulaTestData::default(),
        &slots,
        Rc::new({
            let accepted = accepted.clone();
            move |f| *accepted.borrow_mut() = Some(f)
        }),
    )
    .unwrap();
    *slots.formula.borrow_mut() = Some(w.clone_strong());
    w.invoke_row_clicked(0, false, false);
    w.invoke_edit();
    let rule = slots.rule.borrow().as_ref().unwrap().clone_strong();
    rule.set_tag("span".into());
    rule.set_match_on(true);
    rule.invoke_changed();
    let before = labels(&w.get_rules());
    assert!(w.get_child_open());
    w.invoke_shift(true);
    w.invoke_delete();
    w.invoke_chosen(0);
    w.invoke_add();
    w.invoke_edit_processing();
    w.set_kind(1);
    w.invoke_type_chosen();
    w.invoke_apply();
    assert_eq!(w.get_kind(), 0);
    assert_eq!(labels(&w.get_rules()), before);
    assert!(!w.get_deleting());
    assert!(slots.strings.processor.borrow().is_none());
    assert!(accepted.borrow().is_none());
    rule.invoke_edit_match();
    let matcher = slots.strings.step.borrow().as_ref().unwrap().clone_strong();
    assert!(rule.get_child_open());
    rule.invoke_apply();
    assert!(slots.rule.borrow().is_some());
    matcher.invoke_cancel();
    assert!(!rule.get_child_open());
    assert!(w.get_child_open());
    rule.invoke_apply();
    assert!(!w.get_child_open());
    // The child replaces A, and moving the queue works again after it closes.
    assert!(labels(&w.get_rules())[0].contains("span"));
    assert!(labels(&w.get_rules())[1].contains("<b>"));
    w.invoke_shift(true);
    let after_move = labels(&w.get_rules());
    assert!(after_move[0].contains("<b>"));
    w.invoke_edit_processing();
    let processor = slots
        .strings
        .processor
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    assert!(w.get_child_open());
    w.invoke_shift(false);
    w.invoke_delete();
    w.invoke_apply();
    assert_eq!(labels(&w.get_rules()), after_move);
    assert!(accepted.borrow().is_none());
    processor.invoke_cancel();
    assert!(!w.get_child_open());
    w.invoke_apply();
    let accepted = accepted.borrow();
    let FormulaKind::Html { rules, .. } = &accepted.as_ref().unwrap().kind else {
        panic!()
    };
    assert_eq!(
        rules
            .iter()
            .map(|r| r.tag_name.as_deref())
            .collect::<Vec<_>>(),
        [Some("b"), Some("span")]
    );
}

#[test]
fn lifecycle_string_editors_cancel_descendants_and_reopen() {
    let (_dirs, store) = store();
    let _windows = headless::init();
    let slots = hydrus_gui::string_processor_window::Slots::default();
    let original = StringProcessor {
        steps: vec![ProcessingStep::Convert(StringConverter {
            conversions: vec![Conversion::Append("!".into())],
            example: "example".into(),
        })],
    };
    let accepted = Rc::new(RefCell::new(None));
    let open = |value: &StringProcessor| {
        let w = hydrus_gui::string_processor_window::open(
            &store,
            value,
            vec!["example".into()],
            &slots,
            Rc::new({
                let accepted = accepted.clone();
                move |p| *accepted.borrow_mut() = Some(p)
            }),
        )
        .unwrap();
        *slots.processor.borrow_mut() = Some(w.clone_strong());
        w
    };
    let processor = open(&original);
    processor.invoke_row_clicked(0, false, false);
    processor.invoke_edit();
    let converter = slots.converter.borrow().as_ref().unwrap().clone_strong();
    converter.invoke_row_clicked(0, false, false);
    converter.invoke_edit();
    let conversion = slots.conversion.borrow().as_ref().unwrap().clone_strong();
    conversion.set_text("discarded".into());
    conversion.invoke_changed();
    assert!(processor.get_child_open());
    assert!(converter.get_child_open());
    processor.invoke_apply();
    converter.invoke_apply();
    assert!(accepted.borrow().is_none());
    assert!(slots.processor.borrow().is_some());
    assert!(slots.converter.borrow().is_some());
    processor.invoke_cancel();
    assert!(!slots.has_open());
    assert!(!converter.window().is_visible());
    assert!(!conversion.window().is_visible());
    // Retained canceled handles cannot commit into the old draft or clear
    // newly opened slots.
    let fresh = open(&original);
    conversion.invoke_apply();
    converter.invoke_apply();
    processor.invoke_apply();
    assert!(accepted.borrow().is_none());
    assert!(slots.processor.borrow().is_some());
    fresh.invoke_row_clicked(0, false, false);
    fresh.invoke_edit();
    let converter = slots.converter.borrow().as_ref().unwrap().clone_strong();
    converter.invoke_row_clicked(0, false, false);
    converter.invoke_edit();
    let conversion = slots.conversion.borrow().as_ref().unwrap().clone_strong();
    conversion.invoke_cancel();
    assert!(!converter.get_child_open());
    converter.invoke_apply();
    assert!(!fresh.get_child_open());
    fresh.invoke_apply();
    assert_eq!(*accepted.borrow(), Some(original));
    accepted.borrow_mut().take();
    // A processor's step/tag-filter branch is canceled with the same helper.
    let tags = StringProcessor {
        steps: vec![ProcessingStep::TagFilter(
            hydrus_core::url::strings::TagFilterStep::new(
                hydrus_core::tag_filter::TagFilter::default(),
            ),
        )],
    };
    let processor = open(&tags);
    processor.invoke_row_clicked(0, false, false);
    processor.invoke_edit();
    let step = slots.step.borrow().as_ref().unwrap().clone_strong();
    step.invoke_edit_tag_filter();
    let filter = slots.tag_filter.borrow().as_ref().unwrap().clone_strong();
    assert!(step.get_child_open());
    step.invoke_apply();
    assert!(slots.step.borrow().is_some());
    processor.invoke_cancel();
    assert!(!slots.has_open());
    assert!(!step.window().is_visible());
    assert!(!filter.window().is_visible());
    let fresh = open(&tags);
    fresh.invoke_row_clicked(0, false, false);
    fresh.invoke_edit();
    let fresh_step = slots.step.borrow().as_ref().unwrap().clone_strong();
    fresh_step.invoke_edit_tag_filter();
    filter.invoke_apply();
    step.invoke_apply();
    assert!(accepted.borrow().is_none());
    assert!(slots.tag_filter.borrow().is_some());
    slots.cancel_all();
    assert!(!slots.has_open());
}

#[test]
fn scalar_formula_controls_preview_processing_cancel_and_save() {
    let (_dirs, store) = store();
    let windows = headless::init();
    let slots = formula_window::Slots::default();
    let original = hydrus_gui::formula_editors::new_formula_kind(5);
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
    assert_eq!(w.get_kind(), 5);
    assert_eq!(w.get_static_text(), "example text");
    w.set_name("three constants".into());
    w.set_static_text(" first\n second ".into());
    w.set_output_count(3);
    w.invoke_changed();
    w.invoke_test();
    assert_eq!(
        labels(&w.get_results()),
        ["firstsecond", "firstsecond", "firstsecond"]
    );
    w.invoke_edit_processing();
    let processor = slots
        .strings
        .processor
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    assert_eq!(
        labels(&processor.get_starting()),
        ["firstsecond", "firstsecond", "firstsecond"]
    );
    processor.invoke_cancel();
    let pixels = headless::render(&windows.get(0).unwrap(), 1040, 660);
    let shot = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("formula_static.png");
    headless::save_png(&shot, &pixels, 1040, 660).unwrap();
    w.invoke_apply();
    let f = accepted.borrow_mut().take().unwrap();
    assert_eq!(f.name, "three constants");
    assert_eq!(
        f.kind,
        FormulaKind::Static {
            text: " first\n second ".into(),
            count: 3
        }
    );
    assert_eq!(original, hydrus_gui::formula_editors::new_formula_kind(5));
    let w = formula_window::open(
        &store,
        &f,
        FormulaTestData::default(),
        &slots,
        Rc::new(|_| panic!("cancel applied")),
    )
    .unwrap();
    *slots.formula.borrow_mut() = Some(w.clone_strong());
    assert_eq!(w.get_output_count(), 3);
    w.set_static_text("discarded".into());
    w.invoke_changed();
    w.invoke_cancel();
    assert!(slots.formula.borrow().is_none());
    let w = formula_window::open(
        &store,
        &f,
        FormulaTestData::default(),
        &slots,
        Rc::new({
            let accepted = accepted.clone();
            move |f| *accepted.borrow_mut() = Some(f)
        }),
    )
    .unwrap();
    *slots.formula.borrow_mut() = Some(w.clone_strong());
    w.set_kind(4);
    w.invoke_type_chosen();
    assert_eq!(w.get_variable(), "url");
    assert_eq!(w.get_name(), "");
    w.set_variable("note".into());
    w.set_context("note=custom=value".into());
    w.invoke_changed();
    w.invoke_test();
    assert_eq!(labels(&w.get_results()), ["custom=value"]);
    w.invoke_apply();
    assert_eq!(
        accepted.borrow().as_ref().unwrap().kind,
        FormulaKind::ContextVariable {
            variable: "note".into()
        }
    );
}

fn child_formula(slots: &formula_window::Slots) -> hydrus_gui::FormulaWindow {
    let child = slots.child.borrow();
    let formula = child.as_ref().unwrap().formula.borrow();
    formula.as_ref().unwrap().clone_strong()
}
fn embedded_formula() -> hydrus_parse::formula::Formula {
    let mut formula = hydrus_gui::formula_editors::new_formula_kind(2);
    let FormulaKind::Nested { main, sub } = &mut formula.kind else {
        panic!()
    };
    let FormulaKind::Html { rules, content } = &mut main.kind else {
        panic!()
    };
    rules[0].tag_name = Some("script".into());
    *content = HtmlContent::Text;
    let FormulaKind::Json { rules, .. } = &mut sub.kind else {
        panic!()
    };
    rules.push(JsonRule::AllItems);
    formula
}
#[test]
fn recursive_nested_children_preview_transformed_examples_and_save() {
    let (_dirs, store) = store();
    let windows = headless::init();
    let slots = formula_window::Slots::default();
    let cases: Vec<serde_json::Value> = serde_json::from_value(hydrus_testkit::fixture_json(
        "recursive_formula_editors.json",
    ))
    .unwrap();
    let case = cases
        .iter()
        .find(|c| c["case"] == "nested_formula")
        .unwrap();
    let context: hydrus_parse::formula::ParsingContext =
        serde_json::from_value(case["context"].clone()).unwrap();
    let mut original = embedded_formula();
    original.processor = StringProcessor {
        steps: vec![ProcessingStep::Convert(StringConverter {
            conversions: vec![Conversion::Append("!".into())],
            example: String::new(),
        })],
    };
    let accepted = Rc::new(RefCell::new(None));
    let w = formula_window::open(
        &store,
        &original,
        FormulaTestData {
            context: context.clone(),
            text: case["text"].as_str().unwrap().into(),
            ..FormulaTestData::default()
        },
        &slots,
        Rc::new({
            let accepted = accepted.clone();
            move |f| *accepted.borrow_mut() = Some(f)
        }),
    )
    .unwrap();
    *slots.formula.borrow_mut() = Some(w.clone_strong());
    w.set_name("embedded json".into());
    w.invoke_changed();
    assert_eq!(serde_json::json!(labels(&w.get_results())), case["results"]);
    w.invoke_edit_child(false);
    let main = child_formula(&slots);
    assert_eq!(main.get_document(), case["text"].as_str().unwrap());
    assert!(w.get_child_open());
    w.set_kind(5);
    w.invoke_type_chosen();
    assert_eq!(w.get_kind(), 2);
    w.invoke_apply();
    assert!(accepted.borrow().is_none());
    main.set_kind(5);
    main.invoke_type_chosen();
    main.set_static_text("{\"posts\":[\"discarded\"]}".into());
    main.invoke_changed();
    main.invoke_cancel();
    assert_eq!(serde_json::json!(labels(&w.get_results())), case["results"]);
    w.invoke_edit_child(false);
    let main = child_formula(&slots);
    assert_eq!(main.get_kind(), 0);
    main.set_kind(5);
    main.invoke_type_chosen();
    main.set_static_text("{\"posts\":[\"changed\"]}".into());
    main.invoke_apply();
    let case = cases
        .iter()
        .find(|c| c["case"] == "nested_main_edit")
        .unwrap();
    assert_eq!(serde_json::json!(labels(&w.get_results())), case["results"]);
    w.invoke_edit_child(true);
    let sub = child_formula(&slots);
    assert_eq!(sub.get_document(), case["sub_texts"][0].as_str().unwrap());
    assert!(
        sub.get_context()
            .contains("url=https://formula.example/post")
    );
    sub.set_kind(4);
    sub.invoke_type_chosen();
    sub.invoke_apply();
    let case = cases
        .iter()
        .find(|c| c["case"] == "nested_sub_edit")
        .unwrap();
    assert_eq!(serde_json::json!(labels(&w.get_results())), case["results"]);
    let pixels = headless::render(&windows.get(0).unwrap(), 1040, 660);
    let shot = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("formula_nested.png");
    headless::save_png(&shot, &pixels, 1040, 660).unwrap();
    w.invoke_apply();
    let saved = accepted.borrow_mut().take().unwrap();
    assert_eq!(saved.name, "embedded json");
    assert_eq!(
        serde_json::json!(saved.parse(&context, "", true).unwrap()),
        case["results"]
    );
    assert_eq!(original.name, "");
    let w = formula_window::open(
        &store,
        &saved,
        FormulaTestData::default(),
        &slots,
        Rc::new(|_| panic!("cancel applied")),
    )
    .unwrap();
    *slots.formula.borrow_mut() = Some(w.clone_strong());
    w.invoke_edit_child(false);
    assert_eq!(
        child_formula(&slots).get_static_text(),
        "{\"posts\":[\"changed\"]}"
    );
    w.invoke_cancel();
    assert!(slots.child.borrow().is_none());
}
#[test]
fn recursive_second_formula_can_select_every_transformed_example() {
    let (_dirs, store) = store();
    let _windows = headless::init();
    let slots = formula_window::Slots::default();
    let cases: Vec<serde_json::Value> = serde_json::from_value(hydrus_testkit::fixture_json(
        "recursive_formula_editors.json",
    ))
    .unwrap();
    let case = cases
        .iter()
        .find(|c| c["case"] == "nested_multiple")
        .unwrap();
    let mut formula = embedded_formula();
    formula.processor = StringProcessor {
        steps: vec![ProcessingStep::Convert(StringConverter {
            conversions: vec![Conversion::Append("!".into())],
            example: String::new(),
        })],
    };
    let w = formula_window::open(
        &store,
        &formula,
        FormulaTestData {
            text: case["text"].as_str().unwrap().into(),
            ..FormulaTestData::default()
        },
        &slots,
        Rc::new(|_| panic!("cancel applied")),
    )
    .unwrap();
    *slots.formula.borrow_mut() = Some(w.clone_strong());
    assert_eq!(serde_json::json!(labels(&w.get_results())), case["results"]);
    w.invoke_edit_child(true);
    let sub = child_formula(&slots);
    assert_eq!(
        serde_json::json!([sub.get_document().to_string()]),
        serde_json::json!([case["sub_texts"][0].as_str().unwrap()])
    );
    assert_eq!(
        sub.get_examples().row_count(),
        case["sub_texts"].as_array().unwrap().len()
    );
    assert_eq!(labels(&sub.get_results()), ["first"]);
    sub.set_example(1);
    sub.invoke_example_chosen();
    assert_eq!(sub.get_document(), "{\"posts\":[\"second\"]}");
    assert_eq!(labels(&sub.get_results()), ["second"]);
    sub.set_document("{\"posts\":[\"edited example\"]}".into());
    sub.invoke_changed();
    sub.set_example(0);
    sub.invoke_example_chosen();
    assert_eq!(labels(&sub.get_results()), ["first"]);
    sub.set_example(1);
    sub.invoke_example_chosen();
    assert_eq!(labels(&sub.get_results()), ["edited example"]);
    sub.set_kind(2);
    sub.invoke_type_chosen();
    sub.invoke_edit_child(false);
    let inner = {
        let sub_slots = slots.child.borrow();
        child_formula(sub_slots.as_ref().unwrap())
    };
    assert_eq!(inner.get_document(), "{\"posts\":[\"edited example\"]}");
    inner.invoke_cancel();
    sub.invoke_cancel();
    assert_eq!(serde_json::json!(labels(&w.get_results())), case["results"]);
    w.invoke_cancel();
}
#[test]
fn recursive_zipper_queue_matches_reference_and_blocks_parent_mutations() {
    let (_dirs, store) = store();
    let _windows = headless::init();
    let slots = formula_window::Slots::default();
    let mut original = hydrus_gui::formula_editors::new_formula_kind(3);
    let mut first = hydrus_gui::formula_editors::new_formula_kind(5);
    first.kind = FormulaKind::Static {
        text: "first".into(),
        count: 1,
    };
    let FormulaKind::Zipper { formulae, .. } = &mut original.kind else {
        panic!()
    };
    *formulae = vec![first];
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
    let cases: Vec<serde_json::Value> = serde_json::from_value(hydrus_testkit::fixture_json(
        "recursive_formula_editors.json",
    ))
    .unwrap();
    for case in cases.iter().filter(|c| c["case"] == "zipper_queue") {
        match case["action"].as_str().unwrap() {
            "add" => {
                w.invoke_add();
                let child = child_formula(&slots);
                assert_eq!(child.get_kind(), 0);
                child.set_kind(5);
                child.invoke_type_chosen();
                child.set_static_text("second".into());
                let before = labels(&w.get_rules());
                w.invoke_shift(true);
                w.invoke_delete();
                w.invoke_chosen(0);
                w.invoke_apply();
                assert_eq!(labels(&w.get_rules()), before);
                assert!(accepted.borrow().is_none());
                child.invoke_apply();
            }
            "edit" => {
                w.invoke_row_clicked(1, false, false);
                w.invoke_edit();
                let child = child_formula(&slots);
                assert_eq!(child.get_static_text(), "second");
                child.set_static_text("replacement".into());
                child.invoke_apply();
            }
            "up" => w.invoke_shift(false),
            "down" => w.invoke_shift(true),
            "cancel_add" => {
                w.invoke_add();
                let child = child_formula(&slots);
                child.set_kind(4);
                child.invoke_type_chosen();
                child.invoke_cancel();
            }
            "cancel_edit" => {
                w.invoke_row_clicked(1, false, false);
                w.invoke_edit();
                let child = child_formula(&slots);
                child.set_static_text("discarded".into());
                child.invoke_cancel();
            }
            "delete" => {
                w.invoke_delete();
                w.invoke_chosen(0);
            }
            other => panic!("{other}"),
        }
        let descriptions = labels(&w.get_rules());
        let expected = case["texts"].as_array().unwrap();
        assert_eq!(descriptions.len(), expected.len());
        for (label, text) in descriptions.iter().zip(expected) {
            assert!(label.ends_with(text.as_str().unwrap()));
        }
        let selection = (0..w.get_rules().row_count())
            .filter(|i| w.get_rules().row_data(*i).unwrap().selected)
            .collect::<Vec<_>>();
        assert_eq!(serde_json::json!(selection), case["selected"]);
    }
    // The surviving draft is persisted with its edited phrase, while the input
    // object and canceled edits stay intact.
    w.set_phrase("prefix \\1".into());
    w.invoke_changed();
    assert_eq!(labels(&w.get_results()), ["prefix first"]);
    w.invoke_apply();
    let saved = accepted.borrow_mut().take().unwrap();
    assert_eq!(
        saved.parse(&ParsingContext::default(), "", true).unwrap(),
        ["prefix first"]
    );
    assert_eq!(
        original
            .parse(&ParsingContext::default(), "", true)
            .unwrap(),
        ["first"]
    );
    let w = formula_window::open(
        &store,
        &saved,
        FormulaTestData::default(),
        &slots,
        Rc::new(|_| panic!("cancel applied")),
    )
    .unwrap();
    *slots.formula.borrow_mut() = Some(w.clone_strong());
    assert_eq!(w.get_phrase(), "prefix \\1");
    assert_eq!(w.get_rules().row_count(), 1);
    w.invoke_cancel();
}
#[test]
fn recursive_owner_cancel_invalidates_every_depth_and_retained_handle() {
    let (_dirs, store) = store();
    let _windows = headless::init();
    let slots = formula_window::Slots::default();
    let accepted = Rc::new(RefCell::new(None));
    let open = || {
        let w = formula_window::open(
            &store,
            &embedded_formula(),
            FormulaTestData::default(),
            &slots,
            Rc::new({
                let accepted = accepted.clone();
                move |f| *accepted.borrow_mut() = Some(f)
            }),
        )
        .unwrap();
        *slots.formula.borrow_mut() = Some(w.clone_strong());
        w
    };
    let w = open();
    w.invoke_edit_child(false);
    let child = child_formula(&slots);
    child.set_kind(2);
    child.invoke_type_chosen();
    child.invoke_edit_child(false);
    let grandchild = {
        let child_slots = slots.child.borrow();
        child_formula(child_slots.as_ref().unwrap())
    };
    grandchild.invoke_add();
    let rule = {
        let child_slots = slots.child.borrow();
        let grandchild_slots = child_slots.as_ref().unwrap().child.borrow();
        let rule = grandchild_slots.as_ref().unwrap().rule.borrow();
        rule.as_ref().unwrap().clone_strong()
    };
    rule.set_tag("discarded".into());
    rule.invoke_changed();
    slots.cancel();
    assert!(slots.formula.borrow().is_none());
    assert!(slots.child.borrow().is_none());
    assert!(!child.window().is_visible());
    assert!(!grandchild.window().is_visible());
    assert!(!rule.window().is_visible());
    let fresh = open();
    fresh.invoke_edit_child(false);
    rule.invoke_apply();
    grandchild.invoke_apply();
    child.invoke_apply();
    w.invoke_apply();
    assert!(accepted.borrow().is_none());
    assert!(slots.child.borrow().is_some());
    child_formula(&slots).invoke_cancel();
    assert!(!fresh.get_child_open());
    fresh.invoke_apply();
    assert_eq!(*accepted.borrow(), Some(embedded_formula()));
}

#[test]
fn zipper_components_exchange_appends_only_formulae_and_exports_selection() {
    use hydrus_gui_model::downloader_interchange::{self as exchange, Definition, Native};
    let (_dirs, store) = store();
    let _windows = headless::init();
    let slots = formula_window::Slots::default();
    let accepted = Rc::new(RefCell::new(None));
    let w = formula_window::open(
        &store,
        &hydrus_gui::formula_editors::new_formula_kind(3),
        FormulaTestData::default(),
        &slots,
        Rc::new({
            let accepted = accepted.clone();
            move |f| *accepted.borrow_mut() = Some(f)
        }),
    )
    .unwrap();
    *slots.formula.borrow_mut() = Some(w.clone_strong());
    let definitions = vec![
        Definition::new(Native::Formula(
            hydrus_gui::formula_editors::new_formula_kind(4),
        )),
        Definition::new(Native::Formula(
            hydrus_gui::formula_editors::new_formula_kind(5),
        )),
    ];
    w.invoke_member_exchange(true);
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
    assert!(import.get_error().contains("component formulae"));
    assert_eq!(w.get_rules().row_count(), 1);
    import.set_text(exchange::encode_text(&definitions).unwrap().into());
    import.invoke_action("review".into());
    assert!(import.get_ready());
    assert_eq!(w.get_rules().row_count(), 1);
    w.invoke_apply();
    assert!(accepted.borrow().is_none());
    import.invoke_action("accept".into());
    assert_eq!(w.get_rules().row_count(), 3);
    w.invoke_row_clicked(2, false, false);
    w.invoke_member_exchange(false);
    let export = slots.exchange.0.borrow().as_ref().unwrap().clone_strong();
    let exported = exchange::decode_text(export.get_text().as_str()).unwrap();
    assert_eq!(exported.len(), 1);
    // Decoding attaches the preserved reference tuple and editor auxiliary data.
    // Compare the actual serializable object, rather than an unimported wrapper.
    assert_eq!(
        exported[0].tuple().unwrap(),
        definitions[1].tuple().unwrap()
    );
    assert_eq!(
        exported[0].original.as_ref(),
        Some(&definitions[1].tuple().unwrap())
    );
    export.invoke_action("cancel".into());
    w.invoke_member_exchange(true);
    let import = slots.exchange.0.borrow().as_ref().unwrap().clone_strong();
    import.set_text(exchange::encode_text(&definitions).unwrap().into());
    import.invoke_action("review".into());
    w.invoke_cancel();
    import.invoke_action("accept".into());
    assert!(accepted.borrow().is_none());
    assert!(!slots.exchange.has_open());
}
