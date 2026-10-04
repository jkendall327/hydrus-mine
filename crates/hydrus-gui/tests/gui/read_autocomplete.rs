//! Real read-tabs query consumers, shared Options persistence and page restoration.
use hydrus_core::{
    ServiceKey, Sha256, Tag,
    search::context::{FileSearchContext, LocationContext, TagContext},
};
use hydrus_gui::{MainWindow, Pages, SearchPage, bind, headless};
use hydrus_store::{
    Store,
    content::tag_relations::{self, RelationAction, RelationUpdate},
    display::RelationKind,
    settings,
};
use serde_json::{Value, json};
use slint::{ComponentHandle as _, Model as _};
use std::sync::Arc;

fn seeded(fixture: &Value) -> ([tempfile::TempDir; 2], Arc<Store>, ServiceKey) {
    let (dirs, store) = super::subscriptions::store();
    let service = store
        .snapshot()
        .services
        .by_name("my tags")
        .unwrap()
        .clone();
    let id = service.id;
    let corpus = fixture["corpus"].clone();
    store
        .write_content(move |w| {
            for row in corpus.as_array().unwrap() {
                let tag = hydrus_store::master::intern_tag(
                    w.conn(),
                    &Tag::new(row["tag"].as_str().unwrap()).unwrap(),
                )?;
                let hashes = row["hashes"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|hash| {
                        let hash: Sha256 = hash.as_str().unwrap().parse().unwrap();
                        hydrus_store::master::hash_id(w.conn(), &hash).map(Option::unwrap)
                    })
                    .collect::<hydrus_store::Result<Vec<_>>>()?;
                w.update_mappings(id, &hydrus_store::content::MappingAction::Add, tag, &hashes)?;
            }
            Ok(())
        })
        .unwrap();
    for (kind, field) in [
        (RelationKind::Siblings, "siblings"),
        (RelationKind::Parents, "parents"),
    ] {
        tag_relations::apply(
            &store,
            kind,
            fixture[field]
                .as_array()
                .unwrap()
                .iter()
                .map(|pair| RelationUpdate {
                    service: id,
                    left: Tag::new(pair[0].as_str().unwrap()).unwrap(),
                    right: Tag::new(pair[1].as_str().unwrap()).unwrap(),
                    action: RelationAction::Add,
                })
                .collect(),
        )
        .unwrap();
    }
    let favourites: Vec<String> =
        serde_json::from_value(fixture["events"][0]["favourites"].clone()).unwrap();
    store
        .write(move |ctx| settings::set(ctx.conn(), &settings::FavouriteTags(favourites)))
        .unwrap();
    (dirs, store, service.key.clone())
}
fn options_cap(ui: &MainWindow, bound: &hydrus_gui::Bound, cap: i32, apply: bool) {
    ui.invoke_menu_title_pressed(0, 20.0, 22.0);
    let pane = ui.get_menu_panes().row_data(0).unwrap();
    let index = pane
        .lines
        .iter()
        .position(|row| row.label == "options\u{2026}")
        .unwrap();
    ui.invoke_menu_line_clicked(0, i32::try_from(index).unwrap(), 0.0, 0.0, 0.0);
    let options = bound.options.borrow().as_ref().unwrap().clone_strong();
    let page = options
        .get_pages()
        .iter()
        .position(|row| row.text == "tag autocomplete tabs")
        .unwrap();
    options.invoke_page_chosen(i32::try_from(page).unwrap());
    let row = options
        .get_rows()
        .iter()
        .position(|row| row.label == "How many tags to show in the children tab: ")
        .unwrap();
    options.invoke_none_toggled(i32::try_from(row).unwrap(), false);
    options.invoke_number_edited(i32::try_from(row).unwrap(), cap);
    if apply {
        options.invoke_apply();
    } else {
        options.invoke_cancel();
    }
    options.invoke_number_edited(i32::try_from(row).unwrap(), cap + 1); // Retired controls do not alter saved values.
    options.invoke_apply();
}
#[test]
fn real_page_tabs_apply_caps_keep_zero_count_descendants_and_restore_context() {
    let fixture = hydrus_testkit::fixture_json("read_tag_tabs.json");
    let (_dirs, store, key) = seeded(&fixture);
    let windows = headless::init();
    store
        .write(|ctx| {
            let mut prefs: settings::FileSearchSettings = settings::get(ctx.conn())?;
            prefs.float_autocomplete = true;
            settings::set(ctx.conn(), &prefs)
        })
        .unwrap();
    let ui = MainWindow::new().unwrap();
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
    ui.set_search_focus_requests(ui.get_search_focus_requests() + 1);
    for event in fixture["events"].as_array().unwrap() {
        match event["action"].as_str().unwrap() {
            "favourites" => {
                ui.invoke_autocomplete_tab_chosen(1);
            }
            "favourites_with_text" => {
                ui.invoke_search_edited("draft content".into());
                ui.invoke_autocomplete_tab_chosen(1);
            }
            "choose_favourite" => {
                let at = bound
                    .current
                    .borrow()
                    .borrow()
                    .autocomplete()
                    .suggestions()
                    .iter()
                    .position(|s| s.predicate == "parity:tabs root")
                    .unwrap();
                ui.invoke_suggestion_chosen(i32::try_from(at).unwrap());
                assert!(bound.current.borrow().borrow().results().is_empty()); // Reference paused search.
                ui.invoke_refresh_page(); // F5 resumes and performs the actual query.
                assert_eq!(
                    bound.current.borrow().borrow().results().len(),
                    usize::try_from(event["query_count"].as_u64().unwrap()).unwrap()
                );
                ui.invoke_flip_synchronised();
            }
            "children" => {
                let limit: Option<usize> = serde_json::from_value(event["limit"].clone()).unwrap();
                store
                    .write(move |ctx| {
                        let mut prefs: settings::TagAutocompleteTabs = settings::get(ctx.conn())?;
                        prefs.children_limit = limit;
                        settings::set(ctx.conn(), &prefs)
                    })
                    .unwrap();
                ui.invoke_autocomplete_tab_chosen(2);
            }
            "choose_child" => {
                let at = bound
                    .current
                    .borrow()
                    .borrow()
                    .autocomplete()
                    .suggestions()
                    .iter()
                    .position(|s| s.predicate == "parity:tabs alpha")
                    .unwrap();
                ui.invoke_suggestion_chosen(i32::try_from(at).unwrap());
            }
            "remove_child" => {
                let at = bound
                    .current
                    .borrow()
                    .borrow()
                    .predicates()
                    .iter()
                    .position(|p| p == "parity:tabs alpha")
                    .unwrap();
                ui.invoke_remove_predicate(i32::try_from(at).unwrap());
            }
            "type_returns_to_search" => {
                ui.invoke_search_edited("parity:tabs".into());
            }
            "children_without_search_tag_flags" => {
                ui.invoke_search_edited("".into());
                ui.invoke_autocomplete_tab_chosen(2);
                ui.invoke_include_flipped(0);
                ui.invoke_include_flipped(1);
            }
            "all_known_children" => {
                bound
                    .current
                    .borrow()
                    .borrow_mut()
                    .choose_tag_service(ServiceKey::new(
                        hydrus_core::service::builtin_keys::COMBINED_TAG,
                    ));
                ui.invoke_autocomplete_tab_chosen(2);
            }
            action => panic!("unrecorded tab action {action}"),
        }
        if event["action"] == "children" && event["limit"] == 0 {
            headless::render(&drawn, 1100, 750);
            assert!(
                ui.get_autocomplete_overlay_visible(),
                "an empty child tab must retain its tab picker"
            );
        }
        assert_eq!(json!(ui.get_autocomplete_tab()), event["tab"], "{event}");
        assert_eq!(ui.get_search_text(), event["text"].as_str().unwrap());
        assert_eq!(
            json!(bound.current.borrow().borrow().predicates()),
            event["predicates"],
            "{event}"
        );
        if event["pending_results"] != true {
            let rows: Vec<_> = bound
                .current
                .borrow()
                .borrow()
                .autocomplete()
                .suggestions()
                .iter()
                .map(|s| json!({"tag":s.predicate,"rows":[s.label]}))
                .collect();
            assert_eq!(json!(rows), event["rows"], "{event}");
            let labels: Vec<_> = event["rows"]
                .as_array()
                .unwrap()
                .iter()
                .flat_map(|r| {
                    r["rows"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|r| r.as_str().unwrap())
                })
                .collect();
            assert_eq!(
                ui.get_suggestions()
                    .iter()
                    .map(|r| r.text.to_string())
                    .collect::<Vec<_>>(),
                labels
            );
        }
    }
    // Shared options reach an already-open read consumer only after Apply, and persist on reopening.
    options_cap(&ui, &bound, 1, false);
    ui.invoke_search_fetch();
    assert_eq!(ui.get_suggestions().row_count(), 4);
    options_cap(&ui, &bound, 1, true);
    ui.invoke_search_fetch();
    assert_eq!(ui.get_suggestions().row_count(), 1);
    assert_eq!(
        store
            .read(settings::get::<settings::TagAutocompleteTabs>)
            .unwrap()
            .children_limit,
        Some(1)
    );
    let saved = bound.current.borrow().borrow().favourite_to_save().unwrap();
    let mut restored = SearchPage::restored(store.clone(), saved.search, false, None, Vec::new());
    restored.set_autocomplete_tab(hydrus_gui_model::write_autocomplete::Tab::Children);
    assert_eq!(restored.autocomplete().suggestions().len(), 1);
    restored.remove_predicate(0);
    assert!(restored.autocomplete().suggestions().is_empty());
    restored.set_autocomplete_tab(hydrus_gui_model::write_autocomplete::Tab::Favourites);
    assert_eq!(restored.autocomplete().suggestions().len(), 3);
    // A locked page cannot activate tab controls left over from its editable state.
    ui.invoke_lock_search();
    ui.invoke_answer(true);
    assert!(ui.get_search_locked());
    let before = bound.current.borrow().borrow().autocomplete().tab();
    ui.invoke_autocomplete_tab_chosen(1);
    assert_eq!(bound.current.borrow().borrow().autocomplete().tab(), before);
    drop(restored);
    let reopened = Store::open(store.dir()).unwrap();
    assert_eq!(
        reopened
            .read(settings::get::<settings::FavouriteTags>)
            .unwrap()
            .0
            .len(),
        3
    );
    assert_eq!(
        reopened
            .read(settings::get::<settings::TagAutocompleteTabs>)
            .unwrap()
            .children_limit,
        Some(1)
    );
}
