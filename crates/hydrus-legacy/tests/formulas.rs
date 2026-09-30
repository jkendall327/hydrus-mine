//! Parsing formulas against the reference: `oracle/fixtures/formulas.json`
//! holds random formulas (as the reference serialises them) run on random
//! HTML and JSON documents, and what the reference's `Parse` returned. Each
//! formula is decoded here and run by `hydrus-parse`.

use hydrus_legacy::objects::parsers::formula;
use hydrus_legacy::serialisable::SerialisableObject;
use hydrus_parse::ParsingContext;

#[test]
fn formulas_parse_like_the_reference() {
    let recorded = hydrus_testkit::fixture_json("formulas.json");
    let documents = recorded["documents"].as_array().unwrap();
    let cases = recorded["cases"].as_array().unwrap();
    let mut failures = Vec::new();
    let mut template_differences = 0;
    for (i, case) in cases.iter().enumerate() {
        let object = SerialisableObject::from_tuple_str(&case["formula"].to_string()).unwrap();
        let formula = match formula(&object) {
            Ok(f) => f,
            Err(e) => {
                failures.push(format!("case {i}: could not decode: {e}"));
                continue;
            }
        };
        let document = documents[case["document"].as_u64().unwrap() as usize]
            .as_str()
            .unwrap();
        let context: ParsingContext = serde_json::from_value(case["context"].clone()).unwrap();
        let collapse = case["collapse_newlines"].as_bool().unwrap();
        let got = formula.parse(&context, document, collapse);
        let ok = match (&got, case.get("result")) {
            (Ok(results), Some(expected)) => serde_json::json!(results) == *expected,
            // the reference raised a ParseException, or crashed
            (Err(_), None) => true,
            _ => false,
        };
        // html5lib's <template> handling predates the current HTML standard
        // (it drops table parts inside templates and moves templates out of
        // tables); we follow the standard, as browsers do (DIFFERENCES.md)
        if !ok && document.contains("<template") {
            template_differences += 1;
            continue;
        }
        if !ok {
            failures.push(format!(
                "case {i} ({}): expected {} got {:?}\n    formula {}\n    document {:?}",
                case["formula"][0],
                case.get("result").map_or_else(
                    || format!("error {}", case.get("error").or(case.get("crash")).unwrap()),
                    ToString::to_string
                ),
                got,
                case["formula"],
                &document[..document.len().min(300)],
            ));
        }
    }
    assert!(
        template_differences <= 8,
        "{template_differences} differences in documents with templates"
    );
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
