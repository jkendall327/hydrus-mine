//! Real Qt namespace normalization, protected deletion and staged colour preference consumers.
use hydrus_core::{Tag, search::predicate::Predicate, tag_presentation::NamespaceColours};
use hydrus_gui_model::{
    namespace_colours,
    options::{Editor, Row, Settings},
};
use hydrus_store::{Store, settings};
use serde_json::{Value, json};

fn rows(editor: &namespace_colours::Editor) -> Value {
    json!(
        editor
            .rows()
            .iter()
            .map(|row| json!({"namespace":row.namespace,"label":row.label,"rgb":row.rgb}))
            .collect::<Vec<_>>()
    )
}
#[test]
fn qt_namespace_add_normalization_protected_mixed_delete_and_options_cancel_reopen() {
    let fixture = hydrus_testkit::fixture_json("namespace_colour_controls.json");
    let legacy = hydrus_testkit::legacy_fixture("basic");
    let dir = tempfile::tempdir().unwrap();
    hydrus_store::import::import_legacy(
        legacy.path(),
        &dir.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store = Store::open(dir.path()).unwrap();
    let before = store.read(Settings::load).unwrap();
    let mut list = namespace_colours::Editor::new(before.namespace_colours.colours.clone());
    assert_eq!(rows(&list), fixture["initial"]);
    for event in fixture["events"].as_array().unwrap() {
        if event["action"] == "add" {
            assert_eq!(event["questions"], json!(["Enter the namespace."]));
            if event["cancel"] != true {
                let result = list.add(event["input"].as_str().unwrap(), [12, 34, 56]);
                let warnings: Vec<_> = result.err().into_iter().collect();
                assert_eq!(json!(warnings), event["warnings"]);
            }
        } else {
            if event["clear_selection"] == true {
                for (index, row) in list.rows().iter().enumerate() {
                    if row.selected {
                        list.click(index, true, false);
                    }
                }
            }
            for namespace in event["selected"].as_array().unwrap() {
                let index = list
                    .rows()
                    .iter()
                    .position(|row| json!(row.namespace) == *namespace)
                    .unwrap();
                if !list.rows()[index].selected {
                    list.click(index, true, false);
                }
            }
            assert_eq!(
                json!(list.removal_question().into_iter().collect::<Vec<_>>()),
                event["questions"]
            );
            if event["yes"] == true {
                list.remove_selected();
            }
        }
        assert_eq!(event["delete_enabled"], true);
        assert_eq!(rows(&list), event["rows"]);
        assert_eq!(store.read(Settings::load).unwrap(), before);
    }
    // Protected defaults alone cannot even ask a deletion question.
    let protected = list
        .rows()
        .iter()
        .position(|row| row.namespace.is_none())
        .unwrap();
    list.click(protected, false, false);
    assert!(list.removal_question().is_none());
    let cancelled = namespace_colours::Editor::new(
        store
            .read::<NamespaceColours>(settings::get)
            .unwrap()
            .colours,
    );
    assert_eq!(rows(&cancelled), fixture["cancelled_rows"]);
    let mut options = Editor::new(before.clone());
    let page = options
        .page_names()
        .iter()
        .position(|name| *name == "tag presentation")
        .unwrap();
    options.show_page(page);
    assert_eq!(
        options.applied().0.namespace_colours,
        before.namespace_colours,
        "untouched legacy None is not an edited empty string"
    );
    let mut accepted = namespace_colours::Editor::new(options.edited_namespace_colours());
    accepted.add(" Parity Artists::: ", [12, 34, 56]).unwrap();
    options.set_namespace_colours(accepted.values());
    let predicate = Predicate::Or(vec![
        Predicate::Tag {
            tag: Tag::new("parity artists:alpha").unwrap(),
            inclusive: true,
        },
        Predicate::Tag {
            tag: Tag::new("series:beta").unwrap(),
            inclusive: true,
        },
    ]);
    for case in fixture["colour_cases"].as_array().unwrap() {
        let index=options.rows().iter().position(|row|matches!(row,Row::Opt{option,..} if option.label==fixture["labels"][0].as_str().unwrap())).unwrap();
        options.text(index, case["input"].as_str().unwrap());
        let (after, original, problems) = options.applied();
        assert!(problems.is_empty());
        assert_eq!(json!(after.namespace_colours.or_connector), case["saved"]);
        assert_eq!(
            json!(after.namespace_colours.predicate(&predicate)),
            case["rows"][0][0][1]
        );
        assert_eq!(store.read(Settings::load).unwrap(), *original);
    }
    let (after, _, problems) = options.applied();
    assert!(problems.is_empty());
    let saved = after.namespace_colours.clone();
    store
        .write(move |ctx| after.save(ctx.conn(), &before))
        .unwrap();
    let reopened = Store::open(dir.path()).unwrap();
    assert_eq!(
        reopened.read::<NamespaceColours>(settings::get).unwrap(),
        saved
    );
    assert_eq!(
        rows(&namespace_colours::Editor::new(saved.colours)),
        fixture["reopened_rows"]
    );
}

#[test]
fn sorted_add_keeps_qt_positional_range_anchor_then_delete_resets_it() {
    let fixture = hydrus_testkit::fixture_json("namespace_colour_controls.json");
    for case in fixture["selection_cases"].as_array().unwrap() {
        let events = case["events"].as_array().unwrap();
        let colours = events[0]["rows"]
            .as_array()
            .unwrap()
            .iter()
            .map(|row| {
                (
                    serde_json::from_value(row["namespace"].clone()).unwrap(),
                    serde_json::from_value(row["rgb"].clone()).unwrap(),
                )
            })
            .collect();
        let mut list = namespace_colours::Editor::new(colours);
        for event in events {
            match event["action"].as_str().unwrap() {
                "initial" => {}
                "hit" => {
                    let index = list
                        .rows()
                        .iter()
                        .position(|row| json!(row.namespace) == event["namespace"])
                        .unwrap();
                    list.click(
                        index,
                        event["ctrl"].as_bool().unwrap(),
                        event["shift"].as_bool().unwrap(),
                    );
                }
                "add" => {
                    list.add(event["input"].as_str().unwrap(), [12, 34, 56])
                        .unwrap();
                }
                "delete" => {
                    list.remove_selected();
                }
                other => panic!("unrecorded selection action {other}"),
            }
            assert_eq!(rows(&list), event["rows"]);
            let selected: Vec<_> = list
                .rows()
                .into_iter()
                .filter(|row| row.selected)
                .map(|row| row.namespace)
                .collect();
            assert_eq!(json!(selected), event["selected"], "{event}");
        }
    }
}
