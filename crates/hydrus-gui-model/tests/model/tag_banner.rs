//! Real Qt child states replayed in a detached banner editor.
use hydrus_core::{
    tag_presentation::TagPresentation,
    tag_summary::{NamespaceInfo, TagSummaries, TagSummaryGenerator},
};
use hydrus_gui_model::tag_banner::Editor;
fn value(json: &serde_json::Value) -> TagSummaryGenerator {
    serde_json::from_value(json.clone()).unwrap()
}
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
