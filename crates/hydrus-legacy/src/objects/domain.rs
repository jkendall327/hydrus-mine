//! The network domain manager (type 53): the client's URL classes and which
//! parser each is linked to, decoded into `hydrus_core::url` types.
//!
//! Only what URL handling needs is decoded; the rest of the manager
//! (downloaders, import options, headers) stays in the verbatim copy the
//! importer keeps. Every object is accepted at the version v688 writes;
//! the few older versions with trivial upgrades are upgraded, anything else
//! is refused (the verbatim copy is kept either way).

use hydrus_core::url::strings::{
    Conversion, Encoding, FlexibleMatch, HashFunction, MatchKind, ProcessingStep, PyRegex,
    StringConverter, StringMatch, StringProcessor,
};
use hydrus_core::url::{DomainMask, UrlClass, UrlClassSettings, UrlParameter, UrlType};

use super::util::{
    DecodeResult, boolean, hex_bytes, int, list, list_items, malformed, nested, opt_int,
    opt_string, string, strings, tuple,
};
use crate::pyjson::PyJson;
use crate::serialisable::{
    Body, BytesKey, BytesValue, Meta, SerialisableError, SerialisableObject, SerialisableType,
};

const DOMAIN_MANAGER: SerialisableType = SerialisableType(53);
const URL_CLASS: SerialisableType = SerialisableType(50);
const STRING_MATCH: SerialisableType = SerialisableType(51);
const STRING_CONVERTER: SerialisableType = SerialisableType(55);
const PAGE_PARSER: SerialisableType = SerialisableType(58);
const STRING_SPLITTER: SerialisableType = SerialisableType(83);
const STRING_PROCESSOR: SerialisableType = SerialisableType(84);
const STRING_SLICER: SerialisableType = SerialisableType(100);
const STRING_JOINER: SerialisableType = SerialisableType(125);
const URL_CLASS_PARAMETER: SerialisableType = SerialisableType(127);
const URL_DOMAIN_MASK: SerialisableType = SerialisableType(139);

fn unsupported(kind: SerialisableType, version: u32) -> SerialisableError {
    SerialisableError::UnsupportedVersion {
        kind,
        version,
        detail: "only the version v688 writes is supported",
    }
}

pub(crate) fn expect(
    object: &SerialisableObject,
    kind: SerialisableType,
    versions: &[u32],
) -> DecodeResult<()> {
    object.expect_kind(kind)?;
    object.check_not_future()?;
    if versions.contains(&object.version) {
        Ok(())
    } else {
        Err(unsupported(kind, object.version))
    }
}

/// The objects in a `SerialisableList` stored inside another object's info.
pub(crate) fn nested_list(
    kind: SerialisableType,
    value: &PyJson,
    what: &str,
) -> DecodeResult<Vec<SerialisableObject>> {
    let object = nested(kind, value, what)?;
    list_items(&object)?
        .iter()
        .map(|item| match item {
            Meta::Object(object) => Ok((**object).clone()),
            _ => Err(malformed(
                kind,
                format!("{what} holds something that is not an object"),
            )),
        })
        .collect()
}

pub(crate) fn count(kind: SerialisableType, value: &PyJson, what: &str) -> DecodeResult<usize> {
    let n = int(kind, value, what)?;
    usize::try_from(n).map_err(|_| malformed(kind, format!("{what} is negative")))
}

pub(crate) fn opt_count(
    kind: SerialisableType,
    value: &PyJson,
    what: &str,
) -> DecodeResult<Option<usize>> {
    opt_int(kind, value, what)?
        .map(|n| usize::try_from(n).map_err(|_| malformed(kind, format!("{what} is negative"))))
        .transpose()
}

/// A custom HTTP header sent in a network context.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CustomHeader {
    /// `CC.NETWORK_CONTEXT_*`: 0 global, 2 domain, ...
    pub context_type: i64,
    /// The domain for domain contexts; hex for byte-keyed contexts; `None`
    /// for the global context.
    pub context_data: Option<String>,
    pub name: String,
    pub value: String,
    /// `ClientNetworkingDomain.VALID_*`: 0 denied, 1 approved, 2 pending.
    pub approval: i64,
    pub reason: String,
}

