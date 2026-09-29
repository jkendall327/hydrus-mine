//! The Client API's search syntax: the `tags` parameter of
//! `/get_files/search_files` (and the other endpoints that take a search).
//!
//! `tags` is a JSON list whose items are:
//!
//! - `"system:..."`: a system predicate (the prefix is case-sensitive; a
//!   string like `"System:inbox"` is an ordinary tag);
//! - `"-tag"`: a tag the files must not have;
//! - `"tag"`: a tag the files must have. A tag whose subtag is `*`
//!   (`"series:*"`) means any tag in that namespace; any other tag
//!   containing `*` is a wildcard;
//! - a nested list: an OR group of the same kinds of item.
//!
//! Tags are cleaned like every tag in hydrus (lowercased, whitespace
//! collapsed, ...) and duplicates are dropped. Items of any other JSON type
//! are ignored, as the reference does.

use serde_json::Value;

use hydrus_core::Tag;
use hydrus_core::tag::split_tag;

use crate::error::ApiSearchError;
use crate::parse::parse_system_predicate;
use crate::predicate::{Predicate, Wildcard};

/// Parse a Client API `tags` search list into predicates.
///
/// The result lists OR groups first, then system predicates, then required
/// tags, then excluded tags, each in the order given.
pub fn parse_api_search(tags: &Value) -> Result<Vec<Predicate>, ApiSearchError> {
    let items = tags.as_array().ok_or(ApiSearchError::NotAList)?;
    parse_list(items)
}

fn parse_list(items: &[Value]) -> Result<Vec<Predicate>, ApiSearchError> {
    let mut or_groups = Vec::new();
    let mut system = Vec::new();
    let mut included: Vec<String> = Vec::new();
    let mut excluded: Vec<String> = Vec::new();
    for item in items {
        match item {
            Value::Array(group) => or_groups.push(Predicate::Or(parse_list(group)?)),
            Value::String(text) if text.starts_with("system:") => {
                system.push(Predicate::System(parse_system_predicate(text)?));
            }
            Value::String(text) => {
                let clean =
                    Tag::new(text).ok_or_else(|| ApiSearchError::InvalidTag(text.clone()))?;
                // cleaning strips the leading "-" of an excluded tag
                let list = if text.starts_with('-') {
                    &mut excluded
                } else {
                    &mut included
                };
                let clean = clean.into_string();
                if !list.contains(&clean) {
                    list.push(clean);
                }
            }
            _ => {}
        }
    }
    let mut out = or_groups;
    out.append(&mut system);
    out.extend(included.into_iter().map(|t| tag_predicate(t, true)));
    out.extend(excluded.into_iter().map(|t| tag_predicate(t, false)));
    Ok(out)
}

/// A cleaned tag as a tag, namespace or wildcard predicate.
fn tag_predicate(tag: String, inclusive: bool) -> Predicate {
    if !tag.contains('*') {
        return Predicate::Tag {
            tag: Tag::from_clean(tag),
            inclusive,
        };
    }
    let (namespace, subtag) = split_tag(&tag);
    if subtag == "*" {
        Predicate::Namespace {
            namespace: namespace.to_owned(),
            inclusive,
        }
    } else {
        Predicate::Wildcard {
            pattern: Wildcard::from_clean(tag),
            inclusive,
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::predicate::SystemPredicate;

    #[test]
    fn documented_example() {
        let preds = parse_api_search(&json!([
            "skirt",
            ["space bounty hunter", "jane raider"],
            "system:height > 1000"
        ]))
        .unwrap();
        assert_eq!(preds.len(), 3);
        assert!(matches!(&preds[0], Predicate::Or(group) if group.len() == 2));
        assert!(matches!(
            &preds[1],
            Predicate::System(SystemPredicate::Number { .. })
        ));
        assert_eq!(
            preds[2],
            Predicate::Tag {
                tag: Tag::from_clean("skirt"),
                inclusive: true
            }
        );
    }

    #[test]
    fn negation_wildcards_and_namespaces() {
        let preds =
            parse_api_search(&json!(["-Green Eyes", "series:*", "character:sam*"])).unwrap();
        assert_eq!(
            preds,
            vec![
                Predicate::Namespace {
                    namespace: "series".into(),
                    inclusive: true
                },
                Predicate::Wildcard {
                    pattern: Wildcard::from_clean("character:sam*"),
                    inclusive: true
                },
                Predicate::Tag {
                    tag: Tag::from_clean("green eyes"),
                    inclusive: false
                },
            ]
        );
    }

    #[test]
    fn rejects_bad_input() {
        assert_eq!(
            parse_api_search(&json!("blue eyes")),
            Err(ApiSearchError::NotAList)
        );
        assert_eq!(
            parse_api_search(&json!(["-"])),
            Err(ApiSearchError::InvalidTag("-".into()))
        );
        assert!(matches!(
            parse_api_search(&json!(["system:nonsense"])),
            Err(ApiSearchError::SystemPredicate(_))
        ));
    }
}
