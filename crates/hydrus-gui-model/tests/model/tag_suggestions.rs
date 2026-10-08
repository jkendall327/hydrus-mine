use hydrus_gui_model::{
    manage_tags::ManageTags,
    options::{Editor, Settings},
    tag_suggestions,
};
use hydrus_store::{
    Store,
    settings::{self, TagAutocompleteTabs, TagSuggestionSettings},
};
use std::collections::BTreeMap;

fn control(editor: &Editor, label: &str) -> usize {
    editor
        .rows()
        .iter()
        .position(
            |r| matches!(r,hydrus_gui_model::options::Row::Opt{option,..} if option.label==label),
        )
        .unwrap()
}

// leaf: audit-options-nested-tag-suggestions-favourites
#[test]
fn layout_and_default_choices_replay_all_recorded_available_pages_without_eager_writes() {
    let f = hydrus_testkit::fixture_json("tag_suggestions.json");
    let (_dirs, store) =
        super::options_dialog::fixture_store(&hydrus_testkit::fixture_json("options_dialog.json"));
    for case in f["cases"].as_array().unwrap() {
        let before = store.read(Settings::load).unwrap();
        let mut editor = Editor::new(before.clone());
        let page = editor
            .page_names()
            .iter()
            .position(|name| *name == "tag suggestions")
            .unwrap();
        editor.show_page(page);
        let width = control(&editor, "Width of suggested tags columns: ");
        editor.number(width, case["minimum_width"].as_i64().unwrap());
        let layout = control(&editor, "Column layout: ");
        editor.choose(layout, usize::from(case["layout"] == "columns"));
        let index = ["favourites", "related", "file_lookup_scripts", "recent"]
            .iter()
            .position(|key| Some(*key) == case["default"].as_str())
            .unwrap();
        let default = control(&editor, "Default notebook page: ");
        editor.choose(default, index);
        let (after, original, problems) = editor.applied();
        assert!(problems.is_empty());
        assert_eq!(
            store.read(settings::get::<TagSuggestionSettings>).unwrap(),
            original.tag_suggestions,
            "staged edits/cancel never write"
        );
        assert_eq!(
            serde_json::json!(after.tag_suggestions.default_page),
            case["default"]
        );
        assert_eq!(after.tag_suggestions.columns, case["layout"] == "columns");
        let original = original.clone();
        store
            .write(move |ctx| after.save(ctx.conn(), &original))
            .unwrap();
        let reopened = Store::open(store.dir()).unwrap();
        assert_eq!(
            serde_json::json!(
                reopened
                    .read(settings::get::<TagSuggestionSettings>)
                    .unwrap()
                    .default_page
            ),
            case["default"]
        );
    }
}