const NETWORK_CONTEXT: SerialisableType = SerialisableType(47);

/// Decode the domain manager's custom HTTP headers, in stored order.
pub fn custom_headers(object: &SerialisableObject) -> DecodeResult<Vec<CustomHeader>> {
    let k = DOMAIN_MANAGER;
    expect(object, k, &[7])?;
    let info = object.info();
    let [.., headers] = tuple::<9>(k, &info, "domain manager")?;
    let mut out = Vec::new();
    for entry in list(k, headers, "custom headers")? {
        let [context, dict] = tuple::<2>(k, entry, "custom header entry")?;
        let context = SerialisableObject::from_tuple(context)
            .map_err(|e| malformed(k, format!("network context: {e}")))?;
        expect(&context, NETWORK_CONTEXT, &[2])?;
        let context_info = context.info();
        let [context_type, context_data] = tuple::<2>(NETWORK_CONTEXT, &context_info, "context")?;
        let context_type = int(NETWORK_CONTEXT, context_type, "context type")?;
        let context_data = opt_string(NETWORK_CONTEXT, context_data, "context data")?;
        for header in list(k, dict, "headers")? {
            let [name, row] = tuple::<2>(k, header, "header")?;
            let [value, approval, reason] = tuple::<3>(k, row, "header value")?;
            out.push(CustomHeader {
                context_type,
                context_data: context_data.clone(),
                name: string(k, name, "header name")?,
                value: string(k, value, "header value")?,
                approval: int(k, approval, "header approval")?,
                reason: string(k, reason, "header reason")?,
            });
        }
    }
    Ok(out)
}

/// Decode the domain manager's URL class configuration. The caller adds the
/// client option `collapse_leading_slashes`.
pub fn url_class_settings(object: &SerialisableObject) -> DecodeResult<UrlClassSettings> {
    let k = DOMAIN_MANAGER;
    expect(object, k, &[7])?;
    let info = object.info();
    let [
        _gugs,
        _gugs_to_display,
        url_classes,
        _classes_to_display,
        links,
        _tio,
        _nio,
        parsers,
        _headers,
    ] = tuple::<9>(k, &info, "domain manager")?;
    let url_classes = nested_list(k, url_classes, "url classes")?
        .iter()
        .map(url_class)
        .collect::<DecodeResult<Vec<_>>>()?;
    let links_object = nested(k, links, "parser links")?;
    let parser_links = match &links_object.body {
        Body::BytesDictionary(pairs) => pairs
            .iter()
            .map(|(key, value)| {
                let BytesKey::Bytes(class_key) = key else {
                    return Err(malformed(k, "a parser link's url class key is not bytes"));
                };
                let parser = match value {
                    BytesValue::None => None,
                    BytesValue::Bytes(parser_key) => Some(hex::encode(parser_key)),
                    BytesValue::List(_) => {
                        return Err(malformed(k, "a parser link is a list"));
                    }
                };
                Ok((hex::encode(class_key), parser))
            })
            .collect::<DecodeResult<Vec<_>>>()?,
        _ => return Err(malformed(k, "parser links are not a bytes dictionary")),
    };
    // a parser we can't read is left out: its URL classes then report that
    // they have no parser, rather than the whole manager failing
    let parser_keys = nested_list(k, parsers, "parsers")?
        .iter()
        .filter_map(|parser| parser_key(parser).ok())
        .collect();
    Ok(UrlClassSettings {
        url_classes,
        parser_links,
        parser_keys,
        collapse_leading_slashes: false,
    })
}

/// A page parser's key (v2 and v3 keep it second, after the name).
fn parser_key(object: &SerialisableObject) -> DecodeResult<String> {
    expect(object, PAGE_PARSER, &[2, 3])?;
    let info = object.info();
    let items = list(PAGE_PARSER, &info, "page parser")?;
    let key = items
        .get(1)
        .ok_or_else(|| malformed(PAGE_PARSER, "page parser has no key"))?;
    hex_bytes(PAGE_PARSER, key, "parser key").map(hex::encode)
}

