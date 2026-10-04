//! Reusable HTML/JSON formula editors. Rules stay typed and ordered; parsing
//! and string processing use the same engine as downloaders and sidecars.
use crate::list_selection::ListSelection;
use hydrus_core::url::string_descriptions::index_to_pretty_ordinal;
use hydrus_core::url::strings::{StringMatch, StringProcessor};
use hydrus_parse::formula::{
    Formula, FormulaKind, HtmlContent, HtmlRule, HtmlWalk, JsonContent, JsonRule, ParsingContext,
    TagSearch,
};

/// A formula's test document, context variables and newline policy.
#[derive(Debug, Clone, PartialEq)]
pub struct FormulaTestData {
    pub context: ParsingContext,
    pub text: String,
    pub collapse_newlines: bool,
}
impl Default for FormulaTestData {
    fn default() -> Self {
        Self {
            context: ParsingContext::new(),
            text: String::new(),
            collapse_newlines: true,
        }
    }
}
/// One editable rule; HTML and JSON rules cannot be mixed in a formula.
#[derive(Debug, Clone, PartialEq)]
pub enum Rule {
    Html(HtmlRule),
    Json(JsonRule),
}
impl Rule {
    /// The reference's description in the rule queue.
    pub fn description(&self) -> String {
        match self {
            Self::Json(r) => match r {
                JsonRule::AllItems => "get all items".into(),
                JsonRule::Index(i) => format!(
                    "get the {} item (for Objects, keys sorted)",
                    index_to_pretty_ordinal(*i)
                ),
                JsonRule::DictKey(m) => format!(
                    "get the entries that have keys matching \"{}\"",
                    m.describe(false, false)
                ),
                JsonRule::TestStringItems(m) => {
                    format!("get the values that match \"{}\"", m.describe(false, false))
                }
                JsonRule::Ascend(n) => format!("walk back up {n} ancestors"),
                JsonRule::Deminify(i) => {
                    format!("de-minify json at the {} item", index_to_pretty_ordinal(*i))
                }
            },
            Self::Html(r) => {
                let mut text = match &r.walk {
                    HtmlWalk::Ancestor { depth } => r.tag_name.as_ref().map_or_else(
                        || format!("walk back up ancestors {depth} tag levels"),
                        |n| {
                            format!(
                                "walk back up ancestors to the {} <{n}> tag",
                                index_to_pretty_ordinal(*depth as i64 - 1)
                            )
                        },
                    ),
                    walk => {
                        let (direction, s) = match walk {
                            HtmlWalk::Descendants(s) => ("descendants", s),
                            HtmlWalk::NextSiblings(s) => ("next siblings", s),
                            HtmlWalk::PreviousSiblings(s) => ("previous siblings", s),
                            HtmlWalk::Ancestor { .. } => unreachable!(),
                        };
                        let which = s.index.map_or_else(
                            || "every".into(),
                            |i| format!("the {}", index_to_pretty_ordinal(i)),
                        );
                        let tag = r
                            .tag_name
                            .as_ref()
                            .map_or(String::new(), |n| format!(" <{n}>"));
                        let attrs = if s.attrs.is_empty() {
                            String::new()
                        } else {
                            format!(
                                " with attributes {}",
                                s.attrs
                                    .iter()
                                    .map(|(k, v)| format!("{k}={v}"))
                                    .collect::<Vec<_>>()
                                    .join(", ")
                            )
                        };
                        format!("search {direction} for {which}{tag} tag{attrs}")
                    }
                };
                if let Some(m) = &r.text_match {
                    text.push_str(&format!(
                        " with strings that match {}",
                        m.describe(false, false)
                    ));
                }
                text
            }
        }
    }
}
/// A formula draft, isolated from the caller until Apply.
#[derive(Debug, Clone)]
pub struct FormulaEditor {
    pub formula: Formula,
    pub test: FormulaTestData,
    pub selection: ListSelection<usize>,
}
impl FormulaEditor {
    /// Start an isolated edit, preserving unsupported kinds intact.
    pub fn new(formula: &Formula, test: FormulaTestData) -> Self {
        Self {
            formula: formula.clone(),
            test,
            selection: ListSelection::default(),
        }
    }
    /// Whether this formula has an HTML or JSON rule editor.
    pub fn supported(&self) -> bool {
        matches!(
            self.formula.kind,
            FormulaKind::Html { .. } | FormulaKind::Json { .. }
        )
    }
    /// Ordered rules for the queue and its editors.
    pub fn rules(&self) -> Vec<Rule> {
        match &self.formula.kind {
            FormulaKind::Html { rules, .. } => rules.iter().cloned().map(Rule::Html).collect(),
            FormulaKind::Json { rules, .. } => rules.iter().cloned().map(Rule::Json).collect(),
            _ => Vec::new(),
        }
    }
    /// The default rule used by Add.
    pub fn new_rule(&self) -> Rule {
        if matches!(self.formula.kind, FormulaKind::Json { .. }) {
            Rule::Json(JsonRule::DictKey(StringMatch::fixed("posts")))
        } else {
            Rule::Html(HtmlRule {
                walk: HtmlWalk::Descendants(TagSearch::default()),
                tag_name: Some("a".into()),
                text_match: None,
            })
        }
    }
    /// Replace a rule or append one. Reject a rule of a different kind.
    pub fn put(&mut self, at: Option<usize>, rule: Rule) {
        fn put<T>(rules: &mut Vec<T>, at: Option<usize>, rule: T) {
            if let Some(i) = at.filter(|i| *i < rules.len()) {
                rules[i] = rule;
            } else {
                rules.push(rule);
            }
        }
        match (&mut self.formula.kind, rule) {
            (FormulaKind::Html { rules, .. }, Rule::Html(r)) => put(rules, at, r),
            (FormulaKind::Json { rules, .. }, Rule::Json(r)) => put(rules, at, r),
            _ => {}
        }
    }
    /// Select a rule using the common list modifiers.
    pub fn click(&mut self, row: usize, ctrl: bool, shift: bool) {
        let order = (0..self.rules().len()).collect::<Vec<_>>();
        self.selection.click(&order, row, ctrl, shift);
    }
    /// Change between HTML and JSON, using the reference's fresh defaults.
    /// Separated HTML/JSON content keeps separated extraction on conversion.
    pub fn change_type(&mut self, json: bool) {
        if !self.supported() || json == matches!(self.formula.kind, FormulaKind::Json { .. }) {
            return;
        }
        let separated = matches!(
            self.formula.kind,
            FormulaKind::Html {
                content: HtmlContent::Html,
                ..
            } | FormulaKind::Json {
                content: JsonContent::Json,
                ..
            }
        );
        self.formula = new_formula(json);
        if separated {
            match &mut self.formula.kind {
                FormulaKind::Html { content, .. } => *content = HtmlContent::Html,
                FormulaKind::Json { content, .. } => *content = JsonContent::Json,
                _ => {}
            }
        }
        self.selection = ListSelection::default();
    }
    /// Selected positions in queue order.
    pub fn selected(&self) -> Vec<usize> {
        self.selection
            .in_order(&(0..self.rules().len()).collect::<Vec<_>>())
    }
    /// Remove selected rules, retaining the order of the others.
    pub fn delete(&mut self) {
        let selected = self.selected();
        for i in selected.into_iter().rev() {
            match &mut self.formula.kind {
                FormulaKind::Html { rules, .. } => {
                    rules.remove(i);
                }
                FormulaKind::Json { rules, .. } => {
                    rules.remove(i);
                }
                _ => {}
            }
        }
        self.selection = ListSelection::default();
    }
    /// Move selected rules one position, as the reference queue moves them.
    pub fn shift(&mut self, down: bool) {
        let mut selected = self.selected();
        if down {
            selected.reverse();
        }
        let len = self.rules().len();
        let mut flags = (0..len)
            .map(|i| self.selection.is_selected(i))
            .collect::<Vec<_>>();
        for i in selected {
            let j = if down {
                (i + 1).min(len.saturating_sub(1))
            } else {
                i.saturating_sub(1)
            };
            flags.swap(i, j);
            match &mut self.formula.kind {
                FormulaKind::Html { rules, .. } => rules.swap(i, j),
                FormulaKind::Json { rules, .. } => rules.swap(i, j),
                _ => {}
            }
        }
        self.selection = ListSelection::default();
        for (i, on) in flags.into_iter().enumerate() {
            if on {
                self.click(i, true, false);
            }
        }
    }
    /// Validate the reference's nonempty HTML attribute requirement.
    pub fn value(&self) -> Result<Formula, String> {
        if let FormulaKind::Html {
            content: HtmlContent::Attribute(a),
            ..
        } = &self.formula.kind
            && a.is_empty()
        {
            return Err("Please enter an attribute to fetch!".into());
        }
        Ok(self.formula.clone())
    }
    /// Run the test document with the processor enabled.
    pub fn results(&self) -> Result<Vec<String>, String> {
        self.value()?
            .parse(
                &self.test.context,
                &self.test.text,
                self.test.collapse_newlines,
            )
            .map_err(|e| e.to_string())
    }
    /// Strings for the processor's test panel, before processing.
    pub fn processor_texts(&self) -> Vec<String> {
        let mut f = self.formula.clone();
        f.processor = StringProcessor::default();
        f.parse(
            &self.test.context,
            &self.test.text,
            self.test.collapse_newlines,
        )
        .unwrap_or_else(|_| vec![String::new()])
    }
}
/// Default HTML/JSON formula for a new formula or a deliberate type change.
pub fn new_formula(json: bool) -> Formula {
    Formula {
        reference_auxiliary: None,
        name: String::new(),
        kind: if json {
            FormulaKind::Json {
                rules: vec![JsonRule::DictKey(StringMatch::fixed("posts"))],
                content: JsonContent::Strings,
            }
        } else {
            FormulaKind::Html {
                rules: vec![HtmlRule {
                    walk: HtmlWalk::Descendants(TagSearch::default()),
                    tag_name: Some("a".into()),
                    text_match: None,
                }],
                content: HtmlContent::Attribute("href".into()),
            }
        },
        processor: StringProcessor::default(),
    }
}