#[test]
fn most_used_options_merge_per_service_and_reach_filtered_add_only_media() {
    let f = hydrus_testkit::fixture_json("tag_suggestions.json");
    let (_dirs, store) =
        super::options_dialog::fixture_store(&hydrus_testkit::fixture_json("options_dialog.json"));
    let snapshot = store.snapshot();
    let mine = snapshot.services.by_name("my tags").unwrap();
    let other = snapshot
        .services
        .by_name("second tags")
        .unwrap()
        .key
        .to_hex();
    let key = mine.key.to_hex();
    let service = mine.id;
    let tags: Vec<String> = serde_json::from_value(f["edited"]["tags"].clone()).unwrap();
    let before = store.read(Settings::load).unwrap();
    let mut editor = Editor::new(before);
    editor.set_most_used_tags(BTreeMap::from([(key.clone(), tags)]));
    assert_eq!(
        store
            .read(settings::get::<TagAutocompleteTabs>)
            .unwrap()
            .most_used
            .get(&key),
        None
    );
    let preserved = other.clone();
    store
        .write(move |ctx| {
            let mut live: TagAutocompleteTabs = settings::get(ctx.conn())?;
            live.children_limit = Some(11);
            live.most_used
                .insert(preserved, vec!["parity:concurrent".into()]);
            settings::set(ctx.conn(), &live)
        })
        .unwrap();
    let (after, before, problems) = editor.applied();
    assert!(problems.is_empty());
    let before = before.clone();
    store
        .write(move |ctx| after.save(ctx.conn(), &before))
        .unwrap();
    let saved: TagAutocompleteTabs = store.read(settings::get).unwrap();
    assert_eq!(saved.children_limit, Some(11));
    assert_eq!(saved.most_used[&other], ["parity:concurrent"]);
    let files: Vec<hydrus_core::HashId> = store
        .read(|conn| {
            Ok(conn
                .prepare("SELECT hash_id FROM files ORDER BY hash_id LIMIT 2")?
                .query_map([], |row| row.get(0))?
                .collect::<rusqlite::Result<_>>()?)
        })
        .unwrap();
    let mut model = ManageTags::new(store.clone(), files.clone()).unwrap();
    let index = model
        .service_names()
        .iter()
        .position(|s| s == "my tags")
        .unwrap();
    model.choose_service(index).unwrap();
    model
        .add_side_suggestions(&["parity:present".into()])
        .unwrap();
    model.apply().unwrap();
    assert_eq!(
        serde_json::json!(model.side_suggestions(false)),
        f["cases"][0]["favourites"]
    );
    let activated: Vec<String> =
        serde_json::from_value(f["cases"][0]["activated"][0]["tags"].clone()).unwrap();
    model.add_side_suggestions(&activated).unwrap();
    model.add_side_suggestions(&activated).unwrap();
    assert!(
        !model.side_suggestions(false).contains(&activated[0]),
        "repeated suggestion activation never removes an existing tag"
    );
    model.apply().unwrap();
    let mut reopened = ManageTags::new(Store::open(store.dir()).unwrap(), files).unwrap();
    reopened.choose_service(index).unwrap();
    let presentation: hydrus_core::tag_presentation::TagPresentation =
        reopened.store().read(settings::get).unwrap();
    assert_eq!(
        reopened
            .rows()
            .into_iter()
            .find(|(tag, _)| tag == &activated[0])
            .map(|(_, label)| label),
        Some(format!("{} (2)", presentation.render(&activated[0])))
    );
    assert!(
        tag_suggestions::recent(&store, service, 20)
            .unwrap()
            .contains(&activated[0])
    );
    assert_eq!(
        store
            .read(settings::get::<TagSuggestionSettings>)
            .unwrap()
            .default_page,
        "related"
    );
}

#[test]
fn broadcast_keeps_selected_tag_identity_but_switching_services_retires_selection() {
    let key = hydrus_core::ServiceKey::new(b"synthetic tags".to_vec());
    let mut list = tag_suggestions::List::default();
    list.update(
        Some(key.clone()),
        vec!["parity:new2".into(), "parity:new10".into()],
    );
    list.click(1, false, false);
    list.update(
        Some(key),
        vec![
            "parity:inserted".into(),
            "parity:new2".into(),
            "parity:new10".into(),
        ],
    );
    assert_eq!(list.selected(), ["parity:new10"]);
    list.update(
        Some(hydrus_core::ServiceKey::new(b"other tags".to_vec())),
        list.tags.clone(),
    );
    assert!(list.selected().is_empty());
}

#[test]
fn the_recent_tab_s_count_is_the_noneable_the_reference_recorded() {
    use hydrus_gui_model::options::{Kind, Row};
    let recorded = hydrus_testkit::fixture_json("options_dialog.json");
    let page = recorded["pages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|page| page["page"] == "tag suggestions")
        .unwrap();
    let tabs = &page["items"][0]["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item.get("tabs").is_some())
        .unwrap()["tabs"];
    let recent = tabs
        .as_array()
        .unwrap()
        .iter()
        .find(|tab| tab["tab"] == "recent")
        .unwrap();
    // (the first is the count; the second is for quick entry dialogs, which
    // hydrus-rs has no equivalent of)
    let theirs = &recent["items"][1];
    let (_dirs, store) = super::options_dialog::fixture_store(&recorded);
    let mut editor = Editor::new(store.read(Settings::load).unwrap());
    let at = editor
        .page_names()
        .iter()
        .position(|name| *name == "tag suggestions")
        .unwrap();
    editor.show_page(at);
    let ours = editor
        .rows()
        .into_iter()
        .find_map(|row| match row {
            Row::Opt { option, .. } if option.label == "number of recent tags to show: " => {
                Some(option.kind.clone())
            }
            _ => None,
        })
        .unwrap();
    let Kind::Noneable {
        none_phrase,
        default,
        min,
        max,
        ..
    } = ours
    else {
        panic!("{ours:?}");
    };
    assert_eq!(theirs["none_phrase"], none_phrase);
    assert_eq!(theirs["noneable"], default);
    assert_eq!(
        (theirs["min"].as_i64(), theirs["max"].as_i64()),
        (Some(min), Some(max))
    );
}
