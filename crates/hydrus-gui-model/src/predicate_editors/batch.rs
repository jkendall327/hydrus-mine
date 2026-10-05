//! Actual simple text and mixed populated active-predicate editing.
use super::{Blank, Context, Editor, Page};
use hydrus_core::Tag;
use hydrus_search::{Predicate, TextContext, predicate_text};

/// Non-system controls staged alongside the populated system panels.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Batch {
    pub simple: Vec<String>,
    pub invertible: Vec<Predicate>,
    pub unchanged: Vec<Predicate>,
    /// System panel indices and negative simple-text indices, in Qt row order.
    pub order: Vec<i32>,
}

/// The reference's simple tag text syntax, without autocomplete cleaning.
pub fn simple_predicate(text: &str) -> Result<Predicate, String> {
    let inclusive = !text.starts_with('-');
    let text = text.trim_start_matches('-');
    if text.is_empty() {
        return Err("Please enter some tag, namespace, or wildcard text!".into());
    }
    if text.matches(':').count() == 1 && (text.ends_with(':') || text.ends_with(":*")) {
        let namespace = text
            .strip_suffix(":*")
            .unwrap_or_else(|| &text[..text.len() - 1]);
        if namespace.is_empty() {
            return Err("Please enter some namespace text!".into());
        }
        Ok(Predicate::Namespace {
            namespace: namespace.into(),
            inclusive,
        })
    } else if text.contains('*') {
        Ok(Predicate::Wildcard {
            pattern: hydrus_core::search::predicate::Wildcard::from_clean(text),
            inclusive,
        })
    } else {
        Ok(Predicate::Tag {
            tag: Tag::from_clean(text),
            inclusive,
        })
    }
}
fn simple_text(predicate: &Predicate) -> Option<String> {
    let (value, inclusive) = match predicate {
        Predicate::Tag { tag, inclusive } => (tag.to_string(), inclusive),
        Predicate::Namespace {
            namespace,
            inclusive,
        } => (format!("{namespace}:*"), inclusive),
        Predicate::Wildcard { pattern, inclusive } => (pattern.to_string(), inclusive),
        _ => return None,
    };
    Some(format!("{}{value}", if *inclusive { "" } else { "-" }))
}
impl Editor {
    /// Build the represented mixed dialog. Unknown values are preserved, as Qt
    /// preserves noneditable terms; unsupported OR-only editing returns none.
    pub fn mixed(predicates: &[Predicate], context: &Context, text: &TextContext) -> Option<Self> {
        let mut ordered = predicates.to_vec();
        ordered.sort_by_cached_key(|p| match p {
            Predicate::System(hydrus_core::search::predicate::SystemPredicate::Number {
                property: hydrus_core::search::predicate::NumericProperty::Width,
                ..
            }) => "system:dimensions:0".into(),
            Predicate::System(hydrus_core::search::predicate::SystemPredicate::Number {
                property: hydrus_core::search::predicate::NumericProperty::Height,
                ..
            }) => "system:dimensions:1".into(),
            _ => predicate_text(p, text),
        });
        let mut panels = Vec::new();
        let mut batch = Batch::default();
        for p in ordered {
            if let Some(simple) = simple_text(&p) {
                batch
                    .order
                    .push(-1 - i32::try_from(batch.simple.len()).unwrap_or(i32::MAX - 1));
                batch.simple.push(simple);
            } else if let Some(editor) = Self::existing(&p, context) {
                batch
                    .order
                    .push(i32::try_from(panels.len()).unwrap_or(i32::MAX));
                panels.extend(editor.pages.into_iter().flat_map(|page| page.panels));
            } else if !matches!(p, Predicate::Or(_))
                && p.inverse(&|s| hydrus_search::entry::is_incdec(s, text))
                    .is_some()
            {
                batch.invertible.push(p);
            } else {
                batch.unchanged.push(p);
            }
        }
        if panels.is_empty() && batch.simple.is_empty() && batch.invertible.is_empty() {
            return None;
        }
        let mut editor = Self::new(Blank::Filesize, context);
        editor.note = None;
        editor.pages = vec![Page {
            name: String::new(),
            buttons: Vec::new(),
            panels,
            recent_types: Vec::new(),
        }];
        editor.supplied = predicates.first().cloned();
        editor.batch = Some(batch);
        Some(editor)
    }

    /// Apply validates every staged control before publishing any query change.
    pub fn mixed_predicates(&self, context: &Context) -> Result<Vec<Predicate>, String> {
        let Some(batch) = &self.batch else {
            return Err("not a mixed predicate editor".into());
        };
        let mut made = batch.unchanged.clone();
        made.extend(batch.invertible.clone());
        for simple in &batch.simple {
            made.push(simple_predicate(simple)?);
        }
        for panel in self.pages.iter().flat_map(|page| &page.panels) {
            made.extend(panel.predicates(context)?);
        }
        Ok(made)
    }
}
