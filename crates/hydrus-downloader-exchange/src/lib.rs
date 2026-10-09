//! Reference downloader interchange, separate from the read-only database reader.
//! Native definitions own runtime behavior; original tuples retain auxiliary
//! editor data the native parser does not execute. All inputs are bounded and
//! a package is decoded completely before its caller can stage any changes.

pub mod domain_metadata;
mod encode;
pub mod external_calls;
pub mod import_options;
pub mod logins;
pub mod processing;
pub mod routers;
pub mod subscription_import;
mod subscription_legacy;
mod subscription_seed_cache;
pub mod subscriptions;
pub mod subsidiaries;
mod transport;
mod upgrade;

use hydrus_core::pyjson::PyJson;
use hydrus_core::url::{AnyGug, UrlClass};
use hydrus_legacy::objects::{domain, parsers};
use hydrus_legacy::serialisable::{Body, Meta, SerialisableObject};
use hydrus_parse::content::{ContentParser, PageParser};
use hydrus_parse::formula::Formula;
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub use transport::{decode_png, encode_png};
/// Generic string carriers used by source logs and other reference PNG exports.
pub mod text_png {
    pub use super::transport::{decode_payload as decode, encode_payload_with_header as encode};
}

/// Maximum uncompressed JSON, compressed input, or decoded PNG pixel bytes.
pub const MAX_BYTES: usize = 16 * 1024 * 1024;
/// Maximum number of definitions in a package.
pub const MAX_OBJECTS: usize = 4096;

/// An invalid, oversized, or unsupported exchange payload.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Invalid downloader data: {0}")]
    Invalid(String),
    #[error("Downloader data exceeds the 16 MiB or 4096-object limit.")]
    Limit,
    #[error(
        "This definition cannot be imported safely: {0}. Update it in the reference client or remove that unsupported item, then export again."
    )]
    Unsupported(String),
}
/// Exchange operations which fail without changing settings.
pub type Result<T> = std::result::Result<T, Error>;

/// A native definition supported by both clients.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Native {
    Class(Box<UrlClass>),
    Gug(AnyGug),
    Page(PageParser),
    Content(ContentParser),
    Formula(Formula),
    Simple(hydrus_parse::simple::SimpleFormula),
    Login(hydrus_parse::login::LoginScript),
    /// A domain's shareable headers and bandwidth rules (type 71).
    Domain(domain_metadata::DomainMetadata),
}
/// A decoded native object together with auxiliary reference editor data.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Definition {
    pub native: Native,
    pub original: Option<Value>,
}
impl Definition {
    /// Prepare a newly authored native definition for export.
    pub fn new(native: Native) -> Self {
        Self {
            native,
            original: None,
        }
    }
    /// Its descriptive name for review and duplicate decisions.
    pub fn name(&self) -> &str {
        match &self.native {
            Native::Class(c) => &c.name,
            Native::Gug(g) => g.name(),
            Native::Page(p) => &p.name,
            Native::Content(p) => &p.name,
            Native::Formula(f) => &f.name,
            Native::Simple(f) => &f.name,
            Native::Login(script) => &script.name,
            Native::Domain(metadata) => &metadata.domain,
        }
    }
    /// Encode the native fields while retaining auxiliary reference data.
    pub fn tuple(&self) -> Result<Value> {
        let mut encoded = encode::native(&self.native)?;
        if let Some(original) = &self.original {
            encode::preserve_auxiliary(&mut encoded, original);
        }
        Ok(encoded)
    }
}