fn url_class(object: &SerialisableObject) -> DecodeResult<UrlClass> {
    let k = URL_CLASS;
    expect(object, k, &[15])?;
    let name = object
        .name
        .clone()
        .ok_or_else(|| malformed(k, "url class has no name"))?;
    let info = object.info();
    let [
        key,
        url_type,
        preferred_scheme,
        domain_mask,
        booleans,
        path_components,
        parameters,
        has_single_value_parameters,
        single_value_parameters_match,
        header_overrides,
        api_lookup_converter,
        send_referral_url,
        referral_url_converter,
        gallery_index_type,
        gallery_index_identifier,
        gallery_index_delta,
        example_url,
    ] = tuple::<17>(k, &info, "url class")?;
    let [
        alphabetise,
        no_more_path,
        no_more_params,
        keep_extra,
        multiple_files,
        associate,
        keep_fragment,
    ] = tuple::<7>(k, booleans, "url class booleans")?;
    let b = |value: &PyJson, what: &str| boolean(k, value, what);
    let code = int(k, url_type, "url type")?;
    let url_type = u8::try_from(code)
        .ok()
        .and_then(UrlType::from_code)
        .ok_or_else(|| malformed(k, format!("unknown url type {code}")))?;
    let path_components = list(k, path_components, "path components")?
        .iter()
        .map(|pair| {
            let [test, default] = tuple::<2>(k, pair, "path component")?;
            Ok((
                string_match(&nested(k, test, "path component test")?)?,
                opt_string(k, default, "path component default")?,
            ))
        })
        .collect::<DecodeResult<Vec<_>>>()?;
    let parameters = nested_list(k, parameters, "parameters")?
        .iter()
        .map(url_parameter)
        .collect::<DecodeResult<Vec<_>>>()?;
    let header_overrides = list(k, header_overrides, "header overrides")?
        .iter()
        .map(|pair| {
            let [name, value] = tuple::<2>(k, pair, "header override")?;
            Ok((
                string(k, name, "header")?,
                string(k, value, "header value")?,
            ))
        })
        .collect::<DecodeResult<Vec<_>>>()?;
    let no_more_parameters_than_this = b(no_more_params, "no more parameters")?;
    let other = PyJson::List(vec![
        send_referral_url.clone(),
        referral_url_converter.clone(),
        gallery_index_type.clone(),
        gallery_index_identifier.clone(),
        gallery_index_delta.clone(),
    ]);
    Ok(UrlClass {
        name,
        key: hex_bytes(k, key, "url class key")?,
        url_type,
        preferred_scheme: string(k, preferred_scheme, "preferred scheme")?,
        domain_mask: self::domain_mask(&nested(k, domain_mask, "domain mask")?)?,
        alphabetise_get_parameters: b(alphabetise, "alphabetise")?,
        no_more_path_components_than_this: b(no_more_path, "no more path components")?,
        no_more_parameters_than_this,
        // as the reference does on load
        keep_extra_parameters_for_server: b(keep_extra, "keep extra parameters")?
            && !no_more_parameters_than_this,
        can_produce_multiple_files: b(multiple_files, "multiple files")?,
        should_be_associated_with_files: b(associate, "associate with files")?,
        keep_fragment: b(keep_fragment, "keep fragment")?,
        path_components,
        parameters,
        has_single_value_parameters: b(has_single_value_parameters, "single value parameters")?,
        single_value_parameters_match: string_match(&nested(
            k,
            single_value_parameters_match,
            "single value parameter test",
        )?)?,
        header_overrides,
        api_lookup_converter: string_converter(&nested(k, api_lookup_converter, "api converter")?)?,
        example_url: string(k, example_url, "example url")?,
        other: other.to_python_string(),
    })
}

fn domain_mask(object: &SerialisableObject) -> DecodeResult<DomainMask> {
    let k = URL_DOMAIN_MASK;
    expect(object, k, &[1])?;
    let info = object.info();
    let [raw, regexes, match_subdomains, keep] = tuple::<4>(k, &info, "domain mask")?;
    // the reference coerces a damaged flag with bool()
    let keep = match keep {
        PyJson::Bool(b) => *b,
        PyJson::Int(i) => *i != 0,
        PyJson::Str(s) => !s.is_empty(),
        PyJson::List(items) => !items.is_empty(),
        _ => false,
    };
    Ok(DomainMask::new(
        strings(k, raw, "raw domains")?,
        strings(k, regexes, "domain regexes")?,
        boolean(k, match_subdomains, "match subdomains")?,
        keep,
    ))
}

