//! Scoped metadata-router JSON/PNG exchange, separate from downloader definitions.
use crate::{Error, MAX_BYTES, MAX_OBJECTS, Result, encode, transport, upgrade};
use hydrus_core::content::TimestampType;
use hydrus_legacy::{
    objects::sidecars,
    serialisable::{Body, Meta, SerialisableObject},
};
use hydrus_parse::sidecar::{
    Exporter, Importer, Router, Source, TagDisplay, TimestampLocation, TimestampStub,
};
use serde_json::{Value, json};

fn timestamp(stub: &TimestampStub) -> Result<Value> {
    let location = match (&stub.kind, &stub.location) {
        (
            TimestampType::Imported | TimestampType::Deleted | TimestampType::PreviouslyImported,
            TimestampLocation::Service(key),
        ) => {
            encode::valid_key(key)?;
            json!(key)
        }
        (TimestampType::LastViewed, TimestampLocation::Canvas(canvas)) => json!(canvas),
        (TimestampType::ModifiedDomain, TimestampLocation::Domain(domain)) => json!(domain),
        (
            TimestampType::Archived
            | TimestampType::ModifiedAggregate
            | TimestampType::ModifiedFile,
            TimestampLocation::None,
        ) => Value::Null,
        _ => {
            return Err(Error::Unsupported(
                "timestamp kind/location combination".into(),
            ));
        }
    };
    Ok(json!([121, 2, [stub.kind.code(), location, null]]))
}
fn importer(value: &Importer) -> Result<Value> {
    let p = encode::processor(&value.processor)?;
    Ok(match &value.source {
        Source::MediaNotes => json!([120, 1, p]),
        Source::MediaUrls => json!([118, 2, p]),
        Source::MediaTags {
            service_key,
            display,
        } => {
            encode::valid_key(service_key)?;
            json!([
                111,
                3,
                [
                    p,
                    service_key,
                    match display {
                        TagDisplay::Storage => 0,
                        TagDisplay::DisplayActual => 1,
                    }
                ]
            ])
        }
        Source::MediaTimestamp(stub) => json!([123, 1, [p, timestamp(stub)?]]),
        Source::Txt { naming, separator } => json!([
            110,
            4,
            [
                p,
                naming.remove_actual_filename_ext,
                naming.suffix,
                encode::converter(&naming.filename_converter)?,
                separator
            ]
        ]),
        Source::Json { naming, formula } => json!([
            114,
            3,
            [
                p,
                naming.remove_actual_filename_ext,
                naming.suffix,
                encode::converter(&naming.filename_converter)?,
                encode::formula(formula)?
            ]
        ]),
    })
}
fn exporter(value: &Exporter) -> Result<Value> {
    Ok(match value {
        Exporter::MediaNotes { forced_name } => json!([119, 2, forced_name]),
        Exporter::MediaUrls => json!([117, 1, []]),
        Exporter::MediaTags { service_key } => {
            encode::valid_key(service_key)?;
            json!([115, 1, service_key])
        }
        Exporter::MediaTimestamp(stub) => json!([122, 1, timestamp(stub)?]),
        Exporter::Txt { naming, separator } => json!([
            116,
            3,
            [
                naming.remove_actual_filename_ext,
                naming.suffix,
                encode::converter(&naming.filename_converter)?,
                separator
            ]
        ]),
        Exporter::Json {
            naming,
            nested_object_names,
        } => json!([
            113,
            2,
            [
                naming.remove_actual_filename_ext,
                naming.suffix,
                encode::converter(&naming.filename_converter)?,
                nested_object_names
            ]
        ]),
    })
}
/// Encode every source, processor, destination and timestamp stub in queue order.
pub fn tuple(router: &Router) -> Result<Value> {
    Ok(json!([
        109,
        3,
        [
            encode::serialisable_list(
                router
                    .importers
                    .iter()
                    .map(importer)
                    .collect::<Result<Vec<_>>>()?
            ),
            encode::processor(&router.processor)?,
            exporter(&router.exporter)?
        ]
    ]))
}
/// Export one router or a selected queue as bounded reference text.
pub fn encode_text(routers: &[Router]) -> Result<String> {
    if routers.is_empty() || routers.len() > MAX_OBJECTS {
        return Err(Error::Limit);
    }
    let values = routers.iter().map(tuple).collect::<Result<Vec<_>>>()?;
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
fn check_stubs(value: &Value) -> Result<()> {
    if let Some(fields) = value.as_array() {
        // Routers retain timestamp stubs; reject a non-stub instead of dropping time data.
        if fields.len() == 3 && fields[0] == json!(121) && !fields[2][2].is_null() {
            return Err(Error::Unsupported(
                "router timestamp carries an actual time instead of a stub".into(),
            ));
        }
        if fields.len() == 3 && fields[0] == json!(117) && fields[2] != json!([]) {
            return Err(Error::Unsupported(
                "URL exporter carries unknown editor data".into(),
            ));
        }
        for field in fields {
            check_stubs(field)?;
        }
    }
    Ok(())
}
fn decode_value(mut value: Value, depth: usize, output: &mut Vec<Router>) -> Result<()> {
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
            return Err(Error::Invalid("Malformed router package.".into()));
        };
        for item in items {
            let Meta::Object(nested) = item else {
                return Err(Error::Unsupported(
                    "router package contains a non-object value".into(),
                ));
            };
            let value = serde_json::from_str(&nested.to_tuple().to_python_string())
                .map_err(|e| Error::Invalid(e.to_string()))?;
            decode_value(value, depth + 1, output)?;
        }
        return Ok(());
    }
    if object.kind.code() != 109 {
        return Err(Error::Unsupported(format!(
            "expected metadata router, got object type {}",
            object.kind.code()
        )));
    }
    check_stubs(&value)?;
    let mut router = sidecars::router(&object).map_err(|e| Error::Unsupported(e.to_string()))?;
    tuple(&router)?;
    if let Some(rows) = value[2][0][2].as_array() {
        for (importer, row) in router.importers.iter_mut().zip(rows) {
            if let Source::Json { formula, .. } = &mut importer.source
                && let Some(fields) = row[1][2].as_array()
                && let Some(value) = fields.last()
            {
                crate::attach_formula(formula, value);
            }
        }
    }
    output.push(router);
    Ok(())
}
/// Validate all routers before any enters an owner's draft.
pub fn decode_text(text: &str) -> Result<Vec<Router>> {
    if text.len() > MAX_BYTES {
        return Err(Error::Limit);
    }
    let value = serde_json::from_str(text).map_err(|e| Error::Invalid(e.to_string()))?;
    let mut output = Vec::new();
    decode_value(value, 0, &mut output)?;
    if output.is_empty() {
        return Err(Error::Invalid(
            "The package contains no metadata routers.".into(),
        ));
    }
    Ok(output)
}
/// Export the selected router queue in the reference compressed PNG format.
pub fn encode_png(routers: &[Router]) -> Result<Vec<u8>> {
    transport::encode_payload(&encode_text(routers)?)
}
/// Decode a bounded reference PNG without partial staging.
pub fn decode_png(bytes: &[u8]) -> Result<Vec<Router>> {
    decode_text(&transport::decode_payload(bytes)?)
}
