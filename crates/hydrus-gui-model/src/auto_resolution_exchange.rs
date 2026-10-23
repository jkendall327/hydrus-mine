//! The duplicates auto-resolution rules list's export, import and
//! duplicate buttons (`AddImportExportButtons`, `_ImportRule`): rules go out
//! in the reference's own serialised form (one, or a list), and come in as
//! that or as the hydrus-rs JSON earlier versions exported; the list names
//! them apart from the rules already there, ignoring case.

use std::collections::BTreeSet;

use hydrus_store::duplicates::auto::{Comparator, Rule};

pub const EXPORT_LABELS: [&str; 3] = ["to clipboard", "to json file", "to png file"];
pub const IMPORT_LABELS: [&str; 3] = [
    "from clipboard",
    "from json files",
    "from png files (you can also drag and drop pngs onto this list)",
];
pub const PROBLEM_TITLE: &str = "Problem loading!";

/// The png picker's title (`_ImportFromPNG`).
pub const PNG_PICKER_TITLE: &str = "select the png or pngs with the encoded data";

/// The title and message for an import that could not be read: from a file
/// (`_ImportJSONs`, `_ImportPNGs`: "Problem importing!") or from the
/// clipboard (`_ImportFromClipboard`).
pub fn import_failure(from_file: bool, error: &str) -> (&'static str, String) {
    if from_file {
        ("Problem importing!", error.to_owned())
    } else {
        (
            PROBLEM_TITLE,
            format!("I could not understand what was in the clipboard: {error}"),
        )
    }
}

/// The title for an export that could not be written (hydrus-rs only: a
/// predicate it can't put in the reference's form).
pub const EXPORT_PROBLEM_TITLE: &str = "Problem exporting!";

/// The numerical rating services' scales, for star rating predicates.
pub type Scales<'a> =
    &'a dyn Fn(&hydrus_core::ServiceKey) -> Option<hydrus_legacy::objects::predicates::StarScale>;

/// The key earlier hydrus-rs versions exported rules under.
const NATIVE_KEY: &str = "hydrus_rs_duplicates_auto_resolution_rules";

/// The serialised types of the reference's rule and list.
const RULE_TYPE: u16 = 128;
const LIST_TYPE: u16 = 26;

