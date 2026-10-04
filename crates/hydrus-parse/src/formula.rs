//! Parsing formulas: how a parser picks strings out of a document.
//!
//! A formula finds raw strings (by walking an HTML tree, walking JSON,
//! combining other formulas, or from the parsing context), tidies them
//! (newlines collapsed or ends stripped) and runs them through a string
//! processor. The behaviour is the reference's `ClientParsing` formulas',
//! checked on `oracle/fixtures/formulas.json`.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use hydrus_core::pyjson::PyJson;
use hydrus_core::sort::human_sort;
use hydrus_core::url::strings::{StringMatch, StringProcessor};

use crate::dom::{Html, NodeId};
use crate::text::{py_strip, remove_newlines};

/// Values a parse can refer to: the page's `url`, the `post_index`, ...
pub type ParsingContext = BTreeMap<String, String>;

/// Why a formula could not parse a document.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct ParseError(pub String);

/// A formula, with its name and the processing of what it finds.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Formula {
    /// Reference editor fields not used by parsing, retained through native edits.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reference_auxiliary: Option<serde_json::Value>,
    pub name: String,
    pub kind: FormulaKind,
    pub processor: StringProcessor,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FormulaKind {
    /// Walk the HTML with tag rules, then take something from each tag.
    Html {
        rules: Vec<HtmlRule>,
        content: HtmlContent,
    },
    /// Walk the JSON with rules, then take something from each value.
    Json {
        rules: Vec<JsonRule>,
        content: JsonContent,
    },
    /// Run several formulas and substitute their results, position by
    /// position, into a phrase (`\1`, `\2`, ...).
    Zipper {
        formulae: Vec<Formula>,
        phrase: String,
    },
    /// A value from the parsing context.
    ContextVariable { variable: String },
    /// Run `sub` on each result of `main`.
    Nested {
        main: Box<Formula>,
        sub: Box<Formula>,
    },
    /// The same text, `count` times.
    Static { text: String, count: usize },
}

/// What an HTML formula takes from each tag it finds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HtmlContent {
    Attribute(String),
    /// The text inside.
    Text,
    /// The tag's HTML.
    Html,
}

/// One step of walking an HTML tree.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HtmlRule {
    pub walk: HtmlWalk,
    pub tag_name: Option<String>,
    /// Keep only tags whose text matches.
    pub text_match: Option<StringMatch>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HtmlWalk {
    Descendants(TagSearch),
    NextSiblings(TagSearch),
    PreviousSiblings(TagSearch),
    /// Up to the `depth`th ancestor (with the rule's tag name, if any).
    Ancestor {
        depth: usize,
    },
}

/// Which of the found tags to keep.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct TagSearch {
    pub attrs: Vec<(String, String)>,
    /// Keep only this one (negative counts from the end, as in Python).
    pub index: Option<i64>,
}

/// One step of walking JSON.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JsonRule {
    /// Values of the object's keys that match, by key.
    DictKey(StringMatch),
    /// Every item of a list, or every value of an object by key.
    AllItems,
    /// One item (an object's keys in human order); negative from the end.
    Index(i64),
    /// Keep scalar values whose text matches.
    TestStringItems(StringMatch),
    /// Go back up this many steps.
    Ascend(usize),
    /// Un-minify a list whose integers point at other entries of it.
    Deminify(i64),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JsonContent {
    /// Scalars, as text.
    Strings,
    /// The JSON below.
    Json,
    /// An object's keys.
    DictKeys,
}

/// Python's `list[index]`.
fn py_index<T>(items: &[T], index: i64) -> Option<&T> {
    let len = items.len() as i64;
    let i = if index < 0 { len + index } else { index };
    (0..len).contains(&i).then(|| &items[i as usize])
}

impl Formula {
    /// Parse `text`: the raw strings found, tidied and processed.
    /// `collapse_newlines` joins each result's lines (for everything but
    /// notes); otherwise results are just stripped.
    pub fn parse(
        &self,
        context: &ParsingContext,
        text: &str,
        collapse_newlines: bool,
    ) -> Result<Vec<String>, ParseError> {
        let raw = self.raw_texts(context, text, collapse_newlines)?;
        let tidied: Vec<String> = raw
            .iter()
            .map(|t| {
                if collapse_newlines {
                    remove_newlines(t)
                } else {
                    py_strip(t).to_owned()
                }
            })
            .collect();
        self.processor.process(tidied).map_err(ParseError)
    }

