//! Concrete native-to-reference encoders. No database writes live here.
use crate::{Error, Native, Result};
use hydrus_core::url::class::{GalleryIndexPosition, ReferralMode};
use hydrus_core::url::strings::{
    Conversion, HashFunction, MatchKind, ProcessingStep, StringConverter, StringMatch,
    StringProcessor,
};
use hydrus_core::url::{AnyGug, UrlClass};
use hydrus_parse::content::{ContentKind, ContentParser, PageParser};
use hydrus_parse::formula::{
    Formula, FormulaKind, HtmlContent, HtmlRule, HtmlWalk, JsonContent, JsonRule,
};
use serde_json::{Value, json};

fn object(kind: u16, version: u32, info: Value) -> Value {
    Value::Array(vec![json!(kind), json!(version), info])
}
fn named(kind: u16, name: &str, version: u32, info: Value) -> Value {
    Value::Array(vec![json!(kind), json!(name), json!(version), info])
}
pub(crate) fn serialisable_list(values: Vec<Value>) -> Value {
    object(
        26,
        3,
        json!(
            values
                .into_iter()
                .map(|v| json!([2, v]))
                .collect::<Vec<_>>()
        ),
    )
}
pub(crate) fn string_match(m: &StringMatch) -> Value {
    let (kind, value) = match &m.kind {
        MatchKind::Fixed(s) => (0, json!(s)),
        MatchKind::Flexible(f) => (1, json!(*f as i64)),
        MatchKind::Regex(r) => (2, json!(r.pattern())),
        MatchKind::Any => (3, json!("")),
    };
    object(
        51,
        1,
        json!([kind, value, m.min_chars, m.max_chars, m.example]),
    )
}
pub(crate) fn converter(c: &StringConverter) -> Result<Value> {
    let conversions = c
        .conversions
        .iter()
        .map(|conversion| {
            let upgraded = match conversion {
                Conversion::Unsupported { code, data } => {
                    Conversion::from_preserved_date(*code, data)
                }
                _ => None,
            };
            let conversion = upgraded.as_ref().unwrap_or(conversion);
            Ok(match conversion {
                Conversion::RemoveFromStart(n) => json!([0, n]),
                Conversion::RemoveFromEnd(n) => json!([1, n]),
                Conversion::Prepend(s) => json!([2, s]),
                Conversion::Append(s) => json!([3, s]),
                Conversion::Encode(e) => json!([4, *e as i64]),
                Conversion::Decode(e) => json!([5, *e as i64]),
                Conversion::KeepStart(n) => json!([6, n]),
                Conversion::KeepEnd(n) => json!([7, n]),
                Conversion::Reverse => json!([8, null]),
                Conversion::RegexSub {
                    pattern,
                    replacement,
                } => json!([9, [pattern.pattern(), replacement]]),
                Conversion::DateDecode {
                    phrase,
                    timezone,
                    offset,
                } => json!([10, [phrase, timezone.code(), offset]]),
                Conversion::DateEncode { phrase, timezone } => {
                    json!([12, [phrase, timezone.code()]])
                }
                Conversion::DateParse => json!([14, null]),
                Conversion::IntegerAddition(n) => json!([11, n]),
                Conversion::Hash(h) => json!([
                    13,
                    match h {
                        HashFunction::Md5 => "md5",
                        HashFunction::Sha1 => "sha1",
                        HashFunction::Sha256 => "sha256",
                        HashFunction::Sha512 => "sha512",
                    }
                ]),
                Conversion::AppendRandom { population, count } => json!([15, [population, count]]),
                Conversion::Unsupported { code, .. } => {
                    return Err(Error::Unsupported(format!("string conversion {code}")));
                }
            })
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(object(55, 2, json!([conversions, c.example])))
}
pub(super) fn processor(p: &StringProcessor) -> Result<Value> {
    let steps = p
        .steps
        .iter()
        .map(|step| {
            Ok(match step {
                ProcessingStep::Convert(c) => converter(c)?,
                ProcessingStep::Filter(m) => string_match(m),
                ProcessingStep::Split {
                    separator,
                    max_splits,
                } => object(83, 2, json!([separator, max_splits])),
                ProcessingStep::Slice { start, end } => object(100, 1, json!([start, end])),
                ProcessingStep::Join { joiner, tuple_size } => {
                    object(125, 2, json!([joiner, tuple_size]))
                }
                ProcessingStep::Sort {
                    kind,
                    ascending,
                    regex,
                } => object(
                    99,
                    1,
                    json!([
                        *kind as i64,
                        ascending,
                        regex
                            .as_ref()
                            .map(hydrus_core::url::strings::PyRegex::pattern)
                    ]),
                ),
                ProcessingStep::TagFilter(s) => object(
                    112,
                    1,
                    json!([
                        object(
                            44,
                            1,
                            json!(
                                s.filter
                                    .rules()
                                    .map(|(slice, rule)| json!([
                                        slice,
                                        i32::from(
                                            rule != hydrus_core::tag_filter::FilterRule::Blacklist
                                        )
                                    ]))
                                    .collect::<Vec<_>>()
                            )
                        ),
                        s.example
                    ]),
                ),
                ProcessingStep::Unsupported { type_id } => {
                    return Err(Error::Unsupported(format!(
                        "string processing step {type_id}"
                    )));
                }
            })
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(object(84, 1, serialisable_list(steps)))
}
fn html_rule(r: &HtmlRule) -> Value {
    let (kind, attrs, index, depth) = match &r.walk {
        HtmlWalk::Descendants(s) => (
            0,
            json!(
                s.attrs
                    .iter()
                    .map(|(k, v)| (k.clone(), json!(v)))
                    .collect::<serde_json::Map<_, _>>()
            ),
            json!(s.index),
            Value::Null,
        ),
        HtmlWalk::NextSiblings(s) => (
            2,
            json!(
                s.attrs
                    .iter()
                    .map(|(k, v)| (k.clone(), json!(v)))
                    .collect::<serde_json::Map<_, _>>()
            ),
            json!(s.index),
            Value::Null,
        ),
        HtmlWalk::PreviousSiblings(s) => (
            3,
            json!(
                s.attrs
                    .iter()
                    .map(|(k, v)| (k.clone(), json!(v)))
                    .collect::<serde_json::Map<_, _>>()
            ),
            json!(s.index),
            Value::Null,
        ),
        HtmlWalk::Ancestor { depth } => (1, json!({}), Value::Null, json!(depth)),
    };
    object(
        62,
        3,
        json!([
            kind,
            r.tag_name,
            attrs,
            index,
            depth,
            r.text_match.is_some(),
            string_match(r.text_match.as_ref().unwrap_or(&StringMatch::any()))
        ]),
    )
}
pub(crate) fn formula(f: &Formula) -> Result<Value> {
    let p = processor(&f.processor)?;
    let mut encoded = match &f.kind {
        FormulaKind::Html { rules, content } => {
            let (kind, attr) = match content {
                HtmlContent::Attribute(a) => (0, a.as_str()),
                HtmlContent::Text => (1, "href"),
                HtmlContent::Html => (2, "href"),
            };
            object(
                27,
                8,
                json!([
                    serialisable_list(rules.iter().map(html_rule).collect()),
                    kind,
                    attr,
                    f.name,
                    p
                ]),
            )
        }
        FormulaKind::Json { rules, content } => {
            let rules = rules
                .iter()
                .map(|r| match r {
                    JsonRule::DictKey(m) => json!([0, string_match(m)]),
                    JsonRule::AllItems => json!([1, null]),
                    JsonRule::Index(n) => json!([2, n]),
                    JsonRule::Deminify(n) => json!([3, n]),
                    JsonRule::Ascend(n) => json!([4, n]),
                    JsonRule::TestStringItems(m) => json!([5, string_match(m)]),
                })
                .collect::<Vec<_>>();
            let content = match content {
                JsonContent::Strings => 0,
                JsonContent::Json => 1,
                JsonContent::DictKeys => 2,
            };
            object(31, 4, json!([rules, content, f.name, p]))
        }
        FormulaKind::Zipper { formulae, phrase } => object(
            59,
            3,
            json!([
                serialisable_list(formulae.iter().map(formula).collect::<Result<_>>()?),
                phrase,
                f.name,
                p
            ]),
        ),
        FormulaKind::ContextVariable { variable } => object(60, 3, json!([variable, f.name, p])),
        FormulaKind::Nested { main, sub } => {
            object(133, 2, json!([formula(main)?, formula(sub)?, f.name, p]))
        }
        FormulaKind::Static { text, count } => object(136, 1, json!([text, count, f.name, p])),
    };
    if let Some(original) = &f.reference_auxiliary {
        preserve_auxiliary(&mut encoded, original);
    }
    Ok(encoded)
}

pub(crate) fn content(c: &ContentParser) -> Result<Value> {
    let (kind, extra) = match &c.kind {
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
        ContentKind::Veto {
            if_matches_found,
            string_match: m,
        } => (8, json!([if_matches_found, string_match(m)])),
    };
    Ok(object(
        30,
        7,
        json!([c.name, kind, formula(&c.formula)?, extra]),
    ))
}
pub(crate) fn page(p: &PageParser) -> Result<Value> {
    valid_key(&p.key)?;
    let mut contents = p.content_parsers.iter().collect::<Vec<_>>();
    contents.sort_by_cached_key(|c| hydrus_core::casefold::casefold(&c.name));
    let mut subsidiary = p.subsidiary.iter().collect::<Vec<_>>();
    subsidiary.sort_by_cached_key(|s| hydrus_core::casefold::casefold(&s.parser.name));
    let mut encoded = named(
        58,
        &p.name,
        3,
        json!([
            p.name,
            p.key,
            converter(&p.converter)?,
            serialisable_list(
                subsidiary
                    .into_iter()
                    .map(|s| Ok(object(
                        135,
                        2,
                        json!([
                            formula(&s.formula)?,
                            s.sort_by_source_time,
                            page(&s.parser)?
                        ])
                    )))
                    .collect::<Result<_>>()?
            ),
            serialisable_list(contents.into_iter().map(content).collect::<Result<_>>()?),
            p.example_urls,
            {}
        ]),
    );
    if let Some(original) = &p.reference_auxiliary {
        preserve_auxiliary(&mut encoded, original);
    }
    Ok(encoded)
}

pub(crate) fn valid_key(key: &str) -> Result<()> {
    let bytes = hex::decode(key)
        .map_err(|_| Error::Invalid("Definition key is not hexadecimal.".into()))?;
    if bytes.is_empty() || bytes.len() > 128 {
        return Err(Error::Invalid(
            "Definition key must contain 1–128 bytes.".into(),
        ));
    }
    Ok(())
}
fn class(c: &UrlClass) -> Result<Value> {
    valid_key(&hex::encode(&c.key))?;
    let parameters = c
        .parameters
        .iter()
        .map(|p| {
            Ok(object(
                127,
                2,
                json!([
                    p.name,
                    string_match(&p.value),
                    p.ephemeral,
                    p.default,
                    processor(&p.default_processor)?
                ]),
            ))
        })
        .collect::<Result<_>>()?;
    let (index_kind, index_id, index_delta) =
        c.gallery_index
            .as_ref()
            .map_or((Value::Null, Value::Null, json!(1)), |g| {
                match &g.position {
                    GalleryIndexPosition::PathComponent(i) => (json!(0), json!(i), json!(g.delta)),
                    GalleryIndexPosition::Parameter(n) => (json!(1), json!(n), json!(g.delta)),
                }
            });
    let referral = match c.referral.mode {
        ReferralMode::OnlyIfProvided => 0,
        ReferralMode::Never => 1,
        ReferralMode::ConverterIfNoneProvided => 2,
        ReferralMode::OnlyConverter => 3,
    };
    Ok(named(
        50,
        &c.name,
        15,
        json!([
            hex::encode(&c.key),
            c.url_type.code(),
            c.preferred_scheme,
            object(
                139,
                1,
                json!([
                    c.domain_mask.raw_domains,
                    c.domain_mask.domain_regexes,
                    c.domain_mask.match_subdomains,
                    c.domain_mask.keep_matched_subdomains
                ])
            ),
            [
                c.alphabetise_get_parameters,
                c.no_more_path_components_than_this,
                c.no_more_parameters_than_this,
                c.keep_extra_parameters_for_server,
                c.can_produce_multiple_files,
                c.should_be_associated_with_files,
                c.keep_fragment
            ],
            c.path_components
                .iter()
                .map(|(m, d)| json!([string_match(m), d]))
                .collect::<Vec<_>>(),
            serialisable_list(parameters),
            c.has_single_value_parameters,
            string_match(&c.single_value_parameters_match),
            c.header_overrides,
            converter(&c.api_lookup_converter)?,
            referral,
            converter(&c.referral.converter)?,
            index_kind,
            index_id,
            index_delta,
            c.example_url
        ]),
    ))
}
pub(crate) fn native(n: &Native) -> Result<Value> {
    Ok(match n {
        Native::Class(c) => class(c)?,
        Native::Page(p) => page(p)?,
        Native::Content(c) => content(c)?,
        Native::Formula(f) => formula(f)?,
        Native::Simple(f) => named(63, &f.name, 1, formula(&f.formula)?),
        Native::Gug(g) => {
            valid_key(g.key())?;
            match g {
                AnyGug::Single(g) => named(
                    69,
                    &g.name,
                    1,
                    json!([
                        g.key,
                        g.url_template,
                        g.replacement_phrase,
                        g.separator,
                        g.initial_search_text,
                        g.example_search_text
                    ]),
                ),
                AnyGug::Nested(g) => named(
                    70,
                    &g.name,
                    1,
                    json!([g.key, g.initial_search_text, g.gugs]),
                ),
            }
        }
    })
}

/// Keep inert editor fields, recursively, while native edits replace runtime fields.
pub(crate) fn preserve_auxiliary(encoded: &mut Value, original: &Value) {
    let (Some(e), Some(o)) = (encoded.as_array_mut(), original.as_array()) else {
        return;
    };
    if e.first() != o.first() {
        return;
    }
    let kind = e[0].as_u64().unwrap_or_default();
    let info = if e.len() == 4 { 3 } else { 2 };
    if e.len() != o.len() {
        return;
    }
    let (Some(ei), Some(oi)) = (e[info].as_array_mut(), o[info].as_array()) else {
        return;
    };
    match kind {
        51 if ei.len() == 5 && oi.len() == 5 && ei[0] == json!(3) => ei[1] = oi[1].clone(),
        55 if ei.len() == 2 && oi.len() == 2 => {
            if oi[1].is_null() && ei[1] == json!("") {
                ei[1] = Value::Null;
            }
            if let (Some(conversions), Some(old)) = (ei[0].as_array_mut(), oi[0].as_array()) {
                for (conversion, old) in conversions.iter_mut().zip(old) {
                    if conversion.get(0) == Some(&json!(8))
                        && old.get(0) == Some(&json!(8))
                        && let Some(data) = old.get(1)
                    {
                        conversion[1] = data.clone();
                    }
                }
            }
        }
        136 if ei.len() == 4
            && oi.len() == 4
            && ei[1] == json!(0)
            && oi[1].as_i64().is_some_and(|n| n < 0) =>
        {
            ei[1] = oi[1].clone();
        }
        58 if ei.len() == 7 && oi.len() == 7 => ei[6] = oi[6].clone(),
        27 if ei.len() == 5 && oi.len() == 5 && ei[1] != json!(0) => ei[2] = oi[2].clone(),
        62 if ei.len() == 7 && oi.len() == 7 => {
            if oi[2].is_null() && ei[2] == json!({}) {
                ei[2] = Value::Null;
            }
            if ei[0] == json!(1) {
                ei[2] = oi[2].clone();
                ei[3] = oi[3].clone();
            } else {
                ei[4] = oi[4].clone();
            }
            if ei[5] == json!(false) {
                ei[6] = oi[6].clone();
            }
        }
        _ => (),
    }
    // The structure of supported object trees is unchanged by edits to their
    // children; list rows are paired by their native name where available.
    for (child, old) in ei.iter_mut().zip(oi) {
        preserve_tree(child, old);
    }
}
fn identity(value: &Value) -> Option<String> {
    let a = value.as_array()?;
    let kind = a.first()?.as_u64()?;
    let info = a.last()?.as_array()?;
    match kind {
        58 => info.get(1)?.as_str().map(str::to_owned),
        135 => identity(info.get(2)?),
        30 => info.first()?.as_str().map(str::to_owned),
        _ => None,
    }
}
fn preserve_tree(encoded: &mut Value, original: &Value) {
    if let (Some(e), Some(o)) = (encoded.as_array(), original.as_array())
        && e.len() == 3
        && e[0] == json!(26)
        && o.len() == 3
        && o[0] == json!(26)
    {
        let old = o[2].as_array().cloned().unwrap_or_default();
        if let Some(rows) = encoded[2].as_array_mut() {
            for (i, row) in rows.iter_mut().enumerate() {
                let id = row.get(1).and_then(identity);
                let previous = if let Some(id) = id {
                    old.iter()
                        .find(|v| v.get(1).and_then(identity).as_ref() == Some(&id))
                } else {
                    old.get(i)
                };
                if let Some(previous) = previous {
                    preserve_tree(row, previous);
                }
            }
        }
        return;
    }
    if encoded
        .as_array()
        .is_some_and(|a| matches!(a.len(), 3 | 4) && a[0].is_number())
    {
        preserve_auxiliary(encoded, original);
    } else if let (Some(e), Some(o)) = (encoded.as_array_mut(), original.as_array()) {
        for (child, old) in e.iter_mut().zip(o) {
            preserve_tree(child, old);
        }
    }
}
