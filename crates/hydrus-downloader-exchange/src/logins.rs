//! Reference login script JSON/PNG packages, decoded atomically and bounded.
use crate::{Error, MAX_BYTES, MAX_OBJECTS, Result, encode, transport};
use hydrus_legacy::{
    objects::logins,
    serialisable::{Body, Meta, SerialisableObject},
};
use hydrus_parse::login::{CookieRequirement, CredentialDefinition, LoginScript, LoginStep};
use serde_json::{Value, json};

fn cookies(rows: &[CookieRequirement]) -> Value {
    let pairs = rows
        .iter()
        .map(|row| {
            let mut name = encode::string_match(&row.name);
            let mut value = encode::string_match(&row.value);
            if let Some((old_name, old_value)) = &row.reference_auxiliary {
                encode::preserve_auxiliary(&mut name, old_name);
                encode::preserve_auxiliary(&mut value, old_value);
            }
            json!([[2, name], [2, value]])
        })
        .collect::<Vec<_>>();
    json!([21, 2, pairs])
}
/// Encode the credential name, presentation and permitted input matcher.
pub fn credential_tuple(credential: &CredentialDefinition) -> Value {
    let mut value = json!([
        72,
        credential.name,
        1,
        [
            credential.kind.code(),
            encode::string_match(&credential.string_match)
        ]
    ]);
    if let Some(original) = &credential.reference_auxiliary {
        encode::preserve_auxiliary(&mut value, original);
    }
    value
}
/// Encode all step request variables, cookie tests and content parsers.
pub fn step_tuple(step: &LoginStep) -> Result<Value> {
    if !matches!(step.scheme.as_str(), "http" | "https")
        || !matches!(step.method.as_str(), "GET" | "POST")
    {
        return Err(Error::Unsupported(
            "login steps support http/https and GET/POST".into(),
        ));
    }
    let content = step
        .content_parsers
        .iter()
        .map(encode::content)
        .collect::<Result<Vec<_>>>()?;
    Ok(json!([
        74,
        step.name,
        2,
        [
            step.scheme,
            step.method,
            step.subdomain,
            step.path,
            step.credentials,
            step.static_args,
            step.temp_args,
            cookies(&step.required_cookies),
            encode::serialisable_list(content)
        ]
    ]))
}
/// Encode a complete script in the reference's named v2 format.
pub fn script_tuple(script: &LoginScript) -> Result<Value> {
    encode::valid_key(&script.key)?;
    let steps = script
        .steps
        .iter()
        .map(step_tuple)
        .collect::<Result<Vec<_>>>()?;
    let examples = script
        .examples
        .iter()
        .map(|example| json!([example.domain, example.access.code(), example.description]))
        .collect::<Vec<_>>();
    Ok(json!([
        73,
        script.name,
        2,
        [
            script.key,
            cookies(&script.required_cookies),
            encode::serialisable_list(script.credentials.iter().map(credential_tuple).collect()),
            encode::serialisable_list(steps),
            examples
        ]
    ]))
}
/// Export selected scripts in execution-independent list order.
pub fn encode_text(scripts: &[LoginScript]) -> Result<String> {
    if scripts.is_empty() || scripts.len() > MAX_OBJECTS {
        return Err(Error::Limit);
    }
    let values = scripts
        .iter()
        .map(script_tuple)
        .collect::<Result<Vec<_>>>()?;
    let value = if values.len() == 1 {
        values[0].clone()
    } else {
        encode::serialisable_list(values)
    };
    let text = hydrus_core::pyjson::PyJson::parse(&value.to_string())
        .map_err(|error| Error::Invalid(error.to_string()))?
        .to_python_string();
    if text.len() > MAX_BYTES {
        return Err(Error::Limit);
    }
    Ok(text)
}
fn decode_object(
    object: SerialisableObject,
    depth: usize,
    scripts: &mut Vec<LoginScript>,
) -> Result<()> {
    if depth > 32 || scripts.len() >= MAX_OBJECTS {
        return Err(Error::Limit);
    }
    let error = |error: hydrus_legacy::serialisable::SerialisableError| {
        Error::Unsupported(error.to_string())
    };
    if object.kind.code() == 26 {
        let Body::List(items) = object.upgraded().map_err(error)?.body else {
            return Err(Error::Invalid("Malformed script bundle.".into()));
        };
        for item in items {
            let Meta::Object(object) = item else {
                return Err(Error::Unsupported(
                    "bundle contains a non-script value".into(),
                ));
            };
            decode_object(*object, depth + 1, scripts)?;
        }
    } else {
        let script = logins::login_script(&object).map_err(error)?;
        script_tuple(&script)?;
        scripts.push(script);
    }
    Ok(())
}
/// Decode one script or a nested bundle without partially accepting a package.
pub fn decode_text(text: &str) -> Result<Vec<LoginScript>> {
    if text.len() > MAX_BYTES {
        return Err(Error::Limit);
    }
    let object = SerialisableObject::from_tuple_str(text)
        .map_err(|error| Error::Invalid(error.to_string()))?;
    let mut scripts = Vec::new();
    decode_object(object, 0, &mut scripts)?;
    if scripts.is_empty() {
        return Err(Error::Invalid(
            "The package contains no login scripts.".into(),
        ));
    }
    Ok(scripts)
}
/// Export reference compressed payload PNGs.
pub fn encode_png(scripts: &[LoginScript]) -> Result<Vec<u8>> {
    transport::encode_payload(&encode_text(scripts)?)
}
/// Read scripts from reference compressed payload PNGs.
pub fn decode_png(bytes: &[u8]) -> Result<Vec<LoginScript>> {
    decode_text(&transport::decode_payload(bytes)?)
}
