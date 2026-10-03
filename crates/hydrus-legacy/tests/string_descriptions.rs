//! String processing described as the reference describes it:
//! `oracle/fixtures/string_descriptions.json` (from
//! `oracle/dump_string_descriptions.py`) holds random processors of every
//! kind of step, with the reference's summary, whether it makes changes,
//! its processing strings, and each step's four `ToString`s.

use hydrus_legacy::objects::domain::string_processor;
use hydrus_legacy::serialisable::SerialisableObject;

#[test]
fn processors_and_their_steps_describe_themselves_as_the_reference_does() {
    let recorded = hydrus_testkit::fixture_json("string_descriptions.json");
    for (i, case) in recorded["cases"].as_array().unwrap().iter().enumerate() {
        let object = SerialisableObject::from_tuple_str(&case["processor"].to_string()).unwrap();
        let processor = string_processor(&object).unwrap();
        let at = format!("case {i}: {}", case["processor"]);
        assert_eq!(processor.summary(), case["summary"], "{at}");
        assert_eq!(processor.makes_changes(), case["makes_changes"], "{at}");
        let strings: Vec<String> =
            serde_json::from_value(case["processing_strings"].clone()).unwrap();
        assert_eq!(processor.processing_strings(), strings, "{at}");
        for (step, theirs) in processor
            .steps
            .iter()
            .zip(case["steps"].as_array().unwrap())
        {
            let ours: Vec<String> = [(false, false), (false, true), (true, false), (true, true)]
                .iter()
                .map(|&(simple, with_type)| step.describe(simple, with_type))
                .collect();
            let theirs: Vec<String> = serde_json::from_value(theirs.clone()).unwrap();
            assert_eq!(ours, theirs, "{at}");
        }
    }
}
