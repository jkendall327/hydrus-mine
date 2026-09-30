//! The legacy `options` table: one row of YAML, written by PyYAML's
//! `safe_dump` (plus a `!!python/tuple` representer) and read by `safe_load`.
//!
//! It holds the oldest client settings (`ClientDefaults.GetClientDefaultOptions`):
//! thumbnail size, idle behaviour, trash limits, the client password hash,
//! namespace colours and so on. The Client API reports these as
//! `old_options`.
//!
//! PyYAML resolves plain scalars with YAML 1.1 rules (`yes`/`no` are
//! booleans, `0x1f` is an integer, ...), which differ from the YAML 1.2
//! rules modern parsers use, so this module takes the raw event stream from
//! `yaml-rust2` and resolves scalars itself, the way PyYAML's `SafeLoader`
//! does.

use std::collections::HashMap;

use yaml_rust2::parser::{Event, Parser, Tag};
use yaml_rust2::scanner::TScalarStyle;

/// A value from the legacy YAML options, as Python would hold it.
#[derive(Debug, Clone, PartialEq)]
pub enum YamlValue {
    None,
    Bool(bool),
    Int(i64),
    Float(f64),
    Str(String),
    /// `!!binary`, a Python `bytes`.
    Bytes(Vec<u8>),
    List(Vec<YamlValue>),
    /// `!!python/tuple`.
    Tuple(Vec<YamlValue>),
    /// A mapping, in document order. Keys may be any value (the namespace
    /// colours use `None` as a key).
    Map(Vec<(YamlValue, YamlValue)>),
}

impl YamlValue {
    /// Parse a single-document YAML stream.
    pub fn parse(text: &str) -> Result<YamlValue, String> {
        let mut builder = Builder {
            parser: Parser::new_from_str(text),
            anchors: HashMap::new(),
        };
        let mut document = None;
        loop {
            let (event, _) = builder.parser.next_token().map_err(|e| e.to_string())?;
            match event {
                Event::StreamStart | Event::DocumentStart | Event::DocumentEnd => {}
                Event::StreamEnd => break,
                event => {
                    if document.is_some() {
                        return Err("more than one YAML document".into());
                    }
                    document = Some(builder.node(event, 0)?);
                }
            }
        }
        Ok(document.unwrap_or(YamlValue::None))
    }

    /// Look up a string key in a mapping.
    pub fn get(&self, key: &str) -> Option<&YamlValue> {
        match self {
            YamlValue::Map(entries) => entries
                .iter()
                .find(|(k, _)| matches!(k, YamlValue::Str(s) if s == key))
                .map(|(_, v)| v),
            _ => None,
        }
    }

    pub fn as_i64(&self) -> Option<i64> {
        match self {
            YamlValue::Int(i) => Some(*i),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            YamlValue::Bool(b) => Some(*b),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            YamlValue::Str(s) => Some(s),
            _ => None,
        }
    }

    /// The items of a list or tuple.
    pub fn as_seq(&self) -> Option<&[YamlValue]> {
        match self {
            YamlValue::List(items) | YamlValue::Tuple(items) => Some(items),
            _ => None,
        }
    }
}

const MAX_DEPTH: usize = 256;

struct Builder<'a> {
    parser: Parser<std::str::Chars<'a>>,
    anchors: HashMap<usize, YamlValue>,
}

