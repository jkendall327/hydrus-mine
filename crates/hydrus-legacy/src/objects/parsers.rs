//! Parsing formulas (`ClientParsing.ParseFormula*`, types 27, 31, 59, 60,
//! 133 and 136, with HTML parse rules, type 62), decoded into
//! `hydrus_parse` formulas.
//!
//! Also page parsers (58) with their content parsers (30) and subsidiary
//! parsers (135), and the domain manager's gallery URL generators (69, 70).
//!
//! Formulas live inside parsers, which live in the domain manager; the
//! reference re-saves them at its current versions whenever the domain
//! manager is saved, so only those versions are accepted.

use hydrus_core::url::{AnyGug, Gug, Gugs, NestedGug};
use hydrus_parse::content::{ContentKind, ContentParser, PageParser, SubsidiaryPageParser};
use hydrus_parse::formula::{
    Formula, FormulaKind, HtmlContent, HtmlRule, HtmlWalk, JsonContent, JsonRule, TagSearch,
};
use hydrus_parse::{Downloaders, Unconverted};

use super::domain::{
    count, expect, nested_list, opt_count, string_converter, string_match, string_processor,
};
use super::util::{
    DecodeResult, boolean, hex_bytes, int, list, malformed, opt_int, opt_string, string, tuple,
};
use crate::pyjson::PyJson;
use crate::serialisable::{SerialisableObject, SerialisableType};

const FORMULA_HTML: SerialisableType = SerialisableType(27);
const FORMULA_JSON: SerialisableType = SerialisableType(31);
const FORMULA_ZIPPER: SerialisableType = SerialisableType(59);
const FORMULA_CONTEXT_VARIABLE: SerialisableType = SerialisableType(60);
const RULE_HTML: SerialisableType = SerialisableType(62);
const FORMULA_NESTED: SerialisableType = SerialisableType(133);
const FORMULA_STATIC: SerialisableType = SerialisableType(136);
const CONTENT_PARSER: SerialisableType = SerialisableType(30);
const PAGE_PARSER: SerialisableType = SerialisableType(58);
const SUBSIDIARY_PAGE_PARSER: SerialisableType = SerialisableType(135);
const GUG: SerialisableType = SerialisableType(69);
const SIMPLE_FORMULA: SerialisableType = SerialisableType(63);
const NESTED_GUG: SerialisableType = SerialisableType(70);
const DOMAIN_MANAGER: SerialisableType = SerialisableType(53);

fn object_at(
    kind: SerialisableType,
    value: &PyJson,
    what: &str,
) -> DecodeResult<SerialisableObject> {
    SerialisableObject::from_tuple(value).map_err(|e| malformed(kind, format!("{what}: {e}")))
}

/// Decode a simple downloader's formula (`SimpleDownloaderParsingFormula`):
/// its name, and its formula as its info.
pub fn simple_formula(
    object: &SerialisableObject,
) -> DecodeResult<hydrus_parse::simple::SimpleFormula> {
    let k = SIMPLE_FORMULA;
    expect(object, k, &[1])?;
    let formula = formula(&object_at(k, &object.info(), "formula")?)?;
    Ok(hydrus_parse::simple::SimpleFormula {
        name: object.name.clone().unwrap_or_default(),
        formula,
    })
}

/// The simple downloader formulae of a new reference client
/// (`static/default/simple_downloader_formulae`, as
/// `oracle/dump_simple_downloader_formulae.py` recorded them).
pub fn default_simple_formulae() -> DecodeResult<Vec<hydrus_parse::simple::SimpleFormula>> {
    let tuples = PyJson::parse(include_str!("simple_downloader_formulae_defaults.json"))
        .map_err(|e| malformed(SIMPLE_FORMULA, e.to_string()))?;
    tuples
        .as_list()
        .unwrap_or_default()
        .iter()
        .map(|t| simple_formula(&object_at(SIMPLE_FORMULA, t, "default formula")?))
        .collect()
}

