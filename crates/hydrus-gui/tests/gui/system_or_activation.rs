//! Replay real main/basic-OR system activation, history, recents and queries.
use hydrus_core::{ServiceKey, Sha256, Tag, search::recent::RecentPredicates};
use hydrus_gui::{Bound, MainWindow, Pages, SearchOrWindow, SearchPage, bind, headless};
use hydrus_search::{LocationContext, Predicate, TagContext, TextContext, predicate_text};
use hydrus_store::{Store, settings};
use serde_json::{Value, json};
use slint::{ComponentHandle as _, Model as _};

fn decode(value: &Value) -> Predicate {
    let stored =
        hydrus_legacy::serialisable::SerialisableObject::from_tuple_str(&value.to_string())
            .unwrap();
    hydrus_legacy::objects::predicates::predicate(&stored).unwrap()
}
fn typed(value: &Value) -> Vec<Predicate> {
    value.as_array().unwrap().iter().map(decode).collect()
}
fn seed(store: &Store, recording: &Value) {
    let service = store.snapshot().services.by_name("my tags").unwrap().id;
    let corpus = recording["corpus"].clone();
    store
        .write_content(move |writer| {
            for row in corpus.as_array().unwrap() {
                let tag = hydrus_store::master::intern_tag(
                    writer.conn(),
                    &Tag::new(row["tag"].as_str().unwrap()).unwrap(),
                )?;
                let hashes = row["hashes"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|hash| {
                        let hash: Sha256 = hash.as_str().unwrap().parse().unwrap();
                        hydrus_store::master::hash_id(writer.conn(), &hash).map(Option::unwrap)
                    })
                    .collect::<hydrus_store::Result<Vec<_>>>()?;
                writer.update_mappings(
                    service,
                    &hydrus_store::content::MappingAction::Add,
                    tag,
                    &hashes,
                )?;
            }
            Ok(())
        })
        .unwrap();
}
fn assert_recent(store: &Store, actual: &Value, case: &Value) {
    let recent: RecentPredicates = store.read(settings::get).unwrap();
    let recent: Vec<_> = recent
        .of_types(&[10])
        .into_iter()
        .map(Predicate::System)
        .collect();
    assert_eq!(recent, typed(&actual["recent"]), "{case}");
}
fn assert_history(bound: &Bound, actual: &Value, case: &Value) {
    let history = bound.pages.borrow().predicate_history();
    let labels = |predicates: &[Predicate]| {
        predicates
            .iter()
            .map(|p| predicate_text(p, &TextContext::default()))
            .collect::<Vec<_>>()
    };
    assert_eq!(json!(labels(&history.added)), actual["added"], "{case}");
    assert_eq!(json!(labels(&history.removed)), actual["removed"], "{case}");
}
fn assert_main(bound: &Bound, actual: &Value, case: &Value) {
    let current = bound.current.borrow();
    let page = current.borrow();
    assert_eq!(
        page.favourite_to_save().unwrap().search.predicates,
        typed(&actual["predicates"]),
        "{case}"
    );
    let draft = actual["draft"]
        .as_array()
        .map(|values| values.iter().map(decode).collect::<Vec<_>>());
    assert_eq!(page.or_terms(), draft.as_deref(), "{case}");
    assert_eq!(
        page.autocomplete().text(),
        actual["text"].as_str().unwrap(),
        "{case}"
    );
}
fn assert_basic(child: &SearchOrWindow, actual: &Value, case: &Value) {
    let mut labels: Vec<_> = child
        .get_predicates()
        .iter()
        .map(|p| p.to_string())
        .collect();
    labels.sort();
    assert_eq!(json!(labels), actual["labels"], "{case}");
    assert_eq!(child.get_or_active(), !actual["draft"].is_null(), "{case}");
    if !actual["draft"].is_null() {
        let label = predicate_text(
            &Predicate::Or(typed(&actual["draft"])),
            &TextContext::default(),
        );
        assert_eq!(
            child.get_suggestions().row_data(0).unwrap().text,
            label,
            "{case}"
        );
    }
    assert_eq!(
        child.get_input(),
        actual["text"].as_str().unwrap(),
        "{case}"
    );
}
#[test]
fn actual_main_and_basic_system_acceptance_preserves_shift_and_reaches_consumers() {
    let recording = hydrus_testkit::fixture_json("system_or_activation.json");
    let (_dirs, store) = super::subscriptions::store();
    seed(&store, &recording);
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::single(SearchPage::new(store.clone())));
    ui.show().unwrap();
    let service = store
        .snapshot()
        .services
        .by_name("my tags")
        .unwrap()
        .key
        .clone();
    for (index, case) in recording["cases"].as_array().unwrap().iter().enumerate() {
        store
            .write(|writer| settings::set(writer.conn(), &RecentPredicates::default()))
            .unwrap();
        bound.pages.borrow_mut().open_search_with_context(
            LocationContext::single(ServiceKey::new(
                hydrus_core::service::builtin_keys::MY_FILES,
            )),
            Some(TagContext::new(service.clone(), true, true)),
            Vec::new(),
            &format!("System OR {index}"),
        );
        let (depth, at) = bound.pages.borrow().shown_position();
        ui.invoke_tab_chosen(i32::try_from(depth).unwrap(), i32::try_from(at).unwrap());
        bound.current.borrow().borrow_mut().set_synchronised(false);
        bound.pages.borrow_mut().clear_predicate_history();
        let basic = if case["owner"] == "basic" {
            ui.invoke_search_or_action(3);
            Some(bound.search_or.borrow().as_ref().unwrap().clone_strong())
        } else {
            None
        };
        if let Some(child) = &basic {
            if case["seed_draft"] == true {
                child.invoke_edited(recording["tag"].as_str().unwrap().into());
                child.invoke_enter(true);
            }
            child.invoke_edited("".into());
            assert_basic(child, &case["before"], case);
            let at = child
                .get_suggestions()
                .iter()
                .position(|row| row.text == "system:filesize")
                .unwrap();
            if case["shift"] == true {
                child.invoke_move_highlight(i32::try_from(at).unwrap() - child.get_highlighted());
                child.invoke_enter(true);
            } else {
                child.invoke_chosen(i32::try_from(at).unwrap());
            }
        } else {
            if case["seed_draft"] == true {
                ui.invoke_search_edited(recording["tag"].as_str().unwrap().into());
                ui.invoke_search_or_action(0);
            }
            ui.invoke_search_edited("".into());
            assert_main(&bound, &case["before"], case);
            let at = ui
                .get_suggestions()
                .iter()
                .position(|row| row.text == "system:filesize")
                .unwrap();
            if case["shift"] == true {
                ui.invoke_move_highlight(i32::try_from(at).unwrap() - ui.get_highlighted());
                ui.invoke_search_or_action(0);
            } else {
                ui.invoke_suggestion_chosen(i32::try_from(at).unwrap());
            }
        }
        let system = if basic.is_some() {
            bound
                .search_or
                .system
                .borrow()
                .as_ref()
                .unwrap_or_else(|| {
                    panic!("basic OR system editor missing after activation: {case}")
                })
                .clone_strong()
        } else {
            bound
                .predicate_editor
                .borrow()
                .as_ref()
                .unwrap_or_else(|| panic!("main system editor missing after activation: {case}"))
                .clone_strong()
        };
        assert!(system.window().is_visible(), "{case}");
        system.invoke_chose(0, 1, 0); // <
        system.invoke_number_edited(0, 2, 7);
        system.invoke_chose(0, 3, 1); // KB
        if case["accepted"] == true {
            system.invoke_ok(0);
        } else {
            system.invoke_cancel();
        }
        assert!(!system.window().is_visible());
        assert_recent(&store, &case["after_system"], case);
        assert_history(&bound, &case["after_system"], case);
        if let Some(child) = &basic {
            assert_basic(child, &case["after_system"], case);
            assert!(!child.get_blocked());
            if case["outer_accepted"] == true {
                child.invoke_apply();
            } else {
                child.invoke_cancel();
            }
        } else {
            assert_main(&bound, &case["after_system"], case);
        }
        assert_main(&bound, &case["after_parent"], case);
        assert_recent(&store, &case["after_parent"], case);
        assert_history(&bound, &case["after_parent"], case);
        if let Some(count) = case["after_parent"]["query_count"].as_u64() {
            ui.invoke_refresh_page();
            assert_eq!(
                bound.current.borrow().borrow().results().len(),
                usize::try_from(count).unwrap(),
                "{case}"
            );
        } else {
            assert!(bound.current.borrow().borrow().results().is_empty());
        }
        // A retired system child cannot broadcast twice or change the recents
        // that legitimately survived its accepted value and outer cancellation.
        system.invoke_ok(0);
        assert_recent(&store, &case["after_parent"], case);
        assert_history(&bound, &case["after_parent"], case);
        let reopened = Store::open(store.dir()).unwrap();
        assert_recent(&reopened, &case["after_parent"], case);
    }
}