    fn raw_texts(
        &self,
        context: &ParsingContext,
        text: &str,
        collapse_newlines: bool,
    ) -> Result<Vec<String>, ParseError> {
        match &self.kind {
            FormulaKind::Html { rules, content } => {
                Ok(html_texts(&Html::parse(text), rules, content))
            }
            FormulaKind::Json { rules, content } => json_texts(text, rules, *content),
            FormulaKind::Zipper { formulae, phrase } => {
                let mut streams = Vec::with_capacity(formulae.len());
                for formula in formulae {
                    let stream = formula.parse(context, text, collapse_newlines)?;
                    if stream.is_empty() {
                        return Ok(Vec::new());
                    }
                    streams.push(stream);
                }
                let count = streams.iter().map(Vec::len).max().unwrap_or(0);
                Ok((0..count)
                    .map(|i| {
                        let mut out = phrase.clone();
                        for (n, stream) in streams.iter().enumerate() {
                            let value = stream.get(i).or(stream.last()).map_or("", String::as_str);
                            out = out.replace(&format!("\\{}", n + 1), value);
                        }
                        out
                    })
                    .collect())
            }
            FormulaKind::ContextVariable { variable } => {
                Ok(context.get(variable).cloned().into_iter().collect())
            }
            FormulaKind::Nested { main, sub } => {
                let mut out = Vec::new();
                for main_text in main.parse(context, text, collapse_newlines)? {
                    out.extend(sub.parse(context, &main_text, collapse_newlines)?);
                }
                Ok(out)
            }
            FormulaKind::Static { text, count } => Ok(vec![text.clone(); *count]),
        }
    }
}

// html ------------------------------------------------------------------------

fn html_texts(html: &Html, rules: &[HtmlRule], content: &HtmlContent) -> Vec<String> {
    let mut nodes = vec![html.root()];
    for rule in rules {
        nodes = rule_nodes(html, rule, &nodes);
    }
    nodes
        .into_iter()
        .filter_map(|node| {
            let text = match content {
                HtmlContent::Attribute(name) => {
                    html.attr(node, name).map(crate::dom::AttrValue::joined)?
                }
                HtmlContent::Text => html.text(node),
                HtmlContent::Html => html.outer_html(node),
            };
            (!text.is_empty()).then_some(text)
        })
        .collect()
}

fn rule_nodes(html: &Html, rule: &HtmlRule, nodes: &[NodeId]) -> Vec<NodeId> {
    let name = rule.tag_name.as_deref();
    let mut found = Vec::new();
    for &node in nodes {
        match &rule.walk {
            HtmlWalk::Descendants(search)
            | HtmlWalk::NextSiblings(search)
            | HtmlWalk::PreviousSiblings(search) => {
                let hits = match &rule.walk {
                    HtmlWalk::Descendants(_) => html.find_all(node, name, &search.attrs),
                    HtmlWalk::NextSiblings(_) => html.next_siblings(node, name, &search.attrs),
                    _ => html.previous_siblings(node, name, &search.attrs),
                };
                match search.index {
                    None => found.extend(hits),
                    Some(index) => found.extend(py_index(&hits, index).copied()),
                }
            }
            HtmlWalk::Ancestor { depth } => {
                let mut count = 0;
                let mut current = html.parent(node);
                while let Some(parent) = current.filter(|&p| html.is_tag(p)) {
                    if name.is_none_or(|n| html.name(parent) == Some(n)) {
                        count += 1;
                    }
                    if count == *depth {
                        found.push(parent);
                        break;
                    }
                    current = html.parent(parent);
                }
            }
        }
    }
    if let Some(text_match) = &rule.text_match {
        found.retain(|&node| text_match.matches(&html.text(node)));
    }
    found
}

// json ------------------------------------------------------------------------

/// The reference's `LooksLikeHTML`.
fn looks_like_html(text: &str) -> bool {
    ["<html", "<HTML", "<!DOCTYPE html", "<!DOCTYPE HTML"]
        .iter()
        .any(|s| text.contains(s))
}

/// How deep deminifying may go before it is taken to be a cycle.
const MAX_DEMINIFY_DEPTH: usize = 500;