/// Rule controls retain inactive values while switching traversal modes.
#[derive(Debug, Clone)]
pub struct RuleEditor {
    pub html: bool,
    pub kind: usize,
    pub tag: String,
    pub attrs: Vec<(String, String)>,
    pub index: Option<i64>,
    pub depth: usize,
    pub match_on: bool,
    pub key_match: StringMatch,
    pub value_match: StringMatch,
}
impl RuleEditor {
    /// Populate controls from an existing rule.
    pub fn new(rule: &Rule) -> Self {
        let mut e = Self {
            html: matches!(rule, Rule::Html(_)),
            kind: 0,
            tag: String::new(),
            attrs: Vec::new(),
            index: None,
            depth: 1,
            match_on: false,
            key_match: StringMatch::fixed("posts"),
            value_match: StringMatch {
                kind: hydrus_core::url::strings::MatchKind::Flexible(
                    hydrus_core::url::strings::FlexibleMatch::Numeric,
                ),
                example: "123456".into(),
                ..StringMatch::any()
            },
        };
        match rule {
            Rule::Html(r) => {
                e.value_match = StringMatch::any();
                e.tag = r.tag_name.clone().unwrap_or_default();
                if let Some(m) = &r.text_match {
                    e.value_match = m.clone();
                    e.match_on = true;
                }
                let search = match &r.walk {
                    HtmlWalk::Descendants(s) => Some(s),
                    HtmlWalk::Ancestor { depth } => {
                        e.kind = 1;
                        e.depth = *depth;
                        None
                    }
                    HtmlWalk::PreviousSiblings(s) => {
                        e.kind = 2;
                        Some(s)
                    }
                    HtmlWalk::NextSiblings(s) => {
                        e.kind = 3;
                        Some(s)
                    }
                };
                if let Some(s) = search {
                    e.attrs.clone_from(&s.attrs);
                    e.index = s.index;
                }
            }
            Rule::Json(r) => match r {
                JsonRule::DictKey(m) => e.key_match = m.clone(),
                JsonRule::AllItems => e.kind = 1,
                JsonRule::Index(i) => {
                    e.kind = 2;
                    e.index = Some(*i);
                }
                JsonRule::TestStringItems(m) => {
                    e.kind = 3;
                    e.value_match = m.clone();
                }
                JsonRule::Ascend(n) => {
                    e.kind = 4;
                    e.depth = *n;
                }
                JsonRule::Deminify(i) => {
                    e.kind = 5;
                    e.index = Some(*i);
                }
            },
        }
        e
    }
    /// Add or replace an attribute, as the reference's dictionary control.
    pub fn put_attribute(&mut self, name: String, value: String) {
        if let Some((_, v)) = self.attrs.iter_mut().find(|(k, _)| *k == name) {
            *v = value;
        } else {
            self.attrs.push((name, value));
        }
    }
    /// The match used by the current rule type.
    pub fn string_match(&self) -> &StringMatch {
        if !self.html && self.kind == 0 {
            &self.key_match
        } else {
            &self.value_match
        }
    }
    /// Store a match in the active controls.
    pub fn set_match(&mut self, m: StringMatch) {
        if !self.html && self.kind == 0 {
            self.key_match = m;
        } else {
            self.value_match = m;
        }
    }
    /// Construct a typed rule, excluding controls inactive for its type.
    pub fn value(&self) -> Rule {
        if self.html {
            let search = TagSearch {
                attrs: self.attrs.clone(),
                index: self.index,
            };
            Rule::Html(HtmlRule {
                walk: match self.kind {
                    1 => HtmlWalk::Ancestor { depth: self.depth },
                    2 => HtmlWalk::PreviousSiblings(search),
                    3 => HtmlWalk::NextSiblings(search),
                    _ => HtmlWalk::Descendants(search),
                },
                tag_name: (!self.tag.is_empty()).then(|| self.tag.clone()),
                text_match: self.match_on.then(|| self.value_match.clone()),
            })
        } else {
            Rule::Json(match self.kind {
                1 => JsonRule::AllItems,
                2 => JsonRule::Index(self.index.unwrap_or(0)),
                3 => JsonRule::TestStringItems(self.value_match.clone()),
                4 => JsonRule::Ascend(self.depth),
                5 => JsonRule::Deminify(self.index.unwrap_or(0)),
                _ => JsonRule::DictKey(self.key_match.clone()),
            })
        }
    }
}

/// Insert or replace a simple downloader formula with the reference's unique
/// name suffix. Existing names, including an edited row's old name, count.
pub fn put_simple_formula(
    formulae: &mut Vec<hydrus_parse::simple::SimpleFormula>,
    at: Option<usize>,
    mut formula: hydrus_parse::simple::SimpleFormula,
) {
    let original = formula.name.clone();
    let mut n = 1;
    while formulae.iter().any(|f| f.name == formula.name) {
        formula.name = format!("{original} ({n})");
        n += 1;
    }
    if let Some(i) = at.filter(|i| *i < formulae.len()) {
        formulae[i] = formula;
    } else {
        formulae.push(formula);
    }
}