/// Read a single reference object or a SerialisableList bundle atomically.
pub fn decode_text(text: &str) -> Result<Vec<Definition>> {
    if text.len() > MAX_BYTES {
        return Err(Error::Limit);
    }
    let json: Value = serde_json::from_str(text).map_err(|e| Error::Invalid(e.to_string()))?;
    let mut count = 0;
    let mut output = Vec::new();
    decode_value(json, 0, &mut count, &mut output)?;
    if output.is_empty() {
        return Err(Error::Invalid(
            "The package contains no definitions.".into(),
        ));
    }
    Ok(output)
}
fn decode_value(
    mut value: Value,
    depth: usize,
    count: &mut usize,
    output: &mut Vec<Definition>,
) -> Result<()> {
    if depth > 32 {
        return Err(Error::Limit);
    }
    upgrade::object(&mut value, 0)?;
    let object = SerialisableObject::from_tuple_str(&value.to_string())
        .map_err(|e| Error::Invalid(e.to_string()))?;
    object
        .check_not_future()
        .map_err(|e| Error::Unsupported(e.to_string()))?;
    if object.kind.code() == 26 {
        let upgraded = object
            .upgraded()
            .map_err(|e| Error::Invalid(e.to_string()))?;
        let Body::List(items) = upgraded.body else {
            return Err(Error::Invalid("Malformed definition bundle.".into()));
        };
        for item in items {
            let Meta::Object(nested) = item else {
                return Err(Error::Unsupported(
                    "bundle contains a non-definition value".into(),
                ));
            };
            let nested: Value = serde_json::from_str(&nested.to_tuple().to_python_string())
                .map_err(|e| Error::Invalid(e.to_string()))?;
            decode_value(nested, depth + 1, count, output)?;
        }
        return Ok(());
    }
    if *count >= MAX_OBJECTS {
        return Err(Error::Limit);
    }
    *count += 1;
    let err = |e: hydrus_legacy::serialisable::SerialisableError| Error::Unsupported(e.to_string());
    let mut native = match object.kind.code() {
        50 => Native::Class(Box::new(domain::url_class(&object).map_err(err)?)),
        69 | 70 => Native::Gug(parsers::gug(&object).map_err(err)?),
        58 => Native::Page(parsers::page_parser(&object).map_err(err)?),
        30 => Native::Content(parsers::content_parser(&object).map_err(err)?),
        27 | 31 | 59 | 60 | 133 | 136 => Native::Formula(parsers::formula(&object).map_err(err)?),
        63 => Native::Simple(parsers::simple_formula(&object).map_err(err)?),
        71 => Native::Domain(domain_metadata::decode(&value)?),
        73 => Native::Login(hydrus_legacy::objects::logins::login_script(&object).map_err(err)?),
        code => return Err(Error::Unsupported(format!("object type {code}"))),
    };
    // Encoding is also a losslessness check: unknown processors must never
    // pass through the decoder's reduced Unsupported { type_id } placeholder.
    encode::native(&native)?;
    attach_auxiliary(&mut native, &value);
    output.push(Definition {
        native,
        original: Some(value),
    });
    Ok(())
}
/// Export a single object or the reference's meta-encoded bundle.
pub fn encode_text(definitions: &[Definition]) -> Result<String> {
    if definitions.is_empty() || definitions.len() > MAX_OBJECTS {
        return Err(Error::Limit);
    }
    let values = definitions
        .iter()
        .map(Definition::tuple)
        .collect::<Result<Vec<_>>>()?;
    let value = if values.len() == 1 {
        values[0].clone()
    } else {
        encode::serialisable_list(values)
    };
    let text = PyJson::parse(&value.to_string())
        .map_err(|e| Error::Invalid(e.to_string()))?
        .to_python_string();
    if text.len() > MAX_BYTES {
        return Err(Error::Limit);
    }
    Ok(text)
}

fn attach_auxiliary(native: &mut Native, value: &Value) {
    match native {
        Native::Formula(f) => attach_formula(f, value),
        Native::Content(c) => attach_content(c, value),
        Native::Simple(s) => {
            if let Some(f) = value.as_array().and_then(|a| a.last()) {
                attach_formula(&mut s.formula, f);
            }
        }
        Native::Page(p) => attach_page(p, value),
        _ => (),
    }
}
fn info(value: &Value) -> Option<&Vec<Value>> {
    value.as_array()?.last()?.as_array()
}
fn attach_formula(formula: &mut Formula, value: &Value) {
    formula.reference_auxiliary = Some(value.clone());
    let Some(i) = info(value) else {
        return;
    };
    match &mut formula.kind {
        hydrus_parse::formula::FormulaKind::Nested { main, sub } => {
            attach_formula(main, &i[0]);
            attach_formula(sub, &i[1]);
        }
        hydrus_parse::formula::FormulaKind::Zipper { formulae, .. } => {
            if let Some(rows) = i[0].get(2).and_then(Value::as_array) {
                for (f, row) in formulae.iter_mut().zip(rows) {
                    if let Some(v) = row.get(1) {
                        attach_formula(f, v);
                    }
                }
            }
        }
        _ => (),
    }
}
fn attach_content(content: &mut ContentParser, value: &Value) {
    if let Some(f) = info(value).and_then(|i| i.get(2)) {
        attach_formula(&mut content.formula, f);
    }
}
fn attach_page(page: &mut PageParser, value: &Value) {
    page.reference_auxiliary = Some(value.clone());
    let Some(i) = info(value) else {
        return;
    };
    if let Some(rows) = i[4].get(2).and_then(Value::as_array) {
        for (c, row) in page.content_parsers.iter_mut().zip(rows) {
            if let Some(v) = row.get(1) {
                attach_content(c, v);
            }
        }
    }
    if let Some(rows) = i[3].get(2).and_then(Value::as_array) {
        for (s, row) in page.subsidiary.iter_mut().zip(rows) {
            if let Some(v) = row.get(1).and_then(info) {
                attach_formula(&mut s.formula, &v[0]);
                attach_page(&mut s.parser, &v[2]);
            }
        }
    }
}
