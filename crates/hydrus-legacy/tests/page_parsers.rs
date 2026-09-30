//! Page parsers against the reference: `oracle/fixtures/page_parsers.json`
//! holds random page parsers (as the reference serialises them) run on
//! random HTML and JSON pages, with the posts that came out and what an
//! importer reads from each. Each parser is decoded here and run by
//! `hydrus-parse`.

use serde_json::{Value as Json, json};

use hydrus_legacy::objects::parsers::page_parser;
use hydrus_legacy::serialisable::SerialisableObject;
use hydrus_parse::ParsingContext;
use hydrus_parse::content::{ContentKind, ParseFailure, ParsedPost, title, url_type};

const URL_TYPES: [i64; 4] = [
    url_type::NEXT,
    url_type::DESIRED,
    url_type::SOURCE,
    url_type::SUB_GALLERY,
];
const TIMESTAMP_TYPES: [i64; 3] = [0, 1, 3];

fn describe(post: &ParsedPost) -> Json {
    let contents: Vec<Json> = post
        .contents
        .iter()
        .map(|c| {
            let (content_type, info) = match &c.kind {
                ContentKind::Url { url_type, priority } => (7, json!([url_type, priority])),
                ContentKind::Tag { namespace } => (0, json!(namespace)),
                ContentKind::Note { name } => (18, json!(name)),
                ContentKind::Hash {
                    hash_type,
                    encoding,
                } => (15, json!([hash_type, encoding])),
                ContentKind::Timestamp { timestamp_type } => (16, json!(timestamp_type)),
                ContentKind::Title { priority } => (17, json!(priority)),
                ContentKind::HttpHeader { name } => (22, json!(name)),
                ContentKind::Variable { name } => (14, json!(name)),
                ContentKind::Veto { .. } => (8, Json::Null),
            };
            json!([c.name, content_type, info, c.text])
        })
        .collect();
    let now = hydrus_core::time::TimestampMs::now().millis() / 1000;
    json!({
        "contents": contents,
        "tags": post.tags(),
        "urls": URL_TYPES.iter().map(|t| (t.to_string(), json!(post.urls(&[*t], false)))).collect::<serde_json::Map<_, _>>(),
        "top_file_urls": post.urls(&[url_type::DESIRED], true),
        "notes": post.notes(),
        "hashes": post.hashes().iter().map(|(t, h)| json!([t, hex::encode(h)])).collect::<Vec<_>>(),
        "timestamps": TIMESTAMP_TYPES.iter().map(|t| (t.to_string(), json!(post.timestamp(*t, now)))).collect::<serde_json::Map<_, _>>(),
        "variable": post.variable().map(|(n, v)| json!([n, v])),
        "headers": post.http_headers(),
        "pursuable": post.has_pursuable_urls(),
    })
}

/// Python ints are unbounded, so the reference keeps a parsed time like
/// 10^21 seconds; ours are 64-bit and ignore one that doesn't fit (see
/// DIFFERENCES.md). The recording's only such times are the only time of
/// their type in their post.
fn without_huge_timestamps(posts: &Json) -> Json {
    let mut posts = posts.clone();
    for post in posts.as_array_mut().into_iter().flatten() {
        for (_, time) in post["timestamps"].as_object_mut().into_iter().flatten() {
            if !time.is_null() && time.as_i64().is_none() {
                *time = Json::Null;
            }
        }
    }
    posts
}

#[test]
fn page_parsers_parse_like_the_reference() {
    let recorded = hydrus_testkit::fixture_json("page_parsers.json");
    let documents = recorded["documents"].as_array().unwrap();
    let cases = recorded["cases"].as_array().unwrap();
    let mut failures = Vec::new();
    for (i, case) in cases.iter().enumerate() {
        let object = SerialisableObject::from_tuple_str(&case["parser"].to_string()).unwrap();
        let parser = match page_parser(&object) {
            Ok(p) => p,
            Err(e) => {
                failures.push(format!("case {i}: could not decode: {e}"));
                continue;
            }
        };
        let document = documents[case["document"].as_u64().unwrap() as usize]
            .as_str()
            .unwrap();
        let mut context: ParsingContext = serde_json::from_value(case["context"].clone()).unwrap();
        let got = parser.parse(&mut context, document);
        let (ok, detail) = match (&got, case) {
            (Ok(posts), c) if c.get("posts").is_some() => {
                let got = json!({
                    "posts": posts.iter().map(describe).collect::<Vec<_>>(),
                    "title": title(posts),
                    "context_after": context,
                });
                let expected = json!({
                    "posts": without_huge_timestamps(&c["posts"]),
                    "title": c["title"],
                    "context_after": c["context_after"],
                });
                (
                    got == expected,
                    format!("expected {expected}\n     got {got}"),
                )
            }
            (Err(ParseFailure::Veto(name)), c) if c.get("veto").is_some() => (
                format!("veto: {name}") == c["veto"].as_str().unwrap(),
                format!("veto {name} vs {}", c["veto"]),
            ),
            (Err(ParseFailure::Error(_)), c)
                if c.get("error").is_some() || c.get("crash").is_some() =>
            {
                (true, String::new())
            }
            (got, c) => (false, format!("expected {c} got {got:?}")),
        };
        if !ok {
            failures.push(format!("case {i}: {}", &detail[..detail.len().min(3000)]));
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {} cases differ; first:\n{}",
        failures.len(),
        cases.len(),
        failures
            .iter()
            .take(5)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    );
}