fn url_parameter(object: &SerialisableObject) -> DecodeResult<UrlParameter> {
    let k = URL_CLASS_PARAMETER;
    expect(object, k, &[1, 2])?;
    let info = object.info();
    let items = list(k, &info, "parameter")?;
    let (name, value, ephemeral, default, processor) = match (object.version, items) {
        (1, [name, value, default]) => (name, value, false, default, None),
        (2, [name, value, ephemeral, default, processor]) => (
            name,
            value,
            boolean(k, ephemeral, "ephemeral")?,
            default,
            Some(processor),
        ),
        _ => return Err(malformed(k, "parameter has the wrong number of fields")),
    };
    Ok(UrlParameter {
        name: string(k, name, "parameter name")?,
        value: string_match(&nested(k, value, "parameter test")?)?,
        ephemeral,
        default: opt_string(k, default, "parameter default")?,
        // a v1 parameter's upgrade leaves a processor that never runs
        default_processor: match processor {
            Some(p) => string_processor(&nested(k, p, "default processor")?)?,
            None => StringProcessor::default(),
        },
    })
}

pub(crate) fn string_match(object: &SerialisableObject) -> DecodeResult<StringMatch> {
    let k = STRING_MATCH;
    expect(object, k, &[1])?;
    let info = object.info();
    let [match_type, value, min_chars, max_chars, example] = tuple::<5>(k, &info, "string match")?;
    let kind = match int(k, match_type, "match type")? {
        0 => MatchKind::Fixed(string(k, value, "fixed text")?),
        1 => {
            let code = int(k, value, "flexible type")?;
            MatchKind::Flexible(
                FlexibleMatch::from_code(code)
                    .ok_or_else(|| malformed(k, format!("unknown flexible match {code}")))?,
            )
        }
        2 => MatchKind::Regex(PyRegex::new(string(k, value, "regex")?)),
        3 => MatchKind::Any,
        other => return Err(malformed(k, format!("unknown match type {other}"))),
    };
    Ok(StringMatch {
        kind,
        min_chars: opt_count(k, min_chars, "min chars")?,
        max_chars: opt_count(k, max_chars, "max chars")?,
        example: opt_string(k, example, "example")?.unwrap_or_default(),
    })
}

/// v1 converters named their encodings; v2 numbers them.
fn legacy_encoding(name: &str) -> Option<Encoding> {
    Some(match name {
        "url percent encoding" => Encoding::UrlPercent,
        "unicode escape characters" => Encoding::UnicodeEscape,
        "html entities" => Encoding::HtmlEntities,
        "hex" => Encoding::HexUtf8,
        "base64" => Encoding::Base64Utf8,
        _ => return None,
    })
}