impl Builder<'_> {
    fn next(&mut self) -> Result<Event, String> {
        self.parser
            .next_token()
            .map(|(event, _)| event)
            .map_err(|e| e.to_string())
    }

    fn node(&mut self, event: Event, depth: usize) -> Result<YamlValue, String> {
        if depth > MAX_DEPTH {
            return Err("YAML nested too deeply".into());
        }
        let (anchor, value) = match event {
            Event::Alias(id) => {
                return self
                    .anchors
                    .get(&id)
                    .cloned()
                    .ok_or_else(|| format!("unknown YAML alias {id}"));
            }
            Event::Scalar(text, style, anchor, tag) => {
                (anchor, scalar(&text, style, tag.as_ref())?)
            }
            Event::SequenceStart(anchor, tag) => {
                let mut items = Vec::new();
                loop {
                    match self.next()? {
                        Event::SequenceEnd => break,
                        event => items.push(self.node(event, depth + 1)?),
                    }
                }
                let value = match tag {
                    Some(tag) if is_core(&tag, "python/tuple") => YamlValue::Tuple(items),
                    None => YamlValue::List(items),
                    Some(tag) if is_core(&tag, "seq") => YamlValue::List(items),
                    Some(tag) => return Err(format!("unsupported YAML tag {tag:?} on a sequence")),
                };
                (anchor, value)
            }
            Event::MappingStart(anchor, tag) => {
                if let Some(tag) = tag.filter(|t| !is_core(t, "map")) {
                    return Err(format!("unsupported YAML tag {tag:?} on a mapping"));
                }
                let mut entries = Vec::new();
                loop {
                    match self.next()? {
                        Event::MappingEnd => break,
                        event => {
                            let key = self.node(event, depth + 1)?;
                            let event = self.next()?;
                            let value = self.node(event, depth + 1)?;
                            entries.push((key, value));
                        }
                    }
                }
                (anchor, YamlValue::Map(entries))
            }
            other => return Err(format!("unexpected YAML event {other:?}")),
        };
        if anchor > 0 {
            self.anchors.insert(anchor, value.clone());
        }
        Ok(value)
    }
}

fn is_core(tag: &Tag, suffix: &str) -> bool {
    tag.handle == "tag:yaml.org,2002:" && tag.suffix == suffix
}

fn scalar(text: &str, style: TScalarStyle, tag: Option<&Tag>) -> Result<YamlValue, String> {
    if let Some(tag) = tag {
        if tag.handle != "tag:yaml.org,2002:" {
            return Err(format!("unsupported YAML tag {tag:?}"));
        }
        return match tag.suffix.as_str() {
            "str" | "python/str" | "python/unicode" => Ok(YamlValue::Str(text.to_owned())),
            "binary" => decode_base64(text).map(YamlValue::Bytes),
            "null" => Ok(YamlValue::None),
            "bool" => resolve_bool(text)
                .map(YamlValue::Bool)
                .ok_or_else(|| format!("bad !!bool {text:?}")),
            "int" => resolve_int(text)
                .map(YamlValue::Int)
                .ok_or_else(|| format!("bad !!int {text:?}")),
            "float" => resolve_float(text)
                .map(YamlValue::Float)
                .ok_or_else(|| format!("bad !!float {text:?}")),
            other => Err(format!("unsupported YAML tag !!{other}")),
        };
    }
    if style != TScalarStyle::Plain {
        return Ok(YamlValue::Str(text.to_owned()));
    }
    Ok(resolve_plain(text))
}

/// PyYAML's implicit resolvers (YAML 1.1) for plain scalars. Timestamps are
/// left as strings: the reference never stores dates in its options.
fn resolve_plain(text: &str) -> YamlValue {
    if matches!(text, "" | "~" | "null" | "Null" | "NULL") {
        return YamlValue::None;
    }
    if let Some(b) = resolve_bool(text) {
        return YamlValue::Bool(b);
    }
    if let Some(i) = resolve_int(text) {
        return YamlValue::Int(i);
    }
    if let Some(f) = resolve_float(text) {
        return YamlValue::Float(f);
    }
    YamlValue::Str(text.to_owned())
}

fn resolve_bool(text: &str) -> Option<bool> {
    match text {
        "yes" | "Yes" | "YES" | "true" | "True" | "TRUE" | "on" | "On" | "ON" => Some(true),
        "no" | "No" | "NO" | "false" | "False" | "FALSE" | "off" | "Off" | "OFF" => Some(false),
        _ => None,
    }
}

fn split_sign(text: &str) -> (bool, &str) {
    match text.as_bytes().first() {
        Some(b'-') => (true, &text[1..]),
        Some(b'+') => (false, &text[1..]),
        _ => (false, text),
    }
}