/// Decode any kind of formula.
pub fn formula(object: &SerialisableObject) -> DecodeResult<Formula> {
    let info = object.info();
    match object.kind {
        FORMULA_HTML => {
            let k = FORMULA_HTML;
            expect(object, k, &[8])?;
            let [rules, content, attribute, name, processor] =
                tuple::<5>(k, &info, "html formula")?;
            let rules = nested_list(k, rules, "tag rules")?
                .iter()
                .map(html_rule)
                .collect::<DecodeResult<_>>()?;
            let content = match int(k, content, "content to fetch")? {
                0 => HtmlContent::Attribute(string(k, attribute, "attribute")?),
                1 => HtmlContent::Text,
                2 => HtmlContent::Html,
                other => return Err(malformed(k, format!("unknown html content {other}"))),
            };
            Ok(Formula {
                reference_auxiliary: None,
                name: string(k, name, "name")?,
                kind: FormulaKind::Html { rules, content },
                processor: string_processor(&object_at(k, processor, "string processor")?)?,
            })
        }
        FORMULA_JSON => {
            let k = FORMULA_JSON;
            expect(object, k, &[4])?;
            let [rules, content, name, processor] = tuple::<4>(k, &info, "json formula")?;
            let rules = super::util::list(k, rules, "parse rules")?
                .iter()
                .map(|rule| {
                    let [rule_type, value] = tuple::<2>(k, rule, "parse rule")?;
                    Ok(match int(k, rule_type, "rule type")? {
                        0 => JsonRule::DictKey(string_match(&object_at(k, value, "key match")?)?),
                        1 => JsonRule::AllItems,
                        2 => JsonRule::Index(int(k, value, "index")?),
                        3 => JsonRule::Deminify(int(k, value, "deminify index")?),
                        4 => JsonRule::Ascend(count(k, value, "ascend steps")?),
                        5 => JsonRule::TestStringItems(string_match(&object_at(
                            k,
                            value,
                            "string match",
                        )?)?),
                        other => return Err(malformed(k, format!("unknown json rule {other}"))),
                    })
                })
                .collect::<DecodeResult<_>>()?;
            let content = match int(k, content, "content to fetch")? {
                0 => JsonContent::Strings,
                1 => JsonContent::Json,
                2 => JsonContent::DictKeys,
                other => return Err(malformed(k, format!("unknown json content {other}"))),
            };
            Ok(Formula {
                reference_auxiliary: None,
                name: string(k, name, "name")?,
                kind: FormulaKind::Json { rules, content },
                processor: string_processor(&object_at(k, processor, "string processor")?)?,
            })
        }
        FORMULA_ZIPPER => {
            let k = FORMULA_ZIPPER;
            expect(object, k, &[3])?;
            let [formulae, phrase, name, processor] = tuple::<4>(k, &info, "zipper formula")?;
            Ok(Formula {
                reference_auxiliary: None,
                name: string(k, name, "name")?,
                kind: FormulaKind::Zipper {
                    formulae: nested_list(k, formulae, "formulae")?
                        .iter()
                        .map(formula)
                        .collect::<DecodeResult<_>>()?,
                    phrase: string(k, phrase, "substitution phrase")?,
                },
                processor: string_processor(&object_at(k, processor, "string processor")?)?,
            })
        }
        FORMULA_CONTEXT_VARIABLE => {
            let k = FORMULA_CONTEXT_VARIABLE;
            expect(object, k, &[3])?;
            let [variable, name, processor] = tuple::<3>(k, &info, "context variable formula")?;
            Ok(Formula {
                reference_auxiliary: None,
                name: string(k, name, "name")?,
                kind: FormulaKind::ContextVariable {
                    variable: string(k, variable, "variable")?,
                },
                processor: string_processor(&object_at(k, processor, "string processor")?)?,
            })
        }
        FORMULA_NESTED => {
            let k = FORMULA_NESTED;
            expect(object, k, &[2])?;
            let [main, sub, name, processor] = tuple::<4>(k, &info, "nested formula")?;
            Ok(Formula {
                reference_auxiliary: None,
                name: string(k, name, "name")?,
                kind: FormulaKind::Nested {
                    main: Box::new(formula(&object_at(k, main, "main formula")?)?),
                    sub: Box::new(formula(&object_at(k, sub, "sub formula")?)?),
                },
                processor: string_processor(&object_at(k, processor, "string processor")?)?,
            })
        }
        FORMULA_STATIC => {
            let k = FORMULA_STATIC;
            expect(object, k, &[1])?;
            let [text, num, name, processor] = tuple::<4>(k, &info, "static formula")?;
            Ok(Formula {
                reference_auxiliary: None,
                name: string(k, name, "name")?,
                kind: FormulaKind::Static {
                    text: string(k, text, "static text")?,
                    // Python's `range` of a negative number is empty
                    count: usize::try_from(int(k, num, "count")?).unwrap_or(0),
                },
                processor: string_processor(&object_at(k, processor, "string processor")?)?,
            })
        }
        other => Err(malformed(other, "not a parsing formula")),
    }
}

