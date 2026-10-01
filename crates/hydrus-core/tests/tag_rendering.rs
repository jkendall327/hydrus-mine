//! Tags as the user sees them, against the reference's `RenderTag`
//! (`oracle/dump_tag_rendering.py`): a corpus of awkward tags under several
//! presentation options.

use serde_json::Value as Json;

use hydrus_core::tag_presentation::TagPresentation;

fn fixture() -> Json {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../oracle/fixtures/tag_rendering.json"
    );
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

#[test]
fn tags_render_as_the_reference_renders_them() {
    let recorded = fixture();
    let mut problems = Vec::new();
    let mut checked = 0;
    for case in recorded["cases"].as_array().unwrap() {
        let options: TagPresentation = serde_json::from_value(case["options"].clone()).unwrap();
        for (tag, theirs) in case["rendered"].as_object().unwrap() {
            let ours = options.render(tag);
            if ours != theirs.as_str().unwrap() {
                problems.push(format!(
                    "{tag:?} with {}: ours {ours:?}, theirs {theirs}",
                    case["options"]
                ));
            }
            checked += 1;
        }
    }
    assert!(checked > 100, "{checked}");
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}
