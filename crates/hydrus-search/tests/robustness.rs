//! The parsers take untrusted input from the Client API, so they must never
//! panic and must stay fast on long or adversarial text.

use std::time::{Duration, Instant};

use proptest::prelude::*;
use serde_json::json;

use hydrus_search::{RatingTest, SystemPredicate, parse_api_search, parse_system_predicate};

/// Fragments that steer random text into every predicate's parser.
const FRAGMENTS: &[&str] = &[
    "system:",
    "has ",
    "no ",
    "num ",
    "number of ",
    "tags",
    "rating",
    "for ",
    "count",
    "=",
    "<",
    ">",
    "~=",
    "\u{2248}",
    "\u{2260}",
    "!=",
    "is ",
    "not ",
    "about",
    "3/5",
    "123",
    "like",
    "dislike",
    "rated",
    "all ",
    "any ",
    "only ",
    "(amongst ",
    ")",
    "\"",
    ":",
    ",",
    " ",
    "_",
    "tag",
    "in ",
    "\"my tags\"",
    "ignoring siblings",
    "status current",
    "hash",
    "md5",
    "similar to ",
    "data ",
    "distance ",
    "abcdef0123456789",
    "0702790ffeae5c8a",
    "filetype",
    "image",
    "jpeg",
    "import time",
    "since ",
    "before ",
    "the day of ",
    "the month of ",
    "around ",
    "2011-06-04",
    "13:45",
    "days",
    "7 ",
    "ago",
    "weeks",
    "duration",
    "5m30s",
    "views",
    "media",
    "preview",
    "client api",
    "viewtime",
    "ratio",
    "16:9",
    "square",
    "file service ",
    "is currently in ",
    "my files",
    "url",
    "domain",
    "class",
    "regex",
    "http://x",
    "note",
    "named",
    "tag as number ",
    "page",
    "limit",
    "filesize",
    "kb",
    "px",
    "megapixels",
    "fps",
    "file relationships",
    "alternates",
    "-",
    "\u{130}",
    "\u{3a3}",
    "\u{3164}",
    "\n",
    "\t",
    "\u{1f}",
];

fn fragment_text() -> impl Strategy<Value = String> {
    prop::collection::vec(prop::sample::select(FRAGMENTS), 0..16).prop_map(|parts| parts.concat())
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(4000))]

    #[test]
    fn never_panics_on_predicate_like_text(text in fragment_text()) {
        let _ = parse_system_predicate(&text);
        let _ = parse_system_predicate(&format!("system:{text}"));
    }

    #[test]
    fn never_panics_on_arbitrary_text(text in "\\PC{0,60}") {
        let _ = parse_system_predicate(&text);
        let _ = parse_system_predicate(&format!("system:{text}"));
        let _ = parse_api_search(&json!([text, [text.clone()], format!("-{text}")]));
    }

    #[test]
    fn errors_name_the_input(text in fragment_text()) {
        if let Err(e) = parse_system_predicate(&text) {
            prop_assert_eq!(&e.input, &text);
            prop_assert!(e.to_string().contains("could not parse"));
        }
    }
}

#[test]
fn long_inputs_are_fast() {
    let long = "a".repeat(20_000);
    let digits = "1".repeat(20_000);
    let inputs = [
        format!("system:rating for {long} = 3/5"),
        format!("system:rating for {long}"),
        format!("system:number of {long} tags > 5"),
        format!("system:has {long} embedded {long} metadata"),
        format!("system:all {long} rated"),
        format!("system:has tag {}: \"{long}\"", "\"x\",".repeat(2000)),
        format!("system:hash = {}", "abcdef0123 ".repeat(5000)),
        format!("system:filetype = {}", "jpeg, ".repeat(5000)),
        format!("system:import time < {}", "1 day ".repeat(5000)),
        format!("system:width > {digits}"),
        format!("system:ratio{long}"),
        format!(
            "system:similar to data {}",
            "0702790ffeae5c8a ".repeat(3000)
        ),
    ];
    for input in &inputs {
        let started = Instant::now();
        let _ = parse_system_predicate(input);
        assert!(
            started.elapsed() < Duration::from_secs(2),
            "slow on {}...",
            &input[..40]
        );
    }
    // the look-ahead in the rating names must still match on long names
    assert!(matches!(
        parse_system_predicate(&inputs[0]),
        Ok(SystemPredicate::Rating {
            test: RatingTest::Stars { stars: 3, .. },
            ..
        })
    ));
    assert!(matches!(
        parse_system_predicate(&inputs[2]),
        Ok(SystemPredicate::NumTags { count: 5, .. })
    ));
}
