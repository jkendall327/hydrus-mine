//! A help button, replayed against `documentation.json`
//! (`oracle/record_documentation.py`): `OpenDocumentation` opens the page from
//! the local help where there is one, otherwise asks "You do not have a local
//! help!" and opens what the answer says.

use std::cell::RefCell;
use std::rc::Rc;

use slint::Model as _;

/// Puts the launcher and the help folder back, however the test ends.
struct Reset;

impl Drop for Reset {
    fn drop(&mut self) {
        hydrus_gui::documentation::set_help_dir(None);
        hydrus_gui::set_launcher(|_| {});
    }
}

/// Press `press` (a help button or menu entry for `page`) with the page in the
/// local help, and without it, answering each way the reference was.
pub fn replay(page: &str, press: &dyn Fn()) {
    let _reset = Reset;
    let recording = hydrus_testkit::fixture_json("documentation.json");
    let launched: Rc<RefCell<Vec<String>>> = Rc::default();
    hydrus_gui::set_launcher({
        let launched = launched.clone();
        move |target| launched.borrow_mut().push(target.to_owned())
    });
    let mut cases = 0;
    for case in recording["local"].as_array().unwrap() {
        if case["page"] != page {
            continue;
        }
        cases += 1;
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join(page.split('#').next().unwrap()),
            "<html></html>",
        )
        .unwrap();
        hydrus_gui::documentation::set_help_dir(Some(dir.path().to_owned()));
        launched.borrow_mut().clear();
        press();
        let root = dir.path().to_str().unwrap();
        let opened: Vec<String> = launched
            .borrow()
            .iter()
            .map(|u| u.replace(root, "<help>"))
            .collect();
        assert_eq!(serde_json::json!(opened), case["opened"], "{page} locally");
        assert!(
            hydrus_gui::documentation::last_question().is_none(),
            "{page} locally asks nothing"
        );
    }
    for case in recording["missing"].as_array().unwrap() {
        if case["page"] != page {
            continue;
        }
        cases += 1;
        let dir = tempfile::tempdir().unwrap();
        hydrus_gui::documentation::set_help_dir(Some(dir.path().to_owned()));
        launched.borrow_mut().clear();
        press();
        let question = hydrus_gui::documentation::last_question()
            .unwrap_or_else(|| panic!("{page} without a local help asks"));
        let asked = &case["asked"][0];
        assert_eq!(question.get_message(), asked["message"].as_str().unwrap());
        // (the title without the application's name, which the window adds)
        let title = question.get_composed_title();
        assert_eq!(
            title
                .strip_suffix(hydrus_gui::app_title::current().as_str())
                .unwrap_or(&title),
            asked["title"].as_str().unwrap()
        );
        assert_eq!(question.get_no_label(), asked["no"].as_str().unwrap());
        assert_eq!(
            serde_json::json!(
                question
                    .get_choices()
                    .iter()
                    .map(|c| c.to_string())
                    .collect::<Vec<_>>()
            ),
            asked["yes"]
        );
        assert!(
            launched.borrow().is_empty(),
            "nothing opens before the answer"
        );
        match case["answer"].as_u64() {
            Some(i) => question.invoke_chosen(i32::try_from(i).unwrap()),
            None => question.invoke_cancelled(),
        }
        assert_eq!(
            serde_json::json!(*launched.borrow()),
            case["opened"],
            "{page}, answer {}",
            case["answer"]
        );
        assert!(hydrus_gui::documentation::last_question().is_none());
    }
    assert_eq!(cases, 4, "{page}: one local and three missing recordings");
}