/// `[-+]?(0b[01_]+ | 0[0-7_]+ | (0|[1-9][0-9_]*) | 0x[0-9a-fA-F_]+ | [1-9][0-9_]*(:[0-5]?[0-9])+)`
fn resolve_int(text: &str) -> Option<i64> {
    let (negative, body) = split_sign(text);
    if body.is_empty() {
        return None;
    }
    let magnitude: i64 = if let Some(bin) = body.strip_prefix("0b") {
        parse_radix(bin, 2)?
    } else if let Some(hex) = body.strip_prefix("0x") {
        parse_radix(hex, 16)?
    } else if body.contains(':') {
        // base 60, e.g. 1:30 == 90
        let (first, rest) = body.split_once(':')?;
        if !first.starts_with(|c: char| ('1'..='9').contains(&c)) {
            return None;
        }
        let mut value = parse_radix(first, 10)?;
        for part in rest.split(':') {
            let digit: i64 = part.parse().ok().filter(|d| (0..60).contains(d))?;
            if part.len() > 2 {
                return None;
            }
            value = value.checked_mul(60)?.checked_add(digit)?;
        }
        value
    } else if body.len() > 1 && body.starts_with('0') {
        parse_radix(&body[1..], 8)?
    } else if body == "0" {
        0
    } else if body.starts_with(|c: char| c.is_ascii_digit()) {
        parse_radix(body, 10)?
    } else {
        return None;
    };
    Some(if negative { -magnitude } else { magnitude })
}

fn parse_radix(digits: &str, radix: u32) -> Option<i64> {
    let cleaned: String = digits.chars().filter(|c| *c != '_').collect();
    if cleaned.is_empty() {
        return None;
    }
    i64::from_str_radix(&cleaned, radix).ok()
}

/// `[-+]?([0-9][0-9_]*)?\.[0-9_]*([eE][-+][0-9]+)?`, base 60 floats,
/// `.inf` and `.nan` (PyYAML requires a dot and a signed exponent).
fn resolve_float(text: &str) -> Option<f64> {
    let (negative, body) = split_sign(text);
    let sign = if negative { -1.0 } else { 1.0 };
    match body {
        ".inf" | ".Inf" | ".INF" => return Some(sign * f64::INFINITY),
        ".nan" | ".NaN" | ".NAN" if text == body => return Some(f64::NAN),
        _ => {}
    }
    if body.contains(':') {
        let mut parts = body.split(':');
        let mut value: f64 = parts.next()?.replace('_', "").parse().ok()?;
        for part in parts {
            value = value * 60.0 + part.replace('_', "").parse::<f64>().ok()?;
        }
        return Some(sign * value);
    }
    let (mantissa, exponent) = match body.find(['e', 'E']) {
        Some(i) => (&body[..i], Some(&body[i + 1..])),
        None => (body, None),
    };
    let (int_part, frac_part) = mantissa.split_once('.')?;
    let int_ok = int_part.is_empty()
        || (int_part.starts_with(|c: char| c.is_ascii_digit())
            && int_part.chars().all(|c| c.is_ascii_digit() || c == '_'));
    let frac_ok = frac_part.chars().all(|c| c.is_ascii_digit() || c == '_');
    let exp_ok = exponent.is_none_or(|e| {
        e.len() > 1 && e.starts_with(['+', '-']) && e[1..].chars().all(|c| c.is_ascii_digit())
    });
    // ".5" is a float but "-.5" and "." are not
    let bare_fraction_ok = !int_part.is_empty() || (text == body && !frac_part.is_empty());
    if !(int_ok && frac_ok && exp_ok && bare_fraction_ok) {
        return None;
    }
    let cleaned: String = body.chars().filter(|c| *c != '_').collect();
    cleaned.parse::<f64>().ok().map(|f| sign * f)
}

fn decode_base64(text: &str) -> Result<Vec<u8>, String> {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = Vec::new();
    let mut buffer = 0u32;
    let mut bits = 0;
    for c in text.bytes().filter(|c| !c.is_ascii_whitespace()) {
        if c == b'=' {
            break;
        }
        let value = ALPHABET
            .iter()
            .position(|a| *a == c)
            .ok_or_else(|| format!("bad base64 character {:?}", c as char))?;
        buffer = (buffer << 6) | value as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buffer >> bits) as u8);
        }
    }
    Ok(out)
}

