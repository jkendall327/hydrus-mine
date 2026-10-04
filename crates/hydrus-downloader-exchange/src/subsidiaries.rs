//! Standalone reference subsidiary wrappers, scoped separately from downloaders.
//! Separation formulas, sorting and recursive page auxiliary data travel together.
use crate::{Error, MAX_BYTES, MAX_OBJECTS, Result, encode, transport, upgrade};
use hydrus_legacy::{
    objects::parsers,
    serialisable::{Body, Meta, SerialisableObject},
};
use hydrus_parse::content::SubsidiaryPageParser;
use serde_json::{Value, json};

/// Encode a complete wrapper, preserving its embedded formula and page editor data.
pub fn tuple(parser: &SubsidiaryPageParser) -> Result<Value> {
    Ok(json!([
        135,
        2,
        [
            encode::formula(&parser.formula)?,
            parser.sort_by_source_time,
            encode::page(&parser.parser)?
        ]
    ]))
}

/// Export selected wrappers as one reference object or an ordered package.
pub fn encode_text(parsers: &[SubsidiaryPageParser]) -> Result<String> {
    if parsers.is_empty() || parsers.len() > MAX_OBJECTS {
        return Err(Error::Limit);
    }
    let values = parsers.iter().map(tuple).collect::<Result<Vec<_>>>()?;
    let value = if values.len() == 1 {
        values[0].clone()
    } else {
        encode::serialisable_list(values)
    };
    let text = hydrus_core::pyjson::PyJson::parse(&value.to_string())
        .map_err(|e| Error::Invalid(e.to_string()))?
        .to_python_string();
    if text.len() > MAX_BYTES {
        return Err(Error::Limit);
    }
    Ok(text)
}

/// Validate the complete package before returning any subsidiary to an owner draft.
pub fn decode_text(text: &str) -> Result<Vec<SubsidiaryPageParser>> {
    if text.len() > MAX_BYTES {
        return Err(Error::Limit);
    }
    let value = serde_json::from_str(text).map_err(|e| Error::Invalid(e.to_string()))?;
    let mut output = Vec::new();
    decode_value(value, 0, &mut output)?;
    if output.is_empty() {
        return Err(Error::Invalid(
            "The package contains no subsidiary parsers.".into(),
        ));
    }
    Ok(output)
}
fn decode_value(
    mut value: Value,
    depth: usize,
    output: &mut Vec<SubsidiaryPageParser>,
) -> Result<()> {
    if depth > 32 || output.len() >= MAX_OBJECTS {
        return Err(Error::Limit);
    }
    upgrade::object(&mut value, 0)?;
    let object = SerialisableObject::from_tuple_str(&value.to_string())
        .map_err(|e| Error::Invalid(e.to_string()))?;
    object
        .check_not_future()
        .map_err(|e| Error::Unsupported(e.to_string()))?;
    if object.kind.code() == 26 {
        let object = object
            .upgraded()
            .map_err(|e| Error::Invalid(e.to_string()))?;
        let Body::List(items) = object.body else {
            return Err(Error::Invalid("Malformed subsidiary package.".into()));
        };
        for item in items {
            let Meta::Object(nested) = item else {
                return Err(Error::Unsupported(
                    "subsidiary package contains a non-object value".into(),
                ));
            };
            let value = serde_json::from_str(&nested.to_tuple().to_python_string())
                .map_err(|e| Error::Invalid(e.to_string()))?;
            decode_value(value, depth + 1, output)?;
        }
        return Ok(());
    }
    if object.kind.code() != 135 {
        return Err(Error::Unsupported(format!(
            "expected subsidiary parser, got object type {}",
            object.kind.code()
        )));
    }
    let mut parser =
        parsers::subsidiary_page_parser(&object).map_err(|e| Error::Unsupported(e.to_string()))?;
    // Reject reduced unknown processors before they can enter an editor.
    tuple(&parser)?;
    crate::attach_formula(&mut parser.formula, &value[2][0]);
    crate::attach_page(&mut parser.parser, &value[2][2]);
    output.push(parser);
    Ok(())
}
/// Export selected wrappers in the reference's compressed PNG format.
pub fn encode_png(parsers: &[SubsidiaryPageParser]) -> Result<Vec<u8>> {
    transport::encode_payload(&encode_text(parsers)?)
}
/// Import bounded reference PNG wrappers without partial staging.
pub fn decode_png(bytes: &[u8]) -> Result<Vec<SubsidiaryPageParser>> {
    decode_text(&transport::decode_payload(bytes)?)
}
