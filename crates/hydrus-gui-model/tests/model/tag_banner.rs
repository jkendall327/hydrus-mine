//! Real Qt child states replayed in a detached banner editor.
use hydrus_core::{
    tag_presentation::TagPresentation,
    tag_summary::{NamespaceInfo, TagSummaries, TagSummaryGenerator},
};
use hydrus_gui_model::tag_banner::Editor;
fn value(json: &serde_json::Value) -> TagSummaryGenerator {
    serde_json::from_value(json.clone()).unwrap()
}
// leaf: audit-options-nested-tag-banner-appearance
// leaf: audit-options-nested-tag-banner-namespaces
// leaf: audit-options-nested-tag-banner-preview
// leaf: audit-options-tag-presentation-tag-banners-on-media-viewer-top
// leaf: audit-options-tag-presentation-tag-banners-on-thumbnail-bottom-right
// leaf: audit-options-tag-presentation-tag-banners-on-thumbnail-top
#[test]
fn detached_banner_drafts_match_actual_qt_queue_and_live_preview() {
    let fixture = hydrus_testkit::fixture_json("tag_banner_editors.json");
    let defaults = TagSummaries::default();
    for (key, mut default) in [
        ("thumbnail_top", defaults.thumbnail_top),
        ("thumbnail_bottom_right", defaults.thumbnail_bottom_right),
        ("media_viewer_top", defaults.media_viewer_top),
    ] {
        default.example_tags.sort();
        assert_eq!(value(&fixture["initial"][key]), default);
    }
    let mut editor = None;
    for (index, event) in fixture["events"].as_array().unwrap().iter().enumerate() {
        if index % 10 == 0 {
            let route = &fixture["routes"][index / 10];
            let before = value(&route["before"]);
            editor = Some(Editor::new(&before, TagPresentation::default()));
        }
        let editor = editor.as_mut().unwrap();
        match event["action"].as_str().unwrap() {
            "appearance/examples" => {
                editor.background = [12, 34, 56, 78];
                editor.text = [210, 180, 140, 120];
                editor.separator = " / ".into();
                editor.examples=" CREATOR:Alpha \ncreator:alpha\ntitle:Beta\npage:10\npage:2\npage:3\npage:alpha\n blue_eyes \n".into();
            }
            "hide" => editor.show = false,
            "show" => editor.show = true,
            "add namespace" => {
                editor.put(
                    None,
                    NamespaceInfo {
                        namespace: "page".into(),
                        prefix: "p=".into(),
                        separator: "..".into(),
                    },
                );
                editor.click(editor.rows().len() - 1, false, false);
            }
            "move up" => editor.move_selected(false),
            "move down" => editor.move_selected(true),
            "edit namespace" => {
                let id = editor.first_selected().unwrap().0;
                editor.put(
                    Some(id),
                    NamespaceInfo {
                        namespace: String::new(),
                        prefix: "plain=".into(),
                        separator: " + ".into(),
                    },
                );
            }
            "cancel namespace" | "cancel delete" => {}
            "delete" => editor.delete(&editor.selected()),
            other => panic!("{other}"),
        }
        let actual = editor.value();
        assert_eq!(actual, value(&event["value"]), "{index}: {event}");
        assert_eq!(editor.preview(), event["preview"], "{index}");
        assert_eq!(
            actual.summary(
                [
                    "creator:alpha",
                    "title:beta",
                    "page:2",
                    "page:10",
                    "page:3",
                    "page:alpha",
                    "blue_eyes",
                    "page:１２",
                    "page:１３",
                    "page:１４"
                ],
                str::to_owned
            ),
            event["summary"],
            "{index}"
        );
        assert_eq!(
            actual.summary(["page:2", "page:10", "page:3"], str::to_owned),
            event["numeric"],
            "{index}"
        );
        if index == 9 {
            assert_eq!(
                value(&fixture["routes"][0]["after"]),
                value(&fixture["routes"][0]["before"]),
                "child Cancel leaves owner untouched"
            );
        }
    }
}