fn html_rule(object: &SerialisableObject) -> DecodeResult<HtmlRule> {
    let k = RULE_HTML;
    expect(object, k, &[2, 3])?;
    let info = object.info();
    let [rule_type, name, attrs, index, depth, test, text_match] =
        tuple::<7>(k, &info, "html rule")?;
    let tag_name = if name.is_null() {
        None
    } else {
        Some(string(k, name, "tag name")?)
    };
    let search = || -> DecodeResult<TagSearch> {
        let attrs = match attrs {
            PyJson::Null => Vec::new(),
            PyJson::Object(entries) => entries
                .iter()
                .map(|(key, value)| Ok((key.clone(), string(k, value, "attribute value")?)))
                .collect::<DecodeResult<_>>()?,
            _ => return Err(malformed(k, "tag attributes are not an object")),
        };
        Ok(TagSearch {
            attrs,
            index: opt_int(k, index, "tag index")?,
        })
    };
    let walk = match int(k, rule_type, "rule type")? {
        0 => HtmlWalk::Descendants(search()?),
        1 => HtmlWalk::Ancestor {
            depth: opt_count(k, depth, "tag depth")?.unwrap_or(1),
        },
        2 => HtmlWalk::NextSiblings(search()?),
        3 => HtmlWalk::PreviousSiblings(search()?),
        other => return Err(malformed(k, format!("unknown html rule type {other}"))),
    };
    let text_match = if boolean(k, test, "test tag string")? {
        Some(string_match(&object_at(
            k,
            text_match,
            "tag string match",
        )?)?)
    } else {
        None
    };
    Ok(HtmlRule {
        walk,
        tag_name,
        text_match,
    })
}

/// Decode a page parser, with its content parsers and subsidiary parsers.
pub fn page_parser(object: &SerialisableObject) -> DecodeResult<PageParser> {
    let k = PAGE_PARSER;
    expect(object, k, &[3])?;
    let info = object.info();
    let [
        name,
        key,
        converter,
        subsidiary,
        content_parsers,
        example_urls,
        _context,
    ] = tuple::<7>(k, &info, "page parser")?;
    Ok(PageParser {
        reference_auxiliary: None,
        name: string(k, name, "name")?,
        key: string(k, key, "parser key")?,
        converter: string_converter(&object_at(k, converter, "string converter")?)?,
        subsidiary: nested_list(k, subsidiary, "subsidiary page parsers")?
            .iter()
            .map(subsidiary_page_parser)
            .collect::<DecodeResult<_>>()?,
        content_parsers: nested_list(k, content_parsers, "content parsers")?
            .iter()
            .map(content_parser)
            .collect::<DecodeResult<_>>()?,
        example_urls: list(k, example_urls, "example urls")?
            .iter()
            .map(|u| string(k, u, "example url"))
            .collect::<DecodeResult<_>>()?,
    })
}

/// Decode a subsidiary wrapper with its separation formula and recursive page.
pub fn subsidiary_page_parser(object: &SerialisableObject) -> DecodeResult<SubsidiaryPageParser> {
    let k = SUBSIDIARY_PAGE_PARSER;
    expect(object, k, &[2])?;
    let info = object.info();
    let [formula_tuple, sort, parser] = tuple::<3>(k, &info, "subsidiary page parser")?;
    Ok(SubsidiaryPageParser {
        formula: formula(&object_at(k, formula_tuple, "formula")?)?,
        sort_by_source_time: boolean(k, sort, "sort by source time")?,
        parser: page_parser(&object_at(k, parser, "page parser")?)?,
    })
}

/// Decode a content parser.
pub fn content_parser(object: &SerialisableObject) -> DecodeResult<ContentParser> {
    let k = CONTENT_PARSER;
    expect(object, k, &[7])?;
    let info = object.info();
    let [name, content_type, formula_tuple, extra] = tuple::<4>(k, &info, "content parser")?;
    let pair = || tuple::<2>(k, extra, "additional info");
    let kind = match int(k, content_type, "content type")? {
        7 => {
            let [url_type, priority] = pair()?;
            ContentKind::Url {
                url_type: int(k, url_type, "url type")?,
                priority: int(k, priority, "priority")?,
            }
        }
        0 => ContentKind::Tag {
            namespace: opt_string(k, extra, "namespace")?,
        },
        18 => ContentKind::Note {
            name: string(k, extra, "note name")?,
        },
        15 => {
            let [hash_type, encoding] = pair()?;
            ContentKind::Hash {
                hash_type: string(k, hash_type, "hash type")?,
                encoding: string(k, encoding, "hash encoding")?,
            }
        }
        16 => ContentKind::Timestamp {
            timestamp_type: opt_int(k, extra, "timestamp type")?,
        },
        17 => ContentKind::Title {
            priority: int(k, extra, "title priority")?,
        },
        22 => ContentKind::HttpHeader {
            name: string(k, extra, "header name")?,
        },
        14 => ContentKind::Variable {
            name: string(k, extra, "variable name")?,
        },
        8 => {
            let [if_found, string_match_tuple] = pair()?;
            ContentKind::Veto {
                if_matches_found: boolean(k, if_found, "veto if matches found")?,
                string_match: string_match(&object_at(k, string_match_tuple, "veto match")?)?,
            }
        }
        other => return Err(malformed(k, format!("unknown content type {other}"))),
    };
    Ok(ContentParser {
        name: string(k, name, "name")?,
        kind,
        formula: formula(&object_at(k, formula_tuple, "formula")?)?,
    })
}