/// The decoded legacy options, with the reference's defaults filled in for
/// missing keys (as `ClientDB._GetOptions` does).
#[derive(Debug, Clone, PartialEq)]
pub struct LegacyOptions {
    /// Every stored entry, in document order, then defaults for missing keys.
    pub entries: Vec<(YamlValue, YamlValue)>,
}

impl LegacyOptions {
    /// Decode the options row. `None` (no row) gives the defaults.
    pub fn parse(text: Option<&str>) -> Result<LegacyOptions, String> {
        let mut entries = match text.map(YamlValue::parse).transpose()? {
            None | Some(YamlValue::None) => Vec::new(),
            Some(YamlValue::Map(entries)) => entries,
            Some(other) => return Err(format!("options should be a mapping, found {other:?}")),
        };
        for (key, value) in default_entries() {
            let present = entries
                .iter()
                .any(|(k, _)| matches!(k, YamlValue::Str(s) if s == key));
            if !present {
                entries.push((YamlValue::Str(key.to_owned()), value));
            }
        }
        Ok(LegacyOptions { entries })
    }

    pub fn get(&self, key: &str) -> Option<&YamlValue> {
        self.entries
            .iter()
            .find(|(k, _)| matches!(k, YamlValue::Str(s) if s == key))
            .map(|(_, v)| v)
    }

    /// The entries the Client API reports (those with a reference default).
    pub fn api_entries(&self) -> impl Iterator<Item = (&str, &YamlValue)> {
        let defaults: Vec<&str> = default_entries().into_iter().map(|(k, _)| k).collect();
        self.entries.iter().filter_map(move |(k, v)| match k {
            YamlValue::Str(key) if defaults.contains(&key.as_str()) => Some((key.as_str(), v)),
            _ => None,
        })
    }

    /// Bounding box for thumbnails, `(width, height)`.
    pub fn thumbnail_dimensions(&self) -> Option<(i64, i64)> {
        match self.get("thumbnail_dimensions")?.as_seq()? {
            [w, h] => Some((w.as_i64()?, h.as_i64()?)),
            _ => None,
        }
    }

    /// The sha256 of the client's lock password, if one is set.
    pub fn password_hash(&self) -> Option<&[u8]> {
        match self.get("password")? {
            YamlValue::Bytes(b) => Some(b),
            _ => None,
        }
    }

    /// Namespace colours: `None` is the colour for "any other namespace",
    /// `Some("")` for unnamespaced tags.
    pub fn namespace_colours(&self) -> Vec<(Option<&str>, [u8; 3])> {
        let Some(YamlValue::Map(entries)) = self.get("namespace_colours") else {
            return Vec::new();
        };
        entries
            .iter()
            .filter_map(|(k, v)| {
                let namespace = match k {
                    YamlValue::None => None,
                    YamlValue::Str(s) => Some(s.as_str()),
                    _ => return None,
                };
                let [r, g, b] = v.as_seq()? else {
                    return None;
                };
                let channel = |c: &YamlValue| c.as_i64().and_then(|c| u8::try_from(c).ok());
                Some((namespace, [channel(r)?, channel(g)?, channel(b)?]))
            })
            .collect()
    }
}

