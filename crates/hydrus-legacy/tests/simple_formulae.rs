//! The simple downloader's default formulae, and what each finds on some
//! pages, as the reference's (`oracle/fixtures/simple_downloader_formulae.json`,
//! from `oracle/dump_simple_downloader_formulae.py`).

use hydrus_legacy::objects::parsers::{default_simple_formulae, simple_formula};
use hydrus_legacy::serialisable::SerialisableObject;

#[test]
fn simple_formulae_find_what_the_references_find() {
    let recorded = hydrus_testkit::fixture_json("simple_downloader_formulae.json");
    let defaults = default_simple_formulae().unwrap();
    let names: Vec<&str> = defaults.iter().map(|f| f.name.as_str()).collect();
    assert_eq!(
        names,
        [
            "all files linked by images in page",
            "all images embedded in page"
        ]
    );
    // (the defaults are the recorded ones)
    for (default, tuple) in defaults
        .iter()
        .zip(recorded["formulae"].as_array().unwrap())
    {
        let decoded =
            simple_formula(&SerialisableObject::from_tuple_str(&tuple.to_string()).unwrap())
                .unwrap();
        assert_eq!(*default, decoded);
    }
    for page in recorded["pages"].as_array().unwrap() {
        let url = page["url"].as_str().unwrap();
        let text = page["text"].as_str().unwrap();
        for formula in &defaults {
            assert_eq!(
                serde_json::json!(formula.file_urls(url, text).unwrap()),
                page["found"][&formula.name],
                "{} on {url}",
                formula.name
            );
        }
    }
}
