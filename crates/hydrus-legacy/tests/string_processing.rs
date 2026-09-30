//! String processors against the reference: `oracle/fixtures/string_processing.json`
//! holds random processors (sorters, tag filters and a few other steps, as
//! the reference serialises them), random lists of strings, and what the
//! reference's `ProcessStrings` returned.

use hydrus_core::sort::human_sort_key;
use hydrus_legacy::objects::domain::string_processor;
use hydrus_legacy::serialisable::SerialisableObject;

#[test]
fn processors_process_like_the_reference() {
    let recorded = hydrus_testkit::fixture_json("string_processing.json");
    let cases = recorded["cases"].as_array().unwrap();
    let mut failures = Vec::new();
    for (i, case) in cases.iter().enumerate() {
        let object = SerialisableObject::from_tuple_str(&case["processor"].to_string()).unwrap();
        let processor = string_processor(&object).unwrap();
        let strings: Vec<String> = serde_json::from_value(case["strings"].clone()).unwrap();
        let expected: Vec<String> = serde_json::from_value(case["result"].clone()).unwrap();
        match processor.process(strings.clone()) {
            Ok(got) if got == expected => {}
            // A tag filter's human sort leaves tags with equal keys (e.g.
            // "straße" and "strasse") in the order of a Python set, which
            // is randomised per process; any order of those is right.
            Ok(got) if same_but_for_ties(&got, &expected) => {}
            got => failures.push(format!(
                "case {i}: {strings:?} gave {got:?}, reference {expected:?}\n    processor {}",
                case["processor"]
            )),
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {} cases differ; first:\n{}",
        failures.len(),
        cases.len(),
        failures
            .iter()
            .take(8)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    );
}

fn same_but_for_ties(got: &[String], expected: &[String]) -> bool {
    let mut a = got.to_vec();
    let mut b = expected.to_vec();
    a.sort();
    b.sort();
    a == b
        && got
            .iter()
            .zip(expected)
            .all(|(g, e)| human_sort_key(g) == human_sort_key(e))
}
