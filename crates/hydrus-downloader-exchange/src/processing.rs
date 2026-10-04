//! Reference processing-step JSON and PNG exchange for the shared editor.
//! Packages are validated completely before any step enters the editor draft.

use crate::{Error, MAX_BYTES, MAX_OBJECTS, Result, encode, transport, upgrade};
use hydrus_core::url::strings::{ProcessingStep, StringProcessor};
use hydrus_legacy::{objects::domain, serialisable::SerialisableObject};
use serde_json::{Value, json};

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
pub fn decode_text(text: &str) -> Result<Vec<ProcessingStep>> {
    if text.len() > MAX_BYTES {
        return Err(Error::Limit);
    }
    let mut value: Value = serde_json::from_str(text).map_err(|e| Error::Invalid(e.to_string()))?;
    upgrade::object(&mut value, 0)?;
    let code = value[0]
        .as_i64()
        .ok_or_else(|| Error::Invalid("Expected reference processing-step JSON.".into()))?;
    let processor = match code {
        84 => value,
        26 => json!([84, 1, value]),
        51 | 55 | 83 | 99 | 100 | 125 | 112 => json!([84, 1, [26, 3, [[2, value]]]]),
        _ => return Err(Error::Unsupported(format!("processing-step object {code}"))),
    };
    let object = SerialisableObject::from_tuple_str(&processor.to_string())
        .map_err(|e| Error::Invalid(e.to_string()))?;
    let processor = domain::string_processor(&object).map_err(|e| Error::Invalid(e.to_string()))?;
    if processor.steps.is_empty() {
        return Err(Error::Invalid(
            "The package contains no processing steps.".into(),
        ));
    }
    if processor.steps.len() > MAX_OBJECTS {
        return Err(Error::Limit);
    }
    // Verify every step can be exported losslessly before returning any of it.
    encode::processor(&processor)?;
    Ok(processor.steps)
}

/// Export selected steps in the reference compressed grayscale PNG format.
pub fn encode_png(steps: &[ProcessingStep]) -> Result<Vec<u8>> {
    transport::encode_payload(&encode_text(steps)?)
}

/// Read reference PNG steps using the same bounded transport as downloaders.
pub fn decode_png(bytes: &[u8]) -> Result<Vec<ProcessingStep>> {
    decode_text(&transport::decode_payload(bytes)?)
}