fn json_texts(
    text: &str,
    rules: &[JsonRule],
    content: JsonContent,
) -> Result<Vec<String>, ParseError> {
    let root = PyJson::parse(text)
        .map_err(|e| {
            ParseError(if looks_like_html(text) {
                "Unable to parse: Appeared to receive HTML instead of JSON.".to_owned()
            } else {
                format!("Unable to parse that JSON: {e}.")
            })
        })?
        .with_python_dict_semantics();

    // each node with the path of nodes above it
    let mut current: Vec<(PyJson, Vec<PyJson>)> = vec![(root, Vec::new())];
    for rule in rules {
        let mut next = Vec::new();
        for (node, stack) in current {
            let mut below = stack.clone();
            below.push(node.clone());
            match rule {
                JsonRule::AllItems => match &node {
                    PyJson::List(items) => {
                        next.extend(items.iter().map(|i| (i.clone(), below.clone())));
                    }
                    PyJson::Object(entries) => {
                        next.extend(
                            sorted_entries(entries).map(|(_, v)| (v.clone(), below.clone())),
                        );
                    }
                    _ => {}
                },
                JsonRule::Index(index) => match &node {
                    PyJson::List(items) => {
                        if let Some(item) = py_index(items, *index) {
                            next.push((item.clone(), below));
                        }
                    }
                    PyJson::Object(entries) => {
                        let mut keys: Vec<&str> = entries.iter().map(|(k, _)| k.as_str()).collect();
                        human_sort(&mut keys);
                        if let Some(key) = py_index(&keys, *index) {
                            let value = entries
                                .iter()
                                .find(|(k, _)| k == key)
                                .map(|(_, v)| v.clone());
                            next.extend(value.map(|v| (v, below)));
                        }
                    }
                    _ => {}
                },
                JsonRule::DictKey(key_match) => {
                    if let PyJson::Object(entries) = &node {
                        next.extend(
                            sorted_entries(entries)
                                .filter(|(k, _)| key_match.matches(k))
                                .map(|(_, v)| (v.clone(), below.clone())),
                        );
                    }
                }
                JsonRule::TestStringItems(text_match) => {
                    if let Some(text) = node.py_str().filter(|_| !node.is_null())
                        && text_match.matches(&text)
                    {
                        // a filter: the node keeps its own path
                        next.push((node, stack));
                    }
                }
                JsonRule::Ascend(steps) => {
                    if stack.len() >= *steps {
                        let at = stack.len() - steps;
                        // Python's stack[-0] is the first item
                        let (ancestor, above) = if *steps == 0 {
                            match stack.first() {
                                Some(first) => (first.clone(), Vec::new()),
                                None => continue,
                            }
                        } else {
                            (stack[at].clone(), stack[..at].to_vec())
                        };
                        next.push((ancestor, above));
                    }
                }
                JsonRule::Deminify(index) => {
                    let PyJson::List(items) = &node else { continue };
                    let Some(start) = py_index(items, *index) else {
                        continue;
                    };
                    match deminify(items, start, 0) {
                        Ok(Some(value)) => next.push((value, below)),
                        Ok(None) => {}
                        Err(e) => return Err(e),
                    }
                }
            }
        }
        current = next;
    }

    let mut out = Vec::new();
    for (node, _) in current {
        match content {
            JsonContent::Strings => {
                if !node.is_null()
                    && let Some(text) = node.py_str()
                {
                    out.push(text);
                }
            }
            JsonContent::Json => out.push(node.to_python_string_unicode()),
            JsonContent::DictKeys => {
                if let PyJson::Object(entries) = &node {
                    out.extend(sorted_entries(entries).map(|(k, _)| k.clone()));
                }
            }
        }
    }
    Ok(out)
}

/// An object's entries sorted by key (Python's `sorted(dict.items())`).
fn sorted_entries(entries: &[(String, PyJson)]) -> impl Iterator<Item = &(String, PyJson)> {
    let mut sorted: Vec<&(String, PyJson)> = entries.iter().collect();
    sorted.sort_by(|a, b| a.0.cmp(&b.0));
    sorted.into_iter()
}

/// Resolve the integers of `item` as indices into `list`; `Ok(None)` when an
/// index is out of range (the reference then skips the node).
fn deminify(list: &[PyJson], item: &PyJson, depth: usize) -> Result<Option<PyJson>, ParseError> {
    if depth > MAX_DEMINIFY_DEPTH {
        return Err(ParseError(
            "Could not deminify that JSON: its references go round in a cycle.".to_owned(),
        ));
    }
    Ok(Some(match item {
        PyJson::List(items) => {
            let mut out = Vec::with_capacity(items.len());
            for i in items {
                match deminify(list, i, depth + 1)? {
                    Some(v) => out.push(v),
                    None => return Ok(None),
                }
            }
            PyJson::List(out)
        }
        PyJson::Object(entries) => {
            let mut out = Vec::with_capacity(entries.len());
            for (k, v) in entries {
                match deminify(list, v, depth + 1)? {
                    Some(v) => out.push((k.clone(), v)),
                    None => return Ok(None),
                }
            }
            PyJson::Object(out)
        }
        // Python's bools are ints too
        PyJson::Int(_) | PyJson::Bool(_) => {
            let index = match item {
                PyJson::Int(i) => *i,
                PyJson::Bool(b) => i64::from(*b),
                _ => unreachable!(),
            };
            let Some(target) = py_index(list, index) else {
                return Ok(None);
            };
            match target {
                PyJson::Int(_) | PyJson::Bool(_) => target.clone(),
                other => match deminify(list, other, depth + 1)? {
                    Some(v) => v,
                    None => return Ok(None),
                },
            }
        }
        // an index too big for any list
        PyJson::BigInt(_) => return Ok(None),
        other => other.clone(),
    }))
}
