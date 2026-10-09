//! Historical file-cache upgrades at the exchange boundary. URL classes belong
//! to the client, so this codec applies only the reference's generic URL encoding.
use crate::{Error, Result, encode, subscriptions};
use hydrus_core::url::UrlClasses;
use serde_json::{Value, json};
use std::collections::HashSet;

fn note(value: &Value, version: u32) -> Result<String> {
    if let Some(text) = value.as_str() {
        return Ok(text.to_owned());
    }
    if version == 1 {
        // v1 used Python str(), whose complex/float representations are not JSON.
        match value {
            Value::Null => return Ok("None".into()),
            Value::Bool(value) => return Ok(if *value { "True" } else { "False" }.into()),
            Value::Number(value) if value.is_i64() || value.is_u64() => {
                return Ok(value.to_string());
            }
            Value::Number(value) if value.as_f64().is_some() => {
                let mut out = String::new();
                hydrus_core::pyjson::write_python_float(value.as_f64().unwrap_or_default(), &mut out);
                return Ok(out);
            }
            _ => {
                return Err(Error::Unsupported(
                    "Historical cache note needs an unsupported Python string conversion.".into(),
                ));
            }
        }
    }
    Err(Error::Invalid("Historical cache note is not text.".into()))
}

fn cache(value: &Value) -> Result<Value> {
    let object = subscriptions::object(value)?;
    if object.kind.code() != 8 || !matches!(object.version, 1..=8) {
        return Err(Error::Unsupported("Unsupported file seed cache.".into()));
    }
    if object.version == 8 {
        return Ok(value.clone());
    }
    let rows = value[2]
        .as_array()
        .ok_or_else(|| Error::Invalid("Malformed historical file cache.".into()))?;
    let mut old_texts = HashSet::new();
    let mut native_texts = HashSet::new();
    let mut seeds = Vec::new();
    for row in rows {
        let fields = row
            .as_array()
            .filter(|fields| fields.len() == 2)
            .ok_or_else(|| Error::Invalid("Malformed historical file seed.".into()))?;
        let text = fields[0]
            .as_str()
            .ok_or_else(|| Error::Invalid("Historical file seed is not text.".into()))?;
        // v4 -> v5 keeps the first occurrence, before any URL rewriting.
        if object.version <= 4 && !old_texts.insert(text) {
            continue;
        }
        let info = fields[1]
            .as_object()
            .ok_or_else(|| Error::Invalid("Malformed historical seed metadata.".into()))?;
        let field = |key: &str| {
            info.get(key)
                .cloned()
                .ok_or_else(|| Error::Invalid(format!("Historical seed has no {key}.")))
        };
        let mut text = text.to_owned();
        if object.version <= 6 {
            text = text.replace("//media.tumblr.com", "//data.tumblr.com");
        }
        let seed_type = i64::from(text.starts_with("http"));
        if seed_type == 1 {
            text = UrlClasses::default().normalise(&text, true).unwrap_or(text);
        }
        // A cache holds one seed per type/data identity; the reference drops
        // later repeats (keeping the first) when it first indexes the cache.
        if !native_texts.insert((seed_type, text.clone())) {
            continue;
        }
        let source = if object.version <= 5 {
            Value::Null
        } else {
            field("source_timestamp")?
        };
        seeds.push(json!([
            57,
            8,
            [
                seed_type,
                text,
                text,
                field("added_timestamp")?,
                field("last_modified_timestamp")?,
                source,
                field("status")?,
                note(&field("note")?, object.version)?,
                null,
                {},
                [],
                [77, 1, []],
                [],
                [],
                [],
                [],
                []
            ]
        ]));
    }
    Ok(json!([8, 8, encode::serialisable_list(seeds)]))
}

/// Upgrade only the embedded cache, leaving the log's gallery history intact.
pub(crate) fn log(value: &Value) -> Result<Value> {
    let object = subscriptions::object(value)?;
    if object.kind.code() != 86 || object.version != 1 {
        return Err(Error::Unsupported(
            "Unsupported subscription query log.".into(),
        ));
    }
    let fields = value[3]
        .as_array()
        .filter(|fields| fields.len() == 2)
        .ok_or_else(|| Error::Invalid("Malformed subscription query log.".into()))?;
    let mut value = value.clone();
    value[3][1] = cache(&fields[1])?;
    Ok(value)
}
