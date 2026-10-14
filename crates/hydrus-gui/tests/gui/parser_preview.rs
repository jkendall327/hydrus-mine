//! The parser test panel's results text against the reference's
//! (`oracle/fixtures/parser_preview.json`, from `oracle/record_parser_preview.py`): the real
//! content parser and page parser panels, 'test parse' pressed on each recorded parser and
//! document. Through the content and page parser editor windows here.
use hydrus_gui::{
    ParserEditWindow, headless,
    parser_editors_window::{self as windows, Slots},
};
use hydrus_gui_model::formula_editors::FormulaTestData;
use hydrus_legacy::{objects::parsers, serialisable::SerialisableObject};
use hydrus_parse::downloaders::Downloaders;
use hydrus_store::{Store, settings};
use slint::ComponentHandle as _;
use std::rc::Rc;

fn recorded() -> serde_json::Value {
    hydrus_testkit::fixture_json("parser_preview.json")
}

/// Python words an undecodable hash's error its own way; everything else is compared whole.
fn same(shown: &str, recorded: &str, name: &str) {
    if name == "hash that is not one" {
        let head = "*** 1 RESULTS BEGIN ***\nsha256 hash: Could not decode a hash from zz: ";
        assert!(shown.starts_with(head), "{name}: {shown:?}");
        assert!(recorded.starts_with(head), "{name}: {recorded:?}");
        assert!(shown.ends_with("\n*** RESULTS END ***"), "{name}: {shown:?}");
    } else {
        assert_eq!(shown, recorded, "{name}");
    }
}

// leaf: parser-context
#[test]
fn the_content_parser_test_panel_words_every_kind_of_result_as_the_reference_does() {
    let _windows = headless::init();
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    for case in recorded()["content"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let object = SerialisableObject::from_tuple_str(&case["tuple"].to_string()).unwrap();
        let parser = parsers::content_parser(&object).unwrap();
        let slots = Slots::default();
        let window: ParserEditWindow = windows::open_content(
            &store,
            &parser,
            FormulaTestData::default(),
            &slots,
            &[0, 1, 2, 3, 4, 5, 6, 7, 8],
            Rc::new(|_| Ok(())),
        )
        .unwrap();
        window.set_document(case["document"].as_str().unwrap().into());
        window.invoke_action("test".into());
        same(
            window.get_preview().as_str(),
            case["results"].as_str().unwrap(),
            name,
        );
        window.invoke_action("cancel".into());
    }
}

// leaf: parser-context
#[test]
fn the_page_parser_test_panel_words_posts_and_vetoes_as_the_reference_does() {
    let _windows = headless::init();
    for case in recorded()["page"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path()).unwrap();
        let object = SerialisableObject::from_tuple_str(&case["tuple"].to_string()).unwrap();
        let definitions = Downloaders {
            parsers: vec![parsers::page_parser(&object).unwrap()],
            ..Downloaders::default()
        };
        store
            .write_and_refresh(move |ctx| settings::set(ctx.conn(), &definitions))
            .unwrap();
        let slots = Slots::default();
        let list = windows::open(&store, &slots, false).unwrap();
        list.invoke_row_clicked(0, false, false);
        list.invoke_action("edit".into());
        let page: ParserEditWindow = slots.page.borrow().as_ref().unwrap().clone_strong();
        page.set_document(case["document"].as_str().unwrap().into());
        page.invoke_action("test".into());
        assert_eq!(
            page.get_preview().as_str(),
            case["results"].as_str().unwrap(),
            "{name}"
        );
        page.invoke_action("cancel".into());
    }
}
