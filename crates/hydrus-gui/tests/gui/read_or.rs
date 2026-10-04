use hydrus_core::{
    ServiceKey, Sha256, Tag,
    search::context::{FileSearchContext, LocationContext, TagContext},
};
use hydrus_gui::{MainWindow, Pages, SearchPage, bind, headless};
use hydrus_store::settings;
use serde_json::{Value, json};
use slint::{
    ComponentHandle as _, Model as _,
    platform::{Key, WindowEvent},
};

fn seed_corpus(store: &hydrus_store::Store, corpus: Value) {
    let id = store.snapshot().services.by_name("my tags").unwrap().id;
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
                    id,
                    &hydrus_store::content::MappingAction::Add,
                    tag,
                    &hashes,
                )?;
            }
            Ok(())
        })
        .unwrap();
}

#[test]
fn native_or_keys_replay_drafts_and_commit_real_query_without_saving_cancelled_terms() {
    let fixture: Value = hydrus_testkit::fixture_json("read_or.json");
    let (_dirs, store) = super::subscriptions::store();
    let service = store
        .snapshot()
        .services
        .by_name("my tags")
        .unwrap()
        .clone();
    seed_corpus(&store, fixture["corpus"].clone());
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let context = FileSearchContext {
        location: LocationContext::single(ServiceKey::new(
            hydrus_core::service::builtin_keys::MY_FILES,
        )),
        tags: TagContext::new(service.key.clone(), true, true),
        predicates: Vec::new(),
    };
    let page = SearchPage::restored(store.clone(), context, false, None, Vec::new());
    let bound = bind(&ui, Pages::single(page));
    let drawn = windows.get(0).unwrap();
    headless::render(&drawn, 1100, 750);
    ui.set_search_focus_requests(ui.get_search_focus_requests() + 1);
    for event in fixture["events"].as_array().unwrap() {
        match event["action"].as_str().unwrap() {
            "initial" => {}
            "broadcast" => {
                ui.invoke_search_edited(event["tag"].as_str().unwrap().into());
                if ui.get_or_active() && ui.get_suggestions().row_count() > 1 {
                    let selected = ui
                        .get_suggestions()
                        .row_data(usize::try_from(ui.get_highlighted()).unwrap())
                        .unwrap();
                    assert!(
                        selected.text.starts_with(event["tag"].as_str().unwrap()),
                        "OR construction must skip its draft when selecting the next term"
                    );
                }
                let shift = event["shift"].as_bool().unwrap();
                if shift {
                    ui.window().dispatch_event(WindowEvent::KeyPressed {
                        text: Key::Shift.into(),
                    });
                }
                ui.window().dispatch_event(WindowEvent::KeyPressed {
                    text: Key::Return.into(),
                });
                ui.window().dispatch_event(WindowEvent::KeyReleased {
                    text: Key::Return.into(),
                });
                if shift {
                    ui.window().dispatch_event(WindowEvent::KeyReleased {
                        text: Key::Shift.into(),
                    });
                }
            }
            "rewind" => {
                ui.invoke_search_edited("rewind draft".into());
                ui.invoke_search_or_action(1);
            }
            "escape" => {
                assert_eq!(
                    ui.invoke_search_or_escape(),
                    event["handled"].as_bool().unwrap()
                );
            }
            "cancel" => {
                ui.invoke_search_edited("cancel draft".into());
                ui.invoke_search_or_action(2);
            }
            "commit_draft" => {
                ui.invoke_suggestion_chosen(0);
            }
            unknown => panic!("unknown event {unknown}"),
        }
        let current = bound.current.borrow();
        let page = current.borrow();
        let terms = page.or_terms().map(|terms| {
            terms
                .iter()
                .map(|p| hydrus_search::predicate_text(p, &hydrus_search::TextContext::default()))
                .collect::<Vec<_>>()
        });
        assert_eq!(json!(terms), event["draft"], "{event}");
        let mut predicates = page.predicates();
        predicates.sort();
        assert_eq!(json!(predicates), event["predicates"], "{event}");
        assert_eq!(page.autocomplete().text(), event["text"].as_str().unwrap());
        assert_eq!(
            ui.get_or_active(),
            event["cancel_visible"].as_bool().unwrap(),
            "{event}"
        );
        assert_eq!(
            ui.get_or_rewind_visible(),
            event["rewind_visible"].as_bool().unwrap(),
            "{event}"
        );
        assert!(page.results().is_empty()); // Paused construction never queries.
        if let Some(label) = event["draft_label"].as_str() {
            assert_eq!(ui.get_suggestions().row_data(0).unwrap().text, label);
        }
    }
    ui.invoke_refresh_page();
    assert_eq!(
        bound.current.borrow().borrow().results().len(),
        usize::try_from(fixture["query_count"].as_u64().unwrap()).unwrap()
    );
    let saved = bound.current.borrow().borrow().favourite_to_save().unwrap();
    let expected = bound.current.borrow().borrow().predicates();
    // Real typed saved search, independent of the autocomplete-only draft.
    store
        .write(move |ctx| settings::set(ctx.conn(), &settings::FavouriteSearches(vec![saved])))
        .unwrap();
    let reopened = hydrus_store::Store::open(store.dir()).unwrap();
    let saved: settings::FavouriteSearches = reopened.read(settings::get).unwrap();
    let restored = SearchPage::restored(
        store.clone(),
        saved.0[0].search.clone(),
        false,
        None,
        Vec::new(),
    );
    assert_eq!(restored.predicates(), expected);
    assert!(restored.or_terms().is_none());
    ui.invoke_lock_search();
    ui.invoke_answer(true);
    let before = bound.current.borrow().borrow().predicates();
    ui.invoke_search_or_action(0);
    ui.invoke_search_or_action(1);
    ui.invoke_search_or_action(2);
    assert_eq!(bound.current.borrow().borrow().predicates(), before);
    assert!(!ui.invoke_search_or_escape());
}