pub(crate) fn string_converter(object: &SerialisableObject) -> DecodeResult<StringConverter> {
    let k = STRING_CONVERTER;
    expect(object, k, &[1, 2])?;
    let info = object.info();
    let [conversions, example] = tuple::<2>(k, &info, "string converter")?;
    let mut out = Vec::new();
    for pair in list(k, conversions, "conversions")? {
        let [code, data] = tuple::<2>(k, pair, "conversion")?;
        let code = int(k, code, "conversion type")?;
        let encoding = |data: &PyJson| -> DecodeResult<Option<Encoding>> {
            if object.version == 1 {
                // the reference's upgrade stops at the first bad entry
                return Ok(data.as_str().and_then(legacy_encoding));
            }
            let n = int(k, data, "encoding")?;
            Encoding::from_code(n)
                .map(Some)
                .ok_or_else(|| malformed(k, format!("unknown encoding {n}")))
        };
        let conversion = match code {
            0 => Conversion::RemoveFromStart(count(k, data, "count")?),
            1 => Conversion::RemoveFromEnd(count(k, data, "count")?),
            2 => Conversion::Prepend(string(k, data, "text")?),
            3 => Conversion::Append(string(k, data, "text")?),
            4 | 5 => {
                if object.version == 1
                    && code == 5
                    && matches!(data.as_str(), Some("hex" | "base64"))
                {
                    // v1's decoding to hex/base64 was a no-op, dropped on upgrade
                    continue;
                }
                let Some(encoding) = encoding(data)? else {
                    break;
                };
                if code == 4 {
                    Conversion::Encode(encoding)
                } else {
                    Conversion::Decode(encoding)
                }
            }
            6 => Conversion::KeepStart(count(k, data, "count")?),
            7 => Conversion::KeepEnd(count(k, data, "count")?),
            8 => Conversion::Reverse,
            9 => {
                let [pattern, replacement] = tuple::<2>(k, data, "regex substitution")?;
                Conversion::RegexSub {
                    pattern: PyRegex::new(string(k, pattern, "pattern")?),
                    replacement: string(k, replacement, "replacement")?,
                }
            }
            11 => Conversion::IntegerAddition(int(k, data, "delta")?),
            13 => Conversion::Hash(match string(k, data, "hash function")?.as_str() {
                "md5" => HashFunction::Md5,
                "sha1" => HashFunction::Sha1,
                "sha256" => HashFunction::Sha256,
                "sha512" => HashFunction::Sha512,
                other => return Err(malformed(k, format!("unknown hash function {other}"))),
            }),
            15 => {
                let [population, n] = tuple::<2>(k, data, "random text")?;
                Conversion::AppendRandom {
                    population: string(k, population, "population")?,
                    count: count(k, n, "count")?,
                }
            }
            code => Conversion::Unsupported {
                code,
                data: data.to_python_string(),
            },
        };
        out.push(conversion);
    }
    Ok(StringConverter {
        conversions: out,
        example: opt_string(k, example, "example")?.unwrap_or_default(),
    })
}

pub(crate) fn string_processor(object: &SerialisableObject) -> DecodeResult<StringProcessor> {
    let k = STRING_PROCESSOR;
    expect(object, k, &[1])?;
    let steps = nested_list(k, &object.info(), "processing steps")?
        .iter()
        .map(|step| {
            let info = step.info();
            Ok(match step.kind {
                STRING_CONVERTER => ProcessingStep::Convert(string_converter(step)?),
                STRING_MATCH => ProcessingStep::Filter(string_match(step)?),
                STRING_SPLITTER => {
                    expect(step, STRING_SPLITTER, &[1, 2])?;
                    let [separator, max_splits] = tuple::<2>(STRING_SPLITTER, &info, "splitter")?;
                    let mut separator = string(STRING_SPLITTER, separator, "separator")?;
                    if step.version == 1 {
                        separator = separator.replace('\\', "\\\\");
                    }
                    ProcessingStep::Split {
                        separator,
                        max_splits: opt_count(STRING_SPLITTER, max_splits, "max splits")?,
                    }
                }
                STRING_SLICER => {
                    expect(step, STRING_SLICER, &[1])?;
                    let [start, end] = tuple::<2>(STRING_SLICER, &info, "slicer")?;
                    ProcessingStep::Slice {
                        start: opt_int(STRING_SLICER, start, "start")?,
                        end: opt_int(STRING_SLICER, end, "end")?,
                    }
                }
                STRING_JOINER => {
                    expect(step, STRING_JOINER, &[1, 2])?;
                    let [joiner, tuple_size] = tuple::<2>(STRING_JOINER, &info, "joiner")?;
                    let mut joiner = string(STRING_JOINER, joiner, "joiner")?;
                    let tuple_size = if step.version == 1 {
                        // the reference's upgrade drops the tuple size
                        joiner = joiner.replace('\\', "\\\\");
                        None
                    } else {
                        opt_count(STRING_JOINER, tuple_size, "tuple size")?
                    };
                    ProcessingStep::Join { joiner, tuple_size }
                }
                other => ProcessingStep::Unsupported {
                    type_id: other.code(),
                },
            })
        })
        .collect::<DecodeResult<Vec<_>>>()?;
    Ok(StringProcessor { steps })
}
