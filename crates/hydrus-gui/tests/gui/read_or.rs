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

#[test]
fn native_or_keys_replay_drafts_and_commit_real_query_without_saving_cancelled_terms() {
    let fixture: Value =
        serde_json::from_str(include_str!("../../../../../oracle/fixtures/read_or.json")).unwrap();
    let (_dirs, store) = super::subscriptions::store();
    let service = store
        .snapshot()
        .services
        .by_name("my tags")
        .unwrap()
        .clone();
    let id = service.id;
    let corpus = fixture["corpus"].clone();
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