/// The text the selected rules export as: the reference's serialised rule,
/// or a list of them. A rule's id is its database id, or for a rule not yet
/// saved a negative one, as the reference numbers new rules.
///
/// # Errors
/// If a rule holds a predicate the reference's form can't be written for.
pub fn export_text(rules: &[(Option<i64>, &Rule)], scales: Scales<'_>) -> Result<String, String> {
    let mut new_id = 0;
    let objects = rules
        .iter()
        .map(|(id, rule)| {
            let id = id.unwrap_or_else(|| {
                new_id -= 1;
                new_id
            });
            hydrus_store::duplicates::auto_serialised::rule(rule, id, scales)
                .map_err(|e| format!("\"{}\" could not be exported: {e}", rule.name))
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(hydrus_store::duplicates::auto_serialised::export_text(
        objects,
    ))
}

/// "N objects added!"
pub fn added(n: usize) -> String {
    format!(
        "{} objects added!",
        hydrus_core::numbers::human_int(n as u64)
    )
}

/// What an import found: its rules, and the type names it couldn't take.
#[derive(Debug, Default)]
pub struct Imported {
    pub rules: Vec<Rule>,
    pub refused: BTreeSet<String>,
}

/// The warning for objects of other types (`_ImportObject`).
pub fn refused_message(refused: &BTreeSet<String>) -> String {
    refused_message_for(refused, "DuplicatesAutoResolutionRule")
}

/// The same warning for a control that allows another type.
pub fn refused_message_for(refused: &BTreeSet<String>, allowed: &str) -> String {
    format!(
        "The imported objects included these types:\n\n{}\n\nWhereas this control only allows:\n\n{allowed}",
        refused.iter().cloned().collect::<Vec<_>>().join("\n")
    )
}

/// The key earlier hydrus-rs versions exported comparators under.
const COMPARATOR_KEY: &str = "hydrus_rs_duplicates_auto_resolution_comparators";

/// What the rule editor's comparator list allows (`PairComparator`).
pub const COMPARATOR_TYPE: &str = "PairComparator";

/// The text some comparators export as: the reference's serialised
/// comparator, or a list of them.
///
/// # Errors
/// If a comparator holds a predicate the reference's form can't be written
/// for.
pub fn export_comparators_text(
    comparators: &[Comparator],
    scales: Scales<'_>,
) -> Result<String, String> {
    let objects = comparators
        .iter()
        .map(|c| hydrus_store::duplicates::auto_serialised::comparator(c, scales))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(hydrus_store::duplicates::auto_serialised::export_text(
        objects,
    ))
}

/// What an import into the comparator list found.
#[derive(Debug, Default)]
pub struct ImportedComparators {
    pub comparators: Vec<Comparator>,
    pub refused: BTreeSet<String>,
}

/// Read exported comparator text: hydrus-rs JSON, or the reference's
/// serialised comparator (or a list of them).
///
/// # Errors
/// If the text is neither.
pub fn import_comparators_text(
    text: &str,
    scales: &dyn Fn(
        &hydrus_core::ServiceKey,
    ) -> Option<hydrus_legacy::objects::predicates::StarScale>,
) -> Result<ImportedComparators, String> {
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(text)
        && let Some(comparators) = value.get(COMPARATOR_KEY)
    {
        let comparators: Vec<Comparator> =
            serde_json::from_value(comparators.clone()).map_err(|e| e.to_string())?;
        return Ok(ImportedComparators {
            comparators,
            refused: BTreeSet::new(),
        });
    }
    let object = hydrus_legacy::serialisable::SerialisableObject::from_tuple_str(text.trim())
        .map_err(|e| e.to_string())?;
    let mut out = ImportedComparators::default();
    take_comparators(&object, scales, &mut out)?;
    Ok(out)
}

/// A refused object's type, as the reference names it (its class).
fn type_name(kind: u16) -> String {
    hydrus_downloader_exchange::subscription_import::type_name(kind)
        .unwrap_or_else(|| format!("serialisable type {kind}"))
}

/// The serialised types of the reference's pair comparators.
pub const COMPARATOR_TYPES: [u16; 7] = [130, 131, 137, 138, 140, 141, 152];

fn take_comparators(
    object: &hydrus_legacy::serialisable::SerialisableObject,
    scales: &dyn Fn(
        &hydrus_core::ServiceKey,
    ) -> Option<hydrus_legacy::objects::predicates::StarScale>,
    out: &mut ImportedComparators,
) -> Result<(), String> {
    use hydrus_legacy::serialisable::{Body, Meta};
    match (object.kind.0, &object.body) {
        (LIST_TYPE, Body::List(items)) => {
            for item in items {
                match item {
                    Meta::Object(inner) => take_comparators(inner, scales, out)?,
                    _ => {
                        out.refused.insert("non-serialisable".into());
                    }
                }
            }
        }
        (kind, _) if COMPARATOR_TYPES.contains(&kind) => {
            let legacy = hydrus_legacy::objects::auto_resolution::Comparator::from_object(object)
                .map_err(|e| e.to_string())?;
            out.comparators
                .push(hydrus_store::import::auto_resolution_comparator(
                    &legacy, scales,
                )?);
        }
        (kind, _) => {
            out.refused.insert(type_name(kind));
        }
    }
    Ok(())
}

/// Read exported text: hydrus-rs JSON, or the reference's serialised rule
/// or list of them. `scales` gives numerical rating services' scales for
/// rating predicates.
///
/// # Errors
/// If the text is neither.
pub fn import_text(
    text: &str,
    scales: &dyn Fn(
        &hydrus_core::ServiceKey,
    ) -> Option<hydrus_legacy::objects::predicates::StarScale>,
) -> Result<Imported, String> {
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(text)
        && let Some(rules) = value.get(NATIVE_KEY)
    {
        let rules: Vec<Rule> = serde_json::from_value(rules.clone()).map_err(|e| e.to_string())?;
        return Ok(Imported {
            rules,
            refused: BTreeSet::new(),
        });
    }
    let object = hydrus_legacy::serialisable::SerialisableObject::from_tuple_str(text.trim())
        .map_err(|e| e.to_string())?;
    let mut imported = Imported::default();
    take(&object, scales, &mut imported)?;
    Ok(imported)
}

fn take(
    object: &hydrus_legacy::serialisable::SerialisableObject,
    scales: &dyn Fn(
        &hydrus_core::ServiceKey,
    ) -> Option<hydrus_legacy::objects::predicates::StarScale>,
    out: &mut Imported,
) -> Result<(), String> {
    use hydrus_legacy::serialisable::{Body, Meta};
    match (object.kind.0, &object.body) {
        (LIST_TYPE, Body::List(items)) => {
            for item in items {
                match item {
                    Meta::Object(inner) => take(inner, scales, out)?,
                    _ => {
                        out.refused.insert("non-serialisable".into());
                    }
                }
            }
        }
        (RULE_TYPE, _) => {
            let legacy =
                hydrus_legacy::objects::auto_resolution::AutoResolutionRule::from_object(object)
                    .map_err(|e| e.to_string())?;
            out.rules
                .push(hydrus_store::import::auto_resolution_rule(&legacy, scales)?);
        }
        (kind, _) => {
            out.refused.insert(type_name(kind));
        }
    }
    Ok(())
}

/// What the clipboard import is expecting, in its error (`_ImportFromClipboard`).
pub const CLIPBOARD_EXPECTED: &str = "JSON-serialised Hydrus Object(s)";

/// A message for the user: its title and text.
pub type Message = (String, String);

/// What an import from the clipboard (or a duplicate) brings into a list:
/// what it adds, and the messages to show, in order.
#[derive(Debug)]
pub struct ListImport<T> {
    pub added: Vec<T>,
    pub messages: Vec<Message>,
}

/// The error the reference shows for text it can't read as an object: its
/// `json.loads` error where the text is not JSON (as Python words it),
/// else ours.
fn clipboard_error(text: &str, error: &str) -> Message {
    let error = hydrus_legacy::serialisable::python_json_error(text).map_or_else(
        || error.to_owned(),
        |json| {
            format!(
                "JSONDecodeError({})",
                hydrus_core::pyjson::python_str_repr(&json)
            )
        },
    );
    (
        "Clipboard Error!".into(),
        crate::notes_editor::clipboard_parse_error(CLIPBOARD_EXPECTED, text, &error),
    )
}

/// The messages after an import: the refused types, then the count added.
fn import_messages(refused: &BTreeSet<String>, allowed: &str, added_count: usize) -> Vec<Message> {
    let mut messages = Vec::new();
    if !refused.is_empty() {
        messages.push(("Warning".into(), refused_message_for(refused, allowed)));
    }
    if added_count > 0 {
        messages.push(("Information".into(), added(added_count)));
    }
    messages
}

/// Clipboard text read: `Ok(None)` is an empty clipboard (Qt's `''`), an
/// error a clipboard that could not be read (the reference's `DataMissing`,
/// shown under `missing_title`).
pub type Clipboard = Result<Option<String>, String>;

/// Comparators pasted into a comparator list (the rule editor's, an OR's or
/// an AND's: `EditComparatorList`, a list box).
pub fn comparators_from_clipboard(
    clipboard: Clipboard,
    scales: Scales<'_>,
) -> ListImport<Comparator> {
    let text = match clipboard {
        Ok(text) => text.unwrap_or_default(),
        Err(e) => {
            return ListImport {
                added: Vec::new(),
                messages: vec![("Problem pasting!".into(), e)],
            };
        }
    };
    match import_comparators_text(&text, scales) {
        Ok(imported) => ListImport {
            messages: import_messages(
                &imported.refused,
                COMPARATOR_TYPE,
                imported.comparators.len(),
            ),
            added: imported.comparators,
        },
        Err(e) => ListImport {
            added: Vec::new(),
            messages: vec![clipboard_error(&text, &e)],
        },
    }
}

/// Rules pasted into the rules list (a list control).
pub fn rules_from_clipboard(clipboard: Clipboard, scales: Scales<'_>) -> ListImport<Rule> {
    let text = match clipboard {
        Ok(text) => text.unwrap_or_default(),
        Err(e) => {
            return ListImport {
                added: Vec::new(),
                messages: vec![("Problem importing!".into(), e)],
            };
        }
    };
    match import_text(&text, scales) {
        Ok(imported) => ListImport {
            messages: import_messages(
                &imported.refused,
                "DuplicatesAutoResolutionRule",
                imported.rules.len(),
            ),
            added: imported.rules,
        },
        Err(e) => ListImport {
            added: Vec::new(),
            messages: vec![clipboard_error(&text, &e)],
        },
    }
}

/// A duplicate of the selected (`_Duplicate`): copies, said as an import.
pub fn duplicated<T: Clone>(selected: &[T]) -> ListImport<T> {
    ListImport {
        added: selected.to_vec(),
        messages: import_messages(&BTreeSet::new(), "", selected.len()),
    }
}

/// Comparators from png files into a comparator list (`_ImportFromPNG`):
/// `decoded` is the png's text, `Ok(None)` if no file was picked.
pub fn comparators_from_png(decoded: Clipboard, scales: Scales<'_>) -> ListImport<Comparator> {
    let failed = |text: String| ListImport {
        added: Vec::new(),
        messages: vec![("Problem importing!".into(), text)],
    };
    match decoded {
        Ok(None) => ListImport {
            added: Vec::new(),
            messages: Vec::new(),
        },
        Err(e) => failed(e),
        Ok(Some(text)) => match import_comparators_text(&text, scales) {
            Ok(imported) => ListImport {
                messages: import_messages(
                    &imported.refused,
                    COMPARATOR_TYPE,
                    imported.comparators.len(),
                ),
                added: imported.comparators,
            },
            Err(_) => failed("I could not understand what was encoded in the png!".into()),
        },
    }
}
