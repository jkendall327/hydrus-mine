//! A string processor's tag filter step keeps its example tag, and one
//! stored before it had one still loads (with the reference's example).

use hydrus_core::tag_filter::{FilterRule, TagFilter};
use hydrus_core::url::strings::{ProcessingStep, StringProcessor, TagFilterStep};

#[test]
fn a_tag_filter_step_stored_without_its_example_still_loads() {
    let filter = TagFilter::new().with_rule("series:", FilterRule::Blacklist);
    let old = serde_json::json!({ "steps": [{ "tag_filter": filter }] });
    let loaded: StringProcessor = serde_json::from_value(old).unwrap();
    assert_eq!(
        loaded.steps,
        [ProcessingStep::TagFilter(TagFilterStep::new(
            filter.clone()
        ))]
    );
    assert_eq!(
        loaded.steps[0].describe(false, true),
        "TAG FILTER: allowing all tags except 'series' tags, such as blue eyes"
    );
    // and one with its example comes back as it went
    let step = ProcessingStep::TagFilter(TagFilterStep {
        filter,
        example: "series:x".into(),
    });
    let processor = StringProcessor { steps: vec![step] };
    let again: StringProcessor =
        serde_json::from_value(serde_json::to_value(&processor).unwrap()).unwrap();
    assert_eq!(again, processor);
}