/// `ClientDefaults.GetClientDefaultOptions()`, in its key order.
fn default_entries() -> Vec<(&'static str, YamlValue)> {
    use YamlValue::{Bool, Float, Int, Str, Tuple};
    let colour = |r, g, b| Tuple(vec![Int(r), Int(g), Int(b)]);
    let regex_favourites = YamlValue::List(vec![
        Tuple(vec![
            Str(r"[1-9]+\d*(?=.{4}$)".into()),
            Str("\u{2026}0074.jpg -> 74".into()),
        ]),
        // the reference builds this from os.path.sep; this is the POSIX form
        Tuple(vec![
            Str(r"[^/]+(?=\s-)".into()),
            Str(r"E:\my collection\author name - v4c1p0074.jpg -> author name".into()),
        ]),
    ]);
    let namespace_colours = YamlValue::Map(vec![
        (Str("system".into()), colour(153, 101, 21)),
        (Str("meta".into()), colour(0, 0, 0)),
        (Str("creator".into()), colour(170, 0, 0)),
        (Str("studio".into()), colour(128, 0, 0)),
        (Str("character".into()), colour(0, 170, 0)),
        (Str("person".into()), colour(0, 128, 0)),
        (Str("series".into()), colour(170, 0, 170)),
        (YamlValue::None, colour(114, 160, 193)),
        (Str(String::new()), colour(0, 111, 250)),
    ]);
    vec![
        ("export_path", YamlValue::None),
        ("hpos", Int(400)),
        ("vpos", Int(-240)),
        (
            "thumbnail_dimensions",
            YamlValue::List(vec![Int(150), Int(125)]),
        ),
        ("password", YamlValue::None),
        ("default_gui_session", Str("last session".into())),
        ("idle_period", Int(60 * 30)),
        ("idle_mouse_period", Int(60 * 10)),
        ("idle_normal", Bool(true)),
        ("idle_shutdown", Int(2)),
        ("idle_shutdown_max_minutes", Int(5)),
        ("trash_max_age", Int(72)),
        ("trash_max_size", Int(2048)),
        ("remove_trashed_files", Bool(false)),
        ("remove_filtered_files", Bool(false)),
        ("confirm_trash", Bool(true)),
        ("confirm_archive", Bool(true)),
        ("gallery_file_limit", Int(2000)),
        ("delete_to_recycle_bin", Bool(true)),
        ("animation_start_position", Float(0.0)),
        ("hide_preview", Bool(false)),
        ("regex_favourites", regex_favourites),
        ("namespace_colours", namespace_colours),
        ("proxy", YamlValue::None),
        ("confirm_client_exit", Bool(false)),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pyyaml_scalars() {
        for (text, expected) in [
            ("yes", YamlValue::Bool(true)),
            ("Off", YamlValue::Bool(false)),
            ("~", YamlValue::None),
            ("0x1f", YamlValue::Int(31)),
            ("017", YamlValue::Int(15)),
            ("1_000", YamlValue::Int(1000)),
            ("-240", YamlValue::Int(-240)),
            ("1:30", YamlValue::Int(90)),
            ("0.0", YamlValue::Float(0.0)),
            ("1.5e+3", YamlValue::Float(1500.0)),
            ("-.inf", YamlValue::Float(f64::NEG_INFINITY)),
            ("1e3", YamlValue::Str("1e3".into())),
            ("hello", YamlValue::Str("hello".into())),
        ] {
            assert_eq!(resolve_plain(text), expected, "{text}");
        }
    }

    #[test]
    fn tags_and_null_keys() {
        let value = YamlValue::parse(
            "a: !!python/tuple\n- 1\n- 2\nb: !!binary |\n  aGVsbG8=\n? ''\n: x\nnull: y\nc: 'yes'\n",
        )
        .unwrap();
        assert_eq!(
            value.get("a"),
            Some(&YamlValue::Tuple(vec![
                YamlValue::Int(1),
                YamlValue::Int(2)
            ]))
        );
        assert_eq!(value.get("b"), Some(&YamlValue::Bytes(b"hello".to_vec())));
        assert_eq!(value.get(""), Some(&YamlValue::Str("x".into())));
        assert_eq!(value.get("c"), Some(&YamlValue::Str("yes".into())));
        let YamlValue::Map(entries) = &value else {
            panic!()
        };
        assert!(entries.iter().any(|(k, _)| *k == YamlValue::None));
    }

    #[test]
    fn anchors_and_aliases() {
        let value = YamlValue::parse("a: &x [1, 2]\nb: *x\n").unwrap();
        assert_eq!(value.get("a"), value.get("b"));
    }

    #[test]
    fn defaults_fill_missing_keys() {
        let options = LegacyOptions::parse(Some("hpos: 10\n")).unwrap();
        assert_eq!(options.get("hpos"), Some(&YamlValue::Int(10)));
        assert_eq!(options.thumbnail_dimensions(), Some((150, 125)));
        assert_eq!(options.api_entries().count(), 25);
        assert_eq!(options.namespace_colours().len(), 9);
    }
}
