//! Shared read/write selection and typed favourite persistence replay actual Qt input.
use hydrus_core::{
    ServiceKey, Tag,
    search::context::{LocationContext, TagContext},
};
use hydrus_gui_model::{
    autocomplete::Autocomplete,
    write_autocomplete::{Tab, WriteAutocomplete},
    write_tag_menu::{Action, Entry, favourite_entries},
};
use hydrus_store::{
    content::tag_relations::{self, RelationAction, RelationUpdate},
    display::RelationKind,
    settings,
};
use serde_json::json;

#[test]
fn read_and_write_selection_replay_the_same_qt_tabs_and_persist_only_accepted_edits() {
    let fixture = hydrus_testkit::fixture_json("autocomplete_tab_selection.json");
    let (_dir, store) = super::write_autocomplete::seeded(&fixture);
    let snapshot = store.snapshot();
    let service = snapshot.services.by_name("my tags").unwrap();
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
                    service: service.id,
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
    let location = LocationContext::single(ServiceKey::new(
        hydrus_core::service::builtin_keys::MY_FILES,
    ));
    let context = TagContext::new(service.key.clone(), true, true);
    let mut input = Autocomplete::new(store.clone());
    input.set_context(&location, &context);
    for event in fixture["events"].as_array().unwrap() {
        let action = event["action"].as_str().unwrap();
        match action {
            "favourites" | "return_favourites" => input.set_tab(Tab::Favourites),
            "children" | "return_children" => input.set_tab(Tab::Children),
            "hit" => input.click(
                event["index"].as_u64().unwrap().try_into().unwrap(),
                event["ctrl"].as_bool().unwrap(),
                event["shift"].as_bool().unwrap(),
            ),
            "select_all" => input.select_all(),
            "deselect" => {
                assert!(input.deselect());
            }
            "remove_favourite" | "add_child_favourite" => {
                let tag = if action == "remove_favourite" {
                    "parity:tabs root"
                } else {
                    "parity:tabs beta"
                };
                let entries = favourite_entries(&store, tag);
                let Entry::Item(label, choice) = &entries[0] else {
                    panic!("favourite action expected")
                };
                assert_eq!(label, event["label"].as_str().unwrap());
                if action == "remove_favourite" {
                    assert_eq!(json!([choice.question().unwrap()]), event["questions"]);
                }
                if action == "add_child_favourite" || event["yes"] == true {
                    choice.persist(&store).unwrap();
                }
                input.refresh_tab();
            }
            "reopen" | "write_activate" => continue,
            "activate_favourites"
            | "activate_children"
            | "clear_search"
            | "shift_activate_favourites"
            | "cancel_or" => {}
            other => panic!("unrecorded action {other}"),
        }
        let context_tags: Vec<String> =
            serde_json::from_value(event["predicates"].clone()).unwrap();
        input.set_context_tags(context_tags);
        let rows: Vec<_> = input
            .suggestions()
            .iter()
            .map(|row| row.predicate.clone())
            .collect();
        let selected: Vec<_> = input
            .selected_suggestions()
            .iter()
            .map(|row| row.predicate.clone())
            .collect();
        assert_eq!(json!(rows), event["rows"], "{event}");
        assert_eq!(json!(selected), event["selected"], "{event}");
    }
    for event in fixture["events"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|event| event["action"] == "write_activate")
    {
        let mut write =
            WriteAutocomplete::new(store.clone(), service.key.clone(), location.clone());
        write.set_context_tags(["parity:tabs root".into()]);
        write.set_tab(Tab::from_index(
            event["tab"].as_u64().unwrap().try_into().unwrap(),
        ));
        for (selected, tag) in event["selected"].as_array().unwrap().iter().enumerate() {
            let physical = write
                .rows()
                .iter()
                .position(|row| !row.parent_row && row.tag == tag.as_str().unwrap())
                .unwrap();
            write.click(physical, selected > 0, false);
        }
        assert_eq!(json!(write.selected_tags()), event["selected"]);
        assert_eq!(json!(write.chosen_tags(None)), event["entered"]);
    }
    // Persistence rereads inside the transaction; an answer retains another owner's edit.
    let remove = Action::Favourite {
        tag: "parity:tabs alpha".into(),
        service: None,
        remove: true,
        question: Some("Remove \"parity:tabs alpha\" from the favourites list?".into()),
    };
    store
        .write(|ctx| {
            let mut tags: settings::FavouriteTags = settings::get(ctx.conn())?;
            tags.0.push("parity:concurrent".into());
            settings::set(ctx.conn(), &tags)
        })
        .unwrap();
    remove.persist(&store).unwrap();
    let reopened = hydrus_store::Store::open(store.dir()).unwrap();
    let tags: settings::FavouriteTags = reopened.read(settings::get).unwrap();
    assert!(tags.0.contains(&"parity:concurrent".into()));
    assert!(!tags.0.contains(&"parity:tabs alpha".into()));
}