// leaf: audit-options-nested-tag-banner-appearance
// leaf: audit-options-tag-presentation-tag-banners-on-media-viewer-top
// leaf: audit-options-tag-presentation-tag-banners-on-thumbnail-bottom-right
// leaf: audit-options-tag-presentation-tag-banners-on-thumbnail-top
#[test]
fn option_banner_children_stage_by_original_target_and_save_only_with_parent() {
    use hydrus_gui_model::{options, tag_banner::Target};
    let dir = tempfile::tempdir().unwrap();
    let store = hydrus_store::Store::open(dir.path()).unwrap();
    let before = store.read(options::Settings::load).unwrap();
    let mut owner = options::Editor::new(before.clone());
    let page = owner
        .page_names()
        .iter()
        .position(|name| *name == "tag presentation")
        .unwrap();
    owner.show_page(page);
    let row = owner.rows().iter().position(|row| matches!(row, options::Row::Opt {option,..} if matches!(option.kind, options::Kind::TagBanner(Target::ThumbnailTop)))).unwrap();
    let (target, initial) = owner.edited_banner(row).unwrap();
    assert_eq!(initial, before.tag_summaries.thumbnail_top);
    let mut child = Editor::new(&initial, before.tag_presentation.clone());
    child.show = false;
    child.separator = " | ".into();
    let draft = child.value();
    owner.show_page(0);
    owner.set_banner(target, draft.clone());
    assert_eq!(store.read(options::Settings::load).unwrap(), before);
    let (after, saved_before, problems) = owner.applied();
    assert!(problems.is_empty(), "{problems:?}");
    assert_eq!(after.tag_summaries.thumbnail_top, draft);
    assert_eq!(
        after.tag_summaries.thumbnail_bottom_right,
        before.tag_summaries.thumbnail_bottom_right
    );
    let saved_before = saved_before.clone();
    store
        .write(move |ctx| after.save(ctx.conn(), &saved_before))
        .unwrap();
    assert_eq!(
        store
            .read(options::Settings::load)
            .unwrap()
            .tag_summaries
            .thumbnail_top,
        draft
    );
}

// leaf: audit-options-nested-tag-banner-appearance
// leaf: audit-options-nested-tag-banner-namespaces
// leaf: audit-options-nested-tag-banner-preview
#[test]
fn unicode_decimal_chunk_order_and_live_preview_match_actual_qt() {
    let fixture = hydrus_testkit::fixture_json("tag_banner_sort_boundaries.json");
    for case in fixture["cases"].as_array().unwrap() {
        let subtags: Vec<String> = serde_json::from_value(case["subtags"].clone()).unwrap();
        let mut sorted = subtags.clone();
        hydrus_core::sort::human_sort(&mut sorted);
        assert_eq!(serde_json::json!(sorted), case["sorted"], "{case}");
        let mut generator = TagSummaryGenerator::thumbnail_top();
        generator.namespace_info = vec![NamespaceInfo {
            namespace: "page".into(),
            prefix: "p=".into(),
            separator: "..".into(),
        }];
        generator.example_tags = subtags.iter().map(|tag| format!("page:{tag}")).collect();
        let editor = Editor::new(&generator, TagPresentation::default());
        assert_eq!(
            editor.preview(),
            case["preview"].as_str().unwrap(),
            "{case}"
        );
        assert_eq!(
            generator.summary(
                generator.example_tags.iter().map(String::as_str),
                str::to_owned
            ),
            case["summary"].as_str().unwrap(),
            "{case}"
        );
    }
    for case in fixture["key_equalities"].as_array().unwrap() {
        assert_eq!(
            hydrus_core::sort::human_sort_key(case["left"].as_str().unwrap())
                == hydrus_core::sort::human_sort_key(case["right"].as_str().unwrap()),
            case["equal"].as_bool().unwrap(),
            "{case}"
        );
    }
}
