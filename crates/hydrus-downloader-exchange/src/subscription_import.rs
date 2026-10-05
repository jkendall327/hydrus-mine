//! Subscription-list imports accept known unrelated objects with a type warning.
//! Strict exchange decoding remains atomic; this is the list owner's separate
//! import boundary, matching its permitted-type filtering.
use crate::{Error, MAX_BYTES, MAX_OBJECTS, Result, subscriptions};
use hydrus_core::pyjson::PyJson;
use hydrus_legacy::serialisable::{Body, Meta, SerialisableObject};
use std::collections::{BTreeMap, BTreeSet};

/// One loaded package's complete subscription objects and deduplicated type names.
#[derive(Debug, Clone, PartialEq)]
pub struct Package {
    /// In the reference's recursive list order, before fresh history identities.
    pub subscriptions: Vec<subscriptions::Subscription>,
    /// Known objects outside this list's permitted types, sorted for presentation.
    pub other_types: BTreeSet<String>,
}
impl Package {
    /// The exact warning template; set ordering is immaterial in the reference.
    pub fn warning(&self) -> Option<String> {
        (!self.other_types.is_empty()).then(|| format!(
            "The imported objects included these types:\n\n{}\n\nWhereas this control only allows:\n\nSubscriptionLegacy\nSubscriptionContainer",
            self.other_types.iter().cloned().collect::<Vec<_>>().join("\n")
        ))
    }
}

fn types() -> Result<BTreeMap<String, String>> {
    serde_json::from_str(include_str!("subscription_type_names.json"))
        .map_err(|e| Error::Invalid(e.to_string()))
}

fn check(
    object: &SerialisableObject,
    names: &BTreeMap<String, String>,
    depth: usize,
) -> Result<()> {
    if depth > 32 {
        return Err(Error::Limit);
    }
    object
        .check_not_future()
        .map_err(|e| Error::Unsupported(e.to_string()))?;
    if !names.contains_key(&object.kind.code().to_string()) {
        return Err(Error::Unsupported(format!(
            "Unknown serialisable type {}",
            object.kind.code()
        )));
    }
    if let Body::List(items) = &object.body {
        for item in items {
            if let Meta::Object(object) = item {
                check(object, names, depth + 1)?;
            }
        }
    }
    Ok(())
}

fn plain_type(value: &PyJson) -> &'static str {
    match value {
        PyJson::Null => "NoneType",
        PyJson::Bool(_) => "bool",
        PyJson::Int(_) | PyJson::BigInt(_) => "int",
        PyJson::Float(_) => "float",
        PyJson::Str(_) => "str",
        PyJson::List(_) => "list",
        PyJson::Object(_) => "dict",
    }
}

fn collect(
    object: &SerialisableObject,
    names: &BTreeMap<String, String>,
    now: i64,
    package: &mut Package,
    seen: &mut usize,
) -> Result<()> {
    *seen += 1;
    if *seen > MAX_OBJECTS {
        return Err(Error::Limit);
    }
    if let Body::List(items) = &object.body {
        for item in items {
            match item {
                Meta::Object(object) => collect(object, names, now, package, seen)?,
                Meta::Json(value) => {
                    *seen += 1;
                    if *seen > MAX_OBJECTS {
                        return Err(Error::Limit);
                    }
                    package.other_types.insert(plain_type(value).into());
                }
                Meta::Bytes(_) => {
                    *seen += 1;
                    if *seen > MAX_OBJECTS {
                        return Err(Error::Limit);
                    }
                    package.other_types.insert("bytes".into());
                }
            }
        }
    } else if matches!(object.kind.code(), 3 | 90) {
        let value = serde_json::from_str(&object.to_tuple_string())
            .map_err(|e| Error::Invalid(e.to_string()))?;
        package
            .subscriptions
            .push(subscriptions::decode_container(&value, now)?);
    } else {
        package
            .other_types
            .insert(names[&object.kind.code().to_string()].clone());
    }
    Ok(())
}

/// Load a whole carrier before the list adds permitted objects one by one.
/// Future/unknown serialisable types fail loading instead of becoming type warnings.
pub fn decode_text_at(text: &str, now: i64) -> Result<Package> {
    if text.len() > MAX_BYTES {
        return Err(Error::Limit);
    }
    let names = types()?;
    let object = SerialisableObject::from_tuple_str(text)
        .and_then(|object| object.upgraded())
        .map_err(|e| Error::Invalid(e.to_string()))?;
    check(&object, &names, 0)?;
    let mut package = Package {
        subscriptions: Vec::new(),
        other_types: BTreeSet::new(),
    };
    collect(&object, &names, now, &mut package, &mut 0)?;
    Ok(package)
}

/// Decode the reference PNG carrier with the list's permitted-type filtering.
pub fn decode_png_at(bytes: &[u8], now: i64) -> Result<Package> {
    decode_text_at(&crate::transport::decode_payload(bytes)?, now)
}