#[test]
fn actual_basic_and_advanced_children_apply_cancel_reopen_and_reject_stale_owners() {
    let fixture: Value = hydrus_testkit::fixture_json("read_or_editors.json");
    let (_dirs, store) = super::subscriptions::store();
    store
        .write(|ctx| {
            settings::set(ctx.conn(), &settings::AdvancedMode(true))?;
            settings::set(
                ctx.conn(),
                &settings::FavouriteTags(vec!["parity:or alpha".into(), "parity:or beta".into()]),
            )
        })
        .unwrap();
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    seed_corpus(&store, fixture["corpus"].clone());
    let key = store
        .snapshot()
        .services
        .by_name("my tags")
        .unwrap()
        .key
        .clone();
    let context = FileSearchContext {
        location: LocationContext::single(ServiceKey::new(
            hydrus_core::service::builtin_keys::MY_FILES,
        )),
        tags: TagContext::new(key, true, true),
        predicates: Vec::new(),
    };
    let page = SearchPage::restored(store.clone(), context, false, None, Vec::new());
    let bound = bind(&ui, Pages::single(page));
    let drawn = windows.get(0).unwrap();
    headless::render(&drawn, 1100, 750);
    for dialog in fixture["dialogs"].as_array().unwrap() {
        ui.invoke_search_edited("parent draft".into());
        let advanced = dialog["kind"] == "advanced";
        ui.invoke_search_or_action(if advanced { 4 } else { 3 });
        let child = bound.search_or.borrow().as_ref().unwrap().clone_strong();
        assert!(ui.get_search_or_open());
        ui.invoke_search_edited("blocked parent".into());
        ui.invoke_search_accepted();
        assert_eq!(ui.get_search_text(), "parent draft");
        headless::render(&windows.get(windows.count() - 1).unwrap(), 720, 530);
        assert_eq!(child.get_window_title(), dialog["title"].as_str().unwrap());
        if advanced {
            assert_eq!(child.get_preview(), dialog["before"].as_str().unwrap());
            for case in fixture["cases"].as_array().unwrap() {
                child.invoke_edited(case["input"].as_str().unwrap().into());
                assert_eq!(
                    child.get_preview(),
                    case["preview"].as_str().unwrap(),
                    "{case}"
                );
                assert_eq!(child.get_valid(), case["valid"] == "HydrusValid", "{case}");
            }
            // Invalid input cannot close or mutate its caller.
            child.invoke_edited("(parity:or alpha".into());
            child.invoke_apply();
            assert!(child.window().is_visible());
            assert_eq!(
                child.get_error(),
                "Please enter a string that parses into a set of search rules."
            );
            child.invoke_edited("(parity:or alpha and parity:or beta) or parity:or gamma".into());
            let mut preview: Vec<_> = child.get_preview().split('\n').map(str::to_owned).collect();
            preview.sort();
            assert_eq!(json!(preview), dialog["after"]);
        } else {
            assert!(child.get_predicates().iter().next().is_none());
            child.invoke_tab_chosen(1);
            for tag in dialog["tags"]
                .as_array()
                .unwrap()
                .iter()
                .map(|tag| tag.as_str().unwrap())
            {
                let at = child
                    .get_suggestions()
                    .iter()
                    .position(|row| row.text == tag)
                    .unwrap();
                child.invoke_chosen(i32::try_from(at).unwrap());
            }
            assert_eq!(
                child
                    .get_predicates()
                    .iter()
                    .map(|p| p.to_string())
                    .collect::<Vec<_>>(),
                serde_json::from_value::<Vec<String>>(dialog["tags"].clone()).unwrap()
            );
        }
        if dialog["accepted"].as_bool().unwrap() {
            child.invoke_apply();
        } else {
            child.invoke_cancel();
        }
        assert!(!child.window().is_visible());
        assert!(bound.search_or.borrow().is_none());
        assert_eq!(
            ui.get_search_text(),
            dialog["parent_text"].as_str().unwrap()
        );
        let mut predicates = bound.current.borrow().borrow().predicates();
        predicates.sort();
        assert_eq!(json!(predicates), dialog["parent_predicates"], "{dialog}");
        ui.invoke_refresh_page();
        if let Some(count) = dialog["query_count"].as_u64() {
            assert_eq!(
                bound.current.borrow().borrow().results().len(),
                usize::try_from(count).unwrap(),
                "{dialog}"
            );
        } else {
            assert!(bound.current.borrow().borrow().results().is_empty());
        }
        ui.invoke_flip_synchronised();
        // Retained callbacks from a cancelled/applied child cannot touch parent.
        child.invoke_edited("parity:or stale".into());
        child.invoke_enter(false);
        child.invoke_apply();
        let mut after = bound.current.borrow().borrow().predicates();
        after.sort();
        assert_eq!(after, predicates);
    }
    // A recursive child cancels with its parent, and cannot apply afterwards.
    ui.invoke_search_or_action(3);
    let outer = bound.search_or.borrow().as_ref().unwrap().clone_strong();
    outer.invoke_or_action(4);
    let nested = hydrus_gui::search_or_window::last_opened().unwrap();
    assert!(outer.get_blocked());
    nested.invoke_edited("parity:or nested".into());
    let before = bound.current.borrow().borrow().predicates();
    outer.invoke_cancel();
    assert!(!nested.window().is_visible());
    nested.invoke_apply();
    assert_eq!(bound.current.borrow().borrow().predicates(), before);
    // A hidden caller invalidates an otherwise still-visible child.
    ui.invoke_search_or_action(4);
    let child = bound.search_or.borrow().as_ref().unwrap().clone_strong();
    child.invoke_edited("parity:or closed caller".into());
    ui.hide().unwrap();
    child.invoke_apply();
    assert!(!child.window().is_visible());
    assert_eq!(bound.current.borrow().borrow().predicates(), before);
    ui.show().unwrap();
    ui.invoke_search_or_action(4);
    let child = bound.search_or.borrow().as_ref().unwrap().clone_strong();
    assert_eq!(child.get_input(), "");
    assert_eq!(child.get_preview(), "");
    child.invoke_cancel();
}