/// Decode the domain manager's downloader definitions: its GUGs and page
/// parsers. One that can't be decoded is listed as unconverted rather than
/// failing the rest.
pub fn downloaders(object: &SerialisableObject) -> DecodeResult<Downloaders> {
    let k = DOMAIN_MANAGER;
    expect(object, k, &[7])?;
    let info = object.info();
    let [gugs, keys_to_display, _, _, _, _, _, parsers, _] =
        tuple::<9>(k, &info, "domain manager")?;
    let mut unconverted = Vec::new();
    let mut failed = |kind: &str, object: &SerialisableObject, name: Option<String>, e| {
        unconverted.push(Unconverted {
            kind: kind.to_owned(),
            name: name.or_else(|| object.name.clone()).unwrap_or_default(),
            reason: format!("{e}"),
        });
    };
    let mut converted_gugs = Vec::new();
    for object in nested_list(k, gugs, "gugs")? {
        match gug(&object) {
            Ok(g) => converted_gugs.push(g),
            Err(e) => failed("gug", &object, None, e),
        }
    }
    let mut converted_parsers = Vec::new();
    for object in nested_list(k, parsers, "parsers")? {
        match page_parser(&object) {
            Ok(p) => converted_parsers.push(p),
            Err(e) => {
                // a page parser keeps its name in its info
                let name = list(k, &object.info(), "page parser")
                    .ok()
                    .and_then(|items| items.first().and_then(|n| n.as_str().map(str::to_owned)));
                failed("parser", &object, name, e);
            }
        }
    }
    Ok(Downloaders {
        gugs: Gugs {
            gugs: converted_gugs,
            keys_to_display: list(k, keys_to_display, "gug keys to display")?
                .iter()
                .map(|key| hex_bytes(k, key, "gug key").map(hex::encode))
                .collect::<DecodeResult<_>>()?,
        },
        parsers: converted_parsers,
        unconverted,
    })
}

/// Decode a gallery URL generator or a nested one.
pub fn gug(object: &SerialisableObject) -> DecodeResult<AnyGug> {
    let name = || {
        object
            .name
            .clone()
            .ok_or_else(|| malformed(object.kind, "gug has no name"))
    };
    let info = object.info();
    match object.kind {
        GUG => {
            let k = GUG;
            expect(object, k, &[1])?;
            let [key, template, phrase, separator, initial, example] = tuple::<6>(k, &info, "gug")?;
            Ok(AnyGug::Single(Gug {
                name: name()?,
                key: hex::encode(hex_bytes(k, key, "gug key")?),
                url_template: string(k, template, "url template")?,
                replacement_phrase: string(k, phrase, "replacement phrase")?,
                separator: string(k, separator, "search terms separator")?,
                initial_search_text: string(k, initial, "initial search text")?,
                example_search_text: string(k, example, "example search text")?,
            }))
        }
        NESTED_GUG => {
            let k = NESTED_GUG;
            expect(object, k, &[1])?;
            let [key, initial, gugs] = tuple::<3>(k, &info, "nested gug")?;
            Ok(AnyGug::Nested(NestedGug {
                name: name()?,
                key: hex::encode(hex_bytes(k, key, "gug key")?),
                initial_search_text: string(k, initial, "initial search text")?,
                gugs: list(k, gugs, "gug keys and names")?
                    .iter()
                    .map(|pair| {
                        let [key, name] = tuple::<2>(k, pair, "gug key and name")?;
                        Ok((
                            hex::encode(hex_bytes(k, key, "gug key")?),
                            string(k, name, "gug name")?,
                        ))
                    })
                    .collect::<DecodeResult<_>>()?,
            }))
        }
        other => Err(malformed(other, "not a gallery url generator")),
    }
}
