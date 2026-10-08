//! The duplicates auto-resolution rules list's export, import and
//! duplicate buttons (`AddImportExportButtons`, `_ImportRule`): rules go out
//! as hydrus-rs JSON, and come in as that or as the reference's own
//! serialised rules (one, or a list); the list names them apart from the
//! rules already there, ignoring case.

use std::collections::BTreeSet;

use hydrus_store::duplicates::auto::{Comparator, Rule};

pub const EXPORT_LABELS: [&str; 3] = ["to clipboard", "to json file", "to png file"];
pub const IMPORT_LABELS: [&str; 3] = [
    "from clipboard",
    "from json files",
    "from png files (you can also drag and drop pngs onto this list)",
];
pub const PROBLEM_TITLE: &str = "Problem loading!";

/// The key hydrus-rs exports rules under.
const NATIVE_KEY: &str = "hydrus_rs_duplicates_auto_resolution_rules";

/// The serialised types of the reference's rule and list.
const RULE_TYPE: u16 = 128;
const LIST_TYPE: u16 = 26;

/// The text the selected rules export as.
///
/// # Panics
/// Never: rules always serialise.
pub fn export_text(rules: &[Rule]) -> String {
    serde_json::to_string_pretty(&serde_json::json!({ NATIVE_KEY: rules }))
        .expect("rules serialise")
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

/// The key hydrus-rs exports comparators under.
const COMPARATOR_KEY: &str = "hydrus_rs_duplicates_auto_resolution_comparators";

/// What the rule editor's comparator list allows (`PairComparator`).
pub const COMPARATOR_TYPE: &str = "PairComparator";

/// The text some comparators export as.
///
/// # Panics
/// Never: comparators always serialise.
pub fn export_comparators_text(comparators: &[Comparator]) -> String {
    serde_json::to_string_pretty(&serde_json::json!({ COMPARATOR_KEY: comparators }))
        .expect("comparators serialise")
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

/// The serialised types of the reference's pair comparators.
const COMPARATOR_TYPES: [u16; 7] = [130, 131, 137, 138, 140, 141, 152];

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
            out.refused.insert(format!("serialisable type {kind}"));
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
            out.refused.insert(format!("serialisable type {kind}"));
        }
    }
    Ok(())
}
