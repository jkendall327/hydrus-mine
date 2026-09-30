//! Parsing formulas (`ClientParsing.ParseFormula*`, types 27, 31, 59, 60,
//! 133 and 136, with HTML parse rules, type 62), decoded into
//! `hydrus_parse` formulas.
//!
//! Formulas live inside parsers, which live in the domain manager; the
//! reference re-saves them at its current versions whenever the domain
//! manager is saved, so only those versions are accepted.

use hydrus_parse::formula::{
    Formula, FormulaKind, HtmlContent, HtmlRule, HtmlWalk, JsonContent, JsonRule, TagSearch,
};

use super::domain::{count, expect, nested_list, opt_count, string_match, string_processor};
use super::util::{DecodeResult, boolean, int, malformed, opt_int, string, tuple};
use crate::pyjson::PyJson;
use crate::serialisable::{SerialisableObject, SerialisableType};

const FORMULA_HTML: SerialisableType = SerialisableType(27);
const FORMULA_JSON: SerialisableType = SerialisableType(31);
const FORMULA_ZIPPER: SerialisableType = SerialisableType(59);
const FORMULA_CONTEXT_VARIABLE: SerialisableType = SerialisableType(60);
const RULE_HTML: SerialisableType = SerialisableType(62);
const FORMULA_NESTED: SerialisableType = SerialisableType(133);
const FORMULA_STATIC: SerialisableType = SerialisableType(136);

fn object_at(
    kind: SerialisableType,
    value: &PyJson,
    what: &str,
) -> DecodeResult<SerialisableObject> {
    SerialisableObject::from_tuple(value).map_err(|e| malformed(kind, format!("{what}: {e}")))
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
