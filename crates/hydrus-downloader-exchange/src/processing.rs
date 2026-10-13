//! Reference processing-step JSON and PNG exchange for the shared editor.
//! Packages are validated completely before any step enters the editor draft.
//! As the reference's queue does (`QueueListBox._ImportObject`), a list that
//! mixes processing steps with other objects gives its steps, and names the
//! other objects' types for the reference's warning.

use crate::{Error, MAX_BYTES, MAX_OBJECTS, Result, encode, transport, upgrade};
use hydrus_core::url::strings::{ProcessingStep, StringProcessor};
use hydrus_legacy::{objects::domain, serialisable::SerialisableObject};
use serde_json::{Value, json};
use std::collections::BTreeSet;

/// The reference's processing-step serialisable types (StringMatch,
/// StringConverter, StringSplitter, StringSorter, StringSlicer, StringJoiner,
/// StringTagFilter).
const STEP_CODES: [i64; 7] = [51, 55, 83, 99, 100, 125, 112];
const PROCESSOR: i64 = 84;
const LIST: i64 = 26;

/// An imported package: its processing steps, and the types of the other
/// objects in it, which the queue does not take.
#[derive(Debug, Clone, PartialEq)]
pub struct Package {
    pub steps: Vec<ProcessingStep>,
    pub other_types: BTreeSet<String>,
}

impl Package {
    /// The reference's warning about the other objects; set ordering is
    /// immaterial in the reference.
    pub fn warning(&self) -> Option<String> {
        (!self.other_types.is_empty()).then(|| {
            format!(
                "The imported objects included these types:\n\n{}\n\nWhereas this control only allows:\n\nStringProcessingStep",
                self.other_types.iter().cloned().collect::<Vec<_>>().join("\n")
            )
        })
    }
}

/// Export a selected step, or a reference SerialisableList in queue order.
pub fn encode_text(steps: &[ProcessingStep]) -> Result<String> {
    if steps.is_empty() {
        return Err(Error::Invalid("Select processing steps to export.".into()));
    }
    if steps.len() > MAX_OBJECTS {
        return Err(Error::Limit);
    }
    let processor = encode::processor(&StringProcessor {
        steps: steps.to_vec(),
    })?;
    let steps = processor[2][2]
        .as_array()
        .ok_or_else(|| Error::Invalid("Invalid encoded processor.".into()))?;
    let value = if steps.len() == 1 {
        steps[0][1].clone()
    } else {
        processor[2].clone()
    };
    let text = hydrus_core::pyjson::PyJson::parse(&value.to_string())
        .map_err(|e| Error::Invalid(e.to_string()))?
        .to_python_string();
    if text.len() > MAX_BYTES {
        return Err(Error::Limit);
    }
    Ok(text)
}

/// Import reference steps or a processor without staging partial packages.
/// A package with no steps at all is refused, with the reference's warning
/// when it held other objects.
pub fn decode_text(text: &str) -> Result<Vec<ProcessingStep>> {
    let package = decode_package(text)?;
    if package.steps.is_empty() {
        return Err(Error::Invalid(package.warning().unwrap_or_else(|| {
            "The package contains no processing steps.".into()
        })));
    }
    Ok(package.steps)
}

/// Read a package: its steps (a processor's steps among them), and the types
/// of anything else it holds.
pub fn decode_package(text: &str) -> Result<Package> {
    if text.len() > MAX_BYTES {
        return Err(Error::Limit);
    }
    let mut value: Value = serde_json::from_str(text).map_err(|e| Error::Invalid(e.to_string()))?;
    upgrade::object(&mut value, 0)?;
    if !value[0].is_i64() {
        return Err(Error::Invalid(
            "Expected reference processing-step JSON.".into(),
        ));
    }
    let names = crate::subscription_import::types()?;
    let mut items = Vec::new();
    let mut other_types = BTreeSet::new();
    collect(&value, &names, &mut items, &mut other_types, 0)?;
    if items.len() > MAX_OBJECTS {
        return Err(Error::Limit);
    }
    let processor = json!([PROCESSOR, 1, [LIST, 3, items]]);
    let object = SerialisableObject::from_tuple_str(&processor.to_string())
        .map_err(|e| Error::Invalid(e.to_string()))?;
    let processor = domain::string_processor(&object).map_err(|e| Error::Invalid(e.to_string()))?;
    // Verify every step can be exported losslessly before returning any of it.
    encode::processor(&processor)?;
    Ok(Package {
        steps: processor.steps,
        other_types,
    })
}

/// Gather the steps of one serialised object (a list's members, a
/// processor's steps), naming the types of everything else.
fn collect(
    object: &Value,
    names: &std::collections::BTreeMap<String, String>,
    items: &mut Vec<Value>,
    other_types: &mut BTreeSet<String>,
    depth: usize,
) -> Result<()> {
    if depth > 32 {
        return Err(Error::Limit);
    }
    let Some(code) = object[0].as_i64() else {
        other_types.insert(plain_type(object).into());
        return Ok(());
    };
    match code {
        _ if STEP_CODES.contains(&code) => items.push(json!([2, object])),
        LIST => {
            let members = object[2]
                .as_array()
                .ok_or_else(|| Error::Invalid("Invalid serialisable list.".into()))?;
            for member in members {
                if member[0].as_i64() == Some(2) {
                    collect(&member[1], names, items, other_types, depth + 1)?;
                } else {
                    other_types.insert(plain_type(&member[1]).into());
                }
            }
        }
        // (the reference takes a processor as one nested step; hydrus-rs
        // takes its steps: DIFFERENCES.md)
        PROCESSOR => collect(&object[2], names, items, other_types, depth + 1)?,
        // (a type the reference does not know: it cannot read the package)
        _ => {
            let name = names
                .get(&code.to_string())
                .ok_or_else(|| Error::Unsupported(format!("serialisable type {code}")))?;
            other_types.insert(name.clone());
        }
    }
    Ok(())
}

fn plain_type(value: &Value) -> &'static str {
    match value {
        Value::Null => "NoneType",
        Value::Bool(_) => "bool",
        Value::Number(n) if n.is_f64() => "float",
        Value::Number(_) => "int",
        Value::String(_) => "str",
        Value::Array(_) => "list",
        Value::Object(_) => "dict",
    }
}

/// Export selected steps in the reference compressed grayscale PNG format.
pub fn encode_png(steps: &[ProcessingStep]) -> Result<Vec<u8>> {
    transport::encode_payload(&encode_text(steps)?)
}

/// Read reference PNG steps using the same bounded transport as downloaders.
pub fn decode_png(bytes: &[u8]) -> Result<Vec<ProcessingStep>> {
    decode_text(&transport::decode_payload(bytes)?)
}

/// The reference's warning about a package's objects that are not steps,
/// from its text or PNG bytes (`None` for none, or for a package that cannot
/// be read, whose error is the decoder's to give).
pub fn warning(bytes: &[u8]) -> Option<String> {
    let package = if bytes.starts_with(b"\x89PNG") {
        decode_png_package(bytes)
    } else {
        decode_package(std::str::from_utf8(bytes).ok()?)
    };
    package.ok()?.warning()
}

/// Read a PNG package, as `decode_package` reads text.
pub fn decode_png_package(bytes: &[u8]) -> Result<Package> {
    decode_package(&transport::decode_payload(bytes)?)
}
